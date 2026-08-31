//! Update system: check for updates, download deltas, apply with rollback.

use anyhow::{Context, Result};
use chrono::Timelike;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::fs;
use tracing::{debug, info, warn};

/// Live download progress counters. Updated by download_component_resumable
/// as bytes arrive and read by the update.status RPC so the UI can show
/// a real progress bar instead of a fake creep. Global because the
/// download runs in one place at a time; no need for per-handler state.
pub static DOWNLOAD_BYTES: AtomicU64 = AtomicU64::new(0);
pub static DOWNLOAD_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Set true to ask the in-flight download loop to bail out at the next
/// chunk boundary. Read via `is_canceled`; reset at the start of every
/// `download_update` run. Also flipped by the `cancel_download` RPC.
pub static DOWNLOAD_CANCEL: AtomicBool = AtomicBool::new(false);
/// Monotonic ms timestamp of the last time DOWNLOAD_BYTES advanced.
/// Lets `update.status` flag a download as "stalled" when no bytes have
/// arrived for a while, so the UI can offer a Cancel button with more
/// confidence than "looks stuck at 0%".
pub static DOWNLOAD_PROGRESS_AT: AtomicU64 = AtomicU64::new(0);

/// Serializes the mutating update operations (download, apply, and the
/// staging wipe in cancel). The .198 v1.7.103 bricking (2026-07-18) was
/// exactly this race: two concurrent `update.download` RPCs shared one
/// staging file, a cancel wiped staging mid-flight, a third download began
/// re-filling it, and `apply_update` mv'd the 3-second-old 17MB partial of
/// a 49MB binary into /usr/local/bin → SEGV boot loop. Writers take this
/// via `try_lock` so a concurrent caller gets an explicit "already running"
/// error instead of silently interleaving.
static UPDATE_OP_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn is_canceled() -> bool {
    DOWNLOAD_CANCEL.load(Ordering::Relaxed)
}

/// Parse "MAJOR.MINOR.PATCH[-suffix]" into a tuple; suffix is ignored.
/// Returns None if the numeric portion can't be parsed — callers should
/// fall back to string comparison in that case so we don't silently
/// mis-rank versions we don't understand.
fn parse_version_triple(v: &str) -> Option<(u32, u32, u32)> {
    let core = v.split('-').next().unwrap_or(v);
    let mut parts = core.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    let patch: u32 = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// Is `candidate` strictly newer than `current`? Used to guard against
/// the manifest offering a version we've already passed (e.g. a stale
/// cached manifest or a node that sideloaded past the manifest's
/// latest). Falls back to string inequality if either version doesn't
/// parse, preserving the old behaviour for unusual version strings.
fn is_newer(candidate: &str, current: &str) -> bool {
    match (
        parse_version_triple(candidate),
        parse_version_triple(current),
    ) {
        (Some(a), Some(b)) => a > b,
        _ => candidate != current,
    }
}

/// Primary OTA origin. Named host over TLS rather than the bare IP it used
/// to be: the IP pinned the fleet to one machine and one plaintext port, so
/// moving or fronting the origin meant an OTA to change where OTAs come
/// from — the one update you cannot ship if the origin is unreachable. The
/// signature is what establishes trust (see `trust::anchor`), not the
/// transport, but HTTPS also stops a network observer seeing which version
/// a node runs.
const DEFAULT_UPDATE_MANIFEST_URL: &str =
    "https://source.archipelago-foundation.org/lfg2025/archy/raw/branch/main/releases/manifest.json";

// The previous IP-based origin (http://146.59.87.168:3000/…) was an automatic
// DNS/TLS-broken fallback until 2026-08-11, when bare-IP origins were retired
// from the update registry. `load_mirrors` strips it from saved lists by host.
const UPDATE_STATE_FILE: &str = "update_state.json";
const UPDATE_MIRRORS_FILE: &str = "update-mirrors.json";
/// Marker written by apply_update() just before the service restart and
/// consumed by verify_pending_update() in the NEW binary's startup path.
/// If present, the new binary probes the frontend; if the probe fails,
/// rollback_update() runs and the service restarts on the old binary.
/// Closes the "OTA broke nginx fleet-wide with no auto-rollback" failure
/// mode from 2026-04-22 (v1.7.38/39 tarball-perms bug).
const PENDING_VERIFY_FILE: &str = "update-pending-verify.json";
/// Probe timeout for the frontend health check (total time including
/// retries). Generous: the new binary has to come fully up, health
/// monitor settles, nginx has to re-read any snippet changes. 90s is
/// comfortably longer than the slowest observed startup.
const PENDING_VERIFY_WINDOW_SECS: u64 = 90;
/// If the marker is older than this on read, treat it as stale and
/// delete without probing. Guards against a node that somehow failed
/// to run verification at all (e.g. crashed during startup) from
/// spontaneously rolling back days later when the user reboots.
const PENDING_VERIFY_MAX_AGE_SECS: i64 = 600;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateMirror {
    /// Full URL to `manifest.json`. Download URLs in the fetched
    /// manifest are origin-rewritten to match this URL's scheme+host+
    /// port, so hitting a mirror pulls its components from the same
    /// mirror rather than whatever absolute host the publisher baked in.
    pub url: String,
    /// Human-readable label for the UI ("Server 1", "Home VPS", …).
    #[serde(default)]
    pub label: String,
}

fn mirrors_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(UPDATE_MIRRORS_FILE)
}

fn default_mirrors() -> Vec<UpdateMirror> {
    vec![
        UpdateMirror {
            url: DEFAULT_UPDATE_MANIFEST_URL.to_string(),
            label: "Archipelago Foundation".to_string(),
        },
        // The bare-IP plain-HTTP twin of the entry above was retired as a
        // default on 2026-08-11 (operator decision: no bare-IP origins in the
        // update registry). Its DNS/TLS-broken recovery value is accepted as
        // lost; the manifest signature was always what protected the update,
        // never the transport. `load_mirrors` strips it from saved lists.
    ]
}

/// Load the operator-configured mirror list. Returns defaults if the
/// file doesn't exist yet, so a node OTA'd from a pre-mirrors release
/// starts with the current default mirrors available without any
/// manual config.
///
/// Migration: any default mirror URL that isn't already in the saved
/// list gets appended at the end. This lets us add new default mirrors
/// (e.g. a new Server 3) and have them appear on existing nodes after
/// an update, without requiring manual config edits. Explicit removals
/// stick — once an operator removes a URL it stays gone unless it's
/// later re-added to defaults.
pub async fn load_mirrors(data_dir: &Path) -> Result<Vec<UpdateMirror>> {
    let path = mirrors_path(data_dir);
    if !path.exists() {
        return Ok(default_mirrors());
    }
    let bytes = fs::read(&path)
        .await
        .with_context(|| format!("read {}", path.display()))?;
    let mut list: Vec<UpdateMirror> =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    if list.is_empty() {
        return Ok(default_mirrors());
    }

    // One-time migrations: drop decommissioned release servers that may be
    // baked into existing nodes' saved mirror lists. Strip them on load so
    // we don't spend seconds per install timing out against a dead/stale host.
    //   - 23.182.128.160: Hetzner VPS, decommissioned 2026-04-23.
    //   - git.tx1138.com: retired as a release server 2026-06-13 — its main
    //     branch had diverged and stopped receiving releases, so it only
    //     ever served a stale manifest as the secondary mirror.
    // Exception to the usual "explicit removals stick" rule: the user never
    // chose to add these — they were defaults.
    //   - 146.59.87.168: the release server's own bare-IP HTTP twin, retired
    //     as a default 2026-08-11 — same host as the named origin, so it
    //     provided no redundancy, only a plain-HTTP path the operator no
    //     longer wants advertised in the update registry.
    let before = list.len();
    list.retain(|m| {
        !m.url.contains("23.182.128.160")
            && !m.url.contains("git.tx1138.com")
            && !m.url.contains("146.59.87.168")
    });
    let mut changed = list.len() != before;

    // Merge in any default URLs the saved config is missing.
    let known: std::collections::HashSet<String> = list.iter().map(|m| m.url.clone()).collect();
    let defaults = default_mirrors();
    for def in &defaults {
        if !known.contains(&def.url) {
            list.push(def.clone());
            changed = true;
        }
    }
    let before_order: Vec<String> = list.iter().map(|m| m.url.clone()).collect();
    force_ovh_update_primary(&mut list);
    changed = changed || before_order != list.iter().map(|m| m.url.clone()).collect::<Vec<_>>();
    if changed {
        if let Err(e) = save_mirrors(data_dir, &list).await {
            warn!("Failed to persist migrated update-mirrors list: {e:#}");
        }
    }
    Ok(list)
}

fn force_ovh_update_primary(list: &mut Vec<UpdateMirror>) {
    let defaults = default_mirrors();
    for def in &defaults {
        if !list.iter().any(|m| m.url == def.url) {
            list.push(def.clone());
        }
    }
    for mirror in list.iter_mut() {
        if mirror.url == DEFAULT_UPDATE_MANIFEST_URL {
            mirror.label = "Archipelago Foundation".to_string();
        }
    }
    // Named origin first, anything the operator added after that. Ordering
    // matters: the list is tried in order, so a stale entry sitting first
    // costs a timeout on every check.
    list.sort_by_key(|m| match m.url.as_str() {
        u if u == DEFAULT_UPDATE_MANIFEST_URL => 0,
        _ => 1,
    });
}

pub async fn save_mirrors(data_dir: &Path, mirrors: &[UpdateMirror]) -> Result<()> {
    fs::create_dir_all(data_dir)
        .await
        .with_context(|| format!("mkdir {}", data_dir.display()))?;
    let path = mirrors_path(data_dir);
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(mirrors).context("serialize mirrors")?;
    fs::write(&tmp, json)
        .await
        .with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, &path)
        .await
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

// ─── Update/app fetch source (origin vs DHT swarm) ──────────────────────────
//
// User-selectable per node, persisted in `data_dir/update-source.json`. This is
// the live-testing switch: keep `Origin` (default) to pull releases/app blobs
// purely over HTTP from the configured mirrors — the known-good path — or flip
// to `Swarm` on a test node to exercise the DHT (iroh swarm-assist), knowing the
// origin still always wins as fallback. Independent of the compile-time
// `iroh-swarm` feature and the `swarm_enabled` config: if the swarm engine isn't
// present, `Swarm` simply has no peers to consult and behaves like `Origin`.

const UPDATE_SOURCE_FILE: &str = "update-source.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UpdateSource {
    /// HTTP origin/mirrors only. The safe default and the universal fallback.
    #[default]
    Origin,
    /// Try DHT swarm peers first for content-addressed blobs, origin always wins.
    Swarm,
}

fn default_true() -> bool {
    true
}

/// Node-level swarm preferences, persisted together in `update-source.json`.
/// Two independent switches:
///  - `source`: where THIS node fetches (origin vs swarm). Default origin.
///  - `provide_dht`: whether this node SEEDS/serves blobs to peers. Default on
///    (opt-out) so the swarm has providers; nodes that don't want to serve can
///    turn it off without affecting how they fetch.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct SwarmPrefs {
    #[serde(default)]
    source: UpdateSource,
    #[serde(default = "default_true")]
    provide_dht: bool,
}

impl Default for SwarmPrefs {
    fn default() -> Self {
        Self {
            source: UpdateSource::default(),
            provide_dht: true,
        }
    }
}

fn update_source_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(UPDATE_SOURCE_FILE)
}

async fn load_swarm_prefs(data_dir: &Path) -> SwarmPrefs {
    match fs::read_to_string(update_source_path(data_dir)).await {
        Ok(s) => serde_json::from_str::<SwarmPrefs>(&s).unwrap_or_default(),
        Err(_) => SwarmPrefs::default(),
    }
}

async fn save_swarm_prefs(data_dir: &Path, prefs: &SwarmPrefs) -> Result<()> {
    fs::create_dir_all(data_dir)
        .await
        .with_context(|| format!("mkdir {}", data_dir.display()))?;
    let path = update_source_path(data_dir);
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(prefs).context("serialize swarm prefs")?;
    fs::write(&tmp, json)
        .await
        .with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, &path)
        .await
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

/// Load the node's selected fetch source. Missing/corrupt file → `Origin`.
pub async fn load_update_source(data_dir: &Path) -> UpdateSource {
    load_swarm_prefs(data_dir).await.source
}

/// Persist the node's selected fetch source (preserving `provide_dht`).
pub async fn save_update_source(data_dir: &Path, source: UpdateSource) -> Result<()> {
    let mut prefs = load_swarm_prefs(data_dir).await;
    prefs.source = source;
    save_swarm_prefs(data_dir, &prefs).await
}

/// Whether this node seeds/serves blobs to peers. Default true (opt-out).
pub async fn load_provide_dht(data_dir: &Path) -> bool {
    load_swarm_prefs(data_dir).await.provide_dht
}

/// Persist whether this node provides to the swarm (preserving `source`).
pub async fn save_provide_dht(data_dir: &Path, provide: bool) -> Result<()> {
    let mut prefs = load_swarm_prefs(data_dir).await;
    prefs.provide_dht = provide;
    save_swarm_prefs(data_dir, &prefs).await
}

/// Parse a manifest URL and return its `scheme://host[:port]` prefix.
/// Used by `rewrite_manifest_origins` so a manifest fetched from a
/// mirror points component downloads back at the same mirror rather
/// than whatever absolute URL the publisher baked in.
fn manifest_origin(manifest_url: &str) -> Option<String> {
    let rest = manifest_url
        .strip_prefix("https://")
        .map(|r| ("https", r))
        .or_else(|| manifest_url.strip_prefix("http://").map(|r| ("http", r)))?;
    let (scheme, after_scheme) = rest;
    let host_and_port = after_scheme.split('/').next()?;
    if host_and_port.is_empty() {
        return None;
    }
    Some(format!("{}://{}", scheme, host_and_port))
}

/// Rewrite every component `download_url` so its origin matches the
/// manifest URL we just fetched. Preserves the path portion (which is
/// consistent across mirrors — every gitea serves `/lfg2025/archy/raw/…`).
/// Leaves URLs with a different path shape untouched (some operator
/// might mirror with a custom layout; in that case we don't guess).
fn rewrite_manifest_origins(manifest: &mut UpdateManifest, manifest_url: &str) {
    let Some(new_origin) = manifest_origin(manifest_url) else {
        return;
    };
    for c in manifest.components.iter_mut() {
        if let Some(orig_origin) = manifest_origin(&c.download_url) {
            if orig_origin != new_origin {
                let path = c.download_url.trim_start_matches(&orig_origin).to_string();
                c.download_url = format!("{}{}", new_origin, path);
            }
        }
    }
}

/// Parse a fetched manifest body, verifying its detached release-root
/// signature if one is present. Returns the manifest plus whether it was
/// signed by the pinned release-root anchor.
///
/// * Present-but-invalid signature (tampered payload, wrong signer, bad
///   hex…) → `Err`; the caller treats the mirror as failed. This is the
///   teeth: a MITM can strip the signature but can never forge one.
/// * Unsigned → accepted with `signed == false` during the migration
///   window; the scheduler refuses to auto-apply such manifests and the
///   state surfaces `manifest_signed` to the UI.
fn parse_and_verify_manifest(raw: serde_json::Value) -> Result<(UpdateManifest, bool)> {
    let signed = match crate::trust::verify_detached(&raw).context("manifest signature")? {
        crate::trust::SignatureStatus::Verified { anchored, .. } => anchored,
        crate::trust::SignatureStatus::Unsigned => false,
    };
    let manifest: UpdateManifest = serde_json::from_value(raw).context("parse update manifest")?;
    Ok((manifest, signed))
}

/// Which manifest URL to try FIRST — operator override via env wins,
/// otherwise the first entry in the mirrors list, otherwise the hard
/// default. Callers that need the full mirror walk should use
/// `load_mirrors` directly.
fn update_manifest_url() -> String {
    std::env::var("ARCHIPELAGO_UPDATE_URL")
        .unwrap_or_else(|_| DEFAULT_UPDATE_MANIFEST_URL.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub release_date: String,
    pub changelog: Vec<String>,
    pub components: Vec<ComponentUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentUpdate {
    pub name: String,
    pub current_version: String,
    pub new_version: String,
    pub download_url: String,
    pub sha256: String,
    pub size_bytes: u64,
    /// DHT Phase 1: BLAKE3 content address (bare hex or `"blake3:<hex>"`), the
    /// iroh-native, range-verifiable hash. Optional during the migration
    /// window — when present it is verified ALONGSIDE the mandatory SHA-256.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blake3: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum UpdateSchedule {
    Manual,
    #[default]
    DailyCheck,
    AutoApply,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateState {
    pub current_version: String,
    pub last_check: Option<String>,
    pub available_update: Option<UpdateManifest>,
    pub update_in_progress: bool,
    pub rollback_available: bool,
    #[serde(default)]
    pub schedule: UpdateSchedule,
    /// URL of the mirror whose manifest populated `available_update`.
    /// Surfaces in the UI so operators can tell at a glance which mirror
    /// their node actually hit (vs. just which is configured primary).
    #[serde(default)]
    pub manifest_mirror: Option<String>,
    /// True when `available_update` came from a manifest whose detached
    /// Ed25519 signature verified against the pinned release-root anchor
    /// (`trust::verify_detached`). Unsigned manifests are still offered for
    /// MANUAL apply during the signing-migration window, but the scheduler
    /// refuses to auto-apply them — otherwise a mirror/MITM could push an
    /// arbitrary root binary to the fleet unattended.
    #[serde(default)]
    pub manifest_signed: bool,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self {
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            last_check: None,
            available_update: None,
            update_in_progress: false,
            rollback_available: false,
            schedule: UpdateSchedule::DailyCheck,
            manifest_mirror: None,
            manifest_signed: false,
        }
    }
}

/// Marker written by apply_update() just before the service restart and
/// consumed by verify_pending_update() in the NEW binary's startup path.
/// See PENDING_VERIFY_FILE for the full rationale — this is the hook
/// that turns "nginx 500 on every page after OTA" from an unrecoverable
/// field incident into an automatic rollback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingVerification {
    /// RFC3339 timestamp of the apply that wrote this marker.
    pub applied_at: String,
    /// Version we just applied (what the NEW binary should be running).
    pub new_version: String,
    /// Version the outgoing binary was running (what we roll back to).
    pub previous_version: String,
    /// Unix epoch seconds after which the probe should give up and
    /// trigger rollback. Prevents a probe from retrying forever if e.g.
    /// nginx is totally wedged.
    pub deadline_ts: i64,
}

async fn write_pending_verification(data_dir: &Path, marker: &PendingVerification) -> Result<()> {
    let path = data_dir.join(PENDING_VERIFY_FILE);
    let data = serde_json::to_string_pretty(marker).context("serialize pending-verify marker")?;
    fs::write(&path, data)
        .await
        .with_context(|| format!("write pending-verify marker to {}", path.display()))?;
    Ok(())
}

async fn read_pending_verification(data_dir: &Path) -> Option<PendingVerification> {
    let path = data_dir.join(PENDING_VERIFY_FILE);
    let data = fs::read_to_string(&path).await.ok()?;
    serde_json::from_str(&data).ok()
}

async fn clear_pending_verification(data_dir: &Path) {
    let path = data_dir.join(PENDING_VERIFY_FILE);
    let _ = fs::remove_file(&path).await;
}

/// Probe the local frontend through nginx. Returns Ok(()) on the first
/// response that's 2xx or 3xx; errors on timeout / connection refused /
/// any 4xx/5xx. `accept_self_signed` because nodes use a self-signed
/// cert the reqwest default root-set doesn't trust.
async fn probe_frontend_once() -> Result<()> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .context("build probe client")?;
    // Prefer HTTPS since that's the failure mode we're catching (nginx
    // 500 on the PWA). HTTP usually redirects to HTTPS and would mask
    // the bug. BUT not every node binds 443 on loopback (.116 serves
    // plain HTTP; 443 there belongs to tailscale) — on a *connect*
    // error, fall back to HTTP so a healthy node isn't "verified" into
    // a rollback. An HTTP error status stays fatal on whichever scheme
    // answered.
    let resp = match client.get("https://127.0.0.1/").send().await {
        Ok(resp) => resp,
        Err(e) if e.is_connect() => client
            .get("http://127.0.0.1/")
            .send()
            .await
            .context("probe GET http://127.0.0.1/ (https not bound on loopback)")?,
        Err(e) => return Err(e).context("probe GET https://127.0.0.1/"),
    };
    let status = resp.status();
    if status.is_success() || status.is_redirection() {
        return Ok(());
    }
    anyhow::bail!("frontend probe returned HTTP {}", status);
}

/// Probe the backend RPC API through nginx. An unauthenticated call is
/// EXPECTED to get 401/403 — any such response proves the Rust HTTP stack
/// is alive behind nginx. 5xx (backend dead → nginx 502), 404 (proxy
/// misroute), or connection errors mean the API is down even though the
/// static frontend may still serve — exactly the failure mode the plain
/// `GET /` probe waved through.
async fn probe_backend_once() -> Result<()> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .context("build probe client")?;
    let body = serde_json::json!({ "method": "update.status" });
    let resp = match client
        .post("https://127.0.0.1/rpc/v1")
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) if e.is_connect() => client
            .post("http://127.0.0.1/rpc/v1")
            .json(&body)
            .send()
            .await
            .context("probe POST http://127.0.0.1/rpc/v1 (https not bound on loopback)")?,
        Err(e) => return Err(e).context("probe POST https://127.0.0.1/rpc/v1"),
    };
    let status = resp.status();
    if status.is_server_error() || status == reqwest::StatusCode::NOT_FOUND {
        anyhow::bail!("backend RPC probe returned HTTP {}", status);
    }
    Ok(())
}

/// Probe that the rootless container runtime is reachable from the new
/// binary (uid mapping / podman socket intact after the swap). A healthy
/// node answers `podman ps` in well under a second.
async fn probe_container_runtime_once() -> Result<()> {
    let out = tokio::process::Command::new("podman")
        .args(["ps", "--format", "{{.Names}}"])
        .output()
        .await
        .context("spawn podman ps")?;
    if !out.status.success() {
        anyhow::bail!(
            "podman ps exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Called from main.rs startup. If a pending-verification marker is
/// present, verify the node actually works on the new version — binary
/// version matches the marker, frontend serves, backend RPC answers,
/// rootless podman is reachable. On failure, trigger rollback and
/// restart the service so the OLD binary boots.
///
/// This is the "post-OTA auto-rollback" guardrail. If ANY problem in
/// the new version takes down the PWA (bad tarball perms as in v1.7.38,
/// a broken service worker, a missing asset, a backend panic on first
/// boot), the node self-heals back to the previous working state
/// without SSH intervention.
pub async fn verify_pending_update(data_dir: &Path) {
    let marker = match read_pending_verification(data_dir).await {
        Some(m) => m,
        None => return, // No update pending; nothing to verify.
    };

    // Guard against a marker left behind by some earlier crash path —
    // don't want a user who reboots days later to suddenly get
    // rolled back because the marker was never cleared.
    let applied_at = chrono::DateTime::parse_from_rfc3339(&marker.applied_at);
    if let Ok(ts) = applied_at {
        let age = chrono::Utc::now() - ts.with_timezone(&chrono::Utc);
        if age.num_seconds() > PENDING_VERIFY_MAX_AGE_SECS {
            tracing::warn!(
                age_secs = age.num_seconds(),
                "pending-verify marker is stale, clearing without probing"
            );
            clear_pending_verification(data_dir).await;
            return;
        }
    }

    info!(
        new_version = %marker.new_version,
        previous_version = %marker.previous_version,
        "Post-OTA verification: probing frontend, backend RPC, and container runtime"
    );

    // Binary identity check: if the running binary's version isn't the one
    // the marker says we applied, the swap silently failed (or half-applied
    // — new frontend with old binary). Deterministic, so no retry loop:
    // fall through straight to rollback to restore a matched pair.
    let running = env!("CARGO_PKG_VERSION");
    let mut attempt = 0u32;
    let mut last_err: Option<String> = None;
    let version_ok = running == marker.new_version;
    if !version_ok {
        last_err = Some(format!(
            "running binary is {} but marker says {} was applied — binary swap failed",
            running, marker.new_version
        ));
    } else {
        // Give the new service time to bind its listeners + nginx to
        // pick up any config changes. 15s matches what we observed on
        // .116 during the v1.7.40 rollout recovery.
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;

        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(PENDING_VERIFY_WINDOW_SECS);

        while std::time::Instant::now() < deadline {
            attempt += 1;
            // All three must pass in the same attempt: static frontend via
            // nginx, backend RPC liveness, and rootless-podman reachability.
            // (Individual app containers are NOT asserted — the pre-Quadlet
            // service restart legitimately takes them down and the boot
            // reconciler can need minutes to bring heavy apps back.)
            let result = match probe_frontend_once().await {
                Ok(()) => match probe_backend_once().await {
                    Ok(()) => probe_container_runtime_once().await,
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            };
            match result {
                Ok(()) => {
                    info!(attempt, "Post-OTA verification succeeded — clearing marker");
                    clear_pending_verification(data_dir).await;
                    return;
                }
                Err(e) => {
                    let msg = format!("{e:#}");
                    tracing::warn!(attempt, error = %msg, "Post-OTA probe failed, retrying");
                    last_err = Some(msg);
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }

    tracing::error!(
        attempts = attempt,
        window_secs = PENDING_VERIFY_WINDOW_SECS,
        last_error = last_err.as_deref().unwrap_or(""),
        new_version = %marker.new_version,
        previous_version = %marker.previous_version,
        "Post-OTA verification FAILED — rolling back"
    );

    // Restore web-ui.bak on top of web-ui. update.rs keeps web-ui.bak
    // from the previous apply; moving it back is the frontend half of
    // the rollback. The binary half is handled by rollback_update().
    let web_ui_bak = Path::new("/opt/archipelago/web-ui.bak");
    let web_ui = "/opt/archipelago/web-ui";
    if web_ui_bak.exists() {
        let ts = chrono::Utc::now().timestamp_millis();
        let quarantine = format!("/opt/archipelago/web-ui.failed.{}", ts);
        let _ = host_sudo(&["mv", web_ui, &quarantine]).await;
        let _ = host_sudo(&["mv", web_ui_bak.to_str().unwrap_or(""), web_ui]).await;
        tracing::info!(quarantined = %quarantine, "Restored web-ui from web-ui.bak");
    } else {
        tracing::warn!("web-ui.bak not present — frontend cannot be rolled back, only binary");
    }

    if let Err(e) = rollback_update(data_dir).await {
        tracing::error!(error = %e, "rollback_update() failed during post-OTA verification");
        // Leave the marker in place so a future boot gets another shot.
        return;
    }

    clear_pending_verification(data_dir).await;

    // Record why we rolled back so the UI can show it on the next boot.
    if let Ok(mut state) = load_state(data_dir).await {
        state.current_version = marker.previous_version.clone();
        if let Err(e) = save_state(data_dir, &state).await {
            tracing::warn!(error = %e, "Failed to update state after rollback");
        }
    }

    // Restart so the old binary takes over. --no-block because we're
    // the service; systemd can't wait for us to exit before starting
    // the old process.
    let _ = host_sudo(&["systemctl", "--no-block", "restart", "archipelago"]).await;
}

pub async fn load_state(data_dir: &Path) -> Result<UpdateState> {
    let path = data_dir.join(UPDATE_STATE_FILE);
    if !path.exists() {
        let state = UpdateState::default();
        save_state(data_dir, &state).await?;
        return Ok(state);
    }
    let data = fs::read_to_string(&path)
        .await
        .context("Reading update state")?;
    let mut state: UpdateState = serde_json::from_str(&data).context("Parsing update state")?;

    let mut changed = false;

    // Keep current_version in sync with the binary. Sideloaded nodes
    // (ssh + cp /usr/local/bin/archipelago) don't touch the state file,
    // so without this the running 1.7.0-alpha binary would keep seeing
    // `current_version: "1.6.0-alpha"` and re-offer itself as an update.
    let running = env!("CARGO_PKG_VERSION");
    if state.current_version != running {
        state.current_version = running.to_string();
        // Binary version changed (sideload or apply). Any stored
        // `available_update` is either redundant (points at the running
        // version) or stale (points at a version we've already passed —
        // which would surface as a "downgrade" offer in the UI). Clear
        // it unconditionally; the next check_for_updates will repopulate
        // if there's genuinely something newer.
        state.available_update = None;
        state.manifest_mirror = None;
        state.manifest_signed = false;
        changed = true;
    }

    // `update_in_progress` means a manifest OTA is downloaded and staged,
    // ready for apply. Older git/self-build update paths could leave this
    // flag stuck true without a staging directory, which traps the UI in an
    // unrecoverable state. Heal that on every state load.
    if state.update_in_progress && !has_staged_update(data_dir).await {
        warn!(
            staging = %data_dir.join("update-staging").display(),
            "Clearing stale update_in_progress without staged OTA files"
        );
        state.update_in_progress = false;
        changed = true;
    }

    if changed {
        save_state(data_dir, &state).await?;
    }
    Ok(state)
}

/// Marker written only after EVERY component has downloaded and verified.
/// Distinguishes a complete, install-ready staging from the partial files a
/// resumable-but-failed download leaves behind.
const STAGED_COMPLETE_MARKER: &str = ".download-complete";

async fn has_staged_update(data_dir: &Path) -> bool {
    // A *complete* staged update carries the marker. A partial/failed download
    // leaves component files (kept for resume) but no marker, so it reads as
    // "not staged" — the state self-heal then clears update_in_progress and the
    // UI returns to Download instead of stranding the user on Install.
    fs::metadata(data_dir.join("update-staging").join(STAGED_COMPLETE_MARKER))
        .await
        .is_ok()
}

pub async fn save_state(data_dir: &Path, state: &UpdateState) -> Result<()> {
    let path = data_dir.join(UPDATE_STATE_FILE);
    let data = serde_json::to_string_pretty(state)?;
    fs::write(&path, data).await.context("Writing update state")
}

/// Check for available updates by walking the mirror list. The first
/// mirror that returns a parseable manifest with a strictly-newer
/// version wins; if no mirror offers a newer version, the node is
/// reported as up-to-date. Per-mirror we retry up to 3 times on
/// transient failures.
///
/// Manifest `download_url`s are origin-rewritten to match the mirror
/// we fetched from, so switching mirrors in the UI also switches where
/// component downloads come from — even if the publisher baked an
/// absolute URL pointing at a different server into the manifest.
pub async fn check_for_updates(data_dir: &Path) -> Result<UpdateState> {
    let mut state = load_state(data_dir).await?;

    info!("Checking for updates...");
    let client = reqwest::Client::builder()
        // Short per-attempt HTTP timeout so a wedged mirror doesn't
        // delay the whole check — we'd rather move on to the next
        // mirror quickly than sit waiting on a slow one. 15s covers
        // slow but alive mirrors.
        .timeout(std::time::Duration::from_secs(15))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .context("Failed to create HTTP client")?;

    // Env override (ARCHIPELAGO_UPDATE_URL) short-circuits the mirror
    // list — used on dev boxes that point at a local gitea. Otherwise
    // walk the operator-configured list and fall through on failure.
    let mirrors: Vec<String> = if std::env::var("ARCHIPELAGO_UPDATE_URL").is_ok() {
        vec![update_manifest_url()]
    } else {
        load_mirrors(data_dir)
            .await
            .unwrap_or_else(|_| default_mirrors())
            .into_iter()
            .map(|m| m.url)
            .collect()
    };

    let mut last_err: Option<String> = None;
    let mut handled = false;
    'mirrors: for manifest_url in mirrors.iter() {
        for attempt in 1..=3u8 {
            if attempt > 1 {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            match client.get(manifest_url).send().await {
                Ok(resp) if resp.status().is_success() => match resp
                    .json::<serde_json::Value>()
                    .await
                    .map_err(anyhow::Error::from)
                    .and_then(parse_and_verify_manifest)
                {
                    Ok((mut manifest, signed)) => {
                        rewrite_manifest_origins(&mut manifest, manifest_url);
                        if is_newer(&manifest.version, &state.current_version) {
                            if !signed {
                                warn!(
                                    available = %manifest.version,
                                    mirror = %manifest_url,
                                    "Update manifest is NOT signed by the release root — \
                                     offering for manual apply only; auto-apply is refused"
                                );
                            }
                            info!(
                                current = %state.current_version,
                                available = %manifest.version,
                                mirror = %manifest_url,
                                signed,
                                "Update available"
                            );
                            state.available_update = Some(manifest);
                            state.manifest_mirror = Some(manifest_url.clone());
                            state.manifest_signed = signed;
                        } else {
                            // Manifest version matches us or is behind
                            // us — either we're current, or this mirror
                            // is stale. Try the next mirror; if all are
                            // stale or at our version we'll fall through
                            // to "up to date".
                            debug!(
                                current = %state.current_version,
                                manifest = %manifest.version,
                                mirror = %manifest_url,
                                "No newer version in manifest"
                            );
                            state.manifest_mirror = None;
                            state.available_update = None;
                            state.manifest_signed = false;
                            handled = true;
                            continue 'mirrors;
                        }
                        handled = true;
                        break 'mirrors;
                    }
                    Err(e) => last_err = Some(format!("{}: parse: {}", manifest_url, e)),
                },
                Ok(resp) => {
                    last_err = Some(format!("{}: HTTP {}", manifest_url, resp.status()));
                }
                Err(e) => {
                    last_err = Some(format!("{}: {}", manifest_url, e));
                }
            }
        }
        tracing::debug!(mirror = %manifest_url, "Mirror exhausted, trying next");
    }
    if !handled {
        if let Some(e) = last_err {
            debug!("Update check failed across all mirrors: {}", e);
        }
    }

    state.last_check = Some(chrono::Utc::now().to_rfc3339());
    save_state(data_dir, &state).await?;
    Ok(state)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorTestResult {
    pub reachable: bool,
    pub latency_ms: u64,
    pub http_status: Option<u16>,
    pub error: Option<String>,
}

/// Ping a mirror's manifest URL and return reachability + wall-clock
/// latency. Used by the "Test mirror" button so operators can sanity-
/// check a newly added mirror without running a full update check.
pub async fn test_mirror(url: &str) -> MirrorTestResult {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return MirrorTestResult {
                reachable: false,
                latency_ms: 0,
                http_status: None,
                error: Some(format!("client build failed: {}", e)),
            }
        }
    };
    let start = std::time::Instant::now();
    match client.get(url).send().await {
        Ok(resp) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            let status = resp.status();
            if status.is_success() {
                MirrorTestResult {
                    reachable: true,
                    latency_ms,
                    http_status: Some(status.as_u16()),
                    error: None,
                }
            } else {
                MirrorTestResult {
                    reachable: false,
                    latency_ms,
                    http_status: Some(status.as_u16()),
                    error: Some(format!("HTTP {}", status.as_u16())),
                }
            }
        }
        Err(e) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            MirrorTestResult {
                reachable: false,
                latency_ms,
                http_status: None,
                error: Some(e.to_string()),
            }
        }
    }
}

/// Get current update status without checking remote.
pub async fn get_status(data_dir: &Path) -> Result<UpdateState> {
    load_state(data_dir).await
}

/// Dismiss the available update notification.
pub async fn dismiss_update(data_dir: &Path) -> Result<()> {
    let mut state = load_state(data_dir).await?;
    state.available_update = None;
    save_state(data_dir, &state).await
}

/// Download update components to a staging directory.
/// Verifies SHA256 hash for each component.
///
/// Robustness: each component download is **resumable** via HTTP Range
/// requests and retried up to 6 times with exponential backoff. When
/// gitea drops the connection mid-stream (happens regularly at slow
/// raw-file throughput), the next attempt picks up where the previous
/// one left off instead of restarting from byte zero. SHA256 is
/// verified over the complete file at the end of each component, so a
/// partially-corrupt resume still fails cleanly.
pub async fn download_update(data_dir: &Path) -> Result<DownloadProgress> {
    let _op = UPDATE_OP_LOCK.try_lock().map_err(|_| {
        anyhow::anyhow!("Update already in progress — another download or apply is already running")
    })?;
    let mut state = load_state(data_dir).await?;
    if state.available_update.is_none() {
        state = check_for_updates(data_dir).await?;
    }
    let manifest = state
        .available_update
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No update is available to download"))?;

    let staging_dir = data_dir.join("update-staging");
    fs::create_dir_all(&staging_dir)
        .await
        .context("Failed to create staging dir")?;

    let client = reqwest::Client::builder()
        // Per-request budget; each attempt gets the full window, and a retry
        // resumes via Range from the partial file (download_component_resumable),
        // so this is an upper bound per attempt, not the whole download. Sized
        // generously for slow machines on slow links: a ~200MB release at a
        // crawling ~50KB/s is ~70min, which the old 1h budget could cut off
        // mid-attempt. 3h leaves ample headroom; raising it cannot slow down or
        // break a fast download (those finish well inside the old limit).
        .timeout(std::time::Duration::from_secs(10800))
        .connect_timeout(std::time::Duration::from_secs(60))
        .build()
        .context("Failed to create HTTP client")?;

    let mut downloaded = 0u64;
    let total_bytes: u64 = manifest.components.iter().map(|c| c.size_bytes).sum();

    info!(
        version = %manifest.version,
        components = manifest.components.len(),
        total_bytes,
        staging = %staging_dir.display(),
        "Starting update download"
    );

    // Clear any stale cancel flag from a prior aborted run, then seed
    // the live counters so polls during the handshake show the right
    // denominator immediately instead of 0/0 → NaN%.
    DOWNLOAD_CANCEL.store(false, Ordering::Relaxed);
    DOWNLOAD_TOTAL.store(total_bytes, Ordering::Relaxed);
    DOWNLOAD_BYTES.store(0, Ordering::Relaxed);
    DOWNLOAD_PROGRESS_AT.store(now_ms(), Ordering::Relaxed);

    // Consult swarm peers only when the node has opted into DHT mode. In Origin
    // mode (default) this stays empty so every component goes straight to the
    // HTTP origin — instant, no-rebuild fallback while live-testing the swarm.
    let update_source = load_update_source(data_dir).await;
    let provide_dht = load_provide_dht(data_dir).await;
    let swarm_providers = if update_source == UpdateSource::Swarm {
        crate::swarm::providers()
    } else {
        Vec::new()
    };
    if update_source == UpdateSource::Swarm {
        info!(
            providers = swarm_providers.len(),
            "Update source = DHT swarm (origin still wins as fallback)"
        );
    }

    for component in &manifest.components {
        if is_canceled() {
            DOWNLOAD_TOTAL.store(0, Ordering::Relaxed);
            DOWNLOAD_BYTES.store(0, Ordering::Relaxed);
            anyhow::bail!("Download canceled");
        }
        info!(name = %component.name, url = %component.download_url, "Downloading component");
        let dest = staging_dir.join(&component.name);

        // DHT Phase 2: when the manifest pins a BLAKE3 digest, route the fetch
        // through the swarm seam (swarm-assist, origin always wins). With no
        // providers registered (iroh-swarm feature off) this is identical to
        // calling the resumable HTTP origin directly — same bytes, now
        // content-addressed. A swarm hit is BLAKE3-verified inside the seam;
        // we still enforce the mandatory SHA-256 gate on peer bytes here and
        // re-fetch from origin if a (consistency-broken) peer slips through.
        let digest = component.blake3.as_deref().and_then(|b| {
            let s = b.trim();
            let normalized = if s.contains(':') {
                s.to_string()
            } else {
                format!("blake3:{s}")
            };
            crate::content_hash::ContentDigest::parse(&normalized).ok()
        });
        if let Some(digest) = digest {
            let client_ref = &client;
            let dest_ref = &dest;
            let source = crate::swarm::fetch_content_addressed(
                &digest,
                &swarm_providers,
                &dest,
                move || async move {
                    download_component_resumable(client_ref, component, dest_ref, downloaded).await
                },
            )
            .await?;
            if source == crate::swarm::FetchSource::Swarm {
                let bytes = tokio::fs::read(&dest).await?;
                if crate::content_hash::sha256_hex(&bytes) != component.sha256 {
                    warn!(
                        name = %component.name,
                        "swarm bytes passed BLAKE3 but failed the SHA-256 manifest gate — re-fetching from origin"
                    );
                    let _ = tokio::fs::remove_file(&dest).await;
                    download_component_resumable(&client, component, &dest, downloaded).await?;
                }
            }
            // This is a PUBLIC release blob and it just passed both the BLAKE3 and
            // SHA-256 gates — announce that we can now seed it to peers. Gated on
            // the node's "provide to swarm" preference (default on); best-effort,
            // inert unless the iroh swarm is active, and never blocks the install.
            // Independent of fetch source: an origin-fetching node can still seed.
            if provide_dht {
                crate::swarm::announce_held_blob(&digest.hex, &dest).await;
            }
        } else {
            download_component_resumable(&client, component, &dest, downloaded).await?;
        }
        downloaded += component.size_bytes;
        DOWNLOAD_BYTES.store(downloaded, Ordering::Relaxed);
        info!(
            name = %component.name,
            bytes = component.size_bytes,
            "Component downloaded and verified"
        );
    }

    // Mark update as downloaded. Write the completion marker FIRST so a crash
    // between the two can't leave update_in_progress=true without the marker
    // (which the self-heal would then clear, harmlessly forcing a re-download).
    let _ = fs::write(staging_dir.join(STAGED_COMPLETE_MARKER), b"1").await;
    let mut state = load_state(data_dir).await?;
    state.update_in_progress = true;
    save_state(data_dir, &state).await?;

    Ok(DownloadProgress {
        total_bytes,
        downloaded_bytes: downloaded,
        components_downloaded: manifest.components.len(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
    })
}

/// Download a single component to `dest`, resuming from the end of
/// any existing partial file via a Range request. Retries up to 6
/// times with exponential backoff (5s, 15s, 30s, 60s, 120s, 180s).
/// Verifies the SHA256 over the full file at the end.
async fn download_component_resumable(
    client: &reqwest::Client,
    component: &ComponentUpdate,
    dest: &Path,
    prior_total: u64,
) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    const MAX_ATTEMPTS: u32 = 6;
    const BACKOFFS: [u64; 5] = [5, 15, 30, 60, 120];

    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=MAX_ATTEMPTS {
        let existing_len = match tokio::fs::metadata(dest).await {
            Ok(m) => m.len(),
            Err(_) => 0,
        };
        if existing_len >= component.size_bytes {
            // File is already complete (a resumed run finished it, or a
            // leftover from an earlier attempt) — verify it instead of
            // trusting it. The old code `break`d here, which skipped
            // verification entirely AND landed on the error return below
            // ("download failed without a captured error").
            match verify_component_on_disk(component, dest).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    let _ = tokio::fs::remove_file(dest).await;
                    last_err = Some(e);
                    continue;
                }
            }
        }
        if attempt > 1 {
            let delay = BACKOFFS[(attempt as usize - 2).min(BACKOFFS.len() - 1)];
            tracing::warn!(
                name = %component.name,
                attempt,
                resume_at = existing_len,
                "Retrying download in {}s (previous error: {})",
                delay,
                last_err.as_ref().map(|e| e.to_string()).unwrap_or_default()
            );
            // Sleep in 500ms slices so a Cancel during backoff wakes
            // promptly instead of waiting out the full exponential window.
            let slices = delay * 2;
            for _ in 0..slices {
                if is_canceled() {
                    anyhow::bail!("Download canceled");
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
        if is_canceled() {
            anyhow::bail!("Download canceled");
        }

        let mut req = client.get(&component.download_url);
        if existing_len > 0 {
            req = req.header("Range", format!("bytes={}-", existing_len));
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                last_err = Some(anyhow::anyhow!(e));
                continue;
            }
        };
        let status = resp.status();
        // 200 OK on a fresh start, 206 Partial Content on a resume
        // that the server honoured. Anything else is a problem.
        let is_resume = existing_len > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
        let is_fresh = existing_len == 0 && status.is_success();
        let server_ignored_range = existing_len > 0 && status == reqwest::StatusCode::OK;
        if !is_resume && !is_fresh && !server_ignored_range {
            last_err = Some(anyhow::anyhow!(
                "HTTP {} for {} (resume offset {})",
                status,
                component.name,
                existing_len
            ));
            continue;
        }
        // If the server ignored Range (returned 200 with the full
        // body), wipe the partial file and start over.
        let mut file = if server_ignored_range {
            let _ = tokio::fs::remove_file(dest).await;
            tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(dest)
                .await
                .context("open staging file")?
        } else if is_resume {
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(dest)
                .await
                .context("open staging file for append")?
        } else {
            tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(dest)
                .await
                .context("open staging file")?
        };

        let mut resp = resp;
        let mut stream_err = false;
        let mut on_disk = existing_len;
        let mut canceled = false;
        loop {
            if is_canceled() {
                canceled = true;
                break;
            }
            match resp.chunk().await {
                Ok(Some(bytes)) => {
                    if let Err(e) = file.write_all(&bytes).await {
                        last_err = Some(anyhow::anyhow!(e).context("writing chunk"));
                        stream_err = true;
                        break;
                    }
                    on_disk += bytes.len() as u64;
                    DOWNLOAD_BYTES.store(
                        prior_total + on_disk.min(component.size_bytes),
                        Ordering::Relaxed,
                    );
                    DOWNLOAD_PROGRESS_AT.store(now_ms(), Ordering::Relaxed);
                }
                Ok(None) => break, // stream ended cleanly
                Err(e) => {
                    last_err = Some(anyhow::anyhow!(e).context("reading chunk"));
                    stream_err = true;
                    break;
                }
            }
        }
        if canceled {
            // Best-effort: the partial file is re-validated (size + sha)
            // when the download resumes, so a lost tail here is harmless.
            let _ = file.flush().await;
            drop(file);
            DOWNLOAD_TOTAL.store(0, Ordering::Relaxed);
            DOWNLOAD_BYTES.store(0, Ordering::Relaxed);
            anyhow::bail!("Download canceled");
        }
        // A failed flush/sync surfaces later as a size/sha mismatch and a
        // resume, but log it — this is where ENOSPC first shows itself.
        if let Err(e) = file.flush().await {
            warn!("Staging file flush failed: {e:#}");
        }
        if let Err(e) = file.sync_all().await {
            warn!("Staging file sync failed: {e:#}");
        }
        drop(file);
        if stream_err {
            continue;
        }

        // Stream ended cleanly. If we've got the expected size, verify
        // the SHA and succeed. Otherwise loop to resume from the new
        // offset on the next attempt.
        let final_len = tokio::fs::metadata(dest)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        if final_len < component.size_bytes {
            last_err = Some(anyhow::anyhow!(
                "download truncated: got {} of {} bytes",
                final_len,
                component.size_bytes
            ));
            continue;
        }

        // Full file — verify hashes. On mismatch the file on disk is
        // garbage: nuke it and start over from scratch on the next attempt.
        match verify_component_on_disk(component, dest).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                let _ = tokio::fs::remove_file(dest).await;
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("download failed without a captured error")))
}

/// Verify a fully-downloaded component file on disk: SHA-256 is the
/// mandatory gate; when the manifest also pins a BLAKE3 digest it must
/// match too (BLAKE3 is the hash the iroh swarm fetches/verifies by, so
/// a present-but-wrong BLAKE3 means the bytes aren't swarm-consistent —
/// treated exactly like a SHA mismatch). Err = mismatch; the caller
/// decides whether to remove the file and retry.
async fn verify_component_on_disk(component: &ComponentUpdate, dest: &Path) -> Result<()> {
    use sha2::{Digest, Sha256};
    let bytes = tokio::fs::read(dest)
        .await
        .context("read staging file for hash check")?;
    let hash = hex::encode(Sha256::digest(&bytes));
    if hash != component.sha256 {
        anyhow::bail!(
            "SHA256 mismatch for {}: expected {}, got {}",
            component.name,
            component.sha256,
            hash
        );
    }
    if let Some(b3) = component.blake3.as_deref() {
        let expected = b3.trim().strip_prefix("blake3:").unwrap_or(b3.trim());
        let actual = crate::content_hash::blake3_hex(&bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            anyhow::bail!(
                "BLAKE3 mismatch for {}: expected {}, got {}",
                component.name,
                expected,
                actual
            );
        }
    }
    Ok(())
}

/// Re-verify every manifest component against the bytes actually sitting
/// in staging, immediately before install. The download path verifies as
/// it goes, but staging can change between download and apply — on .198
/// (v1.7.103, 2026-07-18) a concurrent download was re-filling a wiped
/// staging dir when apply ran, and a 17MB partial of the 49MB binary got
/// installed. This apply-time gate is the one that must never be skipped.
async fn verify_staged_components(staging_dir: &Path, manifest: &UpdateManifest) -> Result<()> {
    for component in &manifest.components {
        let dest = staging_dir.join(&component.name);
        let len = tokio::fs::metadata(&dest)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        if len != component.size_bytes {
            anyhow::bail!(
                "Update staging is inconsistent: component {} is {} bytes but the manifest says {} — \
                 re-download before applying (incomplete or concurrently-rewritten download)",
                component.name,
                len,
                component.size_bytes
            );
        }
        verify_component_on_disk(component, &dest)
            .await
            .with_context(|| {
                format!(
                    "staged component {} failed verification — refusing to apply",
                    component.name
                )
            })?;
    }
    Ok(())
}

/// Cancel an in-flight download. Sets the cancellation flag so the
/// download loop bails out at the next chunk or backoff boundary, then
/// zeros the live counters and wipes the staging directory so the UI
/// sees "no active download" immediately and the next attempt starts
/// clean. Safe to call even when no download is running.
pub async fn cancel_download(data_dir: &Path) -> Result<()> {
    DOWNLOAD_CANCEL.store(true, Ordering::Relaxed);
    DOWNLOAD_BYTES.store(0, Ordering::Relaxed);
    DOWNLOAD_TOTAL.store(0, Ordering::Relaxed);
    // Only wipe staging when no download/apply holds the op lock. Wiping
    // under a live operation is how .198 ended up applying a re-filling
    // staging dir; with the lock held elsewhere we just set the cancel
    // flag and let the in-flight loop bail at its next chunk boundary
    // (partials are size+hash revalidated on the next resume anyway).
    let staging = data_dir.join("update-staging");
    let wiped = match UPDATE_OP_LOCK.try_lock() {
        Ok(_op) => {
            if staging.exists() {
                tokio::fs::remove_dir_all(&staging).await.is_ok()
            } else {
                false
            }
        }
        Err(_) => false,
    };
    // Clear the "downloaded, ready to apply" marker too — a canceled
    // download is not a staged update.
    let mut cleared_marker = false;
    if let Ok(mut state) = load_state(data_dir).await {
        if state.update_in_progress {
            state.update_in_progress = false;
            if let Err(e) = save_state(data_dir, &state).await {
                warn!("Failed to persist cleared update_in_progress: {e:#}");
            }
            cleared_marker = true;
        }
    }
    info!(
        staging = %staging.display(),
        wiped,
        cleared_marker,
        "Update download canceled"
    );
    Ok(())
}

/// Run a command as root, but *outside* the archipelago service's
/// restricted mount namespace.
///
/// archipelago.service uses `ProtectSystem=strict`, which makes `/opt`
/// and `/usr` read-only inside the service — and sudo inherits the
/// namespace, so `sudo mv /opt/archipelago/...` fails with EROFS even
/// though sudo itself is root. `systemd-run --wait` spawns a transient
/// service unit that inherits systemd's default protections (i.e. none
/// of ours), escaping the namespace.
pub(crate) async fn host_sudo(args: &[&str]) -> Result<std::process::ExitStatus> {
    let mut full: Vec<&str> = vec![
        "systemd-run",
        "--wait",
        "--quiet",
        "--collect",
        "--pipe",
        // Shell snippets passed as one argument must reach the child intact.
        // systemd-run otherwise expands $VAR/${VAR} against the manager's
        // environment before `sh -lc` can see them (and usually replaces them
        // with empty strings).
        "--expand-environment=no",
        "--",
    ];
    full.extend_from_slice(args);
    tokio::process::Command::new("sudo")
        .args(&full)
        .status()
        .await
        .context("sudo systemd-run spawn failed")
}

/// Same mechanism as `host_sudo` but captures stdout — for read-only probes
/// (e.g. `stat`) where the answer is in the output, not the exit status.
pub(crate) async fn host_sudo_output(args: &[&str]) -> Result<std::process::Output> {
    let mut full: Vec<&str> = vec![
        "systemd-run",
        "--wait",
        "--quiet",
        "--collect",
        "--pipe",
        "--expand-environment=no",
        "--",
    ];
    full.extend_from_slice(args);
    tokio::process::Command::new("sudo")
        .args(&full)
        .output()
        .await
        .context("sudo systemd-run output spawn failed")
}

/// Apply a downloaded update. Backs up current binaries, replaces with staged versions.
pub async fn apply_update(data_dir: &Path) -> Result<()> {
    let _op = UPDATE_OP_LOCK.try_lock().map_err(|_| {
        anyhow::anyhow!("Update already in progress — another download or apply is already running")
    })?;
    let staging_dir = data_dir.join("update-staging");
    if !staging_dir.exists() {
        anyhow::bail!("Update not staged — download it first, then apply.");
    }

    // Gate 1: the completion marker is written only after EVERY component
    // downloaded and hash-verified. A staging dir without it is a partial
    // or in-flight download — exactly what got installed on .198.
    if !has_staged_update(data_dir).await {
        anyhow::bail!(
            "Update download was incomplete (no completion marker) — download the update again before applying"
        );
    }

    // Gate 2: re-verify the actual staged bytes against the manifest.
    let manifest = load_state(data_dir)
        .await?
        .available_update
        .ok_or_else(|| {
            anyhow::anyhow!("Update manifest missing from state — re-download the update")
        })?;
    verify_staged_components(&staging_dir, &manifest).await?;

    let backup_dir = data_dir.join("update-backup");
    fs::create_dir_all(&backup_dir)
        .await
        .context("Failed to create backup dir")?;

    info!(
        staging = %staging_dir.display(),
        backup = %backup_dir.display(),
        "Applying staged update"
    );

    // Back up current backend binary
    let current_binary = Path::new("/usr/local/bin/archipelago");
    if current_binary.exists() {
        let backup_path = backup_dir.join("archipelago");
        // A leftover backup from an earlier rollback can be root-owned
        // (rollback used to chown it in place), and fs::copy O_TRUNCs the
        // existing file — EACCES as the service user, wedging every apply
        // (seen on .116, v1.7.86 OTA). Unlink first; the dir is
        // service-owned so unlink works even when the file isn't ours.
        if backup_path.exists() {
            if let Err(e) = fs::remove_file(&backup_path).await {
                tracing::warn!(error = %e, "unlink of stale binary backup failed, retrying via host_sudo");
                let _ = host_sudo(&["rm", "-f", &backup_path.to_string_lossy()]).await;
            }
        }
        fs::copy(current_binary, &backup_path)
            .await
            .context("Failed to backup current binary")?;
        info!("Current binary backed up");
    }

    // Apply staged components in a DETERMINISTIC order, binary LAST.
    // read_dir order is filesystem-arbitrary, and each component used to be
    // consumed destructively — so a mid-apply failure could leave staging
    // half-emptied and un-reappliable (Gate 2 re-verifies EVERY manifest
    // component against staging, so a missing one wedges every retry:
    // "doesn't apply, still says install, can never apply again"). Two
    // guards against that now: (a) nothing is removed from staging here —
    // the binary is copied, not moved (see its block) — so a failed apply
    // is always retryable from the same staged files; (b) the binary, the
    // one component whose swap changes what runs after restart, is applied
    // only after the frontend/runtime succeed, so a frontend failure never
    // leaves a new binary staged to run against an old frontend on the next
    // restart.
    let mut names: Vec<String> = Vec::new();
    {
        let mut entries = fs::read_dir(&staging_dir)
            .await
            .context("Failed to read staging dir")?;
        while let Some(entry) = entries.next_entry().await? {
            names.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    names.sort_by_key(|n| match n.as_str() {
        "archipelago" => 2, // binary last
        n if n.contains("runtime") && n.ends_with(".tar.gz") => 1,
        _ => 0, // frontend and everything else first
    });

    for name in &names {
        let name = name.as_str();
        let src = staging_dir.join(name);

        match name {
            "archipelago" => {
                // Three constraints this block works around:
                //   1. We're running FROM /usr/local/bin/archipelago, so
                //      `install`/`cp` (O_TRUNC + write) fail with ETXTBSY.
                //      rename() over a busy destination is fine.
                //   2. archipelago.service sets ProtectSystem=strict, so
                //      even `sudo mv` into /usr/local/bin/ fails EROFS —
                //      sudo inherits the service's mount namespace. Route
                //      through host_sudo (systemd-run transient unit with
                //      default protections).
                //   3. The staged binary must SURVIVE this so a later
                //      component's failure leaves the apply retryable. So we
                //      COPY the staged file to a sibling temp in the target
                //      dir, then atomic-rename the temp over the target —
                //      the staging copy is never moved. (mv'ing the staged
                //      file itself was the wedging bug: binary applied, then
                //      frontend fails, staging now missing the binary, every
                //      retry fails re-verification forever.)
                let staged = src.to_string_lossy().to_string();
                let tmp = format!(
                    "/usr/local/bin/.archipelago.new.{}",
                    chrono::Utc::now().timestamp_millis()
                );
                let cp = host_sudo(&["cp", "-f", &staged, &tmp])
                    .await
                    .with_context(|| format!("Failed to copy staged binary for {}", name))?;
                if !cp.success() {
                    let _ = host_sudo(&["rm", "-f", &tmp]).await;
                    anyhow::bail!("copy of staged binary failed for {}", name);
                }
                let _ = host_sudo(&["chmod", "0755", &tmp]).await;
                let _ = host_sudo(&["chown", "root:root", &tmp]).await;
                let status = host_sudo(&["mv", &tmp, "/usr/local/bin/archipelago"])
                    .await
                    .with_context(|| format!("Failed to spawn mv for {}", name))?;
                if !status.success() {
                    let _ = host_sudo(&["rm", "-f", &tmp]).await;
                    anyhow::bail!(
                        "mv into /usr/local/bin failed for {} (exit {:?})",
                        name,
                        status.code()
                    );
                }
                info!(name = %name, "Backend binary applied (staging preserved)");
            }
            _ if name.contains("frontend") && name.ends_with(".tar.gz") => {
                // Tarball contents are the *inside* of web-ui/ (root entries
                // `./test-aiui.html`, `./assets/`, ...). Extract into a
                // uniquely-named staging dir, then mv into place. No `rm
                // -rf` pre-cleanup — that's what hit transient EROFS on
                // .198 and aborted the apply mid-flight.
                let ts = chrono::Utc::now().timestamp_millis();
                let staging_new = format!("/opt/archipelago/web-ui.new.{}", ts);
                let staging_old = format!("/opt/archipelago/web-ui.old.{}", ts);
                let web_ui = "/opt/archipelago/web-ui";
                let backup_path = "/opt/archipelago/web-ui.bak";

                // All sudo calls that touch /opt/archipelago go through
                // host_sudo so they see a normal root mount namespace.
                let mk = host_sudo(&["mkdir", "-p", &staging_new])
                    .await
                    .context("Failed to create frontend staging dir")?;
                if !mk.success() {
                    anyhow::bail!("mkdir {} failed", staging_new);
                }
                // Idle-priority IO: extracting a 200MB tarball at normal
                // priority starved small nodes so badly during the v1.7.118
                // rollout that podman/status calls timed out for minutes and
                // the update looked broken. The extraction can take as long
                // as it likes — the node staying responsive matters more.
                let extract = host_sudo(&[
                    "ionice",
                    "-c3",
                    "nice",
                    "-n10",
                    "tar",
                    "-xzf",
                    &src.to_string_lossy(),
                    "-C",
                    &staging_new,
                ])
                .await
                .with_context(|| format!("Failed to extract {}", name))?;
                if !extract.success() {
                    let _ = host_sudo(&["rm", "-rf", &staging_new]).await;
                    anyhow::bail!("tar extraction failed for {}", name);
                }
                let _ = host_sudo(&["chown", "-R", "archipelago:archipelago", &staging_new]).await;

                // Set world-readable perms so nginx (runs as www-data)
                // can stat + serve the files. Without this, the tar
                // extraction inherits the staging-dir's 700 mode and
                // nginx returns 403/500 for every request after the
                // swap — exactly what bit .116 on the v1.7.38 rollout.
                let _ = host_sudo(&["chmod", "755", &staging_new]).await;
                let _ = host_sudo(&[
                    "find",
                    &staging_new,
                    "-type",
                    "d",
                    "-exec",
                    "chmod",
                    "755",
                    "{}",
                    "+",
                ])
                .await;
                let _ = host_sudo(&[
                    "find",
                    &staging_new,
                    "-type",
                    "f",
                    "-exec",
                    "chmod",
                    "644",
                    "{}",
                    "+",
                ])
                .await;

                // Preserve paths that are installed outside the Vue build
                // (baked in by the ISO or sibling installers) and so
                // aren't in the new tarball. Without this copy, every OTA
                // wipes them — notably aiui/ (Claude Code sidebar) and
                // the companion APK. `cp -a` preserves mode/ownership.
                for preserved in ["aiui", "archipelago-companion.apk"] {
                    let src = format!("{}/{}", web_ui, preserved);
                    let dst = format!("{}/{}", staging_new, preserved);
                    // Only preserve the old copy if the new tarball
                    // doesn't already ship a fresher one.
                    if Path::new(&src).exists() && !Path::new(&dst).exists() {
                        let _ = host_sudo(&["cp", "-a", &src, &dst]).await;
                    }
                }

                // Swap: mv current web-ui aside, then mv new into place.
                if Path::new(web_ui).exists() {
                    let mv_old = host_sudo(&["mv", web_ui, &staging_old])
                        .await
                        .context("Failed to rotate old web-ui")?;
                    if !mv_old.success() {
                        anyhow::bail!("failed to move old web-ui aside");
                    }
                }
                let mv_new = host_sudo(&["mv", &staging_new, web_ui])
                    .await
                    .context("Failed to swap new web-ui into place")?;
                if !mv_new.success() {
                    if Path::new(&staging_old).exists() {
                        let _ = host_sudo(&["mv", &staging_old, web_ui]).await;
                    }
                    anyhow::bail!("failed to move new web-ui into place");
                }

                // Rotate previous rollback aside and install this apply's
                // old copy as the new rollback.
                if Path::new(&staging_old).exists() {
                    if Path::new(backup_path).exists() {
                        let _ = host_sudo(&["mv", backup_path, &format!("{}.{}", backup_path, ts)])
                            .await;
                    }
                    let _ = host_sudo(&["mv", &staging_old, backup_path]).await;
                }
                info!(name = %name, "Frontend archive extracted to /opt/archipelago/web-ui");
            }
            _ if name.contains("runtime") && name.ends_with(".tar.gz") => {
                let ts = chrono::Utc::now().timestamp_millis();
                let staging_new = format!("/opt/archipelago/runtime.new.{}", ts);
                let archive = src.to_string_lossy().to_string();

                let mk = host_sudo(&["mkdir", "-p", &staging_new])
                    .await
                    .context("Failed to create runtime staging dir")?;
                if !mk.success() {
                    anyhow::bail!("mkdir {} failed", staging_new);
                }

                // Idle-priority IO — same reasoning as the frontend tarball.
                let extract = host_sudo(&[
                    "ionice",
                    "-c3",
                    "nice",
                    "-n10",
                    "tar",
                    "-xzf",
                    &archive,
                    "-C",
                    &staging_new,
                ])
                .await
                .with_context(|| format!("Failed to extract {}", name))?;
                if !extract.success() {
                    let _ = host_sudo(&["rm", "-rf", &staging_new]).await;
                    anyhow::bail!("tar extraction failed for {}", name);
                }

                let runtime_paths = [
                    ("apps", "apps"),
                    ("scripts", "scripts"),
                    ("docker", "docker"),
                    (
                        "image-recipe/configs/archipelago-doctor.service",
                        "archipelago-doctor.service",
                    ),
                    (
                        "image-recipe/configs/archipelago-doctor.timer",
                        "archipelago-doctor.timer",
                    ),
                ];

                for (relative, label) in runtime_paths {
                    let staged_path = format!("{}/{}", staging_new, relative);
                    if !Path::new(&staged_path).exists() {
                        tracing::debug!(path = %relative, "Runtime artifact path absent, skipping");
                        continue;
                    }

                    match label {
                        "apps" | "scripts" | "docker" => {
                            let dest = format!("/opt/archipelago/{}", label);
                            let tmp_dest =
                                format!("{}.new.{}", dest, chrono::Utc::now().timestamp_millis());
                            let _ = host_sudo(&["mkdir", "-p", &tmp_dest]).await;
                            let staged_dot = format!("{}/.", staged_path);
                            let copy = host_sudo(&["cp", "-a", &staged_dot, &tmp_dest])
                                .await
                                .with_context(|| format!("Failed to copy runtime {}", label))?;
                            if !copy.success() {
                                let _ = host_sudo(&["rm", "-rf", &tmp_dest]).await;
                                anyhow::bail!("runtime copy failed for {}", label);
                            }
                            let _ = host_sudo(&["mkdir", "-p", &dest]).await;
                            let clean = host_sudo(&[
                                "find",
                                &dest,
                                "-mindepth",
                                "1",
                                "-maxdepth",
                                "1",
                                "-exec",
                                "rm",
                                "-rf",
                                "{}",
                                "+",
                            ])
                            .await
                            .with_context(|| format!("Failed to clean runtime {}", label))?;
                            if !clean.success() {
                                let _ = host_sudo(&["rm", "-rf", &tmp_dest]).await;
                                anyhow::bail!("runtime clean failed for {}", label);
                            }
                            let tmp_dot = format!("{}/.", tmp_dest);
                            let promote = host_sudo(&["cp", "-a", &tmp_dot, &dest])
                                .await
                                .with_context(|| format!("Failed to promote runtime {}", label))?;
                            let _ = host_sudo(&["rm", "-rf", &tmp_dest]).await;
                            if !promote.success() {
                                anyhow::bail!("runtime promote failed for {}", label);
                            }
                            if label == "scripts" {
                                let _ = host_sudo(&[
                                    "find", &dest, "-type", "f", "-name", "*.sh", "-exec", "chmod",
                                    "755", "{}", "+",
                                ])
                                .await;
                            }
                        }
                        "archipelago-doctor.service" | "archipelago-doctor.timer" => {
                            let dest = format!("/etc/systemd/system/{}", label);
                            let install = host_sudo(&["install", "-m", "644", &staged_path, &dest])
                                .await
                                .with_context(|| format!("Failed to install {}", label))?;
                            if !install.success() {
                                anyhow::bail!("runtime unit install failed for {}", label);
                            }
                        }
                        _ => {}
                    }
                }

                if Path::new(&format!("{}/scripts/image-versions.sh", staging_new)).exists() {
                    let _ = host_sudo(&[
                        "cp",
                        &format!("{}/scripts/image-versions.sh", staging_new),
                        "/opt/archipelago/image-versions.sh",
                    ])
                    .await;
                }

                // Install the OTA crash-loop guard as a drop-in on existing
                // nodes (fresh ISOs carry it in the unit file itself). The
                // guard restores the update-backup binary when a freshly
                // applied binary SEGVs before it can run its own post-OTA
                // verification — the .198 v1.7.103 truncated-binary loop.
                // Best-effort: `+-` in the drop-in means a missing script can
                // never block the service, and a failed install here must not
                // abort the apply.
                if Path::new("/opt/archipelago/scripts/ota-crash-guard.sh").exists() {
                    let dropin_dir = "/etc/systemd/system/archipelago.service.d";
                    let _ = host_sudo(&["mkdir", "-p", dropin_dir]).await;
                    let _ = host_sudo(&[
                        "bash",
                        "-c",
                        &format!(
                            "printf '%s\\n' '[Service]' \
                             'ExecStartPre=+-/opt/archipelago/scripts/ota-crash-guard.sh' \
                             > {}/ota-crash-guard.conf",
                            dropin_dir
                        ),
                    ])
                    .await;
                }

                let _ = host_sudo(&["systemctl", "daemon-reload"]).await;
                let _ =
                    host_sudo(&["systemctl", "enable", "--now", "archipelago-doctor.timer"]).await;
                let _ = host_sudo(&["rm", "-rf", &staging_new]).await;
                info!(name = %name, "Runtime assets applied to /opt/archipelago");
            }
            _ => {
                debug!(name = %name, "Unknown component, skipping");
            }
        }
    }

    // Update state
    let previous_version = {
        let state = load_state(data_dir).await?;
        state.current_version.clone()
    };
    let mut state = load_state(data_dir).await?;
    let new_version = if let Some(manifest) = &state.available_update {
        state.current_version = manifest.version.clone();
        manifest.version.clone()
    } else {
        state.current_version.clone()
    };
    state.available_update = None;
    state.update_in_progress = false;
    state.rollback_available = true;
    save_state(data_dir, &state).await?;

    // Write the post-OTA verification marker BEFORE we schedule the
    // restart. The new binary will read it on startup, probe the
    // frontend, and auto-rollback if nginx is serving 5xx. Covers the
    // class of failure where "apply succeeds, restart succeeds, but
    // the UI is dead" (v1.7.38/39 tarball-perms bug). Best-effort —
    // a failed marker write shouldn't abort the apply.
    let marker = PendingVerification {
        applied_at: chrono::Utc::now().to_rfc3339(),
        new_version,
        previous_version,
        deadline_ts: chrono::Utc::now().timestamp() + PENDING_VERIFY_WINDOW_SECS as i64 + 60,
    };
    if let Err(e) = write_pending_verification(data_dir, &marker).await {
        tracing::warn!(error = %e, "Failed to write post-OTA verify marker — rollback disabled for this OTA");
    } else {
        info!("Post-OTA verify marker written; new binary will probe on boot");
    }

    // Clean staging
    let _ = fs::remove_dir_all(&staging_dir).await;

    info!("Update applied — scheduling service restart in 2s so the RPC reply lands first");

    // Restart asynchronously so the JSON-RPC response actually reaches the
    // UI before systemd kills us. --no-block makes sure systemctl doesn't
    // try to wait for the current service (us) to exit cleanly before
    // starting the new process — it would deadlock otherwise.
    // PID1-owned timer transient: submit NOW (synchronously, while we are
    // definitely alive), fire in 2s from systemd itself. The old approach —
    // tokio sleep + `systemd-run --wait -- systemctl --no-block restart` —
    // ran as a child of the process being stopped; on v1.7.114->115 the
    // stop landed but the start never fired and the node sat dead all
    // night. A timer unit owned by PID1 cannot be killed by our own death,
    // and Restart=always on the unit is the second net.
    let submitted = tokio::process::Command::new("sudo")
        .args([
            "systemd-run",
            "--collect",
            "--on-active=2",
            "--timer-property=AccuracySec=100ms",
            "--",
            "systemctl",
            "restart",
            "archipelago",
        ])
        .status()
        .await;
    match submitted {
        Ok(st) if st.success() => {}
        other => {
            tracing::warn!(
                "detached restart submission failed ({other:?}) — falling back to in-process restart"
            );
            tokio::spawn(async {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let _ = host_sudo(&["systemctl", "--no-block", "restart", "archipelago"]).await;
            });
        }
    }

    Ok(())
}

/// Rollback to the previous version from backup.
pub async fn rollback_update(data_dir: &Path) -> Result<()> {
    let backup_dir = data_dir.join("update-backup");
    if !backup_dir.exists() {
        anyhow::bail!("No rollback backup available");
    }

    let backup_binary = backup_dir.join("archipelago");
    if backup_binary.exists() {
        // Same two namespace gotchas as apply_update()'s binary swap:
        // `cp` straight onto the running binary is O_TRUNC and fails
        // ETXTBSY (exit 1 — exactly what broke the .116 rollback), and
        // plain sudo inherits ProtectSystem=strict, so everything goes
        // through host_sudo. Copy to a temp name on the same filesystem,
        // fix ownership on the TEMP file (never the stored backup — an
        // in-place chown is what later wedged apply_update), then mv,
        // which is an atomic rename and tolerates a busy destination.
        let backup_str = backup_binary.to_string_lossy().to_string();
        let tmp = format!(
            "/usr/local/bin/.archipelago.rollback.{}",
            chrono::Utc::now().timestamp_millis()
        );
        let copy = host_sudo(&["cp", &backup_str, &tmp])
            .await
            .context("Failed to stage backup binary via host_sudo")?;
        if !copy.success() {
            anyhow::bail!(
                "cp backup binary to {} failed (exit {:?})",
                tmp,
                copy.code()
            );
        }
        let _ = host_sudo(&["chmod", "0755", &tmp]).await;
        let _ = host_sudo(&["chown", "root:root", &tmp]).await;
        let status = host_sudo(&["mv", &tmp, "/usr/local/bin/archipelago"])
            .await
            .context("Failed to restore backup binary via host_sudo")?;
        if !status.success() {
            let _ = host_sudo(&["rm", "-f", &tmp]).await;
            anyhow::bail!(
                "mv backup binary into /usr/local/bin failed (exit {:?})",
                status.code()
            );
        }
        info!("Binary rolled back to previous version");
    }

    let mut state = load_state(data_dir).await?;
    state.rollback_available = false;
    save_state(data_dir, &state).await?;

    let _ = fs::remove_dir_all(&backup_dir).await;

    info!("Rollback complete. Restart service to take effect.");
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub components_downloaded: usize,
    pub staging_dir: String,
}

/// Set the update schedule preference.
pub async fn set_schedule(data_dir: &Path, schedule: UpdateSchedule) -> Result<()> {
    let mut state = load_state(data_dir).await?;
    state.schedule = schedule;
    save_state(data_dir, &state).await?;
    info!(schedule = ?schedule, "Update schedule changed");
    Ok(())
}

/// Get the current schedule.
pub async fn get_schedule(data_dir: &Path) -> Result<UpdateSchedule> {
    let state = load_state(data_dir).await?;
    Ok(state.schedule)
}

/// Background update scheduler. Runs in a loop, checking/applying based on schedule.
/// Call this once at startup via `tokio::spawn`.
/// Apply per-app auto-update-to-latest for apps the runner opted in
/// (`docs/bitcoin-multi-version-design.md` §3 Phase 3). Independent of the
/// binary OTA schedule below. Conservative: only upgrades an app when the fresh
/// catalog actually advertises a newer image than the one running, and only via
/// the orchestrator's normal upgrade lifecycle (the same safe path as the
/// manual "Update" button). Pinned apps are excluded upstream in
/// `auto_update_apps()`. Best-effort — failures are logged, never fatal.
async fn apply_per_app_auto_updates(
    orchestrator: &Option<std::sync::Arc<dyn crate::container::traits::ContainerOrchestrator>>,
) {
    let Some(orchestrator) = orchestrator.as_ref() else {
        return;
    };
    for app_id in crate::container::version_config::auto_update_apps() {
        // Determine the version actually running by inspecting the backend
        // container's image. Skip when not installed / unreadable.
        let running_image = ["", "archy-"]
            .iter()
            .map(|p| format!("{p}{app_id}"))
            .collect::<Vec<_>>();
        let mut current_image = None;
        for name in &running_image {
            if let Ok(out) = tokio::process::Command::new("podman")
                .args(["inspect", name, "--format", "{{.ImageName}}"])
                .output()
                .await
            {
                if out.status.success() {
                    let img = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !img.is_empty() {
                        current_image = Some(img);
                        break;
                    }
                }
            }
        }
        let Some(current_image) = current_image else {
            continue;
        };
        // Only act when the catalog advertises a genuine update over what's
        // running (this also re-checks the pin guard inside the helper).
        if crate::container::app_catalog::available_update_for_app(&app_id, &current_image)
            .is_none()
        {
            continue;
        }
        info!(
            "auto-update: {} has a newer catalog image (running {}), upgrading",
            app_id, current_image
        );
        match orchestrator.upgrade(&app_id).await {
            Ok(()) => info!("auto-update: {} upgraded to catalog latest", app_id),
            Err(e) => warn!("auto-update: {} upgrade failed: {}", app_id, e),
        }
    }
}

/// After a catalog refresh that changed the cached bytes, rebuild the
/// orchestrator's manifest map so registry-shipped manifest changes take
/// effect now instead of at the next service restart.
async fn reload_manifests_if_changed(
    refresh: crate::container::app_catalog::CatalogRefresh,
    orchestrator: &Option<std::sync::Arc<dyn crate::container::traits::ContainerOrchestrator>>,
) {
    if !refresh.changed {
        return;
    }
    let Some(orch) = orchestrator else { return };
    match orch.reload_manifests().await {
        Ok(n) => info!("Update scheduler: catalog changed, reloaded {n} manifest(s)"),
        Err(e) => warn!("Update scheduler: manifest reload after catalog change failed: {e}"),
    }
}

pub async fn run_update_scheduler(
    data_dir: std::path::PathBuf,
    orchestrator: Option<std::sync::Arc<dyn crate::container::traits::ContainerOrchestrator>>,
) {
    use tokio::time::{interval, Duration};

    // Check every hour; act based on schedule setting
    let mut tick = interval(Duration::from_secs(3600));

    // Refresh the app catalog once at startup so per-app "update available"
    // badges appear without waiting for the first hourly tick.
    match crate::container::app_catalog::refresh_catalog(&data_dir).await {
        Ok(refresh) => reload_manifests_if_changed(refresh, &orchestrator).await,
        Err(e) => debug!(
            "Update scheduler: initial app-catalog refresh failed: {}",
            e
        ),
    }

    loop {
        tick.tick().await;

        // App-catalog refresh is INDEPENDENT of the OTA schedule below: it only
        // populates per-app update availability (the "Update" button still has
        // to be clicked — nothing auto-applies). Best-effort; on failure the
        // previously cached catalog stays in place (origin-always-wins).
        // A changed catalog also reloads the orchestrator's manifest overlay so
        // catalog-shipped manifest fixes apply without a service restart.
        match crate::container::app_catalog::refresh_catalog(&data_dir).await {
            Ok(refresh) => reload_manifests_if_changed(refresh, &orchestrator).await,
            Err(e) => debug!("Update scheduler: app-catalog refresh failed: {}", e),
        }

        // Per-app auto-update-to-latest (multi-version support). Runs every tick
        // regardless of the binary-OTA schedule below; opt-in + pin-respecting.
        apply_per_app_auto_updates(&orchestrator).await;

        let state = match load_state(&data_dir).await {
            Ok(s) => s,
            Err(e) => {
                debug!("Update scheduler: failed to load state: {}", e);
                continue;
            }
        };

        match state.schedule {
            UpdateSchedule::Manual => {
                debug!("Update scheduler: manual mode, skipping");
                continue;
            }
            UpdateSchedule::DailyCheck => {
                // Only check once per day
                if let Some(ref last) = state.last_check {
                    if let Ok(last_time) = chrono::DateTime::parse_from_rfc3339(last) {
                        let elapsed = chrono::Utc::now() - last_time.with_timezone(&chrono::Utc);
                        if elapsed.num_hours() < 24 {
                            debug!("Update scheduler: checked recently, skipping");
                            continue;
                        }
                    }
                }
                info!("Update scheduler: running daily check");
                if let Err(e) = check_for_updates(&data_dir).await {
                    debug!("Update scheduler: check failed: {}", e);
                }
            }
            UpdateSchedule::AutoApply => {
                // Auto-apply: check, download, and apply during 3 AM window
                let hour = chrono::Local::now().hour();
                if hour != 3 {
                    // Still do daily check outside the window
                    if let Some(ref last) = state.last_check {
                        if let Ok(last_time) = chrono::DateTime::parse_from_rfc3339(last) {
                            let elapsed =
                                chrono::Utc::now() - last_time.with_timezone(&chrono::Utc);
                            if elapsed.num_hours() < 24 {
                                continue;
                            }
                        }
                    }
                    info!("Update scheduler: auto-apply check (outside window)");
                    if let Err(e) = check_for_updates(&data_dir).await {
                        debug!("Update scheduler: check failed: {}", e);
                    }
                    continue;
                }

                // 3 AM — check, download, and apply
                info!("Update scheduler: 3 AM auto-apply window");
                match check_for_updates(&data_dir).await {
                    Ok(s) if s.available_update.is_some() => {
                        if !s.manifest_signed {
                            // Unattended apply of an unauthenticated manifest is
                            // the §A supply-chain hole: a mirror/MITM could ship
                            // an arbitrary root binary fleet-wide at 3 AM. Manual
                            // apply from the UI remains possible during the
                            // signing-migration window; auto-apply does not.
                            warn!(
                                available = %s
                                    .available_update
                                    .as_ref()
                                    .map(|m| m.version.as_str())
                                    .unwrap_or("?"),
                                "Update scheduler: manifest is not signed by the \
                                 release root — refusing unattended auto-apply"
                            );
                            continue;
                        }
                        info!("Update scheduler: downloading update");
                        if let Err(e) = download_update(&data_dir).await {
                            debug!("Update scheduler: download failed: {}", e);
                            continue;
                        }
                        info!("Update scheduler: applying update");
                        if let Err(e) = apply_update(&data_dir).await {
                            debug!("Update scheduler: apply failed: {}", e);
                            continue;
                        }
                        info!(
                            "Update scheduler: update applied, restart scheduled by apply_update"
                        );
                        // apply_update has already spawned a 2s-delayed
                        // `systemctl restart archipelago`. Don't call
                        // std::process::exit here — that kills the runtime
                        // before the spawned restart task runs, and since
                        // the unit is Restart=on-failure a clean exit(0)
                        // leaves the service dead. Fall through; the
                        // scheduled restart will bring us back cleanly.
                        return;
                    }
                    Ok(_) => {
                        debug!("Update scheduler: no update available");
                    }
                    Err(e) => {
                        debug!("Update scheduler: check failed: {}", e);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest_value() -> serde_json::Value {
        serde_json::json!({
            "version": "9.9.9",
            "release_date": "2026-07-02",
            "changelog": ["test release"],
            "components": [{
                "name": "archipelago",
                "current_version": "0.0.0",
                "new_version": "9.9.9",
                "download_url": "http://example.invalid/archipelago",
                "sha256": "00",
                "size_bytes": 1
            }]
        })
    }

    /// Same key + env-pin convention as `trust::signed_doc::tests` — every
    /// caller pins the identical value, so parallel tests stay consistent.
    fn test_release_key() -> ed25519_dalek::SigningKey {
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        std::env::set_var(
            "ARCHY_RELEASE_ROOT_PUBKEY",
            hex::encode(key.verifying_key().to_bytes()),
        );
        key
    }

    fn sign_value(
        key: &ed25519_dalek::SigningKey,
        mut doc: serde_json::Value,
    ) -> serde_json::Value {
        let (sig, did) = crate::trust::signed_doc::sign_detached(key, &doc).unwrap();
        let obj = doc.as_object_mut().unwrap();
        obj.insert("signed_by".into(), serde_json::json!(did));
        obj.insert("signature".into(), serde_json::json!(sig));
        doc
    }

    #[test]
    fn unsigned_manifest_parses_but_reports_unsigned() {
        let (manifest, signed) = parse_and_verify_manifest(sample_manifest_value()).unwrap();
        assert_eq!(manifest.version, "9.9.9");
        assert!(!signed, "unsigned manifest must not report as signed");
    }

    #[test]
    fn anchor_signed_manifest_reports_signed() {
        let key = test_release_key();
        let doc = sign_value(&key, sample_manifest_value());
        let (manifest, signed) = parse_and_verify_manifest(doc).unwrap();
        assert_eq!(manifest.version, "9.9.9");
        assert!(signed);
    }

    #[test]
    fn tampered_signed_manifest_is_rejected() {
        let key = test_release_key();
        let mut doc = sign_value(&key, sample_manifest_value());
        // Version swap after signing — exactly what a malicious mirror would do.
        doc.as_object_mut()
            .unwrap()
            .insert("version".into(), serde_json::json!("99.0.0"));
        assert!(parse_and_verify_manifest(doc).is_err());
    }

    #[test]
    fn test_update_schedule_default_is_daily_check() {
        let schedule = UpdateSchedule::default();
        assert_eq!(schedule, UpdateSchedule::DailyCheck);
    }

    #[test]
    fn test_manifest_origin_parses_https() {
        assert_eq!(
            manifest_origin(
                "https://releases.example.com/lfg2025/archy/raw/branch/main/releases/manifest.json"
            ),
            Some("https://releases.example.com".to_string())
        );
    }

    #[test]
    fn test_manifest_origin_parses_http_with_port() {
        assert_eq!(
            manifest_origin(
                "http://23.182.128.160:3000/lfg2025/archy/raw/branch/main/releases/manifest.json"
            ),
            Some("http://23.182.128.160:3000".to_string())
        );
    }

    #[test]
    fn test_manifest_origin_rejects_garbage() {
        assert_eq!(manifest_origin("not a url"), None);
        assert_eq!(manifest_origin("ftp://releases.example.com/x"), None);
    }

    #[test]
    fn test_rewrite_manifest_origins_swaps_all_components() {
        let mut manifest = UpdateManifest {
            version: "1.7.26-alpha".into(),
            release_date: "2026-04-21".into(),
            changelog: vec![],
            components: vec![
                ComponentUpdate {
                    name: "archipelago".into(),
                    current_version: "1.7.25-alpha".into(),
                    new_version: "1.7.26-alpha".into(),
                    download_url: "https://releases.example.com/lfg2025/archy/raw/branch/main/releases/v1.7.26-alpha/archipelago".into(),
                    sha256: "x".into(),
                    size_bytes: 1,
                    blake3: None,
                },
                ComponentUpdate {
                    name: "frontend".into(),
                    current_version: "1.7.25-alpha".into(),
                    new_version: "1.7.26-alpha".into(),
                    download_url: "https://releases.example.com/lfg2025/archy/raw/branch/main/releases/v1.7.26-alpha/frontend.tar.gz".into(),
                    sha256: "y".into(),
                    size_bytes: 2,
                    blake3: None,
                },
            ],
        };
        rewrite_manifest_origins(
            &mut manifest,
            "http://23.182.128.160:3000/lfg2025/archy/raw/branch/main/releases/manifest.json",
        );
        assert_eq!(
            manifest.components[0].download_url,
            "http://23.182.128.160:3000/lfg2025/archy/raw/branch/main/releases/v1.7.26-alpha/archipelago"
        );
        assert_eq!(
            manifest.components[1].download_url,
            "http://23.182.128.160:3000/lfg2025/archy/raw/branch/main/releases/v1.7.26-alpha/frontend.tar.gz"
        );
    }

    #[tokio::test]
    async fn test_load_mirrors_returns_defaults_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let list = load_mirrors(dir.path()).await.unwrap();
        // The named https origin is the ONLY default since 2026-08-11:
        // bare-IP origins were retired from the update registry (the IP
        // twin bought DNS/TLS-broken recovery, deliberately given up).
        assert_eq!(list.len(), 1);
        assert!(
            list[0]
                .url
                .starts_with("https://source.archipelago-foundation.org/"),
            "the named origin must be primary, got {}",
            list[0].url
        );
        assert!(
            !list.iter().any(|m| m.url.contains("git.tx1138.com")),
            "tx1138 was retired as a release server and must not be a default mirror"
        );
        assert!(
            !list.iter().any(|m| m.url.contains("146.59.87.168")),
            "bare-IP origins were retired 2026-08-11 and must not be defaults"
        );
    }

    #[tokio::test]
    async fn test_load_mirrors_strips_retired_tx1138_mirror() {
        // A node that was running before tx1138 was retired has it baked
        // into its saved mirror list. load_mirrors must strip it on load.
        let dir = tempfile::tempdir().unwrap();
        let saved = vec![
            UpdateMirror {
                url: DEFAULT_UPDATE_MANIFEST_URL.to_string(),
                label: "Server 1 (OVH)".to_string(),
            },
            UpdateMirror {
                // Deliberately the retired host: this fixture exists to prove
                // load_mirrors strips it.
                url: "https://git.tx1138.com/lfg2025/archy/raw/branch/main/releases/manifest.json"
                    .to_string(),
                label: "Server 2 (tx1138)".to_string(),
            },
        ];
        save_mirrors(dir.path(), &saved).await.unwrap();
        let list = load_mirrors(dir.path()).await.unwrap();
        assert!(
            !list.iter().any(|m| m.url.contains("git.tx1138.com")),
            "retired tx1138 mirror should be stripped on load; got {:?}",
            list
        );
        assert!(
            !list.iter().any(|m| m.url.contains("146.59.87.168")),
            "the bare-IP twin is retired too and must be stripped on load; got {:?}",
            list
        );
    }

    #[tokio::test]
    async fn test_save_and_load_mirrors_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let list = vec![UpdateMirror {
            url: "https://example.com/m.json".into(),
            label: "Example".into(),
        }];
        save_mirrors(dir.path(), &list).await.unwrap();
        let back = load_mirrors(dir.path()).await.unwrap();
        // load_mirrors merges in any missing default mirrors so a node
        // that explicitly added a single custom mirror still gets the
        // built-in OVH default. The custom mirror is preserved.
        assert!(
            back.iter().any(|m| m.url == "https://example.com/m.json"),
            "custom mirror should round-trip; got {:?}",
            back
        );
        for def in default_mirrors() {
            assert!(
                back.iter().any(|m| m.url == def.url),
                "default mirror {} should be present after load; got {:?}",
                def.url,
                back
            );
        }
    }

    #[test]
    fn test_update_state_default_values() {
        let state = UpdateState::default();
        assert_eq!(state.current_version, env!("CARGO_PKG_VERSION"));
        assert!(state.last_check.is_none());
        assert!(state.available_update.is_none());
        assert!(!state.update_in_progress);
        assert!(!state.rollback_available);
        assert_eq!(state.schedule, UpdateSchedule::DailyCheck);
    }

    #[test]
    fn test_update_state_serialization_roundtrip() {
        let state = UpdateState {
            current_version: "0.2.0".to_string(),
            last_check: Some("2025-01-01T00:00:00Z".to_string()),
            available_update: None,
            update_in_progress: false,
            rollback_available: true,
            schedule: UpdateSchedule::AutoApply,
            manifest_mirror: None,
            manifest_signed: false,
        };
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: UpdateState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.current_version, "0.2.0");
        assert!(deserialized.rollback_available);
        assert_eq!(deserialized.schedule, UpdateSchedule::AutoApply);
    }

    #[test]
    fn test_update_schedule_serde_rename() {
        let json = serde_json::to_string(&UpdateSchedule::DailyCheck).unwrap();
        assert_eq!(json, "\"daily_check\"");
        let json = serde_json::to_string(&UpdateSchedule::Manual).unwrap();
        assert_eq!(json, "\"manual\"");
        let json = serde_json::to_string(&UpdateSchedule::AutoApply).unwrap();
        assert_eq!(json, "\"auto_apply\"");
    }

    #[test]
    fn test_update_state_schedule_defaults_on_missing_field() {
        // When schedule field is missing from JSON, it should default to DailyCheck
        let json = r#"{
            "current_version": "0.1.0",
            "last_check": null,
            "available_update": null,
            "update_in_progress": false,
            "rollback_available": false
        }"#;
        let state: UpdateState = serde_json::from_str(json).unwrap();
        assert_eq!(state.schedule, UpdateSchedule::DailyCheck);
    }

    #[test]
    fn test_parse_version_triple() {
        assert_eq!(parse_version_triple("1.7.18"), Some((1, 7, 18)));
        assert_eq!(parse_version_triple("1.7.18-alpha"), Some((1, 7, 18)));
        assert_eq!(parse_version_triple("0.0.1"), Some((0, 0, 1)));
        assert_eq!(parse_version_triple("garbage"), None);
        assert_eq!(parse_version_triple("1.2"), None);
    }

    #[test]
    fn test_is_newer() {
        assert!(is_newer("1.7.19-alpha", "1.7.18-alpha"));
        assert!(is_newer("1.8.0-alpha", "1.7.99-alpha"));
        assert!(is_newer("1.7.10-alpha", "1.7.9-alpha")); // numeric, not lexical
        assert!(!is_newer("1.7.18-alpha", "1.7.18-alpha"));
        assert!(!is_newer("1.7.17-alpha", "1.7.18-alpha")); // would-be downgrade
        assert!(!is_newer("1.7.9-alpha", "1.7.10-alpha"));
    }

    #[tokio::test]
    async fn test_load_state_clears_stale_available_on_version_bump() {
        // Simulates a sideload: state file on disk says we're on
        // 1.7.16-alpha with 1.7.17-alpha staged as the pending update,
        // but the running binary is 1.7.18-alpha (skipped a version).
        // load_state must drop the stale available_update so the UI
        // doesn't offer a downgrade.
        let dir = tempfile::tempdir().unwrap();
        let stale = UpdateState {
            current_version: "1.7.16-alpha".to_string(),
            available_update: Some(UpdateManifest {
                version: "1.7.17-alpha".to_string(),
                release_date: "2026-04-20".to_string(),
                changelog: vec![],
                components: vec![],
            }),
            ..UpdateState::default()
        };
        save_state(dir.path(), &stale).await.unwrap();
        let loaded = load_state(dir.path()).await.unwrap();
        assert_eq!(loaded.current_version, env!("CARGO_PKG_VERSION"));
        assert!(
            loaded.available_update.is_none(),
            "stale available_update must be cleared after version bump"
        );
    }

    #[tokio::test]
    async fn test_load_state_creates_default_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let state = load_state(dir.path()).await.unwrap();
        assert_eq!(state.current_version, env!("CARGO_PKG_VERSION"));
        assert!(!state.update_in_progress);
        // File should now exist after load created the default
        assert!(dir.path().join(UPDATE_STATE_FILE).exists());
    }

    #[tokio::test]
    async fn test_save_and_load_state_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("update-staging");
        tokio::fs::create_dir_all(&staging).await.unwrap();
        tokio::fs::write(staging.join("archipelago"), b"staged")
            .await
            .unwrap();
        // A *complete* staged update carries the .download-complete marker;
        // without it has_staged_update() reads the staging as partial and the
        // load_state self-heal clears update_in_progress (see #26). This test
        // simulates a complete staging, so write the marker.
        tokio::fs::write(staging.join(STAGED_COMPLETE_MARKER), b"1")
            .await
            .unwrap();
        let state = UpdateState {
            current_version: "1.0.0".to_string(),
            last_check: Some("2025-06-15T12:00:00Z".to_string()),
            available_update: Some(UpdateManifest {
                version: "1.1.0".to_string(),
                release_date: "2025-06-20".to_string(),
                changelog: vec!["Fix bugs".to_string(), "New feature".to_string()],
                components: vec![ComponentUpdate {
                    name: "archipelago".to_string(),
                    current_version: "1.0.0".to_string(),
                    new_version: "1.1.0".to_string(),
                    download_url: "https://example.com/binary".to_string(),
                    sha256: "abc123".to_string(),
                    size_bytes: 5000,
                    blake3: None,
                }],
            }),
            update_in_progress: true,
            rollback_available: false,
            schedule: UpdateSchedule::Manual,
            manifest_mirror: Some(
                "https://releases.example.com/lfg2025/archy/raw/branch/main/releases/manifest.json"
                    .to_string(),
            ),
            manifest_signed: false,
        };
        save_state(dir.path(), &state).await.unwrap();
        let loaded = load_state(dir.path()).await.unwrap();
        // load_state rewrites current_version to match the running
        // binary (sideload self-heal), so don't assert on the saved
        // value. The migration also clears available_update when the
        // version changes — check the other fields survived.
        assert_eq!(loaded.current_version, env!("CARGO_PKG_VERSION"));
        assert!(loaded.update_in_progress);
        assert_eq!(loaded.schedule, UpdateSchedule::Manual);
        assert!(loaded.available_update.is_none());
    }

    #[tokio::test]
    async fn test_load_state_clears_stale_in_progress_without_staging() {
        let dir = tempfile::tempdir().unwrap();
        let state = UpdateState {
            update_in_progress: true,
            ..UpdateState::default()
        };
        save_state(dir.path(), &state).await.unwrap();

        let loaded = load_state(dir.path()).await.unwrap();

        assert!(!loaded.update_in_progress);
        let persisted = load_state(dir.path()).await.unwrap();
        assert!(!persisted.update_in_progress);
    }

    /// apply_update takes the global single-flight UPDATE_OP_LOCK, so tests
    /// that call it must not run concurrently — one would see the other's
    /// lock and fail with "another update operation is already running".
    static APPLY_TEST_SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[tokio::test]
    async fn test_apply_refuses_unmarked_staging() {
        let _serial = APPLY_TEST_SERIAL.lock().await;
        // Regression: .198 v1.7.103 bricking — apply ran against a staging
        // dir that a concurrent download was still filling. Without the
        // .download-complete marker, apply must refuse before touching
        // anything.
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("update-staging");
        tokio::fs::create_dir_all(&staging).await.unwrap();
        tokio::fs::write(staging.join("archipelago"), b"partial")
            .await
            .unwrap();
        let err = apply_update(dir.path()).await.unwrap_err();
        assert!(
            err.to_string().contains("completion marker"),
            "got: {err:#}"
        );
    }

    #[tokio::test]
    async fn test_apply_refuses_staged_bytes_that_mismatch_manifest() {
        let _serial = APPLY_TEST_SERIAL.lock().await;
        // Marker present (a complete download once existed) but the staged
        // bytes no longer match the manifest — apply must re-verify and
        // refuse rather than install whatever is on disk.
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("update-staging");
        tokio::fs::create_dir_all(&staging).await.unwrap();
        tokio::fs::write(staging.join(STAGED_COMPLETE_MARKER), b"1")
            .await
            .unwrap();
        tokio::fs::write(staging.join("archipelago"), b"truncated-garbage")
            .await
            .unwrap();
        let state = UpdateState {
            available_update: Some(UpdateManifest {
                version: "999.0.0".to_string(),
                release_date: "2026-07-18".to_string(),
                changelog: vec![],
                components: vec![ComponentUpdate {
                    name: "archipelago".to_string(),
                    current_version: "1.0.0".to_string(),
                    new_version: "999.0.0".to_string(),
                    download_url: "http://example.invalid/archipelago".to_string(),
                    sha256: "0".repeat(64),
                    size_bytes: 49_949_048,
                    blake3: None,
                }],
            }),
            update_in_progress: true,
            ..UpdateState::default()
        };
        save_state(dir.path(), &state).await.unwrap();
        let err = apply_update(dir.path()).await.unwrap_err();
        assert!(
            err.to_string().contains("re-download before applying"),
            "got: {err:#}"
        );
        // Resilience: a refused apply must leave the update still available and
        // still staged, so the user can re-download and retry — never a wedge.
        let loaded = load_state(dir.path()).await.unwrap();
        assert!(
            loaded.available_update.is_some(),
            "a refused apply must not clear the available update"
        );
        assert!(
            loaded.update_in_progress,
            "a refused apply must leave the staged-update flag set for retry"
        );
    }

    #[tokio::test]
    async fn test_dismiss_update_clears_available() {
        let dir = tempfile::tempdir().unwrap();
        let state = UpdateState {
            available_update: Some(UpdateManifest {
                version: "2.0.0".to_string(),
                release_date: "2025-07-01".to_string(),
                changelog: vec![],
                components: vec![],
            }),
            ..UpdateState::default()
        };
        save_state(dir.path(), &state).await.unwrap();
        dismiss_update(dir.path()).await.unwrap();
        let loaded = load_state(dir.path()).await.unwrap();
        assert!(loaded.available_update.is_none());
    }

    #[tokio::test]
    async fn test_set_and_get_schedule() {
        let dir = tempfile::tempdir().unwrap();
        // Initialize state
        let _ = load_state(dir.path()).await.unwrap();

        set_schedule(dir.path(), UpdateSchedule::AutoApply)
            .await
            .unwrap();
        let schedule = get_schedule(dir.path()).await.unwrap();
        assert_eq!(schedule, UpdateSchedule::AutoApply);

        set_schedule(dir.path(), UpdateSchedule::Manual)
            .await
            .unwrap();
        let schedule = get_schedule(dir.path()).await.unwrap();
        assert_eq!(schedule, UpdateSchedule::Manual);
    }

    #[tokio::test]
    async fn test_get_status_returns_current_state() {
        let dir = tempfile::tempdir().unwrap();
        let state = UpdateState {
            current_version: "3.0.0".to_string(),
            rollback_available: true,
            ..UpdateState::default()
        };
        save_state(dir.path(), &state).await.unwrap();
        let status = get_status(dir.path()).await.unwrap();
        // get_status → load_state, which rewrites current_version to
        // match the running binary (see the sideload-self-heal path).
        assert_eq!(status.current_version, env!("CARGO_PKG_VERSION"));
        assert!(status.rollback_available);
    }

    #[tokio::test]
    async fn test_pending_verification_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let marker = PendingVerification {
            applied_at: chrono::Utc::now().to_rfc3339(),
            new_version: "1.7.41-alpha".into(),
            previous_version: "1.7.40-alpha".into(),
            deadline_ts: chrono::Utc::now().timestamp() + 150,
        };
        write_pending_verification(dir.path(), &marker)
            .await
            .unwrap();
        let read = read_pending_verification(dir.path()).await.unwrap();
        assert_eq!(read.new_version, "1.7.41-alpha");
        assert_eq!(read.previous_version, "1.7.40-alpha");
        clear_pending_verification(dir.path()).await;
        assert!(read_pending_verification(dir.path()).await.is_none());
    }

    #[tokio::test]
    async fn test_pending_verification_absent_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_pending_verification(dir.path()).await.is_none());
    }

    #[tokio::test]
    async fn test_verify_pending_update_noop_without_marker() {
        let dir = tempfile::tempdir().unwrap();
        // No marker written -- must return quickly without doing anything
        // risky (network probes, rollback calls). We're just asserting
        // it doesn't panic or hang.
        verify_pending_update(dir.path()).await;
    }

    #[test]
    fn test_pending_verify_constants_are_sensible() {
        // Window must be generous enough for nginx + backend startup,
        // but less than the stale-marker threshold so a normal cycle
        // can complete without the marker being considered stale.
        assert!(PENDING_VERIFY_WINDOW_SECS < PENDING_VERIFY_MAX_AGE_SECS as u64);
        assert!(PENDING_VERIFY_WINDOW_SECS >= 60);
    }
}
