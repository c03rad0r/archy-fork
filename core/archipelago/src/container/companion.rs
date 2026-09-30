//! Companion UI container lifecycle, entirely Quadlet-managed.
//!
//! A "companion" is a small nginx-based container that exposes a
//! browser-friendly UI on top of a headless backend service:
//!
//! | Backend          | Companion          | Purpose                  |
//! |------------------|--------------------|--------------------------|
//! | bitcoin-knots    | archy-bitcoin-ui   | RPC viewer               |
//! | bitcoin-core     | archy-bitcoin-ui   | RPC viewer               |
//! | lnd              | archy-lnd-ui       | wallet/channel UI        |
//! | electrumx        | archy-electrs-ui   | indexer status UI        |
//! | fedimint         | archy-fedimint-ui  | wait/proxy Guardian UI   |
//! | cuprate          | archy-cuprate-ui   | Monero node status UI    |
//!
//! Lifecycle: `install` writes a Quadlet `.container` unit to
//! `~/.config/containers/systemd/`, daemon-reloads, then starts the
//! generated `.service`. systemd owns supervision from that point on
//! — archipelago can crash, restart, or be uninstalled without
//! touching the companion.
//!
//! This replaces the old `tokio::spawn { podman run }` block in
//! `install.rs` (~165 lines of fire-and-forget shellouts) with a
//! single declarative call.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::process::Command;
use tracing::{info, warn};

use crate::container::quadlet::{self, BindMount, NetworkMode, QuadletUnit};
use archipelago_container::image_uses_insecure_registry;

const COMPANION_REGISTRY: &str = "source.archipelago-foundation.org/lfg2025";
const COMPANION_IMAGE_CHECK_TIMEOUT: Duration = Duration::from_secs(15);
const COMPANION_BUILD_TIMEOUT: Duration = Duration::from_secs(900);
const COMPANION_PULL_TIMEOUT: Duration = Duration::from_secs(300);
/// After a failed repair (image build/pull included), leave the companion
/// alone for this long. Without it, a node under IO pressure retried a 900s
/// image build every 30s reconcile tick — each build pegging the disk that
/// made the probes fail in the first place (live-diagnosed on a test node
/// 2026-07-28: load 50, podman scans starved, apps page stuck).
const REPAIR_COOLDOWN: Duration = Duration::from_secs(600);

static REPAIR_FAILED_AT: LazyLock<Mutex<HashMap<&'static str, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A companion must look orphaned for this long before it is reaped.
///
/// "Backend container absent" is not the same as "backend app uninstalled":
/// a Quadlet-managed app is briefly containerless while it restarts, and this
/// node runs `ARCHIPELAGO_USE_QUADLET_BACKENDS=true`. Reaping on the first
/// absent tick would take down a healthy companion mid-restart and reinstall
/// it on the next pass — an image pull or a 900s build in the worst case.
/// A real uninstall stays absent indefinitely, so waiting costs nothing.
const ORPHAN_GRACE: Duration = Duration::from_secs(300);

/// First tick at which each companion was observed with no installed backend.
/// Cleared as soon as a backend reappears, so the grace period restarts.
static ORPHAN_SINCE: LazyLock<Mutex<HashMap<&'static str, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Static description of one companion. The full list per backend
/// app_id lives in `companions_for`.
#[derive(Debug, Clone)]
pub struct CompanionSpec {
    /// Container + unit name (e.g. "archy-bitcoin-ui").
    pub name: &'static str,
    /// Image base name in the lfg2025 registry namespace
    /// (e.g. "bitcoin-ui" → "source.archipelago-foundation.org/lfg2025/bitcoin-ui:latest").
    pub image_base: &'static str,
    /// Filesystem locations to look for a local Dockerfile (build wins
    /// over registry pull). Searched in order; first hit wins.
    pub build_dir_candidates: &'static [&'static str],
    /// Optional pre-start hook that renders config files referenced
    /// by `bind_mounts`. Returns Ok(()) on success; bind-mount must
    /// be present at start time or the companion will 502.
    pub pre_start: Option<PreStartHook>,
    /// Bind mounts. Always read-only — companions don't write to
    /// host paths.
    pub bind_mounts: &'static [(&'static str, &'static str)],
    /// Host-to-container TCP ports for non-host-network companions.
    pub ports: &'static [(u16, u16)],
    /// Whether the companion must share the host network namespace.
    pub host_network: bool,
}

pub type PreStartHook = fn() -> futures_util::future::BoxFuture<'static, Result<()>>;

/// Companions to install when `package_id` lands. Empty for apps
/// without a companion UI.
pub fn companions_for(package_id: &str) -> &'static [CompanionSpec] {
    match package_id {
        "bitcoin" | "bitcoin-core" | "bitcoin-knots" => BITCOIN_UI,
        "lnd" => LND_UI,
        "electrumx" | "electrs" | "mempool-electrs" => ELECTRS_UI,
        "fedimint" | "fedimintd" => FEDIMINT_UI,
        "cuprate" => CUPRATE_UI,
        _ => &[],
    }
}

/// Every companion this build knows how to provision. Kept beside
/// `companions_for` — a new companion must be added to both, or the reaper
/// will not recognise it as one of ours and will leave it running forever.
const ALL_COMPANIONS: &[&[CompanionSpec]] =
    &[BITCOIN_UI, LND_UI, ELECTRS_UI, FEDIMINT_UI, CUPRATE_UI];

const BITCOIN_UI: &[CompanionSpec] = &[CompanionSpec {
    name: "archy-bitcoin-ui",
    image_base: "bitcoin-ui",
    build_dir_candidates: &[
        "/opt/archipelago/docker/bitcoin-ui",
        "/home/archipelago/archy/docker/bitcoin-ui",
        "/home/archipelago/Projects/archy/docker/bitcoin-ui",
    ],
    pre_start: Some(render_bitcoin_ui),
    bind_mounts: &[(
        "/var/lib/archipelago/bitcoin-ui/nginx.conf",
        "/etc/nginx/conf.d/default.conf",
    )],
    ports: &[],
    host_network: true,
}];

const LND_UI: &[CompanionSpec] = &[CompanionSpec {
    name: "archy-lnd-ui",
    image_base: "lnd-ui",
    build_dir_candidates: &[
        "/opt/archipelago/docker/lnd-ui",
        "/home/archipelago/archy/docker/lnd-ui",
        "/home/archipelago/Projects/archy/docker/lnd-ui",
    ],
    pre_start: None,
    bind_mounts: &[],
    // Host networking so the app's own nginx can proxy the archipelago backend
    // same-origin (127.0.0.1:5678), exactly like fips-ui / electrs-ui. The
    // previous bridge + 18083→80 mapping forced the browser to fetch the
    // backend cross-origin from the app's port, which depended on the host
    // nginx route + a CORS Origin/Host match and broke on http-only nodes
    // (e.g. .116: blank fields, QR "failed to fetch"). The app's nginx now
    // listens on 18083 directly (NOT 80 — that would collide with host nginx).
    ports: &[],
    host_network: true,
}];

const ELECTRS_UI: &[CompanionSpec] = &[CompanionSpec {
    name: "archy-electrs-ui",
    image_base: "electrs-ui",
    build_dir_candidates: &[
        "/opt/archipelago/docker/electrs-ui",
        "/home/archipelago/archy/docker/electrs-ui",
        "/home/archipelago/Projects/archy/docker/electrs-ui",
    ],
    pre_start: None,
    bind_mounts: &[],
    ports: &[],
    host_network: true,
}];

const FEDIMINT_UI: &[CompanionSpec] = &[CompanionSpec {
    name: "archy-fedimint-ui",
    image_base: "fedimint-ui",
    build_dir_candidates: &[
        "/opt/archipelago/docker/fedimint-ui",
        "/home/archipelago/archy/docker/fedimint-ui",
        "/home/archipelago/Projects/archy/docker/fedimint-ui",
    ],
    pre_start: None,
    bind_mounts: &[],
    ports: &[],
    host_network: true,
}];

const CUPRATE_UI: &[CompanionSpec] = &[CompanionSpec {
    name: "archy-cuprate-ui",
    image_base: "cuprate-ui",
    build_dir_candidates: &[
        "/opt/archipelago/docker/cuprate-ui",
        "/home/archipelago/archy/docker/cuprate-ui",
        "/home/archipelago/Projects/archy/docker/cuprate-ui",
    ],
    // No pre-start hook and no bind mounts: unlike bitcoin-ui there is no
    // secret to inject. Cuprate's restricted RPC (the only thing this UI
    // proxies) is unauthenticated by design — Monero's safe-for-public
    // subset — so the nginx.conf is baked into the image.
    pre_start: None,
    bind_mounts: &[],
    ports: &[],
    host_network: true,
}];

fn render_bitcoin_ui() -> futures_util::future::BoxFuture<'static, Result<()>> {
    Box::pin(async {
        let paths = crate::container::bitcoin_ui::RenderPaths::default();
        crate::container::bitcoin_ui::render(&paths)
            .await
            .map(|_| ())
            .context("render bitcoin-ui nginx.conf")
    })
}

/// Provision and start every companion for `package_id`. Each
/// companion is independent — a failure in one is logged but does
/// not abort the others.
pub async fn install_for(package_id: &str) -> Vec<(String, anyhow::Error)> {
    let mut failures = Vec::new();
    for spec in companions_for(package_id) {
        if let Err(e) = install_one(spec).await {
            warn!(companion = spec.name, error = %e, "companion install failed");
            failures.push((spec.name.to_string(), e));
        }
    }
    failures
}

/// Stop and remove every companion for `package_id`. Best effort:
/// errors are logged but do not abort the sequence.
pub async fn remove_for(package_id: &str) {
    let dir = match quadlet::unit_dir().await {
        Ok(d) => d,
        Err(e) => {
            warn!("companion remove: cannot resolve quadlet dir: {e:#}");
            return;
        }
    };
    for spec in companions_for(package_id) {
        if let Err(e) = quadlet::disable_remove(spec.name, &dir).await {
            warn!(companion = spec.name, error = %e, "companion remove failed");
        }
    }
}

/// Provision one companion: pre-start hook → image present → write
/// quadlet → daemon-reload → start.
pub async fn install_one(spec: &CompanionSpec) -> Result<()> {
    if let Some(hook) = spec.pre_start {
        hook().await.with_context(|| {
            format!(
                "pre-start hook failed for {} — companion will not start",
                spec.name
            )
        })?;
    }
    let image = ensure_image_present(spec).await?;
    let unit = build_unit(spec, &image);
    let dir = quadlet::unit_dir().await?;
    let changed = quadlet::write_if_changed(&unit, &dir).await?;
    if changed {
        info!(companion = spec.name, "wrote quadlet unit");
        quadlet::daemon_reload_user().await?;
    }
    // Start is idempotent — if already running, systemctl returns 0.
    quadlet::enable_now(&unit.service_name()).await?;

    // A rebuilt image does NOT reach a container that is already running.
    // `ensure_image_present` rebuilds in place under the same tag, so the unit
    // body is byte-identical, `write_if_changed` reports no change, and
    // `enable_now` is a no-op on a running service — the container keeps the
    // old layers indefinitely. That is exactly how a test node kept serving
    // the LND, FIPS, Electrs and Guardian screens on 0.0.0.0 after v1.7.123
    // rebuilt every one of those images to bind loopback: the images were
    // correct on disk and the running containers were three days old
    // (2026-08-05). Compare image IDs and restart when they diverge.
    if let Some(running) = container_image_id(spec.name).await {
        if let Some(built) = image_id(&image).await {
            if running != built {
                info!(
                    companion = spec.name,
                    "running container uses a stale image; restarting onto the rebuilt one"
                );
                quadlet::restart_service(&unit.service_name()).await?;
            }
        }
    }
    info!(companion = spec.name, "companion started");
    Ok(())
}

/// Image ID a container is actually running, or `None` when it does not exist.
async fn container_image_id(name: &str) -> Option<String> {
    let out = tokio::process::Command::new("podman")
        .args(["inspect", name, "--format", "{{.Image}}"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!id.is_empty()).then_some(id)
}

/// Current ID behind an image reference, or `None` when absent.
async fn image_id(image_ref: &str) -> Option<String> {
    let out = tokio::process::Command::new("podman")
        .args(["image", "inspect", image_ref, "--format", "{{.Id}}"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!id.is_empty()).then_some(id)
}

/// Build companion image locally if a Dockerfile exists, otherwise
/// pull from the lfg2025 registry. Returns the image ref the quadlet
/// should reference (`localhost/<base>:latest` for build, registry
/// URL for pull).
async fn ensure_image_present(spec: &CompanionSpec) -> Result<String> {
    let mut local_image = format!("localhost/{}:latest", spec.image_base);
    let local_image_compat = format!("localhost/{}:local", spec.image_base);
    let registry_image = format!("{}/{}:latest", COMPANION_REGISTRY, spec.image_base);

    // Prefer local build — companions can carry build-time customizations
    // (e.g. nginx.conf templates baked in). Search known candidates.
    for dir in spec.build_dir_candidates {
        let dockerfile = PathBuf::from(dir).join("Dockerfile");
        if fs::try_exists(&dockerfile).await.unwrap_or(false) {
            // Older installers and self-update create :local themselves. It
            // must receive source updates too; treating it as a permanent
            // manual override silently kept the old LND UI after an OTA.
            if image_exists(&local_image_compat).await {
                local_image = local_image_compat.clone();
            }
            // Reuse either local tag only when the build context has NOT
            // changed since it was built. Without this staleness check an
            // already-present image is reused forever, so edits to the baked-in
            // context (Dockerfile, nginx.conf, …) never reach the node — this is
            // exactly why the guardian-CSS nginx fix never reached the fleet.
            if image_exists(&local_image).await {
                if !context_is_newer_than_image(dir, &local_image).await {
                    return Ok(local_image);
                }
                info!(
                    companion = spec.name,
                    "build context changed since image built; rebuilding {dir}"
                );
            } else {
                info!(companion = spec.name, "building locally from {dir}");
            }
            // Stamp the context mtime we are building, so the staleness
            // check has something that advances even when every layer is a
            // cache hit. Without this the rebuild is a no-op that leaves
            // .Created unchanged, the check stays true, and the companion is
            // rebuilt on every reconcile tick forever.
            let context_stamp = newest_mtime_unix(PathBuf::from(dir))
                .await
                .unwrap_or_default();
            let stamp_label = format!("{CONTEXT_STAMP_LABEL}={context_stamp}");
            let out = command_output_with_timeout(
                Command::new("podman").args([
                    "build",
                    "--label",
                    &stamp_label,
                    "-t",
                    &local_image,
                    dir,
                ]),
                COMPANION_BUILD_TIMEOUT,
                "podman build companion image",
            )
            .await?;
            if out.status.success() {
                return Ok(local_image);
            }
            warn!(
                companion = spec.name,
                "local build failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            // Fall through to registry pull rather than fail outright.
            break;
        }
    }

    // Registry pull. Use insecure flag only for whitelisted hosts.
    let mut cmd = Command::new("podman");
    cmd.arg("pull");
    if image_uses_insecure_registry(&registry_image) {
        cmd.arg("--tls-verify=false");
    }
    cmd.arg(&registry_image);
    let out = command_output_with_timeout(
        &mut cmd,
        COMPANION_PULL_TIMEOUT,
        "podman pull companion image",
    )
    .await?;
    if !out.status.success() {
        anyhow::bail!(
            "no local Dockerfile and registry pull failed for {}: {}",
            spec.name,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(registry_image)
}

async fn image_exists(image: &str) -> bool {
    let mut cmd = Command::new("podman");
    // Only the exit status matters. WITHOUT a `--format`, `podman image inspect`
    // prints the image's full multi-KB manifest JSON; `.status()` inherits the
    // service's stdout, so on a hit that whole blob lands in the journal — once
    // per companion image, every reconcile pass. That flood spikes journald +
    // IO and starves the async runtime (UI websocket then drops → "connection
    // lost"/reconnect). Discard the child's stdout/stderr; we read neither.
    cmd.args(["image", "inspect", image])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    match tokio::time::timeout(COMPANION_IMAGE_CHECK_TIMEOUT, cmd.status()).await {
        Ok(Ok(status)) => status.success(),
        Ok(Err(err)) => {
            warn!(image = %image, error = %err, "companion image existence check failed");
            false
        }
        Err(_) => {
            warn!(image = %image, "companion image existence check timed out");
            false
        }
    }
}

/// Returns true if any file in the build context `dir` is newer than the
/// already-built `image`, signalling the cached image is stale and must be
/// rebuilt. Conservative: if either timestamp can't be determined we return
/// false (reuse the cache) to avoid rebuild storms on every reconcile pass.
/// Label carrying the context mtime an image was built from.
///
/// The reason this exists rather than reusing `.Created`: a rebuild whose
/// layers all hit the cache produces the SAME image, and podman leaves its
/// creation time untouched. Comparing against `.Created` therefore never
/// converges — the rebuild does not change the thing being tested, so the
/// companion is rebuilt on every reconcile tick indefinitely. A label is part
/// of the image config, so writing a new value always yields a new image,
/// which makes the comparison settle after exactly one rebuild.
const CONTEXT_STAMP_LABEL: &str = "org.archipelago.context-mtime";

async fn context_is_newer_than_image(dir: &str, image: &str) -> bool {
    let Some(ctx) = newest_mtime_unix(PathBuf::from(dir)).await else {
        return false;
    };
    // Preferred: what the last build actually stamped.
    if let Some(stamped) = image_context_stamp(image).await {
        return ctx > stamped;
    }
    // Images built before stamping existed have no label. Fall back to the
    // old comparison so behaviour is unchanged for them; the rebuild it
    // triggers writes the label, so each such image self-heals exactly once.
    match image_created_unix(image).await {
        Some(created) => ctx > created,
        None => false,
    }
}

/// The context mtime stamped into `image` at build time, if any.
async fn image_context_stamp(image: &str) -> Option<i64> {
    let format = format!("{{{{index .Config.Labels \"{CONTEXT_STAMP_LABEL}\"}}}}");
    let mut cmd = Command::new("podman");
    cmd.args(["image", "inspect", "--format", &format, image]);
    let out = command_output_with_timeout(
        &mut cmd,
        COMPANION_IMAGE_CHECK_TIMEOUT,
        "podman image context stamp",
    )
    .await
    .ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    let raw = raw.trim();
    // podman prints "<no value>" for a missing label.
    if raw.is_empty() || raw == "<no value>" {
        return None;
    }
    raw.parse::<i64>().ok()
}

/// Build timestamp of `image` as Unix seconds, via `podman image inspect`.
async fn image_created_unix(image: &str) -> Option<i64> {
    let mut cmd = Command::new("podman");
    cmd.args(["image", "inspect", "--format", "{{.Created.Unix}}", image]);
    let out = command_output_with_timeout(
        &mut cmd,
        COMPANION_IMAGE_CHECK_TIMEOUT,
        "podman image created time",
    )
    .await
    .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<i64>()
        .ok()
}

/// Newest modification time (Unix seconds) across all files under `dir`,
/// walked recursively. Runs on a blocking thread since it touches the fs.
async fn newest_mtime_unix(dir: PathBuf) -> Option<i64> {
    tokio::task::spawn_blocking(move || newest_mtime_blocking(&dir))
        .await
        .ok()
        .flatten()
}

fn newest_mtime_blocking(dir: &std::path::Path) -> Option<i64> {
    let mut newest: Option<i64> = None;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(p) = stack.pop() {
        let entries = match std::fs::read_dir(&p) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if let Ok(modified) = meta.modified() {
                if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                    let secs = dur.as_secs() as i64;
                    newest = Some(newest.map_or(secs, |n| n.max(secs)));
                }
            }
        }
    }
    newest
}

async fn command_output_with_timeout(
    cmd: &mut Command,
    timeout: Duration,
    description: &str,
) -> Result<std::process::Output> {
    cmd.kill_on_drop(true);
    tokio::time::timeout(timeout, cmd.output())
        .await
        .with_context(|| format!("{description} timed out after {}s", timeout.as_secs()))?
        .with_context(|| format!("spawn {description}"))
}

fn build_unit(spec: &CompanionSpec, image: &str) -> QuadletUnit {
    QuadletUnit {
        name: spec.name.into(),
        description: format!("Archipelago companion UI: {}", spec.name),
        image: image.into(),
        network: if spec.host_network {
            NetworkMode::Host
        } else {
            NetworkMode::Bridge("bridge".into())
        },
        // Run as root inside the container so nginx can chown its
        // worker dirs. Rootless podman maps this to a high host UID,
        // so it is unprivileged on the host.
        user: Some("0:0".into()),
        memory_mb: Some(128),
        cap_drop_all: true,
        cap_add: vec![
            "CHOWN".into(),
            "DAC_OVERRIDE".into(),
            "NET_BIND_SERVICE".into(),
            "SETUID".into(),
            "SETGID".into(),
        ],
        bind_mounts: spec
            .bind_mounts
            .iter()
            .map(|(host, container)| BindMount {
                host: PathBuf::from(*host),
                container: PathBuf::from(*container),
                read_only: true,
            })
            .collect(),
        ports: spec
            .ports
            .iter()
            .map(|(host, container)| (*host, *container, "tcp".into(), String::new()))
            .collect(),
        extra_podman_args: vec![],
        depends_on: vec![],
        // Companions don't use the backend-manifest extension fields;
        // the renderer skips empty/false directives so the rendered
        // bytes are unchanged from before quadlet.rs grew the new fields.
        ..QuadletUnit::default()
    }
}

/// Is a user systemd manager reachable? In production archipelago.service
/// inherits XDG_RUNTIME_DIR from systemd; in unit tests / CI sandboxes it
/// is unset, in which case `systemctl --user` would fail and write to
/// HOME would be an unwanted side effect. The reconciler skips its
/// companion stage when this is false.
fn user_systemd_available() -> bool {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// Reconcile companion presence: every expected companion for the
/// given installed apps must have its quadlet unit on disk and its
/// service active. Returns a list of (companion, error) for anything
/// that needed correction and failed.
///
/// Called from `boot_reconciler` so a deleted unit file or a stopped
/// service is repaired within one tick. No-ops if the user systemd
/// manager is not reachable (CI / test environments).
pub async fn reconcile(installed_apps: &[String]) -> Vec<(String, anyhow::Error)> {
    if !user_systemd_available() {
        return Vec::new();
    }
    let mut failures = Vec::new();
    for app_id in installed_apps {
        for spec in companions_for(app_id) {
            match needs_repair(spec).await {
                Ok(false) => {}
                Ok(true) => {
                    if let Some(failed_at) =
                        REPAIR_FAILED_AT.lock().unwrap().get(spec.name).copied()
                    {
                        if failed_at.elapsed() < REPAIR_COOLDOWN {
                            continue;
                        }
                    }
                    info!(
                        companion = spec.name,
                        "reconcile: companion not active, repairing"
                    );
                    match install_one(spec).await {
                        Ok(()) => {
                            REPAIR_FAILED_AT.lock().unwrap().remove(spec.name);
                        }
                        Err(e) => {
                            REPAIR_FAILED_AT
                                .lock()
                                .unwrap()
                                .insert(spec.name, Instant::now());
                            failures.push((spec.name.to_string(), e));
                        }
                    }
                }
                Err(e) => {
                    warn!(companion = spec.name, error = %e, "reconcile probe failed");
                    failures.push((spec.name.to_string(), e));
                }
            }
        }
    }
    failures
}

/// Companions this build knows about that no app in `installed_apps` claims.
///
/// Pure set arithmetic, split out from `reap_orphans` so the "which ones go"
/// decision is testable without a systemd manager. A companion shared by
/// several backends (archy-bitcoin-ui serves both bitcoin-core and
/// bitcoin-knots) survives while ANY of its backends is installed.
fn orphan_companions(installed_apps: &[String]) -> Vec<&'static CompanionSpec> {
    let expected: std::collections::HashSet<&str> = installed_apps
        .iter()
        .flat_map(|app_id| companions_for(app_id))
        .map(|spec| spec.name)
        .collect();
    ALL_COMPANIONS
        .iter()
        .copied()
        .flatten()
        .filter(|spec| !expected.contains(spec.name))
        .collect()
}

/// Narrow `orphans` to those that have been orphaned for at least
/// `ORPHAN_GRACE`, updating `since` in place.
///
/// Split out of `reap_orphans` and given an explicit `now` so the grace
/// behaviour is testable without sleeping: it is the guard that stops a
/// restarting backend from costing its companion a teardown+reinstall.
fn due_after_grace(
    orphans: Vec<&'static CompanionSpec>,
    orphan_names: &std::collections::HashSet<&str>,
    since: &mut HashMap<&'static str, Instant>,
    now: Instant,
) -> Vec<&'static CompanionSpec> {
    // A companion whose backend came back is no longer a candidate; drop its
    // clock so a later disappearance waits out a fresh grace period rather
    // than inheriting a stale one.
    since.retain(|name, _| orphan_names.contains(name));
    orphans
        .into_iter()
        .filter(|spec| {
            let first_seen = *since.entry(spec.name).or_insert(now);
            now.duration_since(first_seen) >= ORPHAN_GRACE
        })
        .collect()
}

/// Stop and remove any companion whose backend app is not installed.
///
/// ⚠️ WIRED (2026-08-10) to exactly one caller — the boot reconciler's
/// companion loop — and ONLY behind the durable installed-apps registry
/// (`crash_recovery::load_installed_apps_if_recorded`). That satisfies the
/// bar the 2026-08-08 unwire set: a DURABLE record of "this app is
/// installed" drives it, never runtime inference. Do not add callers fed
/// from runtime state; the history below is why.
///
/// It ran on archi-dev-box on 2026-08-08 and removed two companions whose
/// backends were installed — archy-bitcoin-ui (36 minutes of no Bitcoin UI)
/// and archy-lnd-ui. Not a bug in the arithmetic below: the inputs were false.
/// Both backends' containers were missing because of the clean-exit vanishing
/// bug, and both had aged out of `running-containers.json`, which only records
/// what is CURRENTLY RUNNING. The two signals `installed_app_ids` combines are
/// therefore not independent — one root cause falsifies both at once, and
/// `ORPHAN_GRACE` cannot help because the condition is persistent, not
/// transient.
///
/// The asymmetry that settles it: an un-reaped orphan costs a stale UI tile,
/// while a wrongly-reaped companion costs a working screen and turns one lost
/// app into two. Absence of evidence of installation is not evidence of
/// uninstallation.
///
/// The counterpart to `reconcile`, which can only ever *add*. Without this,
/// a companion outlives its backend permanently: `remove_for` fires only on
/// the explicit uninstall RPC path, so an install that fails after the
/// companion lands, a container removed by hand, or a node whose app was
/// never installed at all keeps a `Restart=always` unit alive forever.
///
/// `installed_apps` MUST be the full installed set (see
/// `ProdOrchestrator::installed_app_ids`), never the narrow per-app list
/// `reconcile_companions_for` passes — reaping against a one-app list would
/// tear down every other companion on the node. It must also never be the
/// *manifest* list, which is every available app rather than every installed
/// one; that mistake is what left the orphans this function now clears.
///
/// Callers must not invoke this when they could not determine what is
/// installed. "I could not look" and "nothing is installed" produce the same
/// empty vector but demand opposite behaviour, so the check belongs upstream
/// where the distinction still exists.
pub async fn reap_orphans(installed_apps: &[String]) -> Vec<(String, anyhow::Error)> {
    if !user_systemd_available() {
        return Vec::new();
    }
    let orphans = orphan_companions(installed_apps);

    // Age the observation before acting on it. Anything whose backend is back
    // has its clock cleared; anything still orphaned must have been so for a
    // full ORPHAN_GRACE before it is touched.
    let orphan_names: std::collections::HashSet<&str> =
        orphans.iter().map(|spec| spec.name).collect();
    let due: Vec<&'static CompanionSpec> = {
        let mut since = ORPHAN_SINCE.lock().unwrap();
        due_after_grace(orphans, &orphan_names, &mut since, Instant::now())
    };
    if due.is_empty() {
        return Vec::new();
    }

    let dir = match quadlet::unit_dir().await {
        Ok(d) => d,
        Err(e) => {
            warn!("companion reap: cannot resolve quadlet dir: {e:#}");
            return Vec::new();
        }
    };

    let mut failures = Vec::new();
    for spec in due {
        // Only act on companions that are actually present, so a node that
        // never had the app stays silent instead of logging every tick.
        let unit_path = dir.join(format!("{}.container", spec.name));
        let unit_present = fs::try_exists(&unit_path).await.unwrap_or(false);
        if !unit_present {
            // No unit file, so the only reason to act is a service still
            // running from a removed one. A hung `is-active` under IO pressure
            // must read as "leave it alone" — reaping is destructive, so every
            // uncertain signal resolves toward doing nothing.
            let svc = format!("{}.service", spec.name);
            match tokio::time::timeout(Duration::from_secs(10), quadlet::is_active(&svc)).await {
                Ok(true) => {}
                Ok(false) => continue,
                Err(_) => {
                    warn!(
                        companion = spec.name,
                        "reap: is-active probe timed out; leaving it alone"
                    );
                    continue;
                }
            }
        }
        info!(
            companion = spec.name,
            "reap: backend app is not installed, removing orphaned companion"
        );
        if let Err(e) = quadlet::disable_remove(spec.name, &dir).await {
            warn!(companion = spec.name, error = %e, "companion reap failed");
            failures.push((spec.name.to_string(), e));
        }
    }
    failures
}

/// Does this companion need install_one to be re-run? Returns true if
/// the unit file is missing, stale, or the service is not active.
///
/// This probe runs every reconcile tick for every companion, so it must be
/// PASSIVE: no image builds, no pulls. It used to call ensure_image_present
/// to render the expected unit — under IO pressure the image-existence check
/// inside timed out, read as "image missing", and a 900s `podman build` ran
/// inside the probe even though the companion was up (the .198 load spiral).
async fn needs_repair(spec: &CompanionSpec) -> Result<bool> {
    let dir = quadlet::unit_dir().await?;
    let unit_path = dir.join(format!("{}.container", spec.name));
    if !fs::try_exists(&unit_path).await.unwrap_or(false) {
        return Ok(true);
    }
    let svc = format!("{}.service", spec.name);
    // A hung `systemctl is-active` under IO pressure must not read as
    // "companion dead" — that's a repair (and possibly an image build) fired
    // off exactly when the node can least afford one.
    match tokio::time::timeout(Duration::from_secs(10), quadlet::is_active(&svc)).await {
        Ok(active) => {
            if !active {
                return Ok(true);
            }
        }
        Err(_) => {
            warn!(
                companion = spec.name,
                "is-active probe timed out; assuming active"
            );
        }
    }
    // Service is running. Flag it stale only on definitive, cheap signals:
    // the on-disk unit matching none of the image refs install_one could
    // have written, or a local build context newer than the built image.
    let on_disk = fs::read_to_string(&unit_path).await.unwrap_or_default();
    let local_image = format!("localhost/{}:latest", spec.image_base);
    let local_image_compat = format!("localhost/{}:local", spec.image_base);
    let registry_image = format!("{}/{}:latest", COMPANION_REGISTRY, spec.image_base);
    let matches_known_shape = [&local_image, &local_image_compat, &registry_image]
        .iter()
        .any(|img| build_unit(spec, img).render() == on_disk);
    if !matches_known_shape {
        return Ok(true);
    }
    if let Some(image) = managed_local_image(spec, &on_disk) {
        for dir in spec.build_dir_candidates {
            let dockerfile = PathBuf::from(dir).join("Dockerfile");
            if fs::try_exists(&dockerfile).await.unwrap_or(false) {
                // Conservative on any timeout/error inside: reuse the cache.
                return Ok(context_is_newer_than_image(dir, &image).await);
            }
        }
    }
    Ok(false)
}

fn managed_local_image(spec: &CompanionSpec, unit: &str) -> Option<String> {
    ["latest", "local"]
        .iter()
        .map(|tag| format!("localhost/{}:{tag}", spec.image_base))
        .find(|image| build_unit(spec, image).render() == unit)
}

#[cfg(test)]
mod tests {
    #[test]
    fn legacy_installer_local_tag_is_checked_for_source_updates_like_latest() {
        for spec in ALL_COMPANIONS.iter().flat_map(|group| group.iter()) {
            for tag in ["local", "latest"] {
                let image = format!("localhost/{}:{tag}", spec.image_base);
                let unit = build_unit(spec, &image).render();
                assert_eq!(managed_local_image(spec, &unit), Some(image));
            }
            let registry = format!("{}/{}:latest", COMPANION_REGISTRY, spec.image_base);
            assert_eq!(
                managed_local_image(spec, &build_unit(spec, &registry).render()),
                None
            );
        }
    }

    use super::*;

    fn names(specs: &[&'static CompanionSpec]) -> Vec<&'static str> {
        let mut v: Vec<_> = specs.iter().map(|s| s.name).collect();
        v.sort_unstable();
        v
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn every_companion_in_companions_for_is_also_in_all_companions() {
        // The reaper only recognises companions listed in ALL_COMPANIONS. One
        // missing from it would be provisioned by `reconcile` and then never
        // cleaned up — exactly the leak this module is fixing.
        let backends = [
            "bitcoin",
            "bitcoin-core",
            "bitcoin-knots",
            "lnd",
            "electrumx",
            "electrs",
            "mempool-electrs",
            "fedimint",
            "fedimintd",
            "cuprate",
        ];
        let known: std::collections::HashSet<&str> = ALL_COMPANIONS
            .iter()
            .copied()
            .flatten()
            .map(|s| s.name)
            .collect();
        for backend in backends {
            for spec in companions_for(backend) {
                assert!(
                    known.contains(spec.name),
                    "{} is provisionable but not reapable — add it to ALL_COMPANIONS",
                    spec.name
                );
            }
        }
    }

    #[test]
    fn nothing_installed_orphans_every_companion() {
        assert_eq!(
            names(&orphan_companions(&[])),
            vec![
                "archy-bitcoin-ui",
                "archy-cuprate-ui",
                "archy-electrs-ui",
                "archy-fedimint-ui",
                "archy-lnd-ui"
            ]
        );
    }

    #[test]
    fn an_installed_backend_protects_only_its_own_companion() {
        // The archi-dev-box state that exposed the bug: bitcoin-knots and
        // electrumx installed, fedimint and lnd not — yet all four companions
        // were running because the reconciler was fed the manifest list.
        let orphans = orphan_companions(&ids(&["bitcoin-knots", "electrumx"]));
        assert_eq!(
            names(&orphans),
            vec!["archy-cuprate-ui", "archy-fedimint-ui", "archy-lnd-ui"]
        );
    }

    #[test]
    fn a_shared_companion_survives_on_any_one_of_its_backends() {
        // archy-bitcoin-ui serves bitcoin-core AND bitcoin-knots. Installing
        // either must keep it; a naive per-app reap would remove it while the
        // other backend was still running.
        for backend in ["bitcoin", "bitcoin-core", "bitcoin-knots"] {
            let orphans = orphan_companions(&ids(&[backend]));
            assert!(
                !names(&orphans).contains(&"archy-bitcoin-ui"),
                "archy-bitcoin-ui reaped while {backend} is installed"
            );
        }
    }

    #[test]
    fn apps_without_companions_orphan_everything_and_panic_nothing() {
        let orphans = orphan_companions(&ids(&["nextcloud", "not-a-real-app"]));
        assert_eq!(orphans.len(), 5);
    }

    #[test]
    fn every_backend_installed_leaves_no_orphans() {
        let orphans = orphan_companions(&ids(&[
            "bitcoin-knots",
            "lnd",
            "electrumx",
            "fedimint",
            "cuprate",
        ]));
        assert!(
            names(&orphans).is_empty(),
            "unexpected orphans: {:?}",
            names(&orphans)
        );
    }

    fn name_set(specs: &[&'static CompanionSpec]) -> std::collections::HashSet<&'static str> {
        specs.iter().map(|s| s.name).collect()
    }

    #[test]
    fn a_freshly_orphaned_companion_is_not_reaped_immediately() {
        let orphans = orphan_companions(&ids(&["bitcoin-knots"]));
        let names_seen = name_set(&orphans);
        let mut since = HashMap::new();
        let now = Instant::now();
        let due = due_after_grace(orphans, &names_seen, &mut since, now);
        assert!(
            due.is_empty(),
            "reaped on the first observation: {:?}",
            names(&due)
        );
    }

    #[test]
    fn an_orphan_past_the_grace_period_is_reaped() {
        let orphans = orphan_companions(&ids(&["bitcoin-knots"]));
        let names_seen = name_set(&orphans);
        let mut since = HashMap::new();
        let start = Instant::now();
        // First pass records the clock and reaps nothing.
        let due = due_after_grace(orphans.clone(), &names_seen, &mut since, start);
        assert!(due.is_empty());
        // A pass after the grace window reaps.
        let due = due_after_grace(orphans, &names_seen, &mut since, start + ORPHAN_GRACE);
        assert_eq!(
            names(&due),
            vec![
                "archy-cuprate-ui",
                "archy-electrs-ui",
                "archy-fedimint-ui",
                "archy-lnd-ui"
            ]
        );
    }

    #[test]
    fn a_backend_returning_mid_grace_resets_the_clock() {
        // The restart window this guard exists for: lnd vanishes for a tick
        // while its container is recreated, then comes back. Its companion
        // must never be reaped, and a later real uninstall must wait out a
        // full fresh grace period rather than inheriting the old clock.
        let start = Instant::now();
        let mut since = HashMap::new();

        let orphans = orphan_companions(&ids(&["bitcoin-knots"]));
        let names_seen = name_set(&orphans);
        assert!(due_after_grace(orphans, &names_seen, &mut since, start).is_empty());

        // lnd is back — it is no longer an orphan candidate.
        let orphans = orphan_companions(&ids(&["bitcoin-knots", "lnd"]));
        let names_seen = name_set(&orphans);
        let due = due_after_grace(orphans, &names_seen, &mut since, start + ORPHAN_GRACE);
        assert!(
            !names(&due).contains(&"archy-lnd-ui"),
            "lnd companion reaped even though lnd came back"
        );
        assert!(
            !since.contains_key("archy-lnd-ui"),
            "stale clock kept for lnd"
        );

        // lnd goes away for real. It must wait a fresh full grace period.
        let orphans = orphan_companions(&ids(&["bitcoin-knots"]));
        let names_seen = name_set(&orphans);
        let t = start + ORPHAN_GRACE;
        let due = due_after_grace(orphans.clone(), &names_seen, &mut since, t);
        assert!(
            !names(&due).contains(&"archy-lnd-ui"),
            "lnd companion reaped without a fresh grace period"
        );
        let due = due_after_grace(orphans, &names_seen, &mut since, t + ORPHAN_GRACE);
        assert!(names(&due).contains(&"archy-lnd-ui"));
    }

    #[test]
    fn companions_for_known_apps_returns_expected_set() {
        assert_eq!(companions_for("bitcoin-knots").len(), 1);
        assert_eq!(companions_for("bitcoin-core").len(), 1);
        assert_eq!(companions_for("bitcoin").len(), 1);
        assert_eq!(companions_for("lnd").len(), 1);
        assert_eq!(companions_for("electrumx").len(), 1);
        assert_eq!(companions_for("electrs").len(), 1);
        assert_eq!(companions_for("mempool-electrs").len(), 1);
        assert_eq!(companions_for("fedimint").len(), 1);
        assert_eq!(companions_for("fedimintd").len(), 1);
        assert_eq!(companions_for("cuprate").len(), 1);
        assert_eq!(companions_for("nextcloud").len(), 0);
        assert_eq!(companions_for("not-a-real-app").len(), 0);
    }

    #[test]
    fn build_unit_uses_host_network_and_drops_caps() {
        let spec = &BITCOIN_UI[0];
        let u = build_unit(spec, "localhost/bitcoin-ui:latest");
        assert_eq!(u.name, "archy-bitcoin-ui");
        assert!(matches!(u.network, NetworkMode::Host));
        assert!(u.cap_drop_all);
        assert!(u.cap_add.iter().any(|c| c == "NET_BIND_SERVICE"));
        assert_eq!(u.user.as_deref(), Some("0:0"));
        assert_eq!(u.memory_mb, Some(128));
        assert_eq!(u.bind_mounts.len(), 1);
        assert_eq!(
            u.bind_mounts[0].container,
            PathBuf::from("/etc/nginx/conf.d/default.conf")
        );
        assert!(u.bind_mounts[0].read_only);
    }

    #[test]
    fn lnd_ui_uses_host_network_for_same_origin_backend_proxy() {
        // lnd-ui is host-networked (its nginx listens on 18083 directly) so the
        // app can proxy the archipelago backend same-origin instead of fetching
        // it cross-origin from its app port — see the spec comment for why.
        let spec = &LND_UI[0];
        let u = build_unit(spec, "localhost/lnd-ui:latest");
        assert_eq!(u.name, "archy-lnd-ui");
        assert!(matches!(u.network, NetworkMode::Host));
        assert!(u.ports.is_empty());
    }

    #[test]
    fn fedimint_ui_uses_host_network_for_public_guardian_port() {
        let spec = &FEDIMINT_UI[0];
        let u = build_unit(spec, "localhost/fedimint-ui:latest");
        assert_eq!(u.name, "archy-fedimint-ui");
        assert!(matches!(u.network, NetworkMode::Host));
        assert!(u.ports.is_empty());
    }
}
