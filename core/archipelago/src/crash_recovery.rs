// Crash Recovery Module
// Detects unexpected shutdowns and restarts containers that were running before the crash.
//
// How it works:
// 1. On startup, write a PID file as a "running" marker
// 2. Periodically snapshot which containers are running to a state file
// 3. On clean shutdown, remove the PID file
// 4. On next startup, if the PID file exists → previous instance crashed
// 5. On crash recovery: read the saved container list, restart them, log actions

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::fs;
use tracing::{info, warn};

const PID_FILE: &str = "archipelago.pid";
const CONTAINER_STATE_FILE: &str = "running-containers.json";
const USER_STOPPED_FILE: &str = "user-stopped.json";
const USER_UNINSTALLED_FILE: &str = "user-uninstalled.json";

/// Shared flag: true once boot recovery is complete. Health monitor should wait for this.
pub static RECOVERY_COMPLETE: AtomicBool = AtomicBool::new(false);

/// Process start time for uptime calculation.
static START_TIME: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Initialize the start time. Call once at startup.
pub fn init_start_time() {
    START_TIME.get_or_init(Instant::now);
}

/// Get uptime in seconds since process start.
pub fn uptime_seconds() -> u64 {
    START_TIME.get().map(|t| t.elapsed().as_secs()).unwrap_or(0)
}

/// Mark boot recovery as complete. Call after crash recovery + start_stopped_containers finish.
pub fn mark_recovery_complete() {
    RECOVERY_COMPLETE.store(true, Ordering::SeqCst);
    info!("Boot recovery complete — health monitor may proceed");
}

/// Check if boot recovery is done.
pub fn is_recovery_complete() -> bool {
    RECOVERY_COMPLETE.load(Ordering::SeqCst)
}

// ── Pending boot-start tracking ─────────────────────────────────────────
// Containers that boot recovery / the reconciler is about to start (or is
// starting right now). The package scanner overlays these as `Restarting`
// instead of the raw podman `Stopped`/`Exited`, so a freshly rebooted node
// doesn't tell the user their apps are "Stopped" while the sequential
// recovery pass (3s stagger + up to minutes for heavyweights like bitcoin)
// is still working through the queue. Writers register names when a pass
// begins and remove each name once its start attempt finishes, whatever
// the outcome — a container that truly failed goes back to showing its
// real state on the next scan.

static PENDING_BOOT_STARTS: std::sync::LazyLock<
    std::sync::RwLock<std::collections::HashSet<String>>,
> = std::sync::LazyLock::new(|| std::sync::RwLock::new(std::collections::HashSet::new()));

/// Register container/app names an active recovery or reconcile pass
/// intends to start.
pub fn pending_boot_starts_add<I: IntoIterator<Item = String>>(names: I) {
    if let Ok(mut set) = PENDING_BOOT_STARTS.write() {
        set.extend(names);
    }
}

/// A start attempt for `name` finished (success or failure) — stop
/// overlaying it.
pub fn pending_boot_start_done(name: &str) {
    if let Ok(mut set) = PENDING_BOOT_STARTS.write() {
        set.remove(name);
    }
}

/// Whether `name` (a container name or scanner app id) is queued for a
/// boot/reconcile start. Container names may carry an `archy-` prefix the
/// scanner strips when deriving app ids, so check both forms.
pub fn is_pending_boot_start(name: &str) -> bool {
    let Ok(set) = PENDING_BOOT_STARTS.read() else {
        return false;
    };
    set.contains(name) || set.contains(&format!("archy-{name}"))
}

// ── User-stopped tracking ───────────────────────────────────────────────
// When a user explicitly stops a container via the UI, we record it here
// so crash recovery and health monitor don't auto-restart it.

/// Load the set of user-stopped containers from disk.
pub async fn load_user_stopped(data_dir: &Path) -> std::collections::HashSet<String> {
    let path = data_dir.join(USER_STOPPED_FILE);
    match fs::read_to_string(&path).await {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => std::collections::HashSet::new(),
    }
}

/// Names of the containers that were running at the last periodic snapshot
/// (`running-containers.json`, saved every ~120s by `save_container_snapshot`).
/// Unlike `check_for_crash`, this reads the snapshot unconditionally (no PID/crash
/// gate) — it's the durable "what was running" signal the boot reconciler uses to
/// recreate a previously-running app whose container vanished. Empty if absent.
pub async fn load_last_running_names(data_dir: &Path) -> std::collections::HashSet<String> {
    let path = data_dir.join(CONTAINER_STATE_FILE);
    match fs::read_to_string(&path).await {
        Ok(content) => match serde_json::from_str::<ContainerSnapshot>(&content) {
            Ok(snapshot) => snapshot.containers.into_iter().map(|c| c.name).collect(),
            Err(_) => std::collections::HashSet::new(),
        },
        Err(_) => std::collections::HashSet::new(),
    }
}

/// Test helper: write a running-containers snapshot with the given names —
/// the "installation evidence" the boot reconciler's desired-state recovery
/// reads via `load_last_running_names`.
#[cfg(test)]
pub async fn save_container_snapshot_for_test(data_dir: &Path, names: &[&str]) {
    let snapshot = ContainerSnapshot {
        timestamp: 0,
        containers: names
            .iter()
            .map(|n| RunningContainerRecord {
                name: n.to_string(),
                image: format!("test/{n}:latest"),
            })
            .collect(),
    };
    let path = data_dir.join(CONTAINER_STATE_FILE);
    let _ = fs::write(&path, serde_json::to_string_pretty(&snapshot).unwrap()).await;
}

/// Save the set of user-stopped containers to disk.
pub async fn save_user_stopped(data_dir: &Path, stopped: &std::collections::HashSet<String>) {
    let path = data_dir.join(USER_STOPPED_FILE);
    if let Ok(json) = serde_json::to_string_pretty(stopped) {
        let _ = fs::write(&path, json).await;
    }
}

/// Mark a container as user-stopped (won't be auto-restarted).
pub async fn mark_user_stopped(data_dir: &Path, name: &str) {
    let mut stopped = load_user_stopped(data_dir).await;
    stopped.insert(name.to_string());
    save_user_stopped(data_dir, &stopped).await;
}

/// Clear user-stopped flag (container was manually started by user).
pub async fn clear_user_stopped(data_dir: &Path, name: &str) {
    let mut stopped = load_user_stopped(data_dir).await;
    if stopped.remove(name) {
        save_user_stopped(data_dir, &stopped).await;
    }
}

// ── User-uninstalled tracking ───────────────────────────────────────────
// Baseline apps (bitcoin-knots, electrumx, lnd, mempool, ...) self-heal when
// their container is missing — see `is_required_baseline_app` in
// prod_orchestrator.rs — because they're expected to exist from first boot.
// That self-heal has no way to distinguish "container vanished after a
// crash" from "user explicitly uninstalled this," and the in-memory
// `disabled` set the orchestrator otherwise uses is wiped by every
// `load_manifests()` call (once per archipelago startup). Without a durable
// marker, uninstalling a baseline app only "sticks" until the next reboot or
// archipelago restart, at which point the boot reconciler resurrects it.
// This mirrors `user_stopped` exactly, just for uninstall instead of stop.

// The durable counterpart to `user-uninstalled`: what IS installed.
//
// Recovery of a vanished app used to be decided from `running-containers.json`
// ("what was running at the last snapshot"), which is a different question and
// a perishable answer — it only records what is running NOW, so an app that
// stays down long enough simply ages out of it. Once out, boot's
// `ExistingOnly` mode will not recreate it, because it cannot tell "installed
// and lost" from "merely available in the catalog". The app is then gone for
// good with its manifest still on disk and nothing to bring it back.
//
// That has now happened twice. indeedhub-minio/-postgres went permanently
// absent on one node (2026-08-06) — patched narrowly with
// `absent_stack_member_with_live_sibling`, which only rescues a stack member
// that still has a live sibling. bitcoin-knots is standalone, so on
// archi-dev-box (2026-08-08) it vanished, aged out, and stayed gone while LND
// crash-looped on it for hours and a fedimint container waited 30 hours for a
// host that no longer resolved.
//
// Installation is a decision, not a runtime observation, so it gets a record
// of its own that no amount of downtime erodes.
const INSTALLED_APPS_FILE: &str = "installed-apps.json";
static INSTALLED_APPS_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Load the durable set of installed app ids / container names.
pub async fn load_installed_apps(data_dir: &Path) -> std::collections::HashSet<String> {
    let path = data_dir.join(INSTALLED_APPS_FILE);
    match fs::read_to_string(&path).await {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => std::collections::HashSet::new(),
    }
}

/// Like `load_installed_apps`, but keeps "no record" distinguishable from
/// "empty record". The companion reaper must only ever run on `Some`:
/// "I could not look" and "nothing is installed" both come back as an empty
/// set from the lossy loader, yet they demand opposite behaviour — the
/// distinction has to survive to the caller (see `reap_orphans`' contract).
pub async fn load_installed_apps_if_recorded(
    data_dir: &Path,
) -> Option<std::collections::HashSet<String>> {
    let path = data_dir.join(INSTALLED_APPS_FILE);
    let content = fs::read_to_string(&path).await.ok()?;
    serde_json::from_str(&content).ok()
}

async fn save_installed_apps(data_dir: &Path, installed: &std::collections::HashSet<String>) {
    let path = data_dir.join(INSTALLED_APPS_FILE);
    if let Ok(json) = serde_json::to_string_pretty(installed) {
        let tmp = path.with_extension("json.tmp");
        let result = async {
            fs::write(&tmp, json).await?;
            fs::File::open(&tmp).await?.sync_all().await?;
            fs::rename(&tmp, &path).await?;
            fs::File::open(data_dir).await?.sync_all().await
        }
        .await;
        if let Err(error) = result {
            warn!(%error, "could not persist installed apps");
        }
    }
}

/// Record that an app is installed. Called when an install succeeds.
pub async fn mark_installed(data_dir: &Path, name: &str) {
    let _guard = INSTALLED_APPS_LOCK.lock().await;
    let mut installed = load_installed_apps(data_dir).await;
    if installed.insert(name.to_string()) {
        save_installed_apps(data_dir, &installed).await;
    }
}

/// Forget an app. Called on uninstall, beside `mark_user_uninstalled` — the
/// two must move together or a reinstall-after-uninstall leaves a stale claim.
pub async fn clear_installed(data_dir: &Path, name: &str) {
    let _guard = INSTALLED_APPS_LOCK.lock().await;
    let mut installed = load_installed_apps(data_dir).await;
    if installed.remove(name) {
        save_installed_apps(data_dir, &installed).await;
    }
}

/// Seed the record on a node that predates it, from apps that demonstrably
/// exist right now.
///
/// Deliberately additive and one-way: it only ever ADDS names that have a real
/// container, so it cannot invent an install, and it never removes. A node
/// upgrading into this feature therefore starts with a truthful, conservative
/// record instead of an empty one that would make every existing app look
/// uninstalled — which would disable recovery for exactly the apps that most
/// need it. Runs on every boot, so an app installed before the upgrade is
/// still picked up whenever it is next seen alive.
pub async fn backfill_installed_apps(data_dir: &Path, present_container_names: &[String]) {
    let _guard = INSTALLED_APPS_LOCK.lock().await;
    if present_container_names.is_empty() {
        return;
    }
    let mut installed = load_installed_apps(data_dir).await;
    let uninstalled = load_user_uninstalled(data_dir).await;
    let mut changed = false;
    for name in present_container_names {
        // Never re-claim something the operator deliberately removed: the
        // container can still be running under systemd after an uninstall.
        if uninstalled.contains(name) {
            continue;
        }
        if installed.insert(name.clone()) {
            changed = true;
        }
    }
    if changed {
        save_installed_apps(data_dir, &installed).await;
    }
}

/// Load the set of explicitly user-uninstalled app/container names from disk.
pub async fn load_user_uninstalled(data_dir: &Path) -> std::collections::HashSet<String> {
    let path = data_dir.join(USER_UNINSTALLED_FILE);
    match fs::read_to_string(&path).await {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => std::collections::HashSet::new(),
    }
}

/// Save the set of user-uninstalled app/container names to disk.
pub async fn save_user_uninstalled(
    data_dir: &Path,
    uninstalled: &std::collections::HashSet<String>,
) {
    let path = data_dir.join(USER_UNINSTALLED_FILE);
    if let Ok(json) = serde_json::to_string_pretty(uninstalled) {
        let _ = fs::write(&path, json).await;
    }
}

/// Mark a name as user-uninstalled (won't be self-healed by the baseline-app
/// reconciler across restarts/reboots).
pub async fn mark_user_uninstalled(data_dir: &Path, name: &str) {
    let mut uninstalled = load_user_uninstalled(data_dir).await;
    uninstalled.insert(name.to_string());
    save_user_uninstalled(data_dir, &uninstalled).await;
}

/// Clear the user-uninstalled flag (app was explicitly (re)installed/started).
pub async fn clear_user_uninstalled(data_dir: &Path, name: &str) {
    let mut uninstalled = load_user_uninstalled(data_dir).await;
    if uninstalled.remove(name) {
        save_user_uninstalled(data_dir, &uninstalled).await;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningContainerRecord {
    pub name: String,
    pub image: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContainerSnapshot {
    pub timestamp: u64,
    pub containers: Vec<RunningContainerRecord>,
}

/// Check if the previous instance crashed (PID file exists without a clean shutdown).
/// Returns the list of containers that were running before the crash, if any.
pub async fn check_for_crash(data_dir: &Path) -> Result<Option<Vec<RunningContainerRecord>>> {
    let pid_path = data_dir.join(PID_FILE);

    if !pid_path.exists() {
        return Ok(None);
    }

    // PID file exists — previous instance didn't shut down cleanly
    let old_pid = fs::read_to_string(&pid_path)
        .await
        .unwrap_or_default()
        .trim()
        .to_string();
    warn!(
        "Crash detected: previous instance (PID {}) did not shut down cleanly",
        old_pid
    );

    // Check if that PID is actually still running (zombie/stuck process).
    // Guard against PID reuse: after a reboot the old PID often belongs to an
    // unrelated process (or, before the main.rs ordering fix, to OURSELVES) —
    // only treat it as "previous instance still alive" if it's a live process
    // that is not us and whose cmdline looks like the archipelago binary.
    if !old_pid.is_empty() {
        if let Ok(pid) = old_pid.parse::<u32>() {
            if pid != std::process::id() && is_process_running(pid) && process_is_archipelago(pid) {
                warn!(
                    "Previous process (PID {}) is still running — not a crash, skipping recovery",
                    pid
                );
                // Remove stale PID file and skip recovery
                let _ = fs::remove_file(&pid_path).await;
                return Ok(None);
            }
        }
    }

    // Load the saved container snapshot
    let state_path = data_dir.join(CONTAINER_STATE_FILE);
    let containers = if state_path.exists() {
        match fs::read_to_string(&state_path).await {
            Ok(content) => match serde_json::from_str::<ContainerSnapshot>(&content) {
                Ok(snapshot) => {
                    info!(
                        "Found {} containers from pre-crash snapshot (saved at {})",
                        snapshot.containers.len(),
                        snapshot.timestamp
                    );
                    snapshot.containers
                }
                Err(e) => {
                    warn!("Failed to parse container snapshot: {}", e);
                    Vec::new()
                }
            },
            Err(e) => {
                warn!("Failed to read container snapshot: {}", e);
                Vec::new()
            }
        }
    } else {
        info!("No container snapshot found — cannot determine which containers were running");
        Vec::new()
    };

    // Clean up the stale PID file
    let _ = fs::remove_file(&pid_path).await;

    if containers.is_empty() {
        Ok(None)
    } else {
        Ok(Some(containers))
    }
}

/// Write the PID file to mark the current instance as running.
pub async fn write_pid_marker(data_dir: &Path) -> Result<()> {
    let pid = std::process::id();
    let pid_path = data_dir.join(PID_FILE);
    fs::write(&pid_path, pid.to_string())
        .await
        .context("Failed to write PID marker")?;
    Ok(())
}

/// Remove the PID file on clean shutdown.
pub async fn remove_pid_marker(data_dir: &Path) {
    let pid_path = data_dir.join(PID_FILE);
    if let Err(e) = fs::remove_file(&pid_path).await {
        warn!("Failed to remove PID marker: {}", e);
    }
}

/// Save a snapshot of currently running containers to disk.
/// Called periodically so we know what to restart after a crash.
pub async fn save_container_snapshot(data_dir: &Path) -> Result<()> {
    let mut cmd = tokio::process::Command::new("podman");
    cmd.args(["ps", "--format", "json"]);
    let output = command_with_timeout(cmd, Duration::from_secs(30), "podman ps").await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("podman ps failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let containers: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap_or_default();

    let records: Vec<RunningContainerRecord> = containers
        .iter()
        .filter_map(|c| {
            let name = c.get("Names").and_then(|v| {
                // Podman returns Names as an array
                if let Some(arr) = v.as_array() {
                    arr.first().and_then(|n| n.as_str()).map(|s| s.to_string())
                } else {
                    v.as_str().map(|s| s.to_string())
                }
            })?;
            let image = c
                .get("Image")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            Some(RunningContainerRecord { name, image })
        })
        .collect();

    let snapshot = ContainerSnapshot {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        containers: records,
    };

    let state_path = data_dir.join(CONTAINER_STATE_FILE);
    let json = serde_json::to_string_pretty(&snapshot)
        .context("Failed to serialize container snapshot")?;
    fs::write(&state_path, json)
        .await
        .context("Failed to write container snapshot")?;

    Ok(())
}

// Every automatic mutation shares lifecycle admission with managed updates.
// A stack is indivisible here: alias repair can affect a held sibling even if
// the particular stopped member has no saved unit of its own.
async fn automatic_recovery_intent_allowed(data_dir: &Path, names: &[&str]) -> bool {
    if names
        .iter()
        .any(|name| !crate::health_monitor::automatic_recovery_allowed(data_dir, name))
    {
        return false;
    }
    for file in [USER_STOPPED_FILE, USER_UNINSTALLED_FILE] {
        let values: std::collections::HashSet<String> = match fs::read(data_dir.join(file)).await {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(values) => values,
                Err(_) => return false,
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Default::default(),
            Err(_) => return false,
        };
        if names.iter().any(|name| values.contains(*name)) {
            return false;
        }
    }
    true
}

async fn automatic_recovery_admission(
    data_dir: &Path,
    names: &[&str],
) -> Option<crate::container::update_transaction::Guard> {
    let guard = crate::container::update_transaction::Guard::acquire(data_dir).ok()?;
    automatic_recovery_intent_allowed(data_dir, names)
        .await
        .then_some(guard)
}

/// Recover containers that were running before a crash.
/// Attempts to start each container, logging success/failure.
pub async fn recover_containers(
    data_dir: &Path,
    containers: &[RunningContainerRecord],
) -> RecoveryReport {
    // Snapshot entries can outlive their containers (removed while we were
    // down, or podman storage partially reset by an unclean poweroff).
    // `podman start` on those fails permanently, and recovery runs BEFORE the
    // server binds its port and notifies systemd ready — burning retries on
    // them pushed recovery past TimeoutStartSec and brick-looped the node
    // (killed mid-recovery → next boot sees a crash again, forever).
    let containers: Vec<&RunningContainerRecord> = match existing_container_names().await {
        Some(existing) => {
            let (present, missing): (Vec<_>, Vec<_>) =
                containers.iter().partition(|r| existing.contains(&r.name));
            if !missing.is_empty() {
                warn!(
                    "Skipping {} snapshot container(s) that no longer exist: {:?}",
                    missing.len(),
                    missing.iter().map(|r| r.name.as_str()).collect::<Vec<_>>()
                );
            }
            present
        }
        None => containers.iter().collect(),
    };

    let mut report = RecoveryReport {
        total: containers.len(),
        recovered: 0,
        failed: Vec::new(),
    };

    pending_boot_starts_add(containers.iter().map(|r| r.name.clone()));

    for (i, record) in containers.iter().enumerate() {
        let scope = stack_recovery_specs()
            .iter()
            .find(|stack| stack.containers.contains(&record.name.as_str()))
            .map(|stack| stack.containers.to_vec())
            .unwrap_or_else(|| vec![record.name.as_str()]);
        let Some(_admission) = automatic_recovery_admission(data_dir, &scope).await else {
            pending_boot_start_done(&record.name);
            continue;
        };
        // Skip containers that are already up — `podman start` on a running
        // container produces the noisy benign conmon "Failed to create
        // container" + cgroup Permission-denied journal pair (fleet log
        // sweep 2026-07-22).
        if container_state(&record.name).await.as_deref() == Some("running") {
            report.recovered += 1;
            pending_boot_start_done(&record.name);
            continue;
        }
        info!(
            "Recovering container: {} (image: {})",
            record.name, record.image
        );

        // Recovery counts against systemd's start timeout; a heavy node
        // legitimately needs several minutes for dozens of containers. Push
        // the deadline out ahead of each container so systemd only kills us
        // if we stop making progress (360s covers one full attempt chain).
        let _ = sd_notify::notify(
            false,
            &[sd_notify::NotifyState::ExtendTimeoutUsec(360_000_000)],
        );

        // Rate-limit container starts to avoid overwhelming podman on low-resource systems
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }

        // Try up to 2 attempts with increasing timeout (120s first, 180s retry)
        let mut started = false;
        for attempt in 0..2u32 {
            let timeout_secs = if attempt == 0 { 120 } else { 180 };
            if attempt > 0 {
                info!(
                    "Retrying container {} (attempt {})",
                    record.name,
                    attempt + 1
                );
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
            if !automatic_recovery_intent_allowed(data_dir, &scope).await {
                break;
            }
            let result =
                podman_output(&["start", &record.name], Duration::from_secs(timeout_secs)).await;

            match result {
                Ok(output) if output.status.success() => {
                    info!("Successfully restarted container: {}", record.name);
                    report.recovered += 1;
                    started = true;
                    break;
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    warn!(
                        "Failed to restart container {} (attempt {}): {}",
                        record.name,
                        attempt + 1,
                        stderr.trim()
                    );
                    // The container is gone (raced past the pre-filter, or the
                    // filter query failed) — retrying can never succeed.
                    if stderr.contains("no such container") {
                        break;
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to start container {} ({}s, attempt {}): {}",
                        record.name,
                        timeout_secs,
                        attempt + 1,
                        e
                    );
                }
            }
        }
        if !started {
            report.failed.push(record.name.clone());
        }
        pending_boot_start_done(&record.name);
    }

    report
}

/// All container names podman knows about (running or not). `None` if the
/// query fails — callers fail open and attempt every snapshot entry.
async fn existing_container_names() -> Option<std::collections::HashSet<String>> {
    let output = podman_output(
        &["ps", "-a", "--format", "{{.Names}}"],
        Duration::from_secs(30),
    )
    .await
    .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .map(|s| s.to_string())
            .collect(),
    )
}

#[derive(Debug)]
pub struct RecoveryReport {
    pub total: usize,
    pub recovered: usize,
    pub failed: Vec<String>,
}

/// Check if a process with the given PID is still running.
fn is_process_running(pid: u32) -> bool {
    // Check /proc/{pid} on Linux
    std::path::Path::new(&format!("/proc/{}", pid)).exists()
}

/// Whether the process at `pid` looks like an archipelago instance. Used to
/// tell "the previous instance is genuinely still alive" apart from PID
/// reuse by an unrelated process after a reboot.
fn process_is_archipelago(pid: u32) -> bool {
    match std::fs::read(format!("/proc/{pid}/cmdline")) {
        Ok(cmdline) => String::from_utf8_lossy(&cmdline).contains("archipelago"),
        Err(_) => false,
    }
}

/// Start all stopped containers that were previously installed.
/// Runs on every startup to ensure containers come back after clean reboots.
/// The crash recovery (PID-based) handles dirty shutdowns; this handles clean ones.
/// Skips containers that the user intentionally stopped via the UI.
pub async fn start_stopped_containers(data_dir: &Path) -> RecoveryReport {
    start_stopped_containers_for(data_dir, false).await
}

/// Start stopped multi-container stack members after the backend is already
/// ready. These can take minutes after a reboot, so they must not block
/// systemd readiness.
pub async fn start_stopped_stack_containers(data_dir: &Path) -> RecoveryReport {
    start_stopped_app_stacks(data_dir).await
}

fn stack_member_needs_recovery(state: Option<&str>, user_stopped: bool) -> bool {
    !user_stopped && matches!(state, Some("exited" | "stopped" | "created" | "configured"))
}

async fn start_stopped_app_stacks(data_dir: &Path) -> RecoveryReport {
    let user_stopped = load_user_stopped(data_dir).await;
    let mut report = RecoveryReport {
        total: 0,
        recovered: 0,
        failed: Vec::new(),
    };

    for stack in stack_recovery_specs() {
        let Some(_admission) = automatic_recovery_admission(data_dir, stack.containers).await
        else {
            for name in stack.containers {
                pending_boot_start_done(name);
            }
            continue;
        };
        if !stack_anchor_container_exists(stack).await {
            continue;
        }

        // Healthy members must never acquire a restarting overlay merely
        // because the periodic recovery scan ran. Queue existing stopped
        // members only; recheck each immediately before starting below.
        let mut pending = Vec::new();
        for container in stack.containers {
            let state = container_state(container).await;
            if stack_member_needs_recovery(state.as_deref(), user_stopped.contains(*container)) {
                pending.push((*container).to_string());
            }
        }
        if pending.is_empty() {
            continue;
        }
        if !automatic_recovery_intent_allowed(data_dir, stack.containers).await {
            continue;
        }
        info!("Recovering stopped {} stack containers", stack.name);
        repair_stack_network_aliases(stack).await;
        pending_boot_starts_add(pending.iter().cloned());

        for container in stack.containers {
            if !pending.iter().any(|name| name.as_str() == *container) {
                continue;
            }
            if user_stopped.contains(*container) {
                info!("Skipping user-stopped container: {}", container);
                continue;
            }

            let state = container_state(container).await;
            match state {
                Some(state) if state == "running" => {
                    pending_boot_start_done(container);
                    continue;
                }
                Some(state) if stack_member_needs_recovery(Some(&state), false) => {}
                _ => {
                    pending_boot_start_done(container);
                    continue;
                }
            }

            if !automatic_recovery_intent_allowed(data_dir, stack.containers).await {
                for name in &pending {
                    pending_boot_start_done(name);
                }
                break;
            }
            repair_stack_network_aliases(stack).await;
            wait_before_stack_container_recovery(data_dir, stack, container).await;

            if !automatic_recovery_intent_allowed(data_dir, stack.containers).await {
                for name in &pending {
                    pending_boot_start_done(name);
                }
                break;
            }
            report.total += 1;
            if start_existing_container(container).await {
                report.recovered += 1;
            } else {
                report.failed.push((*container).to_string());
            }
            pending_boot_start_done(container);
        }
    }

    report
}

async fn wait_before_stack_container_recovery(
    data_dir: &Path,
    stack: &StackRecoverySpec,
    container: &str,
) {
    if stack.name != "indeedhub" || container != "indeedhub" {
        return;
    }

    for _ in 0..60 {
        if indeedhub_recovery_dependencies_running().await {
            if !automatic_recovery_intent_allowed(data_dir, stack.containers).await {
                return;
            }
            repair_stack_network_aliases(stack).await;
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    for _ in 0..60 {
        let ready = podman_output(
            &["exec", "indeedhub-api", "getent", "hosts", "minio"],
            Duration::from_secs(5),
        )
        .await
        .map(|output| output.status.success())
        .unwrap_or(false);
        if ready {
            return;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn indeedhub_recovery_dependencies_running() -> bool {
    for name in ["indeedhub-redis", "indeedhub-minio", "indeedhub-api"] {
        if container_state(name).await.as_deref() != Some("running") {
            return false;
        }
    }
    true
}

async fn start_stopped_containers_for(
    data_dir: &Path,
    include_stack_members: bool,
) -> RecoveryReport {
    let mut cmd = tokio::process::Command::new("podman");
    cmd.args([
        "ps",
        "-a",
        "--filter",
        "status=exited",
        "--filter",
        "status=created",
        "--format",
        "{{.Names}}",
    ]);
    let output = match command_with_timeout(cmd, Duration::from_secs(60), "podman ps stopped").await
    {
        Ok(output) => output,
        Err(e) => {
            warn!("Failed listing stopped containers: {}", e);
            return RecoveryReport {
                total: 0,
                recovered: 0,
                failed: Vec::new(),
            };
        }
    };

    let all_names: Vec<String> = if output.status.success() {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(|s| s.to_string())
            .collect()
    } else {
        Vec::new()
    };

    if all_names.is_empty() {
        return RecoveryReport {
            total: 0,
            recovered: 0,
            failed: Vec::new(),
        };
    }

    // Filter out user-stopped containers
    let user_stopped = load_user_stopped(data_dir).await;
    let names: Vec<String> = all_names
        .into_iter()
        .filter(|n| {
            if user_stopped.contains(n) {
                info!("Skipping user-stopped container: {}", n);
                false
            } else {
                true
            }
        })
        .collect();

    if names.is_empty() {
        return RecoveryReport {
            total: 0,
            recovered: 0,
            failed: Vec::new(),
        };
    }

    let names: Vec<String> = names
        .into_iter()
        .filter(|n| should_auto_start_stopped_container(n, include_stack_members))
        .collect();

    if names.is_empty() {
        return RecoveryReport {
            total: 0,
            recovered: 0,
            failed: Vec::new(),
        };
    }

    // Sort by startup tier: databases first, then core, then dependent services, then apps
    let mut records: Vec<RunningContainerRecord> = names
        .iter()
        .map(|n| RunningContainerRecord {
            name: n.clone(),
            image: String::new(),
        })
        .collect();
    records.sort_by_key(|r| container_boot_tier(&r.name));

    info!(
        "Starting {} stopped containers after boot (skipped {} user-stopped)...",
        records.len(),
        user_stopped.len()
    );
    recover_containers(data_dir, &records).await
}

fn should_auto_start_stopped_container(name: &str, include_stack_members: bool) -> bool {
    // Keep generic boot recovery narrow. The Rust manifest reconciler owns
    // managed app stacks; starting every exited Podman container here races
    // it and resurrects legacy/orphan helper containers.
    if matches!(name, "filebrowser" | "nostr-rs-relay") {
        return true;
    }
    include_stack_members
        && matches!(
            name,
            "immich_postgres"
                | "immich_redis"
                | "immich_server"
                | "indeedhub-postgres"
                | "indeedhub-redis"
                | "indeedhub-minio"
                | "indeedhub-relay"
                | "indeedhub-api"
                | "indeedhub-ffmpeg"
                | "indeedhub"
                | "netbird-server"
                | "netbird-dashboard"
                | "netbird"
                | "pine-whisper"
                | "pine-piper"
                | "pine-openwakeword"
                | "pine"
        )
}

struct StackRecoverySpec {
    name: &'static str,
    network: &'static str,
    aliases: &'static [(&'static str, &'static str)],
    containers: &'static [&'static str],
    /// The stack's core dependency (its DB / server container) — every other
    /// member depends on this being present. Used to distinguish "a genuinely
    /// installed stack has a crashed member" from "orphan debris from a
    /// partial/failed install" (see `stack_anchor_container_exists`).
    anchor: &'static str,
}

fn stack_recovery_specs() -> &'static [StackRecoverySpec] {
    &[
        StackRecoverySpec {
            name: "immich",
            network: "immich-net",
            aliases: &[
                ("immich_postgres", "immich_postgres"),
                ("immich_redis", "immich_redis"),
                ("immich_server", "immich_server"),
            ],
            containers: &["immich_postgres", "immich_redis", "immich_server"],
            anchor: "immich_postgres",
        },
        StackRecoverySpec {
            name: "indeedhub",
            network: "indeedhub-net",
            aliases: &[
                ("indeedhub-postgres", "postgres"),
                ("indeedhub-redis", "redis"),
                ("indeedhub-minio", "minio"),
                ("indeedhub-relay", "relay"),
                ("indeedhub-api", "api"),
                ("indeedhub", "indeedhub"),
            ],
            containers: &[
                "indeedhub-postgres",
                "indeedhub-redis",
                "indeedhub-minio",
                "indeedhub-relay",
                "indeedhub-api",
                "indeedhub-ffmpeg",
                "indeedhub",
            ],
            anchor: "indeedhub-postgres",
        },
        StackRecoverySpec {
            name: "netbird",
            network: "netbird-net",
            aliases: &[
                ("netbird-server", "netbird-server"),
                ("netbird-dashboard", "netbird-dashboard"),
                ("netbird", "netbird"),
            ],
            containers: &["netbird-server", "netbird-dashboard", "netbird"],
            anchor: "netbird-server",
        },
        StackRecoverySpec {
            name: "pine",
            network: "archy-net",
            aliases: &[
                ("pine-whisper", "pine-whisper"),
                ("pine-piper", "pine-piper"),
                ("pine-openwakeword", "pine-openwakeword"),
                ("pine", "pine"),
            ],
            containers: &["pine-whisper", "pine-piper", "pine-openwakeword", "pine"],
            // The launcher only depends on the two engines; whisper is the
            // heaviest/first member, so treat it as the stack anchor (its
            // presence means the stack was really installed, not orphan debris).
            anchor: "pine-whisper",
        },
    ]
}

/// Whether the stack's core dependency container exists at all (running or
/// not — existence, not health, is what matters here). `false` means any
/// other stack member still lying around is orphan debris from a partial or
/// already-uninstalled install, not a legitimately-installed-but-crashed
/// stack — blindly restarting those siblings just crash-loops them forever
/// against a dependency that was never created (indeedhub-api on `.116`,
/// 2026-07-01: retried every 120s against a nonexistent indeedhub-postgres).
async fn stack_anchor_container_exists(stack: &StackRecoverySpec) -> bool {
    container_state(stack.anchor).await.is_some()
}

async fn repair_stack_network_aliases(stack: &StackRecoverySpec) {
    let _ = podman_status(
        &["network", "create", stack.network],
        Duration::from_secs(15),
    )
    .await;

    for (container, alias) in stack.aliases {
        if container_state(container).await.is_none() {
            continue;
        }
        if network_alias_present(stack.network, container, alias).await {
            continue;
        }

        let _ = podman_status(
            &["network", "disconnect", "-f", stack.network, container],
            Duration::from_secs(15),
        )
        .await;
        let _ = podman_status(
            &[
                "network",
                "connect",
                "--alias",
                alias,
                stack.network,
                container,
            ],
            Duration::from_secs(15),
        )
        .await;
    }
}

async fn network_alias_present(network_name: &str, container: &str, alias: &str) -> bool {
    let output = match podman_output(
        &[
            "inspect",
            container,
            "--format",
            "{{json .NetworkSettings.Networks}}",
        ],
        Duration::from_secs(10),
    )
    .await
    {
        Ok(output) if output.status.success() => output,
        _ => return false,
    };

    let Ok(networks) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return false;
    };
    networks
        .get(network_name)
        .and_then(|network| network.get("Aliases"))
        .and_then(|aliases| aliases.as_array())
        .map(|aliases| aliases.iter().any(|value| value.as_str() == Some(alias)))
        .unwrap_or(false)
}

async fn container_state(container: &str) -> Option<String> {
    let output = podman_output(
        &["inspect", container, "--format", "{{.State.Status}}"],
        Duration::from_secs(10),
    )
    .await
    .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn start_existing_container(container: &str) -> bool {
    let timeout = match container {
        "immich_server" | "netbird-server" => Duration::from_secs(120),
        _ => Duration::from_secs(90),
    };
    match container_state(container).await.as_deref() {
        // Already up — `podman start` on a running container makes conmon
        // try to join the live cgroup and fail with a scary "Failed to
        // create container: exit status 1" + cgroup.procs Permission denied
        // pair in the journal. Fleet log sweep 2026-07-22 found hundreds of
        // these per day per node, all benign. Skip instead.
        Some("running") => return true,
        Some("initialized") => cleanup_container_runtime_state(container).await,
        _ => {}
    }
    info!("Recovering stack container: {}", container);
    match podman_output(&["start", container], timeout).await {
        Ok(output) if output.status.success() => {
            tokio::time::sleep(Duration::from_secs(3)).await;
            if container_state(container).await.as_deref() == Some("exited") {
                warn!("Stack container {} exited shortly after start", container);
                false
            } else {
                info!("Successfully recovered stack container: {}", container);
                true
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.contains("exec.fifo") || stderr.contains("failed to start container") {
                cleanup_container_runtime_state(container).await;
                if let Ok(retry) = podman_output(&["start", container], timeout).await {
                    if retry.status.success() {
                        info!(
                            "Successfully recovered stack container after cleanup: {}",
                            container
                        );
                        return true;
                    }
                    warn!(
                        "Failed to recover stack container {} after cleanup: {}",
                        container,
                        String::from_utf8_lossy(&retry.stderr).trim()
                    );
                    return false;
                }
            }
            warn!(
                "Failed to recover stack container {}: {}",
                container, stderr
            );
            false
        }
        Err(e) => {
            warn!("Failed to recover stack container {}: {}", container, e);
            false
        }
    }
}

async fn cleanup_container_runtime_state(container: &str) {
    let _ = podman_output(
        &["container", "cleanup", container],
        Duration::from_secs(30),
    )
    .await;
}

async fn podman_status(args: &[&str], timeout: Duration) -> Option<std::process::ExitStatus> {
    podman_output(args, timeout)
        .await
        .ok()
        .map(|output| output.status)
}

#[cfg(test)]
thread_local! {
    static RECOVERY_PODMAN_TEST: std::cell::RefCell<Option<Box<dyn FnMut(&[&str]) -> Output>>> = std::cell::RefCell::new(None);
}

async fn podman_output(args: &[&str], timeout: Duration) -> Result<Output> {
    #[cfg(test)]
    if let Some(output) =
        RECOVERY_PODMAN_TEST.with(|hook| hook.borrow_mut().as_mut().map(|f| f(args)))
    {
        return Ok(output);
    }
    let mut cmd = tokio::process::Command::new("podman");
    cmd.args(args);
    command_with_timeout(cmd, timeout, &format!("podman {}", args.join(" "))).await
}

/// Simple tier ordering for boot recovery (mirrors health_monitor tiers).
fn container_boot_tier(name: &str) -> u8 {
    let id = name.strip_prefix("archy-").unwrap_or(name);
    match id {
        // Tier 0: Databases and data stores
        "btcpay-db" | "mempool-db" | "mysql-mempool" | "penpot-postgres" | "immich_postgres"
        | "immich_redis" | "penpot-valkey" | "endurain-db" | "nextcloud-db"
        | "indeedhub-postgres" | "indeedhub-redis" | "indeedhub-minio" => 0,
        // Tier 1: Core infrastructure
        "bitcoin-knots" | "bitcoin-core" | "bitcoin" => 1,
        // Tier 2: Dependent services
        "lnd" | "electrumx" | "mempool-electrs" | "electrs" | "nbxplorer" | "mempool-api"
        | "indeedhub-api" => 2,
        // Tier 4: Frontend/UI
        "mempool-web" | "bitcoin-ui" | "lnd-ui" | "electrs-ui" | "penpot-frontend"
        | "penpot-exporter" | "indeedhub" => 4,
        // Tier 3: Everything else
        _ => 3,
    }
}

/// Run the reconciliation script after boot to fix any config drift.
/// Ensures all containers match their canonical specs from container-specs.sh.
#[allow(dead_code)]
pub async fn run_boot_reconciliation() {
    let script = "/home/archipelago/archy/scripts/reconcile-containers.sh";
    if !std::path::Path::new(script).exists() {
        info!("Reconciliation script not found (dev mode?) — skipping boot reconciliation");
        return;
    }
    info!("Running boot reconciliation...");
    let cmd = tokio::process::Command::new(script);
    let result = command_with_timeout(cmd, Duration::from_secs(300), script).await;
    match result {
        Ok(output) if output.status.success() => {
            info!("Boot reconciliation complete");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                "Boot reconciliation had failures: {}",
                stderr.chars().take(500).collect::<String>()
            );
        }
        Err(e) => warn!("Boot reconciliation failed: {}", e),
    }
}

async fn command_with_timeout(
    mut cmd: tokio::process::Command,
    timeout: Duration,
    description: &str,
) -> Result<Output> {
    cmd.kill_on_drop(true);
    tokio::time::timeout(timeout, cmd.output())
        .await
        .with_context(|| format!("{} timed out after {}s", description, timeout.as_secs()))?
        .with_context(|| format!("Failed to run {}", description))
}

/// Spawn a background task that periodically saves the container snapshot.
pub fn spawn_snapshot_task(data_dir: PathBuf) {
    tokio::spawn(async move {
        // Wait 2 minutes before first snapshot (let crash recovery finish and containers stabilize)
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(120));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(e) = save_container_snapshot(&data_dir).await {
                tracing::debug!("Container snapshot (non-fatal): {}", e);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct RecoveryPodmanFixture;
    impl Drop for RecoveryPodmanFixture {
        fn drop(&mut self) {
            RECOVERY_PODMAN_TEST.with(|hook| *hook.borrow_mut() = None);
        }
    }
    fn recovery_podman_fixture(
        root: &Path,
    ) -> (
        RecoveryPodmanFixture,
        std::rc::Rc<std::cell::RefCell<Vec<Vec<String>>>>,
    ) {
        use std::os::unix::process::ExitStatusExt;
        let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = calls.clone();
        let data = root.to_path_buf();
        let mut started = false;
        RECOVERY_PODMAN_TEST.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |args| {
                captured
                    .borrow_mut()
                    .push(args.iter().map(|v| v.to_string()).collect());
                let mut status = 0;
                let mut stdout = String::new();
                match args.first().copied() {
                    Some("inspect") if args.get(1).is_some_and(|v| v.starts_with("immich_")) => {
                        if args.last() == Some(&"{{json .NetworkSettings.Networks}}") {
                            stdout = "{}".into();
                        } else {
                            stdout = if args[1] == "immich_redis" && !started {
                                "exited"
                            } else {
                                "running"
                            }
                            .into();
                        }
                    }
                    Some("inspect") => status = 1,
                    Some("ps") => stdout = "immich_redis\n".into(),
                    Some("start") | Some("network") => {
                        assert!(
                            crate::container::update_transaction::Guard::acquire(&data).is_err(),
                            "mutation escaped lifecycle guard"
                        );
                        if args[0] == "start" {
                            started = true;
                        }
                    }
                    _ => panic!("unexpected recovery command: {args:?}"),
                }
                Output {
                    status: std::process::ExitStatus::from_raw(status << 8),
                    stdout: stdout.into_bytes(),
                    stderr: vec![],
                }
            }))
        });
        (RecoveryPodmanFixture, calls)
    }

    #[tokio::test]
    async fn stack_recovery_never_mutates_saved_held_or_operator_disabled_siblings() {
        for reason in [
            "saved",
            "broken-saved",
            "held",
            "broken-hold",
            "stopped",
            "uninstalled",
            "broken-intent",
            "locked",
        ] {
            let root = TempDir::new().unwrap();
            let transaction = root.path().join("update-transactions");
            let mut lock = None;
            match reason {
                "saved" | "broken-saved" => {
                    let dir = transaction.join("installed-units");
                    std::fs::create_dir_all(&dir).unwrap();
                    let body = if reason == "saved" {
                        serde_json::json!({"schema":1,"operation":uuid::Uuid::new_v4().to_string(),"name":"immich_server","body":"[Container]\nImage=retained\n","mode":384}).to_string()
                    } else {
                        "broken".into()
                    };
                    std::fs::write(dir.join("immich_server.json"), body).unwrap();
                }
                "held" | "broken-hold" => {
                    let dir = transaction.join("holds");
                    std::fs::create_dir_all(&dir).unwrap();
                    std::fs::write(
                        dir.join("immich_server"),
                        if reason == "held" {
                            uuid::Uuid::new_v4().to_string()
                        } else {
                            "broken".into()
                        },
                    )
                    .unwrap();
                }
                "locked" => {
                    lock = Some(
                        crate::container::update_transaction::Guard::acquire(root.path()).unwrap(),
                    )
                }
                "stopped" => {
                    std::fs::write(root.path().join(USER_STOPPED_FILE), r#"["immich_server"]"#)
                        .unwrap()
                }
                "uninstalled" => std::fs::write(
                    root.path().join(USER_UNINSTALLED_FILE),
                    r#"["immich_server"]"#,
                )
                .unwrap(),
                _ => std::fs::write(root.path().join(USER_UNINSTALLED_FILE), "broken").unwrap(),
            }
            let (_fixture, calls) = recovery_podman_fixture(root.path());
            let report = start_stopped_app_stacks(root.path()).await;
            assert_eq!(report.recovered, 0, "{reason}");
            assert!(
                !calls
                    .borrow()
                    .iter()
                    .any(|args| matches!(args[0].as_str(), "network" | "start")),
                "{reason}"
            );
            let snapshot = recover_containers(
                root.path(),
                &[RunningContainerRecord {
                    name: "immich_redis".into(),
                    image: String::new(),
                }],
            )
            .await;
            assert_eq!(
                snapshot.recovered, 0,
                "snapshot sibling admission: {reason}"
            );
            assert!(
                !calls.borrow().iter().any(|args| args[0] == "start"),
                "snapshot {reason}"
            );
            drop(lock);
        }
    }

    #[tokio::test]
    async fn stack_and_snapshot_recovery_hold_lifecycle_guard_for_real_mutation_path() {
        let root = TempDir::new().unwrap();
        let (_fixture, calls) = recovery_podman_fixture(root.path());
        let report = start_stopped_app_stacks(root.path()).await;
        assert_eq!(report.recovered, 1);
        assert!(calls
            .borrow()
            .iter()
            .any(|args| args == &["start", "immich_redis"]));
        assert!(crate::container::update_transaction::Guard::acquire(root.path()).is_ok());
        let (_positive_snapshot_fixture, positive_calls) = recovery_podman_fixture(root.path());
        let positive = recover_containers(
            root.path(),
            &[RunningContainerRecord {
                name: "immich_redis".into(),
                image: String::new(),
            }],
        )
        .await;
        assert_eq!(positive.recovered, 1);
        assert!(positive_calls
            .borrow()
            .iter()
            .any(|args| args == &["start", "immich_redis"]));
        let (_snapshot_fixture, snapshot_calls) = recovery_podman_fixture(root.path());
        std::fs::write(root.path().join(USER_STOPPED_FILE), r#"["immich_redis"]"#).unwrap();
        let report = recover_containers(
            root.path(),
            &[RunningContainerRecord {
                name: "immich_redis".into(),
                image: String::new(),
            }],
        )
        .await;
        assert_eq!(report.recovered, 0);
        assert!(!snapshot_calls
            .borrow()
            .iter()
            .any(|args| args[0] == "start"));
    }

    #[tokio::test]
    async fn if_recorded_distinguishes_no_record_from_empty_record() {
        let tmp = TempDir::new().unwrap();
        // No file: the reaper must see "could not look", never "empty".
        assert!(load_installed_apps_if_recorded(tmp.path()).await.is_none());
        // Corrupt file: same — refuse to answer rather than guess.
        tokio::fs::write(tmp.path().join(INSTALLED_APPS_FILE), "{ not json")
            .await
            .unwrap();
        assert!(load_installed_apps_if_recorded(tmp.path()).await.is_none());
        // A real (even empty) record answers.
        tokio::fs::write(tmp.path().join(INSTALLED_APPS_FILE), "[]")
            .await
            .unwrap();
        assert_eq!(
            load_installed_apps_if_recorded(tmp.path()).await,
            Some(std::collections::HashSet::new())
        );
        mark_installed(tmp.path(), "bitcoin-knots").await;
        assert!(load_installed_apps_if_recorded(tmp.path())
            .await
            .unwrap()
            .contains("bitcoin-knots"));
    }

    #[tokio::test]
    async fn installed_record_survives_and_forgets_on_uninstall() {
        let tmp = TempDir::new().unwrap();
        assert!(load_installed_apps(tmp.path()).await.is_empty());

        mark_installed(tmp.path(), "bitcoin-knots").await;
        mark_installed(tmp.path(), "lnd").await;
        assert!(load_installed_apps(tmp.path())
            .await
            .contains("bitcoin-knots"));

        // Uninstall forgets it, or desired-state recovery would recreate the
        // very app that was just removed.
        clear_installed(tmp.path(), "bitcoin-knots").await;
        let after = load_installed_apps(tmp.path()).await;
        assert!(!after.contains("bitcoin-knots"));
        assert!(after.contains("lnd"), "clearing one must not drop the rest");
    }

    #[tokio::test]
    async fn the_record_does_not_erode_the_way_the_running_snapshot_does() {
        // The whole point. running-containers.json answers "what is running
        // NOW", so an app that stays down long enough ages out of it and can
        // never be recovered — bitcoin-knots on archi-dev-box, 2026-08-08.
        // Installation is a decision, so no amount of downtime may erase it.
        let tmp = TempDir::new().unwrap();
        mark_installed(tmp.path(), "bitcoin-knots").await;

        // Nothing is running at all, and a snapshot is taken saying so.
        save_container_snapshot_for_test(tmp.path(), &[]).await;

        assert!(load_last_running_names(tmp.path()).await.is_empty());
        assert!(
            load_installed_apps(tmp.path())
                .await
                .contains("bitcoin-knots"),
            "installation record must outlive the running snapshot"
        );
    }

    #[tokio::test]
    async fn backfill_claims_only_what_exists_and_never_what_was_uninstalled() {
        let tmp = TempDir::new().unwrap();

        // The operator uninstalled filebrowser; its container may still be up,
        // because a Quadlet unit is owned by systemd. Backfilling from live
        // containers must not re-claim it.
        mark_user_uninstalled(tmp.path(), "filebrowser").await;

        backfill_installed_apps(
            tmp.path(),
            &[
                "bitcoin-knots".to_string(),
                "electrumx".to_string(),
                "filebrowser".to_string(),
            ],
        )
        .await;

        let installed = load_installed_apps(tmp.path()).await;
        assert!(installed.contains("bitcoin-knots"));
        assert!(installed.contains("electrumx"));
        assert!(
            !installed.contains("filebrowser"),
            "backfill re-claimed an app the operator uninstalled"
        );
    }

    #[tokio::test]
    async fn backfill_is_additive_and_never_removes() {
        // It runs on every boot. If it replaced rather than merged, an app
        // that happened to be down at that moment would be dropped from the
        // record — reintroducing the erosion this record exists to prevent.
        let tmp = TempDir::new().unwrap();
        mark_installed(tmp.path(), "lnd").await;

        backfill_installed_apps(tmp.path(), &["bitcoin-knots".to_string()]).await;

        let installed = load_installed_apps(tmp.path()).await;
        assert!(
            installed.contains("lnd"),
            "a down app was dropped by backfill"
        );
        assert!(installed.contains("bitcoin-knots"));

        // An empty adoption list (podman unreachable, say) must change nothing.
        backfill_installed_apps(tmp.path(), &[]).await;
        assert_eq!(load_installed_apps(tmp.path()).await.len(), 2);
    }

    #[tokio::test]
    async fn test_no_crash_without_pid_file() {
        let tmp = TempDir::new().unwrap();
        let result = check_for_crash(tmp.path()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_crash_detected_with_pid_file() {
        let tmp = TempDir::new().unwrap();
        // Write a PID file with a non-existent PID
        fs::write(tmp.path().join(PID_FILE), "999999999")
            .await
            .unwrap();
        let result = check_for_crash(tmp.path()).await.unwrap();
        // No snapshot file → crash detected but no containers to recover
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_crash_with_snapshot() {
        let tmp = TempDir::new().unwrap();
        // Write PID file
        fs::write(tmp.path().join(PID_FILE), "999999999")
            .await
            .unwrap();
        // Write container snapshot
        let snapshot = ContainerSnapshot {
            timestamp: 1000,
            containers: vec![
                RunningContainerRecord {
                    name: "archy-bitcoin-knots".to_string(),
                    image: "bitcoin-knots:27.1".to_string(),
                },
                RunningContainerRecord {
                    name: "archy-mempool-web".to_string(),
                    image: "mempool/frontend:3.0".to_string(),
                },
            ],
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        fs::write(tmp.path().join(CONTAINER_STATE_FILE), json)
            .await
            .unwrap();

        let result = check_for_crash(tmp.path()).await.unwrap();
        assert!(result.is_some());
        let containers = result.unwrap();
        assert_eq!(containers.len(), 2);
        assert_eq!(containers[0].name, "archy-bitcoin-knots");
        assert_eq!(containers[1].name, "archy-mempool-web");
    }

    #[tokio::test]
    async fn test_load_last_running_names_reads_snapshot_without_pid_gate() {
        let tmp = TempDir::new().unwrap();
        // No PID file written — load_last_running_names must NOT require a crash.
        let snapshot = ContainerSnapshot {
            timestamp: 1000,
            containers: vec![
                RunningContainerRecord {
                    name: "immich_server".to_string(),
                    image: "immich:2.7".to_string(),
                },
                RunningContainerRecord {
                    name: "immich_postgres".to_string(),
                    image: "postgres:16".to_string(),
                },
            ],
        };
        fs::write(
            tmp.path().join(CONTAINER_STATE_FILE),
            serde_json::to_string(&snapshot).unwrap(),
        )
        .await
        .unwrap();

        let names = load_last_running_names(tmp.path()).await;
        assert_eq!(names.len(), 2);
        assert!(names.contains("immich_server"));
        assert!(names.contains("immich_postgres"));
        assert!(!names.contains("immich_redis"));
    }

    #[tokio::test]
    async fn test_load_last_running_names_empty_when_absent() {
        let tmp = TempDir::new().unwrap();
        assert!(load_last_running_names(tmp.path()).await.is_empty());
    }

    #[tokio::test]
    async fn test_write_and_remove_pid_marker() {
        let tmp = TempDir::new().unwrap();
        write_pid_marker(tmp.path()).await.unwrap();
        assert!(tmp.path().join(PID_FILE).exists());

        remove_pid_marker(tmp.path()).await;
        assert!(!tmp.path().join(PID_FILE).exists());
    }

    #[tokio::test]
    async fn test_pid_marker_contains_current_pid() {
        let tmp = TempDir::new().unwrap();
        write_pid_marker(tmp.path()).await.unwrap();
        let content = fs::read_to_string(tmp.path().join(PID_FILE)).await.unwrap();
        let saved_pid: u32 = content.parse().unwrap();
        assert_eq!(saved_pid, std::process::id());
    }

    #[tokio::test]
    async fn test_clean_shutdown_no_crash_on_restart() {
        let tmp = TempDir::new().unwrap();

        // Simulate: startup → write PID → clean shutdown → remove PID
        write_pid_marker(tmp.path()).await.unwrap();
        remove_pid_marker(tmp.path()).await;

        // Next startup should detect no crash
        let result = check_for_crash(tmp.path()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_corrupt_snapshot_handled() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(PID_FILE), "999999999")
            .await
            .unwrap();
        fs::write(tmp.path().join(CONTAINER_STATE_FILE), "not valid json")
            .await
            .unwrap();

        // Should not crash, returns None (no recoverable containers)
        let result = check_for_crash(tmp.path()).await.unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn generic_boot_recovery_skips_manifest_owned_and_legacy_stacks() {
        assert!(should_auto_start_stopped_container("filebrowser", false));
        assert!(should_auto_start_stopped_container("nostr-rs-relay", false));
        assert!(!should_auto_start_stopped_container("bitcoin-knots", false));
        assert!(!should_auto_start_stopped_container("lnd", false));
        assert!(!should_auto_start_stopped_container(
            "indeedhub-postgres",
            false
        ));
        assert!(should_auto_start_stopped_container(
            "indeedhub-postgres",
            true
        ));
    }

    #[test]
    fn stack_recovery_anchor_is_the_stacks_own_core_dependency() {
        // Every stack's anchor must be one of its own containers (typically
        // the DB/server the rest depend on) — a typo here would silently
        // disable orphan-debris protection for that stack.
        for stack in stack_recovery_specs() {
            assert!(
                stack.containers.contains(&stack.anchor),
                "{}: anchor {} not among its own containers",
                stack.name,
                stack.anchor
            );
        }
        assert_eq!(
            stack_recovery_specs()
                .iter()
                .find(|s| s.name == "indeedhub")
                .unwrap()
                .anchor,
            "indeedhub-postgres"
        );
    }
}

#[cfg(test)]
mod installed_concurrency_tests {
    use super::*;
    #[tokio::test]
    async fn concurrent_install_records_are_not_lost() {
        let dir = tempfile::tempdir().unwrap();
        let mut tasks = Vec::new();
        for i in 0..24 {
            let path = dir.path().to_owned();
            tasks.push(tokio::spawn(async move {
                mark_installed(&path, &format!("app-{i}")).await;
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(load_installed_apps(dir.path()).await.len(), 24);
        clear_installed(dir.path(), "app-3").await;
        assert_eq!(load_installed_apps(dir.path()).await.len(), 23);
        assert!(!dir.path().join("installed-apps.json.tmp").exists());
    }
}

#[cfg(test)]
mod stack_recovery_overlay_tests {
    use super::stack_member_needs_recovery;
    #[test]
    fn only_existing_stopped_members_receive_recovery_overlay() {
        for state in [
            None,
            Some("running"),
            Some("paused"),
            Some("restarting"),
            Some("removing"),
        ] {
            assert!(!stack_member_needs_recovery(state, false));
        }
        for state in ["exited", "stopped", "created", "configured"] {
            assert!(stack_member_needs_recovery(Some(state), false));
            assert!(!stack_member_needs_recovery(Some(state), true));
        }
    }
}
