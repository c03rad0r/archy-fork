// Container Health Monitor
// Checks container health every 120s, auto-restarts unhealthy containers (max 10 times)
// with exponential backoff (10s..120s), dependency-aware restart ordering (deps first),
// handles "created" state containers, resets dependent counters when deps recover,
// and sends WebSocket notifications to the UI on failure.

use crate::data_model::{Notification, NotificationLevel, PackageState};
use crate::state::StateManager;
use crate::webhooks::{self, WebhookEvent};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

const MAX_RESTART_ATTEMPTS: u32 = 10;
const CHECK_INTERVAL_SECS: u64 = 120;
/// Backoff delays per attempt — escalating from 10s to 120s
const BACKOFF_DELAYS_SECS: [u64; 10] = [10, 15, 20, 30, 30, 45, 60, 60, 90, 120];
/// Reset restart counter after 1 hour of stability
const STABILITY_RESET_SECS: u64 = 3600;

/// Container startup tier for dependency ordering.
/// Lower tiers start first. Containers in the same tier start together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum StartupTier {
    /// Databases: postgres, redis, mariadb, mysql
    Database = 0,
    /// Core infrastructure: bitcoin-knots, bitcoin-core
    CoreInfra = 1,
    /// Services depending on core: lnd, electrs, nbxplorer
    DependentService = 2,
    /// Application layer: mempool-api, btcpay-server, fedimint, nextcloud, etc.
    Application = 3,
    /// UI/frontend containers: mempool-web, bitcoin-ui, lnd-ui
    Frontend = 4,
}

fn container_tier(name: &str) -> StartupTier {
    let id = name.strip_prefix("archy-").unwrap_or(name);
    match id {
        // Tier 0: Databases and data stores
        "btcpay-db" | "mempool-db" | "mysql-mempool" | "penpot-postgres" | "immich_postgres"
        | "immich_redis" | "penpot-valkey" | "endurain-db" | "nextcloud-db"
        | "indeedhub-postgres" | "indeedhub-redis" | "indeedhub-minio" => StartupTier::Database,

        // Tier 1: Core infrastructure
        "bitcoin-knots" | "bitcoin-core" | "bitcoin" => StartupTier::CoreInfra,

        // Tier 2: Dependent services (need databases or bitcoin)
        "lnd" | "electrumx" | "mempool-electrs" | "electrs" | "nbxplorer" | "mempool-api"
        | "indeedhub-api" => StartupTier::DependentService,

        // Tier 4: Frontend/UI
        "mempool-web" | "bitcoin-ui" | "lnd-ui" | "electrs-ui" | "cuprate-ui"
        | "penpot-frontend" | "penpot-exporter" | "indeedhub" => StartupTier::Frontend,

        // Tier 3: Application layer (everything else)
        _ => StartupTier::Application,
    }
}

/// Map containers to their required dependencies.
/// When a dependent fails, check and restart its dependencies first.
fn container_dependencies(name: &str) -> &'static [&'static str] {
    let id = name.strip_prefix("archy-").unwrap_or(name);
    match id {
        // Bitcoin-dependent chain
        "lnd" => &["bitcoin"],
        "electrumx" | "mempool-electrs" | "electrs" => &["bitcoin"],
        "nbxplorer" => &["bitcoin"],
        "btcpay-server" => &["btcpay-db", "nbxplorer"],
        "mempool-api" => &["mempool-db", "electrumx"],
        "mempool-web" => &["mempool-api"],
        "fedimint" => &["bitcoin"],
        "fedimint-gateway" => &["bitcoin", "fedimint"],

        // IndeedHub stack. The API needs MinIO (object storage) up before it
        // can serve — without listing it the health monitor would restart the
        // API while MinIO was still coming up, which is the "needs 1-2 restarts
        // to recover" symptom (#41). MinIO has no deps of its own, so the
        // monitor restarts it independently first; no deadlock.
        "indeedhub-api" => &["indeedhub-postgres", "indeedhub-redis", "indeedhub-minio"],
        "indeedhub" => &["indeedhub-api"],
        "indeedhub-relay" => &["indeedhub-postgres"],
        "indeedhub-ffmpeg" => &["indeedhub-api"],

        // Multi-container stacks
        "immich_server" => &["immich_postgres", "immich_redis"],
        "penpot-backend" => &["penpot-postgres", "penpot-valkey"],
        "penpot-frontend" => &["penpot-backend"],

        // UI containers
        "bitcoin-ui" => &["bitcoin"],
        "lnd-ui" => &["lnd"],
        "electrs-ui" => &["electrumx"],

        _ => &[],
    }
}

/// Check if all of a container's dependencies are currently running.
fn deps_are_running(name: &str, containers: &[ContainerHealth]) -> bool {
    let deps = container_dependencies(name);
    if deps.is_empty() {
        return true;
    }
    for dep in deps {
        if *dep == "bitcoin" {
            let bitcoin_running = containers.iter().any(|c| {
                let c_id = c.name.strip_prefix("archy-").unwrap_or(&c.name);
                matches!(c_id, "bitcoin" | "bitcoin-knots" | "bitcoin-core") && c.state == "running"
            });
            if !bitcoin_running {
                return false;
            }
            continue;
        }
        // Check both plain name and archy- prefixed name
        let dep_running = containers.iter().any(|c| {
            let c_id = c.name.strip_prefix("archy-").unwrap_or(&c.name);
            (c_id == *dep || c.name == *dep) && c.state == "running"
        });
        if !dep_running {
            return false;
        }
    }
    true
}

fn conflicting_bitcoin_variant(name: &str) -> Option<&'static str> {
    match name.strip_prefix("archy-").unwrap_or(name) {
        "bitcoin-core" => Some("bitcoin-knots"),
        "bitcoin-knots" | "bitcoin" => Some("bitcoin-core"),
        _ => None,
    }
}

fn has_running_bitcoin_conflict(name: &str, containers: &[ContainerHealth]) -> bool {
    let Some(conflict) = conflicting_bitcoin_variant(name) else {
        return false;
    };
    containers.iter().any(|c| {
        let id = c.name.strip_prefix("archy-").unwrap_or(&c.name);
        id == conflict && c.state == "running"
    })
}

/// True when a lifecycle op lock covering this container is held. Probes the
/// container name, the derived app id, and a '_'→'-' normalization of the
/// name — legacy stack containers are named with underscores
/// (immich_postgres) while op-lock keys and the stack-member table use the
/// hyphenated app ids.
fn lifecycle_op_covers_container(container_name: &str, app_id: &str) -> bool {
    let hyphenated = container_name.replace('_', "-");
    let mut keys = vec![container_name, app_id];
    if hyphenated != container_name {
        keys.push(hyphenated.as_str());
    }
    keys.iter()
        .any(|key| crate::app_ops::lifecycle_op_in_flight(key))
}

/// Track restart attempts per container with exponential backoff and stability reset.
struct RestartTracker {
    attempts: HashMap<String, u32>,
    last_failure: HashMap<String, Instant>,
    last_healthy: HashMap<String, Instant>,
}

impl RestartTracker {
    fn new() -> Self {
        Self {
            attempts: HashMap::new(),
            last_failure: HashMap::new(),
            last_healthy: HashMap::new(),
        }
    }

    /// Record a restart attempt. Returns false if max attempts exceeded.
    fn record_attempt(&mut self, name: &str) -> bool {
        let count = self.attempts.entry(name.to_string()).or_insert(0);
        *count += 1;
        self.last_failure.insert(name.to_string(), Instant::now());
        *count <= MAX_RESTART_ATTEMPTS
    }

    /// Clear restart count when a container is healthy again.
    fn clear(&mut self, name: &str) {
        self.attempts.remove(name);
        self.last_failure.remove(name);
        self.last_healthy.insert(name.to_string(), Instant::now());
    }

    fn attempt_count(&self, name: &str) -> u32 {
        *self.attempts.get(name).unwrap_or(&0)
    }

    /// Get the backoff delay in seconds for the current attempt number.
    fn backoff_delay_secs(&self, name: &str) -> u64 {
        let attempts = self.attempt_count(name);
        if attempts == 0 {
            return BACKOFF_DELAYS_SECS[0];
        }
        let idx = (attempts as usize)
            .saturating_sub(1)
            .min(BACKOFF_DELAYS_SECS.len() - 1);
        BACKOFF_DELAYS_SECS[idx]
    }

    /// Check if enough time has passed since last failure for the backoff delay.
    fn backoff_elapsed(&self, name: &str) -> bool {
        let delay = self.backoff_delay_secs(name);
        match self.last_failure.get(name) {
            Some(last) => last.elapsed().as_secs() >= delay,
            None => true,
        }
    }

    /// Check if a failed container should have its counter reset (1h stability window).
    fn should_reset_failed(&self, name: &str) -> bool {
        if self.attempt_count(name) < MAX_RESTART_ATTEMPTS {
            return false;
        }
        match self.last_failure.get(name) {
            Some(last) => last.elapsed().as_secs() >= STABILITY_RESET_SECS,
            None => false,
        }
    }
}

#[derive(Debug, Clone)]
struct ContainerHealth {
    name: String,
    app_id: String,
    state: String,
    podman_health: Option<String>,
    host_port_ready: Option<bool>,
    healthy: bool,
}

/// Track container memory usage over time for leak detection.
struct MemoryTracker {
    /// Per-container memory samples: (timestamp, rss_bytes)
    samples: HashMap<String, Vec<(Instant, u64)>>,
}

impl MemoryTracker {
    fn new() -> Self {
        Self {
            samples: HashMap::new(),
        }
    }

    /// Record a memory sample for a container.
    fn record(&mut self, name: &str, rss_bytes: u64) {
        let entry = self.samples.entry(name.to_string()).or_default();
        entry.push((Instant::now(), rss_bytes));
        // Keep only last 288 samples (24h at 5min intervals)
        if entry.len() > 288 {
            entry.remove(0);
        }
    }

    /// Check if a container's memory has grown by more than 50% over the tracking period.
    /// Returns Some(growth_percent) if a leak is detected, None otherwise.
    fn check_leak(&self, name: &str) -> Option<f64> {
        let samples = self.samples.get(name)?;
        if samples.len() < 12 {
            return None; // Need at least 1 hour of data
        }
        let (oldest_time, oldest_rss) = samples.first()?;
        let (_, latest_rss) = samples.last()?;
        let elapsed_hours = oldest_time.elapsed().as_secs() as f64 / 3600.0;
        if elapsed_hours < 1.0 || *oldest_rss == 0 {
            return None;
        }
        let growth = (*latest_rss as f64 - *oldest_rss as f64) / *oldest_rss as f64 * 100.0;
        if growth > 50.0 {
            Some(growth)
        } else {
            None
        }
    }
}

// ── Persistent restart tracking ────────────────────────────────────────
// Survives process restarts so a container can't loop infinitely by
// crashing 3 times → triggering process restart → resetting counter → repeat.

const RESTART_HISTORY_FILE: &str = "restart-tracker.json";

#[derive(Serialize, Deserialize, Default)]
struct RestartHistory {
    containers: HashMap<String, ContainerRestartRecord>,
}

#[derive(Serialize, Deserialize, Clone)]
struct ContainerRestartRecord {
    attempts: u32,
    last_failure_epoch: i64,
}

impl RestartHistory {
    async fn load(data_dir: &Path) -> Self {
        let path = data_dir.join(RESTART_HISTORY_FILE);
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    async fn save(&self, data_dir: &Path) {
        let path = data_dir.join(RESTART_HISTORY_FILE);
        if let Ok(json) = serde_json::to_string(self) {
            let _ = tokio::fs::write(&path, json).await;
        }
    }

    /// Seed the in-memory RestartTracker from persisted history.
    fn seed_tracker(&self, tracker: &mut RestartTracker) {
        let now_epoch = chrono::Utc::now().timestamp();
        for (name, record) in &self.containers {
            // Only seed if last failure was within the stability window
            let secs_since_failure = now_epoch - record.last_failure_epoch;
            if secs_since_failure < STABILITY_RESET_SECS as i64 && record.attempts > 0 {
                tracker.attempts.insert(name.clone(), record.attempts);
                info!(
                    "Restored restart counter for {}: {} attempts ({}s ago)",
                    name, record.attempts, secs_since_failure
                );
            }
        }
    }

    fn record_attempt(&mut self, name: &str) {
        let entry = self
            .containers
            .entry(name.to_string())
            .or_insert(ContainerRestartRecord {
                attempts: 0,
                last_failure_epoch: 0,
            });
        entry.attempts += 1;
        entry.last_failure_epoch = chrono::Utc::now().timestamp();
    }

    fn clear(&mut self, name: &str) {
        self.containers.remove(name);
    }
}

/// Query container memory stats from podman.
async fn check_container_memory() -> HashMap<String, u64> {
    let output = match tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new("podman")
            .args([
                "stats",
                "--no-stream",
                "--format",
                "{{.Name}} {{.MemUsage}}",
            ])
            .output(),
    )
    .await
    {
        Ok(Ok(o)) if o.status.success() => o,
        Ok(Err(e)) => {
            debug!("podman stats failed: {}", e);
            return HashMap::new();
        }
        Err(_) => {
            debug!("podman stats timed out (30s)");
            return HashMap::new();
        }
        _ => return HashMap::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut result = HashMap::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts[0].to_string();
            // Parse memory like "123.4MiB", "1.2GiB", "45.6kB"
            let mem_str = parts[1];
            if let Some(bytes) = parse_memory_string(mem_str) {
                result.insert(name, bytes);
            }
        }
    }
    result
}

/// Parse memory string like "123.4MiB" or "1.2GiB" to bytes.
fn parse_memory_string(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.ends_with("GiB") {
        let num: f64 = s.strip_suffix("GiB")?.parse().ok()?;
        Some((num * 1_073_741_824.0) as u64)
    } else if s.ends_with("MiB") {
        let num: f64 = s.strip_suffix("MiB")?.parse().ok()?;
        Some((num * 1_048_576.0) as u64)
    } else if s.ends_with("KiB") || s.ends_with("kB") {
        let suffix = if s.ends_with("KiB") { "KiB" } else { "kB" };
        let num: f64 = s.strip_suffix(suffix)?.parse().ok()?;
        Some((num * 1024.0) as u64)
    } else if s.ends_with("B") {
        let num: f64 = s.strip_suffix('B')?.parse().ok()?;
        Some(num as u64)
    } else {
        None
    }
}

/// Query all containers and their health status.
async fn check_containers() -> Vec<ContainerHealth> {
    let output = match tokio::time::timeout(
        std::time::Duration::from_secs(60),
        tokio::process::Command::new("podman")
            .args(["ps", "-a", "--format", "json"])
            .output(),
    )
    .await
    {
        Ok(Ok(o)) if o.status.success() => o,
        Ok(Err(e)) => {
            debug!("podman ps failed: {}", e);
            return Vec::new();
        }
        Err(_) => {
            debug!("podman ps timed out (60s)");
            return Vec::new();
        }
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let containers: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap_or_default();

    let live_container_ids = live_container_ids(&containers);
    cleanup_stale_podman_healthcheck_units(&live_container_ids).await;

    // Monitor ALL long-running containers for health — backend services (databases,
    // nbxplorer, mempool-api) and UI containers need auto-restart too.
    // Only skip ephemeral containers (build infrastructure, init one-shots).

    let mut out = Vec::new();
    for c in &containers {
        let name = c.get("Names").and_then(|v| {
            if let Some(arr) = v.as_array() {
                arr.first().and_then(|n| n.as_str()).map(|s| s.to_string())
            } else {
                v.as_str().map(|s| s.to_string())
            }
        });
        let Some(name) = name else {
            continue;
        };

        // Skip podman-compose infrastructure and one-shot init containers
        if name.starts_with("indeedhub-build_") || name.contains("-init") {
            continue;
        }

        let app_id = name.strip_prefix("archy-").unwrap_or(&name).to_string();

        let state = c
            .get("State")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_lowercase();

        let podman_health = parse_podman_health(c, &state);
        let host_ports = host_tcp_ports_from_container(c);
        // Only raw-probe published ports for containers WITHOUT their own
        // podman healthcheck. The healthcheck is the better signal, and the
        // bare TCP connect+close is noisy against TLS listeners — LND logged
        // "http: TLS handshake error … EOF" on every monitor cycle because
        // this probe hit its REST/gRPC ports and hung up mid-handshake.
        let host_port_ready = if host_ports.is_empty() || podman_health.is_some() {
            None
        } else {
            Some(host_ports_ready(&host_ports).await)
        };
        let healthy = state == "running"
            && podman_health.as_deref() != Some("unhealthy")
            && host_port_ready != Some(false);

        out.push(ContainerHealth {
            name,
            app_id,
            state,
            podman_health,
            host_port_ready,
            healthy,
        });
    }
    out
}

fn host_tcp_ports_from_container(c: &serde_json::Value) -> Vec<std::net::SocketAddr> {
    let Some(ports) = c.get("Ports").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut out: Vec<std::net::SocketAddr> = ports
        .iter()
        .filter(|p| {
            p.get("protocol")
                .and_then(|v| v.as_str())
                .unwrap_or("tcp")
                .eq_ignore_ascii_case("tcp")
        })
        .filter_map(|p| {
            let port = p.get("host_port")?.as_u64()?;
            let port = u16::try_from(port).ok().filter(|port| *port != 0)?;
            let bind = p.get("host_ip").and_then(|v| v.as_str()).unwrap_or("");
            // Wildcard listeners are reachable through the corresponding
            // loopback family. Explicit binds must be probed at that address:
            // probing a WireGuard-only port on 127.0.0.1 creates false failures
            // and endlessly restarts an otherwise healthy app.
            let address: std::net::IpAddr = match bind {
                "" | "0.0.0.0" => "127.0.0.1".parse().ok()?,
                "::" => "::1".parse().ok()?,
                explicit => explicit.parse().ok()?,
            };
            Some(std::net::SocketAddr::new(address, port))
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

async fn host_ports_ready(ports: &[std::net::SocketAddr]) -> bool {
    for port in ports {
        let ready = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            tokio::net::TcpStream::connect(*port),
        )
        .await
        .is_ok_and(|r| r.is_ok());
        if !ready {
            return false;
        }
    }
    true
}

fn live_container_ids(containers: &[serde_json::Value]) -> HashSet<String> {
    containers
        .iter()
        .filter_map(|c| {
            c.get("Id")
                .or_else(|| c.get("ID"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect()
}

async fn cleanup_stale_podman_healthcheck_units(live_container_ids: &HashSet<String>) {
    if live_container_ids.is_empty() {
        return;
    }

    let mut units = stale_healthcheck_units_from_systemd(live_container_ids).await;
    if units.is_empty() {
        return;
    }
    units.sort();
    units.dedup();

    let mut cleaned = 0;
    for unit in units {
        let Some(container_id) = parse_podman_healthcheck_unit(&unit) else {
            continue;
        };
        let service = format!("{}.service", unit.trim_end_matches(".timer"));
        if stop_user_unit(&unit).await {
            cleaned += 1;
        }
        let _ = stop_user_unit(&service).await;
        let _ = reset_failed_user_unit(&service).await;
        debug!(
            "Stopped stale Podman healthcheck unit {} for removed container {}",
            unit, container_id
        );
    }

    if cleaned > 0 {
        info!("Cleaned {} stale Podman healthcheck timer(s)", cleaned);
    }
}

async fn stale_healthcheck_units_from_systemd(live_container_ids: &HashSet<String>) -> Vec<String> {
    let mut units = Vec::new();
    for args in [
        [
            "--user",
            "list-timers",
            "--all",
            "--no-legend",
            "--no-pager",
        ]
        .as_slice(),
        ["--user", "list-units", "--all", "--no-legend", "--no-pager"].as_slice(),
    ] {
        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(20),
            tokio::process::Command::new("systemctl")
                .args(args.iter().copied())
                .output(),
        )
        .await
        {
            Ok(Ok(output)) if output.status.success() => output,
            Ok(Ok(output)) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                debug!("systemctl {} failed: {}", args.join(" "), stderr.trim());
                continue;
            }
            Ok(Err(e)) => {
                debug!("Failed to run systemctl {}: {}", args.join(" "), e);
                continue;
            }
            Err(_) => {
                debug!("systemctl {} timed out", args.join(" "));
                continue;
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        units.extend(stale_healthcheck_units(&stdout, live_container_ids));
    }
    units
}

fn stale_healthcheck_units(output: &str, live_container_ids: &HashSet<String>) -> Vec<String> {
    output
        .lines()
        .flat_map(|line| line.split_whitespace())
        .filter_map(|token| {
            let unit = token.trim_start_matches('●');
            let id = parse_podman_healthcheck_unit(unit)?;
            (!live_container_ids.contains(id)).then(|| unit.to_string())
        })
        .collect()
}

fn parse_podman_healthcheck_unit(unit: &str) -> Option<&str> {
    let unit = unit
        .strip_suffix(".timer")
        .or_else(|| unit.strip_suffix(".service"))?;
    let (container_id, _suffix) = unit.split_once('-')?;
    if container_id.len() == 64 && container_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(container_id)
    } else {
        None
    }
}

async fn stop_user_unit(unit: &str) -> bool {
    run_systemctl_user(["stop", unit]).await
}

async fn reset_failed_user_unit(unit: &str) -> bool {
    run_systemctl_user(["reset-failed", unit]).await
}

async fn run_systemctl_user<const N: usize>(args: [&str; N]) -> bool {
    let output = match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::process::Command::new("systemctl")
            .arg("--user")
            .args(args.iter().copied())
            .output(),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            debug!("Failed to run systemctl --user {}: {}", args.join(" "), e);
            return false;
        }
        Err(_) => {
            debug!("systemctl --user {} timed out", args.join(" "));
            return false;
        }
    };

    if output.status.success() {
        true
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug!(
            "systemctl --user {} failed: {}",
            args.join(" "),
            stderr.trim()
        );
        false
    }
}

fn parse_podman_health(c: &serde_json::Value, state: &str) -> Option<String> {
    c.get("Status")
        .and_then(|v| v.as_str())
        .and_then(parse_health_from_status)
        .or_else(|| {
            c.get("State")
                .and_then(|v| v.get("Health"))
                .and_then(|v| v.get("Status"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| state.contains("unhealthy").then(|| "unhealthy".to_string()))
}

fn parse_health_from_status(status: &str) -> Option<String> {
    let start = status.rfind('(')?;
    let end = status.rfind(')')?;
    (start < end).then(|| status[start + 1..end].to_string())
}

/// Try to recover a container. Running containers need a real restart so
/// rootless network helpers such as pasta are recreated; `podman start` is a
/// no-op for a running container with a missing host listener.
pub(crate) fn automatic_recovery_allowed(data_dir: &Path, name: &str) -> bool {
    match (
        crate::container::supervised_update::installed_unit(data_dir, name),
        crate::container::update_transaction::is_held(data_dir, name),
    ) {
        (Ok(None), Ok(false)) => true,
        _ => false, // Saved, held, or unreadable recovery evidence fails closed.
    }
}

async fn restart_container(name: &str, state: &str, data_dir: &Path) -> bool {
    // Keep admission held across the command, so an update cannot acquire
    // ownership after the policy check and race this automatic restart.
    let Ok(_admission) = crate::container::update_transaction::Guard::acquire(data_dir) else {
        return false;
    };
    if !automatic_recovery_allowed(data_dir, name) {
        warn!(container = %name, "Automatic restart refused: reviewed managed runtime needs explicit recovery");
        return false;
    }
    let action = if state == "running" {
        "restart"
    } else {
        "start"
    };
    info!("Auto-{}ing unhealthy container: {}", action, name);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        tokio::process::Command::new("systemd-run")
            .args(["--user", "--scope", "--quiet", "--collect", "podman"])
            .args([action, name])
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) if output.status.success() => {
            info!("Successfully recovered container: {}", name);
            true
        }
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to {} container {}: {}", action, name, stderr.trim());
            false
        }
        Ok(Err(e)) => {
            warn!("Failed to execute podman {} for {}: {}", action, name, e);
            false
        }
        Err(_) => {
            warn!("Timeout {}ing container {} (120s)", action, name);
            false
        }
    }
}

/// ElectrumX/electrs on-disk data dir. Wiped to force a clean resync when its
/// LevelDB is detected corrupt (see `maybe_recover_corrupt_electrumx`).
const ELECTRUMX_DATA_DIR: &str = "/var/lib/archipelago/electrumx";
/// Restart attempt at which we check for — and recover from — a corrupt
/// ElectrumX database. Late enough that a transient restart won't trigger a
/// destructive resync, early enough to self-heal before MAX_RESTART_ATTEMPTS.
const ELECTRUMX_DB_RESET_ATTEMPT: u32 = 3;

fn is_electrumx(name: &str) -> bool {
    let id = name.strip_prefix("archy-").unwrap_or(name);
    matches!(id, "electrumx" | "electrs" | "mempool-electrs")
}

/// True when a container's logs show the specific corrupt-LevelDB signature.
/// ElectrumX exit-loops with a plyvel error when its `hist`/`utxo` LevelDB
/// loses its CURRENT/MANIFEST pointer — typically after an unclean SIGKILL
/// (e.g. the cgroup cascade on a service restart). We match the exact failure
/// so a normal restart never triggers the destructive resync below.
fn looks_like_corrupt_electrumx_db(logs: &str) -> bool {
    logs.contains("create_if_missing is false")
        || logs.contains("Corruption:")
        || (logs.contains("plyvel") && logs.contains("does not exist"))
}

async fn electrumx_db_corrupt(name: &str) -> bool {
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio::process::Command::new("podman")
            .args(["logs", "--tail", "60", name])
            .output(),
    )
    .await;
    match out {
        Ok(Ok(output)) => {
            let logs = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
            looks_like_corrupt_electrumx_db(&logs)
        }
        _ => false,
    }
}

/// Wipe the ElectrumX LevelDB stores so the next start resyncs from scratch.
/// Files are owned by the container's mapped UID, so removal needs host sudo.
/// The mount point itself is preserved (the container expects it to exist).
/// Returns true if the reset succeeded.
async fn reset_electrumx_data() -> bool {
    let rm = format!(
        "rm -rf {dir}/hist {dir}/utxo {dir}/meta {dir}/COIN",
        dir = ELECTRUMX_DATA_DIR,
    );
    matches!(
        crate::update::host_sudo(&["sh", "-c", &rm]).await,
        Ok(status) if status.success()
    )
}

/// Self-heal a wedged ElectrumX: if it's exit-looping on a corrupt database,
/// wipe its data dir once (at `ELECTRUMX_DB_RESET_ATTEMPT`) so the impending
/// restart resyncs cleanly. Bounded to electrs containers, gated on the exact
/// corruption signature, and fired once per failure streak — when the resync
/// stabilises the restart tracker clears, so a future corruption can heal too.
async fn maybe_recover_corrupt_electrumx(name: &str, attempt: u32) {
    if attempt != ELECTRUMX_DB_RESET_ATTEMPT || !is_electrumx(name) {
        return;
    }
    if !electrumx_db_corrupt(name).await {
        return;
    }
    warn!(
        "ElectrumX {} is exit-looping on a corrupt database — resetting its data dir to force a clean resync",
        name
    );
    if reset_electrumx_data().await {
        info!("ElectrumX data dir reset; a fresh resync will begin on restart");
    } else {
        warn!("Failed to reset ElectrumX data dir (host sudo rm failed) — manual recovery may be needed");
    }
}

/// Spawn the health monitor background task.
pub fn spawn_health_monitor(state: Arc<StateManager>, data_dir: PathBuf) {
    tokio::spawn(async move {
        // Wait for boot recovery to complete before starting health checks.
        // This prevents the health monitor from fighting with crash_recovery
        // which is starting containers in tier order.
        info!("Health monitor: waiting for boot recovery to complete...");
        let wait_start = std::time::Instant::now();
        loop {
            if crate::crash_recovery::is_recovery_complete() {
                break;
            }
            // Safety timeout: start anyway after 30 minutes even if recovery hangs.
            // Stack recovery can take many minutes on low-resource nodes after reboot.
            if wait_start.elapsed().as_secs() > 1800 {
                warn!("Health monitor: boot recovery did not complete within 30 minutes, starting anyway");
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
        // Additional cooldown after recovery to let containers stabilize
        info!("Health monitor: recovery done, waiting 60s for containers to stabilize...");
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        info!("Health monitor: starting health checks");

        let mut tracker = RestartTracker::new();
        let mut mem_tracker = MemoryTracker::new();
        let mut mem_check_counter: u32 = 0;
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(CHECK_INTERVAL_SECS));
        // Skip missed ticks — prevents burst of health checks after slow podman response
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Load persistent restart history and seed the in-memory tracker
        let mut restart_history = RestartHistory::load(&data_dir).await;
        restart_history.seed_tracker(&mut tracker);
        #[allow(unused_assignments)]
        let mut history_dirty = false;

        loop {
            interval.tick().await;
            mem_check_counter += 1;

            // Check container memory every 5 minutes (every 5th health check)
            if mem_check_counter.is_multiple_of(5) {
                let mem_stats = check_container_memory().await;
                for (name, rss) in &mem_stats {
                    mem_tracker.record(name, *rss);
                    if let Some(growth) = mem_tracker.check_leak(name) {
                        warn!(
                            "Potential memory leak in {}: {:.0}% growth over tracking period",
                            name, growth
                        );
                    }
                }
            }

            let containers = check_containers().await;
            if containers.is_empty() {
                continue;
            }

            // Load user-stopped list to skip intentionally stopped containers
            let user_stopped = crate::crash_recovery::load_user_stopped(&data_dir).await;

            // Sort containers by startup tier so databases restart before dependent services
            let mut unhealthy: Vec<&ContainerHealth> = Vec::new();
            let mut state_changed = false;
            let (mut data, _) = state.get_snapshot().await;

            for container in &containers {
                // Skip optional/marketplace containers that aren't installed
                if let Some(pkg) = data.package_data.get(&container.app_id) {
                    if pkg.installed.is_none() {
                        debug!("Skipping uninstalled container: {}", container.name);
                        continue;
                    }
                    if matches!(
                        pkg.state,
                        PackageState::Starting | PackageState::Stopping | PackageState::Restarting | PackageState::Updating
                    ) {
                        debug!(
                            "Skipping container during package lifecycle transition: {} ({:?})",
                            container.name, pkg.state
                        );
                        continue;
                    }
                } else {
                    // Orphan: container exists in podman but archipelago has
                    // no package_data entry for it. Common after a variant
                    // switch (bitcoin-core ↔ bitcoin-knots) where the
                    // uninstall removed the package entry but the prior
                    // variant's container survived in stopped state. Without
                    // this guard the health monitor pages every minute with
                    // "Auto-restart failed (attempt N/10)" for an app the
                    // user can no longer see in the dashboard.
                    debug!(
                        "Skipping orphan container (not in package_data): {}",
                        container.name
                    );
                    let before = data.notifications.len();
                    data.notifications.retain(|n| {
                        n.app_id.as_deref() != Some(&container.app_id)
                            && !n.title.contains(&container.app_id)
                    });
                    if data.notifications.len() != before {
                        state_changed = true;
                    }
                    continue;
                }

                if container.healthy {
                    let before = data.notifications.len();
                    data.notifications.retain(|n| {
                        n.app_id.as_deref() != Some(&container.app_id)
                            && !n.title.contains(&container.app_id)
                    });
                    if data.notifications.len() != before {
                        state_changed = true;
                    }
                    if tracker.attempt_count(&container.name) > 0 {
                        info!(
                            "Container {} is healthy again after restart",
                            container.name
                        );
                        // Reset attempt counters for containers that depend on this one,
                        // since their previous failures may have been caused by this
                        // dependency being down
                        let recovered_id = container
                            .name
                            .strip_prefix("archy-")
                            .unwrap_or(&container.name)
                            .to_string();
                        for other in &containers {
                            let deps = container_dependencies(&other.name);
                            if deps
                                .iter()
                                .any(|d| *d == recovered_id || *d == container.name)
                                && tracker.attempt_count(&other.name) > 0
                            {
                                info!(
                                    "Resetting restart counter for {} (dependency {} recovered)",
                                    other.name, container.name
                                );
                                tracker.clear(&other.name);
                                restart_history.clear(&other.name);
                            }
                        }
                        tracker.clear(&container.name);
                        restart_history.clear(&container.name);
                        history_dirty = true;
                    }
                    continue;
                }
                // Handle exited, stopped, created, and Podman-unhealthy running containers.
                if container.podman_health.as_deref() == Some("unhealthy")
                    || container.host_port_ready == Some(false)
                    || container.state == "exited"
                    || container.state == "stopped"
                    || container.state == "created"
                {
                    // Skip user-stopped containers
                    if user_stopped.contains(&container.name) {
                        debug!("Skipping user-stopped container: {}", container.name);
                        continue;
                    }
                    unhealthy.push(container);
                }
            }

            let unhealthy_app_ids: HashSet<&str> = unhealthy
                .iter()
                .map(|container| container.app_id.as_str())
                .collect();
            let before = data.notifications.len();
            data.notifications.retain(|n| {
                !n.id.starts_with("health-")
                    || n.app_id
                        .as_deref()
                        .is_some_and(|app_id| unhealthy_app_ids.contains(app_id))
            });
            if data.notifications.len() != before {
                state_changed = true;
            }

            // Sort by startup tier: databases first, then core, then dependent, then apps, then UIs
            unhealthy.sort_by_key(|c| container_tier(&c.name));

            let mut prev_tier: Option<StartupTier> = None;
            for container in &unhealthy {
                if !automatic_recovery_allowed(&data_dir, &container.name) {
                    let id = format!("health-managed-{}", container.name);
                    if !data.notifications.iter().any(|n| n.id == id) {
                        data.notifications.push(Notification {
                            id,
                            level: NotificationLevel::Error,
                            title: format!("{} is unhealthy", container.app_id),
                            message: "Automatic restart is paused to preserve the installed configuration. Check the app before using its Start or Restart controls.".into(),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            app_id: Some(container.app_id.clone()),
                        });
                        if data.notifications.len() > 20 {
                            data.notifications = data.notifications.split_off(data.notifications.len() - 20);
                        }
                        state_changed = true;
                    }
                    continue;
                }
                let tier = container_tier(&container.name);

                // Reset counter after 1 hour for permanently failed containers
                if tracker.should_reset_failed(&container.name) {
                    info!(
                        "Resetting restart counter for {} after {}s stability window",
                        container.name, STABILITY_RESET_SECS
                    );
                    tracker.clear(&container.name);
                    restart_history.clear(&container.name);
                    history_dirty = true;
                }

                if tracker.attempt_count(&container.name) >= MAX_RESTART_ATTEMPTS {
                    debug!(
                        "Container {} exceeded max restart attempts ({})",
                        container.name, MAX_RESTART_ATTEMPTS
                    );
                    continue;
                }

                // Wait for backoff delay before retrying
                if !tracker.backoff_elapsed(&container.name) {
                    let delay = tracker.backoff_delay_secs(&container.name);
                    debug!(
                        "Container {} waiting for backoff ({}s)",
                        container.name, delay
                    );
                    continue;
                }

                // Skip if dependencies aren't running — they need to start first
                if !deps_are_running(&container.name, &containers) {
                    let deps = container_dependencies(&container.name);
                    debug!(
                        "Container {} waiting for dependencies {:?}",
                        container.name, deps
                    );
                    continue;
                }

                if has_running_bitcoin_conflict(&container.name, &containers) {
                    debug!(
                        "Skipping auto-restart for {} because the other Bitcoin implementation is running",
                        container.name
                    );
                    continue;
                }

                // A package.start/stop/restart worker is mid-sequence on this
                // app (or its owning stack): the container is legitimately
                // down between the worker's stop and start halves. Restarting
                // it here races the worker the same way the reconciler's
                // repair-recreate did (e275494a) — skip for this cycle.
                if lifecycle_op_covers_container(&container.name, &container.app_id) {
                    debug!(
                        "Skipping auto-restart for {}: lifecycle operation in flight",
                        container.name
                    );
                    continue;
                }

                // When transitioning to a higher tier, wait briefly for previous tier to stabilize
                if let Some(prev) = prev_tier {
                    if tier > prev {
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
                prev_tier = Some(tier);

                if tracker.record_attempt(&container.name) {
                    restart_history.record_attempt(&container.name);
                    history_dirty = true;
                    let attempt = tracker.attempt_count(&container.name);
                    info!(
                        "Restarting {} (tier {:?}, attempt {}/{}, backoff {}s)",
                        container.name,
                        tier,
                        attempt,
                        MAX_RESTART_ATTEMPTS,
                        BACKOFF_DELAYS_SECS
                            .get(attempt.saturating_sub(1) as usize)
                            .unwrap_or(&90)
                    );

                    // Before restarting, self-heal a corrupt ElectrumX DB so
                    // the restart resyncs cleanly instead of crash-looping.
                    maybe_recover_corrupt_electrumx(&container.name, attempt).await;

                    let restarted = restart_container(&container.name, &container.state, &data_dir).await;

                    if !restarted || attempt >= MAX_RESTART_ATTEMPTS {
                        let notification = Notification {
                            id: format!(
                                "health-{}-{}",
                                container.app_id,
                                chrono::Utc::now().timestamp()
                            ),
                            level: NotificationLevel::Error,
                            title: format!("{} is unhealthy", container.app_id),
                            message: if restarted {
                                format!(
                                    "Container restarted ({}/{} attempts). May need manual attention.",
                                    attempt, MAX_RESTART_ATTEMPTS
                                )
                            } else {
                                format!(
                                    "Auto-restart failed (attempt {}/{}). Container state: {}",
                                    attempt, MAX_RESTART_ATTEMPTS, container.state
                                )
                            },
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            app_id: Some(container.app_id.clone()),
                        };

                        data.notifications.push(notification.clone());
                        if data.notifications.len() > 20 {
                            data.notifications =
                                data.notifications.split_off(data.notifications.len() - 20);
                        }
                        state_changed = true;

                        let webhook_payload = webhooks::WebhookPayload {
                            event: WebhookEvent::ContainerCrash,
                            title: notification.title,
                            message: notification.message,
                            timestamp: notification.timestamp,
                            node_id: String::new(),
                            details: Some(serde_json::json!({
                                "container": container.name,
                                "app_id": container.app_id,
                                "state": container.state,
                                "attempt": attempt,
                                "tier": format!("{:?}", tier),
                            })),
                        };
                        webhooks::send_webhook(&data_dir, webhook_payload).await;
                    }
                }
            }

            if state_changed {
                state.update_data(data).await;
                debug!("Health monitor: state updated with notifications");
            }

            // Persist restart history to disk (debounced: once per check cycle)
            if history_dirty {
                restart_history.save(&data_dir).await;
                history_dirty = false;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn automatic_restart_refuses_competing_lifecycle_before_command() {
        let root = tempfile::TempDir::new().unwrap();
        let _guard = crate::container::update_transaction::Guard::acquire(root.path()).unwrap();
        assert!(!restart_container("indeedhub-ffmpeg", "running", root.path()).await);
    }

    #[tokio::test]
    async fn automatic_recovery_never_restarts_saved_held_or_damaged_managed_runtime() {
        let root = tempfile::tempdir().unwrap();
        let name = "indeedhub-api";
        assert!(automatic_recovery_allowed(root.path(), name));
        let installed = root.path().join("update-transactions/installed-units");
        std::fs::create_dir_all(&installed).unwrap();
        let record = installed.join(format!("{name}.json"));
        std::fs::write(&record, serde_json::to_vec(&serde_json::json!({
            "schema": 1, "operation": uuid::Uuid::new_v4().to_string(),
            "name": name, "body": "[Container]\nImage=original:retained\n", "mode": 0o600
        })).unwrap()).unwrap();
        assert!(!automatic_recovery_allowed(root.path(), name));
        assert!(!restart_container(name, "running", root.path()).await);
        std::fs::write(&record, b"damaged").unwrap();
        assert!(!automatic_recovery_allowed(root.path(), name));
        assert!(!restart_container(name, "stopped", root.path()).await);
        std::fs::remove_file(record).unwrap();
        let holds = root.path().join("update-transactions/holds");
        std::fs::create_dir_all(&holds).unwrap();
        std::fs::write(holds.join(name), uuid::Uuid::new_v4().to_string()).unwrap();
        assert!(!automatic_recovery_allowed(root.path(), name));
        assert!(!restart_container(name, "running", root.path()).await);
        std::fs::write(holds.join(name), b"damaged").unwrap();
        assert!(!automatic_recovery_allowed(root.path(), name));
    }

    #[test]
    fn test_restart_tracker_new_is_empty() {
        let tracker = RestartTracker::new();
        assert_eq!(tracker.attempt_count("any-container"), 0);
    }

    #[test]
    fn test_restart_tracker_record_attempt_increments() {
        let mut tracker = RestartTracker::new();
        assert!(tracker.record_attempt("test-container"));
        assert_eq!(tracker.attempt_count("test-container"), 1);

        assert!(tracker.record_attempt("test-container"));
        assert_eq!(tracker.attempt_count("test-container"), 2);

        assert!(tracker.record_attempt("test-container"));
        assert_eq!(tracker.attempt_count("test-container"), 3);
    }

    #[test]
    fn test_restart_tracker_max_attempts_exceeded() {
        let mut tracker = RestartTracker::new();
        for i in 1..=MAX_RESTART_ATTEMPTS {
            assert!(
                tracker.record_attempt("container-a"),
                "Attempt {} should be allowed",
                i
            );
        }
        assert!(!tracker.record_attempt("container-a"));
        assert_eq!(
            tracker.attempt_count("container-a"),
            MAX_RESTART_ATTEMPTS + 1
        );
    }

    #[test]
    fn test_restart_tracker_independent_containers() {
        let mut tracker = RestartTracker::new();
        tracker.record_attempt("container-a");
        tracker.record_attempt("container-a");
        tracker.record_attempt("container-b");

        assert_eq!(tracker.attempt_count("container-a"), 2);
        assert_eq!(tracker.attempt_count("container-b"), 1);
        assert_eq!(tracker.attempt_count("container-c"), 0);
    }

    #[test]
    fn test_restart_tracker_clear_resets_count() {
        let mut tracker = RestartTracker::new();
        tracker.record_attempt("container-x");
        tracker.record_attempt("container-x");
        assert_eq!(tracker.attempt_count("container-x"), 2);

        tracker.clear("container-x");
        assert_eq!(tracker.attempt_count("container-x"), 0);
    }

    #[test]
    fn test_restart_tracker_clear_allows_new_attempts() {
        let mut tracker = RestartTracker::new();
        for _ in 0..=MAX_RESTART_ATTEMPTS {
            tracker.record_attempt("container-y");
        }
        assert!(!tracker.record_attempt("container-y"));

        tracker.clear("container-y");
        assert!(tracker.record_attempt("container-y"));
        assert_eq!(tracker.attempt_count("container-y"), 1);
    }

    #[test]
    fn test_restart_tracker_clear_nonexistent_is_safe() {
        let mut tracker = RestartTracker::new();
        tracker.clear("nonexistent");
        assert_eq!(tracker.attempt_count("nonexistent"), 0);
    }

    #[test]
    fn test_container_health_struct() {
        let health = ContainerHealth {
            name: "archy-bitcoin-knots".to_string(),
            app_id: "bitcoin-knots".to_string(),
            state: "running".to_string(),
            podman_health: Some("healthy".to_string()),
            host_port_ready: None,
            healthy: true,
        };
        assert!(health.healthy);
        assert_eq!(health.name, "archy-bitcoin-knots");
        assert_eq!(health.app_id, "bitcoin-knots");
        assert_eq!(health.state, "running");
    }

    #[test]
    fn test_container_health_unhealthy() {
        let health = ContainerHealth {
            name: "archy-mempool-web".to_string(),
            app_id: "mempool-web".to_string(),
            state: "exited".to_string(),
            podman_health: None,
            host_port_ready: None,
            healthy: false,
        };
        assert!(!health.healthy);
        assert_eq!(health.state, "exited");
    }

    #[test]
    fn test_max_restart_attempts_constant() {
        assert!(MAX_RESTART_ATTEMPTS >= 1);
        assert!(MAX_RESTART_ATTEMPTS <= 20);
        assert_eq!(MAX_RESTART_ATTEMPTS, 10);
    }

    #[test]
    fn test_check_interval_constant() {
        assert_eq!(CHECK_INTERVAL_SECS, 120);
    }

    #[test]
    fn test_backoff_delays() {
        let tracker = RestartTracker::new();
        // Before any attempts, delay is first backoff
        assert_eq!(tracker.backoff_delay_secs("test"), BACKOFF_DELAYS_SECS[0]);
    }

    #[test]
    fn test_backoff_delays_escalate() {
        let mut tracker = RestartTracker::new();
        tracker.record_attempt("test");
        assert_eq!(tracker.backoff_delay_secs("test"), BACKOFF_DELAYS_SECS[0]); // 10s
        tracker.record_attempt("test");
        assert_eq!(tracker.backoff_delay_secs("test"), BACKOFF_DELAYS_SECS[1]); // 30s
        tracker.record_attempt("test");
        assert_eq!(tracker.backoff_delay_secs("test"), BACKOFF_DELAYS_SECS[2]); // 90s
    }

    #[test]
    fn test_backoff_elapsed_true_for_new() {
        let tracker = RestartTracker::new();
        assert!(tracker.backoff_elapsed("test"));
    }

    #[test]
    fn test_stability_reset_not_triggered_early() {
        let mut tracker = RestartTracker::new();
        tracker.record_attempt("test");
        assert!(!tracker.should_reset_failed("test"));
    }

    #[test]
    fn test_container_tier_database() {
        assert_eq!(container_tier("archy-btcpay-db"), StartupTier::Database);
        assert_eq!(container_tier("immich_postgres"), StartupTier::Database);
        assert_eq!(container_tier("penpot-valkey"), StartupTier::Database);
        assert_eq!(container_tier("indeedhub-postgres"), StartupTier::Database);
        assert_eq!(container_tier("indeedhub-redis"), StartupTier::Database);
        assert_eq!(container_tier("indeedhub-minio"), StartupTier::Database);
    }

    #[test]
    fn test_container_tier_indeedhub_api() {
        assert_eq!(
            container_tier("indeedhub-api"),
            StartupTier::DependentService
        );
    }

    #[test]
    fn test_container_tier_mempool_api() {
        assert_eq!(container_tier("mempool-api"), StartupTier::DependentService);
    }

    #[test]
    fn test_container_dependencies() {
        assert!(container_dependencies("lnd").contains(&"bitcoin"));
        assert!(container_dependencies("indeedhub-api").contains(&"indeedhub-postgres"));
        assert!(container_dependencies("indeedhub-api").contains(&"indeedhub-redis"));
        assert!(container_dependencies("mempool-api").contains(&"mempool-db"));
        assert!(container_dependencies("mempool-api").contains(&"electrumx"));
        assert!(container_dependencies("nextcloud").is_empty());
    }

    #[test]
    fn test_deps_are_running() {
        let containers = vec![
            ContainerHealth {
                name: "indeedhub-postgres".into(),
                app_id: "indeedhub-postgres".into(),
                state: "running".into(),
                podman_health: None,
                host_port_ready: None,
                healthy: true,
            },
            ContainerHealth {
                name: "indeedhub-redis".into(),
                app_id: "indeedhub-redis".into(),
                state: "running".into(),
                podman_health: None,
                host_port_ready: None,
                healthy: true,
            },
            ContainerHealth {
                name: "indeedhub-minio".into(),
                app_id: "indeedhub-minio".into(),
                state: "running".into(),
                podman_health: None,
                host_port_ready: None,
                healthy: true,
            },
            ContainerHealth {
                name: "indeedhub-api".into(),
                app_id: "indeedhub-api".into(),
                state: "exited".into(),
                podman_health: None,
                host_port_ready: None,
                healthy: false,
            },
        ];
        assert!(deps_are_running("indeedhub-api", &containers));
        // Missing postgres
        let partial = vec![ContainerHealth {
            name: "indeedhub-redis".into(),
            app_id: "indeedhub-redis".into(),
            state: "running".into(),
            podman_health: None,
            host_port_ready: None,
            healthy: true,
        }];
        assert!(!deps_are_running("indeedhub-api", &partial));
    }

    #[test]
    fn test_bitcoin_dependency_accepts_core_or_knots() {
        let core = vec![ContainerHealth {
            name: "bitcoin-core".into(),
            app_id: "bitcoin-core".into(),
            state: "running".into(),
            podman_health: None,
            host_port_ready: None,
            healthy: true,
        }];
        assert!(deps_are_running("lnd", &core));

        let knots = vec![ContainerHealth {
            name: "bitcoin-knots".into(),
            app_id: "bitcoin-knots".into(),
            state: "running".into(),
            podman_health: None,
            host_port_ready: None,
            healthy: true,
        }];
        assert!(deps_are_running("fedimint", &knots));

        let stopped = vec![ContainerHealth {
            name: "bitcoin-core".into(),
            app_id: "bitcoin-core".into(),
            state: "stopped".into(),
            podman_health: None,
            host_port_ready: None,
            healthy: false,
        }];
        assert!(!deps_are_running("electrumx", &stopped));
    }

    #[test]
    fn test_bitcoin_conflict_detection() {
        let containers = vec![ContainerHealth {
            name: "bitcoin-core".into(),
            app_id: "bitcoin-core".into(),
            state: "running".into(),
            podman_health: None,
            host_port_ready: None,
            healthy: true,
        }];

        assert!(has_running_bitcoin_conflict("bitcoin-knots", &containers));
        assert!(!has_running_bitcoin_conflict("bitcoin-core", &containers));
        assert!(!has_running_bitcoin_conflict("lnd", &containers));
    }

    #[test]
    fn test_bitcoin_conflict_ignores_stopped_sibling() {
        let containers = vec![ContainerHealth {
            name: "bitcoin-core".into(),
            app_id: "bitcoin-core".into(),
            state: "stopped".into(),
            podman_health: None,
            host_port_ready: None,
            healthy: false,
        }];

        assert!(!has_running_bitcoin_conflict("bitcoin-knots", &containers));
    }

    #[test]
    fn test_container_tier_core() {
        assert_eq!(container_tier("bitcoin-knots"), StartupTier::CoreInfra);
    }

    #[tokio::test]
    async fn lifecycle_guard_covers_name_app_id_and_legacy_underscores() {
        assert!(!lifecycle_op_covers_container(
            "hm-guard-app",
            "hm-guard-app"
        ));

        // Held package lock covers the container directly and via stack
        // membership; the '_'→'-' probe covers legacy underscore names.
        let lock = crate::app_ops::op_lock("immich");
        let _guard = lock.lock().await;
        assert!(lifecycle_op_covers_container(
            "immich-postgres",
            "immich-postgres"
        ));
        assert!(lifecycle_op_covers_container(
            "immich_postgres",
            "immich_postgres"
        ));
        assert!(!lifecycle_op_covers_container("grafana", "grafana"));

        // archy- prefixed containers probe via the stripped app id too.
        let lnd = crate::app_ops::op_lock("lnd");
        let _lnd_guard = lnd.lock().await;
        assert!(lifecycle_op_covers_container("archy-lnd", "lnd"));
    }

    #[test]
    fn test_container_tier_dependent() {
        assert_eq!(container_tier("lnd"), StartupTier::DependentService);
        assert_eq!(
            container_tier("mempool-electrs"),
            StartupTier::DependentService
        );
        assert_eq!(
            container_tier("archy-nbxplorer"),
            StartupTier::DependentService
        );
    }

    #[test]
    fn test_container_tier_frontend() {
        assert_eq!(container_tier("archy-mempool-web"), StartupTier::Frontend);
        assert_eq!(container_tier("archy-bitcoin-ui"), StartupTier::Frontend);
    }

    #[test]
    fn test_container_tier_application_default() {
        assert_eq!(container_tier("nextcloud"), StartupTier::Application);
        assert_eq!(container_tier("grafana"), StartupTier::Application);
        assert_eq!(container_tier("btcpay-server"), StartupTier::Application);
    }

    #[test]
    fn test_tier_ordering() {
        assert!(StartupTier::Database < StartupTier::CoreInfra);
        assert!(StartupTier::CoreInfra < StartupTier::DependentService);
        assert!(StartupTier::DependentService < StartupTier::Application);
        assert!(StartupTier::Application < StartupTier::Frontend);
    }

    #[test]
    fn test_parse_memory_gib() {
        assert_eq!(parse_memory_string("1.5GiB"), Some(1_610_612_736));
    }

    #[test]
    fn test_parse_memory_mib() {
        assert_eq!(parse_memory_string("256MiB"), Some(268_435_456));
    }

    #[test]
    fn test_parse_memory_kib() {
        assert_eq!(parse_memory_string("512KiB"), Some(524_288));
    }

    #[test]
    fn test_parse_memory_invalid() {
        assert_eq!(parse_memory_string("abc"), None);
    }

    #[test]
    fn test_memory_tracker_no_leak_few_samples() {
        let mut tracker = MemoryTracker::new();
        tracker.record("test", 100_000_000);
        assert!(tracker.check_leak("test").is_none());
    }

    #[test]
    fn podman_unhealthy_makes_running_container_unhealthy() {
        let c = serde_json::json!({ "Status": "Up 5 minutes (unhealthy)" });
        assert_eq!(
            parse_podman_health(&c, "running").as_deref(),
            Some("unhealthy")
        );
    }

    #[test]
    fn parses_podman_healthcheck_systemd_units() {
        let id = "c1f44a6369c91d65f9e9f6134a5591aa02792cff2f1a4e0f689b5a6c03b6c77c";
        assert_eq!(
            parse_podman_healthcheck_unit(&format!("{}-15c66ddfefa8a763.timer", id)),
            Some(id)
        );
        assert_eq!(
            parse_podman_healthcheck_unit(&format!("{}-15c66ddfefa8a763.service", id)),
            Some(id)
        );
        assert_eq!(parse_podman_healthcheck_unit("grafana.service"), None);
        assert_eq!(
            parse_podman_healthcheck_unit(
                "nothexzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz-x.timer"
            ),
            None
        );
    }

    #[test]
    fn stale_healthcheck_units_filters_only_removed_container_ids() {
        let live = "6467e25fd87d791a63fe9dbf6e2fabc7bf26533aa2c402b1089effeacf7ebbba";
        let stale = "c1f44a6369c91d65f9e9f6134a5591aa02792cff2f1a4e0f689b5a6c03b6c77c";
        let mut live_ids = HashSet::new();
        live_ids.insert(live.to_string());

        let output = format!(
            "  {live}-6fdc497fd3ba3b62.timer loaded active waiting\n\
             ● {stale}-15c66ddfefa8a763.service loaded failed failed\n\
             grafana.service loaded active running\n\
             {stale}-1898d85de0bb707f.timer loaded active waiting\n"
        );

        let mut units = stale_healthcheck_units(&output, &live_ids);
        units.sort();
        assert_eq!(
            units,
            vec![
                format!("{stale}-15c66ddfefa8a763.service"),
                format!("{stale}-1898d85de0bb707f.timer"),
            ]
        );
    }

    #[test]
    fn is_electrumx_matches_electrs_variants_only() {
        assert!(is_electrumx("electrumx"));
        assert!(is_electrumx("archy-electrumx"));
        assert!(is_electrumx("electrs"));
        assert!(is_electrumx("mempool-electrs"));
        assert!(!is_electrumx("bitcoin-knots"));
        assert!(!is_electrumx("lnd"));
        assert!(!is_electrumx("electrs-ui")); // the web UI, not the indexer
    }

    #[test]
    fn corrupt_db_detection_matches_plyvel_signature() {
        // The exact line ElectrumX exit-loops on (observed on .116).
        let corrupt = "plyvel._plyvel.Error: b'Invalid argument: hist: does not exist (create_if_missing is false)'";
        assert!(looks_like_corrupt_electrumx_db(corrupt));
        assert!(looks_like_corrupt_electrumx_db(
            "Corruption: bad block in hist"
        ));
    }

    #[test]
    fn corrupt_db_detection_ignores_healthy_logs() {
        let healthy = "INFO:BlockProcessor:our height: 117,009 daemon: 953,480 UTXOs 28MB\n\
                       INFO:SessionManager:RPC server listening on 0.0.0.0:8000";
        assert!(!looks_like_corrupt_electrumx_db(healthy));
        // "catching up" / normal restart noise must not trigger a destructive wipe.
        assert!(!looks_like_corrupt_electrumx_db(
            "Prefetcher:catching up to daemon height 953,480"
        ));
    }
    #[test]
    fn published_port_probes_preserve_explicit_bind_addresses() {
        let c = serde_json::json!({"Ports": [
            {"host_ip":"127.0.0.1","host_port":8081,"protocol":"tcp"},
            {"host_ip":"10.77.0.2","host_port":18081,"protocol":"tcp"},
            {"host_ip":"10.77.0.2","host_port":18443,"protocol":"tcp"},
            {"host_ip":"::1","host_port":8082,"protocol":"tcp"}
        ]});
        let targets = host_tcp_ports_from_container(&c);
        for target in [
            "127.0.0.1:8081",
            "10.77.0.2:18081",
            "10.77.0.2:18443",
            "[::1]:8082",
        ] {
            assert!(targets.contains(&target.parse().unwrap()));
        }
        assert!(!targets.contains(&"127.0.0.1:18081".parse().unwrap()));
    }

    #[test]
    fn published_port_probes_normalize_wildcards_and_ignore_invalid_entries() {
        let c = serde_json::json!({"Ports": [
            {"host_ip":"0.0.0.0","host_port":8080},
            {"host_ip":"","host_port":8080},
            {"host_ip":"::","host_port":8080},
            {"host_ip":"10.0.0.1","host_port":53,"protocol":"udp"},
            {"host_ip":"bad","host_port":8080},
            {"host_port":0}, {"host_port":65536}, {"container_port":80}
        ]});
        let targets = host_tcp_ports_from_container(&c);
        assert_eq!(targets.len(), 2);
        assert!(targets.contains(&"127.0.0.1:8080".parse().unwrap()));
        assert!(targets.contains(&"[::1]:8080".parse().unwrap()));
    }

    #[tokio::test]
    async fn health_probe_reaches_non_default_loopback_and_detects_closed_port() {
        let listener = tokio::net::TcpListener::bind("127.0.0.2:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        assert!(host_ports_ready(&[address]).await);
        // Same port, wrong local address reproduces the former false failure.
        let wrong = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), address.port());
        assert!(!host_ports_ready(&[wrong]).await);
        drop(listener);
        assert!(!host_ports_ready(&[address]).await);
    }
}
