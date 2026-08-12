//! Async lifecycle helper for container Stop/Start/Restart RPCs.
//!
//! The `ContainerOrchestrator` trait is intentionally synchronous — blocking
//! calls keep the reconciler, boot flow, chaos harness, and unit tests
//! deterministic. But the RPC layer must return to the UI in <1s so the
//! dashboard can render a transitional "Stopping…" / "Starting…" label while
//! the underlying `podman stop` (up to 600s for bitcoin-core) runs in the
//! background.
//!
//! `RpcHandler::spawn_transitional` bridges the two: it
//!   1. flips the package state in `StateManager` to the appropriate
//!      transitional variant (`Stopping` / `Starting` / `Restarting`),
//!      which fans out to WebSocket clients immediately.
//!   2. `tokio::spawn`s the actual orchestrator call.
//!   3. on success, writes the final state (`Stopped` / `Running`).
//!   4. on error, reverts to the pre-transition state and logs via
//!      `install_log()` so the incident shows up in
//!      `/var/log/archipelago/container-installs.log`.
//!
//! The server.rs package-scan loop must also be taught to preserve
//! transitional states — see `server.rs:scan_and_update_packages`'s merge
//! logic and the companion `merge_preserving_transitional` helper.

use super::package::install_log;
use super::RpcHandler;
use crate::container::ContainerOrchestrator;
use crate::data_model::PackageState;
use crate::state::StateManager;
use anyhow::Result;
use std::sync::Arc;
use tracing::{error, info, warn};

/// The three transitional lifecycle operations that run asynchronously from
/// the RPC handler. `Install` and `Remove` are intentionally NOT here — they
/// already have their own progress-tracking paths (`install_progress`,
/// `uninstall_stage`) with multi-step UI feedback.
#[derive(Debug, Clone, Copy)]
pub(super) enum Op {
    Stop,
    Start,
    Restart,
}

impl Op {
    /// The `PackageState` to set on the entry while the operation is in
    /// flight. The package-scan merge loop must preserve this variant and
    /// refuse to overwrite it with whatever podman reports (see
    /// `merge_preserving_transitional` in server.rs).
    fn transitional_state(self) -> PackageState {
        match self {
            Op::Stop => PackageState::Stopping,
            Op::Start => PackageState::Starting,
            Op::Restart => PackageState::Restarting,
        }
    }

    /// The `PackageState` to set on success. On error the caller reverts to
    /// the pre-transition state rather than using these.
    fn final_state_on_success(self) -> PackageState {
        match self {
            Op::Stop => PackageState::Stopped,
            Op::Start => PackageState::Running,
            Op::Restart => PackageState::Running,
        }
    }

    /// Prefix used in `install_log` entries so post-mortem readers can grep
    /// the operation that failed.
    fn log_prefix(self) -> &'static str {
        match self {
            Op::Stop => "STOP",
            Op::Start => "START",
            Op::Restart => "RESTART",
        }
    }

    /// Call the orchestrator for this op. Kept in one place so the spawned
    /// task doesn't repeat the match four times.
    async fn dispatch(self, orch: &dyn ContainerOrchestrator, app_id: &str) -> Result<()> {
        match self {
            Op::Stop => orch.stop(app_id).await,
            Op::Start => orch.start(app_id).await,
            Op::Restart => orch.restart(app_id).await,
        }
    }
}

impl RpcHandler {
    /// Flip the package state to `op.transitional_state()`, spawn a background
    /// task that runs `op.dispatch()`, and return immediately. The spawned
    /// task writes the final state on completion or reverts to the
    /// pre-transition state on failure.
    ///
    /// If no package entry exists for `app_id` (e.g. Start on a container
    /// that was never installed), no pre-state is recorded and the spawn
    /// still runs — the post-success path will no-op the state write and
    /// the next scan will pick up the newly-created entry with the correct
    /// state. This keeps the helper usable for stacks that lazily create
    /// their entries.
    pub(super) async fn spawn_transitional(&self, op: Op, app_id: String) -> Result<()> {
        let orchestrator = self
            .orchestrator
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Container orchestrator not available"))?
            .clone();
        let state_manager = Arc::clone(&self.state_manager);

        // Snapshot pre-transition state (for revert on error) and flip to
        // transitional variant. Done BEFORE the spawn so the WebSocket push
        // beats the RPC response — the UI should see "Stopping…" the moment
        // it gets the RPC ok, not on the next scan.
        let pre_state =
            flip_to_transitional(&state_manager, &app_id, op.transitional_state()).await;

        let log_prefix = op.log_prefix();
        let app_id_log = app_id.clone();
        install_log(&format!("{}: {}", log_prefix, app_id_log)).await;

        tokio::spawn(async move {
            match op.dispatch(orchestrator.as_ref(), &app_id).await {
                Ok(()) => {
                    info!("{} complete: {}", log_prefix, app_id);
                    set_state(&state_manager, &app_id, op.final_state_on_success()).await;
                }
                Err(e) => {
                    error!("{} failed for {}: {:#}", log_prefix, app_id, e);
                    install_log(&format!("{} FAIL: {} — {:#}", log_prefix, app_id, e)).await;
                    // Revert to pre-transition state if we had one; otherwise
                    // leave the entry untouched so the next scan reconciles.
                    if let Some(prev) = pre_state {
                        set_state(&state_manager, &app_id, prev).await;
                    } else {
                        warn!(
                            "{}: no pre-transition state recorded for {}; leaving entry to next scan",
                            log_prefix, app_id
                        );
                    }
                }
            }
        });

        Ok(())
    }
}

/// Flip the entry's state to `transitional` and return the previous state.
/// Returns `None` if there is no entry for `app_id`.
async fn flip_to_transitional(
    state_manager: &StateManager,
    app_id: &str,
    transitional: PackageState,
) -> Option<PackageState> {
    let (mut data, _) = state_manager.get_snapshot().await;
    let prev = data.package_data.get(app_id).map(|e| e.state.clone());
    if let Some(entry) = data.package_data.get_mut(app_id) {
        entry.state = transitional;
        state_manager.update_data(data).await;
    }
    prev
}

/// Set the entry's state to `new_state`. No-ops if the entry has since been
/// removed (e.g. uninstall ran concurrently).
async fn set_state(state_manager: &StateManager, app_id: &str, new_state: PackageState) {
    let (mut data, _) = state_manager.get_snapshot().await;
    if let Some(entry) = data.package_data.get_mut(app_id) {
        if entry.state != new_state {
            entry.state = new_state;
            state_manager.update_data(data).await;
        }
    }
}
