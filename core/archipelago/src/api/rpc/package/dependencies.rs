use super::config::get_containers_for_app;
use super::runtime::manifest_apps_dirs;
use crate::data_model::{PackageDataEntry, PackageState};
use anyhow::{Context, Result};
use archipelago_container::{AppManifest, Dependency};
use std::collections::HashMap;
use tracing::info;

/// Names of container variants that represent a running Bitcoin node
const BITCOIN_NAMES: &[&str] = &["bitcoin-knots", "bitcoin-core", "bitcoin"];

/// Names of container variants that represent a running Electrum indexer
const ELECTRUM_NAMES: &[&str] = &["electrumx", "mempool-electrs", "electrs"];
const ARCHIVAL_BITCOIN_DISK_GB: u64 = 1000;

/// The manifest string dependency that declares "needs an archival
/// (unpruned + txindex) Bitcoin node" — see `manifest_declares_archival_bitcoin`.
const ARCHIVAL_BITCOIN_DEPENDENCY: &str = "bitcoin:archival";

/// Whether `package_id`'s own on-disk manifest declares
/// `dependencies: [bitcoin:archival]`. Manifest-driven alternative to the
/// hardcoded id list below — a new app just declares the dependency instead
/// of needing a code change here.
fn manifest_declares_archival_bitcoin(package_id: &str) -> bool {
    // Registry-only apps need the same guard as OTA-bundled manifests. Honor
    // the verified catalog's effective manifest before the disk fallback.
    if let Some((_, value)) = crate::container::app_catalog::catalog_manifest_values()
        .into_iter()
        .find(|(id, _)| id == package_id)
    {
        if let Some(manifest) =
            crate::container::app_catalog::catalog_manifest_overlay(package_id, value)
        {
            return dependency_list_declares_archival_bitcoin(&manifest.app.dependencies);
        }
    }
    for apps_dir in manifest_apps_dirs() {
        let path = apps_dir.join(package_id).join("manifest.yml");
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(manifest) = AppManifest::parse(&contents) else {
            continue;
        };
        return dependency_list_declares_archival_bitcoin(&manifest.app.dependencies);
    }
    false
}

fn dependency_list_declares_archival_bitcoin(deps: &[Dependency]) -> bool {
    deps.iter()
        .any(|dep| matches!(dep, Dependency::Simple(s) if s == ARCHIVAL_BITCOIN_DEPENDENCY))
}

fn requires_unpruned_bitcoin(package_id: &str) -> bool {
    if manifest_declares_archival_bitcoin(package_id) {
        return true;
    }
    // Fallback for apps not yet migrated to the manifest declaration above.
    matches!(
        package_id,
        "electrumx" | "mempool-electrs" | "electrs" | "mempool" | "mempool-web"
    )
}

fn archival_bitcoin_required_message(package_id: &str) -> String {
    format!(
        "Requires an archival Bitcoin node while indexing: {package_id}. This node is running pruned Bitcoin because it does not have enough disk for full block history. Add enough storage for an archival node (about 1 TB or more), resync Bitcoin without pruning/with txindex, then install {package_id}."
    )
}

/// Snapshot of which dependency services are currently running.
#[derive(Debug)]
pub(super) struct RunningDeps {
    pub has_bitcoin: bool,
    pub has_electrumx: bool,
    pub has_lnd: bool,
}

pub(super) fn detect_running_deps_from_package_data(
    packages: &HashMap<String, PackageDataEntry>,
) -> RunningDeps {
    let is_running = |names: &[&str]| {
        names.iter().any(|name| {
            packages
                .get(*name)
                .map(|pkg| pkg.state == PackageState::Running)
                .unwrap_or(false)
        })
    };

    RunningDeps {
        has_bitcoin: is_running(BITCOIN_NAMES),
        has_electrumx: is_running(ELECTRUM_NAMES),
        has_lnd: is_running(&["lnd"]),
    }
}

/// Query podman for currently running containers and return dependency status.
pub(super) async fn detect_running_deps() -> Result<RunningDeps> {
    let dep_check = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new("podman")
            .args(["ps", "--format", "{{.Names}}"])
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Timed out checking running containers"))?
    .map_err(|e| anyhow::anyhow!("Failed to check running containers: {}", e))?;

    if !dep_check.status.success() {
        anyhow::bail!(
            "Failed to check running containers: {}",
            String::from_utf8_lossy(&dep_check.stderr).trim()
        );
    }

    let running = String::from_utf8_lossy(&dep_check.stdout);
    let is_running = |names: &[&str]| {
        running.lines().any(|l| {
            let name = l.trim();
            names.contains(&name)
        })
    };

    Ok(RunningDeps {
        has_bitcoin: is_running(BITCOIN_NAMES),
        has_electrumx: is_running(ELECTRUM_NAMES),
        has_lnd: is_running(&["lnd"]),
    })
}

/// Detect the container name of the running Bitcoin node so dependent stacks
/// (mempool) can point CORE_RPC_HOST at the right host. Bitcoin Knots and Bitcoin
/// Core are both reachable on archy-net by their container name — only the name
/// differs (`bitcoin-knots` vs `bitcoin-core`), so hardcoding one breaks the
/// other. Returns the first running BITCOIN_NAMES match; falls back to the
/// default `bitcoin-knots` if none is detected (callers gate on has_bitcoin).
pub(super) async fn detect_bitcoin_rpc_host() -> String {
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio::process::Command::new("podman")
            .args(["ps", "--format", "{{.Names}}"])
            .output(),
    )
    .await;
    if let Ok(Ok(o)) = out {
        if o.status.success() {
            let running = String::from_utf8_lossy(&o.stdout);
            if let Some(name) = pick_bitcoin_host(&running) {
                return name;
            }
        }
    }
    "bitcoin-knots".to_string()
}

/// Pure host-selection step of [`detect_bitcoin_rpc_host`], split out so it can
/// be unit-tested without a podman runtime. Returns the first `podman ps` line
/// whose trimmed name is one of [`BITCOIN_NAMES`]. (The Quadlet orchestrator
/// mirrors this in `prod_orchestrator::bitcoin_host`.)
fn pick_bitcoin_host(podman_names: &str) -> Option<String> {
    podman_names
        .lines()
        .map(|l| l.trim())
        .find(|name| BITCOIN_NAMES.contains(name))
        .map(|name| name.to_string())
}

#[cfg(test)]
mod bitcoin_host_tests {
    use super::pick_bitcoin_host;

    #[test]
    fn picks_knots() {
        let ps = "electrumx\nbitcoin-knots\narchy-mempool-db\n";
        assert_eq!(pick_bitcoin_host(ps).as_deref(), Some("bitcoin-knots"));
    }

    #[test]
    fn picks_core() {
        let ps = "lnd\nbitcoin-core\nelectrumx\n";
        assert_eq!(pick_bitcoin_host(ps).as_deref(), Some("bitcoin-core"));
    }

    #[test]
    fn picks_plain_bitcoin() {
        assert_eq!(pick_bitcoin_host("bitcoin\n").as_deref(), Some("bitcoin"));
    }

    #[test]
    fn none_when_no_bitcoin_node() {
        let ps = "electrumx\nlnd\narchy-mempool-db\n";
        assert_eq!(pick_bitcoin_host(ps), None);
    }

    #[test]
    fn ignores_substring_matches() {
        // A companion UI container must NOT be mistaken for the node itself.
        let ps = "archy-bitcoin-ui\nbitcoin-knots-foo\n";
        assert_eq!(pick_bitcoin_host(ps), None);
    }
}

/// Verify that required dependency services are running before installing an app.
/// Returns an error with a user-friendly message if dependencies are missing.
pub(super) fn check_install_deps(package_id: &str, deps: &RunningDeps) -> Result<()> {
    match package_id {
        "electrumx" | "mempool-electrs" | "electrs" if !deps.has_bitcoin => Err(anyhow::anyhow!(
            "ElectrumX requires a running Bitcoin node (Bitcoin Knots). \
                 Please install and start Bitcoin Knots first."
        )),
        "lnd" if !deps.has_bitcoin => Err(anyhow::anyhow!(
            "LND requires a running Bitcoin node (Bitcoin Knots). \
             Please install and start Bitcoin Knots first."
        )),
        "btcpay-server" | "btcpayserver" if !deps.has_bitcoin => Err(anyhow::anyhow!(
            "BTCPay Server requires a running Bitcoin node (Bitcoin Knots). \
             Please install and start Bitcoin Knots first."
        )),
        "mempool" | "mempool-web" if !deps.has_bitcoin => Err(anyhow::anyhow!(
            "Mempool requires Bitcoin Knots to be running. \
             Please install and start Bitcoin Knots first."
        )),
        // ElectrumX is deliberately NOT a hard gate for mempool: mempool-api
        // reconnects to electrum on its own (verified live — it retries
        // ECONNREFUSED in a loop and comes good when electrum starts
        // listening), and an ElectrumX mid-resync can be legitimately down
        // for DAYS. The hard gate here blocked repeated mempool installs on
        // .116 (2026-07-22) while electrumx was compacting. Block/fee views
        // work from bitcoind immediately; address lookups light up when
        // ElectrumX finishes.
        "mempool" | "mempool-web" if !deps.has_electrumx => {
            info!(
                "Mempool installing while ElectrumX is not running — \
                 mempool-api reconnects automatically once ElectrumX is up"
            );
            Ok(())
        }
        "fedimint" if !deps.has_bitcoin => {
            info!("Fedimint installing without local Bitcoin node — configure remote Bitcoin RPC in Fedimint guardian setup");
            Ok(())
        }
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Bounded dependency wait (install race fix)
// ---------------------------------------------------------------------------
//
// Confirmed race on fresh nodes: the user clicks "Install LND" while
// bitcoin-knots is itself still installing/starting. `check_install_deps`
// rejected instantly ("LND requires a running Bitcoin node…") even though
// Bitcoin came up 55s later. The fix: when the dependency is INSTALLED
// (container exists in `podman ps -a`, or the package state knows about it)
// but not Running yet, poll for up to DEP_WAIT_MAX_ATTEMPTS × DEP_WAIT_INTERVAL
// (~3 minutes) before failing, surfacing "Waiting for X to start…" via the
// install-progress message. If the dependency is not installed at all, fail
// fast with the canonical `check_install_deps` message — waiting can't help.

/// Poll interval while waiting for an installed dependency to start.
pub(super) const DEP_WAIT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
/// 36 × 5s = 3 minutes of bounded waiting.
pub(super) const DEP_WAIT_MAX_ATTEMPTS: u32 = 36;
/// Separate, much larger budget while a dependency is still INSTALLING
/// (image pulling, container not created yet): 360 × 5s = 30 minutes. A
/// fresh-node Bitcoin pull routinely takes >3 minutes, and rejecting LND
/// mid-pull was the confirmed "first install fails, second works" failure.
pub(super) const DEP_INSTALLING_WAIT_MAX_ATTEMPTS: u32 = 360;

/// Marker error: the install was rejected by the dependency gate BEFORE any
/// resource (container, image, data dir) was created for the package. The
/// async install wrapper (`async_lifecycle.rs`) downcasts to this to remove
/// the optimistic `Installing` state entry instead of leaving a phantom
/// "Stopped" tile whose Start fails with `no such object`.
#[derive(Debug)]
pub(in crate::api::rpc) struct DependencyGateError(pub String);

impl std::fmt::Display for DependencyGateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DependencyGateError {}

/// One unsatisfied install dependency: a user-facing label plus the container
/// name variants that would satisfy it.
struct MissingDep {
    label: &'static str,
    containers: &'static [&'static str],
}

/// Which dependencies `check_install_deps` would reject `package_id` over.
/// Must stay in lockstep with the match arms in `check_install_deps` (the
/// wait loop re-runs `check_install_deps` for the canonical error message).
fn missing_install_deps(package_id: &str, deps: &RunningDeps) -> Vec<MissingDep> {
    const BITCOIN: MissingDep = MissingDep {
        label: "Bitcoin",
        containers: BITCOIN_NAMES,
    };
    let mut missing = Vec::new();
    match package_id {
        "electrumx" | "mempool-electrs" | "electrs" | "lnd" | "btcpay-server" | "btcpayserver" => {
            if !deps.has_bitcoin {
                missing.push(BITCOIN);
            }
        }
        "mempool" | "mempool-web" => {
            if !deps.has_bitcoin {
                missing.push(BITCOIN);
            }
            // ElectrumX intentionally not required — see check_install_deps:
            // mempool-api reconnects when electrum comes up, and a resyncing
            // ElectrumX can be down for days (the 3-minute bounded wait here
            // would still fail the install for no benefit).
        }
        // fedimint deliberately absent: check_install_deps allows it without
        // a local Bitcoin node (remote RPC configured in guardian setup).
        _ => {}
    }
    missing
}

fn join_dep_labels(missing: &[MissingDep]) -> String {
    missing
        .iter()
        .map(|d| d.label)
        .collect::<Vec<_>>()
        .join(" and ")
}

/// One snapshot of the dependency world, fed to [`wait_for_install_deps`].
pub(super) struct DepProbe {
    /// Which dependency services are currently Running.
    pub running: RunningDeps,
    /// Container/package names that EXIST in any state — installed, but
    /// possibly not running yet (`podman ps -a`).
    pub existing: Vec<String>,
    /// Package ids currently mid-install (state Installing/Updating) —
    /// no container exists yet, but one is on the way, so the gate must
    /// wait for the install to finish instead of failing fast.
    pub installing: Vec<String>,
}

/// All container names known to podman in any state (`podman ps -a`).
/// Conservative on probe failure: returns an empty list, which makes the
/// wait loop fall back to the pre-fix fail-fast behavior.
pub(super) async fn detect_existing_containers() -> Vec<String> {
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new("podman")
            .args(["ps", "-a", "--format", "{{.Names}}"])
            .output(),
    )
    .await;
    match out {
        Ok(Ok(o)) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// Bounded dependency gate. Returns the (satisfied) `RunningDeps` snapshot,
/// or a [`DependencyGateError`]:
///   - immediately, when a missing dependency is not installed at all
///     (canonical `check_install_deps` message), or
///   - after `max_attempts × interval`, when an installed dependency never
///     reached Running.
///
/// `probe` and `on_waiting` are injected so unit tests can drive the loop
/// without a podman runtime; production wires them to
/// `RpcHandler::dep_probe_for_install` / `set_install_message`.
pub(super) async fn wait_for_install_deps<P, PF, L, LF>(
    package_id: &str,
    mut probe: P,
    mut on_waiting: L,
    max_attempts: u32,
    interval: std::time::Duration,
) -> Result<RunningDeps>
where
    P: FnMut() -> PF,
    PF: std::future::Future<Output = Result<DepProbe>>,
    L: FnMut(String) -> LF,
    LF: std::future::Future<Output = ()>,
{
    let mut waited_attempts = 0u32;
    let mut installing_attempts = 0u32;
    loop {
        let DepProbe {
            running,
            existing,
            installing,
        } = probe().await?;
        let missing = missing_install_deps(package_id, &running);
        if missing.is_empty() {
            // Keep behavior in lockstep with the canonical gate (covers any
            // future arm added there but not mirrored in missing_install_deps).
            check_install_deps(package_id, &running)?;
            return Ok(running);
        }

        // Fail fast if any missing dependency has no installed container
        // under any name variant AND no install in flight — waiting cannot
        // satisfy it.
        let some_dep_not_installed = missing.iter().any(|dep| {
            !dep.containers
                .iter()
                .any(|c| existing.iter().any(|e| e == c) || installing.iter().any(|i| i == c))
        });
        if some_dep_not_installed {
            let msg = match check_install_deps(package_id, &running) {
                Err(e) => e.to_string(),
                Ok(()) => format!("{package_id} dependencies are not running"),
            };
            return Err(anyhow::Error::new(DependencyGateError(msg)));
        }

        // A dependency still mid-install (no container yet) gets its own,
        // much larger budget: a fresh-node image pull can take far longer
        // than the started-but-not-running window.
        let some_dep_installing = missing.iter().any(|dep| {
            dep.containers
                .iter()
                .any(|c| installing.iter().any(|i| i == c))
        });

        let labels = join_dep_labels(&missing);
        if some_dep_installing {
            if installing_attempts >= DEP_INSTALLING_WAIT_MAX_ATTEMPTS {
                return Err(anyhow::Error::new(DependencyGateError(format!(
                    "{labels} is still installing after {} seconds. Wait for it \
                     to finish, then install {package_id} again.",
                    u64::from(DEP_INSTALLING_WAIT_MAX_ATTEMPTS) * interval.as_secs()
                ))));
            }
            installing_attempts += 1;
            if installing_attempts == 1 {
                info!(
                    "Install {package_id}: dependency {labels} is still installing — \
                     waiting up to {}s for it to finish",
                    u64::from(DEP_INSTALLING_WAIT_MAX_ATTEMPTS) * interval.as_secs()
                );
            }
            on_waiting(format!("Waiting for {labels} to finish installing…")).await;
            tokio::time::sleep(interval).await;
            continue;
        }

        if waited_attempts >= max_attempts {
            return Err(anyhow::Error::new(DependencyGateError(format!(
                "{labels} is installed but did not reach the running state within \
                 {} seconds. Start {labels}, then install {package_id} again.",
                u64::from(max_attempts) * interval.as_secs()
            ))));
        }
        waited_attempts += 1;

        if waited_attempts == 1 {
            info!(
                "Install {package_id}: dependency {labels} installed but not running yet — \
                 waiting up to {}s for it to start",
                u64::from(max_attempts) * interval.as_secs()
            );
        }
        on_waiting(format!("Waiting for {labels} to start…")).await;
        tokio::time::sleep(interval).await;
    }
}

/// ElectrumX and Mempool's Electrum backend need historical blocks from an
/// unpruned node while building their indexes. A pruned Bitcoin node can be
/// running and RPC-reachable but still leave them stuck with closed ports.
/// How long to let Bitcoin finish starting before giving up on the pruning
/// pre-check. Matches `wait_for_bitcoin_rpc_gate`'s 180s so the two waits in
/// one install path agree about how patient "Bitcoin is starting" deserves to
/// be.
const BITCOIN_WARMUP_BUDGET: std::time::Duration = std::time::Duration::from_secs(180);

/// Is this JSON-RPC error bitcoind saying "not ready yet" rather than "broken"?
///
/// `-28` is RPC_IN_WARMUP: "Loading block index…", "Verifying blocks…",
/// "Rewinding blocks…". It is the normal path on every start, not a fault, and
/// it is the only error class worth waiting out — anything else (bad auth,
/// method not found) will not fix itself by retrying. Matched on the code, with
/// the message text as a fallback for any proxy that rewrites the envelope.
fn bitcoin_is_warming_up(error: &serde_json::Value) -> bool {
    if error.get("code").and_then(serde_json::Value::as_i64) == Some(-28) {
        return true;
    }
    error
        .get("message")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|m| {
            let m = m.to_ascii_lowercase();
            m.contains("loading block index")
                || m.contains("verifying blocks")
                || m.contains("rewinding blocks")
                || m.contains("loading wallet")
                || m.contains("starting network threads")
        })
}

/// Say once, in the install log, that we are waiting on Bitcoin — so a slow
/// install reads as "waiting for Bitcoin" instead of a stall, without one line
/// every two seconds.
async fn announce_warmup(announced: &mut bool, package_id: &str, error: &serde_json::Value) {
    if *announced {
        return;
    }
    *announced = true;
    let detail = error
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("starting up");
    super::install::install_log(&format!(
        "INSTALL WAIT: {package_id} — Bitcoin is still starting ({detail}); waiting up to {}s before the pruning check",
        BITCOIN_WARMUP_BUDGET.as_secs()
    ))
    .await;
}

pub(super) async fn check_bitcoin_pruning_compatibility(package_id: &str) -> Result<()> {
    if !requires_unpruned_bitcoin(package_id) {
        return Ok(());
    }

    let (rpc_user, rpc_pass) = crate::bitcoin_rpc::bitcoin_rpc_credentials().await;
    let body = serde_json::json!({
        "jsonrpc": "1.0",
        "id": "package-install-prune-check",
        "method": "getblockchaininfo",
        "params": [],
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("building Bitcoin RPC client")?;

    // Bitcoin's warm-up is not a failure, and this check used to treat it as
    // one. `for _ in 0..3` with a 2s sleep gave the node about six seconds to
    // answer; bitcoind replies `-28` ("Loading block index…", "Verifying
    // blocks…") for MINUTES on a large chainstate. So pressing install on any
    // app that needs unpruned Bitcoin, in the ordinary window after Bitcoin
    // starts, failed with a raw JSON-RPC error — reproduced on archi-dev-box
    // 2026-08-08 installing ElectrumX right after Bitcoin Knots came up, and
    // the likely mechanism behind "fedimint gateway disappeared at 88%
    // install".
    //
    // The same install path already knows better: `wait_for_bitcoin_rpc_gate`
    // waits up to 180s precisely because getblockchaininfo answers during
    // sync. This check runs earlier and now shares that budget, so one concern
    // is not handled two contradictory ways in one install.
    //
    // Only NOT-READY is waited out. A genuine fault — bad auth, connection
    // refused, garbage response — still ends the loop on its own terms below.
    let deadline = tokio::time::Instant::now() + BITCOIN_WARMUP_BUDGET;
    let mut last_error = None;
    let mut announced_warmup = false;
    loop {
        match client
            .post(crate::constants::BITCOIN_RPC_URL)
            .basic_auth(&rpc_user, Some(&rpc_pass))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                match resp.json::<serde_json::Value>().await {
                    Ok(json) if status.is_success() => {
                        if let Some(error) = json.get("error").filter(|e| !e.is_null()) {
                            if bitcoin_is_warming_up(error) {
                                announce_warmup(&mut announced_warmup, package_id, error).await;
                            } else {
                                last_error = Some(format!(
                                    "Bitcoin RPC error while checking pruning status: {error}"
                                ));
                            }
                        } else {
                            return check_blockchain_info_for_pruning(package_id, &json);
                        }
                    }
                    // bitcoind answers -28 with an HTTP 500, so warm-up lands
                    // here rather than in the success arm above.
                    Ok(json) => {
                        let rpc_error = json.get("error").filter(|e| !e.is_null());
                        if rpc_error.is_some_and(bitcoin_is_warming_up) {
                            announce_warmup(
                                &mut announced_warmup,
                                package_id,
                                rpc_error.unwrap_or(&serde_json::Value::Null),
                            )
                            .await;
                        } else {
                            last_error = Some(format!(
                                "Bitcoin RPC returned {status} while checking pruning status: {json}"
                            ));
                        }
                    }
                    Err(e) => {
                        last_error = Some(format!("decode Bitcoin RPC response: {e}"));
                    }
                }
            }
            Err(e) => {
                last_error = Some(format!("checking Bitcoin pruning status: {e}"));
            }
        }
        // A real fault ends the wait immediately — only NOT-READY loops.
        if last_error.is_some() {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            last_error = Some(format!(
                "Bitcoin was still starting up after {}s while checking pruning status",
                BITCOIN_WARMUP_BUDGET.as_secs()
            ));
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    if detect_disk_gb().await < ARCHIVAL_BITCOIN_DISK_GB {
        anyhow::bail!(archival_bitcoin_required_message(package_id));
    }

    // Say what the operator can act on. The old message pasted the raw
    // JSON-RPC envelope, so a node that was merely still starting reported
    // `{"error":{"code":-28,"message":"Verifying blocks…"}}` — accurate and
    // useless to anyone deciding what to do next.
    let detail = last_error.unwrap_or_else(|| "unknown error".to_string());
    if announced_warmup {
        anyhow::bail!(
            "Bitcoin is still starting up, so {package_id} can't be installed yet. \
             This can take several minutes on a large chain. Wait until Bitcoin \
             reports it's synced, then try again. (Details: {detail})"
        );
    }
    anyhow::bail!("Bitcoin RPC unavailable while checking pruning status: {detail}");
}

fn check_blockchain_info_for_pruning(package_id: &str, json: &serde_json::Value) -> Result<()> {
    let Some(result) = json.get("result") else {
        anyhow::bail!("Bitcoin RPC response missing result while checking pruning status");
    };
    if result
        .get("pruned")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        anyhow::bail!(archival_bitcoin_required_message(package_id));
    }

    Ok(())
}

async fn detect_disk_gb() -> u64 {
    let output = tokio::process::Command::new("df")
        .args(["-BG", "/var/lib/archipelago"])
        .output()
        .await;
    let Ok(output) = output else {
        return u64::MAX;
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .nth(1)
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|size| size.trim_end_matches('G').parse::<u64>().ok())
        .unwrap_or(u64::MAX)
}

/// Smallest disk (GB, total) a cuprate node can live on. The value and its
/// rationale live in ONE place — `crate::constants::CUPRATE_MIN_DISK_GB` —
/// shared with the boot reconciler so install/start and boot can never
/// disagree about where cuprate may run.
use crate::constants::CUPRATE_MIN_DISK_GB;

/// The bitcoin apps pick `-prune` automatically when disk is scarce, because
/// bitcoind supports pruning. Cuprate CANNOT: upstream has no pruning config
/// at all (the `pruning` crate in its workspace is Monero's p2p *protocol*
/// pruning, not on-disk pruning), so the disk-scarce equivalent is to refuse
/// to run cuprate at all rather than let it sync until the filesystem fills —
/// which took Archipelago itself down on nodes with too little disk.
fn cuprate_insufficient_disk_message(disk_gb: u64) -> String {
    format!(
        "Cuprate needs a disk of at least {} GB and this node has {} GB. \
         A Monero node cannot run pruned — upstream cuprate has no pruning \
         support — so the chain (~250 GB and growing) would fill the disk and \
         take Archipelago down with it. Attach a larger disk (or move \
         /var/lib/archipelago to one) and try again. Bitcoin apps CAN run \
         pruned on smaller disks; Monero currently cannot.",
        CUPRATE_MIN_DISK_GB, disk_gb
    )
}

/// Pure decision half of the cuprate disk gate — testable without df.
pub(super) fn cuprate_disk_gate(disk_gb: u64) -> Option<String> {
    (disk_gb < CUPRATE_MIN_DISK_GB).then(|| cuprate_insufficient_disk_message(disk_gb))
}

/// Install/start-time pre-check: refuse cuprate on disks too small to hold
/// the Monero chain. Mirrors `check_bitcoin_pruning_compatibility`'s
/// fail-open-on-unknown-disk behaviour (`detect_disk_gb` returns u64::MAX
/// when df fails, so an unreadable disk never blocks an install).
pub(super) async fn check_cuprate_disk_compatibility(package_id: &str) -> Result<()> {
    if package_id != "cuprate" {
        return Ok(());
    }
    let disk_gb = detect_disk_gb().await;
    if let Some(message) = cuprate_disk_gate(disk_gb) {
        anyhow::bail!(message);
    }
    Ok(())
}

/// Log informational messages about optional dependencies.
pub(super) fn log_optional_dep_info(package_id: &str, deps: &RunningDeps) {
    if matches!(package_id, "btcpay-server" | "btcpayserver") && !deps.has_lnd {
        tracing::info!(
            "BTCPay Server installing without LND \
             — Lightning payments won't be available until LND is installed"
        );
    }
}

/// Whether an app requires the shared `archy-net` Podman network for
/// inter-container DNS resolution.
pub(super) fn needs_archy_net(package_id: &str) -> bool {
    matches!(
        package_id,
        "bitcoin-knots"
            | "bitcoin"
            | "bitcoin-core"
            | "lnd"
            | "mempool"
            | "mempool-web"
            | "mempool-api"
            | "electrumx"
            | "mempool-electrs"
            | "electrs"
            | "mysql-mempool"
            | "archy-mempool-db"
            | "archy-mempool-web"
            | "btcpay-server"
            | "btcpayserver"
            | "archy-btcpay-db"
            | "archy-nbxplorer"
            | "nbxplorer"
            | "fedimint"
            | "fedimint-gateway"
            | "pine"
            | "pine-whisper"
            | "pine-piper"
            | "pine-openwakeword"
    )
}

/// Return the correct startup order for a multi-container app stack.
/// Containers are started in this order to satisfy dependency chains.
pub(super) fn startup_order(package_id: &str) -> &'static [&'static str] {
    match package_id {
        "mempool" | "mempool-web" => &[
            "archy-mempool-db",
            "mysql-mempool",
            "electrumx",
            "mempool-electrs",
            "mempool-api",
            "archy-mempool-api",
            "archy-mempool-web",
            "mempool",
        ],
        "immich" => &["immich_postgres", "immich_redis", "immich_server"],
        "indeedhub" => &[
            "indeedhub-postgres",
            "indeedhub-redis",
            "indeedhub-minio",
            "indeedhub-relay",
            "indeedhub-api",
            "indeedhub-ffmpeg",
            "indeedhub",
        ],
        "btcpay-server" | "btcpayserver" | "btcpay" => {
            &["archy-btcpay-db", "archy-nbxplorer", "btcpay-server"]
        }
        "netbird" => &["netbird-server", "netbird-dashboard", "netbird"],
        "pine" => &["pine-whisper", "pine-piper", "pine-openwakeword", "pine"],
        "penpot" | "penpot-frontend" => &[
            "penpot-postgres",
            "penpot-valkey",
            "penpot-backend",
            "penpot-exporter",
            "penpot-frontend",
        ],
        _ => &[],
    }
}

/// Sort a list of container names according to the dependency-aware startup
/// order for the given app. Unknown containers sort to the end.
pub(super) async fn ordered_containers_for_start(package_id: &str) -> Result<Vec<String>> {
    let containers = get_containers_for_app(package_id).await?;
    Ok(order_present_containers(package_id, containers))
}

/// Order the *actually-present* containers of an app by its dependency-aware
/// startup order. Containers whose name is unknown to the order list sort to
/// the end, preserving their relative input order.
///
/// This deliberately does NOT inject order entries that aren't live
/// containers. `startup_order` is a union of container-name variants across
/// install generations (e.g. `mysql-mempool` vs `archy-mempool-db`), so any
/// single install only ever has a subset of those names. Injecting a phantom
/// name makes the start path fail on a "no such object" inspect — and because
/// `do_orchestrator_package_start` propagates the unknown-app-id fallback
/// error via `?`, every later member (the api + frontend) is then skipped,
/// leaving the stack down until the health monitor recovers it minutes later.
/// That was the source of mempool gate flakes #73 (frontend) / #74 (api).
/// Orchestrator APP ids for a multi-container stack, in dependency-start
/// order. Unlike `startup_order` (a union of CONTAINER-name variants across
/// install generations, sometimes including foreign dependencies), every
/// entry here is a real manifest app id the orchestrator can `start()` to
/// recreate the member from scratch.
///
/// Used when a stack has NO live containers: under quadlet, `package.stop`
/// removes every member container, so a subsequent `package.start` must
/// resurrect member-by-member. Falling back to just the package id left the
/// other members absent AND user-stopped (.228 indeedhub, 2026-07-09 — the
/// reconciler then correctly refused to revive them).
pub(super) fn stack_member_app_ids(package_id: &str) -> &'static [&'static str] {
    // Canonical table moved to crate::app_ops (shared with the reconciler's
    // in-flight-op guard).
    crate::app_ops::stack_member_app_ids(package_id)
}

fn order_present_containers(package_id: &str, containers: Vec<String>) -> Vec<String> {
    if containers.is_empty() {
        // Nothing is live under any known name. For known stacks, resurrect
        // every member via its app id (see stack_member_app_ids). Otherwise
        // fall back to the package id so a single-container app whose
        // container matches its id still gets one start attempt.
        let members = stack_member_app_ids(package_id);
        if !members.is_empty() {
            return members.iter().map(|s| s.to_string()).collect();
        }
        return vec![package_id.to_string()];
    }
    let order = startup_order(package_id);
    // If no special order is defined, fall back to mempool order for legacy
    // multi-container names that may still be returned by config lookups.
    let effective_order: &[&str] = if order.is_empty() {
        startup_order("mempool")
    } else {
        order
    };
    let mut sorted = containers;
    sorted.sort_by_key(|c| {
        effective_order
            .iter()
            .position(|o| *o == c)
            .unwrap_or(usize::MAX)
    });
    sorted
}

/// Configure Fedimint Gateway to use LND instead of LDK.
/// Modifies ports, volumes, and command args in place when LND credentials exist.
///
/// `fedi_hash` is the already-resolved per-install gateway credential
/// (`container::secrets::gateway_bcrypt_hash`) — this function does not read
/// the secrets file itself, so there is exactly one read site and one
/// failure point for that credential (FED-07).
pub(super) fn configure_fedimint_lnd(
    host_ip: &str,
    ports: &mut Vec<String>,
    volumes: &mut Vec<String>,
    custom_args: &mut Option<Vec<String>>,
    rpc_user: &str,
    rpc_pass: &str,
    fedi_hash: &str,
) {
    let lnd_cert = "/var/lib/archipelago/lnd/tls.cert";
    let lnd_macaroon = "/var/lib/archipelago/lnd/data/chain/bitcoin/mainnet/admin.macaroon";
    if std::path::Path::new(lnd_cert).exists() && std::path::Path::new(lnd_macaroon).exists() {
        info!("LND detected with credentials — configuring gateway in lnd mode");

        ports.retain(|p| p != "9737:9737");
        volumes.push(format!("{}:/lnd/tls.cert:ro", lnd_cert));
        volumes.push(format!("{}:/lnd/admin.macaroon:ro", lnd_macaroon));
        *custom_args = Some(vec![
            "gatewayd".to_string(),
            "--data-dir".to_string(),
            "/data".to_string(),
            "--listen".to_string(),
            "0.0.0.0:8176".to_string(),
            "--bcrypt-password-hash".to_string(),
            fedi_hash.to_string(),
            "--network".to_string(),
            "bitcoin".to_string(),
            "--bitcoind-url".to_string(),
            format!("http://{}:8332", host_ip),
            "--bitcoind-username".to_string(),
            rpc_user.to_string(),
            "--bitcoind-password".to_string(),
            rpc_pass.to_string(),
            "lnd".to_string(),
            "--lnd-rpc-host".to_string(),
            format!("{}:10009", host_ip),
            "--lnd-tls-cert".to_string(),
            "/lnd/tls.cert".to_string(),
            "--lnd-macaroon".to_string(),
            "/lnd/admin.macaroon".to_string(),
        ]);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bitcoin_is_warming_up, cuprate_disk_gate, dependency_list_declares_archival_bitcoin,
        manifest_declares_archival_bitcoin, order_present_containers, requires_unpruned_bitcoin,
        startup_order, BITCOIN_WARMUP_BUDGET, CUPRATE_MIN_DISK_GB,
    };
    use archipelago_container::Dependency;

    #[test]
    fn order_present_containers_never_injects_phantom_stack_members() {
        // The live mempool stack on a node: db + api + frontend. These are the
        // only real container names; the startup_order list also contains
        // variant/legacy names (mysql-mempool, archy-mempool-api, ...) that are
        // NOT live here and must never appear in the result — a phantom name in
        // the start list aborts the orchestrator start mid-sequence (gate
        // #73/#74).
        let present = vec![
            "mempool".to_string(),
            "mempool-api".to_string(),
            "archy-mempool-db".to_string(),
        ];
        let ordered = order_present_containers("mempool", present);
        // Dependency order: db -> api -> frontend.
        assert_eq!(ordered, vec!["archy-mempool-db", "mempool-api", "mempool"]);
        // No phantom variants leaked in.
        for phantom in ["mysql-mempool", "archy-mempool-api", "archy-mempool-web"] {
            assert!(
                !ordered.iter().any(|c| c == phantom),
                "phantom {phantom} must not be injected"
            );
        }
    }

    #[test]
    fn order_present_containers_orders_known_before_unknown() {
        let present = vec!["mempool".to_string(), "some-sidecar".to_string()];
        let ordered = order_present_containers("mempool", present);
        // The known frontend sorts ahead of an unknown sidecar.
        assert_eq!(ordered, vec!["mempool", "some-sidecar"]);
    }

    #[test]
    fn order_present_containers_empty_resurrects_stack_members() {
        // Under quadlet, package.stop removes every member container; the
        // start fallback must name each member APP id so the orchestrator can
        // recreate the whole stack (.228 indeedhub, 2026-07-09) — not just
        // the umbrella/package id.
        assert_eq!(
            order_present_containers("mempool", vec![]),
            vec!["archy-mempool-db", "mempool-api", "archy-mempool-web"]
        );
        assert_eq!(
            order_present_containers("indeedhub", vec![]),
            vec![
                "indeedhub-postgres",
                "indeedhub-redis",
                "indeedhub-minio",
                "indeedhub-relay",
                "indeedhub-api",
                "indeedhub-ffmpeg",
                "indeedhub",
            ]
        );
        assert_eq!(
            order_present_containers("immich", vec![]),
            vec!["immich-postgres", "immich-redis", "immich"]
        );
        // Single-container apps keep the package-id fallback.
        assert_eq!(
            order_present_containers("vaultwarden", vec![]),
            vec!["vaultwarden".to_string()]
        );
    }

    #[test]
    fn btcpay_start_order_includes_required_stack_members() {
        assert_eq!(
            startup_order("btcpay-server"),
            &["archy-btcpay-db", "archy-nbxplorer", "btcpay-server"]
        );
    }

    #[test]
    fn netbird_start_order_starts_server_before_dashboard() {
        assert_eq!(
            startup_order("netbird"),
            &["netbird-server", "netbird-dashboard", "netbird"]
        );
    }

    #[test]
    fn unpruned_bitcoin_required_for_electrum_indexers_and_mempool() {
        for package_id in [
            "electrumx",
            "mempool-electrs",
            "electrs",
            "mempool",
            "mempool-web",
        ] {
            assert!(requires_unpruned_bitcoin(package_id), "{package_id}");
        }
        for package_id in ["bitcoin-knots", "btcpay-server", "lnd", "fedimint"] {
            assert!(!requires_unpruned_bitcoin(package_id), "{package_id}");
        }
    }

    #[test]
    fn dependency_matcher_finds_the_archival_marker_among_other_deps() {
        let deps = vec![
            Dependency::App {
                app_id: "bitcoin-knots".to_string(),
                version: Some(">=26.0".to_string()),
            },
            Dependency::Storage {
                storage: "50Gi".to_string(),
            },
            Dependency::Simple("bitcoin:archival".to_string()),
        ];
        assert!(dependency_list_declares_archival_bitcoin(&deps));
    }

    #[test]
    fn dependency_matcher_false_when_marker_absent() {
        let deps = vec![Dependency::App {
            app_id: "bitcoin-knots".to_string(),
            version: Some(">=26.0".to_string()),
        }];
        assert!(!dependency_list_declares_archival_bitcoin(&deps));
        assert!(!dependency_list_declares_archival_bitcoin(&[]));
    }

    #[test]
    fn manifest_declared_archival_bitcoin_covers_a_new_app_without_a_code_change() {
        // electrumx and mempool declare `dependencies: [..., bitcoin:archival]`
        // on disk (apps/electrumx/manifest.yml, apps/mempool/manifest.yml) —
        // this is the manifest-driven path working end-to-end, not the
        // hardcoded id list. A future app only needs this manifest line, no
        // edit to `requires_unpruned_bitcoin`.
        assert!(manifest_declares_archival_bitcoin("electrumx"));
        assert!(manifest_declares_archival_bitcoin("mempool"));
        let angor = archipelago_container::AppManifest::parse(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../apps/angor-indexer/manifest.yml"
        )))
        .unwrap();
        assert!(dependency_list_declares_archival_bitcoin(
            &angor.app.dependencies
        ));
        // An app whose manifest exists but never declares the marker.
        assert!(!manifest_declares_archival_bitcoin("bitcoin-knots"));
        // An id with no manifest on disk at all.
        assert!(!manifest_declares_archival_bitcoin("does-not-exist"));
    }

    #[test]
    fn cuprate_disk_gate_refuses_disks_too_small_for_the_monero_chain() {
        // 250 GB VPS class: the ~250 GiB chain does not fit, full stop.
        assert!(cuprate_disk_gate(0).is_some());
        assert!(cuprate_disk_gate(250).is_some());
        assert!(cuprate_disk_gate(CUPRATE_MIN_DISK_GB - 1).is_some());
        assert!(cuprate_disk_gate(CUPRATE_MIN_DISK_GB).is_none());
        assert!(cuprate_disk_gate(1000).is_none());
        // df failure reads as u64::MAX — an unreadable disk must not block.
        assert!(cuprate_disk_gate(u64::MAX).is_none());
    }

    #[test]
    fn cuprate_disk_gate_message_names_the_fix_not_just_the_problem() {
        let msg = cuprate_disk_gate(250).expect("250 GB must be refused");
        assert!(msg.contains("cannot run pruned"), "{msg}");
        assert!(msg.contains("larger disk"), "{msg}");
        assert!(msg.contains("250 GB"), "{msg}");
    }

    #[tokio::test]
    async fn cuprate_disk_gate_only_applies_to_cuprate() {
        // Every other package passes regardless of disk — including the
        // bitcoin apps, which self-prune via their manifest entrypoint.
        for package_id in ["bitcoin-knots", "bitcoin-core", "electrumx", "mempool"] {
            super::check_cuprate_disk_compatibility(package_id)
                .await
                .expect("non-cuprate installs must not be gated here");
        }
    }

    mod dep_wait {
        use super::super::{wait_for_install_deps, DepProbe, DependencyGateError, RunningDeps};
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        fn deps(has_bitcoin: bool, has_electrumx: bool) -> RunningDeps {
            RunningDeps {
                has_bitcoin,
                has_electrumx,
                has_lnd: false,
            }
        }

        fn probe(has_bitcoin: bool, has_electrumx: bool, existing: &[&str]) -> DepProbe {
            probe_with_installing(has_bitcoin, has_electrumx, existing, &[])
        }

        fn probe_with_installing(
            has_bitcoin: bool,
            has_electrumx: bool,
            existing: &[&str],
            installing: &[&str],
        ) -> DepProbe {
            DepProbe {
                running: deps(has_bitcoin, has_electrumx),
                existing: existing.iter().map(|s| s.to_string()).collect(),
                installing: installing.iter().map(|s| s.to_string()).collect(),
            }
        }

        /// Collects "Waiting for X to start…" labels emitted during the wait.
        fn label_sink() -> (
            Arc<Mutex<Vec<String>>>,
            impl FnMut(String) -> std::future::Ready<()>,
        ) {
            let labels = Arc::new(Mutex::new(Vec::new()));
            let sink = {
                let labels = Arc::clone(&labels);
                move |msg: String| {
                    labels.lock().unwrap().push(msg);
                    std::future::ready(())
                }
            };
            (labels, sink)
        }

        #[tokio::test]
        async fn passes_immediately_when_dependency_is_running() {
            let (labels, sink) = label_sink();
            let result = wait_for_install_deps(
                "lnd",
                || async { Ok(probe(true, false, &["bitcoin-knots"])) },
                sink,
                3,
                Duration::ZERO,
            )
            .await;
            assert!(result.is_ok());
            assert!(labels.lock().unwrap().is_empty(), "no waiting expected");
        }

        #[tokio::test]
        async fn fails_fast_when_dependency_not_installed_at_all() {
            let calls = AtomicU32::new(0);
            let (labels, sink) = label_sink();
            let err = wait_for_install_deps(
                "lnd",
                || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async { Ok(probe(false, false, &["uptime-kuma"])) }
                },
                sink,
                36,
                Duration::ZERO,
            )
            .await
            .unwrap_err();
            // Single probe — no polling when waiting cannot help.
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert!(labels.lock().unwrap().is_empty());
            // Canonical check_install_deps message, wrapped in the gate marker
            // so async_lifecycle removes the optimistic Installing entry.
            assert!(err.downcast_ref::<DependencyGateError>().is_some());
            assert!(
                err.to_string()
                    .contains("LND requires a running Bitcoin node"),
                "unexpected message: {err}"
            );
        }

        #[tokio::test]
        async fn waits_while_installed_dependency_starts_then_passes() {
            // Bitcoin container exists (installing/starting) but only reports
            // Running from the 3rd probe onward — the log-confirmed LND race.
            let calls = Arc::new(AtomicU32::new(0));
            let (labels, sink) = label_sink();
            let probe_calls = Arc::clone(&calls);
            let result = wait_for_install_deps(
                "lnd",
                move || {
                    let n = probe_calls.fetch_add(1, Ordering::SeqCst);
                    async move { Ok(probe(n >= 2, false, &["bitcoin-knots"])) }
                },
                sink,
                36,
                Duration::ZERO,
            )
            .await;
            assert!(result.is_ok(), "{result:?}");
            assert_eq!(calls.load(Ordering::SeqCst), 3);
            let labels = labels.lock().unwrap();
            assert_eq!(labels.len(), 2, "one waiting label per polling attempt");
            assert!(labels.iter().all(|l| l == "Waiting for Bitcoin to start…"));
        }

        #[tokio::test]
        async fn waits_while_dependency_is_still_installing_then_passes() {
            // Bitcoin has NO container yet (image still pulling) but its
            // package state is Installing. Old behavior failed fast here —
            // the confirmed fresh-node LND "first install fails" bug. The
            // gate must wait past the normal started-but-not-running budget
            // (max_attempts=1 below) while the install is in flight.
            let calls = Arc::new(AtomicU32::new(0));
            let (labels, sink) = label_sink();
            let probe_calls = Arc::clone(&calls);
            let result = wait_for_install_deps(
                "lnd",
                move || {
                    let n = probe_calls.fetch_add(1, Ordering::SeqCst);
                    async move {
                        Ok(match n {
                            // pulling: no container, package Installing
                            0 | 1 => probe_with_installing(false, false, &[], &["bitcoin-knots"]),
                            // container created, starting
                            2 => probe(false, false, &["bitcoin-knots"]),
                            // running
                            _ => probe(true, false, &["bitcoin-knots"]),
                        })
                    }
                },
                sink,
                1,
                Duration::ZERO,
            )
            .await;
            assert!(result.is_ok(), "{result:?}");
            assert_eq!(calls.load(Ordering::SeqCst), 4);
            let labels = labels.lock().unwrap();
            assert_eq!(
                labels.as_slice(),
                [
                    "Waiting for Bitcoin to finish installing…",
                    "Waiting for Bitcoin to finish installing…",
                    "Waiting for Bitcoin to start…",
                ]
            );
        }

        #[tokio::test]
        async fn fails_fast_when_dependency_neither_installed_nor_installing() {
            // installing list has unrelated packages only — no reason to wait.
            let calls = AtomicU32::new(0);
            let (labels, sink) = label_sink();
            let err = wait_for_install_deps(
                "lnd",
                || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async { Ok(probe_with_installing(false, false, &[], &["grafana"])) }
                },
                sink,
                36,
                Duration::ZERO,
            )
            .await
            .unwrap_err();
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert!(labels.lock().unwrap().is_empty());
            assert!(err.downcast_ref::<DependencyGateError>().is_some());
        }

        #[tokio::test]
        async fn times_out_when_installed_dependency_never_runs() {
            let (labels, sink) = label_sink();
            let err = wait_for_install_deps(
                "lnd",
                || async { Ok(probe(false, false, &["bitcoin-knots"])) },
                sink,
                4,
                Duration::ZERO,
            )
            .await
            .unwrap_err();
            assert!(err.downcast_ref::<DependencyGateError>().is_some());
            assert!(
                err.to_string()
                    .contains("did not reach the running state within 0 seconds"),
                "unexpected message: {err}"
            );
            assert_eq!(labels.lock().unwrap().len(), 4);
        }

        #[tokio::test]
        async fn mempool_waits_on_bitcoin_only() {
            // ElectrumX is deliberately NOT gated (2026-07-22): a resyncing
            // ElectrumX can be down for days and mempool-api reconnects on
            // its own, so the wait only covers Bitcoin — even when electrumx
            // is installed and stopped.
            let calls = Arc::new(AtomicU32::new(0));
            let (labels, sink) = label_sink();
            let probe_calls = Arc::clone(&calls);
            let result = wait_for_install_deps(
                "mempool",
                move || {
                    let n = probe_calls.fetch_add(1, Ordering::SeqCst);
                    // Bitcoin comes up on probe 2; electrumx never does.
                    async move { Ok(probe(n >= 1, false, &["bitcoin-knots", "electrumx"])) }
                },
                sink,
                36,
                Duration::ZERO,
            )
            .await;
            assert!(result.is_ok(), "{result:?}");
            let labels = labels.lock().unwrap();
            assert_eq!(
                labels.as_slice(),
                &["Waiting for Bitcoin to start…".to_string()]
            );
        }

        #[tokio::test]
        async fn mempool_fails_fast_when_bitcoin_is_not_installed() {
            // Bitcoin isn't installed at all — waiting can never satisfy the
            // gate, so fail fast with the canonical message. (A missing
            // ElectrumX no longer blocks anything.)
            let (labels, sink) = label_sink();
            let err = wait_for_install_deps(
                "mempool",
                || async { Ok(probe(false, false, &["electrumx"])) },
                sink,
                36,
                Duration::ZERO,
            )
            .await
            .unwrap_err();
            assert!(err.downcast_ref::<DependencyGateError>().is_some());
            assert!(labels.lock().unwrap().is_empty());
            assert!(
                err.to_string().contains("Mempool requires"),
                "unexpected message: {err}"
            );
        }

        #[tokio::test]
        async fn variant_container_names_count_as_installed() {
            // bitcoin-core (not just bitcoin-knots) satisfies the "installed"
            // check for the wait path.
            let calls = Arc::new(AtomicU32::new(0));
            let (_labels, sink) = label_sink();
            let probe_calls = Arc::clone(&calls);
            let result = wait_for_install_deps(
                "electrumx",
                move || {
                    let n = probe_calls.fetch_add(1, Ordering::SeqCst);
                    async move { Ok(probe(n >= 1, false, &["bitcoin-core"])) }
                },
                sink,
                36,
                Duration::ZERO,
            )
            .await;
            assert!(result.is_ok(), "{result:?}");
        }

        #[tokio::test]
        async fn apps_without_dependency_gate_pass_untouched() {
            let (labels, sink) = label_sink();
            let result = wait_for_install_deps(
                "uptime-kuma",
                || async { Ok(probe(false, false, &[])) },
                sink,
                36,
                Duration::ZERO,
            )
            .await;
            assert!(result.is_ok());
            assert!(labels.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn mempool_api_is_directly_installable_and_covered_by_the_archival_gate() {
        // `mempool-api` is a legitimate direct `package.install` target
        // (`uses_orchestrator_install_flow` in install.rs), reachable without
        // going through the `mempool`/`mempool-web` umbrella id that the old
        // hardcoded fallback list only recognized. It was missing from that
        // list, so installing/repairing it directly skipped the archival
        // Bitcoin gate entirely. Its manifest now declares `bitcoin:archival`
        // directly, closing the gap the manifest-driven path exists for.
        assert!(requires_unpruned_bitcoin("mempool-api"));
        assert!(manifest_declares_archival_bitcoin("mempool-api"));
        // `archy-mempool-web` has no direct Bitcoin RPC access
        // (bitcoin_integration.rpc_access: none) and correctly stays excluded.
        assert!(!requires_unpruned_bitcoin("archy-mempool-web"));
    }

    /// The distinction the pruning pre-check now turns on: bitcoind saying
    /// "not ready yet" must be waited out, everything else must fail fast.
    /// Getting this wrong in either direction is a real bug — too strict and
    /// installs fail during every startup window (the ElectrumX failure on
    /// archi-dev-box, 2026-08-08); too loose and a genuinely broken RPC hangs
    /// the install for the full budget.
    #[test]
    fn warmup_is_recognised_by_code_and_by_message() {
        let by_code = serde_json::json!({"code": -28, "message": "Verifying blocks…"});
        assert!(bitcoin_is_warming_up(&by_code));

        // The exact payload that failed the ElectrumX install.
        let observed = serde_json::json!({"code": -28, "message": "Verifying blocks…"});
        assert!(bitcoin_is_warming_up(&observed));

        // Message fallback, for a proxy that rewrites the envelope and drops
        // the code.
        for msg in [
            "Loading block index...",
            "Verifying blocks…",
            "Rewinding blocks...",
            "LOADING BLOCK INDEX",
        ] {
            let e = serde_json::json!({ "message": msg });
            assert!(bitcoin_is_warming_up(&e), "should be warm-up: {msg}");
        }
    }

    #[test]
    fn real_faults_are_not_mistaken_for_warmup() {
        // These never fix themselves by waiting, so they must end the loop
        // immediately rather than burn the full 180s budget.
        for e in [
            serde_json::json!({"code": -32601, "message": "Method not found"}),
            serde_json::json!({"code": -1, "message": "unauthorized"}),
            serde_json::json!({"message": "Work queue depth exceeded"}),
            serde_json::json!({}),
            serde_json::Value::Null,
        ] {
            assert!(!bitcoin_is_warming_up(&e), "should NOT be warm-up: {e}");
        }
    }

    #[test]
    fn the_warmup_budget_matches_the_other_bitcoin_wait_in_this_install_path() {
        // install.rs's wait_for_bitcoin_rpc_gate waits 180s for the same
        // condition. Two different answers to "how long is Bitcoin allowed to
        // be starting?" in one install is how this bug happened.
        assert_eq!(BITCOIN_WARMUP_BUDGET.as_secs(), 180);
    }
}
