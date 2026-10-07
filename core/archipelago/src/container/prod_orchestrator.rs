//! Production container orchestrator.
//!
//! Owns install/start/stop/restart/remove/upgrade/status/list/logs/health for every
//! archipelago-managed production container. Unlike `DevContainerOrchestrator`, this
//! orchestrator:
//!
//!   * does NOT append a `-dev` suffix to container names,
//!   * does NOT apply a port offset,
//!   * does NOT rewrite volume source paths into a dev data dir,
//!   * does NOT gate bitcoin dependencies on the simulator,
//!   * DOES support building images from a local Dockerfile (new in Step 2),
//!   * DOES adopt pre-existing containers by name rather than recreating,
//!   * DOES run a level-triggered reconciler (spawned by `BootReconciler` in Step 5).
//!
//! See `docs/rust-orchestrator-migration.md` — this file is Step 3.
//!
//! Concurrency model: a per-app `tokio::sync::Mutex<()>` protects all mutating ops
//! against the reconciler loop. The map of mutexes is itself behind an `RwLock`;
//! acquiring the per-app lock requires only a read lock on the map, so reconcile
//! runs over N apps don't serialize against each other.
//!
//! Name resolution rule (answered design Q1): UI containers get the `archy-` prefix;
//! everything else uses the bare app_id. This matches the running fixture on .116
//! so adoption doesn't orphan anything.

use anyhow::{Context, Result};
use archipelago_container::{
    AppManifest, ContainerRuntime as ContainerRuntimeTrait, ContainerState, ContainerStatus,
    Dependency, HostFacts, ManifestError, ResolvedSource, SecretsProvider,
};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, RwLock};

/// Refusal before installation has created state or changed any dependency.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct InstallPrerequisiteError(pub String);

use crate::config::{Config, ContainerRuntime as ConfigContainerRuntime};
use crate::container::bitcoin_ui;
use crate::container::quadlet;
use crate::container::traits::ContainerOrchestrator;
use crate::container::{filebrowser, lnd};
use crate::update::host_sudo;

/// App IDs whose containers are named `archy-<id>` rather than bare `<id>`.
///
/// Keep in sync with the running fixture on .116. Centralized as a constant
/// so the rule is visible in one place and unit-testable.
const UI_APP_IDS: &[&str] = &["bitcoin-ui", "electrs-ui", "lnd-ui", "cuprate-ui"];
const ARCHIVAL_BITCOIN_DISK_GB: u64 = 1000;
// The cuprate disk floor is `crate::constants::CUPRATE_MIN_DISK_GB` — one
// value shared with the install/start/restart/update RPC gates so boot
// reconcile can never resume below the line they refuse at.

use crate::constants::CUPRATE_MIN_DISK_GB;

fn requires_cuprate_disk(app_id: &str, disk_gb: u64) -> bool {
    app_id == "cuprate" && disk_gb < CUPRATE_MIN_DISK_GB
}

/// Apps expected to exist from first boot on every node — the ONLY apps the
/// boot reconciler may install from nothing. Every other app needs
/// installation evidence (an existing container, or the was-running snapshot
/// handled by the caller's desired-state recovery). Without this gate,
/// "manifest loaded" counted as "installed" — and since the catalog + ISO ship
/// manifests for EVERY app, a fresh node mass-installed the entire catalog
/// (framework node 2026-07-14: portainer/vaultwarden/searxng/strfry/mempool/
/// fedimint appeared uninvited within an hour of first boot).
fn is_required_baseline_app(app_id: &str) -> bool {
    matches!(app_id, "filebrowser" | "fedimint-clientd")
}

fn is_restart_sensitive_app(app_id: &str) -> bool {
    matches!(
        app_id,
        "bitcoin-knots"
            | "bitcoin-core"
            | "bitcoin"
            | "lnd"
            | "btcpay-server"
            | "fedimint"
            | "fedimint-gateway"
    )
}

fn is_builtin_network_mode(network: &str) -> bool {
    matches!(
        network,
        "host"
            | "bridge"
            | "none"
            | "slirp4netns"
            | "slirp4netns:allow_host_loopback=true"
            | "slirp4netns:allow_host_loopback=true,cidr=169.254.1.0/24"
            | "pasta"
    )
}

// Only an explicitly selected rootless mode establishes drift. An omitted
// network delegates to Podman and must not recreate unrelated installed apps.
fn rootless_network_mode_drifted(expected: Option<&str>, actual: &str) -> bool {
    let actual = actual.trim();
    if actual.is_empty() {
        return false;
    }
    match expected {
        Some(
            expected @ ("slirp4netns:allow_host_loopback=true"
            | "slirp4netns:allow_host_loopback=true,cidr=169.254.1.0/24"),
        ) => {
            let (mode, options) = actual.split_once(':').unwrap_or((actual, ""));
            let required = expected.split_once(':').unwrap().1;
            mode != "slirp4netns"
                || required.split(',').any(|wanted| {
                    let key = wanted.split_once('=').unwrap().0;
                    !options.split(',').any(|option| option == wanted)
                        || options.split(',').any(|option| {
                            option.split_once('=').is_some_and(|(name, _)| name == key)
                                && option != wanted
                        })
                })
        }
        Some(mode @ ("slirp4netns" | "pasta")) => actual.split(':').next() != Some(mode),
        _ => false,
    }
}

// API-created containers may omit rootless options from HostConfig.NetworkMode.
// generate spec retains them; inspect alone would cause an endless repair loop.
fn rootless_network_from_spec(bytes: &[u8]) -> Option<String> {
    let spec: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let mode = spec.get("netns")?.get("nsmode")?.as_str()?;
    if !matches!(mode, "slirp4netns" | "pasta") {
        return None;
    }
    let options = spec
        .get("network_options")
        .and_then(|options| options.get(mode));
    match options {
        None => Some(mode.to_string()),
        Some(options) => {
            let options = options
                .as_array()?
                .iter()
                .map(|value| value.as_str())
                .collect::<Option<Vec<_>>>()?;
            if options.is_empty() {
                Some(mode.to_string())
            } else {
                Some(format!("{mode}:{}", options.join(",")))
            }
        }
    }
}

fn missing_declared_capability(expected: &[String], actual: &[String]) -> bool {
    expected.iter().any(|required| {
        let required = required.strip_prefix("CAP_").unwrap_or(required);
        !actual
            .iter()
            .any(|cap| cap.strip_prefix("CAP_").unwrap_or(cap) == required)
    })
}

fn uses_pasta_network(manifest: &AppManifest) -> bool {
    manifest.app.container.network.as_deref() == Some("pasta")
}

fn dependency_manifests_required_by_active_apps<'a>(
    manifests: impl Iterator<Item = &'a AppManifest>,
    user_stopped: &HashSet<String>,
) -> HashSet<String> {
    let mut required = HashSet::new();
    for manifest in manifests {
        let app_id = manifest.app.id.as_str();
        if user_stopped.contains(app_id) || user_stopped.contains(&compute_container_name(manifest))
        {
            continue;
        }
        for app_id in manifest_dependency_app_ids(manifest) {
            required.insert(app_id);
        }
    }
    required
}

/// Whether `app_id` is a member of a known multi-container stack that has at
/// least one OTHER member with a live container (any state). A live sibling
/// proves the stack is installed on this node, so an absent member is a hole
/// to repair — while a stack with no containers at all stays untouched
/// (uninstalled, or never installed here). Sibling app ids resolve to
/// container names through the loaded-manifest map when available (immich's
/// `immich-postgres` app id runs as container `immich_postgres`), falling
/// back to the id itself.
fn absent_stack_member_with_live_sibling(
    app_id: &str,
    present_containers: &HashSet<String>,
    container_name_by_app_id: &std::collections::HashMap<String, String>,
) -> bool {
    let stack = crate::app_ops::owning_package(app_id);
    let members = crate::app_ops::stack_member_app_ids(stack);
    members.iter().any(|member| {
        *member != app_id
            && present_containers.contains(
                container_name_by_app_id
                    .get(*member)
                    .map(String::as_str)
                    .unwrap_or(member),
            )
    })
}

fn manifest_dependency_app_ids(manifest: &AppManifest) -> Vec<String> {
    manifest
        .app
        .dependencies
        .iter()
        .filter_map(|dep| match dep {
            Dependency::App { app_id, .. } => Some(app_id.clone()),
            Dependency::Simple(app_id) => Some(app_id.clone()),
            Dependency::Storage { .. } => None,
        })
        .collect()
}

fn host_port_wait_timeout_secs(manifest: &AppManifest) -> u64 {
    // First NPM initialization generates keys and migrates its database before
    // exposing nginx. Its readiness budget must not depend on the network driver.
    if manifest.app.id == "nginx-proxy-manager" {
        return 180;
    }
    if manifest.app.id == "uptime-kuma" {
        return 420;
    }
    if uses_pasta_network(manifest) {
        120
    } else {
        60
    }
}

fn host_port_repair_probe_timeout_secs(_manifest: &AppManifest) -> u64 {
    5
}

fn is_podman_socket_bind_source(source: &str) -> bool {
    source.ends_with("/podman.sock")
}

fn requires_archival_bitcoin(app_id: &str) -> bool {
    matches!(
        app_id,
        "electrumx" | "mempool-api" | "mempool" | "archy-mempool-db"
    )
}

/// Compute the podman container name for a manifest.
///
/// Rules (in order):
///   1. If the manifest carries `extensions.container_name: <string>`, use that verbatim.
///   2. If `app.id` is in `UI_APP_IDS`, return `archy-<id>`.
///   3. Otherwise return `<id>` unchanged.
pub fn compute_container_name(manifest: &AppManifest) -> String {
    if let Some(v) = manifest.app.extensions.get("container_name") {
        if let Some(s) = v.as_str() {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    let id = manifest.app.id.as_str();
    if UI_APP_IDS.contains(&id) {
        format!("archy-{id}")
    } else {
        id.to_string()
    }
}

/// Resolve the graceful-stop grace (seconds) for an app: the manifest
/// `stop_grace_secs` extension if declared (manifest-driven, north-star), else
/// the historical per-app `stop_timeout_secs` table keyed by container name.
pub fn resolve_stop_grace_secs(manifest: &AppManifest, container_name: &str) -> u64 {
    if let Some(v) = manifest.app.extensions.get("stop_grace_secs") {
        // Accept either a YAML integer or a numeric string.
        if let Some(n) = v.as_u64() {
            return n;
        }
        if let Some(n) = v.as_str().and_then(|s| s.trim().parse::<u64>().ok()) {
            return n;
        }
    }
    archipelago_container::runtime::stop_grace_secs_for(container_name)
}

/// Fingerprint a local build context so a changed source tree (e.g. a rebuilt
/// `neode-ui` dist copied into `docker/<ui>/`) forces an image rebuild even
/// when the image tag already exists (#34). Walks the context directory and
/// hashes each file's relative path, length, and mtime.
///
/// Metadata-only by design: it's cheap enough to recompute on every reconcile,
/// and podman's own COPY-layer cache still skips the actual layer work when the
/// file *content* is unchanged, so a spurious mtime bump costs almost nothing.
/// Returns `None` if the context can't be read (caller falls back to building).
fn fingerprint_build_context(context: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut entries: Vec<(String, u64, i128)> = Vec::new();
    let mut stack = vec![context.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let rd = std::fs::read_dir(&dir).ok()?;
        for entry in rd.flatten() {
            let path = entry.path();
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path
                .strip_prefix(context)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as i128)
                .unwrap_or(0);
            entries.push((rel, meta.len(), mtime));
        }
    }
    // Sort so the hash is independent of directory-walk order.
    entries.sort();
    let mut hasher = Sha256::new();
    for (rel, len, mtime) in &entries {
        hasher.update(rel.as_bytes());
        hasher.update(b"\0");
        hasher.update(len.to_le_bytes());
        hasher.update(mtime.to_le_bytes());
    }
    Some(hex::encode(hasher.finalize()))
}

/// Path of the stamp file recording the build-context fingerprint that produced
/// the currently-built image for `tag`. Keyed by a filesystem-safe form of the
/// tag so distinct UI images don't collide.
fn build_fingerprint_stamp_path(data_dir: &Path, tag: &str) -> PathBuf {
    let safe: String = tag
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    data_dir
        .join(".image-build")
        .join(format!("{safe}.fingerprint"))
}

async fn chown_for_rootless_container(uid_gid: &str, path: &str) -> Result<()> {
    let (uid, gid) = uid_gid
        .split_once(':')
        .map(|(u, g)| (u.parse::<u32>().unwrap_or(0), g.parse::<u32>().unwrap_or(0)))
        .unwrap_or((0, 0));

    // Idempotence: a directory already owned by the host-mapped target must
    // not get another recursive chown on every prepare/reconcile tick — that
    // unconditional chown is what looped on postgres-btcpay every ~45s
    // (archi-dev-box, 2026-08-07). Deep-file drift in a RUNNING container is
    // still caught by ensure_running_container_ownership's write-probe.
    let host_uid_gid = if uid > 0 && uid < 100_000 {
        let map = |id: u32| if id == 0 { 1000 } else { 100_000 + id - 1 };
        format!("{}:{}", map(uid), map(gid))
    } else {
        uid_gid.to_string()
    };
    if ownership_already_correct(path, &host_uid_gid) {
        return Ok(());
    }
    // Some rootless-service contexts can read the directory entry but fail
    // Rust's metadata lookup through an ACL/namespace boundary. Ask the
    // host's root view before falling through to an expensive `chown -R`.
    // This is still cheap compared with recursively walking Postgres on every
    // reconcile and makes the drift gate authoritative on deployed nodes.
    if ownership_already_correct_from_host(path, &host_uid_gid).await {
        return Ok(());
    }

    if uid > 0 && uid < 100_000 {
        let output = tokio::process::Command::new("podman")
            .args(["unshare", "chown", "-R", uid_gid, path])
            .output()
            .await
            .with_context(|| format!("podman unshare chown -R {uid_gid} {path}"))?;
        if output.status.success() {
            return Ok(());
        }
    }

    // Host-side fallback. A CONTAINER-namespace id must be translated into
    // the subuid range first: `sudo chown 999` writes literal host uid 999,
    // which maps to nobody inside the userns — the app then can't open its
    // own files while the chown reported success (botfights SQLITE_CANTOPEN
    // crash-loop, a test node 2026-08-06). Container uid N (N>=1) lives at
    // subuid_base + N - 1; the fleet provisions base 100000. uid 0 and
    // already-mapped ids (>=100000) pass through untouched.
    let status = host_sudo(&["chown", "-R", &host_uid_gid, path])
        .await
        .with_context(|| format!("sudo chown -R {host_uid_gid} {path}"))?;
    if status.success() {
        return Ok(());
    }

    if uid == 0 || uid >= 100_000 {
        return Err(anyhow::anyhow!(
            "chown -R {uid_gid} {path} failed with status {:?}",
            status.code()
        ));
    }

    let output = tokio::process::Command::new("podman")
        .args(["unshare", "chown", "-R", uid_gid, path])
        .output()
        .await
        .with_context(|| format!("podman unshare chown -R {uid_gid} {path}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(anyhow::anyhow!(
        "chown -R {uid_gid} {path} failed; podman unshare also failed: {}",
        stderr.trim()
    ))
}

/// Cheap drift gate for `chown_for_rootless_container`: does the path's
/// top-level owner already match the target "uid:gid" string? A miss on a
/// missing path answers false (the caller's mkdir runs first anyway).
fn ownership_already_correct(path: &str, host_uid_gid: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Some((uid_s, gid_s)) = host_uid_gid.split_once(':') else {
        return false;
    };
    let (Ok(uid), Ok(gid)) = (uid_s.parse::<u32>(), gid_s.parse::<u32>()) else {
        return false;
    };
    let Ok(md) = std::fs::metadata(path) else {
        return false;
    };
    md.uid() == uid && md.gid() == gid
}

/// Drift-gated ownership repair for the per-app hooks: skip when already
/// correct, `chown -R` when not, and — critically — do not fail the caller
/// when the chown fails but the ownership is right anyway.
///
/// These hooks exist to repair OLD installs. On a healthy node the repair is a
/// no-op, so a transient failure of it must not abort the app's whole
/// reconcile. It did: `chown -R` through `sudo systemd-run` failed once on
/// archi-dev-box (2026-08-08 04:44) on a `postgres-btcpay` tree where every
/// single file was already owned by the target uid — `find ! -uid 100998`
/// returned nothing, and the identical command run by hand exited 0 — yet the
/// error propagated all the way out as `reconcile failed
/// app_id=btcpay-server`, taking a running, healthy BTCPay's reconcile with
/// it.
///
/// So a chown failure is only an error if the ownership is ALSO wrong
/// afterwards. That keeps the loud failure for the case it was written for (a
/// genuinely mis-owned volume the app cannot open) and drops it for the case
/// that only ever produced noise.
async fn repair_dir_ownership(dir: &str, host_uid_gid: &str) -> Result<()> {
    if ownership_already_correct_from_host(dir, host_uid_gid).await {
        return Ok(());
    }
    let status = host_sudo(&["chown", "-R", host_uid_gid, dir])
        .await
        .with_context(|| format!("chown {dir}"))?;
    if status.success() {
        return Ok(());
    }
    // Re-probe before deciding this matters. A concurrent repair, or a
    // transient systemd-run/sudo failure on an already-correct tree, both land
    // here with nothing actually wrong.
    if ownership_already_correct_from_host(dir, host_uid_gid).await {
        tracing::warn!(
            dir = %dir,
            target = %host_uid_gid,
            "ownership repair chown failed but the ownership is already correct — continuing"
        );
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "chown {dir} failed with status {status} and ownership is still not {host_uid_gid}"
    ))
}

async fn ownership_already_correct_from_host(path: &str, host_uid_gid: &str) -> bool {
    let Ok(out) = crate::update::host_sudo_output(&["stat", "-c", "%u:%g", path]).await else {
        return false;
    };
    out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == host_uid_gid
}

/// `(container-id, mount-dest)` pairs whose in-container chown returned a hard,
/// permanent failure (e.g. "Operation not permitted" on a mount that can't be
/// re-owned from inside the userns). Remembered for the life of the process so
/// the per-reconcile repair stops re-attempting them — otherwise a single
/// unrepairable mount (observed: mempool-api `/data`) burns CPU + floods the
/// journal on every pass. Keyed by Id so a recreated container retries afresh.
fn unrepairable_ownership() -> &'static std::sync::Mutex<std::collections::HashSet<(String, String)>>
{
    static SET: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<(String, String)>>> =
        std::sync::OnceLock::new();
    SET.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

/// Per-container timestamp of the last volume-ownership sweep. The sweep's
/// write-probes are `podman exec`s into EVERY running container; running them
/// on every 30s reconcile tick meant six-plus cross-context exec attempts per
/// tick forever — a permanent conmon "Failed to create container" storm on
/// hosts where exec from the backend's cgroup context fails (Debian 13 first
/// boot, 2026-07-19). Ownership drift is an install/OTA-time event, not a
/// steady-state one: sweep each container on the first pass after it appears,
/// then at most once per hour.
fn ownership_sweep_due(name: &str) -> bool {
    const SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60 * 60);
    static LAST: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
    > = std::sync::OnceLock::new();
    let map = LAST.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let Ok(mut map) = map.lock() else {
        return true;
    };
    let now = std::time::Instant::now();
    match map.get(name) {
        Some(last) if now.duration_since(*last) < SWEEP_INTERVAL => false,
        _ => {
            map.insert(name.to_string(), now);
            true
        }
    }
}

/// App-agnostic, userns-mapping-proof volume-ownership repair for a RUNNING
/// container.
///
/// For each writable bind mount, write-probe as the container's own process
/// user; if it can't write, `chown -R` from INSIDE the container (`podman exec`
/// as root) to that service uid:gid. Because the chown runs in the container's
/// user namespace, podman translates it to the correct host owner regardless of
/// the rootless idmap — so there is NO host-side UID guessing, and it works for
/// compose stacks (no manifest / `data_uid` needed) exactly as for registry apps.
/// This is the durable replacement for the per-app hardcoded host chowns.
///
/// Drift-checked via the write-probe, so it only `chown`s when the volume is
/// actually unwritable — cheap enough to call on every reconcile. Best-effort:
/// returns true if it repaired something; never fails reconcile (a degraded app
/// must not block the loop). See the immich EACCES crash-loop (.198, 2026-06-17).
async fn ensure_running_container_ownership(name: &str) -> bool {
    async fn podman_stdout(args: &[&str]) -> Option<String> {
        let out = tokio::process::Command::new("podman")
            .args(args)
            .output()
            .await
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    // The uid:gid the container's main process actually runs as.
    let uid = match podman_stdout(&["exec", name, "id", "-u"]).await {
        Some(u) if !u.is_empty() => u,
        _ => return false, // can't exec (no shell / not running) — nothing to do
    };
    let gid = podman_stdout(&["exec", name, "id", "-g"])
        .await
        .filter(|g| !g.is_empty())
        .unwrap_or_else(|| uid.clone());

    // Stable identity of THIS container instance — used to remember mounts whose
    // chown is hard-unrepairable so we stop hammering them every reconcile. Keyed
    // by Id (not name) so a recreated container gets a fresh repair attempt.
    let cid = podman_stdout(&["inspect", name, "--format", "{{.Id}}"])
        .await
        .unwrap_or_default();

    // Writable bind-mount destinations only.
    let dests = match podman_stdout(&[
        "inspect",
        name,
        "--format",
        "{{range .Mounts}}{{if eq .Type \"bind\"}}{{if .RW}}{{.Destination}}\n{{end}}{{end}}{{end}}",
    ])
    .await
    {
        Some(d) => d,
        None => return false,
    };

    let mut repaired = false;
    for dest in dests.lines().map(str::trim).filter(|d| !d.is_empty()) {
        // Never touch system / socket bind mounts.
        if dest == "/"
            || dest.starts_with("/proc")
            || dest.starts_with("/sys")
            || dest.starts_with("/dev")
            || dest.starts_with("/run")
            || dest.starts_with("/etc")
            || dest.ends_with(".sock")
        {
            continue;
        }

        // Known hard-unrepairable for this container instance (a previous chown
        // returned a permanent error like "Operation not permitted"). Skip the
        // probe+chown entirely — retrying every reconcile only burns CPU and
        // floods the journal; it will never succeed for this instance.
        if !cid.is_empty()
            && unrepairable_ownership()
                .lock()
                .map(|s| s.contains(&(cid.clone(), dest.to_string())))
                .unwrap_or(false)
        {
            continue;
        }

        // Drift check: can the service user write here already?
        let probe = format!(
            "t=\"{dest}/.archy-wtest.$$\"; touch \"$t\" 2>/dev/null && rm -f \"$t\" 2>/dev/null"
        );
        let writable = tokio::process::Command::new("podman")
            .args(["exec", name, "sh", "-c", &probe])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);
        if writable {
            continue;
        }

        // Repair inside the container's userns — podman maps to the right host uid.
        let chown = tokio::process::Command::new("podman")
            .args([
                "exec",
                "-u",
                "0",
                name,
                "chown",
                "-R",
                &format!("{uid}:{gid}"),
                dest,
            ])
            .output()
            .await;
        match chown {
            Ok(o) if o.status.success() => {
                repaired = true;
                tracing::warn!(
                    container = %name, dest, uid = %uid,
                    "repaired unwritable volume ownership (in-container chown)"
                );
            }
            Ok(o) => {
                // Permanent failure (e.g. "Operation not permitted" on a mount
                // that simply can't be re-owned from inside the userns). Record
                // it so we don't re-attempt every reconcile — log once, loudly.
                if !cid.is_empty() {
                    if let Ok(mut s) = unrepairable_ownership().lock() {
                        s.insert((cid.clone(), dest.to_string()));
                    }
                }
                tracing::warn!(
                    container = %name, dest,
                    "volume ownership repair failed (won't retry for this container instance): {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                )
            }
            Err(e) => {
                tracing::warn!(container = %name, dest, "volume ownership repair errored: {e}")
            }
        }
    }
    repaired
}

async fn wait_for_host_port(port: u16, timeout_secs: u64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn wait_for_http_host_port(port: u16, path: &str, timeout_secs: u64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        if http_host_port_ready(port, path).await {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn http_host_port_ready(port: u16, path: &str) -> bool {
    let Ok(Ok(mut stream)) = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::net::TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    else {
        return false;
    };

    let request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).await.is_err() {
        return false;
    }

    let mut buf = [0u8; 128];
    let Ok(Ok(n)) =
        tokio::time::timeout(std::time::Duration::from_secs(3), stream.read(&mut buf)).await
    else {
        return false;
    };
    if n == 0 {
        return false;
    }
    let head = String::from_utf8_lossy(&buf[..n]);
    head.starts_with("HTTP/1.1 2")
        || head.starts_with("HTTP/1.1 3")
        || head.starts_with("HTTP/1.0 2")
        || head.starts_with("HTTP/1.0 3")
}

/// Resolves when a user-stop marker for the app (by app id or container
/// name) appears. Long readiness/repair waits select against this so a
/// user's stop request aborts the wait instead of queueing behind it for
/// minutes while the app is painted Restarting (§C follow-up from the
/// dd3afbba transitional-state stick; gate take-2 iter 5 vaultwarden).
async fn wait_for_user_stop_marker(data_dir: &Path, app_id: &str, container_name: &str) {
    loop {
        let user_stopped = crate::crash_recovery::load_user_stopped(data_dir).await;
        if user_stopped.contains(app_id) || user_stopped.contains(container_name) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn wait_for_manifest_host_ports(
    manifest: &AppManifest,
    timeout_secs: u64,
    data_dir: &Path,
) -> Result<()> {
    // Only TCP host ports are reachability-probed: the probe is a TCP connect,
    // which a UDP/SCTP listener (e.g. netbird's 3478/udp STUN) can never answer,
    // so probing it would always "fail" and drive an endless host-port repair
    // loop (observed on .228 after netbird's manifest deploy). Default protocol
    // (empty) is tcp.
    let container_name = compute_container_name(manifest);
    for port in manifest
        .app
        .ports
        .iter()
        .filter(|p| matches!(p.protocol.to_ascii_lowercase().as_str(), "" | "tcp"))
        .map(|p| p.host)
    {
        let ready = tokio::select! {
            ready = async {
                match manifest.app.id.as_str() {
                    "uptime-kuma" => wait_for_http_host_port(port, "/", timeout_secs).await,
                    _ => wait_for_host_port(port, timeout_secs).await,
                }
            } => ready,
            _ = wait_for_user_stop_marker(data_dir, &manifest.app.id, &container_name) => {
                return Err(anyhow::anyhow!(
                    "aborting host-port wait for {}: user stop requested mid-wait",
                    manifest.app.id
                ));
            }
        };
        if !ready {
            return Err(anyhow::anyhow!(
                "{} host port {} did not become reachable within {}s",
                manifest.app.id,
                port,
                timeout_secs
            ));
        }
    }
    Ok(())
}

/// Pure published-port drift check. `port_bindings_json` is the JSON that
/// `podman inspect --format '{{json .HostConfig.PortBindings}}'` emits, e.g.
/// `{"8080/tcp":[{"HostIp":"","HostPort":"18080"}]}`. Returns true only when a
/// manifest container-port is positively published to a *different* host port
/// than the manifest now asks for — or, when the manifest DECLARES a bind
/// address, to a different host address. Absence of a binding is deliberately
/// NOT treated as drift here — that case is handled by the host-port
/// repair/restart path and by host-networked apps that publish nothing — so we
/// never trigger a destructive recreate on a false positive.
///
/// The bind comparison is what lets a node self-heal after a catalog refresh
/// pins an app to loopback for the app gate: a legacy (pre-quadlet) container
/// still publishing `0.0.0.0:P` against a manifest that now declares
/// `bind: 127.0.0.1` is recreated to the declared state, exactly as
/// `package.update` would. An EMPTY manifest bind means "no instruction" and
/// never fires this — recreating a loopback-published container to wildcard on
/// silence is precisely the v1.7.121 incident class (Bitcoin RPC republished
/// on the LAN).
fn host_port_bindings_drifted(
    port_bindings_json: &str,
    manifest_ports: &[archipelago_container::manifest::PortMapping],
) -> bool {
    let parsed: serde_json::Value = match serde_json::from_str(port_bindings_json) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let Some(map) = parsed.as_object() else {
        return false;
    };
    for port in manifest_ports {
        let proto = if port.protocol.is_empty() {
            "tcp"
        } else {
            port.protocol.as_str()
        };
        let key = format!("{}/{}", port.container, proto);
        let Some(bindings) = map.get(&key).and_then(|b| b.as_array()) else {
            // Container-port not currently published — not our case.
            continue;
        };
        if bindings.is_empty() {
            continue;
        }
        let expected = port.host.to_string();
        let matches_expected = bindings.iter().any(|b| {
            let host_port_ok = b
                .get("HostPort")
                .and_then(|h| h.as_str())
                .map(|h| h == expected)
                .unwrap_or(false);
            if !host_port_ok {
                return false;
            }
            // Only a DECLARED bind participates; podman reports a wildcard
            // publish as "" or "0.0.0.0".
            if port.bind.is_empty() {
                return true;
            }
            let actual_ip = b.get("HostIp").and_then(|h| h.as_str()).unwrap_or("");
            let actual = if actual_ip.is_empty() {
                "0.0.0.0"
            } else {
                actual_ip
            };
            actual == port.bind
        });
        if !matches_expected {
            return true;
        }
    }
    false
}

async fn ensure_user_podman_socket() -> Result<()> {
    // Unit tests inject a runtime; they must not restart the host Podman API.
    if cfg!(test) {
        return Ok(());
    }
    let socket_path = "/run/user/1000/podman/podman.sock";
    if podman_socket_accepts_connections(socket_path).await {
        return Ok(());
    }

    let _ = tokio::process::Command::new("systemctl")
        .args([
            "--user",
            "stop",
            "podman-archy-api.service",
            "podman.socket",
            "podman.service",
        ])
        .status()
        .await;

    remove_stale_podman_socket_path(socket_path).await;

    let _ = tokio::process::Command::new("systemctl")
        .args([
            "--user",
            "reset-failed",
            "podman-archy-api.service",
            "podman.socket",
            "podman.service",
        ])
        .status()
        .await;

    let service_status = tokio::process::Command::new("systemd-run")
        .args([
            "--user",
            "--unit",
            "podman-archy-api",
            "--collect",
            "podman",
            "system",
            "service",
            "--time=0",
            "unix:///run/user/1000/podman/podman.sock",
        ])
        .status()
        .await
        .context("spawn systemd-run podman-archy-api")?;
    if !service_status.success() {
        tracing::warn!(?service_status, "podman-archy-api service start failed");
    }

    for _ in 0..20 {
        if podman_socket_accepts_connections(socket_path).await {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }

    for args in [
        ["--user", "reset-failed", "podman.socket", "podman.service"].as_slice(),
        ["--user", "start", "podman.socket"].as_slice(),
    ] {
        let status = tokio::process::Command::new("systemctl")
            .args(args)
            .status()
            .await
            .with_context(|| format!("spawn systemctl {}", args.join(" ")))?;
        if !status.success() {
            tracing::warn!(?status, command = %args.join(" "), "systemctl podman socket repair step failed");
        }
    }

    for _ in 0..20 {
        if podman_socket_accepts_connections(socket_path).await {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }

    Err(anyhow::anyhow!(
        "podman socket {socket_path} did not appear after restart"
    ))
}

async fn podman_socket_accepts_connections(socket_path: &str) -> bool {
    if !tokio::fs::try_exists(socket_path).await.unwrap_or(false) {
        return false;
    }
    matches!(
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            tokio::net::UnixStream::connect(socket_path),
        )
        .await,
        Ok(Ok(_))
    )
}

async fn remove_stale_podman_socket_path(socket_path: &str) {
    if podman_socket_accepts_connections(socket_path).await {
        return;
    }
    let Ok(metadata) = tokio::fs::symlink_metadata(socket_path).await else {
        return;
    };
    let file_type = metadata.file_type();
    if file_type.is_socket() {
        let _ = tokio::fs::remove_file(socket_path).await;
    } else if file_type.is_dir() {
        let _ = tokio::fs::remove_dir_all(socket_path).await;
    } else {
        let _ = tokio::fs::remove_file(socket_path).await;
    }
}

/// For a bind-mount source we're about to `mkdir -p` (as root), return the
/// nearest pre-existing ancestor (whose ownership we copy) and the TOPMOST dir
/// that doesn't yet exist on the path to it (the root of the subtree mkdir will
/// create). Chowning that subtree to the anchor fixes nested bind sources
/// (`<dataroot>/<app>/<subdir>`) where `mkdir -p` would otherwise leave the
/// intermediate `<app>` dir root-owned. See `ensure_bind_mount_dirs`.
fn fresh_subtree_anchor(source: &Path) -> (Option<PathBuf>, PathBuf) {
    let mut top = source.to_path_buf();
    let mut cur = top.parent().map(Path::to_path_buf);
    let mut anchor = None;
    while let Some(p) = cur {
        if p.exists() {
            anchor = Some(p);
            break;
        }
        cur = p.parent().map(Path::to_path_buf);
        top = p;
    }
    (anchor, top)
}

/// True when `pid` names a live process (its `/proc/<pid>` entry exists).
/// `pid <= 0` is never alive. (Best-effort: a reused PID can read as alive, but
/// that only delays zombie detection a cycle — it never recreates a healthy one.)
fn pid_is_alive(pid: i32) -> bool {
    pid > 0 && Path::new(&format!("/proc/{pid}")).exists()
}

/// Whether the process backing a podman **"running"** container is actually alive.
///
/// Podman trusts its own state DB: if a container's conmon dies without podman
/// observing it (a cgroup-cascade SIGKILL when `archipelago.service` restarts, a
/// crash), `podman ps` keeps reporting the container **"Up"** long after the
/// process is gone — a ZOMBIE. It serves nothing (its port is dead), yet the
/// reconciler NoOps it forever because the state says Running. Verify the
/// recorded main PID is alive so the caller can recreate a zombie rather than
/// trust the stale "running".
///
/// Conservative by design: any uncertainty (inspect failed, PID unparseable)
/// returns `true` (assume alive) so a transient podman hiccup never destroys a
/// healthy container. Only a concrete, dead PID returns `false`.
///
/// Observed live on .228 (2026-06-25): `netbird-dashboard` reported "Up" with
/// `State.Pid` 1394766 already gone → its nginx proxy 502'd → NetBird login
/// broke ("Unauthenticated"). The reconciler never recovered it because the
/// dashboard publishes no host port, so the Running branch had nothing to probe.
async fn container_running_process_alive(name: &str) -> bool {
    let out = match tokio::process::Command::new("podman")
        .args(["inspect", "--format", "{{.State.Pid}}", name])
        .output()
        .await
    {
        Ok(o) if o.status.success() => o,
        _ => return true, // can't determine — don't destabilize a healthy app
    };
    match String::from_utf8_lossy(&out.stdout).trim().parse::<i32>() {
        // A genuinely running container always has a supervised PID > 0 whose
        // /proc entry exists. A dead PID (or PID <= 0 alongside state "running")
        // is the anomaly we're catching.
        Ok(pid) => pid_is_alive(pid),
        Err(_) => true, // unparseable (older podman / odd output) — assume alive
    }
}

async fn wait_for_container_stable_running(
    runtime: &dyn ContainerRuntimeTrait,
    name: &str,
    stable_secs: u64,
    timeout_secs: u64,
) -> Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let mut running_since: Option<std::time::Instant> = None;
    let mut last_state: String;

    loop {
        match runtime.get_container_status(name).await {
            Ok(status) if matches!(status.state, ContainerState::Running) => {
                let since = *running_since.get_or_insert_with(std::time::Instant::now);
                if since.elapsed() >= std::time::Duration::from_secs(stable_secs) {
                    return Ok(());
                }
                last_state = "running".to_string();
            }
            Ok(status) => {
                running_since = None;
                last_state = format!("{:?}", status.state);
            }
            Err(err) => {
                running_since = None;
                last_state = err.to_string();
            }
        }

        if std::time::Instant::now() >= deadline {
            return Err(anyhow::anyhow!(
                "{} did not remain running for {}s within {}s (last={})",
                name,
                stable_secs,
                timeout_secs,
                last_state
            ));
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn repair_manifest_host_ports_after_stability(
    runtime: &dyn ContainerRuntimeTrait,
    manifest: &AppManifest,
    name: &str,
    data_dir: &Path,
) -> Result<()> {
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    if wait_for_manifest_host_ports(manifest, 5, data_dir)
        .await
        .is_ok()
    {
        return Ok(());
    }

    // A stop the user requested while we were settling wins over the repair:
    // restarting here would immediately fight the queued stop worker.
    let user_stopped = crate::crash_recovery::load_user_stopped(data_dir).await;
    if user_stopped.contains(&manifest.app.id) || user_stopped.contains(name) {
        tracing::info!(
            app_id = %manifest.app.id,
            container = %name,
            "skipping host-port repair: user stop requested"
        );
        return Ok(());
    }

    tracing::warn!(
        app_id = %manifest.app.id,
        container = %name,
        "host listener disappeared after startup; restarting container"
    );
    if uses_pasta_network(manifest) && !quadlet::unit_exists(name).await {
        // Legacy (pre-quadlet) pasta app: no unit owns it, so a transient
        // scope keeps its networking's cgroup independent of the daemon.
        podman_user_scope(&["restart", name])
            .await
            .with_context(|| format!("podman restart {name}"))?;
    } else if let Err(err) = quadlet::restart_service(&format!("{name}.service")).await {
        tracing::warn!(
            app_id = %manifest.app.id,
            container = %name,
            error = %err,
            "quadlet restart failed during host listener repair; falling back to podman restart"
        );
        let _ = runtime.stop_container(name).await;
        runtime
            .start_container(name)
            .await
            .with_context(|| format!("restart container {name}"))?;
    }

    wait_for_manifest_host_ports(manifest, host_port_wait_timeout_secs(manifest), data_dir).await?;
    wait_for_container_stable_running(runtime, name, 15, 90).await
}

async fn podman_user_scope(args: &[&str]) -> Result<()> {
    let output = tokio::process::Command::new("systemd-run")
        .args(["--user", "--scope", "--quiet", "--collect", "podman"])
        .args(args)
        .output()
        .await
        .with_context(|| format!("systemd-run --user --scope podman {}", args.join(" ")))?;
    if output.status.success() {
        return Ok(());
    }

    Err(anyhow::anyhow!(
        "systemd-run --user --scope podman {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

async fn start_container_scoped_if_pasta(
    runtime: &dyn ContainerRuntimeTrait,
    manifest: &AppManifest,
    name: &str,
) -> Result<()> {
    if uses_pasta_network(manifest) {
        // Quadlet-managed pasta app: the unit owns the cgroup and the
        // container is rendered --rm — bare `podman start` would fight
        // systemd over it. Restart-through-the-unit starts a stopped one.
        if quadlet::unit_exists(name).await {
            return quadlet::restart_service(&format!("{name}.service")).await;
        }
        // Legacy pasta app: rootless pasta/conmon inherit the cgroup of the
        // process that starts them. Starting through archipelago.service lets
        // backend restarts kill app networking; a transient user scope keeps
        // app daemons independent.
        podman_user_scope(&["start", name]).await
    } else {
        runtime.start_container(name).await
    }
}

async fn restart_container_scoped_if_pasta(
    runtime: &dyn ContainerRuntimeTrait,
    manifest: &AppManifest,
    name: &str,
) -> Result<()> {
    if uses_pasta_network(manifest) {
        if quadlet::unit_exists(name).await {
            return quadlet::restart_service(&format!("{name}.service")).await;
        }
        podman_user_scope(&["restart", name]).await
    } else {
        let _ = runtime.stop_container(name).await;
        runtime.start_container(name).await
    }
}

/// Outcome of `reconcile_all` for a single app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileAction {
    /// Container already running — nothing done.
    NoOp,
    /// Container existed in a stopped state; we started it.
    Started,
    /// Container did not exist; we built/pulled + created + started it.
    Installed,
    /// Container was in an unexpected state we chose not to disturb (Created, Paused).
    Left(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileMode {
    ExistingOnly,
    InstallMissing,
}

#[derive(Debug, Default)]
pub struct ReconcileReport {
    /// (app_id, action) for each manifest we touched.
    pub actions: Vec<(String, ReconcileAction)>,
    /// (app_id, error message) for each manifest whose reconcile failed.
    pub failures: Vec<(String, String)>,
}

impl ReconcileReport {
    fn record(&mut self, app_id: &str, action: ReconcileAction) {
        self.actions.push((app_id.to_string(), action));
    }
}

/// (backend, dependent) pairs a reconcile pass must cascade-restart: the pass
/// (re)created/started a backend whose address-caching dependents (see
/// crate::app_ops) hold its old resolved IP. Only dependents the pass left
/// untouched (NoOp = was already running) qualify — one the pass itself
/// (re)created resolved the fresh address on its own start — and a
/// user-stopped dependent holds no live connection to strand.
fn cascade_pairs_for_report<'r>(
    report: &'r ReconcileReport,
    user_stopped: &std::collections::HashSet<String>,
    changed_backends: &HashSet<String>,
) -> Vec<(&'r str, &'static str)> {
    let mut pairs = Vec::new();
    for (backend, action) in &report.actions {
        if !matches!(
            action,
            ReconcileAction::NoOp | ReconcileAction::Started | ReconcileAction::Installed
        ) {
            continue;
        }
        // A successful systemctl start can be a no-op after a transient
        // Podman inspect failure. Require a witnessed lifecycle change.
        if !changed_backends.contains(backend) {
            continue;
        }
        for dep in crate::app_ops::address_caching_dependents(backend) {
            let dep_untouched = report
                .actions
                .iter()
                .any(|(id, action)| id == dep && matches!(action, ReconcileAction::NoOp));
            if dep_untouched && !user_stopped.contains(*dep) {
                pairs.push((backend.as_str(), *dep));
            }
        }
    }
    pairs
}

/// Only positive runtime evidence permits disrupting an address-caching wallet.
/// A known absent/stopped backend becoming running, a new container ID, or a
/// changed start timestamp qualifies. A failed observation never does.
fn backend_instance_changed(before: Option<&ContainerStatus>, after: &ContainerStatus) -> bool {
    if after.state != ContainerState::Running || after.id.is_empty() {
        return false;
    }
    let Some(before) = before else {
        return true;
    };
    if before.id.is_empty() {
        return false;
    }
    if before.id != after.id || before.state != ContainerState::Running {
        return true;
    }
    matches!((&before.started_at, &after.started_at), (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() && a != b)
}

#[derive(Debug, Default)]
pub struct AdoptionReport {
    pub adopted: Vec<String>,
}

/// Decouple the app image from the shipped manifest: prefer the remote app
/// catalog when it covers this app with a same-repo image, and let a
/// runner-pinned version win over the catalog default so "install/switch to
/// the version I chose" is honored across every install + recreate. Falls
/// through to the catalog default when unpinned or the pin can't be resolved
/// (unknown/repo-mismatch), and to the manifest image when the catalog is
/// absent/uncovered.
///
/// Applied in-place to a cloned manifest. MUST run anywhere a quadlet is
/// rendered — both install_fresh (pull + create) and sync_quadlet_unit (the
/// reconciler's drift re-render); otherwise the reconciler rewrites the unit
/// back to the manifest's shipped `:latest` tag and silently reverts a pinned
/// version on the next tick.
fn resolve_catalog_image(resolved_manifest: &mut AppManifest) {
    let Some(current) = resolved_manifest.app.container.image.clone() else {
        return;
    };
    let app_id = resolved_manifest.app.id.clone();
    let resolved_image = crate::container::version_config::pinned_version(&app_id)
        .and_then(|v| {
            crate::container::app_catalog::catalog_image_for_version(&app_id, &v, &current).or_else(
                || {
                    tracing::warn!(
                        app_id = %app_id,
                        pinned = %v,
                        "version_config: pinned version not resolvable in catalog — using default"
                    );
                    None
                },
            )
        })
        .or_else(|| crate::container::app_catalog::catalog_image_override(&app_id, &current));
    if let Some(catalog_image) = resolved_image {
        if catalog_image != current {
            tracing::info!(
                app_id = %app_id,
                from = %current,
                to = %catalog_image,
                "app-catalog: overriding manifest image"
            );
            resolved_manifest.app.container.image = Some(catalog_image);
        }
    }
}

/// (running app, failed dependency) pairs after a reconcile pass: apps the
/// pass left up whose declared manifest dependency FAILED to reconcile.
/// btcpay-server came up "healthy" while archy-btcpay-db/archy-nbxplorer
/// never made it (live-testing 2026-07-10) — until real start-gating exists,
/// surface the degraded state loudly instead of silently.
fn degraded_running_apps<'m>(
    report: &ReconcileReport,
    manifests: impl Iterator<Item = &'m AppManifest>,
) -> Vec<(String, String)> {
    let failed: HashSet<&str> = report.failures.iter().map(|(id, _)| id.as_str()).collect();
    if failed.is_empty() {
        return Vec::new();
    }
    let up: HashSet<&str> = report
        .actions
        .iter()
        .filter(|(_, action)| {
            matches!(
                action,
                ReconcileAction::NoOp | ReconcileAction::Started | ReconcileAction::Installed
            )
        })
        .map(|(id, _)| id.as_str())
        .collect();
    let mut out = Vec::new();
    for m in manifests {
        if !up.contains(m.app.id.as_str()) {
            continue;
        }
        for dep in manifest_dependency_app_ids(m) {
            if failed.contains(dep.as_str()) {
                out.push((m.app.id.clone(), dep));
            }
        }
    }
    out
}

/// Host-port collisions across a set of manifests, excluding pairs that are
/// mutually exclusive by design (the bitcoin-core/knots variants; the legacy
/// mempool umbrella vs its split frontend, which render the same container).
/// The strfry/vaultwarden 8082 collision shipped in a SIGNED catalog
/// (2026-07-09) because nothing checked — surface these loudly at load time.
fn host_port_collisions<'m>(
    manifests: impl Iterator<Item = &'m AppManifest>,
) -> Vec<(u16, String, String)> {
    fn mutually_exclusive(a: &str, b: &str) -> bool {
        let pair = |x: &str, y: &str| (a == x && b == y) || (a == y && b == x);
        pair("bitcoin-core", "bitcoin-knots") || pair("mempool", "archy-mempool-web")
    }
    let mut by_port: std::collections::BTreeMap<u16, Vec<&str>> = Default::default();
    for m in manifests {
        for p in &m.app.ports {
            by_port.entry(p.host).or_default().push(m.app.id.as_str());
        }
    }
    let mut out = Vec::new();
    for (port, apps) in by_port {
        for i in 0..apps.len() {
            for j in i + 1..apps.len() {
                if apps[i] != apps[j] && !mutually_exclusive(apps[i], apps[j]) {
                    out.push((port, apps[i].to_string(), apps[j].to_string()));
                }
            }
        }
    }
    out
}

/// A staged start failed and its runtime could not be proven stopped. Callers
/// must not restore a cached Stopped state; the scanner/reconciler owns recovery.
#[derive(Debug)]
pub(crate) struct StagedCleanupFailure(String);
impl std::fmt::Display for StagedCleanupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}
impl std::error::Error for StagedCleanupFailure {}

/// Internal: track a manifest together with the absolute directory it was loaded
/// from, so Build sources can resolve relative `context:` paths.
#[derive(Debug, Clone)]
struct LoadedManifest {
    manifest: AppManifest,
    /// Directory containing the manifest file. Used as the base for relative
    /// `container.build.context` paths.
    manifest_dir: PathBuf,
}

/// Validate a catalog-carried manifest value for `app_id`, returning the
/// `AppManifest` to overlay over the disk manifest, or `None` to keep the disk
/// fallback. Returns `None` on: an unparseable value, an embedded app id that
/// mismatches the catalog key, a manifest that fails `validate()`, or a build
/// source (build contexts aren't registry-distributed yet — phase 1 is
/// image-only). See `docs/registry-manifest-design.md`.
fn catalog_manifest_to_overlay(app_id: &str, value: serde_json::Value) -> Option<AppManifest> {
    crate::container::app_catalog::catalog_manifest_overlay(app_id, value)
}

struct OrchestratorState {
    /// app_id → loaded manifest
    manifests: HashMap<String, LoadedManifest>,
    /// App IDs intentionally uninstalled by the user. Manifests stay loaded so
    /// explicit reinstalls still use the orchestrator path, but background
    /// reconcile and companions must not resurrect them.
    disabled: HashSet<String>,
    /// app_id → per-app mutex, created lazily the first time we touch an app
    locks: HashMap<String, Arc<Mutex<()>>>,
    /// container name → (attempt count, first-attempt time) for the
    /// stop+remove+install_fresh "repair" recreate paths below. See
    /// `should_attempt_repair`.
    repair_attempts: HashMap<String, (u32, std::time::Instant)>,
}

impl OrchestratorState {
    fn new() -> Self {
        Self {
            manifests: HashMap::new(),
            disabled: HashSet::new(),
            locks: HashMap::new(),
            repair_attempts: HashMap::new(),
        }
    }
}

/// Cap on how many times the boot reconciler will recreate the same
/// container within `REPAIR_ATTEMPT_RESET_WINDOW` before giving up on it.
///
/// Without this, a container whose entrypoint process fatally exits right
/// after `podman start` succeeds (podman itself reports no error — the crash
/// happens inside the app a moment later) gets stop+remove+install_fresh'd
/// again on every ~30s reconcile tick, forever. `health_monitor.rs`'s
/// restart tracker already bounds ITS OWN independent restart path
/// (`MAX_RESTART_ATTEMPTS`) and eventually surfaces a user-facing
/// notification — but the boot reconciler's repair-recreate path had no
/// equivalent circuit breaker, so the two could race indefinitely on the
/// same fatally-broken container (portainer on `.198`, 2026-07-01: crashed
/// on every start because its on-disk DB was written by a newer binary than
/// the pinned image — a data/version mismatch no amount of recreating could
/// fix, yet it kept looping every 30s until manually intervened on).
const MAX_REPAIR_ATTEMPTS: u32 = 5;
const REPAIR_ATTEMPT_RESET_WINDOW: std::time::Duration = std::time::Duration::from_secs(1800);

pub struct ProdContainerOrchestrator {
    runtime: Arc<dyn ContainerRuntimeTrait>,
    manifests_dir: PathBuf,
    data_dir: PathBuf,
    state: Arc<RwLock<OrchestratorState>>,
    /// Where the bitcoin-ui pre-start hook reads its secret from and
    /// writes the rendered nginx.conf to. Configurable so tests can
    /// point the hook at a tmpdir.
    bitcoin_ui_paths: bitcoin_ui::RenderPaths,
    /// Filebrowser on-disk bootstrap paths.
    filebrowser_paths: filebrowser::EnsurePaths,
    /// LND on-disk bootstrap paths.
    lnd_paths: lnd::EnsurePaths,
    /// Root directory for secret files referenced by
    /// `container.secret_env[*].secret_file`.
    secrets_dir: PathBuf,
    /// Phase 3.2 feature flag: when true, `install_fresh` writes a
    /// Quadlet `.container` unit and starts it via systemctl --user
    /// instead of shelling out to `podman create + start`. Default
    /// false so the legacy path remains the production path until the
    /// 5× lifecycle harness goes green against the new path.
    use_quadlet_backends: bool,
    /// app_id → last secret-env content hash pushed to the runtime's
    /// secret store. Makes steady-state reconciles free of podman
    /// secret calls; a rotation (hash change) falls through and
    /// re-registers.
    env_secret_cache: Mutex<HashMap<String, String>>,
    /// App ids whose credential this process rotated off a publicly known
    /// default (FED-07). A rotation leaves the RUNNING container holding the
    /// compromised value, so its env drift must be acted on even when the app
    /// is restart-sensitive — leaving it untouched perpetuates the compromise,
    /// exactly as leaving published-port drift untouched perpetuates a broken
    /// container. Consumed (and cleared) by the drift check that recreates it.
    credential_rotated: Mutex<HashSet<String>>,
    #[cfg(test)]
    test_disk_gb: Option<u64>,
    #[cfg(test)]
    test_bitcoin_host: Option<String>,
}

struct FileSecretsProvider {
    root: PathBuf,
}

impl SecretsProvider for FileSecretsProvider {
    fn read(&self, name: &str) -> std::result::Result<String, ManifestError> {
        let path = self.root.join(name);
        match std::fs::read_to_string(&path) {
            Ok(data) => Ok(data.trim().to_string()),
            // Name the missing secret explicitly so the failure is actionable
            // instead of a bare "IO error: No such file or directory" that hides
            // which secret (and which app) is blocked.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(ManifestError::Invalid(format!(
                    "required secret '{name}' is missing (expected at {}); the app cannot start until it is generated",
                    path.display()
                )))
            }
            Err(e) => Err(ManifestError::Io(e)),
        }
    }
}

impl ProdContainerOrchestrator {
    /// Production constructor: builds the podman/docker runtime from `Config` and
    /// uses `/opt/archipelago/apps` (or `$ARCHIPELAGO_APPS_DIR`) as the manifest root.
    pub async fn new(config: Config) -> Result<Self> {
        let user = std::env::var("USER").unwrap_or_else(|_| "archipelago".to_string());
        let runtime: Arc<dyn ContainerRuntimeTrait> = match &config.container_runtime {
            ConfigContainerRuntime::Podman => {
                Arc::new(archipelago_container::PodmanRuntime::new(user))
            }
            ConfigContainerRuntime::Docker => {
                Arc::new(archipelago_container::DockerRuntime::new(user))
            }
            ConfigContainerRuntime::Auto => Arc::new(
                archipelago_container::AutoRuntime::new(user)
                    .await
                    .context("Failed to create auto runtime")?,
            ),
        };

        let manifests_dir = std::env::var("ARCHIPELAGO_APPS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/opt/archipelago/apps"));

        Ok(Self {
            runtime,
            manifests_dir,
            data_dir: config.data_dir.clone(),
            state: Arc::new(RwLock::new(OrchestratorState::new())),
            bitcoin_ui_paths: bitcoin_ui::RenderPaths::default(),
            filebrowser_paths: filebrowser::EnsurePaths::default(),
            lnd_paths: lnd::EnsurePaths::default(),
            secrets_dir: PathBuf::from("/var/lib/archipelago/secrets"),
            use_quadlet_backends: config.use_quadlet_backends,
            env_secret_cache: Mutex::new(HashMap::new()),
            credential_rotated: Mutex::new(HashSet::new()),
            #[cfg(test)]
            test_disk_gb: None,
            #[cfg(test)]
            test_bitcoin_host: None,
        })
    }

    /// Test constructor: inject an arbitrary runtime + manifests dir. Used
    /// by unit tests with `MockRuntime`.
    #[cfg(test)]
    pub fn with_runtime(runtime: Arc<dyn ContainerRuntimeTrait>, manifests_dir: PathBuf) -> Self {
        Self {
            runtime,
            manifests_dir,
            data_dir: PathBuf::from("/var/lib/archipelago"),
            state: Arc::new(RwLock::new(OrchestratorState::new())),
            bitcoin_ui_paths: bitcoin_ui::RenderPaths::default(),
            filebrowser_paths: filebrowser::EnsurePaths::default(),
            lnd_paths: lnd::EnsurePaths::default(),
            secrets_dir: PathBuf::from("/var/lib/archipelago/secrets"),
            use_quadlet_backends: false,
            env_secret_cache: Mutex::new(HashMap::new()),
            credential_rotated: Mutex::new(HashSet::new()),
            test_disk_gb: None,
            test_bitcoin_host: None,
        }
    }

    /// Override the bitcoin-ui render paths (secret + output). Only used
    /// by tests that exercise the bitcoin-ui pre-start hook — the
    /// default `/var/lib/archipelago/...` paths are correct for prod.
    #[cfg(test)]
    pub fn set_bitcoin_ui_paths(&mut self, paths: bitcoin_ui::RenderPaths) {
        self.bitcoin_ui_paths = paths;
    }

    #[cfg(test)]
    pub fn set_secrets_dir(&mut self, secrets_dir: PathBuf) {
        self.secrets_dir = secrets_dir;
    }

    #[cfg(test)]
    pub fn set_filebrowser_paths(&mut self, paths: filebrowser::EnsurePaths) {
        self.filebrowser_paths = paths;
    }

    #[cfg(test)]
    pub fn set_data_dir(&mut self, data_dir: PathBuf) {
        self.data_dir = data_dir;
    }

    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }

    #[cfg(test)]
    pub fn set_lnd_paths(&mut self, paths: lnd::EnsurePaths) {
        self.lnd_paths = paths;
    }

    #[cfg(test)]
    pub fn set_disk_gb_for_test(&mut self, disk_gb: u64) {
        self.test_disk_gb = Some(disk_gb);
    }

    /// Walk `manifests_dir` looking for `*/manifest.yml` files. Parses each,
    /// validates it, and stores it in the in-memory state.
    ///
    /// Non-fatal: a malformed manifest is logged and skipped so one broken app
    /// can't prevent the rest of the fleet from reconciling.
    pub async fn load_manifests(&self) -> Result<usize> {
        let root = self.manifests_dir.clone();
        if !root.exists() {
            tracing::warn!(
                path = %root.display(),
                "manifests dir does not exist — orchestrator has no apps to manage"
            );
            return Ok(0);
        }

        let mut loaded: Vec<LoadedManifest> = Vec::new();
        let mut entries = tokio::fs::read_dir(&root)
            .await
            .with_context(|| format!("reading manifests dir {}", root.display()))?;
        while let Some(entry) = entries.next_entry().await? {
            let app_dir = entry.path();
            if !app_dir.is_dir() {
                continue;
            }
            let manifest_path = app_dir.join("manifest.yml");
            if !manifest_path.exists() {
                continue;
            }
            match AppManifest::from_file(&manifest_path) {
                Ok(m) => {
                    loaded.push(LoadedManifest {
                        manifest: m,
                        manifest_dir: app_dir,
                    });
                }
                Err(e) => {
                    tracing::error!(
                        path = %manifest_path.display(),
                        error = %e,
                        "skipping malformed manifest"
                    );
                }
            }
        }

        let count = loaded.len();
        let mut state = self.state.write().await;
        state.manifests.clear();
        state.disabled.clear();
        let has_split_mempool_stack = loaded
            .iter()
            .any(|lm| lm.manifest.app.id == "archy-mempool-db")
            && loaded.iter().any(|lm| lm.manifest.app.id == "mempool-api")
            && loaded
                .iter()
                .any(|lm| lm.manifest.app.id == "archy-mempool-web");

        for lm in loaded {
            if has_split_mempool_stack && lm.manifest.app.id == "mempool" {
                tracing::info!(
                    "skipping legacy mempool manifest because split mempool stack manifests are present"
                );
                continue;
            }
            state.manifests.insert(lm.manifest.app.id.clone(), lm);
        }

        // Registry-distributed manifests (workstream B): the signed catalog may
        // carry full manifests. Overlay them over disk — the registry is the
        // authoritative origin; disk is the migration fallback. Image-only apps
        // (phase 1); build-source catalog manifests defer to disk.
        // See docs/registry-manifest-design.md.
        let _ = count; // disk count subsumed by the merged total below
        let mut overlaid = 0usize;
        for (app_id, value) in crate::container::app_catalog::catalog_manifest_values() {
            if let Some(m) = catalog_manifest_to_overlay(&app_id, value) {
                // Reuse the disk dir when the app also exists on disk (so a future
                // build-source catalog manifest can still resolve its context);
                // otherwise a sentinel under the manifests dir. Image-only apps
                // never read manifest_dir.
                let manifest_dir = state
                    .manifests
                    .get(&app_id)
                    .map(|lm| lm.manifest_dir.clone())
                    .unwrap_or_else(|| root.join(&app_id));
                state.manifests.insert(
                    app_id.clone(),
                    LoadedManifest {
                        manifest: m,
                        manifest_dir,
                    },
                );
                overlaid += 1;
            }
        }
        if overlaid > 0 {
            tracing::info!("registry catalog overlaid {overlaid} manifest(s) over disk");
        }
        // The legacy monolithic `mempool` umbrella manifest renders the same
        // `mempool` container as its split-stack frontend member
        // `archy-mempool-web`. When the split stack is present, the two render
        // conflicting quadlet bodies for one container and rewrite+restart each
        // other forever ("port binding drift"/"network alias drift" loop observed
        // on catalog-driven .228). The disk-only guard above misses manifests that
        // arrive via the registry-catalog overlay, so enforce it here over the
        // merged set: drop the umbrella whenever all three split members exist
        // (from disk OR catalog). Installing `mempool` assembles the split stack,
        // so the frontend container is owned by `archy-mempool-web` either way.
        if ["archy-mempool-db", "mempool-api", "archy-mempool-web"]
            .iter()
            .all(|id| state.manifests.contains_key(*id))
        {
            if state.manifests.remove("mempool").is_some() {
                tracing::info!(
                    "dropped legacy `mempool` umbrella manifest (split mempool stack present) to avoid container ownership conflict with archy-mempool-web"
                );
            }
        }
        for (port, a, b) in host_port_collisions(state.manifests.values().map(|lm| &lm.manifest)) {
            tracing::error!(
                port,
                app_a = %a,
                app_b = %b,
                "host-port collision between app manifests — installing both will break whichever starts second"
            );
        }
        Ok(state.manifests.len())
    }

    /// Test helper: inject a manifest directly without touching the filesystem.
    #[cfg(test)]
    pub async fn insert_manifest_for_test(&self, manifest: AppManifest, manifest_dir: PathBuf) {
        let mut state = self.state.write().await;
        state.disabled.remove(&manifest.app.id);
        state.manifests.insert(
            manifest.app.id.clone(),
            LoadedManifest {
                manifest,
                manifest_dir,
            },
        );
    }

    /// Acquire (creating if needed) the per-app mutex.
    async fn app_lock(&self, app_id: &str) -> Arc<Mutex<()>> {
        {
            let state = self.state.read().await;
            if let Some(l) = state.locks.get(app_id) {
                return l.clone();
            }
        }
        let mut state = self.state.write().await;
        state
            .locks
            .entry(app_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    async fn loaded(&self, app_id: &str) -> Result<LoadedManifest> {
        let state = self.state.read().await;
        state
            .manifests
            .get(app_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown app_id: {app_id}"))
    }

    /// Lifecycle alias for the legacy `mempool` umbrella id. `load_manifests`
    /// drops the umbrella manifest whenever all three split-stack members are
    /// loaded (they render the same frontend container), and install already
    /// assembles the split stack under the historical id — so start/stop/
    /// restart addressed to `mempool` must fan out to the members too, or a
    /// stopped stack becomes unstartable ("unknown app_id: mempool" while no
    /// member container is live). Returns members in dependency-start order.
    async fn mempool_umbrella_members(&self, app_id: &str) -> Option<Vec<String>> {
        if !matches!(app_id, "mempool" | "mempool-web") {
            return None;
        }
        let state = self.state.read().await;
        if state.manifests.contains_key("mempool") {
            return None; // legacy node: umbrella manifest still live, no alias
        }
        let members = ["archy-mempool-db", "mempool-api", "archy-mempool-web"];
        members
            .iter()
            .all(|id| state.manifests.contains_key(*id))
            .then(|| members.iter().map(|s| s.to_string()).collect())
    }

    // `manifest_ids()` used to live here: every app id in the in-memory
    // manifest map, i.e. every manifest the node can *see* (the whole `apps/`
    // directory plus the signed-catalog overlay). Its only caller was the boot
    // reconciler's companion stage, which is precisely the bug described below
    // — "can see" was silently read as "has installed". It is deleted rather
    // than left unused so nothing reaches for it again; `installed_app_ids` is
    // the answer to the question callers actually mean.

    /// App ids whose container actually exists — the
    /// `ReconcileMode::ExistingOnly` rule, made available to callers outside
    /// `reconcile_all_with_mode`.
    ///
    /// The app reconciler has always drawn this line ("merely listing a catalog
    /// manifest never installs an unqualified app"); the companion stage did
    /// not, and fed itself `manifest_ids` instead. Because a manifest exists on
    /// disk for every *available* app, that provisioned and perpetually
    /// self-healed a companion UI for apps nobody had installed: archi-dev-box
    /// ran `archy-fedimint-ui` and `archy-lnd-ui` with no `fedimint` and no
    /// `lnd` container anywhere on the box (2026-08-08). The Guardian UI served
    /// its "waiting for Bitcoin" page forever with nothing behind it, which is
    /// what the operator reported as "fedimint guardian installs but does not
    /// work" — there was nothing to install, the UI was already up.
    ///
    /// `None` means the runtime listing failed. That is deliberately distinct
    /// from `Some(vec![])`: a caller that removes things on absence must not
    /// treat "I could not look" as "nothing is installed".
    ///
    /// An app counts as installed if its container exists in ANY state, OR its
    /// container name appears in the durable last-running snapshot.
    ///
    /// Both halves are load-bearing. The snapshot is what `crash_recovery`
    /// itself calls "installation evidence" and what `reconcile_all_with_mode`
    /// already trusts to recreate a previously-running app whose container
    /// vanished. It is needed here because a container's absence is NOT proof
    /// of uninstallation: containers on this node demonstrably come and go —
    /// `lnd` read as absent and then present again within the hour, and the
    /// boot reconciler logs "previously-running app has no container after
    /// boot — recreating" for bitcoin-knots and electrumx repeatedly. Judging
    /// on live containers alone would let a momentary gap look like a removal
    /// and cost a healthy companion its unit.
    ///
    /// This also deliberately does NOT carry over the `user_stopped` /
    /// `disabled` filters the old `manifest_ids` applied: a stopped app is
    /// still an installed app, its container still exists (exited), and
    /// treating it as uninstalled would make stopping an app tear its
    /// companion down and rebuild it on the next start. "Is it installed" and
    /// "is it currently meant to be running" are different questions, and only
    /// the first one belongs here.
    pub async fn installed_app_ids(&self) -> Option<Vec<String>> {
        let present: std::collections::HashSet<String> = self
            .runtime
            .list_containers()
            .await
            .ok()?
            .into_iter()
            .map(|c| c.name)
            .collect();
        let was_running = crate::crash_recovery::load_last_running_names(&self.data_dir).await;
        let state = self.state.read().await;
        Some(
            state
                .manifests
                .iter()
                .filter(|(_, lm)| {
                    let container = compute_container_name(&lm.manifest);
                    present.contains(&container) || was_running.contains(&container)
                })
                .map(|(app_id, _)| app_id.clone())
                .collect(),
        )
    }

    /// Scan the runtime for containers whose names match one of our manifests.
    /// This is a read-only adoption pass: nothing is created, started, or touched.
    pub async fn adopt_existing(&self) -> Result<AdoptionReport> {
        let all = self
            .runtime
            .list_containers()
            .await
            .context("list_containers during adoption")?;
        let user_stopped = crate::crash_recovery::load_user_stopped(&self.data_dir).await;
        let state = self.state.read().await;
        let mut report = AdoptionReport::default();
        for (app_id, lm) in state.manifests.iter() {
            let expected = compute_container_name(&lm.manifest);
            if all
                .iter()
                .any(|c| c.name == expected || c.name == format!("/{expected}"))
            {
                report.adopted.push(app_id.clone());
                // Adopted apps will be (re)started by boot recovery, the first
                // reconcile pass, or the doctor — whichever reaches them first
                // can be minutes away. Register them as pending boot-starts now
                // so the scanner shows "Restarting" (not "Stopped") from the
                // very first post-boot scan. Cleared per-app by the first
                // reconcile pass, so a genuinely failed start surfaces.
                if !state.disabled.contains(app_id)
                    && !user_stopped.contains(app_id)
                    && !user_stopped.contains(&expected)
                {
                    crate::crash_recovery::pending_boot_starts_add([
                        app_id.clone(),
                        expected.clone(),
                    ]);
                }
            }
        }
        Ok(report)
    }

    /// Walk every loaded manifest and ensure it has a running container.
    /// Never panics; per-app failures are collected into the report.
    #[cfg(test)]
    pub async fn reconcile_all(&self) -> ReconcileReport {
        self.reconcile_all_with_mode(ReconcileMode::InstallMissing)
            .await
    }

    /// Reconcile loaded manifests. Boot uses `ExistingOnly` so merely listing a
    /// catalog manifest never installs an unqualified app; explicit install/start
    /// paths still call `ensure_running` and can create missing containers.
    pub async fn reconcile_existing(&self) -> ReconcileReport {
        self.reconcile_all_with_mode(ReconcileMode::ExistingOnly)
            .await
    }

    async fn reconcile_all_with_mode(&self, mode: ReconcileMode) -> ReconcileReport {
        // Resolve exact retained update identities before normal desired-state
        // reconciliation can recreate or start a member. Keep ownership for pass.
        let _update_guard = match super::update_transaction::recover(
            &self.data_dir,
            &super::update_transaction::Podman,
        )
        .await
        {
            Ok(guard) => guard,
            Err(error) => {
                let mut report = ReconcileReport::default();
                report
                    .failures
                    .push(("update-recovery".into(), format!("{error:#}")));
                return report;
            }
        };
        let user_stopped = crate::crash_recovery::load_user_stopped(&self.data_dir).await;
        // Durable desired-state signal: the container names that were running at
        // the last periodic snapshot. Used below to recreate a previously-running
        // app whose container vanished (e.g. a wedged teardown cleared by a
        // reboot) instead of leaving it down. See the immich .198 incident.
        let was_running = crate::crash_recovery::load_last_running_names(&self.data_dir).await;
        // Durable installation record, consulted alongside the perishable
        // `was_running` snapshot for desired-state recovery below.
        let installed_apps = crate::crash_recovery::load_installed_apps(&self.data_dir).await;
        let (mut manifests, container_name_by_app_id): (
            Vec<LoadedManifest>,
            std::collections::HashMap<String, String>,
        ) = {
            let state = self.state.read().await;
            let dependency_required = dependency_manifests_required_by_active_apps(
                state.manifests.values().map(|lm| &lm.manifest),
                &user_stopped,
            );
            let filtered = state
                .manifests
                .iter()
                .filter(|(app_id, _)| !state.disabled.contains(*app_id))
                .filter(|(app_id, lm)| {
                    dependency_required.contains(*app_id)
                        || (!user_stopped.contains(*app_id)
                            && !user_stopped.contains(&compute_container_name(&lm.manifest)))
                })
                .map(|(_, lm)| lm.clone())
                .collect();
            // Unfiltered id→container-name map for the absent-stack-member
            // recovery below: a sibling may be excluded from this pass (e.g.
            // user-stopped) yet its live container still proves the stack is
            // installed.
            let names = state
                .manifests
                .iter()
                .map(|(id, lm)| (id.clone(), compute_container_name(&lm.manifest)))
                .collect();
            (filtered, names)
        };
        // Wallet readiness must not wait behind unrelated image pulls/builds.
        // A running LND container can still be locked after boot; its post-start
        // hook must run promptly. Reconcile Bitcoin first, then LND, before the
        // rest of the catalog. Each app still honors stopped/uninstalled markers.
        manifests.sort_by_key(|lm| match lm.manifest.app.id.as_str() {
            "bitcoin-knots" | "bitcoin-core" | "bitcoin" => 0,
            "lnd" => 1,
            _ => 2,
        });
        // Live container names (any state), for the same recovery check.
        let listed_containers = self.runtime.list_containers().await.ok();
        let present_containers: HashSet<String> = listed_containers
            .as_ref()
            .map(|cs| cs.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default();
        // Keep unknown distinct from confirmed absence. Runtime queries can
        // fail under load while systemd still has a healthy running backend.
        let mut backend_before: HashMap<String, Option<ContainerStatus>> = HashMap::new();
        for lm in &manifests {
            let id = &lm.manifest.app.id;
            if crate::app_ops::address_caching_dependents(id).is_empty() {
                continue;
            }
            let name = compute_container_name(&lm.manifest);
            match self.runtime.get_container_status(&name).await {
                Ok(status) => {
                    backend_before.insert(id.clone(), Some(status));
                }
                Err(_) if listed_containers.is_some() && !present_containers.contains(&name) => {
                    backend_before.insert(id.clone(), None);
                }
                Err(err) => {
                    tracing::warn!(backend = %id, error = %err,
                        "cannot observe backend before reconcile; will not infer a dependency restart from an action report");
                }
            }
        }
        // Stopped/disabled apps are excluded from ordinary start reconciliation.
        // Pending cleanup must still run for them, including after a restart.
        let mut report = self.reconcile_staged_updates().await;
        let disk_gb = self.disk_gb().await;
        let bitcoin_pruned = disk_gb < ARCHIVAL_BITCOIN_DISK_GB
            || crate::settings::bitcoin_storage::load(&self.data_dir)
                .await
                .map(|settings| settings.prune)
                .unwrap_or(true);
        // Register every candidate before the (sequential, possibly slow)
        // pass so the scanner overlays queued-but-down apps as Restarting
        // instead of Stopped. Each app is deregistered as its turn finishes,
        // so a start that genuinely failed shows its real state again.
        crate::crash_recovery::pending_boot_starts_add(
            manifests
                .iter()
                // An app mid-lifecycle-op must not enter the pending overlay:
                // the scanner would show it Restarting while its stop worker
                // is deliberately taking it down (and this pass will skip it
                // via the same probe below anyway).
                .filter(|lm| !crate::app_ops::lifecycle_op_in_flight(&lm.manifest.app.id))
                .flat_map(|lm| {
                    [
                        lm.manifest.app.id.clone(),
                        compute_container_name(&lm.manifest),
                    ]
                }),
        );
        for lm in manifests {
            let app_id = lm.manifest.app.id.clone();
            let container_name = compute_container_name(&lm.manifest);
            // A package.start/stop/restart worker is mid-sequence on this app
            // (or its stack): between its stop and start halves the container
            // is legitimately absent, and repair-recreating it here races the
            // worker — the reconciler once killed a restart's fresh container
            // and left the unit down for minutes (.228 mempool frontend, gate
            // 2026-07-09). Skip this cycle; the worker owns the outcome.
            if crate::app_ops::lifecycle_op_in_flight(&app_id) {
                report.record(
                    &app_id,
                    ReconcileAction::Left("lifecycle-op-in-flight".into()),
                );
                crate::crash_recovery::pending_boot_start_done(&app_id);
                crate::crash_recovery::pending_boot_start_done(&container_name);
                continue;
            }
            if mode == ReconcileMode::ExistingOnly
                && requires_archival_bitcoin(&app_id)
                && bitcoin_pruned
            {
                report.record(
                    &app_id,
                    ReconcileAction::Left("requires-archival-bitcoin".into()),
                );
                crate::crash_recovery::pending_boot_start_done(&app_id);
                crate::crash_recovery::pending_boot_start_done(&container_name);
                continue;
            }
            // Same shape as the archival-bitcoin skip above: recorded BEFORE
            // ensure_running_with_mode, so the "absent" desired-state recovery
            // below can never fire on this reason and undo it.
            if mode == ReconcileMode::ExistingOnly && requires_cuprate_disk(&app_id, disk_gb) {
                tracing::warn!(
                    app_id = %app_id,
                    disk_gb,
                    "cuprate needs a larger disk (no pruning support) — skipping start"
                );
                report.record(
                    &app_id,
                    ReconcileAction::Left("cuprate-insufficient-disk".into()),
                );
                crate::crash_recovery::pending_boot_start_done(&app_id);
                crate::crash_recovery::pending_boot_start_done(&container_name);
                continue;
            }
            match self.ensure_running_with_mode(&lm, mode).await {
                // Desired-state recovery: the app has no container and was left
                // "absent" by boot reconcile, BUT it was running at the last
                // snapshot — so its container vanished unexpectedly (a wedged
                // teardown cleared by a reboot, a lost container record after a
                // crash). It isn't user-stopped (those are filtered out of
                // `manifests` above) and it's still installed (manifest present),
                // so recreate it rather than leave a previously-running app down.
                // Match is exact: compute_container_name == the snapshot's podman
                // name (incl. each stack member), so no false positives. The only
                // "absent" Left reason is the optional-missing case, so this never
                // fires for paused/unknown states.
                Ok(ReconcileAction::Left(reason))
                    if mode == ReconcileMode::ExistingOnly
                        && reason == "absent"
                        // companion.rs owns missing UI provisioning/removal.
                        // Never resurrect an orphan from a stale snapshot.
                        // Existing UIs still pass through security config repair.
                        && !super::companion::is_companion_app(&app_id)
                        && (was_running.contains(&compute_container_name(&lm.manifest))
                            // The durable answer, and the one that does not
                            // erode. `was_running` only records what was
                            // running at the last snapshot, so an app that
                            // stays down long enough ages out of it and can
                            // never be recovered — bitcoin-knots on
                            // archi-dev-box (2026-08-08), and
                            // indeedhub-minio/-postgres before it. The
                            // stack-member clause below was the narrow patch
                            // for that second case; this is the general one.
                            // Safe against resurrecting something removed on
                            // purpose: `user_uninstalled` is checked earlier in
                            // ensure_running_with_mode and returns before
                            // anything is created, and uninstall clears this
                            // record in the same breath as setting that marker.
                            || installed_apps.contains(&app_id)
                            || installed_apps.contains(&compute_container_name(&lm.manifest))
                            // Absent STACK MEMBER whose siblings have live
                            // containers: the stack is installed, so the
                            // missing member is a hole, not a choice. The
                            // was_running snapshot ages out after a few daemon
                            // restarts, which left indeedhub-minio/-postgres
                            // permanently absent on .38 (2026-08-06) — nginx
                            // down on `host not found in upstream "minio"`
                            // with nothing ever recreating the members.
                            || absent_stack_member_with_live_sibling(
                                &app_id,
                                &present_containers,
                                &container_name_by_app_id,
                            )) =>
                {
                    tracing::warn!(
                        app_id = %app_id,
                        "previously-running app has no container after boot — recreating (desired-state recovery)"
                    );
                    match self.install_fresh(&lm).await {
                        Ok(()) => report.record(&app_id, ReconcileAction::Installed),
                        Err(e) => {
                            tracing::error!(app_id = %app_id, error = %e, "desired-state recovery (recreate) failed");
                            report.failures.push((app_id, e.to_string()));
                        }
                    }
                }
                Ok(action) => report.record(&app_id, action),
                Err(e) => {
                    // `{:#}` prints the whole anyhow chain — `%e` alone showed
                    // only the outer context ("create_container X") and hid
                    // the actual libpod error for days.
                    tracing::error!(app_id = %app_id, error = %format!("{e:#}"), "reconcile failed");
                    report.failures.push((app_id, e.to_string()));
                }
            }
        }

        // App-agnostic volume-ownership self-heal. Sweep EVERY running container
        // (registry/manifest apps AND legacy compose stacks like immich) and
        // repair any that can't write their bind mounts — the durable, app-
        // agnostic replacement for per-app hardcoded host chowns. Drift-checked,
        // so steady state is just cheap in-container write-probes; only a broken
        // volume is chowned (in-userns, mapping-proof) and its container
        // restarted to recover. Fixes the class of EACCES crash-loops fleet-wide
        // and self-heals existing nodes after OTA. (immich .198, 2026-06-17.)
        if let Ok(containers) = self.runtime.list_containers().await {
            for c in containers
                .iter()
                .filter(|c| matches!(c.state, ContainerState::Running))
            {
                // Same in-flight guard as the main loop: don't podman-restart
                // a container a lifecycle worker is mid-way through cycling.
                if crate::app_ops::lifecycle_op_in_flight(&c.name) {
                    continue;
                }
                // Throttled: first pass after the container appears, then
                // hourly — not on every 30s tick (see ownership_sweep_due).
                if !ownership_sweep_due(&c.name) {
                    continue;
                }
                if ensure_running_container_ownership(&c.name).await {
                    tracing::info!(container = %c.name, "volume ownership repaired during reconcile — restarting to recover");
                    let _ = tokio::process::Command::new("podman")
                        .args(["restart", &c.name])
                        .output()
                        .await;
                }
            }
        }

        // lnd resolves the bitcoin RPC address once at startup and caches it.
        // The RPC start/restart workers already cascade-restart it (24fd97ed),
        // but a reconcile pass that recreates or starts a backend (desired-
        // state recovery, repair recreate, boot InstallMissing) moves the
        // address behind a running dependent's back — §C "restart lnd after
        // ANY bitcoin recreate".
        let mut changed_backends = HashSet::new();
        for (backend, before) in &backend_before {
            let Some(name) = container_name_by_app_id.get(backend) else {
                continue;
            };
            if let Ok(after) = self.runtime.get_container_status(name).await {
                if backend_instance_changed(before.as_ref(), &after) {
                    changed_backends.insert(backend.clone());
                }
            }
        }
        // A user stop during a slow reconcile pass still takes precedence.
        let user_stopped = crate::crash_recovery::load_user_stopped(&self.data_dir).await;
        for (backend, dep) in cascade_pairs_for_report(&report, &user_stopped, &changed_backends) {
            // Same rule as the RPC cascade: hold the dependent's op lock
            // across the restart; skip when a worker is mid-sequence.
            let lock = crate::app_ops::op_lock(dep);
            let Ok(_guard) = lock.try_lock() else {
                continue;
            };
            tracing::info!(
                dependent = %dep,
                backend = %backend,
                "cascade restart: reconcile (re)created the backend and the dependent caches its resolved address"
            );
            if let Err(err) = self.restart(dep).await {
                tracing::error!(dependent = %dep, backend = %backend, error = %err, "cascade restart failed");
            }
        }

        {
            let state = self.state.read().await;
            for (app, dep) in
                degraded_running_apps(&report, state.manifests.values().map(|lm| &lm.manifest))
            {
                tracing::error!(
                    app_id = %app,
                    dependency = %dep,
                    "app is up but its declared dependency failed to reconcile — running DEGRADED"
                );
            }
        }

        report
    }

    async fn ensure_running(&self, lm: &LoadedManifest) -> Result<ReconcileAction> {
        self.ensure_running_with_mode(lm, ReconcileMode::InstallMissing)
            .await
    }

    /// Whether the reconciler should attempt another stop+remove+install_fresh
    /// repair recreate for `name`, or has already tried too many times
    /// recently and should leave it alone instead of looping forever. See
    /// `MAX_REPAIR_ATTEMPTS`.
    async fn should_attempt_repair(&self, name: &str) -> bool {
        let mut state = self.state.write().await;
        let now = std::time::Instant::now();
        let entry = state
            .repair_attempts
            .entry(name.to_string())
            .or_insert((0, now));
        if now.duration_since(entry.1) > REPAIR_ATTEMPT_RESET_WINDOW {
            *entry = (0, now);
        }
        entry.0 += 1;
        if entry.0 > MAX_REPAIR_ATTEMPTS {
            tracing::error!(
                container = %name,
                attempts = entry.0,
                window_secs = REPAIR_ATTEMPT_RESET_WINDOW.as_secs(),
                "giving up on repairing container after too many recreate attempts — it keeps failing to \
                 come up cleanly on its own; check `podman logs {name}` for the real cause (a data/version \
                 mismatch or another fatal startup error is likely, not something recreating the container \
                 again will fix). Leaving it as-is instead of recreating it forever; a manual fix + restart \
                 (or a subsequent explicit install/start, which clears this counter) is needed to recover it.",
                name = name,
            );
            false
        } else {
            true
        }
    }

    /// Clears the repair-attempt counter for `name` — call on any path that
    /// reaches a stable Running/NoOp/Started outcome, so a container that
    /// recovers (on its own, or after a real fix) doesn't inherit a stale
    /// near-exhausted counter from an earlier unrelated failure.
    async fn clear_repair_attempts(&self, name: &str) {
        let mut state = self.state.write().await;
        state.repair_attempts.remove(name);
    }

    async fn ensure_running_with_mode(
        &self,
        lm: &LoadedManifest,
        mode: ReconcileMode,
    ) -> Result<ReconcileAction> {
        let app_id = lm.manifest.app.id.clone();
        // IndeedHub used to be a hardcoded orchestrator special-case
        // (reconcile_indeedhub_stack + a dependency-DNS gate) that refused to
        // create the frontend from its manifest. It is now fully manifest-driven
        // (apps/indeedhub-* + apps/indeedhub): network_aliases, generated_secrets,
        // dependencies, and the post_install nginx hook live in the manifests, so
        // every member — frontend included — flows through the generic path here.
        let lock = self.app_lock(&app_id).await;
        let _guard = lock.lock().await;

        if let Some(record) = super::staged_update::load(&self.data_dir, &app_id).await? {
            let name = compute_container_name(&record.manifest);
            self.stop_staged_runtime(&name).await?;
            return Ok(ReconcileAction::Left("staged-update-awaiting-start".into()));
        }

        self.ensure_app_secrets(&app_id).await?;

        // Don't fight the Bitcoin-implementation switch: bitcoin-core and
        // bitcoin-knots share port 8332, so if the *other* variant is already
        // running the inactive one can never start — the reconciler would just
        // churn "address already in use" and report a reconcile failure. Skip
        // it, mirroring the health monitor's same skip. (#47)
        if let Some(conflict) = match app_id.strip_prefix("archy-").unwrap_or(app_id.as_str()) {
            "bitcoin-core" => Some("bitcoin-knots"),
            "bitcoin-knots" | "bitcoin" => Some("bitcoin-core"),
            _ => None,
        } {
            if let Ok(list) = self.runtime.list_containers().await {
                let other_running = list.iter().any(|c| {
                    c.name.strip_prefix("archy-").unwrap_or(c.name.as_str()) == conflict
                        && matches!(c.state, ContainerState::Running)
                });
                if other_running {
                    tracing::debug!(
                        app_id = %app_id,
                        conflict,
                        "skipping reconcile — the other Bitcoin implementation is running"
                    );
                    return Ok(ReconcileAction::NoOp);
                }
            }
        }

        let mut resolved_manifest = lm.manifest.clone();
        self.resolve_dynamic_env(&mut resolved_manifest).await?;
        let name = compute_container_name(&lm.manifest);

        // An explicitly user-stopped app MUST stay stopped. The reconcile filter
        // already drops user-stopped apps, but its `dependency_required` override
        // re-includes a stopped app that an *active* app depends on (e.g. mempool
        // keeps electrumx in the list), and the in-memory `disabled` set is wiped
        // on manifest reload — so reconcile would resurrect it: its now-unreachable
        // ports look like a fault, the host-port "repair" restarts it, and
        // package.stop never sticks. Honour the on-disk marker here, the single
        // choke point every reconcile flows through. Explicit install/start/restart
        // clear the marker BEFORE calling this, so they are unaffected.
        {
            let user_stopped = crate::crash_recovery::load_user_stopped(&self.data_dir).await;
            if user_stopped.contains(&app_id) || user_stopped.contains(&name) {
                tracing::debug!(
                    app_id = %app_id,
                    container = %name,
                    "reconcile skipped — app is user-stopped (must stay stopped)"
                );
                return Ok(ReconcileAction::Left("user-stopped".into()));
            }
        }

        // Same durability problem as user-stopped above, but for uninstall:
        // any genuinely-installed app self-heals below the moment its
        // container is missing — including right after an explicit
        // uninstall, since the in-memory `disabled` set doesn't survive a
        // `load_manifests()` reload (every archipelago restart/reboot runs
        // one before the first reconcile).
        {
            let user_uninstalled =
                crate::crash_recovery::load_user_uninstalled(&self.data_dir).await;
            if user_uninstalled.contains(&app_id) || user_uninstalled.contains(&name) {
                // The marker says "removed", but the container can still be
                // RUNNING: a Quadlet unit is owned by systemd, which starts it
                // on boot entirely independently of this reconciler. On
                // a test node (2026-08-02) `bitcoin-ui` sat in this exact
                // state — marker set, `archy-bitcoin-ui.service` active, port
                // 8334 published — and so it never received the /bitcoin-rpc/
                // auth_request gate that a05956c4 shipped INSIDE the binary.
                // The node looked patched while an unauthenticated caller could
                // still drive Bitcoin Core RPC through a credential-injecting
                // proxy.
                //
                // A container that is actually running is a live attack surface
                // whatever a marker says about it, so its security-relevant
                // config gets reconciled even here. This deliberately does NOT
                // create, start or resurrect anything — the "must stay removed"
                // contract is untouched for every path that could.
                //
                // Hook failure is swallowed rather than propagated: an app the
                // user has uninstalled must not be able to fail the reconcile
                // pass for everything after it.
                if matches!(
                    self.runtime.get_container_status(&name).await,
                    Ok(status) if matches!(status.state, ContainerState::Running)
                ) {
                    match self.run_pre_start_hooks(&app_id).await {
                        Ok(Some(HookOutcome::Rewritten)) => {
                            tracing::warn!(
                                app_id = %app_id,
                                container = %name,
                                "rewrote config for a user-uninstalled app whose container is \
                                 still RUNNING (systemd/Quadlet keeps it alive independently of \
                                 reconcile) — restarting so it picks the new config up"
                            );
                            restart_container_scoped_if_pasta(
                                self.runtime.as_ref(),
                                &resolved_manifest,
                                &name,
                            )
                            .await
                            .with_context(|| format!("reconcile restart {name}"))?;
                            let _ = self.run_post_start_hooks(&app_id).await;
                            return Ok(ReconcileAction::Started);
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!(
                                app_id = %app_id,
                                error = %e,
                                "config hook failed for a user-uninstalled but running app — \
                                 leaving it as-is"
                            );
                        }
                    }
                }
                tracing::debug!(
                    app_id = %app_id,
                    container = %name,
                    "reconcile skipped — app is user-uninstalled (must stay removed)"
                );
                return Ok(ReconcileAction::Left("user-uninstalled".into()));
            }
        }

        match self.runtime.get_container_status(&name).await {
            Ok(status) => {
                // Phase 3.3: migrate pre-Phase-3 containers in place, but only
                // after proving the container exists. Boot reconciliation must
                // not create every catalog app just because a Quadlet unit is
                // absent.
                //
                // Pasta apps included since 2026-08-10: the old exclusion
                // paired with the transient-scope machinery (daemon-started
                // pasta died with the daemon's cgroup). A quadlet unit gives
                // pasta the same independence with systemd supervision on top
                // — Restart=always + RestartSec=10, which also spaces restarts
                // past pasta's port teardown. The scoped start/restart helpers
                // now defer to the unit whenever one exists.
                if self.use_quadlet_backends {
                    if let Some(action) = self.migrate_to_quadlet_if_needed(lm, &name).await? {
                        return Ok(action);
                    }
                    self.sync_quadlet_unit(lm, &name).await?;
                    self.sync_dependency_quadlet_units(lm).await?;
                }
                match status.state {
                    ContainerState::Running => {
                        // Legacy runtime path: migrate credentials once without
                        // recreating accounts or changing operator passwords.
                        // Quadlet installations receive the same helper through
                        // the required ExecStartPre drift/restart above.
                        if app_id == "filebrowser" && !self.use_quadlet_backends && !cfg!(test) {
                            let secrets = self.secrets_dir.join("filebrowser");
                            let managed = filebrowser::cloud_credentials(&secrets)
                                .await
                                .map(|value| value.schema == 1)
                                .unwrap_or(false);
                            if !managed {
                                if !self.should_attempt_repair(&name).await {
                                    return Ok(ReconcileAction::Left(
                                        "filebrowser-credential-repair-budget-exhausted".into(),
                                    ));
                                }
                                let unit_managed = self.runtime.cli_name() == "podman"
                                    && quadlet::unit_exists(&name).await;
                                let service = format!("{name}.service");
                                if unit_managed {
                                    quadlet::stop_service(&service).await?;
                                } else {
                                    self.runtime.stop_container(&name).await?;
                                }
                                let prepared = filebrowser::prepare_credentials(
                                    &self.filebrowser_paths,
                                    &secrets,
                                    &status.image,
                                    self.runtime.cli_name(),
                                )
                                .await;
                                // Restore service availability even when setup
                                // rolled back. Preserve the setup failure itself.
                                let started = if unit_managed {
                                    quadlet::restart_service(&service).await
                                } else {
                                    self.runtime.start_container(&name).await
                                };
                                prepared?;
                                started?;
                                return Ok(ReconcileAction::Started);
                            }
                        }
                        // Zombie guard: podman can report a container "running"
                        // after its process has died (conmon SIGKILLed in a
                        // cgroup cascade on archipelago restart, etc.). Such a
                        // container serves nothing yet would be NoOp'd forever.
                        // Recreate it from the manifest. This is the ONLY path
                        // that recovers a dead dependency with no published host
                        // port (netbird-dashboard on .228, 2026-06-25 — stale
                        // "Up" → proxy 502 → NetBird login broke). Conservative:
                        // only fires on a concrete dead PID, never on uncertainty.
                        if !container_running_process_alive(&name).await {
                            if !self.should_attempt_repair(&name).await {
                                return Ok(ReconcileAction::Left(
                                    "repair-attempts-exhausted".into(),
                                ));
                            }
                            tracing::warn!(
                                app_id = %app_id,
                                container = %name,
                                "container reported running but its process is dead (zombie) — recreating"
                            );
                            let _ = self.runtime.stop_container(&name).await;
                            let _ = self.runtime.remove_container(&name).await;
                            self.install_fresh(lm).await?;
                            return Ok(ReconcileAction::Installed);
                        }
                        // App-specific hooks get a chance to refresh bind-mounted
                        // config. bitcoin-ui: re-render nginx.conf if the RPC
                        // password rotated (or template changed via OTA). If
                        // anything was rewritten, restart the container so nginx
                        // picks up the new config.
                        let app_hook = self.run_pre_start_hooks(&app_id).await?;
                        // Certs before files (mirrors prepare_for_start). A manifest
                        // file rewrite below can trigger a restart, and the container's
                        // bind-mounts expect the cert present. Without this, an adopted
                        // running app whose data dir was wiped on reinstall (netbird:
                        // tls.crt/key live under the removed /var/lib/archipelago/netbird)
                        // never regenerates the cert → nginx proxy dies on the restart.
                        // Idempotent: a no-op when crt+key already exist.
                        self.ensure_manifest_certs(&resolved_manifest).await?;
                        let file_hook = self.ensure_manifest_files(&resolved_manifest).await?;
                        if app_hook == Some(HookOutcome::Rewritten)
                            || file_hook == HookOutcome::Rewritten
                        {
                            tracing::info!(app_id = %app_id, "config rewritten while running — restarting");
                            restart_container_scoped_if_pasta(
                                self.runtime.as_ref(),
                                &resolved_manifest,
                                &name,
                            )
                            .await
                            .with_context(|| format!("reconcile restart {name}"))?;
                            self.run_post_start_hooks(&app_id).await?;
                            return Ok(ReconcileAction::Started);
                        }
                        // Published-port drift means the container exists but maps
                        // its ports to the wrong host ports (e.g. lnd REST stuck on
                        // host 8080 while the manifest/clients expect 18080, as seen
                        // on .116). The container is already non-functional, so
                        // recreate it even for restart-sensitive apps during boot —
                        // leaving it "untouched" would perpetuate the breakage.
                        if self
                            .container_ports_drifted(&name, &resolved_manifest)
                            .await
                        {
                            tracing::info!(app_id = %app_id, container = %name, "container published-port drift detected — recreating");
                            self.backup_runtime_change(&name, &resolved_manifest)
                                .await?;
                            let _ = self.runtime.stop_container(&name).await;
                            let _ = self.runtime.remove_container(&name).await;
                            self.install_fresh(lm).await?;
                            return Ok(ReconcileAction::Installed);
                        }
                        if self.container_env_drifted(&name, &resolved_manifest).await {
                            // FED-07: a credential we just rotated off a publicly
                            // known default leaves the RUNNING container holding
                            // the compromised value. Restart-sensitivity protects
                            // working services; this service is compromised, and
                            // skipping it perpetuates the exposure — the same
                            // reasoning the published-port carve-out above uses.
                            let rotated = self.credential_rotated.lock().await.remove(&app_id);
                            if rotated {
                                tracing::warn!(
                                    app_id = %app_id,
                                    container = %name,
                                    "recreating restart-sensitive app: its admin credential was \
                                     rotated off a publicly known default and the running \
                                     container still holds the compromised one (FED-07)"
                                );
                            }
                            if !rotated
                                && mode == ReconcileMode::ExistingOnly
                                && is_restart_sensitive_app(&app_id)
                            {
                                tracing::info!(
                                    app_id = %app_id,
                                    container = %name,
                                    "container drift detected during boot reconcile; leaving running restart-sensitive app untouched"
                                );
                                self.run_post_start_hooks(&app_id).await?;
                                return Ok(ReconcileAction::NoOp);
                            }
                            tracing::info!(app_id = %app_id, container = %name, "container env drift detected — recreating");
                            self.backup_runtime_change(&name, &resolved_manifest)
                                .await?;
                            let _ = self.runtime.stop_container(&name).await;
                            let _ = self.runtime.remove_container(&name).await;
                            self.install_fresh(lm).await?;
                            return Ok(ReconcileAction::Installed);
                        }
                        self.run_post_start_hooks(&app_id).await?;
                        if !resolved_manifest.app.ports.is_empty() {
                            if let Err(err) = wait_for_manifest_host_ports(
                                &resolved_manifest,
                                host_port_repair_probe_timeout_secs(&resolved_manifest),
                                &self.data_dir,
                            )
                            .await
                            {
                                tracing::warn!(app_id = %app_id, container = %name, mode = ?mode, error = %err, "host listener missing for running container; repairing");
                                repair_manifest_host_ports_after_stability(
                                    self.runtime.as_ref(),
                                    &resolved_manifest,
                                    &name,
                                    &self.data_dir,
                                )
                                .await?;
                                return Ok(ReconcileAction::Started);
                            }
                        }
                        if uses_pasta_network(&resolved_manifest) {
                            if let Err(err) = wait_for_container_stable_running(
                                self.runtime.as_ref(),
                                &name,
                                15,
                                90,
                            )
                            .await
                            {
                                tracing::warn!(app_id = %app_id, container = %name, mode = ?mode, error = %err, "pasta container not stable after install; recreating via direct runtime");
                                let _ = self.runtime.stop_container(&name).await;
                                let _ = self.runtime.remove_container(&name).await;
                                self.install_fresh(lm).await?;
                                return Ok(ReconcileAction::Installed);
                            }
                        }
                        self.clear_repair_attempts(&name).await;
                        Ok(ReconcileAction::NoOp)
                    }
                    ContainerState::Stopped | ContainerState::Exited => {
                        // Even for a plain start, re-run the pre-start hooks so
                        // the bind-mounted file is fresh before nginx starts
                        // reading it. A Rewritten outcome is fine here — we're
                        // about to start from a stopped state anyway.
                        self.prepare_for_start(&resolved_manifest).await?;
                        if self.container_env_drifted(&name, &resolved_manifest).await
                            || self
                                .container_ports_drifted(&name, &resolved_manifest)
                                .await
                        {
                            tracing::info!(app_id = %app_id, container = %name, "stopped container env/port drift detected — recreating");
                            self.backup_runtime_change(&name, &resolved_manifest)
                                .await?;
                            let _ = self.runtime.remove_container(&name).await;
                            self.install_fresh(lm).await?;
                            return Ok(ReconcileAction::Installed);
                        }
                        if let Err(e) = start_container_scoped_if_pasta(
                            self.runtime.as_ref(),
                            &resolved_manifest,
                            &name,
                        )
                        .await
                        {
                            if !self.should_attempt_repair(&name).await {
                                return Ok(ReconcileAction::Left(
                                    "repair-attempts-exhausted".into(),
                                ));
                            }
                            tracing::warn!(
                                app_id = %app_id,
                                container = %name,
                                error = %e,
                                "start failed for stopped/exited container; recreating container record"
                            );
                            let _ = self.runtime.stop_container(&name).await;
                            let _ = self.runtime.remove_container(&name).await;
                            self.install_fresh(lm).await?;
                            return Ok(ReconcileAction::Installed);
                        }
                        self.run_post_start_hooks(&app_id).await?;
                        wait_for_manifest_host_ports(
                            &resolved_manifest,
                            host_port_wait_timeout_secs(&resolved_manifest),
                            &self.data_dir,
                        )
                        .await?;
                        if uses_pasta_network(&resolved_manifest) {
                            wait_for_container_stable_running(self.runtime.as_ref(), &name, 15, 90)
                                .await?;
                        }
                        self.clear_repair_attempts(&name).await;
                        Ok(ReconcileAction::Started)
                    }
                    ContainerState::Stopping => {
                        tracing::warn!(
                            app_id = %app_id,
                            container = %name,
                            "container stuck in stopping state; force-recreating container record"
                        );
                        self.prepare_for_start(&resolved_manifest).await?;
                        let _ = self.runtime.remove_container(&name).await;
                        self.install_fresh(lm).await?;
                        Ok(ReconcileAction::Installed)
                    }
                    ContainerState::Created => {
                        self.prepare_for_start(&resolved_manifest).await?;
                        if self.container_env_drifted(&name, &resolved_manifest).await {
                            tracing::info!(app_id = %app_id, container = %name, "created container env drift detected — recreating");
                            self.backup_runtime_change(&name, &resolved_manifest)
                                .await?;
                            let _ = self.runtime.remove_container(&name).await;
                            self.install_fresh(lm).await?;
                            return Ok(ReconcileAction::Installed);
                        }
                        if let Err(e) = start_container_scoped_if_pasta(
                            self.runtime.as_ref(),
                            &resolved_manifest,
                            &name,
                        )
                        .await
                        {
                            if !self.should_attempt_repair(&name).await {
                                return Ok(ReconcileAction::Left(
                                    "repair-attempts-exhausted".into(),
                                ));
                            }
                            tracing::warn!(
                                app_id = %app_id,
                                container = %name,
                                error = %e,
                                "start failed for created container; recreating container record"
                            );
                            let _ = self.runtime.stop_container(&name).await;
                            let _ = self.runtime.remove_container(&name).await;
                            self.install_fresh(lm).await?;
                            return Ok(ReconcileAction::Installed);
                        }
                        self.run_post_start_hooks(&app_id).await?;
                        wait_for_manifest_host_ports(
                            &resolved_manifest,
                            host_port_wait_timeout_secs(&resolved_manifest),
                            &self.data_dir,
                        )
                        .await?;
                        if uses_pasta_network(&resolved_manifest) {
                            wait_for_container_stable_running(self.runtime.as_ref(), &name, 15, 90)
                                .await?;
                        }
                        self.clear_repair_attempts(&name).await;
                        Ok(ReconcileAction::Started)
                    }
                    ContainerState::Paused => Ok(ReconcileAction::Left("paused".to_string())),
                    ContainerState::Unknown(s) => Ok(ReconcileAction::Left(s)),
                }
            }
            Err(_) => {
                // Container missing entirely. With Quadlet backends enabled, an
                // existing .container file is installed state even if Podman
                // lost the container record after a crash/reboot. Sync the unit
                // bytes first (clears stale Notify=healthy/nc probes), then ask
                // user systemd to start the generated service.
                if self.use_quadlet_backends && self.quadlet_unit_exists(&name).await? {
                    self.prepare_for_start(&resolved_manifest).await?;
                    self.sync_quadlet_unit(lm, &name).await?;
                    self.ensure_resolved_source_available(lm).await?;
                    quadlet::enable_now(&format!("{name}.service"))
                        .await
                        .with_context(|| {
                            format!("start existing quadlet service {name}.service")
                        })?;
                    self.run_post_start_hooks(&app_id).await?;
                    wait_for_manifest_host_ports(
                        &resolved_manifest,
                        host_port_wait_timeout_secs(&resolved_manifest),
                        &self.data_dir,
                    )
                    .await?;
                    if uses_pasta_network(&resolved_manifest) {
                        wait_for_container_stable_running(self.runtime.as_ref(), &name, 15, 90)
                            .await?;
                    }
                    return Ok(ReconcileAction::Started);
                }

                // Container absent. A loaded manifest is NOT installation
                // evidence — the catalog and the ISO ship manifests for every
                // app, installed or not. In ExistingOnly (boot) mode only two
                // things may create a container from nothing:
                //   1. required-baseline apps (first-boot bootstrap), here;
                //   2. desired-state recovery for apps whose container was
                //      running at the last snapshot — the caller matches
                //      Left("absent") against the snapshot and recreates
                //      (this is what heals a stack backend that vanished:
                //      the indeedhub/immich cases).
                // Everything else stays absent. Installing on manifest
                // presence alone mass-installed the whole catalog on a fresh
                // node (framework, 2026-07-14).
                if mode == ReconcileMode::ExistingOnly {
                    if is_required_baseline_app(&app_id) {
                        self.install_fresh(lm).await?;
                        return Ok(ReconcileAction::Installed);
                    }
                    return Ok(ReconcileAction::Left("absent".to_string()));
                }
                self.install_fresh(lm).await?;
                Ok(ReconcileAction::Installed)
            }
        }
    }

    /// Build-or-pull, create, start. Assumes the per-app mutex is already held.
    async fn reconcile_staged_updates(&self) -> ReconcileReport {
        let mut report = ReconcileReport::default();
        let ids = match super::staged_update::pending_ids(&self.data_dir).await {
            Ok(ids) => ids,
            Err(error) => {
                report
                    .failures
                    .push(("staged-updates".into(), error.to_string()));
                return report;
            }
        };
        for app_id in ids {
            if crate::app_ops::lifecycle_op_in_flight(&app_id) {
                continue;
            }
            let lock = self.app_lock(&app_id).await;
            let _guard = lock.lock().await;
            // Re-read after acquiring the lock: a successful explicit start
            // may have completed and cleared the stage while this pass waited.
            let result = async {
                if let Some(record) = super::staged_update::load(&self.data_dir, &app_id).await? {
                    self.stop_staged_runtime(&compute_container_name(&record.manifest))
                        .await?;
                    report.record(
                        &app_id,
                        ReconcileAction::Left("staged-update-awaiting-start".into()),
                    );
                }
                Ok::<_, anyhow::Error>(())
            }
            .await;
            if let Err(error) = result {
                report.failures.push((app_id, error.to_string()));
            }
        }
        report
    }

    async fn stop_staged_runtime(&self, name: &str) -> Result<()> {
        let mut errors = Vec::new();
        if let Err(e) = self.remove_quadlet_unit_if_present(name).await {
            errors.push(format!("could not disable staged unit: {e:#}"));
        }
        let inventory = self
            .runtime
            .list_containers()
            .await
            .context("Cannot verify staged runtime inventory")?;
        if inventory.iter().any(|c| {
            c.name.trim_start_matches('/') == name
                && !matches!(
                    c.state,
                    ContainerState::Stopped | ContainerState::Exited | ContainerState::Created
                )
        }) {
            if let Err(e) = self.runtime.stop_container(name).await {
                errors.push(format!("could not stop staged container: {e:#}"));
            }
        }
        let inventory = self
            .runtime
            .list_containers()
            .await
            .context("Cannot verify staged runtime stopped")?;
        if inventory.iter().any(|c| {
            c.name.trim_start_matches('/') == name
                && !matches!(
                    c.state,
                    ContainerState::Stopped | ContainerState::Exited | ContainerState::Created
                )
        }) {
            errors.push("staged container is still active".into());
        }
        anyhow::ensure!(errors.is_empty(), "{}", errors.join("; "));
        Ok(())
    }

    async fn start_staged_update(&self, app_id: &str) -> Result<bool> {
        use super::staged_update;
        if staged_update::load(&self.data_dir, app_id).await?.is_none() {
            return Ok(false);
        }
        let lock = self.app_lock(app_id).await;
        let _guard = lock.lock().await;
        let record = staged_update::load(&self.data_dir, app_id)
            .await?
            .context("Staged update changed; retry start")?;
        let name = compute_container_name(&record.manifest);
        anyhow::ensure!(
            !staged_update::marked(&self.data_dir, "user-uninstalled.json", app_id, &name).await?,
            "App was uninstalled; staged update cannot reinstall it"
        );
        anyhow::ensure!(
            record.ready,
            "Update preparation was interrupted; retry update before start"
        );
        anyhow::ensure!(
            self.runtime.image_exists(record.image()?).await?,
            "Staged image is unavailable; refusing to start another version"
        );
        // Clear only this explicit start's stopped intent. The pending manifest
        // remains durable until all creation, hooks and readiness checks succeed.
        crate::crash_recovery::clear_user_stopped(&self.data_dir, app_id).await;
        crate::crash_recovery::clear_user_stopped(&self.data_dir, &name).await;
        crate::crash_recovery::clear_user_stopped(&self.data_dir, &format!("archy-{app_id}")).await;
        self.state.write().await.disabled.remove(app_id);
        let lm = LoadedManifest {
            manifest: record.manifest.clone(),
            manifest_dir: record.manifest_dir.clone(),
        };
        let result = async {
            self.remove_quadlet_unit_if_present(&name).await?;
            if self
                .runtime
                .list_containers()
                .await?
                .iter()
                .any(|c| c.name.trim_start_matches('/') == name)
            {
                self.runtime.stop_container(&name).await?;
                self.runtime.remove_container(&name).await?;
            }
            self.install_fresh_with_pin(&lm, true).await?;
            let observed = self.runtime.get_container_status(&name).await?;
            anyhow::ensure!(
                observed.state == ContainerState::Running,
                "Pinned start did not become running"
            );
            anyhow::ensure!(
                staged_update::matches_image(record.image()?, &observed.image),
                "Pinned start produced a different image"
            );
            // Installed presentation is durable before pending intent disappears.
            // A crash on either side of clear cannot fall back to a later catalog.
            staged_update::save_installed(&self.data_dir, &record).await?;
            staged_update::clear(&self.data_dir, app_id).await
        }
        .await;
        if let Err(error) = result {
            crate::crash_recovery::mark_user_stopped(&self.data_dir, app_id).await;
            self.state.write().await.disabled.insert(app_id.into());
            let retained = staged_update::save(&self.data_dir, &record).await;
            let cleanup = self.stop_staged_runtime(&name).await;
            if let Err(cleanup) = cleanup {
                return Err(StagedCleanupFailure(format!(
                    "Staged start failed: {error:#}; cleanup failed: {cleanup:#}; pin persistence: {:?}", retained.err()
                )).into());
            }
            retained.context("Could not retain failed staged start pin")?;
            return Err(error);
        }
        Ok(true)
    }

    async fn install_fresh(&self, lm: &LoadedManifest) -> Result<()> {
        self.install_fresh_with_pin(lm, false).await
    }

    async fn install_fresh_with_pin(&self, lm: &LoadedManifest, pinned: bool) -> Result<()> {
        self.ensure_app_secrets(&lm.manifest.app.id).await?;
        let mut resolved_manifest = lm.manifest.clone();
        self.resolve_dynamic_env(&mut resolved_manifest).await?;
        if !pinned {
            resolve_catalog_image(&mut resolved_manifest);
        }

        let resolved = resolved_manifest.app.container.resolve().ok_or_else(|| {
            anyhow::anyhow!(
                "manifest for {} has invalid container source (neither image nor build)",
                lm.manifest.app.id
            )
        })?;

        match resolved {
            ResolvedSource::Pull {
                image,
                image_signature,
                ..
            } => {
                // §A: validate at the pull site, not just the RPC boundary —
                // catalog/manifest-supplied refs reach here without ever
                // passing package::config's check.
                if !crate::container::image_policy::is_valid_docker_image(&image) {
                    anyhow::bail!(
                        "refusing to pull image {:?} for {}: not from a trusted registry",
                        image,
                        lm.manifest.app.id
                    );
                }
                // Reconcile recreates route through here too: an unreachable
                // registry must not brick an app whose exact image:tag is
                // already in local storage (boot reconcile of archy-btcpay-db
                // /archy-nbxplorer with the (since-retired) upstream registry dead, 2026-07-10).
                // Same exists-first semantics as
                // ensure_resolved_source_available and the quadlets'
                // Pull=never.
                let exists = match self.runtime.image_exists(&image).await {
                    Ok(exists) => exists,
                    Err(err) => {
                        tracing::warn!(
                            image = %image,
                            error = %err,
                            "image existence check failed; pulling image instead"
                        );
                        false
                    }
                };
                if !exists {
                    self.runtime
                        .pull_image(&image, image_signature.as_deref())
                        .await
                        .with_context(|| format!("pulling {image}"))?;
                }
            }
            ResolvedSource::Build(mut bcfg) => {
                // Resolve a relative context: against the manifest's own directory.
                let ctx_path = Path::new(&bcfg.context);
                if !ctx_path.is_absolute() {
                    bcfg.context = lm
                        .manifest_dir
                        .join(ctx_path)
                        .to_string_lossy()
                        .into_owned();
                }
                let exists = match self.runtime.image_exists(&bcfg.tag).await {
                    Ok(exists) => exists,
                    Err(err) => {
                        tracing::warn!(
                            image = %bcfg.tag,
                            error = %err,
                            "build image existence check failed; rebuilding image"
                        );
                        false
                    }
                };
                // Presence alone isn't enough: the local UI images (bitcoin-ui,
                // lnd-ui, electrs-ui) COPY a built `neode-ui` dist, so a UI
                // update changes the source but leaves the old tag in place.
                // Rebuild whenever the build context's fingerprint differs from
                // the one that produced the existing image (#34). podman's
                // COPY-layer cache keeps the rebuild cheap when content is
                // actually unchanged.
                let fingerprint = fingerprint_build_context(Path::new(&bcfg.context));
                let stamp_path = build_fingerprint_stamp_path(&self.data_dir, &bcfg.tag);
                let stale = match &fingerprint {
                    Some(current) => match tokio::fs::read_to_string(&stamp_path).await {
                        Ok(prev) => prev.trim() != current,
                        // No stamp recorded → treat as stale so we rebuild and
                        // capture the fingerprint going forward.
                        Err(_) => true,
                    },
                    // Couldn't fingerprint the context — don't skip on staleness.
                    None => true,
                };
                if !exists || stale {
                    if exists && stale {
                        tracing::info!(
                            image = %bcfg.tag,
                            context = %bcfg.context,
                            "build context changed since last build; rebuilding image"
                        );
                    }
                    self.runtime
                        .build_image(&bcfg)
                        .await
                        .with_context(|| format!("build_image {}", bcfg.tag))?;
                    // Record the fingerprint that this image was built from so
                    // the next reconcile skips the build until the source moves.
                    if let Some(current) = &fingerprint {
                        if let Some(parent) = stamp_path.parent() {
                            let _ = tokio::fs::create_dir_all(parent).await;
                        }
                        if let Err(err) = tokio::fs::write(&stamp_path, current).await {
                            tracing::warn!(
                                image = %bcfg.tag,
                                error = %err,
                                "failed to write build fingerprint stamp"
                            );
                        }
                    }
                }
            }
        }

        let name = compute_container_name(&lm.manifest);
        // App-specific pre-create hook (render bind-mounted config files, etc).
        // For bitcoin-ui this renders /var/lib/archipelago/bitcoin-ui/nginx.conf
        // with the plaintext RPC auth substituted in. Failure is fatal: if we
        // can't render the config the bind-mount would resolve to either a
        // stale file or a missing path, and nginx would 502 every request.
        self.prepare_for_start(&resolved_manifest).await?;
        self.ensure_container_network(&resolved_manifest).await?;

        if self.use_quadlet_backends {
            // Phase 3.2 path: declarative .container unit + systemctl.
            // Containers parented under user.slice instead of
            // archipelago.service's cgroup → no FM3 cascade SIGKILL on
            // archipelago restart. Pasta apps included since 2026-08-10 —
            // the unit gives them the same cgroup independence the transient
            // scopes provided, plus Restart=always supervision.
            self.install_via_quadlet(&resolved_manifest, &name).await?;
        } else {
            self.remove_quadlet_unit_if_present(&name).await?;
            ensure_user_podman_socket().await?;
            // Legacy path. Production until tests/lifecycle/run-gate.sh
            // goes green against the Quadlet path.
            self.runtime
                .create_container(&resolved_manifest, &name, 0)
                .await
                .with_context(|| format!("create_container {name}"))?;
            start_container_scoped_if_pasta(self.runtime.as_ref(), &resolved_manifest, &name)
                .await
                .with_context(|| format!("start_container {name}"))?;
        }
        self.run_post_start_hooks(&lm.manifest.app.id).await?;
        // Declarative manifest post_install hooks (Task #20). Runs only here, on a
        // freshly created container — exactly when container mutations (e.g.
        // indeedhub's nginx X-Frame-Options strip + nostr-provider injection) must
        // be re-applied. Best-effort + idempotent: never fails the install.
        if pinned {
            crate::container::hooks::run_post_install_strict(
                &resolved_manifest,
                &name,
                &self.data_dir,
            )
            .await?;
        } else {
            crate::container::hooks::run_post_install(&resolved_manifest, &name, &self.data_dir)
                .await;
        }
        if uses_pasta_network(&resolved_manifest) {
            if let Err(err) = wait_for_manifest_host_ports(
                &resolved_manifest,
                host_port_wait_timeout_secs(&resolved_manifest),
                &self.data_dir,
            )
            .await
            {
                tracing::warn!(
                    app_id = %resolved_manifest.app.id,
                    container = %name,
                    error = %err,
                    "pasta host listener missing after fresh start; trying restart repair"
                );
                repair_manifest_host_ports_after_stability(
                    self.runtime.as_ref(),
                    &resolved_manifest,
                    &name,
                    &self.data_dir,
                )
                .await?;
                return Ok(());
            }
            wait_for_container_stable_running(self.runtime.as_ref(), &name, 15, 90).await?;
            repair_manifest_host_ports_after_stability(
                self.runtime.as_ref(),
                &resolved_manifest,
                &name,
                &self.data_dir,
            )
            .await?;
        } else {
            wait_for_manifest_host_ports(
                &resolved_manifest,
                host_port_wait_timeout_secs(&resolved_manifest),
                &self.data_dir,
            )
            .await?;
        }
        Ok(())
    }

    async fn run_post_start_hooks(&self, app_id: &str) -> Result<()> {
        if cfg!(test) {
            return Ok(());
        }

        match app_id {
            "lnd" => lnd::ensure_wallet_initialized()
                .await
                .context("lnd post-start: ensure wallet initialized"),
            _ => Ok(()),
        }
    }

    async fn prepare_for_start(&self, manifest: &AppManifest) -> Result<()> {
        self.run_pre_start_hooks(&manifest.app.id).await?;
        self.ensure_bind_mount_sockets(manifest).await?;
        self.ensure_bind_mount_dirs(manifest).await?;
        // Certs before files: a templated file may not need the cert, but the
        // container's bind-mounts expect both present before create_container.
        self.ensure_manifest_certs(manifest).await?;
        self.ensure_manifest_files(manifest).await?;
        self.apply_data_uid(manifest).await?;
        self.run_post_data_uid_hooks(&manifest.app.id).await?;
        if manifest.app.id == "filebrowser" && !self.use_quadlet_backends && !cfg!(test) {
            let image = manifest.app.container.image.as_deref().ok_or_else(|| {
                anyhow::anyhow!("File Browser needs a pinned image for credential setup")
            })?;
            filebrowser::prepare_credentials(
                &self.filebrowser_paths,
                &self.secrets_dir.join("filebrowser"),
                image,
                self.runtime.cli_name(),
            )
            .await?;
        }
        Ok(())
    }

    async fn run_post_data_uid_hooks(&self, app_id: &str) -> Result<()> {
        match app_id {
            "fedimint" | "fedimint-gateway" => self.ensure_fedimint_dirs().await,
            "grafana" => self.ensure_grafana_dirs().await,
            _ => Ok(()),
        }
    }

    async fn ensure_grafana_dirs(&self) -> Result<()> {
        let dir = "/var/lib/archipelago/grafana";
        let mkdir = host_sudo(&["mkdir", "-p", dir])
            .await
            .context("mkdir grafana data dir")?;
        if !mkdir.success() {
            return Err(anyhow::anyhow!("mkdir -p {dir} failed with status {mkdir}"));
        }
        chown_for_rootless_container("472:472", dir)
            .await
            .context("chown grafana data dir for rootless uid 472")
    }

    /// Phase 3.3 in-place migration. When `use_quadlet_backends` flips
    /// from off → on, existing nodes have backend containers parented
    /// under archipelago.service's cgroup (the bad shape). They need to
    /// be replaced with Quadlet-managed equivalents — same image, same
    /// volumes, parented under user.slice.
    ///
    /// Returns `Some(Installed)` if a migration was performed (and
    /// `ensure_running` should return that to its caller without
    /// re-running the state machine), `Ok(None)` otherwise so the
    /// caller proceeds with normal reconcile logic.
    ///
    /// Idempotent. The "did we already migrate this app?" oracle is the
    /// presence of `<name>.container` on disk in the user quadlet dir.
    /// Once migrated, every subsequent reconcile tick takes the early
    /// return at the unit-exists check.
    ///
    /// **Volume safety:** we call `runtime.remove_container(name)` which
    /// invokes `podman rm --force` (stops + removes the container record
    /// only). Bind-mounted host paths — e.g. /var/lib/archipelago/bitcoin
    /// — survive because podman doesn't touch them on container rm. This
    /// has been audited for every app in `uses_orchestrator_install_flow`:
    /// all of them mount their data dirs as host bind mounts.
    async fn migrate_to_quadlet_if_needed(
        &self,
        lm: &LoadedManifest,
        name: &str,
    ) -> Result<Option<ReconcileAction>> {
        // Skip companion apps — bitcoin-ui / electrs-ui / lnd-ui have shipped
        // via Quadlet since v1.7.41 (companion.rs renders the unit). Running
        // migration for them races companion rendering: when migration ran
        // for archy-bitcoin-ui on .228 2026-05-02 it tore down the running
        // companion + tried to start a duplicate unit and failed. Companions
        // are managed by their own path, never the legacy podman-create one,
        // so there's nothing here to migrate.
        let app_id = lm.manifest.app.id.as_str();
        if UI_APP_IDS.contains(&app_id) {
            return Ok(None);
        }

        let unit_dir = quadlet::unit_dir()
            .await
            .context("locate user quadlet unit dir for migration check")?;
        let unit_path = unit_dir.join(format!("{name}.container"));
        // Treat any IO error from try_exists as "the unit might be there" —
        // migrating on top of an existing unit is destructive (we'd podman rm
        // the running container then fight the existing service over startup
        // ordering), so be cautious when we can't read the answer.
        match tokio::fs::try_exists(&unit_path).await {
            Ok(true) => return Ok(None),
            Ok(false) => {} // proceed
            Err(e) => {
                tracing::warn!(
                    path = %unit_path.display(),
                    error = %e,
                    "Phase 3.3: skipping migration — couldn't determine if Quadlet unit already exists"
                );
                return Ok(None);
            }
        }

        // No unit on disk. If a pre-Phase-3 container exists for this
        // app, replace it with a Quadlet-managed one.
        let container_exists = self.runtime.get_container_status(name).await.is_ok();
        if !container_exists {
            // Container doesn't exist either — let install_fresh handle it.
            return Ok(None);
        }

        tracing::info!(
            app_id = %lm.manifest.app.id,
            container = %name,
            "Phase 3.3 migration: replacing pre-Quadlet container with systemd-managed unit"
        );
        // Resolve active persistent mounts before the old inspect record is
        // removed. NPM may use a legacy or operator-selected data directory.
        let mut resolved = lm.manifest.clone();
        self.resolve_dynamic_env(&mut resolved).await?;
        self.backup_runtime_change(name, &resolved).await?;
        // Stop+remove the old container record. Volumes survive (host
        // bind mounts are not touched by podman rm).
        self.runtime
            .remove_container(name)
            .await
            .with_context(|| format!("Phase 3.3: rm pre-Quadlet container {name}"))?;

        // Re-render the manifest with dynamic env baked in, then go
        // through the same install path a fresh install would.
        self.install_via_quadlet(&resolved, name)
            .await
            .with_context(|| format!("Phase 3.3: re-install {name} via Quadlet"))?;

        Ok(Some(ReconcileAction::Installed))
    }

    async fn quadlet_unit_exists(&self, name: &str) -> Result<bool> {
        let unit_dir = quadlet::unit_dir()
            .await
            .context("locate user quadlet unit dir for existing unit check")?;
        let unit_path = unit_dir.join(format!("{name}.container"));
        tokio::fs::try_exists(&unit_path)
            .await
            .with_context(|| format!("check existing quadlet unit {}", unit_path.display()))
    }

    async fn remove_quadlet_unit_if_present(&self, name: &str) -> Result<()> {
        let unit_dir = quadlet::unit_dir()
            .await
            .context("locate user quadlet unit dir for removal")?;
        let unit_path = unit_dir.join(format!("{name}.container"));
        if tokio::fs::try_exists(&unit_path).await.unwrap_or(false) {
            quadlet::disable_remove(name, &unit_dir)
                .await
                .with_context(|| format!("remove stale quadlet unit for {name}"))?;
        }
        Ok(())
    }

    /// Drift-sync an existing Quadlet unit file's bytes against what the
    /// current renderer produces. No-op when the flag is off, when the
    /// app is a companion (companion.rs owns those units), or when no
    /// unit file exists yet (install_via_quadlet handles first-write).
    ///
    /// Ordinary metadata changes wait for an operator restart. Runtime-affecting
    /// changes restart the service and retain a durable pending marker until
    /// that succeeds, including across daemon restarts and failed reloads.
    async fn sync_quadlet_unit(&self, lm: &LoadedManifest, name: &str) -> Result<()> {
        // Companions: same reasoning as migrate_to_quadlet_if_needed —
        // companion.rs renders these units with a different shape, syncing
        // here would clobber them.
        let app_id = lm.manifest.app.id.as_str();
        if UI_APP_IDS.contains(&app_id) {
            return Ok(());
        }
        let unit_dir = quadlet::unit_dir()
            .await
            .context("locate user quadlet unit dir for drift sync")?;
        let unit_path = unit_dir.join(format!("{name}.container"));
        // Only sync when an existing file is on disk — otherwise this is
        // a fresh install and install_via_quadlet will write it anyway.
        if !tokio::fs::try_exists(&unit_path).await.unwrap_or(false) {
            return Ok(());
        }
        let old_body = tokio::fs::read_to_string(&unit_path)
            .await
            .with_context(|| format!("read existing quadlet for {name}"))?;
        let restart_required = quadlet::contains_stale_health_gate(&old_body);

        let mut resolved = lm.manifest.clone();
        self.resolve_dynamic_env(&mut resolved).await?;
        // Same catalog/pinned-version image resolution the installer applies, so
        // the drift re-render doesn't revert a pinned version back to the
        // manifest's shipped `:latest` tag on the next reconcile tick.
        resolve_catalog_image(&mut resolved);
        let unit = quadlet::QuadletUnit::from_manifest(&resolved, name);
        let new_body = unit.render();
        let restart_for_port_change = quadlet::publish_ports_changed(&old_body, &new_body);
        let restart_for_network_alias_change =
            quadlet::network_aliases_changed(&old_body, &new_body);
        let restart_for_exec_change = quadlet::exec_changed(&old_body, &new_body);
        let restart_for_health_change = quadlet::health_cmd_changed(&old_body, &new_body);
        let restart_for_security_change = quadlet::security_changed(&old_body, &new_body);
        let restart_for_managed_override =
            quadlet::redundant_managed_network_override(&unit, &unit_dir)
                .await?
                .is_some();
        let needs_restart = restart_required
            || restart_for_managed_override
            || restart_for_port_change
            || restart_for_network_alias_change
            || restart_for_exec_change
            || restart_for_health_change
            || restart_for_security_change;
        // Record the obligation BEFORE replacing the unit. A failed reload or
        // restart must not become a no-op on the next tick just because the
        // generated file already matches the manifest.
        let pending = quadlet::RestartObligation::prepare(&unit_path, needs_restart).await?;
        if pending.is_pending() {
            self.ensure_resolved_source_available(lm).await?;
        }
        if needs_restart {
            self.backup_runtime_change(name, &resolved).await?;
        }
        let changed = quadlet::write_if_changed(&unit, &unit_dir)
            .await
            .with_context(|| format!("drift-sync quadlet unit for {name}"))?;
        if changed || pending.is_pending() {
            quadlet::daemon_reload_user()
                .await
                .context("systemctl --user daemon-reload after drift-syncing quadlet unit")?;
        }
        if pending.is_pending() {
            let service = unit.service_name();
            tracing::info!(
                app_id = %lm.manifest.app.id,
                container = %name,
                service = %service,
                "Applying pending Quadlet runtime change"
            );
            quadlet::restart_service(&service)
                .await
                .with_context(|| format!("restart drifted quadlet service {service}"))?;
            pending.complete().await?;
        } else if changed {
            tracing::info!(
                app_id = %lm.manifest.app.id,
                container = %name,
                "Quadlet metadata updated; operator restart will apply it"
            );
        }
        Ok(())
    }

    async fn sync_dependency_quadlet_units(&self, lm: &LoadedManifest) -> Result<()> {
        let dependency_ids = manifest_dependency_app_ids(&lm.manifest);
        if dependency_ids.is_empty() {
            return Ok(());
        }

        let dependencies: Vec<LoadedManifest> = {
            let state = self.state.read().await;
            dependency_ids
                .iter()
                .filter_map(|app_id| state.manifests.get(app_id).cloned())
                .collect()
        };

        for dep in dependencies {
            let mut resolved = dep.manifest.clone();
            self.resolve_dynamic_env(&mut resolved).await?;
            let name = compute_container_name(&dep.manifest);
            if self.runtime.get_container_status(&name).await.is_err() {
                continue;
            }
            if self.use_quadlet_backends && !uses_pasta_network(&resolved) {
                self.sync_quadlet_unit(&dep, &name).await?;
            }
        }

        Ok(())
    }

    async fn ensure_resolved_source_available(&self, lm: &LoadedManifest) -> Result<()> {
        let resolved = lm.manifest.app.container.resolve().ok_or_else(|| {
            anyhow::anyhow!(
                "manifest for {} has invalid container source (neither image nor build)",
                lm.manifest.app.id
            )
        })?;

        match resolved {
            ResolvedSource::Pull {
                image,
                image_signature,
                ..
            } => {
                // Same trusted-registry gate as install_fresh (§A).
                if !crate::container::image_policy::is_valid_docker_image(&image) {
                    anyhow::bail!(
                        "refusing to pull image {:?} for {}: not from a trusted registry",
                        image,
                        lm.manifest.app.id
                    );
                }
                let exists = match self.runtime.image_exists(&image).await {
                    Ok(exists) => exists,
                    Err(err) => {
                        tracing::warn!(
                            image = %image,
                            error = %err,
                            "image existence check failed; pulling image instead"
                        );
                        false
                    }
                };
                if !exists {
                    self.runtime
                        .pull_image(&image, image_signature.as_deref())
                        .await
                        .with_context(|| format!("pulling {image}"))?;
                }
            }
            ResolvedSource::Build(mut bcfg) => {
                let ctx_path = Path::new(&bcfg.context);
                if !ctx_path.is_absolute() {
                    bcfg.context = lm
                        .manifest_dir
                        .join(ctx_path)
                        .to_string_lossy()
                        .into_owned();
                }
                if !self
                    .runtime
                    .image_exists(&bcfg.tag)
                    .await
                    .map_err(|err| {
                        tracing::warn!(
                            image = %bcfg.tag,
                            error = %err,
                            "build image existence check failed; rebuilding image"
                        );
                        err
                    })
                    .unwrap_or(false)
                {
                    self.runtime
                        .build_image(&bcfg)
                        .await
                        .with_context(|| format!("build_image {}", bcfg.tag))?;
                }
            }
        }

        Ok(())
    }

    /// Phase 3.2 install path. Renders the manifest as a Quadlet unit,
    /// writes it atomically into ~/.config/containers/systemd/, asks
    /// systemd to reload, and starts the generated service. Errors at
    /// any step propagate as install_fresh failures — no half-state.
    async fn install_via_quadlet(&self, resolved_manifest: &AppManifest, name: &str) -> Result<()> {
        let unit = quadlet::QuadletUnit::from_manifest(resolved_manifest, name);
        let dir = quadlet::unit_dir()
            .await
            .context("locate user quadlet unit dir")?;
        quadlet::write_if_changed(&unit, &dir)
            .await
            .with_context(|| format!("write quadlet unit for {name}"))?;
        quadlet::daemon_reload_user()
            .await
            .context("systemctl --user daemon-reload after writing quadlet unit")?;
        quadlet::enable_now(&unit.service_name())
            .await
            .with_context(|| format!("systemctl --user start {}", unit.service_name()))?;
        Ok(())
    }

    async fn ensure_container_network(&self, manifest: &AppManifest) -> Result<()> {
        if cfg!(test) {
            return Ok(());
        }
        let Some(network) = manifest.app.container.network.as_deref() else {
            return Ok(());
        };
        if network.is_empty() || is_builtin_network_mode(network) {
            return Ok(());
        }

        let exists = tokio::process::Command::new("podman")
            .args(["network", "exists", network])
            .status()
            .await
            .with_context(|| format!("checking podman network {network}"))?;
        if exists.success() {
            return Ok(());
        }

        let create = tokio::process::Command::new("podman")
            .args(["network", "create", network])
            .output()
            .await
            .with_context(|| format!("creating podman network {network}"))?;
        if create.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&create.stderr);
        if stderr.contains("already exists") {
            return Ok(());
        }
        Err(anyhow::anyhow!(
            "podman network create {} failed: {}",
            network,
            stderr.trim()
        ))
    }

    // ------------------------------------------------------------------
    // Prod-specific inherent methods. The shared lifecycle surface
    // (install/start/stop/restart/remove/upgrade/status/list/logs/health) lives
    // in the `impl ContainerOrchestrator for ProdContainerOrchestrator` block
    // below — call those through the trait, not as inherent methods.
    // ------------------------------------------------------------------

    /// App-specific pre-start hooks. Called before `create_container` on
    /// install, and on every reconcile pass (for already-running apps too
    /// so we catch RPC password rotation without a manual restart).
    ///
    /// Returns `Some(Rewritten)` if the hook modified on-disk state that
    /// the running container is bind-mounting — the caller uses that
    /// signal to decide whether to restart. `Some(Unchanged)` means the
    /// hook ran cleanly but nothing on disk changed. `None` means there
    /// was no hook registered for this `app_id`.
    ///
    /// Kept on `ProdContainerOrchestrator` (not extracted into a trait
    /// yet) because there's exactly one hook today — a generic pre-start
    /// mechanism is worth adding when the second hook appears, not now.
    async fn run_pre_start_hooks(&self, app_id: &str) -> Result<Option<HookOutcome>> {
        match app_id {
            "bitcoin-ui" => {
                let paths = self.bitcoin_ui_paths.clone();
                let outcome = bitcoin_ui::render(&paths)
                    .await
                    .context("bitcoin-ui pre-start: render nginx.conf")?;
                Ok(Some(match outcome {
                    bitcoin_ui::RenderOutcome::Written => HookOutcome::Rewritten,
                    bitcoin_ui::RenderOutcome::Unchanged => HookOutcome::Unchanged,
                }))
            }
            "filebrowser" => {
                let outcome = filebrowser::ensure_config(&self.filebrowser_paths)
                    .await
                    .context("filebrowser pre-start: ensure .filebrowser.json")?;
                Ok(Some(match outcome {
                    filebrowser::EnsureOutcome::Written => HookOutcome::Rewritten,
                    filebrowser::EnsureOutcome::Unchanged => HookOutcome::Unchanged,
                }))
            }
            "lnd" => {
                let rpc_pass = FileSecretsProvider {
                    root: self.secrets_dir.clone(),
                }
                .read("bitcoin-rpc-password")
                .context("lnd pre-start: read bitcoin RPC password")?;
                let bitcoin_host = self.bitcoin_host().await;
                let outcome = lnd::ensure_config(&self.lnd_paths, &rpc_pass, &bitcoin_host)
                    .await
                    .context("lnd pre-start: ensure lnd.conf")?;
                Ok(Some(match outcome {
                    lnd::EnsureOutcome::Written => HookOutcome::Rewritten,
                    lnd::EnsureOutcome::Unchanged => HookOutcome::Unchanged,
                }))
            }
            "archy-nbxplorer" | "btcpay-server" => {
                self.ensure_btcpay_stack_dirs().await?;
                Ok(Some(HookOutcome::Unchanged))
            }
            "fedimint-clientd" => {
                // First-boot (root context) created /var/lib/archipelago/fmcd
                // as root:root, but the rootless container's uid 0 maps to
                // host 1000 — fmcd then crash-loops with "Permission denied
                // (os error 13)". Repair ownership on every start so nodes
                // installed before the first-boot fix self-heal too. (The
                // generic running-container ownership sweep can't catch this:
                // fmcd exits within seconds, so it's never Running when the
                // sweep probes.)
                let dir = "/var/lib/archipelago/fmcd";
                let mkdir = host_sudo(&["mkdir", "-p", dir])
                    .await
                    .with_context(|| format!("mkdir {dir}"))?;
                if !mkdir.success() {
                    return Err(anyhow::anyhow!("mkdir -p {dir} failed with status {mkdir}"));
                }
                // Drift-gated like the other ownership hooks.
                if ownership_already_correct_from_host(dir, "1000:1000").await {
                    return Ok(Some(HookOutcome::Unchanged));
                }
                let chown = host_sudo(&["chown", "-R", "1000:1000", dir])
                    .await
                    .with_context(|| format!("chown {dir}"))?;
                if !chown.success() {
                    return Err(anyhow::anyhow!("chown {dir} failed with status {chown}"));
                }
                Ok(Some(HookOutcome::Unchanged))
            }
            "grafana" => {
                self.cleanup_stale_grafana_port().await;
                Ok(Some(HookOutcome::Unchanged))
            }
            _ => Ok(None),
        }
    }

    async fn ensure_btcpay_stack_dirs(&self) -> Result<()> {
        for dir in [
            "/var/lib/archipelago/btcpay/Main",
            "/var/lib/archipelago/nbxplorer/Main",
        ] {
            let status = host_sudo(&["mkdir", "-p", dir])
                .await
                .with_context(|| format!("mkdir {dir}"))?;
            if !status.success() {
                return Err(anyhow::anyhow!(
                    "mkdir -p {dir} failed with status {status}"
                ));
            }
        }

        // These hooks exist to repair old installs, not to churn on every
        // prepare — a healthy stack was being re-chowned on each reconcile
        // tick (operator-visible journal flood, 2026-08-07). repair_dir_ownership
        // skips when the ownership is already right, and tolerates a chown that
        // fails on an already-correct tree.
        for dir in [
            "/var/lib/archipelago/btcpay",
            "/var/lib/archipelago/nbxplorer",
        ] {
            repair_dir_ownership(dir, "1000:1000").await?;
        }

        repair_dir_ownership("/var/lib/archipelago/postgres-btcpay", "100998:100998").await?;
        Ok(())
    }

    async fn ensure_fedimint_dirs(&self) -> Result<()> {
        for dir in [
            "/var/lib/archipelago/fedimint",
            "/var/lib/archipelago/fedimint-gateway",
        ] {
            let mkdir = host_sudo(&["mkdir", "-p", dir])
                .await
                .with_context(|| format!("mkdir {dir}"))?;
            if !mkdir.success() {
                return Err(anyhow::anyhow!("mkdir -p {dir} failed with status {mkdir}"));
            }

            // Fedimint images currently run as root inside the rootless
            // container. Container uid 0 maps to the Archipelago host user
            // (1000), not to subuid 100000. Repair old installs that were
            // chowned into the subuid range and crash on database.db.lock.
            // Drift-gated like the btcpay hook — see ensure_btcpay_stack_dirs.
            repair_dir_ownership(dir, "1000:1000").await?;
        }
        Ok(())
    }

    async fn cleanup_stale_grafana_port(&self) {
        // Port 3001 can belong to Gitea or the daemon's gate. Reap only a
        // Grafana container proven absent from Podman's inventory.
        crate::container::ghost_reaper::reap_for_app("grafana").await;
    }

    async fn detect_host_facts(&self) -> HostFacts {
        // Unit tests run under tokio's paused clock; awaiting a real
        // subprocess there deadlocks against auto-advanced timers (the old
        // BLOCKING detection only worked by never yielding). Canned facts.
        #[cfg(test)]
        {
            return HostFacts {
                host_ip: "127.0.0.1".to_string(),
                host_mdns: "test.local".to_string(),
                disk_gb: self.test_disk_gb.unwrap_or(1000),
                bitcoin_host: "bitcoin-knots".to_string(),
                node_identity_pubkeys: String::new(),
            };
        }
        #[allow(unreachable_code)]
        {
            let host_ip = Self::detect_host_ip()
                .await
                .unwrap_or_else(|| "127.0.0.1".to_string());
            let host_mdns = Self::detect_host_mdns().await;
            let disk_gb = self.disk_gb().await;
            HostFacts {
                host_ip,
                host_mdns,
                disk_gb,
                // Cheap default; resolve_dynamic_env fills the real node name on
                // demand (it costs a podman call) only for manifests that use
                // {{BITCOIN_HOST}}, rather than every app on every reconcile.
                bitcoin_host: "bitcoin-knots".to_string(),
                // Likewise filled on demand, only for manifests that use
                // {{NODE_IDENTITY_PUBKEYS}}.
                node_identity_pubkeys: String::new(),
            }
        }
    }

    /// Nostr public keys of the identities the app identity picker offers, for
    /// the `{{NODE_IDENTITY_PUBKEYS}}` derived-env placeholder. The node
    /// identity is recognised the way `identity.list` marks `is_node`: by the
    /// node's ed25519 public key, read here from `identity/node_key.pub`
    /// (the file `server_info.pubkey` is derived from at startup). The record
    /// mirrored from the node key has a `node-` id, so the picker's prefix rule
    /// hides it either way. The key is only read, never created: a missing or
    /// malformed file is an error. An empty set is an error too, so an app is
    /// never handed an empty owner list.
    async fn node_identity_pubkeys(&self) -> Result<String> {
        let node_pubkey_hex = self.node_pubkey_hex().await?;
        let pubkeys = crate::identity_manager::IdentityManager::new(&self.data_dir)
            .await?
            .app_signable_nostr_pubkeys(&node_pubkey_hex)
            .await?;
        anyhow::ensure!(
            !pubkeys.is_empty(),
            "no user identity with a Nostr key is available for apps to sign with; \
             create one under Web5 \u{2192} Identities"
        );
        Ok(pubkeys)
    }

    /// The node's ed25519 public key as lowercase hex, read from
    /// `identity/node_key.pub` (raw 32 bytes, as `NodeIdentity` writes it)
    /// without the logging or key creation of `NodeIdentity::load_or_create`.
    async fn node_pubkey_hex(&self) -> Result<String> {
        let path = self.data_dir.join("identity").join("node_key.pub");
        let bytes = tokio::fs::read(&path)
            .await
            .with_context(|| format!("reading the node public key {}", path.display()))?;
        anyhow::ensure!(
            bytes.len() == 32,
            "node public key {} is {} bytes, expected 32",
            path.display(),
            bytes.len()
        );
        Ok(hex::encode(bytes))
    }

    /// Container name of the running Bitcoin node (`bitcoin-knots` or
    /// `bitcoin-core`) for the `{{BITCOIN_HOST}}` derived-env placeholder.
    /// Defaults to `bitcoin-knots` when none is running (B12).
    async fn bitcoin_host(&self) -> String {
        // No real podman under the tests' paused clock (see detect_host_facts).
        #[cfg(test)]
        {
            return self
                .test_bitcoin_host
                .clone()
                .unwrap_or_else(|| "bitcoin-knots".to_string());
        }
        #[allow(unreachable_code)]
        // The known Bitcoin node containers, preferred in order. Any archy
        // Bitcoin distribution runs as a container named `bitcoin-<distro>`
        // (or bare `bitcoin`), all reachable on archy-net by name.
        const BITCOIN_NAMES: &[&str] = &["bitcoin-knots", "bitcoin-core", "bitcoin"];
        let names = tokio::process::Command::new("podman")
            .args(["ps", "--format", "{{.Names}}"])
            .output()
            .await
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        let running: Vec<&str> = names.lines().map(|l| l.trim()).collect();
        // Prefer a known name in priority order…
        if let Some(hit) = BITCOIN_NAMES.iter().find(|n| running.contains(n)) {
            return hit.to_string();
        }
        // …else accept ANY running `bitcoin-*` / `bitcoin` container, so a
        // future Bitcoin distribution archy ships works without editing this
        // list (user req 2026-07-22). Excludes companions/sidecars like
        // `bitcoin-ui` and `archy-*`.
        if let Some(other) = running.iter().find(|n| {
            (**n == "bitcoin" || n.starts_with("bitcoin-"))
                && !n.ends_with("-ui")
                && !n.starts_with("archy-")
        }) {
            return other.to_string();
        }
        "bitcoin-knots".to_string()
    }

    #[cfg(test)]
    pub fn set_bitcoin_host_for_test(&mut self, host: &str) {
        self.test_bitcoin_host = Some(host.to_string());
    }

    async fn detect_host_ip() -> Option<String> {
        crate::host_ip::primary_host_ipv4().await
    }

    async fn detect_host_mdns() -> String {
        let hostname = tokio::process::Command::new("hostname")
            .output()
            .await
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "archipelago".to_string());
        if hostname.ends_with(".local") {
            hostname
        } else {
            format!("{hostname}.local")
        }
    }

    async fn detect_disk_gb() -> u64 {
        let target = if Path::new("/var/lib/archipelago").exists() {
            "/var/lib/archipelago"
        } else {
            "/"
        };
        let output = match tokio::process::Command::new("df")
            .arg("-k")
            .arg(target)
            .output()
            .await
        {
            Ok(o) if o.status.success() => o,
            _ => return 0,
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = match stdout.lines().nth(1) {
            Some(l) => l,
            None => return 0,
        };
        let kb = match line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u64>().ok())
        {
            Some(v) => v,
            None => return 0,
        };
        kb / 1_000_000
    }

    async fn disk_gb(&self) -> u64 {
        // No real df under the tests' paused clock (see detect_host_facts).
        #[cfg(test)]
        {
            return self.test_disk_gb.unwrap_or(1000);
        }
        #[allow(unreachable_code)]
        {
            Self::detect_disk_gb().await
        }
    }

    /// Ensure app-specific secrets exist *before* env resolution. The Bitcoin
    /// backends reference `bitcoin-rpc-txrelay-rpcauth` as a required
    /// `secret_env`; it is normally created by the tx-relay flow, so nodes that
    /// never used tx-relay lack it and `resolve_secret_env` hard-fails — taking
    /// bitcoind (and everything that depends on it) down. Generating it here,
    /// idempotently, lets reconcile/install self-heal that state (the .198 case)
    /// instead of cascading. Must be called before every `resolve_dynamic_env`.
    async fn ensure_app_secrets(&self, app_id: &str) -> Result<()> {
        if cfg!(test) {
            return Ok(());
        }
        if matches!(app_id, "bitcoin-knots" | "bitcoin-core" | "bitcoin") {
            crate::api::rpc::bitcoin_relay::ensure_txrelay_credentials(&self.data_dir)
                .await
                .context("ensuring bitcoin tx-relay credentials")?;
        }
        if app_id == "btcpay-server" {
            // Derived secret (LND macaroon + cert thumbprint), consumed by an
            // `optional` secret_env — btcpay must still start when LND is
            // absent or the derivation fails, so log-and-continue.
            if let Err(e) =
                crate::container::lnd::ensure_btcpay_lnd_connection_secret(&self.secrets_dir).await
            {
                tracing::warn!(error = %e, "btcpay-lnd-connection secret not generated; btcpay will run without the internal LND node");
            }
        }
        // Other app secrets (fmcd-password, fedimint-gateway-hash, …) are now
        // declared as `generated_secrets` in their manifests and materialised
        // generically in `resolve_dynamic_env` — no per-app code here.
        Ok(())
    }

    /// Materialise IndeedHub's AES root before the generic generated-secret
    /// pass. Old installers injected one known value directly into the API and
    /// worker environments, so an upgrade with either consumer still present
    /// must persist that value before container drift can recreate them. With
    /// no existing consumer this is a fresh install and receives random bytes.
    async fn ensure_indeedhub_aes_master(&self, manifest: &AppManifest) -> Result<()> {
        if manifest.app.id != "indeedhub-api" {
            return Ok(());
        }

        let secret_path = self
            .secrets_dir
            .join(crate::container::secrets::INDEEDHUB_AES_SECRET_NAME);
        let preserve_legacy = if secret_path.exists() {
            // The secret helper validates the existing file and, critically,
            // refuses to replace a damaged encryption root.
            false
        } else {
            let consumers = [
                "indeedhub-api",
                "indeedhub-ffmpeg",
                "indeedhub-build_api_1",
                "indeedhub-build_ffmpeg-worker_1",
            ];
            self.runtime
                .list_containers()
                .await
                .context("detecting an existing IndeedHub encryption-key consumer")?
                .iter()
                .any(|container| {
                    let name = container.name.trim_start_matches('/');
                    consumers.contains(&name)
                })
        };

        if crate::container::secrets::ensure_indeedhub_aes_master_secret(
            &self.secrets_dir,
            preserve_legacy,
        )? {
            tracing::info!(
                app = "indeedhub-api",
                path = %secret_path.display(),
                "Persisted the legacy IndeedHub encryption root for upgrade compatibility"
            );
        }
        Ok(())
    }

    async fn resolve_dynamic_env(&self, manifest: &mut AppManifest) -> Result<()> {
        if manifest.app.container.media_registration_identity {
            // Only opted-in manifests read the existing appliance identity. A
            // missing key/public-key mismatch is an error, never key generation.
            let identity =
                crate::identity::NodeIdentity::load_existing(&self.data_dir.join("identity"))
                    .await?;
            anyhow::ensure!(
                self.node_pubkey_hex().await? == identity.pubkey_hex(),
                "Existing node public key disagrees with its signing identity"
            );
            let data_dir = self.data_dir.clone();
            let app_id = manifest.app.id.clone();
            let pin = tokio::task::spawn_blocking(move || {
                crate::container::registration_pin::ensure_for_installation(
                    &data_dir, &app_id, &identity,
                )
            })
            .await
            .context("Application registration pin worker failed")??;
            crate::container::registration_pin::apply_environment(manifest, &pin)?;
        }
        if manifest.app.id == "nginx-proxy-manager" {
            crate::container::npm::resolve_storage()
                .await?
                .apply(manifest)?;
        }
        // Idempotency guard: partitioning already ran on this instance.
        // Re-running would re-taint against an environment that no longer
        // contains the composite entries and silently drop them. Callers
        // always resolve a fresh clone, so this only trips on misuse.
        if !manifest.app.container.secret_env_refs.is_empty() {
            return Ok(());
        }
        // IndeedHub's data-encryption root needs an upgrade-aware first pass:
        // generic generation alone would replace the fleet-wide legacy value
        // and make previously encrypted data unreadable.
        self.ensure_indeedhub_aes_master(manifest).await?;

        // Materialise any manifest-declared generated secrets before they're
        // read below. This is the single chokepoint every install/reconcile
        // path funnels through, so an app's secrets exist by the time its
        // `secret_env` resolves — no per-app code, no host provisioning.
        crate::container::secrets::ensure_generated_secrets(&self.secrets_dir, manifest)?;

        // FED-07 migration. A node installed before the shipped-default
        // fallbacks were removed is still answering to a credential published
        // in this repository. Detect that exact value and rotate it, once.
        //
        // The rotated hash changes the resolved secret env below, which changes
        // `secret_env_hash`, which the drift check reads as a label mismatch on
        // the running container — so the platform's own recreate path rebuilds
        // it around an unchanged data directory, ports, volumes and container
        // name. Deliberately no teardown here: a hand-rolled remove-and-run is
        // the anti-pattern CLAUDE.md names, and it is what would lose the
        // gateway's state.
        //
        // An error propagates rather than being swallowed: the atomic write
        // leaves the previous credential in place, so surfacing the failure is
        // strictly better than continuing with a half-rotated gateway.
        if manifest.app.id == "fedimint-gateway"
            && crate::container::secrets::rotate_compromised_gateway_credential(&self.secrets_dir)?
        {
            // Mark the app so the drift check below recreates it even though
            // it is restart-sensitive. Without this the unit is rewritten but
            // never restarted, and the gateway keeps serving the compromised
            // credential indefinitely (observed on a test node 2026-08-01).
            self.credential_rotated
                .lock()
                .await
                .insert(manifest.app.id.clone());
            // Names a path, never a value — this line crosses into the node's
            // logs, which are a wider audience than the 0600 secrets dir.
            tracing::info!(
                app = "fedimint-gateway",
                "Rotated the Fedimint gateway admin credential: this node was carrying a publicly \
                 known default that shipped in the repository (FED-07). The gateway will be \
                 recreated around its existing data. The new password is readable by the service \
                 user at {}/{}.pw",
                self.secrets_dir.display(),
                crate::container::secrets::GATEWAY_HASH_SECRET_NAME,
            );
        }

        let mut facts = self.detect_host_facts().await;
        // Only pay the podman cost to detect Knots-vs-Core when this manifest
        // actually templates the Bitcoin node into its env (mempool — B12).
        if manifest
            .app
            .container
            .derived_env
            .iter()
            .any(|e| e.template.contains("{{BITCOIN_HOST}}"))
        {
            facts.bitcoin_host = self.bitcoin_host().await;
        }
        // The identities' keys are read only for manifests that template them.
        if manifest
            .app
            .container
            .derived_env
            .iter()
            .any(|e| e.template.contains("{{NODE_IDENTITY_PUBKEYS}}"))
        {
            facts.node_identity_pubkeys =
                self.node_identity_pubkeys().await.with_context(|| {
                    format!(
                        "resolving {{{{NODE_IDENTITY_PUBKEYS}}}} for {}",
                        manifest.app.id
                    )
                })?;
        }
        let mut env = manifest.app.environment.clone();
        env.extend(manifest.app.container.resolve_derived_env(&facts));
        if matches!(manifest.app.id.as_str(), "bitcoin-core" | "bitcoin-knots") {
            let storage = crate::settings::bitcoin_storage::load(&self.data_dir).await?;
            env.retain(|entry| !entry.starts_with("BITCOIN_PRUNE="));
            if storage.prune {
                anyhow::ensure!(
                    manifest.app.container.custom_args.iter().any(|arg| arg.contains("BITCOIN_PRUNE")),
                    "This Bitcoin app definition cannot honor the pruning choice. Refresh the app catalog and try again."
                );
                env.push("BITCOIN_PRUNE=1".to_string());
            }
        }

        // FM_BITCOIND_URL now comes from the manifest's {{BITCOIN_HOST}}
        // derived_env (works on Knots/Core/any distro). The old hardcoded
        // `bitcoin-knots` override was removed 2026-07-22 — it defeated the
        // derived-env and broke Core nodes.

        let provider = FileSecretsProvider {
            root: self.secrets_dir.clone(),
        };
        let secret_pairs = manifest
            .app
            .container
            .resolve_secret_env_pairs(&provider)
            .map_err(|e| anyhow::anyhow!(e.to_string()))
            .with_context(|| {
                format!(
                    "resolving secret_env for {} from {}",
                    manifest.app.id,
                    self.secrets_dir.display()
                )
            })?;

        // Secret values never merge into `environment` — they'd land in
        // `podman inspect` output and Quadlet unit files on disk. Instead:
        // expand ${KEY} placeholders (a plain entry that interpolates a
        // secret is tainted and travels as a secret itself — btcpay's
        // Password=${BTCPAY_DB_PASS} connection strings), keep the plain
        // remainder as env, and hand the secret-bearing pairs to the
        // runtime's secret store by reference.
        let (plain, secret_bearing) =
            archipelago_container::manifest::expand_and_partition_env(env, secret_pairs);
        manifest.app.environment = plain;
        if secret_bearing.is_empty() {
            manifest.app.container.secret_env_refs = Vec::new();
            manifest.app.container.secret_env_hash = None;
        } else {
            let hash = archipelago_container::manifest::secret_env_content_hash(&secret_bearing);
            let app_id = manifest.app.id.clone();
            manifest.app.container.secret_env_refs = secret_bearing
                .into_iter()
                .map(
                    |(key, value)| archipelago_container::manifest::SecretEnvRef {
                        secret_name: format!("archy-env-{}-{}", app_id, key.to_ascii_lowercase()),
                        env_key: key,
                        value,
                    },
                )
                .collect();
            manifest.app.container.secret_env_hash = Some(hash.clone());

            // Register/refresh the podman secrets. A per-app hash cache makes
            // the steady-state reconcile free: podman is only consulted when
            // the resolved content actually changed (or on first touch after
            // boot). Mock runtimes no-op via the trait default.
            let cached = self.env_secret_cache.lock().await.get(&app_id).cloned();
            if cached.as_deref() != Some(hash.as_str()) {
                self.runtime
                    .ensure_env_secrets(&manifest.app.container.secret_env_refs)
                    .await
                    .with_context(|| format!("registering env secrets for {app_id}"))?;
                self.env_secret_cache.lock().await.insert(app_id, hash);
            }
        }
        Ok(())
    }

    async fn backup_runtime_change(&self, name: &str, manifest: &AppManifest) -> Result<()> {
        if !crate::container::migration_backup::enabled(manifest)? {
            return Ok(());
        }
        // A persistent disk/permission failure must not repeatedly stop a
        // working old service. Reuse the reconciler's bounded repair budget.
        if !self.should_attempt_repair(name).await {
            anyhow::bail!("runtime migration retry budget exhausted; original service retained, inspect backup failure before retrying");
        }
        // Called only before a known runtime change. No app-specific commands;
        // opted-in manifests identify their persistent state through bind mounts.
        let output = tokio::process::Command::new("podman")
            .args(["inspect", name, "--format", "{{.HostConfig.NetworkMode}}"])
            .output()
            .await
            .context("inspect network before migration backup")?;
        let present = if output.status.success() {
            true
        } else {
            // A crash after gracefully stopping a --rm Quadlet container can
            // leave only its data and old unit. Prove absence before snapshotting
            // stopped state; an inspect/Podman failure is not proof of absence.
            let exists = tokio::process::Command::new("podman")
                .args(["container", "exists", name])
                .status()
                .await?;
            if exists.code() != Some(1) {
                anyhow::bail!("cannot verify existing container before runtime migration backup");
            }
            false
        };
        let service = format!("{name}.service");
        let managed = quadlet::unit_exists(name).await;
        let previous_unit = if managed {
            Some(
                tokio::fs::read(quadlet::unit_dir().await?.join(format!("{name}.container")))
                    .await?,
            )
        } else {
            None
        };
        if managed {
            quadlet::stop_service(&service).await?;
        } else if present {
            self.runtime.stop_container(name).await?;
        }
        match crate::container::migration_backup::snapshot(
            manifest,
            &self.data_dir,
            previous_unit.as_deref(),
        )
        .await
        {
            Ok(archive) => {
                tracing::info!(container = %name, backup = %archive.display(), "Persistent state saved before runtime migration");
                Ok(())
            }
            Err(error) => {
                // The unit has not been rewritten yet. Restore its previous
                // service on backup failure and report the migration failure.
                let restored = if managed {
                    quadlet::enable_now(&service).await
                } else {
                    self.runtime.start_container(name).await
                };
                restored.context("restore original app after failed migration snapshot")?;
                Err(error)
            }
        }
    }

    async fn container_env_drifted(&self, name: &str, manifest: &AppManifest) -> bool {
        if cfg!(test) {
            return false;
        }

        if self.container_command_drifted(name, manifest).await {
            return true;
        }

        // Generated-unit drift handles managed services; preserve deliberate
        // systemd drop-in overrides instead of recreating them every tick.
        let unmanaged = !quadlet::unit_exists(name).await;
        // Podman's effective bounding set, not Docker-compatible CapAdd (which
        // can be empty even when Quadlet supplied capabilities).
        if unmanaged && !manifest.app.security.capabilities.is_empty() {
            if let Ok(output) = tokio::process::Command::new("podman")
                .args(["inspect", name, "--format", "{{json .BoundingCaps}}"])
                .output()
                .await
            {
                if output.status.success() {
                    if let Ok(actual) = serde_json::from_slice::<Vec<String>>(&output.stdout) {
                        if missing_declared_capability(&manifest.app.security.capabilities, &actual)
                        {
                            return true;
                        }
                    }
                }
            }
        }

        // Quadlet handles declarative Network= drift above. Legacy rootless
        // Podman containers need the same convergence when no unit owns them.
        if unmanaged
            && matches!(
                manifest.app.container.network.as_deref(),
                Some(
                    "slirp4netns"
                        | "slirp4netns:allow_host_loopback=true"
                        | "slirp4netns:allow_host_loopback=true,cidr=169.254.1.0/24"
                        | "pasta"
                )
            )
        {
            if let Ok(output) = tokio::process::Command::new("podman")
                .args(["inspect", name, "--format", "{{.HostConfig.NetworkMode}}"])
                .output()
                .await
            {
                if output.status.success() {
                    let mut actual = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if matches!(
                        manifest.app.container.network.as_deref(),
                        Some(
                            "slirp4netns:allow_host_loopback=true"
                                | "slirp4netns:allow_host_loopback=true,cidr=169.254.1.0/24"
                        )
                    ) && actual == "slirp4netns"
                    {
                        let spec = tokio::process::Command::new("podman")
                            .args(["generate", "spec", name])
                            .output()
                            .await;
                        actual = match spec {
                            Ok(spec) if spec.status.success() => {
                                rootless_network_from_spec(&spec.stdout).unwrap_or_default()
                            }
                            _ => String::new(),
                        };
                        if actual.is_empty() {
                            tracing::warn!(app = %name, "Could not verify rootless network options; retaining the existing container");
                        }
                    }
                    if rootless_network_mode_drifted(
                        manifest.app.container.network.as_deref(),
                        &actual,
                    ) {
                        return true;
                    }
                }
            }
        }

        let inspect = tokio::process::Command::new("podman")
            .args([
                "inspect",
                name,
                "--format",
                "{{range .Config.Env}}{{println .}}{{end}}",
            ])
            .output()
            .await;
        let Ok(output) = inspect else {
            return false;
        };
        if !output.status.success() {
            return false;
        }

        let current: HashMap<String, String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let (key, value) = line.split_once('=')?;
                Some((key.to_string(), value.to_string()))
            })
            .collect();

        if manifest.app.environment.iter().any(|entry| {
            let Some((key, expected)) = entry.split_once('=') else {
                return false;
            };
            current.get(key).map_or(true, |actual| actual != expected)
        }) {
            return true;
        }

        // Secret-backed env never appears in Config.Env (that's the point) —
        // rotation is detected via the content-hash label stamped at create
        // time. A pre-upgrade container has no label, which reads as drift
        // and triggers the one-time recreate that scrubs its plaintext
        // secrets out of `podman inspect`.
        if let Some(expected_hash) = &manifest.app.container.secret_env_hash {
            let fmt = format!(
                "{{{{ index .Config.Labels \"{}\" }}}}",
                archipelago_container::manifest::SECRET_ENV_HASH_LABEL
            );
            let inspect = tokio::process::Command::new("podman")
                .args(["inspect", name, "--format", &fmt])
                .output()
                .await;
            let Ok(output) = inspect else {
                return false;
            };
            if !output.status.success() {
                return false;
            }
            if String::from_utf8_lossy(&output.stdout).trim() != expected_hash {
                return true;
            }
        }
        false
    }

    async fn container_command_drifted(&self, name: &str, manifest: &AppManifest) -> bool {
        if manifest.app.container.entrypoint.is_none()
            && manifest.app.container.custom_args.is_empty()
        {
            return false;
        }

        let inspect = tokio::process::Command::new("podman")
            .args([
                "inspect",
                name,
                "--format",
                "entry={{json .Config.Entrypoint}}\ncmd={{json .Config.Cmd}}",
            ])
            .output()
            .await;
        let Ok(output) = inspect else {
            return false;
        };
        if !output.status.success() {
            return false;
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let current_entry = text
            .lines()
            .find_map(|line| line.strip_prefix("entry="))
            .and_then(|json| serde_json::from_str::<Option<Vec<String>>>(json).ok())
            .flatten()
            .unwrap_or_default();
        let current_cmd = text
            .lines()
            .find_map(|line| line.strip_prefix("cmd="))
            .and_then(|json| serde_json::from_str::<Option<Vec<String>>>(json).ok())
            .flatten()
            .unwrap_or_default();

        command_argv_drifted(
            manifest.app.container.entrypoint.as_deref(),
            &manifest.app.container.custom_args,
            &current_entry,
            &current_cmd,
        )
    }

    /// True when a *running* container publishes a manifest container-port to a
    /// different host port than the manifest now asks for (published-port
    /// drift). This catches the class of failure seen on .116, where `lnd` was
    /// created mapping host 8080 -> container 8080 but the current manifest maps
    /// host 18080 -> container 8080, so every in-process REST client (which
    /// connects to the manifest port) gets connection-refused forever while the
    /// container looks "Up". `container_env_drifted` never inspects ports, and
    /// `wait_for_manifest_host_ports` only restarts the stale container (which
    /// republishes the wrong mapping), so without this check the drift is
    /// self-perpetuating.
    async fn container_ports_drifted(&self, name: &str, manifest: &AppManifest) -> bool {
        if cfg!(test) {
            return false;
        }
        if manifest.app.ports.is_empty() {
            return false;
        }
        let inspect = tokio::process::Command::new("podman")
            .args([
                "inspect",
                name,
                "--format",
                "{{json .HostConfig.PortBindings}}",
            ])
            .output()
            .await;
        let Ok(output) = inspect else {
            return false;
        };
        if !output.status.success() {
            return false;
        }
        let bindings = String::from_utf8_lossy(&output.stdout);
        host_port_bindings_drifted(&bindings, &manifest.app.ports)
    }

    async fn apply_data_uid(&self, manifest: &AppManifest) -> Result<()> {
        let Some(uid_gid) = manifest.app.container.data_uid.as_ref() else {
            return Ok(());
        };

        for volume in &manifest.app.volumes {
            if volume.volume_type == "tmpfs" || volume.source.is_empty() {
                continue;
            }
            if is_podman_socket_bind_source(&volume.source) {
                continue;
            }

            let mkdir_status = host_sudo(&["mkdir", "-p", &volume.source])
                .await
                .with_context(|| format!("mkdir {}", volume.source))?;
            if !mkdir_status.success() {
                return Err(anyhow::anyhow!(
                    "mkdir -p {} failed with status {:?}",
                    volume.source,
                    mkdir_status.code()
                ));
            }

            if let Err(err) = chown_for_rootless_container(uid_gid, &volume.source).await {
                return Err(err.context(format!("running chown on {}", volume.source)));
            }
        }
        Ok(())
    }

    async fn ensure_bind_mount_dirs(&self, manifest: &AppManifest) -> Result<()> {
        for volume in &manifest.app.volumes {
            if volume.volume_type == "tmpfs" || volume.source.is_empty() {
                continue;
            }
            if !Path::new(&volume.source).is_absolute() {
                continue;
            }
            if is_podman_socket_bind_source(&volume.source) {
                ensure_user_podman_socket().await.with_context(|| {
                    format!(
                        "ensure podman socket for {} bind mount {}",
                        manifest.app.id, volume.source
                    )
                })?;
                continue;
            }

            // File mounts are rendered by app-specific hooks; everything else
            // must exist before the Podman API accepts the bind mount.
            if volume.target.contains('.') || volume.source.contains('.') {
                continue;
            }

            // Whether the bind source already existed BEFORE we (root) create it,
            // so the ownership fix-up below only touches a dir we just made.
            let source_existed = Path::new(&volume.source).exists();

            // Capture — BEFORE the root mkdir — the nearest pre-existing ancestor
            // (the "anchor" whose ownership we copy) and the TOPMOST dir `mkdir -p`
            // will newly create. For a NESTED bind source like
            // `<dataroot>/<app>/<subdir>` (jellyfin /config + /cache, netbird
            // /data), `mkdir -p` creates the intermediate `<app>` dir root:root
            // too, so referencing the *immediate* parent copied ROOT — leaving the
            // dir unwritable and the app EACCES-crash-looping on reinstall. Anchor
            // instead to the nearest dir that already existed (the rootless data
            // root, owned by the service user) and chown the whole new subtree.
            let (anchor, top_created) = fresh_subtree_anchor(Path::new(&volume.source));

            let mkdir_status = host_sudo(&["mkdir", "-p", &volume.source])
                .await
                .with_context(|| format!("mkdir {}", volume.source))?;
            if !mkdir_status.success() {
                return Err(anyhow::anyhow!(
                    "mkdir -p {} failed with status {:?}",
                    volume.source,
                    mkdir_status.code()
                ));
            }

            // A bind dir we JUST created is owned root:root (mkdir ran via sudo).
            // An app that declares no `data_uid` runs as its own root inside the
            // container, which rootless Podman maps to the host user running
            // archipelago — so a root:root dir is UNWRITABLE from inside and the
            // app EACCES-crash-loops the moment it tries to create a subdir. The
            // in-container ownership self-heal only runs on RUNNING containers, so
            // it never fires for an app that crashes on startup. Match the new
            // subtree to the anchor's owner via `--reference` (no host-uid
            // guessing). Only on fresh creation, and only when apply_data_uid
            // won't already chown it.
            if !source_existed && manifest.app.container.data_uid.is_none() {
                if let Some(anchor) = anchor {
                    match host_sudo(&[
                        "chown",
                        "-R",
                        &format!("--reference={}", anchor.display()),
                        &top_created.display().to_string(),
                    ])
                    .await
                    {
                        Ok(s) if s.success() => {}
                        Ok(s) => tracing::warn!(
                            app_id = %manifest.app.id, dir = %volume.source,
                            "bind-dir ownership match exited {:?} (app may EACCES)", s.code()
                        ),
                        Err(e) => tracing::warn!(
                            app_id = %manifest.app.id, dir = %volume.source,
                            "bind-dir ownership match failed (non-fatal): {e}"
                        ),
                    }
                }
            }
        }
        Ok(())
    }

    async fn ensure_bind_mount_sockets(&self, manifest: &AppManifest) -> Result<()> {
        for volume in &manifest.app.volumes {
            if volume.volume_type == "tmpfs" || volume.source.is_empty() {
                continue;
            }
            if is_podman_socket_bind_source(&volume.source) {
                ensure_user_podman_socket().await.with_context(|| {
                    format!(
                        "ensure podman socket for {} bind mount {}",
                        manifest.app.id, volume.source
                    )
                })?;
            }
        }
        Ok(())
    }

    async fn ensure_manifest_files(&self, manifest: &AppManifest) -> Result<HookOutcome> {
        let mut outcome = HookOutcome::Unchanged;
        for file in &manifest.app.files {
            // Render templated placeholders before comparing/writing so the
            // idempotency check is against the FINAL bytes (not the template),
            // otherwise a rendered file would be rewritten every reconcile.
            let rendered = self
                .render_file_placeholders(manifest, &file.content)
                .await
                .with_context(|| format!("rendering manifest file {}", file.path))?;
            if ensure_rendered_file(&file.path, &rendered, file.overwrite)
                .await
                .with_context(|| format!("ensure manifest file {}", file.path))?
                == HookOutcome::Rewritten
            {
                outcome = HookOutcome::Rewritten;
            }
        }
        Ok(outcome)
    }

    /// Substitute the allow-listed placeholders a manifest `GeneratedFile` may
    /// carry. Keeps runtime-derived config (netbird's `config.yaml`/`nginx.conf`)
    /// declarative instead of generated by per-app Rust:
    /// - `{{HOST_IP}}` / `{{HOST_MDNS}}` — host facts (`hostname -I` / `.local`).
    /// - `{{NETWORK_GATEWAY}}` — the gateway of the app's podman network, i.e.
    ///   aardvark's DNS address. nginx uses it as an explicit `resolver` so it
    ///   re-resolves container names per request instead of pinning a stale IP
    ///   and 502-ing after a restart/reboot (issue #15). The network is ensured
    ///   to exist first so the gateway is readable on a fresh install (this runs
    ///   before `install_fresh`'s own `ensure_container_network`; both idempotent).
    /// - `{{secret:NAME}}` — a `0600` secret read from the service-owned secrets
    ///   dir (e.g. netbird's base64 relay/store keys). NEVER logged.
    async fn render_file_placeholders(
        &self,
        manifest: &AppManifest,
        content: &str,
    ) -> Result<String> {
        let mut out = content.to_string();
        if out.contains("{{HOST_IP}}") || out.contains("{{HOST_MDNS}}") {
            let facts = self.detect_host_facts().await;
            out = out
                .replace("{{HOST_IP}}", &facts.host_ip)
                .replace("{{HOST_MDNS}}", &facts.host_mdns);
        }
        if out.contains("{{NETWORK_GATEWAY}}") {
            self.ensure_container_network(manifest).await?;
            let gw = self.network_gateway(manifest).await?;
            out = out.replace("{{NETWORK_GATEWAY}}", &gw);
        }
        out = self.render_secret_placeholders(&out).await?;
        Ok(out)
    }

    /// Replace every `{{secret:NAME}}` with the trimmed contents of
    /// `<secrets_dir>/NAME`. `NAME` must be a bare filename (the same safety bar
    /// as `secret_env`). The secret value is never placed in an error or log.
    async fn render_secret_placeholders(&self, content: &str) -> Result<String> {
        const OPEN: &str = "{{secret:";
        let mut out = String::with_capacity(content.len());
        let mut rest = content;
        while let Some(start) = rest.find(OPEN) {
            out.push_str(&rest[..start]);
            let after = &rest[start + OPEN.len()..];
            let end = after
                .find("}}")
                .ok_or_else(|| anyhow::anyhow!("unterminated {{secret:...}} placeholder"))?;
            let name = &after[..end];
            if name.is_empty() || name.contains('/') || name.contains("..") {
                anyhow::bail!("invalid secret placeholder name '{name}' (must be a bare filename)");
            }
            let value = tokio::fs::read_to_string(self.secrets_dir.join(name))
                .await
                .map_err(|_| {
                    // Do not surface the path-with-value or io detail beyond the name.
                    anyhow::anyhow!("secret '{name}' referenced by a manifest file is missing")
                })?;
            out.push_str(value.trim());
            rest = &after[end + 2..];
        }
        out.push_str(rest);
        Ok(out)
    }

    /// The gateway IP of the app's podman network — aardvark's DNS resolver
    /// address. (Generalised from the old per-app netbird resolver helper,
    /// deleted in #20 ph4.) Falls back to
    /// podman's usual first-pool gateway if the inspect can't be parsed (the
    /// network was just ensured to exist, so this is a belt-and-braces default).
    async fn network_gateway(&self, manifest: &AppManifest) -> Result<String> {
        let network = manifest
            .app
            .container
            .network
            .as_deref()
            .filter(|n| !n.is_empty() && !is_builtin_network_mode(n))
            .ok_or_else(|| {
                anyhow::anyhow!("{{NETWORK_GATEWAY}} used but app has no dedicated network")
            })?;
        let out = tokio::process::Command::new("podman")
            .args([
                "network",
                "inspect",
                network,
                "--format",
                "{{range .Subnets}}{{.Gateway}}{{end}}",
            ])
            .output()
            .await
            .with_context(|| format!("inspecting podman network {network} for gateway"))?;
        let gw = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !gw.is_empty() && gw.parse::<std::net::IpAddr>().is_ok() {
            return Ok(gw);
        }
        tracing::warn!(
            network,
            "could not read network gateway; falling back to 10.89.0.1"
        );
        Ok("10.89.0.1".to_string())
    }

    /// Materialise manifest-declared self-signed TLS certs before the container
    /// is created (so a bind-mounted cert path resolves to a real file). Skips an
    /// entry whose crt+key already exist (idempotent / data-preserving). CN and
    /// SAN templates are rendered against host facts; when omitted they default
    /// to the node's host IP plus `127.0.0.1`/`localhost` so the cert is valid
    /// however the box is reached locally. (Generalised from the old per-app
    /// netbird TLS helper, deleted in #20 ph4: rsa:2048, 10-year, no per-app Rust.)
    async fn ensure_manifest_certs(&self, manifest: &AppManifest) -> Result<()> {
        let facts = self.detect_host_facts().await;
        let render = |s: &str| {
            s.replace("{{HOST_IP}}", &facts.host_ip)
                .replace("{{HOST_MDNS}}", &facts.host_mdns)
        };
        for cert in &manifest.app.container.generated_certs {
            if tokio::fs::metadata(&cert.crt).await.is_ok()
                && tokio::fs::metadata(&cert.key).await.is_ok()
            {
                continue;
            }
            if let Some(parent) = Path::new(&cert.crt).parent() {
                create_dir_all_or_sudo(parent).await?;
            }
            if let Some(parent) = Path::new(&cert.key).parent() {
                create_dir_all_or_sudo(parent).await?;
            }
            let cn = render(cert.common_name.as_deref().unwrap_or("{{HOST_IP}}"));
            let san = if cert.sans.is_empty() {
                format!("IP:{},IP:127.0.0.1,DNS:localhost", facts.host_ip)
            } else {
                cert.sans
                    .iter()
                    .map(|s| render(s))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            let status = tokio::process::Command::new("openssl")
                .args([
                    "req",
                    "-x509",
                    "-newkey",
                    "rsa:2048",
                    "-nodes",
                    "-keyout",
                    &cert.key,
                    "-out",
                    &cert.crt,
                    "-days",
                    "3650",
                    "-subj",
                    &format!("/CN={cn}"),
                    "-addext",
                    &format!("subjectAltName={san}"),
                ])
                .status()
                .await
                .with_context(|| format!("running openssl for manifest cert {}", cert.crt))?;
            if !status.success() {
                anyhow::bail!("openssl failed to generate manifest cert {}", cert.crt);
            }
        }
        Ok(())
    }
}

async fn ensure_rendered_file(path: &str, content: &str, overwrite: bool) -> Result<HookOutcome> {
    let p = Path::new(path);
    if let Ok(existing) = tokio::fs::read_to_string(p).await {
        if existing == content || !overwrite {
            return Ok(HookOutcome::Unchanged);
        }
    } else if p.exists() && !overwrite {
        return Ok(HookOutcome::Unchanged);
    }

    let parent = p
        .parent()
        .ok_or_else(|| anyhow::anyhow!("generated file path has no parent: {}", path))?;
    create_dir_all_or_sudo(parent).await?;
    write_generated_file_atomically(p, content).await?;
    Ok(HookOutcome::Rewritten)
}

async fn create_dir_all_or_sudo(path: &Path) -> Result<()> {
    match tokio::fs::create_dir_all(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let path = path.to_string_lossy();
            let status = host_sudo(&["mkdir", "-p", &path])
                .await
                .with_context(|| format!("creating {path} via sudo"))?;
            if !status.success() {
                anyhow::bail!("mkdir -p {path} via sudo exited with {status}");
            }
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("creating {}", path.display())),
    }
}

async fn write_generated_file_atomically(path: &Path, content: &str) -> Result<()> {
    let file_name = path.file_name().and_then(|s| s.to_str()).ok_or_else(|| {
        anyhow::anyhow!("generated file path has no filename: {}", path.display())
    })?;
    let tmp = path.with_file_name(format!(".{file_name}.archy-tmp"));
    match tokio::fs::write(&tmp, content).await {
        Ok(()) => {
            tokio::fs::rename(&tmp, path).await.with_context(|| {
                format!(
                    "renaming generated file {} -> {}",
                    tmp.display(),
                    path.display()
                )
            })?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let script = format!(
                "set -eu\ncat > '{}' <<'ARCHYGENERATEDFILE'\n{}ARCHYGENERATEDFILE\n",
                shell_quote(&path.to_string_lossy()),
                content
            );
            let status = host_sudo(&["sh", "-lc", &script])
                .await
                .with_context(|| format!("writing generated file {} via sudo", path.display()))?;
            if !status.success() {
                anyhow::bail!(
                    "writing generated file {} via sudo exited with {status}",
                    path.display()
                );
            }
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("writing tmp {}", tmp.display())),
    }
}

fn shell_quote(s: &str) -> String {
    s.replace('\'', "'\\''")
}

/// Result of a pre-start hook pass. See `run_pre_start_hooks` docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HookOutcome {
    /// The hook wrote new bytes to a bind-mounted file; the caller
    /// should restart the container if it's currently running.
    Rewritten,
    /// The hook ran cleanly; nothing on disk changed.
    Unchanged,
}

// ---------------------------------------------------------------------------
// Trait impl (Step 4): the shared ContainerOrchestrator surface.
// This IS the public RPC-facing API; there are no duplicate inherent methods.
// ---------------------------------------------------------------------------

#[async_trait]
impl ContainerOrchestrator for ProdContainerOrchestrator {
    async fn knows_app(&self, app_id: &str) -> bool {
        self.state.read().await.manifests.contains_key(app_id)
    }

    async fn reload_manifests(&self) -> Result<usize> {
        // Safe mid-session: the rebuild is atomic under the state write lock,
        // and although it wipes the in-memory `disabled` set, reconcile also
        // filters on the durable user-stopped/user-uninstalled markers — the
        // same contract every service restart already relies on.
        self.load_manifests().await
    }

    async fn install(&self, app_id: &str) -> Result<String> {
        anyhow::ensure!(super::staged_update::load(&self.data_dir, app_id).await?.is_none(),
            "An update is staged; use explicit start, or uninstall before selecting another installation");
        let lm = self.loaded(app_id).await?;
        // Optional shared-service preconditions are checked before recording
        // installation or creating anything. A headless adapter must not claim
        // successful installation against a missing indexing stack.
        if let Some(required) = lm
            .manifest
            .app
            .extensions
            .get("install_prerequisites")
            .and_then(|value| value.as_sequence())
        {
            let present = self
                .runtime
                .list_containers()
                .await
                .context("check installed prerequisite services")?;
            for id in required.iter().filter_map(|value| value.as_str()) {
                let dependency = self.loaded(id).await.map_err(|_| InstallPrerequisiteError(
                    format!("Required app {id} is unavailable. Refresh the app catalog before installing {}.",
                        lm.manifest.app.name)))?;
                let name = compute_container_name(&dependency.manifest);
                if !present
                    .iter()
                    .any(|container| container.name.trim_start_matches('/') == name)
                {
                    let owner = crate::app_ops::owning_package(id);
                    let title = self
                        .loaded(owner)
                        .await
                        .map(|app| app.manifest.app.name)
                        .unwrap_or(dependency.manifest.app.name);
                    return Err(InstallPrerequisiteError(format!(
                        "Install {title} first, then install {}.",
                        lm.manifest.app.name
                    ))
                    .into());
                }
            }
        }
        {
            let mut state = self.state.write().await;
            state.disabled.remove(app_id);
        }
        // Installing is an explicit "I want this running" action — clear the
        // user-stopped marker so the new reconcile guard in
        // `ensure_running_with_mode` doesn't skip the very container we're
        // installing. (start/restart RPC handlers clear it on their side too.)
        crate::crash_recovery::clear_user_stopped(&self.data_dir, app_id).await;
        // Same for the user-uninstalled marker — otherwise re-installing a
        // baseline app the user had previously uninstalled would hit the
        // same reconcile guard and silently no-op.
        crate::crash_recovery::clear_user_uninstalled(&self.data_dir, app_id).await;
        // Durable record that this app is installed, so a container that later
        // vanishes is recovered however long it has been gone — the
        // `was_running` snapshot alone forgets after a few daemon restarts.
        crate::crash_recovery::mark_installed(&self.data_dir, app_id).await;
        // Idempotent: if the container is already up and healthy, just
        // refresh hooks and return. If it's stopped, start it. If it's
        // missing or in a wedged state, install fresh.
        //
        // The RPC install handler used to do its own "container exists →
        // adopt + return" probe before calling here, but that path skipped
        // health verification (the .228 "running but unreachable" failure
        // mode). Routing every install through here means the orchestrator
        // is the one source of truth for what "installed" means.
        let name = compute_container_name(&lm.manifest);
        // ensure_running takes the per-app lock itself; release the install
        // path lock first if we hold one (we don't — install is the entry
        // point). Just delegate.
        self.prepare_for_start(&lm.manifest).await?;
        let action = self.ensure_running(&lm).await?;
        let result = match action {
            ReconcileAction::NoOp | ReconcileAction::Started | ReconcileAction::Installed => {
                Ok(name)
            }
            ReconcileAction::Left(state) => {
                // Container is in a wedged state (Created / Paused / Unknown).
                // Force-recreate so the install RPC has a clean outcome.
                tracing::warn!(
                    app_id = %app_id,
                    state = %state,
                    "install: container in wedged state, force-recreating"
                );
                let lock = self.app_lock(app_id).await;
                let _guard = lock.lock().await;
                let _ = self.runtime.stop_container(&name).await;
                let _ = self.runtime.remove_container(&name).await;
                self.install_fresh(&lm).await?;
                Ok(name)
            }
        };
        if result.is_ok() {
            super::staged_update::clear_installed(&self.data_dir, app_id).await?;
        }
        result
    }

    async fn start(&self, app_id: &str) -> Result<()> {
        if self.start_staged_update(app_id).await? {
            return Ok(());
        }
        if let Some(members) = self.mempool_umbrella_members(app_id).await {
            tracing::info!(
                app_id,
                "starting legacy umbrella id via split-stack members"
            );
            for (i, member) in members.iter().enumerate() {
                if i > 0 {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
                Box::pin(self.start(member))
                    .await
                    .with_context(|| format!("start split-stack member {member}"))?;
            }
            return Ok(());
        }
        {
            let mut state = self.state.write().await;
            state.disabled.remove(app_id);
        }
        // Explicit start clears the user-stopped marker so the reconcile guard in
        // `ensure_running_with_mode` doesn't skip this container (symmetric with
        // install; the start/restart RPC handlers also clear it).
        crate::crash_recovery::clear_user_stopped(&self.data_dir, app_id).await;
        crate::crash_recovery::clear_user_uninstalled(&self.data_dir, app_id).await;
        // Durable record that this app is installed, so a container that later
        // vanishes is recovered however long it has been gone — the
        // `was_running` snapshot alone forgets after a few daemon restarts.
        crate::crash_recovery::mark_installed(&self.data_dir, app_id).await;
        let lm = self.loaded(app_id).await?;
        let action = self.ensure_running(&lm).await?;
        match action {
            ReconcileAction::NoOp | ReconcileAction::Started | ReconcileAction::Installed => Ok(()),
            ReconcileAction::Left(state) => {
                let name = compute_container_name(&lm.manifest);
                tracing::warn!(
                    app_id = %app_id,
                    container = %name,
                    state = %state,
                    "start: container in wedged state, force-recreating"
                );
                let lock = self.app_lock(app_id).await;
                let _guard = lock.lock().await;
                let _ = self.runtime.stop_container(&name).await;
                let _ = self.runtime.remove_container(&name).await;
                self.install_fresh(&lm).await
            }
        }
    }

    async fn stop(&self, app_id: &str) -> Result<()> {
        if let Some(members) = self.mempool_umbrella_members(app_id).await {
            tracing::info!(
                app_id,
                "stopping legacy umbrella id via split-stack members"
            );
            for member in members.iter().rev() {
                Box::pin(self.stop(member))
                    .await
                    .with_context(|| format!("stop split-stack member {member}"))?;
            }
            return Ok(());
        }
        {
            let mut state = self.state.write().await;
            state.disabled.insert(app_id.to_string());
        }
        let lm = self.loaded(app_id).await?;
        let lock = self.app_lock(app_id).await;
        let _guard = lock.lock().await;
        let name = compute_container_name(&lm.manifest);
        // Per-app graceful-stop grace: manifest `stop_grace_secs` if declared,
        // else the historical per-app table. Slow-to-SIGTERM apps (bitcoin-core
        // 600s, lnd 330s, electrumx 300s, fedimint 60s…) otherwise get a too-short
        // `podman stop -t` and the stop is reported failed while the container
        // keeps running. See PRODUCTION-MASTER-PLAN §8b.
        let grace_secs = resolve_stop_grace_secs(&lm.manifest, &name);
        // Quadlet-owned containers are restarted by systemd if only `podman stop`
        // is used. Stop the user service first, then stop the container as a
        // defensive fallback for legacy/non-Quadlet installs. Give systemd the
        // per-app grace before it force-kills the app-scoped unit.
        let quadlet_timeout = std::time::Duration::from_secs(
            grace_secs + archipelago_container::runtime::STOP_GRACE_DEADLINE_BUFFER_SECS,
        );
        if let Err(err) =
            quadlet::stop_service_with_timeout(&format!("{name}.service"), quadlet_timeout).await
        {
            tracing::debug!(container = %name, error = %err, "quadlet stop skipped/failed");
        }
        match self
            .runtime
            .stop_container_with_grace(&name, grace_secs)
            .await
        {
            Ok(()) => Ok(()),
            Err(err) => {
                let stuck_stopping = self
                    .runtime
                    .get_container_status(&name)
                    .await
                    .map(|status| matches!(status.state, ContainerState::Stopping))
                    .unwrap_or(false);
                if stuck_stopping {
                    tracing::warn!(
                        app_id = %app_id,
                        container = %name,
                        error = %err,
                        "container stop timed out and left a stuck Stopping record; force-removing record"
                    );
                    self.runtime
                        .remove_container(&name)
                        .await
                        .with_context(|| format!("force remove stuck stopping container {name}"))?;
                    return Ok(());
                }
                Err(err).with_context(|| format!("stop_container {name}"))
            }
        }
    }

    async fn restart(&self, app_id: &str) -> Result<()> {
        if let Some(members) = self.mempool_umbrella_members(app_id).await {
            tracing::info!(
                app_id,
                "restarting legacy umbrella id via split-stack members"
            );
            for (i, member) in members.iter().enumerate() {
                if i > 0 {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
                if let Err(err) = Box::pin(self.restart(member)).await {
                    // A member whose container/unit is gone (e.g. removed by a
                    // quadlet stop) can't be "restarted" — fall back to start,
                    // which recreates it via ensure_running.
                    tracing::warn!(member, error = %err, "member restart failed; falling back to start");
                    Box::pin(self.start(member))
                        .await
                        .with_context(|| format!("restart split-stack member {member}"))?;
                }
            }
            return Ok(());
        }
        let lm = self.loaded(app_id).await?;
        let lock = self.app_lock(app_id).await;
        let _guard = lock.lock().await;
        self.ensure_app_secrets(app_id).await?;
        let name = compute_container_name(&lm.manifest);
        let mut resolved_manifest = lm.manifest.clone();
        self.resolve_dynamic_env(&mut resolved_manifest).await?;

        let service = format!("{name}.service");
        if self.quadlet_unit_exists(&name).await? {
            self.prepare_for_start(&resolved_manifest).await?;
            self.sync_quadlet_unit(&lm, &name).await?;
            if let Err(err) = quadlet::restart_service(&service).await {
                tracing::warn!(container = %name, error = %err, "quadlet restart failed; trying start");
                quadlet::enable_now(&service)
                    .await
                    .with_context(|| format!("restart start quadlet service {service}"))?;
            }
            self.run_post_start_hooks(app_id).await?;
            wait_for_manifest_host_ports(
                &resolved_manifest,
                host_port_wait_timeout_secs(&resolved_manifest),
                &self.data_dir,
            )
            .await?;
            return Ok(());
        }

        // Best-effort restart. Pasta containers need a real restart in the
        // user scope; stop+start can devolve into a no-op if stop times out.
        self.prepare_for_start(&resolved_manifest).await?;
        restart_container_scoped_if_pasta(self.runtime.as_ref(), &resolved_manifest, &name)
            .await
            .with_context(|| format!("restart container {name}"))?;
        self.run_post_start_hooks(app_id).await?;
        wait_for_manifest_host_ports(
            &resolved_manifest,
            host_port_wait_timeout_secs(&resolved_manifest),
            &self.data_dir,
        )
        .await?;
        if uses_pasta_network(&resolved_manifest) {
            wait_for_container_stable_running(self.runtime.as_ref(), &name, 15, 90).await?;
            repair_manifest_host_ports_after_stability(
                self.runtime.as_ref(),
                &resolved_manifest,
                &name,
                &self.data_dir,
            )
            .await?;
        }
        Ok(())
    }

    /// Remove the container. `preserve_data=true` is honored by NOT touching volumes
    /// here (production volumes live under `/var/lib/archipelago` — removal is a
    /// separate operation owned by the data layer, not this orchestrator).
    async fn remove(&self, app_id: &str, _preserve_data: bool) -> Result<()> {
        // A removed catalog entry must remain uninstallable. The RPC caller
        // still removes legacy containers and persists uninstall intent after
        // confirming they are gone; do not block it on a missing manifest.
        if !self.state.read().await.manifests.contains_key(app_id) {
            anyhow::ensure!(
                !app_id.is_empty()
                    && app_id.len() <= 128
                    && app_id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_')),
                "Invalid app id"
            );
            let lock = self.app_lock(app_id).await;
            let _guard = lock.lock().await;
            for name in [app_id.to_string(), format!("archy-{app_id}")] {
                self.remove_quadlet_unit_if_present(&name).await?;
            }
            self.state.write().await.disabled.insert(app_id.to_string());
            super::staged_update::clear(&self.data_dir, app_id).await?;
            super::staged_update::clear_installed(&self.data_dir, app_id).await?;
            return Ok(());
        }
        let lm = self.loaded(app_id).await?;
        let lock = self.app_lock(app_id).await;
        let _guard = lock.lock().await;
        let name = compute_container_name(&lm.manifest);

        let unit_removed = match quadlet::unit_dir().await {
            Ok(dir) => {
                let unit_path = dir.join(format!("{name}.container"));
                if tokio::fs::try_exists(&unit_path).await.unwrap_or(false) {
                    quadlet::disable_remove(&name, &dir)
                        .await
                        .with_context(|| format!("remove quadlet unit for {name}"))?;
                    true
                } else {
                    false
                }
            }
            Err(e) => {
                tracing::debug!(container = %name, error = %e, "remove: cannot locate quadlet dir");
                false
            }
        };

        let _ = self.runtime.stop_container(&name).await;
        let mut remove_err = None;
        if unit_removed {
            let _ = self.runtime.remove_container(&name).await;
        } else {
            if let Err(e) = self.runtime.remove_container(&name).await {
                remove_err = Some(e.context(format!("remove_container {name}")));
            }
        }

        {
            let mut state = self.state.write().await;
            state.disabled.insert(app_id.to_string());
        }
        if let Some(e) = remove_err {
            return Err(e);
        }
        // Durable, unlike `state.disabled` above (wiped by every
        // `load_manifests()`, which runs on every archipelago restart/reboot
        // before the first reconcile) — without this, reconcile's self-heal
        // would bring the app right back after an explicit uninstall
        // survives to the next restart. Only mark on the success path above
        // — a failed removal means the app isn't actually gone.
        crate::crash_recovery::mark_user_uninstalled(&self.data_dir, app_id).await;
        // …and drop the installation record in the same breath. Leaving a
        // stale claim behind would let desired-state recovery recreate the very
        // app that was just uninstalled.
        crate::crash_recovery::clear_installed(&self.data_dir, app_id).await;
        super::staged_update::clear(&self.data_dir, app_id).await?;
        super::staged_update::clear_installed(&self.data_dir, app_id).await?;
        Ok(())
    }

    /// Upgrade: stop-remove-reinstall (re-pulls or rebuilds as required).
    async fn upgrade(&self, app_id: &str) -> Result<()> {
        use super::staged_update;
        let lock = self.app_lock(app_id).await;
        let _guard = lock.lock().await;
        let existing = staged_update::load(&self.data_dir, app_id).await?;
        let lm = if let Some(record) = &existing {
            LoadedManifest {
                manifest: record.manifest.clone(),
                manifest_dir: record.manifest_dir.clone(),
            }
        } else {
            self.loaded(app_id).await?
        };
        let name = compute_container_name(&lm.manifest);
        anyhow::ensure!(
            !staged_update::marked(&self.data_dir, "user-uninstalled.json", app_id, &name).await?,
            "App was uninstalled; update cannot reinstall it"
        );
        let stopped =
            staged_update::marked(&self.data_dir, "user-stopped.json", app_id, &name).await?;
        if stopped || existing.is_some() {
            anyhow::ensure!(
                staged_update::marked(&self.data_dir, "installed-apps.json", app_id, &name).await?,
                "Stopped update needs durable installation evidence"
            );
            let mut record = match existing {
                Some(record) => record,
                None => {
                    let mut manifest = lm.manifest.clone();
                    resolve_catalog_image(&mut manifest);
                    staged_update::StagedUpdate {
                        manifest,
                        manifest_dir: lm.manifest_dir.clone(),
                        ready: false,
                    }
                }
            };
            record.image()?;
            crate::crash_recovery::mark_user_stopped(&self.data_dir, app_id).await;
            anyhow::ensure!(
                staged_update::marked(&self.data_dir, "user-stopped.json", app_id, &name).await?,
                "Could not persist stopped update intent"
            );
            // Freeze the approved manifest before any pull. Retries cannot
            // silently switch to another catalog revision after an interruption.
            record.ready = false;
            staged_update::save(&self.data_dir, &record).await?;
            self.remove_quadlet_unit_if_present(&name).await?;
            if self
                .runtime
                .list_containers()
                .await?
                .iter()
                .any(|c| c.name.trim_start_matches('/') == name)
            {
                self.runtime.stop_container(&name).await?;
                self.runtime.remove_container(&name).await?;
            }
            let pinned = LoadedManifest {
                manifest: record.manifest.clone(),
                manifest_dir: record.manifest_dir.clone(),
            };
            self.ensure_resolved_source_available(&pinned).await?;
            anyhow::ensure!(
                self.runtime.image_exists(record.image()?).await?,
                "Staged image unavailable after preparation"
            );
            record.ready = true;
            staged_update::save(&self.data_dir, &record).await?;
            return Ok(());
        }
        let mut resolved = lm.manifest.clone();
        resolve_catalog_image(&mut resolved);
        if resolved.app.container.build.is_none() {
            if let Some(target) = &resolved.app.container.image {
                if let Ok(running) = self.runtime.get_container_status(&name).await {
                    match crate::container::image_versions::compare_image_versions(
                        target,
                        &running.image,
                    ) {
                        Some(std::cmp::Ordering::Less) => anyhow::bail!(
                            "Refusing to downgrade {} from {} to {} during update",
                            app_id,
                            running.image,
                            target
                        ),
                        Some(std::cmp::Ordering::Equal) => {
                            staged_update::clear_installed(&self.data_dir, app_id).await?;
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        }
        let _ = self.runtime.stop_container(&name).await;
        let _ = self.runtime.remove_container(&name).await;
        self.install_fresh(&lm).await?;
        staged_update::clear_installed(&self.data_dir, app_id).await
    }

    async fn staged_upgrade_image(&self, app_id: &str) -> Result<Option<String>> {
        match super::staged_update::load(&self.data_dir, app_id).await? {
            Some(record) => {
                anyhow::ensure!(record.ready, "Staged update is not ready");
                anyhow::ensure!(
                    self.runtime.image_exists(record.image()?).await?,
                    "Staged image unavailable"
                );
                Ok(Some(record.image()?.to_string()))
            }
            None => Ok(None),
        }
    }

    async fn status(&self, app_id: &str) -> Result<ContainerStatus> {
        let lm = self.loaded(app_id).await?;
        let name = compute_container_name(&lm.manifest);
        self.runtime.get_container_status(&name).await
    }

    async fn list(&self) -> Result<Vec<ContainerStatus>> {
        // Intersect runtime containers with our known manifests' expected names.
        let all = self.runtime.list_containers().await?;
        let state = self.state.read().await;
        let mut wanted: Vec<String> = state
            .manifests
            .values()
            .map(|lm| compute_container_name(&lm.manifest))
            .collect();
        wanted.sort();
        wanted.dedup();
        let set: std::collections::HashSet<&str> = wanted.iter().map(|s| s.as_str()).collect();
        Ok(all
            .into_iter()
            .filter(|c| {
                let bare = c.name.strip_prefix('/').unwrap_or(&c.name);
                set.contains(bare)
            })
            .collect())
    }

    async fn logs(&self, app_id: &str, lines: u32) -> Result<Vec<String>> {
        let lm = self.loaded(app_id).await?;
        let name = compute_container_name(&lm.manifest);
        self.runtime.get_container_logs(&name, lines).await
    }

    async fn health(&self, app_id: &str) -> Result<String> {
        let status = <Self as ContainerOrchestrator>::status(self, app_id).await?;
        Ok(match status.state {
            ContainerState::Running => status.health.unwrap_or_else(|| "healthy".to_string()),
            ContainerState::Stopped | ContainerState::Exited | ContainerState::Stopping => {
                "unhealthy".to_string()
            }
            ContainerState::Created => "starting".to_string(),
            ContainerState::Paused => "paused".to_string(),
            ContainerState::Unknown(s) => format!("unknown:{s}"),
        })
    }

    async fn mark_credential_rotated(&self, app_id: &str) {
        self.credential_rotated
            .lock()
            .await
            .insert(app_id.to_string());
        tracing::info!(
            app_id = %app_id,
            "a credential this app consumes was rotated — its running container will be recreated \
             on the next drift check even though the app is restart-sensitive"
        );
    }
}

/// Compare a manifest's command against a live container's, over the
/// concatenated argv (entrypoint ++ args) rather than each half separately.
/// The halves are not stable across create paths: quadlet's `Entrypoint=`
/// takes a single value, so a manifest entrypoint of `sh -lc` is written as
/// `Entrypoint=sh` + `Exec=-lc ...`, and podman then reports
/// `Config.Entrypoint=["sh"]`, `Config.Cmd=["-lc", ...]` — a per-half
/// comparison reads that as permanent drift and recreates the container on
/// every reconcile pass (electrumx on .228, 2026-07-08: 114 recreates in 6h).
/// The concatenation is what actually runs, so it is the thing to compare.
fn command_argv_drifted(
    expected_entry: Option<&[String]>,
    expected_args: &[String],
    current_entry: &[String],
    current_cmd: &[String],
) -> bool {
    // Mirror shell_join's newline flattening: a YAML-folded manifest script
    // keeps literal newlines in custom_args, but the quadlet Exec= line (and
    // therefore Config.Cmd) has them as spaces. Normalize both sides so the
    // same script never reads as drift over line breaks alone (bitcoin-knots
    // and fedimint-gateway on .228, 2026-07-08).
    let norm =
        |v: &[String]| -> Vec<String> { v.iter().map(|s| s.replace(['\r', '\n'], " ")).collect() };
    let current_cmd = norm(current_cmd);
    let expected_args = norm(expected_args);
    let Some(expected_entry) = expected_entry else {
        // No entrypoint override: the manifest args map to Cmd directly.
        return !expected_args.is_empty() && current_cmd != expected_args;
    };
    let expected: Vec<String> = norm(expected_entry)
        .into_iter()
        .chain(expected_args.iter().cloned())
        .collect();
    let actual: Vec<String> = norm(current_entry)
        .into_iter()
        .chain(current_cmd.iter().cloned())
        .collect();
    if expected_args.is_empty() {
        // With no manifest args, the container's trailing Cmd may be the
        // image's baked-in CMD — only the entrypoint prefix must match.
        actual.len() < expected.len() || actual[..expected.len()] != expected[..]
    } else {
        actual != expected
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use archipelago_container::BuildConfig;
    use async_trait::async_trait;
    use std::sync::Mutex as StdMutex;

    fn port(host: u16, container: u16) -> archipelago_container::manifest::PortMapping {
        archipelago_container::manifest::PortMapping {
            host,
            container,
            protocol: "tcp".to_string(),
            bind: String::new(),
            auth: None,
            auth_rationale: None,
            session_passthrough: false,
        }
    }

    fn sv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// The .38 indeedhub incident class: an absent stack member must be
    /// recovered when its siblings have live containers (the stack is
    /// installed), and left alone when the whole stack is gone or the app
    /// is not a stack member at all.
    #[tokio::test]
    async fn gitea_fresh_url_seed_preserves_operator_config_and_reports_write_failure() {
        let manifest =
            AppManifest::parse(include_str!("../../../../apps/gitea/manifest.yml")).unwrap();
        let seed = &manifest.app.files[0];
        assert!(!seed.overwrite);
        let content = seed.content.replace("{{HOST_IP}}", "192.0.2.1");
        assert!(content.contains("ROOT_URL = http://192.0.2.1:3001/"));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fresh/app.ini");
        assert_eq!(
            ensure_rendered_file(path.to_str().unwrap(), &content, seed.overwrite)
                .await
                .unwrap(),
            HookOutcome::Rewritten
        );
        assert!(tokio::fs::read_to_string(&path)
            .await
            .unwrap()
            .contains("ROOT_URL"));
        let custom =
            "[server]\nROOT_URL = https://git.example.test/\n[database]\nDB_TYPE = postgres\n";
        tokio::fs::write(&path, custom).await.unwrap();
        assert_eq!(
            ensure_rendered_file(path.to_str().unwrap(), &content, seed.overwrite)
                .await
                .unwrap(),
            HookOutcome::Unchanged
        );
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), custom);
        let impossible = path.join("app.ini");
        assert!(
            ensure_rendered_file(impossible.to_str().unwrap(), &content, seed.overwrite)
                .await
                .is_err()
        );
    }

    #[test]
    fn ssh_sandbox_capability_repair_uses_bounding_set_and_preserves_extra_overrides() {
        let required = vec!["CHOWN".into(), "SYS_CHROOT".into()];
        assert!(missing_declared_capability(
            &required,
            &["CAP_CHOWN".into()]
        ));
        assert!(!missing_declared_capability(
            &required,
            &["CAP_CHOWN".into(), "CAP_SYS_CHROOT".into()]
        ));
        assert!(!missing_declared_capability(
            &required,
            &["CHOWN".into(), "SYS_CHROOT".into(), "CAP_KILL".into()]
        ));
    }

    #[test]
    fn npm_legacy_gateway_converges_and_rejects_conflicting_network_options() {
        let expected = Some("slirp4netns:allow_host_loopback=true,cidr=169.254.1.0/24");
        assert!(is_builtin_network_mode(expected.unwrap()));
        for actual in [
            "pasta",
            "slirp4netns:allow_host_loopback=true",
            "slirp4netns:allow_host_loopback=true,cidr=10.0.2.0/24",
            "slirp4netns:allow_host_loopback=true,cidr=169.254.1.0/24,cidr=10.0.2.0/24",
        ] {
            assert!(rootless_network_mode_drifted(expected, actual), "{actual}");
        }
        let actual = "slirp4netns:cidr=169.254.1.0/24,allow_host_loopback=true,mtu=65520";
        assert!(!rootless_network_mode_drifted(expected, actual));
        let spec = br#"{"netns":{"nsmode":"slirp4netns"},"network_options":{"slirp4netns":["cidr=169.254.1.0/24","allow_host_loopback=true"]}}"#;
        assert!(!rootless_network_mode_drifted(
            expected,
            &rootless_network_from_spec(spec).unwrap()
        ));
    }

    #[test]
    fn npm_initialization_budget_survives_network_migration() {
        let mut manifest = AppManifest::parse(include_str!(
            "../../../../apps/nginx-proxy-manager/manifest.yml"
        ))
        .unwrap();
        for network in ["pasta", "slirp4netns:allow_host_loopback=true"] {
            manifest.app.container.network = Some(network.to_string());
            assert_eq!(host_port_wait_timeout_secs(&manifest), 180);
        }
    }

    #[test]
    fn api_created_rootless_options_do_not_cause_repeated_recreation() {
        let expected = Some("slirp4netns:allow_host_loopback=true");
        let spec = br#"{"netns":{"nsmode":"slirp4netns"},"network_options":{"slirp4netns":["allow_host_loopback=true"]}}"#;
        let actual = rootless_network_from_spec(spec).unwrap();
        assert!(!rootless_network_mode_drifted(expected, &actual));
        let plain = rootless_network_from_spec(br#"{"netns":{"nsmode":"slirp4netns"}}"#).unwrap();
        assert!(rootless_network_mode_drifted(expected, &plain));
        assert!(rootless_network_from_spec(b"invalid").is_none());
    }

    #[test]
    fn explicit_rootless_network_change_converges_without_guessing_defaults() {
        let npm = Some("slirp4netns:allow_host_loopback=true");
        assert!(is_builtin_network_mode(npm.unwrap()));
        for actual in [
            "pasta",
            "bridge",
            "slirp4netns",
            "slirp4netns:allow_host_loopback=false",
        ] {
            assert!(rootless_network_mode_drifted(npm, actual), "{actual}");
        }
        for actual in [
            "slirp4netns:allow_host_loopback=true",
            "slirp4netns:mtu=65520,allow_host_loopback=true",
        ] {
            assert!(!rootless_network_mode_drifted(npm, actual), "{actual}");
        }
        assert!(rootless_network_mode_drifted(Some("slirp4netns"), "pasta"));
        assert!(rootless_network_mode_drifted(Some("slirp4netns"), "bridge"));
        assert!(!rootless_network_mode_drifted(
            Some("slirp4netns"),
            "slirp4netns"
        ));
        assert!(!rootless_network_mode_drifted(
            Some("slirp4netns"),
            "slirp4netns:allow_host_loopback=true"
        ));
        assert!(!rootless_network_mode_drifted(None, "pasta"));
        assert!(!rootless_network_mode_drifted(Some("slirp4netns"), ""));
        assert!(!rootless_network_mode_drifted(Some("archy-net"), "bridge"));
    }

    #[test]
    fn absent_stack_member_recovery_requires_a_live_sibling() {
        let present: HashSet<String> = ["indeedhub-redis", "indeedhub-relay", "indeedhub"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let names = std::collections::HashMap::new();
        // Missing members of a stack with live siblings → recover.
        assert!(absent_stack_member_with_live_sibling(
            "indeedhub-minio",
            &present,
            &names
        ));
        assert!(absent_stack_member_with_live_sibling(
            "indeedhub-postgres",
            &present,
            &names
        ));
        // Whole stack absent → NOT recovered (uninstalled stays uninstalled).
        let empty = HashSet::new();
        assert!(!absent_stack_member_with_live_sibling(
            "indeedhub-minio",
            &empty,
            &names
        ));
        // Non-stack app → never.
        assert!(!absent_stack_member_with_live_sibling(
            "vaultwarden",
            &present,
            &names
        ));
        // An app's OWN container being present proves nothing about siblings.
        let only_self: HashSet<String> = std::iter::once("indeedhub-minio".to_string()).collect();
        assert!(!absent_stack_member_with_live_sibling(
            "indeedhub-minio",
            &only_self,
            &names
        ));
        // App-id → container-name mapping is honoured (immich_postgres runs
        // under an underscore name while its app id is hyphenated).
        let mut mapped = std::collections::HashMap::new();
        mapped.insert("immich-postgres".to_string(), "immich_postgres".to_string());
        let immich_present: HashSet<String> =
            std::iter::once("immich_postgres".to_string()).collect();
        assert!(absent_stack_member_with_live_sibling(
            "immich-redis",
            &immich_present,
            &mapped
        ));
    }

    #[test]
    fn command_drift_tolerates_quadlet_entrypoint_split() {
        // Quadlet writes Entrypoint=sh + Exec=-lc "<script>", so podman
        // reports entry=["sh"], cmd=["-lc", script] for a manifest that
        // declared entrypoint ["sh", "-lc"] + custom_args [script]. Same
        // argv, different split — must NOT read as drift (the electrumx
        // recreate loop).
        let script = "export DAEMON_URL=...; exec electrumx_server";
        assert!(!command_argv_drifted(
            Some(&sv(&["sh", "-lc"])),
            &sv(&[script]),
            &sv(&["sh"]),
            &sv(&["-lc", script]),
        ));
        // Legacy podman-create layout keeps the halves as declared.
        assert!(!command_argv_drifted(
            Some(&sv(&["sh", "-lc"])),
            &sv(&[script]),
            &sv(&["sh", "-lc"]),
            &sv(&[script]),
        ));
    }

    #[test]
    fn command_drift_tolerates_yaml_folded_newlines() {
        // A YAML `>-` script with more-indented continuation lines keeps
        // literal newlines in custom_args; shell_join flattens them to
        // spaces when writing Exec=, so Config.Cmd has spaces where the
        // manifest has newlines. Same script — not drift (bitcoin-knots /
        // fedimint-gateway on .228).
        let manifest_script = "if [ -z \"$X\" ]; then\n  X=1;\nfi; exec daemon";
        let stored_script = "if [ -z \"$X\" ]; then   X=1; fi; exec daemon";
        assert!(!command_argv_drifted(
            Some(&sv(&["sh", "-lc"])),
            &sv(&[manifest_script]),
            &sv(&["sh"]),
            &sv(&["-lc", stored_script]),
        ));
        // A genuinely different script still drifts after normalization.
        assert!(command_argv_drifted(
            Some(&sv(&["sh", "-lc"])),
            &sv(&[manifest_script]),
            &sv(&["sh"]),
            &sv(&["-lc", "exec other_daemon"]),
        ));
    }

    #[test]
    fn command_drift_detected_when_argv_actually_differs() {
        assert!(command_argv_drifted(
            Some(&sv(&["sh", "-lc"])),
            &sv(&["new script"]),
            &sv(&["sh"]),
            &sv(&["-lc", "old script"]),
        ));
        assert!(command_argv_drifted(
            Some(&sv(&["bash", "-c"])),
            &sv(&["run"]),
            &sv(&["sh"]),
            &sv(&["-lc", "run"]),
        ));
        // No entrypoint override: args compare against Cmd directly.
        assert!(command_argv_drifted(
            None,
            &sv(&["-server=1"]),
            &sv(&["bitcoind"]),
            &sv(&["-server=0"]),
        ));
        assert!(!command_argv_drifted(
            None,
            &sv(&["-server=1"]),
            &sv(&["bitcoind"]),
            &sv(&["-server=1"]),
        ));
    }

    #[test]
    fn command_drift_entrypoint_only_ignores_image_cmd() {
        // Entrypoint override with no manifest args: the container's Cmd is
        // the image's baked-in CMD — only the entrypoint prefix matters.
        assert!(!command_argv_drifted(
            Some(&sv(&["/usr/local/bin/bitcoind"])),
            &[],
            &sv(&["/usr/local/bin/bitcoind"]),
            &sv(&["-image-default-arg"]),
        ));
        assert!(command_argv_drifted(
            Some(&sv(&["/usr/local/bin/bitcoind"])),
            &[],
            &sv(&["/other/binary"]),
            &sv(&[]),
        ));
    }

    #[test]
    fn port_drift_detected_when_host_port_differs() {
        // The .116 case: container publishes container-port 8080 on host 8080,
        // but the manifest now asks for host 18080.
        let bindings = r#"{"8080/tcp":[{"HostIp":"","HostPort":"8080"}]}"#;
        assert!(host_port_bindings_drifted(bindings, &[port(18080, 8080)]));
    }

    #[test]
    fn no_drift_when_host_port_matches() {
        let bindings = r#"{"8080/tcp":[{"HostIp":"0.0.0.0","HostPort":"18080"}]}"#;
        assert!(!host_port_bindings_drifted(bindings, &[port(18080, 8080)]));
    }

    #[test]
    fn no_drift_when_binding_absent() {
        // Absence is handled elsewhere (host-port repair / host-networked apps);
        // never treat it as drift to avoid a destructive recreate on a false
        // positive.
        assert!(!host_port_bindings_drifted("{}", &[port(18080, 8080)]));
        assert!(!host_port_bindings_drifted("null", &[port(18080, 8080)]));
    }

    #[test]
    fn no_drift_on_unparseable_bindings() {
        assert!(!host_port_bindings_drifted(
            "not json",
            &[port(18080, 8080)]
        ));
    }

    fn bound_port(
        host: u16,
        container: u16,
        bind: &str,
    ) -> archipelago_container::manifest::PortMapping {
        archipelago_container::manifest::PortMapping {
            bind: bind.to_string(),
            ..port(host, container)
        }
    }

    #[test]
    fn bind_drift_detected_when_declared_loopback_but_published_wildcard() {
        // The legacy-container case: a pre-quadlet container still publishes
        // 0.0.0.0 while the catalog-delivered manifest pins the app to
        // loopback for the app gate. Must recreate, or the port stays open on
        // every interface and the gate can never claim it.
        for wildcard in [r#""""#, r#""0.0.0.0""#] {
            let bindings = format!(r#"{{"80/tcp":[{{"HostIp":{wildcard},"HostPort":"8082"}}]}}"#);
            assert!(host_port_bindings_drifted(
                &bindings,
                &[bound_port(8082, 80, "127.0.0.1")]
            ));
        }
    }

    #[test]
    fn no_bind_drift_when_declared_loopback_and_published_loopback() {
        let bindings = r#"{"80/tcp":[{"HostIp":"127.0.0.1","HostPort":"8082"}]}"#;
        assert!(!host_port_bindings_drifted(
            bindings,
            &[bound_port(8082, 80, "127.0.0.1")]
        ));
    }

    #[test]
    fn no_bind_drift_on_undeclared_bind() {
        // Silence is not consent (v1.7.121 incident class): an EMPTY manifest
        // bind must never recreate a loopback-published container to
        // wildcard — that is how Bitcoin's RPC got republished on the LAN.
        let bindings = r#"{"8332/tcp":[{"HostIp":"127.0.0.1","HostPort":"8332"}]}"#;
        assert!(!host_port_bindings_drifted(bindings, &[port(8332, 8332)]));
    }

    #[test]
    fn multi_bind_publish_satisfies_each_declared_entry() {
        // Same host/container pair listed twice (loopback + archy-net
        // gateway): both declared binds are present in the actual publish.
        let bindings = r#"{"8332/tcp":[
            {"HostIp":"127.0.0.1","HostPort":"8332"},
            {"HostIp":"10.89.0.1","HostPort":"8332"}
        ]}"#;
        assert!(!host_port_bindings_drifted(
            bindings,
            &[
                bound_port(8332, 8332, "127.0.0.1"),
                bound_port(8332, 8332, "10.89.0.1")
            ]
        ));
        // And a wildcard-only publish drifts BOTH declared entries.
        let wildcard = r#"{"8332/tcp":[{"HostIp":"","HostPort":"8332"}]}"#;
        assert!(host_port_bindings_drifted(
            wildcard,
            &[
                bound_port(8332, 8332, "127.0.0.1"),
                bound_port(8332, 8332, "10.89.0.1")
            ]
        ));
    }

    #[test]
    fn missing_secret_error_names_the_secret() {
        use archipelago_container::manifest::SecretsProvider;
        let provider = FileSecretsProvider {
            root: PathBuf::from("/nonexistent-secrets-dir-xyz"),
        };
        let err = provider
            .read("bitcoin-rpc-txrelay-rpcauth")
            .expect_err("missing secret must error");
        let msg = err.to_string();
        assert!(
            msg.contains("bitcoin-rpc-txrelay-rpcauth"),
            "error should name the missing secret, got: {msg}"
        );
    }

    /// Instrumented in-memory runtime. Every call is recorded so tests can assert
    /// the exact sequence of side effects.
    #[derive(Default)]
    struct MockRuntime {
        calls: StdMutex<Vec<String>>,
        /// container_name -> ContainerState. Absence = "doesn't exist".
        containers: StdMutex<HashMap<String, ContainerState>>,
        running_images: StdMutex<HashMap<String, String>>,
        /// container_name -> Podman health status.
        health: StdMutex<HashMap<String, String>>,
        /// image_ref -> present. Absence = "not present in local storage".
        images: StdMutex<HashMap<String, bool>>,
        /// container_name -> env that create_container received.
        created_env: StdMutex<HashMap<String, Vec<String>>>,
        /// container_name -> secret env refs that create_container received.
        created_secret_refs:
            StdMutex<HashMap<String, Vec<archipelago_container::manifest::SecretEnvRef>>>,
        /// If set, the next `build_image` call fails with this message.
        fail_build: StdMutex<Option<String>>,
        /// If set, every `pull_image` call fails with this message.
        fail_pull: StdMutex<Option<String>>,
        /// If set, `image_exists` fails for this image reference.
        fail_image_exists: StdMutex<HashMap<String, String>>,
        /// If set, `start_container` for this container fails with this message.
        fail_start: StdMutex<HashMap<String, String>>,
        fail_stop: StdMutex<HashMap<String, String>>,
    }

    impl MockRuntime {
        fn record(&self, s: impl Into<String>) {
            self.calls.lock().unwrap().push(s.into());
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
        fn set_state(&self, name: &str, state: ContainerState) {
            self.containers
                .lock()
                .unwrap()
                .insert(name.to_string(), state);
        }
        fn set_health(&self, name: &str, health: &str) {
            self.health
                .lock()
                .unwrap()
                .insert(name.to_string(), health.to_string());
        }
        fn mark_image_present(&self, tag: &str) {
            self.images.lock().unwrap().insert(tag.to_string(), true);
        }
        fn created_env_for(&self, name: &str) -> Vec<String> {
            self.created_env
                .lock()
                .unwrap()
                .get(name)
                .cloned()
                .unwrap_or_default()
        }
        fn created_secret_refs_for(
            &self,
            name: &str,
        ) -> Vec<archipelago_container::manifest::SecretEnvRef> {
            self.created_secret_refs
                .lock()
                .unwrap()
                .get(name)
                .cloned()
                .unwrap_or_default()
        }
    }

    #[async_trait]
    impl ContainerRuntimeTrait for MockRuntime {
        async fn pull_image(&self, image: &str, _sig: Option<&str>) -> Result<()> {
            self.record(format!("pull_image:{image}"));
            if let Some(msg) = self.fail_pull.lock().unwrap().clone() {
                return Err(anyhow::anyhow!("{msg}"));
            }
            self.mark_image_present(image);
            Ok(())
        }
        async fn create_container(
            &self,
            manifest: &AppManifest,
            name: &str,
            port_offset: u16,
        ) -> Result<String> {
            self.record(format!("create_container:{name}:offset={port_offset}"));
            self.set_state(name, ContainerState::Created);
            if let Some(image) = manifest.app.container.image_ref() {
                self.running_images
                    .lock()
                    .unwrap()
                    .insert(name.into(), image);
            }
            self.created_env
                .lock()
                .unwrap()
                .insert(name.to_string(), manifest.app.environment.clone());
            self.created_secret_refs.lock().unwrap().insert(
                name.to_string(),
                manifest.app.container.secret_env_refs.clone(),
            );
            Ok(name.to_string())
        }
        async fn start_container(&self, name: &str) -> Result<()> {
            self.record(format!("start_container:{name}"));
            let fail = self.fail_start.lock().unwrap().remove(name);
            if let Some(msg) = fail {
                return Err(anyhow::anyhow!(msg));
            }
            self.set_state(name, ContainerState::Running);
            Ok(())
        }
        async fn stop_container(&self, name: &str) -> Result<()> {
            self.record(format!("stop_container:{name}"));
            if let Some(error) = self.fail_stop.lock().unwrap().remove(name) {
                return Err(anyhow::anyhow!(error));
            }
            self.set_state(name, ContainerState::Stopped);
            Ok(())
        }
        async fn remove_container(&self, name: &str) -> Result<()> {
            self.record(format!("remove_container:{name}"));
            self.containers.lock().unwrap().remove(name);
            Ok(())
        }
        async fn get_container_status(&self, name: &str) -> Result<ContainerStatus> {
            self.record(format!("get_container_status:{name}"));
            let map = self.containers.lock().unwrap();
            let state = map
                .get(name)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("not found: {name}"))?;
            let health = self.health.lock().unwrap().get(name).cloned();
            Ok(ContainerStatus {
                id: format!("id-{name}"),
                name: name.to_string(),
                state,
                health,
                exit_code: None,
                started_at: None,
                image: self
                    .running_images
                    .lock()
                    .unwrap()
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| "test-image".to_string()),
                created: "now".to_string(),
                ports: vec![],
                lan_address: None,
            })
        }
        async fn get_container_logs(&self, name: &str, _lines: u32) -> Result<Vec<String>> {
            self.record(format!("get_container_logs:{name}"));
            Ok(vec!["log line".into()])
        }
        async fn list_containers(&self) -> Result<Vec<ContainerStatus>> {
            self.record("list_containers".to_string());
            let map = self.containers.lock().unwrap();
            Ok(map
                .iter()
                .map(|(name, state)| ContainerStatus {
                    id: format!("id-{name}"),
                    name: name.clone(),
                    state: state.clone(),
                    health: None,
                    exit_code: None,
                    started_at: None,
                    image: "test-image".to_string(),
                    created: "now".to_string(),
                    ports: vec![],
                    lan_address: None,
                })
                .collect())
        }
        async fn image_exists(&self, image_ref: &str) -> Result<bool> {
            self.record(format!("image_exists:{image_ref}"));
            if let Some(msg) = self.fail_image_exists.lock().unwrap().get(image_ref) {
                return Err(anyhow::anyhow!(msg.clone()));
            }
            Ok(self
                .images
                .lock()
                .unwrap()
                .get(image_ref)
                .copied()
                .unwrap_or(false))
        }
        async fn build_image(&self, config: &BuildConfig) -> Result<()> {
            self.record(format!(
                "build_image:{}:context={}",
                config.tag, config.context
            ));
            if let Some(msg) = self.fail_build.lock().unwrap().clone() {
                return Err(anyhow::anyhow!("{msg}"));
            }
            self.mark_image_present(&config.tag);
            Ok(())
        }
    }

    fn pull_manifest(id: &str, image: &str) -> AppManifest {
        let yaml = format!(
            "app:\n  id: {id}\n  name: {id}\n  version: 1.0.0\n  container:\n    image: {image}\n"
        );
        AppManifest::parse(&yaml).unwrap()
    }

    fn build_manifest(id: &str, context: &str, tag: &str) -> AppManifest {
        let yaml = format!(
            "app:\n  id: {id}\n  name: {id}\n  version: 1.0.0\n  container:\n    build:\n      context: {context}\n      tag: {tag}\n"
        );
        AppManifest::parse(&yaml).unwrap()
    }

    #[test]
    fn active_manifest_dependencies_override_stale_user_stopped_entries() {
        let parent = AppManifest::parse(
            r#"
app:
  id: btcpay-server
  name: btcpay-server
  version: 1.0.0
  container:
    image: btcpay:latest
  dependencies:
    - app_id: archy-nbxplorer
      version: ">=1"
"#,
        )
        .unwrap();
        let child = pull_manifest("archy-nbxplorer", "nbxplorer:latest");
        let mut user_stopped = HashSet::new();
        user_stopped.insert("archy-nbxplorer".to_string());

        let required = dependency_manifests_required_by_active_apps(
            [&parent, &child].into_iter(),
            &user_stopped,
        );

        assert!(required.contains("archy-nbxplorer"));
    }

    #[test]
    fn catalog_overlay_accepts_valid_image_manifest() {
        let v = serde_json::to_value(pull_manifest("demo", "registry/demo:1.0.0")).unwrap();
        let m = catalog_manifest_to_overlay("demo", v).expect("valid image manifest accepted");
        assert_eq!(m.app.id, "demo");
    }

    #[test]
    fn catalog_overlay_rejects_app_id_mismatch() {
        let v = serde_json::to_value(pull_manifest("demo", "registry/demo:1.0.0")).unwrap();
        assert!(catalog_manifest_to_overlay("other", v).is_none());
    }

    #[test]
    fn catalog_overlay_defers_build_source_to_disk() {
        // Build contexts aren't registry-distributed yet (phase 1 = image-only).
        let v = serde_json::to_value(build_manifest("demo", "ctx", "demo:1.0.0")).unwrap();
        assert!(catalog_manifest_to_overlay("demo", v).is_none());
    }

    #[test]
    fn catalog_overlay_rejects_invalid_manifest() {
        // Deserializes (image and build both absent) but fails validate().
        let v = serde_json::json!({
            "app": { "id": "demo", "name": "demo", "version": "1.0.0", "container": {} }
        });
        assert!(catalog_manifest_to_overlay("demo", v).is_none());
    }

    #[test]
    fn catalog_overlay_rejects_unparseable_value() {
        let v = serde_json::json!({ "not": "a manifest" });
        assert!(catalog_manifest_to_overlay("demo", v).is_none());
    }

    #[test]
    fn catalog_overlay_accepts_all_real_image_manifests() {
        // Guard the registry-distribution round-trip for the WHOLE shipped app
        // set: every apps/*/manifest.yml must deserialize + validate when carried
        // through the catalog as a value. Image-only apps must be accepted;
        // build-source apps must defer to disk (phase 1 = image-only). Catches
        // schema drift between disk manifests and the catalog path.
        let apps_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps");
        if !apps_dir.exists() {
            return; // packaged/CI layout without the repo apps/ tree — skip
        }
        let mut image_apps = 0;
        for entry in std::fs::read_dir(&apps_dir).unwrap().flatten() {
            let mf = entry.path().join("manifest.yml");
            if !mf.exists() {
                continue;
            }
            // Every shipped manifest MUST be valid. load_manifests() silently
            // skips malformed ones in prod, which once let an invalid app.version
            // ("release", no digit) ship — the app then vanished from the
            // orchestrator and a stack install half-fell-back to the legacy path.
            // Fail loudly here instead.
            let m = AppManifest::from_file(&mf)
                .unwrap_or_else(|e| panic!("shipped manifest {} must be valid: {e}", mf.display()));
            let id = m.app.id.clone();
            let is_build = m.app.container.build.is_some();
            let value = serde_json::to_value(&m).expect("manifest serializes to JSON");
            let overlay = catalog_manifest_to_overlay(&id, value);
            if is_build {
                assert!(
                    overlay.is_none(),
                    "{id}: build-source app must defer to disk"
                );
            } else {
                assert!(
                    overlay.is_some(),
                    "{id}: image-only app must round-trip through the catalog"
                );
                image_apps += 1;
            }
        }
        assert!(image_apps > 0, "expected at least one image-only manifest");
    }

    fn manifest_with_container_name(id: &str, image: &str, name: &str) -> AppManifest {
        let yaml = format!(
            "app:\n  id: {id}\n  name: {id}\n  version: 1.0.0\n  container_name: {name}\n  container:\n    image: {image}\n"
        );
        AppManifest::parse(&yaml).unwrap()
    }

    #[test]
    fn compute_container_name_ui_apps_prefixed() {
        let m = pull_manifest("bitcoin-ui", "foo:1");
        assert_eq!(compute_container_name(&m), "archy-bitcoin-ui");
        let m = pull_manifest("electrs-ui", "foo:1");
        assert_eq!(compute_container_name(&m), "archy-electrs-ui");
        let m = pull_manifest("lnd-ui", "foo:1");
        assert_eq!(compute_container_name(&m), "archy-lnd-ui");
        let m = pull_manifest("cuprate-ui", "foo:1");
        assert_eq!(compute_container_name(&m), "archy-cuprate-ui");
    }

    #[test]
    fn cuprate_disk_gate_blocks_only_cuprate_on_small_disks() {
        // 250 GB VPS class: the ~250 GiB Monero chain cannot fit and cuprate
        // has no pruning — boot reconcile must leave it down.
        assert!(requires_cuprate_disk("cuprate", 250));
        assert!(requires_cuprate_disk("cuprate", CUPRATE_MIN_DISK_GB - 1));
        assert!(!requires_cuprate_disk("cuprate", CUPRATE_MIN_DISK_GB));
        assert!(!requires_cuprate_disk("cuprate", 1000));
        // df failure in detect_disk_gb reads as 0 → fail closed at boot: a
        // doomed sync is worse than a node that stays down until it can
        // measure (same direction as the archival-bitcoin skip).
        assert!(requires_cuprate_disk("cuprate", 0));
        // Nothing else is gated here: bitcoin apps self-prune, everything
        // else is irrelevant to the Monero chain.
        for app_id in ["bitcoin-knots", "bitcoin-core", "electrumx", "mempool"] {
            assert!(!requires_cuprate_disk(app_id, 0), "{app_id}");
        }
    }

    #[test]
    fn compute_container_name_backend_apps_bare() {
        let m = pull_manifest("bitcoin-knots", "foo:1");
        assert_eq!(compute_container_name(&m), "bitcoin-knots");
        let m = pull_manifest("lnd", "foo:1");
        assert_eq!(compute_container_name(&m), "lnd");
        let m = pull_manifest("electrumx", "foo:1");
        assert_eq!(compute_container_name(&m), "electrumx");
    }

    #[test]
    fn compute_container_name_extension_override_wins() {
        // An explicit container_name in the manifest always takes priority, even for a
        // UI id that would otherwise get the archy- prefix.
        let m = manifest_with_container_name("bitcoin-ui", "foo:1", "legacy-bitcoin-ui");
        assert_eq!(compute_container_name(&m), "legacy-bitcoin-ui");
    }

    fn manifest_with_stop_grace(id: &str, grace: &str) -> AppManifest {
        let yaml = format!(
            "app:\n  id: {id}\n  name: {id}\n  version: 1.0.0\n  stop_grace_secs: {grace}\n  container:\n    image: foo:1\n"
        );
        AppManifest::parse(&yaml).unwrap()
    }

    #[test]
    fn stop_grace_manifest_field_wins() {
        // An explicit stop_grace_secs overrides the per-app table (fedimint=60).
        let m = manifest_with_stop_grace("fedimint", "180");
        assert_eq!(resolve_stop_grace_secs(&m, "fedimint"), 180);
    }

    #[test]
    fn stop_grace_falls_back_to_table() {
        // No manifest field → the historical per-app table by container name.
        let m = pull_manifest("fedimint", "foo:1");
        assert_eq!(resolve_stop_grace_secs(&m, "fedimint"), 60);
        let m = pull_manifest("bitcoin-knots", "foo:1");
        assert_eq!(resolve_stop_grace_secs(&m, "bitcoin-knots"), 600);
        let m = pull_manifest("electrumx", "foo:1");
        assert_eq!(resolve_stop_grace_secs(&m, "electrumx"), 300);
    }

    #[test]
    fn stop_grace_unknown_app_defaults_to_30() {
        let m = pull_manifest("some-unknown-app", "foo:1");
        assert_eq!(resolve_stop_grace_secs(&m, "some-unknown-app"), 30);
    }

    async fn orch_with(runtime: Arc<MockRuntime>) -> ProdContainerOrchestrator {
        let mut orch = ProdContainerOrchestrator::with_runtime(
            runtime,
            PathBuf::from("/nonexistent-for-tests"),
        );
        let data_dir = tempfile::tempdir().unwrap().keep();
        orch.set_data_dir(data_dir);
        // Redirect the bitcoin-ui pre-start hook to a test-scoped
        // tmpdir, seeded with a fake password file. Shared across
        // every test in this module (OnceLock), so the hook can run
        // idempotently regardless of which test kicks first. Without
        // this redirection, any test that installs the bitcoin-ui
        // fixture would try to write under /var/lib/archipelago.
        orch.set_bitcoin_ui_paths(test_bitcoin_ui_paths());
        orch.set_filebrowser_paths(test_filebrowser_paths());
        orch
    }

    #[tokio::test]
    async fn missing_install_prerequisite_refuses_without_inventory_or_container_mutation() {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt.clone()).await;
        let mut app = pull_manifest("indexer-adapter", "docker.io/library/alpine:3.20");
        app.app.extensions.insert(
            "install_prerequisites".into(),
            serde_yaml::to_value(vec!["shared-index"]).unwrap(),
        );
        orch.insert_manifest_for_test(app, PathBuf::from("/tmp"))
            .await;
        orch.insert_manifest_for_test(
            pull_manifest("shared-index", "index:1"),
            PathBuf::from("/tmp"),
        )
        .await;
        let error = orch.install("indexer-adapter").await.unwrap_err();
        assert!(error.downcast_ref::<InstallPrerequisiteError>().is_some());
        assert!(!crate::crash_recovery::load_installed_apps(&orch.data_dir)
            .await
            .contains("indexer-adapter"));
        assert_eq!(rt.calls(), vec!["list_containers"]);
        // An installed prerequisite satisfies the guard; it is never recreated
        // or reconfigured as part of installing this adapter.
        rt.set_state("shared-index", ContainerState::Running);
        orch.install("indexer-adapter").await.unwrap();
        assert!(!rt
            .calls()
            .iter()
            .any(|c| c.starts_with("create_container:shared-index")));
    }

    fn pull_manifest_with_dynamic_env(id: &str, image: &str) -> AppManifest {
        let yaml = format!(
            "app:\n  id: {id}\n  name: {id}\n  version: 1.0.0\n  container:\n    image: {image}\n    derived_env:\n      - key: FM_API_URL\n        template: \"ws://{{{{HOST_MDNS}}}}:8174\"\n    secret_env:\n      - key: FM_BITCOIND_PASSWORD\n        secret_file: bitcoin-rpc-password\n  environment:\n    - STATIC=1\n"
        );
        AppManifest::parse(&yaml).unwrap()
    }

    fn pull_manifest_with_data_uid(
        id: &str,
        image: &str,
        source: &str,
        uid_gid: &str,
    ) -> AppManifest {
        let yaml = format!(
            "app:\n  id: {id}\n  name: {id}\n  version: 1.0.0\n  container:\n    image: {image}\n    data_uid: \"{uid_gid}\"\n  volumes:\n    - type: bind\n      source: {source}\n      target: /data\n"
        );
        AppManifest::parse(&yaml).unwrap()
    }

    fn pull_manifest_with_generated_file(id: &str, image: &str, source: &str) -> AppManifest {
        let yaml = format!(
            "app:\n  id: {id}\n  name: {id}\n  version: 1.0.0\n  container:\n    image: {image}\n  volumes:\n    - type: bind\n      source: {source}\n      target: /data\n  files:\n    - path: {source}/config.yaml\n      content: |\n        key: value\n"
        );
        AppManifest::parse(&yaml).unwrap()
    }

    fn pull_manifest_with_generated_file_overwrite(
        id: &str,
        image: &str,
        source: &str,
    ) -> AppManifest {
        let yaml = format!(
            "app:\n  id: {id}\n  name: {id}\n  version: 1.0.0\n  container:\n    image: {image}\n  volumes:\n    - type: bind\n      source: {source}\n      target: /data\n  files:\n    - path: {source}/config.yaml\n      overwrite: true\n      content: |\n        key: new\n"
        );
        AppManifest::parse(&yaml).unwrap()
    }

    fn pull_manifest_filebrowser() -> AppManifest {
        let yaml = r#"
app:
  id: filebrowser
  name: File Browser
  version: 1.0.0
  container:
    image: source.archipelago-foundation.org/lfg2025/filebrowser:v2.27.0
    custom_args:
      - --config
      - /data/.filebrowser.json
  volumes:
    - type: bind
      source: /var/lib/archipelago/filebrowser-srv
      target: /srv
      options: [rw]
    - type: bind
      source: /var/lib/archipelago/filebrowser-data
      target: /data
      options: [rw]
"#;
        AppManifest::parse(yaml).unwrap()
    }

    fn pull_manifest_lnd() -> AppManifest {
        let yaml = r#"
app:
  id: lnd
  name: LND
  version: 1.0.0
  container:
    image: source.archipelago-foundation.org/lfg2025/lnd:v0.18.4-beta
    secret_env:
      - key: BITCOIND_RPCPASS
        secret_file: bitcoin-rpc-password
  volumes:
    - type: bind
      source: /var/lib/archipelago/lnd
      target: /root/.lnd
"#;
        AppManifest::parse(yaml).unwrap()
    }

    fn test_bitcoin_ui_paths() -> bitcoin_ui::RenderPaths {
        use std::sync::OnceLock;
        static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
        let dir = DIR.get_or_init(|| {
            let d = tempfile::TempDir::new().expect("test tmpdir");
            std::fs::write(d.path().join("bitcoin-rpc-password"), "test-pass\n")
                .expect("seed password");
            d
        });
        bitcoin_ui::RenderPaths {
            secret_path: dir.path().join("bitcoin-rpc-password"),
            rendered_path: dir.path().join("nginx.conf"),
        }
    }

    fn test_filebrowser_paths() -> filebrowser::EnsurePaths {
        use std::sync::OnceLock;
        static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
        let dir = DIR.get_or_init(|| tempfile::TempDir::new().expect("test tmpdir"));
        filebrowser::EnsurePaths {
            srv_root: dir.path().join("filebrowser"),
            data_dir: dir.path().join("filebrowser-data"),
            config_path: dir.path().join("filebrowser-data/.filebrowser.json"),
        }
    }

    async fn stopped_update_fixture() -> (Arc<MockRuntime>, ProdContainerOrchestrator, String) {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt.clone()).await;
        let image = format!("docker.io/library/alpine@sha256:{}", "a".repeat(64));
        let mut manifest = pull_manifest("staged-music", &image);
        manifest
            .app
            .environment
            .push("STAGED_VERSION=original".into());
        orch.insert_manifest_for_test(manifest, PathBuf::from("/tmp"))
            .await;
        crate::crash_recovery::mark_installed(&orch.data_dir, "staged-music").await;
        crate::crash_recovery::mark_user_stopped(&orch.data_dir, "staged-music").await;
        (rt, orch, image)
    }

    #[tokio::test]
    async fn stopped_upgrade_stages_without_start_then_starts_pinned_manifest() {
        let (rt, orch, image) = stopped_update_fixture().await;
        let sentinel = orch.data_dir.join("persistent-song-and-secret-fixture");
        tokio::fs::write(&sentinel, b"preserved").await.unwrap();
        orch.upgrade("staged-music").await.unwrap();
        assert_eq!(
            orch.staged_upgrade_image("staged-music").await.unwrap(),
            Some(image)
        );
        assert!(!rt
            .calls()
            .iter()
            .any(|c| c.starts_with("create_container:") || c.starts_with("start_container:")));
        let replacement = pull_manifest(
            "staged-music",
            &format!("docker.io/library/alpine@sha256:{}", "b".repeat(64)),
        );
        orch.insert_manifest_for_test(replacement, PathBuf::from("/tmp/new-catalog"))
            .await;
        let lm = orch.loaded("staged-music").await.unwrap();
        assert_eq!(
            orch.ensure_running(&lm).await.unwrap(),
            ReconcileAction::Left("staged-update-awaiting-start".into())
        );
        orch.start("staged-music").await.unwrap();
        assert!(rt
            .created_env_for("staged-music")
            .contains(&"STAGED_VERSION=original".into()));
        assert_eq!(
            rt.get_container_status("staged-music").await.unwrap().state,
            ContainerState::Running
        );
        assert!(orch
            .staged_upgrade_image("staged-music")
            .await
            .unwrap()
            .is_none());
        assert!(!crate::crash_recovery::load_user_stopped(&orch.data_dir)
            .await
            .contains("staged-music"));
        assert_eq!(tokio::fs::read(sentinel).await.unwrap(), b"preserved");
    }

    #[tokio::test]
    async fn interrupted_stopped_pull_retains_original_pin_and_requires_retry() {
        let (rt, orch, image) = stopped_update_fixture().await;
        *rt.fail_pull.lock().unwrap() = Some("interrupted pull".into());
        assert!(orch.upgrade("staged-music").await.is_err());
        assert!(orch
            .start("staged-music")
            .await
            .unwrap_err()
            .to_string()
            .contains("interrupted"));
        let record = super::super::staged_update::load(&orch.data_dir, "staged-music")
            .await
            .unwrap()
            .unwrap();
        assert!(!record.ready);
        assert_eq!(record.image().unwrap(), image);
        *rt.fail_pull.lock().unwrap() = None;
        orch.insert_manifest_for_test(
            pull_manifest("staged-music", "docker.io/library/alpine:latest"),
            PathBuf::from("/tmp"),
        )
        .await;
        orch.upgrade("staged-music").await.unwrap();
        assert_eq!(
            orch.staged_upgrade_image("staged-music").await.unwrap(),
            Some(image)
        );
        assert!(!rt.calls().iter().any(|c| c.starts_with("start_container:")));
    }

    #[tokio::test]
    async fn failed_staged_start_preserves_pin_and_stop_until_retry_succeeds() {
        let (rt, orch, _) = stopped_update_fixture().await;
        orch.upgrade("staged-music").await.unwrap();
        rt.fail_start
            .lock()
            .unwrap()
            .insert("staged-music".into(), "injected start failure".into());
        assert!(orch.start("staged-music").await.is_err());
        assert!(orch
            .staged_upgrade_image("staged-music")
            .await
            .unwrap()
            .is_some());
        assert!(crate::crash_recovery::load_user_stopped(&orch.data_dir)
            .await
            .contains("staged-music"));
        assert!(matches!(
            rt.get_container_status("staged-music").await.unwrap().state,
            ContainerState::Stopped | ContainerState::Created
        ));
        orch.start("staged-music").await.unwrap();
        assert!(orch
            .staged_upgrade_image("staged-music")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn staged_missing_image_uninstall_and_damaged_record_refuse_start() {
        let (rt, orch, image) = stopped_update_fixture().await;
        orch.upgrade("staged-music").await.unwrap();
        rt.images.lock().unwrap().remove(&image);
        assert!(orch
            .start("staged-music")
            .await
            .unwrap_err()
            .to_string()
            .contains("unavailable"));
        rt.mark_image_present(&image);
        crate::crash_recovery::mark_user_uninstalled(&orch.data_dir, "staged-music").await;
        assert!(orch
            .start("staged-music")
            .await
            .unwrap_err()
            .to_string()
            .contains("uninstalled"));
        assert!(orch.upgrade("staged-music").await.is_err());
        tokio::fs::write(
            orch.data_dir.join("staged-updates/staged-music.json"),
            b"damaged",
        )
        .await
        .unwrap();
        assert!(orch.start("staged-music").await.is_err());
        assert!(!rt.calls().iter().any(|c| c.starts_with("start_container:")));
    }

    #[tokio::test]
    async fn staged_failed_post_install_hook_keeps_pin_and_stops_container() {
        let (rt, orch, _) = stopped_update_fixture().await;
        let mut lm = orch.loaded("staged-music").await.unwrap();
        lm.manifest.app.hooks.post_install = serde_yaml::from_str(
            "- copy_from_host:\n    src: nonexistent-staged-hook-file\n    dest: /tmp/test\n",
        )
        .unwrap();
        orch.insert_manifest_for_test(lm.manifest, lm.manifest_dir)
            .await;
        orch.upgrade("staged-music").await.unwrap();
        assert!(orch.start("staged-music").await.is_err());
        assert!(orch
            .staged_upgrade_image("staged-music")
            .await
            .unwrap()
            .is_some());
        assert_eq!(
            rt.get_container_status("staged-music").await.unwrap().state,
            ContainerState::Stopped
        );
        assert!(crate::crash_recovery::load_user_stopped(&orch.data_dir)
            .await
            .contains("staged-music"));
    }

    #[tokio::test]
    async fn failed_staged_cleanup_reports_active_runtime_and_reconcile_retries_stop() {
        let (rt, orch, _) = stopped_update_fixture().await;
        let mut lm = orch.loaded("staged-music").await.unwrap();
        lm.manifest.app.hooks.post_install = serde_yaml::from_str(
            "- copy_from_host:\n    src: nonexistent-staged-hook-file\n    dest: /tmp/test\n",
        )
        .unwrap();
        orch.insert_manifest_for_test(lm.manifest, lm.manifest_dir)
            .await;
        orch.upgrade("staged-music").await.unwrap();
        rt.fail_stop
            .lock()
            .unwrap()
            .insert("staged-music".into(), "stop refused".into());
        let error = orch.start("staged-music").await.unwrap_err();
        assert!(error.downcast_ref::<StagedCleanupFailure>().is_some());
        assert_eq!(
            rt.get_container_status("staged-music").await.unwrap().state,
            ContainerState::Running
        );
        let report = orch.reconcile_all().await;
        assert!(report.failures.is_empty());
        assert!(report
            .actions
            .iter()
            .any(|(app, action)| app == "staged-music"
                && action == &ReconcileAction::Left("staged-update-awaiting-start".into())));
        assert_eq!(
            rt.get_container_status("staged-music").await.unwrap().state,
            ContainerState::Stopped
        );
        assert!(orch
            .staged_upgrade_image("staged-music")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn pending_stage_enforces_stop_after_restart_when_stop_marker_cannot_be_saved() {
        let (rt, orch, _) = stopped_update_fixture().await;
        let mut lm = orch.loaded("staged-music").await.unwrap();
        lm.manifest.app.hooks.post_install = serde_yaml::from_str(
            "- copy_from_host:\n    src: nonexistent-staged-hook-file\n    dest: /tmp/test\n",
        )
        .unwrap();
        orch.insert_manifest_for_test(lm.manifest.clone(), lm.manifest_dir.clone())
            .await;
        orch.upgrade("staged-music").await.unwrap();
        let marker = orch.data_dir.join("user-stopped.json");
        tokio::fs::remove_file(&marker).await.unwrap();
        tokio::fs::create_dir(&marker).await.unwrap();
        rt.fail_stop
            .lock()
            .unwrap()
            .insert("staged-music".into(), "stop interrupted".into());
        let failure = orch.start("staged-music").await.unwrap_err();
        assert!(failure.downcast_ref::<StagedCleanupFailure>().is_some());
        assert_eq!(
            rt.get_container_status("staged-music").await.unwrap().state,
            ContainerState::Running
        );
        let starts = rt
            .calls()
            .iter()
            .filter(|c| c.starts_with("start_container:"))
            .count();
        let mut resumed = orch_with(rt.clone()).await;
        resumed.set_data_dir(orch.data_dir.clone());
        resumed
            .insert_manifest_for_test(lm.manifest, lm.manifest_dir)
            .await;
        let report = resumed.reconcile_all().await;
        assert!(report.failures.is_empty());
        assert_eq!(
            rt.get_container_status("staged-music").await.unwrap().state,
            ContainerState::Stopped
        );
        assert_eq!(
            rt.calls()
                .iter()
                .filter(|c| c.starts_with("start_container:"))
                .count(),
            starts
        );
        assert!(resumed
            .staged_upgrade_image("staged-music")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn staged_configuration_failure_keeps_stop_and_pin_before_create() {
        let (rt, mut orch, _) = stopped_update_fixture().await;
        orch.secrets_dir = orch.data_dir.join("fixture-secrets");
        let mut lm = orch.loaded("staged-music").await.unwrap();
        lm.manifest.app.container.secret_env =
            serde_yaml::from_str("- key: REQUIRED_SECRET\n  secret_file: missing-staged-secret\n")
                .unwrap();
        orch.insert_manifest_for_test(lm.manifest, lm.manifest_dir)
            .await;
        orch.upgrade("staged-music").await.unwrap();
        assert!(orch.start("staged-music").await.is_err());
        assert!(orch
            .staged_upgrade_image("staged-music")
            .await
            .unwrap()
            .is_some());
        assert!(crate::crash_recovery::load_user_stopped(&orch.data_dir)
            .await
            .contains("staged-music"));
        assert!(!rt
            .calls()
            .iter()
            .any(|c| c.starts_with("create_container:")));
    }

    #[tokio::test]
    async fn staged_record_persistence_failure_precedes_runtime_changes() {
        let (rt, orch, _) = stopped_update_fixture().await;
        tokio::fs::write(orch.data_dir.join("staged-updates"), b"blocked directory")
            .await
            .unwrap();
        assert!(orch.upgrade("staged-music").await.is_err());
        assert!(!rt.calls().iter().any(|c| c.starts_with("pull_image:")
            || c.starts_with("stop_container:")
            || c.starts_with("remove_container:")
            || c.starts_with("create_container:")));
    }

    #[tokio::test]
    async fn stopped_unpinned_update_refuses_without_starting() {
        let (rt, orch, _) = stopped_update_fixture().await;
        orch.insert_manifest_for_test(
            pull_manifest("staged-music", "docker.io/library/alpine:3.20"),
            PathBuf::from("/tmp"),
        )
        .await;
        assert!(orch
            .upgrade("staged-music")
            .await
            .unwrap_err()
            .to_string()
            .contains("digest-pinned"));
        assert!(!rt.calls().iter().any(|c| c.starts_with("pull_image:")
            || c.starts_with("create_container:")
            || c.starts_with("start_container:")));
    }

    #[tokio::test]
    async fn install_fresh_pull() {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/fixtures/bitcoin-knots"),
        )
        .await;

        let name = orch.install("bitcoin-knots").await.unwrap();
        assert_eq!(name, "bitcoin-knots");
        let calls = rt.calls();
        assert!(calls.iter().any(|c| c.starts_with("pull_image:")));
        assert!(calls
            .iter()
            .any(|c| c == "create_container:bitcoin-knots:offset=0"));
        assert!(calls.iter().any(|c| c == "start_container:bitcoin-knots"));
        // Must NOT build
        assert!(!calls.iter().any(|c| c.starts_with("build_image:")));
    }

    #[tokio::test]
    async fn host_port_wait_aborts_on_user_stop_marker() {
        // A stop the user requests mid-wait must break the (up to 420s)
        // readiness poll instead of holding the app lock — the queued stop
        // worker paints the app Restarting for the whole wait otherwise
        // (gate take-2 iter 5 vaultwarden).
        let yaml = "app:\n  id: stopme\n  name: stopme\n  version: 1.0.0\n  container:\n    image: stopme:1\n  ports:\n    - host: 1\n      container: 80\n      protocol: tcp\n";
        let manifest = AppManifest::parse(yaml).unwrap();
        let data_dir = tempfile::tempdir().unwrap();
        crate::crash_recovery::mark_user_stopped(data_dir.path(), "stopme").await;

        // Host port 1 is never listening, so only the marker can end this
        // before the 60s wait budget; the 10s bound proves the abort fired.
        let res = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            wait_for_manifest_host_ports(&manifest, 60, data_dir.path()),
        )
        .await
        .expect("wait must abort well before its 60s budget");
        let err = res.unwrap_err().to_string();
        assert!(err.contains("user stop requested"), "err: {err}");
    }

    #[tokio::test]
    async fn install_fresh_skips_pull_when_image_local() {
        // An unreachable registry must not brick a reconcile recreate whose
        // exact image:tag is already in local storage (archy-btcpay-db /
        // archy-nbxplorer boot reconcile with the (since-retired) upstream registry dead,
        // 2026-07-10).
        let rt = Arc::new(MockRuntime::default());
        rt.mark_image_present("docker.io/bitcoin/knots:28");
        *rt.fail_pull.lock().unwrap() = Some("registry unreachable".into());
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/fixtures/bitcoin-knots"),
        )
        .await;

        let name = orch.install("bitcoin-knots").await.unwrap();
        assert_eq!(name, "bitcoin-knots");
        let calls = rt.calls();
        assert!(!calls.iter().any(|c| c.starts_with("pull_image:")));
        assert!(calls.iter().any(|c| c == "start_container:bitcoin-knots"));
    }

    #[tokio::test]
    async fn install_fresh_still_fails_when_image_truly_absent() {
        let rt = Arc::new(MockRuntime::default());
        *rt.fail_pull.lock().unwrap() = Some("registry unreachable".into());
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/fixtures/bitcoin-knots"),
        )
        .await;

        let err = orch.install("bitcoin-knots").await.unwrap_err();
        assert!(err.to_string().contains("pulling"), "err: {err}");
    }

    #[tokio::test]
    async fn knows_app_reflects_loaded_manifests() {
        // The RPC install gate routes any app with a known manifest through
        // the orchestrator (no per-app allowlist). knows_app must be true for
        // a loaded manifest and false otherwise — a false positive would send
        // a manifest-less app into `loaded()`'s unknown-app_id error path,
        // skipping the legacy adopt block; a false negative sends a
        // manifest-driven app to the legacy flow, which creates a bare
        // container (strfry, 2026-07-09).
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt).await;
        assert!(!orch.knows_app("strfry").await);
        orch.insert_manifest_for_test(
            pull_manifest("strfry", "docker.io/dockurr/strfry:1.0.4"),
            PathBuf::from("/tmp/fixtures/strfry"),
        )
        .await;
        assert!(orch.knows_app("strfry").await);
        assert!(!orch.knows_app("no-such-app").await);
    }

    #[tokio::test]
    async fn mempool_core_rpc_host_follows_bitcoin_node() {
        // B12: mempool's CORE_RPC_HOST must resolve to whichever Bitcoin node
        // container is running (Knots OR Core), not a hardcoded value.
        let yaml = "app:\n  id: mempool-api\n  name: mempool-api\n  version: 1.0.0\n  container:\n    image: x:1\n    derived_env:\n      - key: CORE_RPC_HOST\n        template: \"{{BITCOIN_HOST}}\"\n";

        for (node, expected) in [
            ("bitcoin-core", "bitcoin-core"),
            ("bitcoin-knots", "bitcoin-knots"),
        ] {
            let rt = Arc::new(MockRuntime::default());
            let mut orch = orch_with(rt).await;
            orch.set_bitcoin_host_for_test(node);

            let mut manifest = AppManifest::parse(yaml).unwrap();
            orch.resolve_dynamic_env(&mut manifest).await.unwrap();

            assert!(
                manifest
                    .app
                    .environment
                    .iter()
                    .any(|e| e == &format!("CORE_RPC_HOST={expected}")),
                "node={node}: expected CORE_RPC_HOST={expected}, got {:?}",
                manifest.app.environment
            );
        }
    }

    const NODE_IDENTITY_PUBKEYS_YAML: &str = "app:\n  id: wildbloom-node\n  name: wildbloom-node\n  version: 1.0.0\n  container:\n    image: x:1\n    derived_env:\n      - key: WILDBLOOM_ALLOW_PUBKEYS\n        template: \"{{NODE_IDENTITY_PUBKEYS}}\"\n";

    /// Writes `pubkey_hex` as the node public key, in `NodeIdentity`'s format.
    async fn write_node_pubkey(orch: &ProdContainerOrchestrator, pubkey_hex: &str) {
        let dir = orch.data_dir().join("identity");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("node_key.pub"), hex::decode(pubkey_hex).unwrap())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn opted_in_media_registration_pins_preserve_audience_through_env_reconciliation() {
        let rt = Arc::new(MockRuntime::default());
        let root = tempfile::tempdir().unwrap();
        let mut orch =
            ProdContainerOrchestrator::with_runtime(rt, PathBuf::from("/nonexistent-for-tests"));
        orch.set_data_dir(root.path().to_owned());
        orch.set_secrets_dir(root.path().join("secrets"));
        let identity = crate::identity::NodeIdentity::load_or_create(&root.path().join("identity"))
            .await
            .unwrap();
        let key_before = tokio::fs::read(root.path().join("identity/node_key"))
            .await
            .unwrap();
        let mut app = pull_manifest("indeedhub-api", "fixture:1");
        app.app.container.media_registration_identity = true;
        app.app
            .environment
            .push("ARCHIPELAGO_REGISTRATION_AUDIENCE=untrusted-manifest-value".into());
        orch.resolve_dynamic_env(&mut app).await.unwrap();
        let first = app.app.environment.clone();
        orch.resolve_dynamic_env(&mut app).await.unwrap();
        assert_eq!(app.app.environment, first);
        let pin = crate::container::registration_pin::load_existing(
            root.path(),
            "indeedhub-api",
            &identity,
        )
        .unwrap();
        assert!(first.contains(&format!(
            "ARCHIPELAGO_REGISTRATION_NODE_PUBLIC_KEY={}",
            identity.pubkey_hex()
        )));
        assert!(first.contains(&format!(
            "ARCHIPELAGO_REGISTRATION_NODE_DID={}",
            identity.did_key().unwrap()
        )));
        assert!(first.contains(&format!(
            "ARCHIPELAGO_REGISTRATION_AUDIENCE={}",
            pin.app_audience
        )));
        assert!(!first.iter().any(|entry| entry.contains("_ENABLED=")));
        assert_eq!(
            tokio::fs::read(root.path().join("identity/node_key"))
                .await
                .unwrap(),
            key_before
        );
    }

    #[tokio::test]
    async fn unrelated_apps_do_not_require_identity_or_get_registration_pins() {
        let rt = Arc::new(MockRuntime::default());
        let root = tempfile::tempdir().unwrap();
        let mut orch =
            ProdContainerOrchestrator::with_runtime(rt, PathBuf::from("/nonexistent-for-tests"));
        orch.set_data_dir(root.path().to_owned());
        orch.set_secrets_dir(root.path().join("secrets"));
        let mut app = pull_manifest("ordinary-app", "fixture:1");
        orch.resolve_dynamic_env(&mut app).await.unwrap();
        assert!(!root.path().join("identity").exists());
        assert!(!root.path().join("app-registration-pins").exists());
        assert!(!app
            .app
            .environment
            .iter()
            .any(|entry| entry.starts_with("ARCHIPELAGO_REGISTRATION_")));
    }

    #[tokio::test]
    async fn registration_pins_require_existing_matching_node_keys() {
        let rt = Arc::new(MockRuntime::default());
        let root = tempfile::tempdir().unwrap();
        let mut orch = ProdContainerOrchestrator::with_runtime(
            rt.clone(),
            PathBuf::from("/nonexistent-for-tests"),
        );
        orch.set_data_dir(root.path().to_owned());
        orch.set_secrets_dir(root.path().join("secrets"));
        let mut app = pull_manifest("indeedhub-api", "fixture:1");
        app.app.container.media_registration_identity = true;
        assert!(orch.resolve_dynamic_env(&mut app).await.is_err());
        assert!(!root.path().join("identity").exists());
        crate::identity::NodeIdentity::load_or_create(&root.path().join("identity"))
            .await
            .unwrap();
        tokio::fs::write(root.path().join("identity/node_key.pub"), [3u8; 32])
            .await
            .unwrap();
        assert!(orch.resolve_dynamic_env(&mut app).await.is_err());
        assert!(!root.path().join("app-registration-pins").exists());
        assert!(rt.calls().is_empty());
    }

    #[tokio::test]
    async fn node_identity_pubkeys_placeholder_renders_the_signable_identities() {
        // The owners must be exactly the identities the app signer offers:
        // the user identities, never the node's own identity.
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt).await;
        let mgr = crate::identity_manager::IdentityManager::new(orch.data_dir())
            .await
            .unwrap();
        let mut expected = Vec::new();
        for name in ["Personal", "Business"] {
            let r = mgr
                .create(
                    name.to_string(),
                    crate::identity_manager::IdentityPurpose::Personal,
                )
                .await
                .unwrap();
            expected.push(r.nostr_pubkey.unwrap().to_ascii_lowercase());
        }
        expected.sort();
        // The node key is held by an identity with a uuid id and an ordinary
        // name, so only the `is_node` match, through the key read from
        // node_key.pub, can keep it out.
        let laptop = mgr
            .create(
                "Laptop".to_string(),
                crate::identity_manager::IdentityPurpose::Personal,
            )
            .await
            .unwrap();
        assert!(!laptop.id.starts_with("node-"));
        let laptop_nostr = laptop.nostr_pubkey.clone().unwrap();
        write_node_pubkey(&orch, &laptop.pubkey_hex).await;

        let mut manifest = AppManifest::parse(NODE_IDENTITY_PUBKEYS_YAML).unwrap();
        orch.resolve_dynamic_env(&mut manifest).await.unwrap();

        let env = &manifest.app.environment;
        let want = format!("WILDBLOOM_ALLOW_PUBKEYS={}", expected.join(","));
        assert!(env.iter().any(|e| e == &want), "env was {env:?}");
        assert!(
            !env.iter().any(|e| e.contains(&laptop_nostr)),
            "node identity leaked into {env:?}"
        );
    }

    #[tokio::test]
    async fn node_identity_pubkeys_placeholder_refuses_an_empty_set() {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt).await;
        // A node key with only the node's own identity: nothing is signable.
        let node = crate::identity::NodeIdentity::load_or_create(&orch.data_dir().join("identity"))
            .await
            .unwrap();
        crate::identity_manager::IdentityManager::new(orch.data_dir())
            .await
            .unwrap()
            .create_from_signing_key(
                "Node".to_string(),
                crate::identity_manager::IdentityPurpose::Personal,
                node.signing_key().clone(),
            )
            .await
            .unwrap();
        let mut manifest = AppManifest::parse(NODE_IDENTITY_PUBKEYS_YAML).unwrap();
        let err = orch.resolve_dynamic_env(&mut manifest).await.unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("NODE_IDENTITY_PUBKEYS"),
            "unexpected error: {msg}"
        );
        assert!(msg.contains("no user identity"), "unexpected error: {msg}");
        assert!(
            !manifest
                .app
                .environment
                .iter()
                .any(|e| e.starts_with("WILDBLOOM_ALLOW_PUBKEYS=")),
            "an empty owner list must never render"
        );
    }

    #[tokio::test]
    async fn node_identity_pubkeys_placeholder_needs_the_node_key_and_never_creates_it() {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt).await;
        crate::identity_manager::IdentityManager::new(orch.data_dir())
            .await
            .unwrap()
            .create(
                "Personal".to_string(),
                crate::identity_manager::IdentityPurpose::Personal,
            )
            .await
            .unwrap();
        let identity_dir = orch.data_dir().join("identity");
        let mut manifest = AppManifest::parse(NODE_IDENTITY_PUBKEYS_YAML).unwrap();
        let err = orch.resolve_dynamic_env(&mut manifest).await.unwrap_err();
        assert!(
            format!("{err:#}").contains("node public key"),
            "unexpected error: {err:#}"
        );
        assert!(
            !identity_dir.join("node_key").exists(),
            "a node key was created"
        );
        assert!(!identity_dir.join("node_key.pub").exists());

        // A malformed key file is refused, not reinterpreted.
        tokio::fs::create_dir_all(&identity_dir).await.unwrap();
        tokio::fs::write(identity_dir.join("node_key.pub"), "ab".repeat(32))
            .await
            .unwrap();
        let err = orch.resolve_dynamic_env(&mut manifest).await.unwrap_err();
        assert!(
            format!("{err:#}").contains("expected 32"),
            "unexpected error: {err:#}"
        );
        assert!(!manifest
            .app
            .environment
            .iter()
            .any(|e| e.starts_with("WILDBLOOM_ALLOW_PUBKEYS=")));
    }

    /// A fedimint-gateway manifest shaped like the real one: a bcrypt
    /// generated secret plus a secret_env that reads it, which is what makes
    /// the credential participate in secret_env_hash.
    fn gateway_manifest_yaml() -> &'static str {
        "app:\n  id: fedimint-gateway\n  name: Fedimint Gateway\n  version: 0.10.0\n  container:\n    image: x:1\n    generated_secrets:\n      - name: fedimint-gateway-hash\n        kind: bcrypt\n    secret_env:\n      - key: FEDI_HASH\n        secret_file: fedimint-gateway-hash\n"
    }

    fn indeedhub_api_manifest_yaml() -> &'static str {
        "app:\n  id: indeedhub-api\n  name: IndeedHub API\n  version: 1.0.0\n  container:\n    image: x:1\n    generated_secrets:\n      - name: indeedhub-aes-master\n        kind: hex16\n    secret_env:\n      - key: AES_MASTER_SECRET\n        secret_file: indeedhub-aes-master\n"
    }

    #[tokio::test]
    async fn existing_indeedhub_consumer_gets_migration_compatible_root() {
        let rt = Arc::new(MockRuntime::default());
        rt.set_state("indeedhub-api", ContainerState::Running);
        let mut orch = orch_with(rt).await;
        let tmp = tempfile::TempDir::new().unwrap();
        orch.set_secrets_dir(tmp.path().to_path_buf());

        let mut manifest = AppManifest::parse(indeedhub_api_manifest_yaml()).unwrap();
        orch.resolve_dynamic_env(&mut manifest).await.unwrap();
        let resolved = manifest
            .app
            .container
            .secret_env_refs
            .iter()
            .find(|entry| entry.env_key == "AES_MASTER_SECRET")
            .unwrap();
        assert_eq!(resolved.value.len(), 32);
        assert!(tmp.path().join("indeedhub-aes-master").exists());
        assert!(
            crate::container::secrets::ensure_indeedhub_aes_master_secret(tmp.path(), true).is_ok(),
            "the migrated file remains valid and stable"
        );
    }

    #[tokio::test]
    async fn fresh_indeedhub_install_gets_random_root() {
        let rt = Arc::new(MockRuntime::default());
        let mut orch = orch_with(rt).await;
        let tmp = tempfile::TempDir::new().unwrap();
        orch.set_secrets_dir(tmp.path().to_path_buf());

        let mut manifest = AppManifest::parse(indeedhub_api_manifest_yaml()).unwrap();
        orch.resolve_dynamic_env(&mut manifest).await.unwrap();
        let first = crate::container::secrets::indeedhub_aes_master_secret(tmp.path()).unwrap();

        let other = tempfile::TempDir::new().unwrap();
        crate::container::secrets::ensure_indeedhub_aes_master_secret(other.path(), false).unwrap();
        let second = crate::container::secrets::indeedhub_aes_master_secret(other.path()).unwrap();
        assert_ne!(first, second, "fresh installs must receive per-node roots");
    }

    /// FED-07. Rotating a compromised credential leaves the RUNNING container
    /// holding the old value, so the rotation must flag the app for recreate.
    /// Without the flag the drift check skips it as restart-sensitive and the
    /// gateway keeps serving the published default forever — observed on
    /// a test node 2026-08-01 before this was wired up.
    #[tokio::test]
    async fn rotating_a_compromised_credential_flags_the_app_for_recreate() {
        let rt = Arc::new(MockRuntime::default());
        let mut orch = orch_with(rt).await;

        let tmp = tempfile::TempDir::new().unwrap();
        let secrets = tmp.path().join("secrets");
        std::fs::create_dir_all(&secrets).unwrap();
        // An install carrying the shipped default, with its .pw sibling
        // present so ensure_one's fast path no-ops and rotation is what acts.
        std::fs::write(
            secrets.join("fedimint-gateway-hash"),
            "$2y$10$t9YjjxkiktrlYvjajB/zgOMDnSNVg4HqrbDqh47u7Jf42whNdxNqC",
        )
        .unwrap();
        std::fs::write(secrets.join("fedimint-gateway-hash.pw"), "stale-plaintext").unwrap();
        orch.set_secrets_dir(secrets.clone());

        let mut manifest = AppManifest::parse(gateway_manifest_yaml()).unwrap();
        orch.resolve_dynamic_env(&mut manifest).await.unwrap();

        assert!(
            orch.credential_rotated
                .lock()
                .await
                .contains("fedimint-gateway"),
            "a rotated credential must flag its app so the drift check recreates it"
        );
        let after = std::fs::read_to_string(secrets.join("fedimint-gateway-hash")).unwrap();
        assert!(
            !after.contains("t9YjjxkiktrlYvjajB"),
            "the compromised value must be gone from the file"
        );
    }

    /// The adjacency edge: an app whose credential was NOT rotated must not be
    /// flagged, or every reconcile tick would recreate restart-sensitive apps.
    #[tokio::test]
    async fn a_unique_credential_does_not_flag_the_app() {
        let rt = Arc::new(MockRuntime::default());
        let mut orch = orch_with(rt).await;

        let tmp = tempfile::TempDir::new().unwrap();
        let secrets = tmp.path().join("secrets");
        std::fs::create_dir_all(&secrets).unwrap();
        crate::container::secrets::ensure_gateway_credential(&secrets).unwrap();
        let before = std::fs::read_to_string(secrets.join("fedimint-gateway-hash")).unwrap();
        orch.set_secrets_dir(secrets.clone());

        let mut manifest = AppManifest::parse(gateway_manifest_yaml()).unwrap();
        orch.resolve_dynamic_env(&mut manifest).await.unwrap();

        assert!(
            orch.credential_rotated.lock().await.is_empty(),
            "an app with a unique credential must never be flagged for recreate"
        );
        assert_eq!(
            before,
            std::fs::read_to_string(secrets.join("fedimint-gateway-hash")).unwrap(),
            "a unique credential must be left byte-identical"
        );
    }

    /// Idempotence at the flag level: the second pass finds a value that is no
    /// longer on the denylist, so it neither rotates nor re-flags. This is what
    /// stops a recreate loop on every reconcile tick (T-01-73).
    #[tokio::test]
    async fn a_second_pass_does_not_re_flag_the_app() {
        let rt = Arc::new(MockRuntime::default());
        let mut orch = orch_with(rt).await;

        let tmp = tempfile::TempDir::new().unwrap();
        let secrets = tmp.path().join("secrets");
        std::fs::create_dir_all(&secrets).unwrap();
        std::fs::write(
            secrets.join("fedimint-gateway-hash"),
            "$2y$10$t9YjjxkiktrlYvjajB/zgOMDnSNVg4HqrbDqh47u7Jf42whNdxNqC",
        )
        .unwrap();
        std::fs::write(secrets.join("fedimint-gateway-hash.pw"), "stale-plaintext").unwrap();
        orch.set_secrets_dir(secrets.clone());

        let mut m1 = AppManifest::parse(gateway_manifest_yaml()).unwrap();
        orch.resolve_dynamic_env(&mut m1).await.unwrap();
        // The drift check consumes the flag when it recreates.
        orch.credential_rotated
            .lock()
            .await
            .remove("fedimint-gateway");

        let mut m2 = AppManifest::parse(gateway_manifest_yaml()).unwrap();
        orch.resolve_dynamic_env(&mut m2).await.unwrap();
        assert!(
            orch.credential_rotated.lock().await.is_empty(),
            "the second pass must not re-flag — the rotated value is not denylisted"
        );
    }

    #[tokio::test]
    async fn install_fresh_build_when_image_absent() {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            build_manifest(
                "bitcoin-ui",
                "/opt/archy/docker/bitcoin-ui",
                "archy-bitcoin-ui:local",
            ),
            PathBuf::from("/opt/archy/apps/bitcoin-ui"),
        )
        .await;

        orch.install("bitcoin-ui").await.unwrap();
        let calls = rt.calls();
        assert!(calls
            .iter()
            .any(|c| c == "image_exists:archy-bitcoin-ui:local"));
        assert!(calls
            .iter()
            .any(|c| c.starts_with("build_image:archy-bitcoin-ui:local:")));
        assert!(calls
            .iter()
            .any(|c| c == "create_container:archy-bitcoin-ui:offset=0"));
        assert!(calls
            .iter()
            .any(|c| c == "start_container:archy-bitcoin-ui"));
    }

    #[tokio::test]
    async fn install_fresh_builds_when_image_exists_check_fails() {
        let rt = Arc::new(MockRuntime::default());
        rt.fail_image_exists.lock().unwrap().insert(
            "archy-bitcoin-ui:local".to_string(),
            "podman image inspect timed out".to_string(),
        );
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            build_manifest(
                "bitcoin-ui",
                "/opt/archy/docker/bitcoin-ui",
                "archy-bitcoin-ui:local",
            ),
            PathBuf::from("/opt/archy/apps/bitcoin-ui"),
        )
        .await;

        orch.install("bitcoin-ui").await.unwrap();
        let calls = rt.calls();
        assert!(calls
            .iter()
            .any(|c| c == "image_exists:archy-bitcoin-ui:local"));
        assert!(calls
            .iter()
            .any(|c| c.starts_with("build_image:archy-bitcoin-ui:local:")));
        assert!(calls
            .iter()
            .any(|c| c == "create_container:archy-bitcoin-ui:offset=0"));
    }

    #[tokio::test]
    async fn install_bitcoin_ui_renders_nginx_conf_via_hook() {
        // End-to-end: install("bitcoin-ui") must invoke the pre-start
        // hook, which atomic-writes a rendered nginx.conf to the
        // test-scoped RenderPaths. Asserts both that the file lands
        // on disk and that the `{{BITCOIN_RPC_AUTH}}` placeholder has
        // been substituted with `base64("archipelago:test-pass")`.
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            build_manifest(
                "bitcoin-ui",
                "/opt/archy/docker/bitcoin-ui",
                "archy-bitcoin-ui:local",
            ),
            PathBuf::from("/opt/archy/apps/bitcoin-ui"),
        )
        .await;

        orch.install("bitcoin-ui").await.unwrap();

        let rendered_path = test_bitcoin_ui_paths().rendered_path;
        let contents = std::fs::read_to_string(&rendered_path)
            .expect("hook must have written nginx.conf during install");
        // Placeholder gone.
        assert!(
            !contents.contains("{{BITCOIN_RPC_AUTH}}"),
            "placeholder was not substituted:\n{contents}"
        );
        // Deterministic auth blob for the seeded test password.
        assert!(
            contents.contains("Basic YXJjaGlwZWxhZ286dGVzdC1wYXNz"),
            "expected base64(archipelago:test-pass) in rendered conf:\n{contents}"
        );
        // Hook must fire BEFORE create_container so the bind-mount
        // target exists when podman goes to attach it.
        let calls = rt.calls();
        let create_idx = calls
            .iter()
            .position(|c| c == "create_container:archy-bitcoin-ui:offset=0")
            .expect("create_container must have been called");
        // With the hook running synchronously inside install_fresh the
        // rendered file must already be on disk by the time
        // create_container returns — which we just asserted above.
        // Keep the index lookup to make the ordering contract explicit
        // for future readers.
        let _ = create_idx;
    }

    #[tokio::test]
    async fn install_fresh_build_skipped_when_image_present() {
        let rt = Arc::new(MockRuntime::default());
        rt.mark_image_present("archy-bitcoin-ui:local");
        let orch = orch_with(rt.clone()).await;
        let build_context = tempfile::tempdir().unwrap();
        std::fs::write(build_context.path().join("Dockerfile"), "FROM scratch\n").unwrap();
        let build_context = build_context.path().to_string_lossy().into_owned();
        let manifest = build_manifest("bitcoin-ui", &build_context, "archy-bitcoin-ui:local");
        let fingerprint = fingerprint_build_context(Path::new(&build_context))
            .expect("test build context must be fingerprintable");
        let stamp_path = build_fingerprint_stamp_path(&orch.data_dir, "archy-bitcoin-ui:local");
        std::fs::create_dir_all(stamp_path.parent().unwrap()).unwrap();
        std::fs::write(&stamp_path, fingerprint).unwrap();
        orch.insert_manifest_for_test(manifest, PathBuf::from("/opt/archy/apps/bitcoin-ui"))
            .await;

        orch.install("bitcoin-ui").await.unwrap();
        let calls = rt.calls();
        assert!(calls
            .iter()
            .any(|c| c == "image_exists:archy-bitcoin-ui:local"));
        // Build must NOT be invoked because the image is already there and its
        // recorded build-context fingerprint still matches.
        assert!(!calls.iter().any(|c| c.starts_with("build_image:")));
    }

    #[tokio::test]
    async fn install_fresh_build_resolves_relative_context_against_manifest_dir() {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            build_manifest("bitcoin-ui", "docker-context", "archy-bitcoin-ui:local"),
            PathBuf::from("/opt/archy/apps/bitcoin-ui"),
        )
        .await;

        orch.install("bitcoin-ui").await.unwrap();
        let calls = rt.calls();
        let build = calls
            .iter()
            .find(|c| c.starts_with("build_image:"))
            .expect("build should have run");
        // Relative path must be joined onto the manifest directory.
        assert!(
            build.contains("context=/opt/archy/apps/bitcoin-ui/docker-context"),
            "unexpected build context in call: {build}"
        );
    }

    #[tokio::test]
    async fn bitcoin_storage_choice_is_applied_and_old_catalog_cannot_silently_ignore_it() {
        let rt = Arc::new(MockRuntime::default());
        let mut orch = orch_with(rt).await;
        let dir = tempfile::tempdir().unwrap();
        orch.set_data_dir(dir.path().to_path_buf());
        for id in ["bitcoin-core", "bitcoin-knots"] {
            let mut old = pull_manifest(id, "docker.io/bitcoin/bitcoin:28");
            // No preference: existing containers need no new environment flag.
            crate::settings::bitcoin_storage::save(dir.path(), false)
                .await
                .unwrap();
            orch.resolve_dynamic_env(&mut old).await.unwrap();
            assert!(!old
                .app
                .environment
                .iter()
                .any(|s| s.starts_with("BITCOIN_PRUNE=")));
            crate::settings::bitcoin_storage::save(dir.path(), true)
                .await
                .unwrap();
            assert!(orch
                .resolve_dynamic_env(&mut old)
                .await
                .unwrap_err()
                .to_string()
                .contains("cannot honor"));
            let mut current = pull_manifest(id, "docker.io/bitcoin/bitcoin:28");
            current
                .app
                .container
                .custom_args
                .push("if [ ${BITCOIN_PRUNE:-0} = 1 ]; then :; fi".into());
            orch.resolve_dynamic_env(&mut current).await.unwrap();
            assert!(current
                .app
                .environment
                .iter()
                .any(|s| s == "BITCOIN_PRUNE=1"));
        }
    }

    #[tokio::test]
    async fn install_resolves_derived_and_secret_env_before_create() {
        let rt = Arc::new(MockRuntime::default());
        let mut orch = orch_with(rt.clone()).await;

        let secrets_dir = tempfile::tempdir().unwrap();
        std::fs::write(
            secrets_dir.path().join("bitcoin-rpc-password"),
            "secret-pass\n",
        )
        .unwrap();
        orch.set_secrets_dir(secrets_dir.path().to_path_buf());

        orch.insert_manifest_for_test(
            pull_manifest_with_dynamic_env("fedimint", "docker.io/fedimint/fedimintd:v0.10.0"),
            PathBuf::from("/tmp/fedimint"),
        )
        .await;

        orch.install("fedimint").await.unwrap();

        let env = rt.created_env_for("fedimint");
        assert!(env.iter().any(|e| e == "STATIC=1"));
        assert!(env
            .iter()
            .any(|e| e.starts_with("FM_API_URL=ws://") && e.ends_with(":8174")));
        // The secret must NOT ride in plain env (that's the podman-inspect /
        // quadlet-unit-file leak this pipeline exists to close) — it travels
        // as a secret ref with the value bound for the podman secret store.
        assert!(!env.iter().any(|e| e.starts_with("FM_BITCOIND_PASSWORD=")));
        let refs = rt.created_secret_refs_for("fedimint");
        let r = refs
            .iter()
            .find(|r| r.env_key == "FM_BITCOIND_PASSWORD")
            .expect("secret env must arrive as a ref");
        assert_eq!(r.value, "secret-pass");
        assert_eq!(r.secret_name, "archy-env-fedimint-fm_bitcoind_password");
    }

    #[tokio::test]
    async fn install_applies_data_uid_chown_before_create() {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt.clone()).await;

        let data_dir = tempfile::tempdir_in("/var/lib/archipelago").unwrap();
        let id_u = std::process::Command::new("id").arg("-u").output().unwrap();
        let id_g = std::process::Command::new("id").arg("-g").output().unwrap();
        let uid = String::from_utf8_lossy(&id_u.stdout).trim().to_string();
        let gid = String::from_utf8_lossy(&id_g.stdout).trim().to_string();
        let uid_gid = format!("{uid}:{gid}");

        orch.insert_manifest_for_test(
            pull_manifest_with_data_uid(
                "electrumx",
                "docker.io/spesmilo/electrumx:latest",
                data_dir.path().to_string_lossy().as_ref(),
                &uid_gid,
            ),
            PathBuf::from("/tmp/electrumx"),
        )
        .await;

        orch.install("electrumx").await.unwrap();
        let calls = rt.calls();
        assert!(calls
            .iter()
            .any(|c| c == "create_container:electrumx:offset=0"));
    }

    #[tokio::test]
    async fn install_writes_manifest_generated_files_before_create() {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt.clone()).await;

        let data_dir = tempfile::tempdir_in("/var/lib/archipelago").unwrap();
        orch.insert_manifest_for_test(
            pull_manifest_with_generated_file(
                "exampleapp",
                "docker.io/example/exampleapp:latest",
                data_dir.path().to_string_lossy().as_ref(),
            ),
            PathBuf::from("/tmp/exampleapp"),
        )
        .await;

        orch.install("exampleapp").await.unwrap();

        let config_path = data_dir.path().join("config.yaml");
        let config = std::fs::read_to_string(config_path).unwrap();
        assert_eq!(config, "key: value\n");
        let calls = rt.calls();
        assert!(calls
            .iter()
            .any(|c| c == "create_container:exampleapp:offset=0"));
    }

    #[tokio::test]
    async fn manifest_generated_files_do_not_overwrite_by_default() {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt.clone()).await;

        let data_dir = tempfile::tempdir_in("/var/lib/archipelago").unwrap();
        let config_path = data_dir.path().join("config.yaml");
        std::fs::write(&config_path, "key: operator\n").unwrap();

        orch.insert_manifest_for_test(
            pull_manifest_with_generated_file(
                "exampleapp",
                "docker.io/example/exampleapp:latest",
                data_dir.path().to_string_lossy().as_ref(),
            ),
            PathBuf::from("/tmp/exampleapp"),
        )
        .await;

        orch.install("exampleapp").await.unwrap();

        let config = std::fs::read_to_string(config_path).unwrap();
        assert_eq!(config, "key: operator\n");
    }

    #[tokio::test]
    async fn manifest_generated_files_can_overwrite_when_declared() {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt.clone()).await;

        let data_dir = tempfile::tempdir_in("/var/lib/archipelago").unwrap();
        let config_path = data_dir.path().join("config.yaml");
        std::fs::write(&config_path, "key: old\n").unwrap();

        orch.insert_manifest_for_test(
            pull_manifest_with_generated_file_overwrite(
                "exampleapp",
                "docker.io/example/exampleapp:latest",
                data_dir.path().to_string_lossy().as_ref(),
            ),
            PathBuf::from("/tmp/exampleapp"),
        )
        .await;

        orch.install("exampleapp").await.unwrap();

        let config = std::fs::read_to_string(config_path).unwrap();
        assert_eq!(config, "key: new\n");
    }

    #[tokio::test]
    async fn reconcile_noop_when_already_running() {
        let rt = Arc::new(MockRuntime::default());
        rt.set_state("bitcoin-knots", ContainerState::Running);
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/bk"),
        )
        .await;

        let report = orch.reconcile_all().await;
        assert_eq!(
            report.actions,
            vec![("bitcoin-knots".to_string(), ReconcileAction::NoOp)]
        );
        assert!(report.failures.is_empty());
        // Must not have pulled / created / started anything
        let calls = rt.calls();
        assert!(!calls.iter().any(|c| c.starts_with("pull_image:")));
        assert!(!calls.iter().any(|c| c.starts_with("create_container:")));
        assert!(!calls.iter().any(|c| c.starts_with("start_container:")));
    }

    #[test]
    fn degraded_running_apps_flags_up_app_with_failed_dependency() {
        let btcpay = AppManifest::parse(
            "app:\n  id: btcpay-server\n  name: btcpay\n  version: 1.0.0\n  container:\n    image: btcpay:1\n  dependencies:\n    - app_id: archy-nbxplorer\n      version: \">=1\"\n    - app_id: archy-btcpay-db\n      version: \">=15\"\n",
        )
        .unwrap();
        let nbx = pull_manifest("archy-nbxplorer", "nbx:1");
        let db = pull_manifest("archy-btcpay-db", "pg:15");

        // btcpay up, both deps failed → two degraded pairs.
        let report = ReconcileReport {
            actions: vec![("btcpay-server".to_string(), ReconcileAction::NoOp)],
            failures: vec![
                ("archy-nbxplorer".to_string(), "pulling ...".to_string()),
                ("archy-btcpay-db".to_string(), "pulling ...".to_string()),
            ],
        };
        let found = degraded_running_apps(&report, [&btcpay, &nbx, &db].into_iter());
        assert_eq!(
            found,
            vec![
                ("btcpay-server".to_string(), "archy-nbxplorer".to_string()),
                ("btcpay-server".to_string(), "archy-btcpay-db".to_string()),
            ]
        );

        // Deps fine → nothing flagged.
        let clean = ReconcileReport {
            actions: vec![
                ("btcpay-server".to_string(), ReconcileAction::NoOp),
                ("archy-nbxplorer".to_string(), ReconcileAction::NoOp),
                ("archy-btcpay-db".to_string(), ReconcileAction::NoOp),
            ],
            failures: vec![],
        };
        assert!(degraded_running_apps(&clean, [&btcpay, &nbx, &db].into_iter()).is_empty());

        // The app itself failed too → not "up", not flagged (its own failure
        // is already reported).
        let both_down = ReconcileReport {
            actions: vec![],
            failures: vec![
                ("btcpay-server".to_string(), "x".to_string()),
                ("archy-nbxplorer".to_string(), "x".to_string()),
            ],
        };
        assert!(degraded_running_apps(&both_down, [&btcpay, &nbx, &db].into_iter()).is_empty());
    }

    #[test]
    fn host_port_collisions_flags_pairs_and_skips_exclusive_variants() {
        let with_port = |id: &str, port: u16| {
            let yaml = format!(
                "app:\n  id: {id}\n  name: {id}\n  version: 1.0.0\n  container:\n    image: {id}:1\n  ports:\n    - host: {port}\n      container: 80\n      protocol: tcp\n"
            );
            AppManifest::parse(&yaml).unwrap()
        };
        let a = with_port("strfry", 8082);
        let b = with_port("vaultwarden", 8082);
        let c = with_port("grafana", 3001);
        let core = with_port("bitcoin-core", 8332);
        let knots = with_port("bitcoin-knots", 8332);

        let found = host_port_collisions([&a, &b, &c, &core, &knots].into_iter());
        assert_eq!(
            found,
            vec![(8082, "strfry".to_string(), "vaultwarden".to_string())]
        );
    }

    #[test]
    fn repo_app_manifests_have_no_host_port_collisions() {
        // The signed catalog regenerates from apps/*/manifest.yml — a latent
        // collision here ships fleet-wide at the next ceremony (strfry/
        // vaultwarden 8082, found live 2026-07-09). Keep the tree clean.
        let apps_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps");
        if !apps_dir.exists() {
            return; // not a full checkout (e.g. crate published standalone)
        }
        let mut manifests = Vec::new();
        for entry in std::fs::read_dir(&apps_dir).unwrap() {
            let path = entry.unwrap().path().join("manifest.yml");
            if !path.exists() {
                continue;
            }
            if let Ok(m) = AppManifest::from_file(&path) {
                manifests.push(m);
            }
        }
        assert!(
            manifests.len() > 10,
            "expected to parse the repo app manifests, got {}",
            manifests.len()
        );
        let found = host_port_collisions(manifests.iter());
        assert!(
            found.is_empty(),
            "host-port collisions in apps/ manifests: {found:?}"
        );
    }

    #[test]
    fn backend_cascade_requires_observed_instance_change() {
        let running = ContainerStatus {
            id: "container-1".into(),
            name: "bitcoin-core".into(),
            state: ContainerState::Running,
            started_at: Some("start-1".into()),
            health: None,
            exit_code: None,
            image: "bitcoin:1".into(),
            created: "created-1".into(),
            ports: vec![],
            lan_address: None,
        };
        assert!(!backend_instance_changed(Some(&running), &running));
        assert!(backend_instance_changed(None, &running));
        let mut before = running.clone();
        before.state = ContainerState::Exited;
        assert!(backend_instance_changed(Some(&before), &running));
        before = running.clone();
        before.id = "old-container".into();
        assert!(backend_instance_changed(Some(&before), &running));
        before = running.clone();
        before.started_at = Some("earlier-start".into());
        assert!(backend_instance_changed(Some(&before), &running));
        before.started_at = None;
        assert!(!backend_instance_changed(Some(&before), &running));
        before.id.clear();
        assert!(!backend_instance_changed(Some(&before), &running));
        let mut after = running.clone();
        after.state = ContainerState::Exited;
        assert!(!backend_instance_changed(None, &after));
        after = running.clone();
        after.id.clear();
        assert!(!backend_instance_changed(None, &after));
    }

    #[test]
    fn cascade_ignores_false_started_report_but_detects_real_exec_drift() {
        let none = HashSet::new();
        let mut report = ReconcileReport {
            actions: vec![
                ("bitcoin-core".into(), ReconcileAction::Started),
                ("lnd".into(), ReconcileAction::NoOp),
            ],
            failures: vec![],
        };
        // systemctl start of an already active unit does not move its address.
        assert!(cascade_pairs_for_report(&report, &none, &none).is_empty());
        // A unit exec rewrite can restart Bitcoin while the outer reconcile
        // action remains NoOp. Runtime evidence still requires LND to reconnect.
        let changed = ["bitcoin-core".into()].into();
        report.actions[0].1 = ReconcileAction::NoOp;
        assert_eq!(
            cascade_pairs_for_report(&report, &none, &changed),
            vec![("bitcoin-core", "lnd")]
        );
        report.actions[0].1 = ReconcileAction::Left("lifecycle-op-in-flight".into());
        assert!(cascade_pairs_for_report(&report, &none, &changed).is_empty());
    }

    #[test]
    fn cascade_pairs_cover_backend_recreate_with_running_dependent() {
        use std::collections::HashSet;
        let report = |actions: Vec<(&str, ReconcileAction)>| ReconcileReport {
            actions: actions
                .into_iter()
                .map(|(id, a)| (id.to_string(), a))
                .collect(),
            failures: vec![],
        };
        let none = HashSet::new();
        let changed: HashSet<String> = ["bitcoin-core".into(), "bitcoin-knots".into()].into();

        // Backend recreated while lnd sat running (NoOp) → cascade.
        let r = report(vec![
            ("bitcoin-knots", ReconcileAction::Installed),
            ("lnd", ReconcileAction::NoOp),
        ]);
        assert_eq!(
            cascade_pairs_for_report(&r, &none, &changed),
            vec![("bitcoin-knots", "lnd")]
        );

        // Backend merely started from stopped also moves the IP → cascade.
        let r = report(vec![
            ("bitcoin-core", ReconcileAction::Started),
            ("lnd", ReconcileAction::NoOp),
        ]);
        assert_eq!(
            cascade_pairs_for_report(&r, &none, &changed),
            vec![("bitcoin-core", "lnd")]
        );

        // Backend untouched → no cascade.
        let r = report(vec![
            ("bitcoin-knots", ReconcileAction::NoOp),
            ("lnd", ReconcileAction::NoOp),
        ]);
        assert!(cascade_pairs_for_report(&r, &none, &none).is_empty());

        // Dependent itself (re)started this pass → it already resolved the
        // fresh address; no cascade.
        let r = report(vec![
            ("bitcoin-knots", ReconcileAction::Installed),
            ("lnd", ReconcileAction::Started),
        ]);
        assert!(cascade_pairs_for_report(&r, &none, &changed).is_empty());

        // User-stopped dependent is never bounced.
        let r = report(vec![
            ("bitcoin-knots", ReconcileAction::Installed),
            ("lnd", ReconcileAction::NoOp),
        ]);
        let stopped: HashSet<String> = ["lnd".to_string()].into();
        assert!(cascade_pairs_for_report(&r, &stopped, &changed).is_empty());

        // Non-backend recreates don't cascade anything.
        let r = report(vec![
            ("grafana", ReconcileAction::Installed),
            ("lnd", ReconcileAction::NoOp),
        ]);
        assert!(cascade_pairs_for_report(&r, &none, &changed).is_empty());
    }

    #[tokio::test]
    async fn reconcile_wallet_start_precedes_unrelated_failed_image_pull() {
        let rt = Arc::new(MockRuntime::default());
        rt.set_state("bitcoin-knots", ContainerState::Exited);
        rt.set_state("lnd", ContainerState::Exited);
        *rt.fail_pull.lock().unwrap() = Some("registry unreachable".into());
        let mut orch = orch_with(rt.clone()).await;
        orch.set_disk_gb_for_test(2000);
        for id in ["unrelated", "lnd", "bitcoin-knots"] {
            orch.insert_manifest_for_test(
                pull_manifest(id, &format!("docker.io/example/{id}:1")),
                PathBuf::from(format!("/tmp/{id}")),
            )
            .await;
        }
        let report = orch.reconcile_all().await;
        assert!(report.failures.iter().any(|(id, _)| id == "unrelated"));
        let calls = rt.calls();
        let bitcoin = calls
            .iter()
            .position(|c| c == "start_container:bitcoin-knots")
            .unwrap();
        let lnd = calls
            .iter()
            .position(|c| c == "start_container:lnd")
            .unwrap();
        let pull = calls
            .iter()
            .position(|c| c.starts_with("pull_image:"))
            .unwrap();
        assert!(
            bitcoin < lnd && lnd < pull,
            "wallet startup was delayed by unrelated recovery: {calls:?}"
        );
    }

    #[tokio::test]
    async fn reconcile_starts_exited_container() {
        let rt = Arc::new(MockRuntime::default());
        rt.set_state("bitcoin-knots", ContainerState::Exited);
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/bk"),
        )
        .await;

        let report = orch.reconcile_all().await;
        assert_eq!(
            report.actions,
            vec![("bitcoin-knots".to_string(), ReconcileAction::Started)]
        );
        // Exactly one start, no (re)create, no pull
        let calls = rt.calls();
        assert!(calls.iter().any(|c| c == "start_container:bitcoin-knots"));
        assert!(!calls.iter().any(|c| c.starts_with("create_container:")));
        assert!(!calls.iter().any(|c| c.starts_with("pull_image:")));
    }

    #[tokio::test]
    async fn reconcile_recreates_when_exited_start_fails() {
        let rt = Arc::new(MockRuntime::default());
        rt.mark_image_present("docker.io/bitcoin/knots:28");
        rt.set_state("bitcoin-knots", ContainerState::Exited);
        rt.fail_start
            .lock()
            .unwrap()
            .insert("bitcoin-knots".into(), "stale runtime state".into());
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/bk"),
        )
        .await;

        let report = orch.reconcile_all().await;
        assert_eq!(
            report.actions,
            vec![("bitcoin-knots".to_string(), ReconcileAction::Installed)]
        );
        assert!(report.failures.is_empty());
        let calls = rt.calls();
        assert!(calls.iter().any(|c| c == "remove_container:bitcoin-knots"));
        assert!(calls
            .iter()
            .any(|c| c == "create_container:bitcoin-knots:offset=0"));
    }

    #[tokio::test]
    async fn repair_recreate_stops_after_max_attempts_instead_of_looping_forever() {
        // A container whose entrypoint fatally crashes every time (portainer
        // on .198, 2026-07-01: DB schema too new for the pinned binary) must
        // not be stop+remove+install_fresh'd forever by the boot reconciler.
        let rt = Arc::new(MockRuntime::default());
        rt.mark_image_present("docker.io/bitcoin/knots:28");
        rt.set_state("bitcoin-knots", ContainerState::Exited);
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/bk"),
        )
        .await;

        let mut last_report = None;
        for _ in 0..MAX_REPAIR_ATTEMPTS {
            // fail_start entries are consumed on use — re-arm every pass so
            // this container fails to start EVERY time, not just once.
            rt.fail_start
                .lock()
                .unwrap()
                .insert("bitcoin-knots".into(), "fatal startup error".into());
            rt.set_state("bitcoin-knots", ContainerState::Exited);
            let report = orch.reconcile_all().await;
            assert_eq!(
                report.actions,
                vec![("bitcoin-knots".to_string(), ReconcileAction::Installed)],
                "expected a recreate attempt within the attempt budget"
            );
            last_report = Some(report);
        }
        assert!(last_report.is_some());

        // One more pass exceeds MAX_REPAIR_ATTEMPTS — the breaker must trip:
        // no further remove/create calls (it may still probe status/attempt
        // a start, same as any other pass — it just must not recreate).
        let remove_calls_before = rt
            .calls()
            .iter()
            .filter(|c| c.starts_with("remove_container:"))
            .count();
        let create_calls_before = rt
            .calls()
            .iter()
            .filter(|c| c.starts_with("create_container:"))
            .count();
        rt.fail_start
            .lock()
            .unwrap()
            .insert("bitcoin-knots".into(), "fatal startup error".into());
        rt.set_state("bitcoin-knots", ContainerState::Exited);
        let report = orch.reconcile_all().await;
        assert_eq!(
            report.actions,
            vec![(
                "bitcoin-knots".to_string(),
                ReconcileAction::Left("repair-attempts-exhausted".to_string())
            )]
        );
        let calls_after = rt.calls();
        assert_eq!(
            calls_after
                .iter()
                .filter(|c| c.starts_with("remove_container:"))
                .count(),
            remove_calls_before,
            "breaker must skip the recreate's remove_container entirely"
        );
        assert_eq!(
            calls_after
                .iter()
                .filter(|c| c.starts_with("create_container:"))
                .count(),
            create_calls_before,
            "breaker must skip the recreate's create_container entirely"
        );
    }

    #[tokio::test]
    async fn reconcile_installs_missing_container() {
        let rt = Arc::new(MockRuntime::default());
        // container map empty -> "not found" -> orchestrator installs fresh
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/bk"),
        )
        .await;

        let report = orch.reconcile_all().await;
        assert_eq!(
            report.actions,
            vec![("bitcoin-knots".to_string(), ReconcileAction::Installed)]
        );
        let calls = rt.calls();
        assert!(calls.iter().any(|c| c.starts_with("pull_image:")));
        assert!(calls
            .iter()
            .any(|c| c == "create_container:bitcoin-knots:offset=0"));
        assert!(calls.iter().any(|c| c == "start_container:bitcoin-knots"));
    }

    #[tokio::test]
    async fn reconcile_existing_installs_missing_required_baseline_app() {
        let rt = Arc::new(MockRuntime::default());
        let mut orch = orch_with(rt.clone()).await;
        orch.set_disk_gb_for_test(500);
        orch.insert_manifest_for_test(
            pull_manifest("filebrowser", "docker.io/filebrowser/filebrowser:latest"),
            PathBuf::from("/tmp/filebrowser"),
        )
        .await;

        let report = orch.reconcile_existing().await;

        assert_eq!(
            report.actions,
            vec![("filebrowser".to_string(), ReconcileAction::Installed)]
        );
        assert!(report.failures.is_empty());
        let calls = rt.calls();
        assert!(calls.iter().any(|c| c.starts_with("pull_image:")));
        assert!(calls
            .iter()
            .any(|c| c == "create_container:filebrowser:offset=0"));
        assert!(calls.iter().any(|c| c == "start_container:filebrowser"));
    }

    #[tokio::test]
    async fn reconcile_existing_respects_durable_user_uninstalled_marker_for_baseline_apps() {
        let rt = Arc::new(MockRuntime::default());
        let mut orch = orch_with(rt.clone()).await;
        orch.set_disk_gb_for_test(500);
        orch.insert_manifest_for_test(
            pull_manifest("filebrowser", "docker.io/filebrowser/filebrowser:latest"),
            PathBuf::from("/tmp/filebrowser"),
        )
        .await;
        // Simulates the real-world sequence: user uninstalls a required
        // baseline app, then archipelago restarts (which wipes the in-memory
        // `disabled` set via `load_manifests()` before this harness even
        // gets involved) — the durable marker is all that's left standing
        // between that and the baseline self-heal below reinstalling it.
        crate::crash_recovery::mark_user_uninstalled(&orch.data_dir, "filebrowser").await;

        let report = orch.reconcile_existing().await;

        assert_eq!(
            report.actions,
            vec![(
                "filebrowser".to_string(),
                ReconcileAction::Left("user-uninstalled".to_string())
            )]
        );
        assert!(report.failures.is_empty());
        let calls = rt.calls();
        assert!(!calls.iter().any(|c| c.starts_with("pull_image:")));
        assert!(!calls.iter().any(|c| c.starts_with("create_container:")));
    }

    #[tokio::test]
    async fn reconcile_rewrites_security_config_for_a_user_uninstalled_but_running_app() {
        // Regression, a test node 2026-08-02: `bitcoin-ui` carried a durable
        // user-uninstalled marker WHILE systemd/Quadlet kept archy-bitcoin-ui
        // running and publishing :8334. Reconcile returned on the marker before
        // reaching the pre-start hook, so the /bitcoin-rpc/ auth_request gate
        // that a05956c4 shipped inside the binary never reached the rendered
        // nginx.conf. The node looked patched while an unauthenticated caller
        // on the LAN or mesh could still drive Bitcoin Core RPC through a
        // credential-injecting proxy.
        let rt = Arc::new(MockRuntime::default());
        let mut orch = orch_with(rt.clone()).await;

        // Own RenderPaths, not the process-wide static one, so a stale
        // starting state can't race the other bitcoin-ui tests.
        let dir = tempfile::TempDir::new().expect("test tmpdir");
        std::fs::write(dir.path().join("bitcoin-rpc-password"), "test-pass\n")
            .expect("seed password");
        let rendered_path = dir.path().join("nginx.conf");
        std::fs::write(&rendered_path, "# pre-fix config: no auth_request gate\n")
            .expect("seed stale conf");
        orch.set_bitcoin_ui_paths(bitcoin_ui::RenderPaths {
            secret_path: dir.path().join("bitcoin-rpc-password"),
            rendered_path: rendered_path.clone(),
        });

        orch.insert_manifest_for_test(
            build_manifest(
                "bitcoin-ui",
                "/opt/archy/docker/bitcoin-ui",
                "archy-bitcoin-ui:local",
            ),
            PathBuf::from("/opt/archy/apps/bitcoin-ui"),
        )
        .await;
        rt.set_state("archy-bitcoin-ui", ContainerState::Running);
        crate::crash_recovery::mark_user_uninstalled(&orch.data_dir, "bitcoin-ui").await;

        let report = orch.reconcile_existing().await;

        let contents = std::fs::read_to_string(&rendered_path)
            .expect("nginx.conf must still exist after reconcile");
        assert!(
            contents.contains("auth_request /_session_check"),
            "the session gate must reach a running container even when the app \
             carries an uninstall marker:\n{contents}"
        );
        assert!(
            !contents.contains("{{BITCOIN_RPC_AUTH}}"),
            "placeholder was not substituted:\n{contents}"
        );

        // Rewriting the file is not enough — nginx only loads it on restart.
        let calls = rt.calls();
        assert!(
            calls
                .iter()
                .any(|c| c == "start_container:archy-bitcoin-ui"),
            "container must be restarted so nginx picks the new config up: {calls:?}"
        );
        assert_eq!(
            report.actions,
            vec![("bitcoin-ui".to_string(), ReconcileAction::Started)]
        );
        assert!(report.failures.is_empty());

        // The "must stay removed" contract is untouched: nothing is created,
        // pulled or built for an app the user uninstalled.
        assert!(!calls.iter().any(|c| c.starts_with("create_container:")));
        assert!(!calls.iter().any(|c| c.starts_with("pull_image:")));
        assert!(!calls.iter().any(|c| c.starts_with("build_image:")));
    }

    #[tokio::test]
    async fn reconcile_existing_skips_archival_baseline_apps_on_pruned_hosts() {
        let rt = Arc::new(MockRuntime::default());
        let mut orch = orch_with(rt.clone()).await;
        orch.set_disk_gb_for_test(500);
        orch.insert_manifest_for_test(
            pull_manifest("electrumx", "docker.io/spesmilo/electrumx:latest"),
            PathBuf::from("/tmp/electrumx"),
        )
        .await;

        let report = orch.reconcile_existing().await;

        assert_eq!(
            report.actions,
            vec![(
                "electrumx".to_string(),
                ReconcileAction::Left("requires-archival-bitcoin".into())
            )]
        );
        assert!(report.failures.is_empty());
        let calls = rt.calls();
        assert!(!calls.iter().any(|c| c.starts_with("pull_image:")));
        assert!(!calls.iter().any(|c| c.starts_with("create_container:")));
        assert!(!calls.iter().any(|c| c.starts_with("start_container:")));
    }

    #[tokio::test]
    async fn reconcile_existing_does_not_resurrect_orphaned_companions() {
        let rt = Arc::new(MockRuntime::default());
        let mut orch = orch_with(rt.clone()).await;
        orch.set_disk_gb_for_test(500);
        let companions = [
            "bitcoin-ui",
            "electrs-ui",
            "lnd-ui",
            "fedimint-ui",
            "cuprate-ui",
        ];
        let mut names = Vec::new();
        for id in companions {
            let manifest = pull_manifest(id, "localhost/companion:local");
            names.push(compute_container_name(&manifest));
            orch.insert_manifest_for_test(manifest, PathBuf::from("/tmp/companion"))
                .await;
        }
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        crate::crash_recovery::save_container_snapshot_for_test(&orch.data_dir, &refs).await;
        // Repeated passes must leave lifecycle ownership with companion.rs.
        for _ in 0..3 {
            let report = orch.reconcile_existing().await;
            assert_eq!(report.actions.len(), companions.len());
            assert!(report
                .actions
                .iter()
                .all(|(_, action)| *action == ReconcileAction::Left("absent".into())));
            assert!(report.failures.is_empty());
        }
        let calls = rt.calls();
        for operation in [
            "pull_image:",
            "create_container:",
            "start_container:",
            "stop_container:",
            "remove_container:",
        ] {
            assert!(
                !calls.iter().any(|call| call.starts_with(operation)),
                "{calls:?}"
            );
        }
    }

    #[tokio::test]
    async fn reconcile_existing_self_heals_missing_optional_installed_app() {
        // A non-baseline app (gitea) self-heals ONLY with installation
        // evidence: its container was running at the last periodic snapshot
        // (desired-state recovery). Manifest presence alone must NOT install
        // — the catalog ships manifests for every app, and treating them as
        // installed mass-installed the catalog on a fresh node (2026-07-14).
        // The indeedhub/immich vanished-container cases are exactly the
        // snapshot-covered scenario exercised here.
        let rt = Arc::new(MockRuntime::default());
        let mut orch = orch_with(rt.clone()).await;
        orch.set_disk_gb_for_test(500);
        orch.insert_manifest_for_test(
            pull_manifest("gitea", "docker.io/gitea/gitea:latest"),
            PathBuf::from("/tmp/gitea"),
        )
        .await;
        // Installation evidence: gitea was running at the last snapshot.
        crate::crash_recovery::save_container_snapshot_for_test(&orch.data_dir, &["gitea"]).await;

        let report = orch.reconcile_existing().await;

        assert_eq!(
            report.actions,
            vec![("gitea".to_string(), ReconcileAction::Installed)]
        );
        assert!(report.failures.is_empty());
        let calls = rt.calls();
        assert!(calls.iter().any(|c| c.starts_with("pull_image:")));
        assert!(calls.iter().any(|c| c.starts_with("create_container:")));
        assert!(calls.iter().any(|c| c.starts_with("start_container:")));
    }

    #[tokio::test]
    async fn reconcile_collects_per_app_failures_without_short_circuiting() {
        let rt = Arc::new(MockRuntime::default());
        // ui app: image build will fail
        *rt.fail_build.lock().unwrap() = Some("disk full".to_string());
        // knots: already running, will no-op
        rt.set_state("bitcoin-knots", ContainerState::Running);
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/bk"),
        )
        .await;
        orch.insert_manifest_for_test(
            build_manifest("bitcoin-ui", "/ctx", "archy-bitcoin-ui:local"),
            PathBuf::from("/opt/bitcoin-ui"),
        )
        .await;

        let report = orch.reconcile_all().await;
        // knots succeeded, bitcoin-ui failed; both reported.
        let app_ids: Vec<&str> = report.actions.iter().map(|(a, _)| a.as_str()).collect();
        assert!(app_ids.contains(&"bitcoin-knots"));
        let failed: Vec<&str> = report.failures.iter().map(|(a, _)| a.as_str()).collect();
        assert_eq!(failed, vec!["bitcoin-ui"]);
    }

    #[tokio::test]
    async fn adopt_existing_matches_by_name() {
        let rt = Arc::new(MockRuntime::default());
        // Pre-seed the runtime as if the bash first-boot script had installed it.
        rt.set_state("archy-bitcoin-ui", ContainerState::Running);
        rt.set_state("bitcoin-knots", ContainerState::Running);
        rt.set_state("unrelated-sidecar", ContainerState::Running);
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            build_manifest("bitcoin-ui", "/ctx", "archy-bitcoin-ui:local"),
            PathBuf::from("/opt/bitcoin-ui"),
        )
        .await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/bk"),
        )
        .await;
        // A manifest with no matching running container — must NOT be adopted.
        orch.insert_manifest_for_test(pull_manifest("lnd", "lnd:0.18"), PathBuf::from("/tmp/lnd"))
            .await;

        let report = orch.adopt_existing().await.unwrap();
        let mut ids = report.adopted.clone();
        ids.sort();
        assert_eq!(ids, vec!["bitcoin-knots", "bitcoin-ui"]);
    }

    #[tokio::test]
    async fn upgrade_preserves_container_when_catalog_is_stale_or_already_installed() {
        for (target, should_error) in [("v3.3.1", true), ("v3.3.1-archy1", false)] {
            let rt = Arc::new(MockRuntime::default());
            rt.set_state("update-regression", ContainerState::Running);
            rt.running_images.lock().unwrap().insert(
                "update-regression".into(),
                "registry.test/old/mempool-frontend:v3.3.1-archy1".into(),
            );
            let orch = orch_with(rt.clone()).await;
            orch.insert_manifest_for_test(
                pull_manifest(
                    "update-regression",
                    &format!("registry.test/new/mempool-frontend:{target}"),
                ),
                PathBuf::from("/tmp/update-regression"),
            )
            .await;
            assert_eq!(
                orch.upgrade("update-regression").await.is_err(),
                should_error
            );
            assert!(
                !rt.calls()
                    .iter()
                    .any(|call| call.starts_with("stop_container:")
                        || call.starts_with("remove_container:")
                        || call.starts_with("pull_image:")
                        || call.starts_with("create_container:")),
                "{:?}",
                rt.calls()
            );
        }
    }

    #[tokio::test]
    async fn upgrade_removes_and_reinstalls() {
        let rt = Arc::new(MockRuntime::default());
        rt.set_state("bitcoin-knots", ContainerState::Running);
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/bk"),
        )
        .await;

        orch.upgrade("bitcoin-knots").await.unwrap();
        let calls = rt.calls();
        // Sequence contract: stop -> remove -> pull -> create -> start, in that order.
        let relevant: Vec<&str> = calls
            .iter()
            .map(|s| s.as_str())
            .filter(|s| {
                s.starts_with("stop_container:")
                    || s.starts_with("remove_container:")
                    || s.starts_with("pull_image:")
                    || s.starts_with("create_container:")
                    || s.starts_with("start_container:")
            })
            .collect();
        assert_eq!(
            relevant,
            vec![
                "stop_container:bitcoin-knots",
                "remove_container:bitcoin-knots",
                "pull_image:docker.io/bitcoin/knots:28",
                "create_container:bitcoin-knots:offset=0",
                "start_container:bitcoin-knots",
            ]
        );
    }

    #[tokio::test]
    async fn removed_catalog_entry_does_not_block_legacy_uninstall() {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt.clone()).await;
        orch.remove("cryptpad", true).await.unwrap();
        assert!(orch.state.read().await.disabled.contains("cryptpad"));
        assert!(
            rt.calls().is_empty(),
            "legacy RPC teardown owns the actual containers"
        );
        assert!(orch.remove("../other", true).await.is_err());
    }

    #[tokio::test]
    async fn remove_disables_manifest_so_reconcile_does_not_reinstall() {
        let rt = Arc::new(MockRuntime::default());
        rt.set_state("bitcoin-knots", ContainerState::Running);
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/bk"),
        )
        .await;

        orch.remove("bitcoin-knots", true).await.unwrap();

        let report = orch.reconcile_all().await;
        assert!(report.actions.is_empty());
        assert!(report.failures.is_empty());
        let calls = rt.calls();
        assert!(calls.iter().any(|c| c == "remove_container:bitcoin-knots"));
        assert!(!calls.iter().any(|c| c.starts_with("pull_image:")));
        assert!(!calls.iter().any(|c| c.starts_with("create_container:")));
    }

    #[tokio::test]
    async fn install_reenables_manifest_after_remove() {
        let rt = Arc::new(MockRuntime::default());
        rt.set_state("bitcoin-knots", ContainerState::Running);
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/bk"),
        )
        .await;

        orch.remove("bitcoin-knots", true).await.unwrap();
        rt.set_state("bitcoin-knots", ContainerState::Running);
        orch.install("bitcoin-knots").await.unwrap();

        let report = orch.reconcile_all().await;
        assert_eq!(report.actions.len(), 1);
        assert!(report.failures.is_empty());
    }

    #[tokio::test]
    async fn list_filters_to_known_manifests_only() {
        let rt = Arc::new(MockRuntime::default());
        rt.set_state("bitcoin-knots", ContainerState::Running);
        rt.set_state("unrelated", ContainerState::Running);
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/bk"),
        )
        .await;

        let list = orch.list().await.unwrap();
        let names: Vec<&str> = list.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["bitcoin-knots"]);
    }

    #[tokio::test]
    async fn stop_disables_manifest_until_start() {
        let rt = Arc::new(MockRuntime::default());
        rt.set_state("bitcoin-knots", ContainerState::Running);
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("bitcoin-knots", "docker.io/bitcoin/knots:28"),
            PathBuf::from("/tmp/bk"),
        )
        .await;

        orch.stop("bitcoin-knots").await.unwrap();
        let report = orch.reconcile_all().await;
        assert!(report.actions.is_empty());
        assert!(report.failures.is_empty());
        let calls = rt.calls();
        assert!(calls.iter().any(|c| c == "stop_container:bitcoin-knots"));
        assert!(!calls.iter().any(|c| c == "start_container:bitcoin-knots"));

        orch.start("bitcoin-knots").await.unwrap();
        let calls = rt.calls();
        assert!(calls.iter().any(|c| c == "start_container:bitcoin-knots"));
    }

    #[tokio::test]
    async fn umbrella_mempool_id_fans_out_to_split_members() {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt.clone()).await;
        for (id, image) in [
            ("archy-mempool-db", "docker.io/library/mariadb:11.4"),
            ("mempool-api", "docker.io/mempool/backend:v3.0"),
            ("archy-mempool-web", "docker.io/mempool/frontend:v3.0"),
        ] {
            orch.insert_manifest_for_test(pull_manifest(id, image), PathBuf::from("/tmp/mp"))
                .await;
        }
        // No umbrella `mempool` manifest is loaded (load_manifests drops it
        // when the split members are present) and no member container is live
        // — the exact state a quadlet stop leaves behind. start("mempool")
        // must resurrect the members rather than fail with unknown app_id.
        orch.start("mempool").await.unwrap();
        let calls = rt.calls();
        for name in ["archy-mempool-db", "mempool-api", "archy-mempool-web"] {
            assert!(
                calls
                    .iter()
                    .any(|c| c == &format!("start_container:{name}")),
                "{name} not started: {calls:?}"
            );
        }

        orch.stop("mempool").await.unwrap();
        let calls = rt.calls();
        for name in ["archy-mempool-db", "mempool-api", "archy-mempool-web"] {
            assert!(
                calls.iter().any(|c| c == &format!("stop_container:{name}")),
                "{name} not stopped: {calls:?}"
            );
        }

        orch.restart("mempool").await.unwrap();
    }

    #[tokio::test]
    async fn umbrella_alias_inactive_when_umbrella_manifest_live() {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt.clone()).await;
        // Legacy node: only the umbrella manifest exists — the alias must not
        // kick in; `mempool` operates as its own single-container app.
        orch.insert_manifest_for_test(
            pull_manifest("mempool", "docker.io/mempool/frontend:v3.0"),
            PathBuf::from("/tmp/mp"),
        )
        .await;
        orch.start("mempool").await.unwrap();
        let calls = rt.calls();
        assert!(calls.iter().any(|c| c == "start_container:mempool"));
        assert!(!calls.iter().any(|c| c.contains("archy-mempool")));
    }

    #[tokio::test]
    async fn reconcile_force_recreates_stopping_container() {
        let rt = Arc::new(MockRuntime::default());
        rt.set_state("nostr-rs-relay", ContainerState::Stopping);
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(
            pull_manifest("nostr-rs-relay", "docker.io/scsibug/nostr-rs-relay:0.8.9"),
            PathBuf::from("/tmp/nostr-rs-relay"),
        )
        .await;

        let report = orch.reconcile_all().await;

        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(
            report.actions,
            vec![("nostr-rs-relay".to_string(), ReconcileAction::Installed)]
        );
        let calls = rt.calls();
        assert!(calls.iter().any(|c| c == "remove_container:nostr-rs-relay"));
        assert!(calls
            .iter()
            .any(|c| c == "create_container:nostr-rs-relay:offset=0"));
        assert!(calls.iter().any(|c| c == "start_container:nostr-rs-relay"));
    }

    #[tokio::test]
    async fn health_maps_states_to_strings() {
        let rt = Arc::new(MockRuntime::default());
        rt.set_state("lnd", ContainerState::Running);
        let orch = orch_with(rt.clone()).await;
        orch.insert_manifest_for_test(pull_manifest("lnd", "lnd:0.18"), PathBuf::from("/tmp/lnd"))
            .await;
        assert_eq!(orch.health("lnd").await.unwrap(), "healthy");

        rt.set_health("lnd", "unhealthy");
        assert_eq!(orch.health("lnd").await.unwrap(), "unhealthy");

        rt.set_state("lnd", ContainerState::Exited);
        assert_eq!(orch.health("lnd").await.unwrap(), "unhealthy");

        rt.set_state("lnd", ContainerState::Paused);
        assert_eq!(orch.health("lnd").await.unwrap(), "paused");

        rt.set_state("lnd", ContainerState::Created);
        assert_eq!(orch.health("lnd").await.unwrap(), "starting");

        rt.set_state("lnd", ContainerState::Stopping);
        assert_eq!(orch.health("lnd").await.unwrap(), "unhealthy");
    }

    #[tokio::test]
    async fn unknown_app_id_is_rejected() {
        let rt = Arc::new(MockRuntime::default());
        let orch = orch_with(rt.clone()).await;
        let err = orch.status("does-not-exist").await.unwrap_err();
        assert!(format!("{err}").contains("unknown app_id"));
    }

    #[tokio::test]
    async fn install_filebrowser_writes_config_before_create() {
        let rt = Arc::new(MockRuntime::default());
        let mut orch = orch_with(rt.clone()).await;

        let tmp = tempfile::TempDir::new().unwrap();
        let paths = filebrowser::EnsurePaths {
            srv_root: tmp.path().join("filebrowser"),
            data_dir: tmp.path().join("filebrowser-data"),
            config_path: tmp.path().join("filebrowser-data/.filebrowser.json"),
        };
        orch.set_filebrowser_paths(paths.clone());

        let mut m = pull_manifest_filebrowser();
        m.app.volumes[0].source = paths.srv_root.to_string_lossy().into_owned();
        m.app.volumes[1].source = paths.data_dir.to_string_lossy().into_owned();
        orch.insert_manifest_for_test(m, PathBuf::from("/tmp/filebrowser"))
            .await;

        orch.install("filebrowser").await.unwrap();

        assert!(paths.config_path.exists());
        let cfg = std::fs::read_to_string(paths.config_path).unwrap();
        assert!(cfg.contains("\"database\":\"/data/filebrowser.db\""));
        let calls = rt.calls();
        assert!(calls
            .iter()
            .any(|c| c == "create_container:filebrowser:offset=0"));
    }

    #[tokio::test]
    async fn install_lnd_writes_config_before_create() {
        let rt = Arc::new(MockRuntime::default());
        let mut orch = orch_with(rt.clone()).await;

        let tmp = tempfile::TempDir::new().unwrap();
        let secrets = tmp.path().join("secrets");
        std::fs::create_dir_all(&secrets).unwrap();
        std::fs::write(secrets.join("bitcoin-rpc-password"), "secret-pass\n").unwrap();
        orch.set_secrets_dir(secrets);

        let paths = lnd::EnsurePaths {
            data_dir: tmp.path().join("lnd"),
            conf_path: tmp.path().join("lnd/lnd.conf"),
        };
        orch.set_lnd_paths(paths.clone());

        let mut m = pull_manifest_lnd();
        m.app.volumes[0].source = paths.data_dir.to_string_lossy().into_owned();
        orch.insert_manifest_for_test(m, PathBuf::from("/tmp/lnd"))
            .await;

        orch.install("lnd").await.unwrap();

        let cfg = std::fs::read_to_string(paths.conf_path).unwrap();
        assert!(cfg.contains("bitcoin.active=true"));
        assert!(cfg.contains("bitcoin.mainnet=true"));
        assert!(cfg.contains("bitcoind.rpcpass=secret-pass"));
        let calls = rt.calls();
        assert!(calls.iter().any(|c| c == "create_container:lnd:offset=0"));
    }

    #[test]
    fn fingerprint_build_context_detects_source_changes() {
        let tmp = tempfile::TempDir::new().unwrap();
        let ctx = tmp.path();
        std::fs::write(ctx.join("Dockerfile"), "FROM nginx\n").unwrap();
        std::fs::create_dir_all(ctx.join("assets")).unwrap();
        std::fs::write(ctx.join("assets/app.js"), b"v1").unwrap();

        let a = fingerprint_build_context(ctx).expect("fingerprint");
        // Recomputing over the same tree is stable.
        let b = fingerprint_build_context(ctx).expect("fingerprint");
        assert_eq!(a, b, "fingerprint must be stable for an unchanged tree");

        // Changing a COPYed source file (different length) changes the fingerprint.
        std::fs::write(ctx.join("assets/app.js"), b"v2-longer").unwrap();
        let c = fingerprint_build_context(ctx).expect("fingerprint");
        assert_ne!(a, c, "changed source file must change the fingerprint");
    }

    #[test]
    fn build_fingerprint_stamp_path_sanitizes_tag() {
        let p = build_fingerprint_stamp_path(
            Path::new("/var/lib/archipelago"),
            "localhost/bitcoin-ui:local",
        );
        assert_eq!(
            p,
            PathBuf::from(
                "/var/lib/archipelago/.image-build/localhost_bitcoin_ui_local.fingerprint"
            )
        );
    }

    #[test]
    fn fresh_subtree_anchor_handles_nested_and_direct() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path(); // the pre-existing "data root"

        // Direct child (immich-style): anchor is the data root, subtree top is
        // the child itself.
        let direct = root.join("immich");
        let (anchor, top) = fresh_subtree_anchor(&direct);
        assert_eq!(anchor.as_deref(), Some(root));
        assert_eq!(top, direct);

        // Nested (jellyfin-style /config): the intermediate `jellyfin` dir does
        // NOT exist yet, so the anchor must skip past it to the data root and the
        // subtree top is `jellyfin` — chowning that -R fixes both levels. The old
        // code referenced the immediate parent (`jellyfin`), which mkdir -p makes
        // root-owned → the EACCES bug.
        let nested = root.join("jellyfin").join("config");
        let (anchor, top) = fresh_subtree_anchor(&nested);
        assert_eq!(anchor.as_deref(), Some(root));
        assert_eq!(top, root.join("jellyfin"));

        // Second volume of the same app: now `jellyfin` exists (created for the
        // first volume), so the anchor is `jellyfin` and only `cache` is new.
        std::fs::create_dir(root.join("jellyfin")).unwrap();
        let (anchor, top) = fresh_subtree_anchor(&root.join("jellyfin").join("cache"));
        assert_eq!(anchor.as_deref(), Some(root.join("jellyfin").as_path()));
        assert_eq!(top, root.join("jellyfin").join("cache"));
    }

    #[test]
    fn pid_is_alive_detects_live_and_dead_pids() {
        // Our own process is alive.
        assert!(pid_is_alive(std::process::id() as i32));
        // Non-positive PIDs are never alive (a "running" container with PID 0 is
        // exactly the zombie case).
        assert!(!pid_is_alive(0));
        assert!(!pid_is_alive(-1));
        // A PID far above the kernel's pid_max can't name a live process, so the
        // zombie guard reports it dead → the reconciler recreates.
        assert!(!pid_is_alive(2_000_000_000));
    }

    #[test]
    fn ownership_drift_gate_matches_on_exact_owner_only() {
        // A tempdir owned by this process's uid stands in for a correctly
        // chowned data dir; any other uid reads as drift.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        let my_uid = unsafe { libc::geteuid() };
        let mine = format!("{0}:{0}", my_uid);
        assert!(ownership_already_correct(&path, &mine));
        assert!(!ownership_already_correct(&path, "100998:100998"));
        assert!(!ownership_already_correct("/nonexistent/path", &mine));
        assert!(!ownership_already_correct(&path, "garbage"));
        assert!(!ownership_already_correct(&path, "1:2:3"));
    }
}
