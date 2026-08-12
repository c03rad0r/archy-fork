//! BootReconciler — the long-running task that keeps the prod orchestrator's
//! desired-state view in lockstep with what podman actually has.
//!
//! Step 5 of the rust-orchestrator migration. Spawned once from `main.rs`
//! (Step 6) after the initial `adopt_existing()` pass. Every `interval` it
//! calls `ProdContainerOrchestrator::reconcile_existing()`, which repairs
//! containers that already exist without installing every catalog manifest.
//!
//! Per answered design Q3, `interval` defaults to 30 seconds.
//!
//! Shutdown is signalled via `Arc<Notify>`. The reconciler finishes its
//! current `reconcile_all` call before exiting — we don't interrupt an
//! in-flight pull or build.
//!
//! See `docs/rust-orchestrator-migration.md` §269-352.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::{self, Instant};

use crate::container::prod_orchestrator::{ProdContainerOrchestrator, ReconcileReport};

/// Default reconciler cadence (answered design Q3).
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(30);

pub struct BootReconciler {
    orchestrator: Arc<ProdContainerOrchestrator>,
    interval: Duration,
    shutdown: Arc<Notify>,
    /// Run the companion-unit repair stage each tick. Default true.
    /// Tests disable this — companion reconcile shells out to
    /// `systemctl --user` and `podman`, which both block real time
    /// and would race the paused-clock test fixtures.
    companion_stage: bool,
    wait_for_recovery: bool,
}

impl BootReconciler {
    pub fn new(
        orchestrator: Arc<ProdContainerOrchestrator>,
        interval: Duration,
        shutdown: Arc<Notify>,
    ) -> Self {
        Self {
            orchestrator,
            interval,
            shutdown,
            companion_stage: true,
            wait_for_recovery: true,
        }
    }

    /// Disable the companion-unit reconcile stage. Used by unit tests
    /// that exercise loop cadence without the real systemd / podman
    /// surface. Production must not call this.
    #[cfg(test)]
    pub fn without_companion_stage(mut self) -> Self {
        self.companion_stage = false;
        self.wait_for_recovery = false;
        self
    }

    /// Run the reconcile loop until `shutdown` is notified.
    ///
    /// Does one reconcile immediately, then sleeps `interval` between
    /// subsequent passes. A `shutdown.notify_one()` call unblocks the sleep
    /// and the task returns after the *next* pass completes.
    ///
    /// Each pass is two stages:
    /// 1. App reconcile: `reconcile_all()` keeps every loaded manifest's
    ///    container running.
    /// 2. Companion reconcile: any expected Quadlet companion unit that
    ///    is missing or inactive is repaired (writes the unit, daemon-
    ///    reloads, starts the service). This is the safety net for the
    ///    "someone deleted my unit file" / "systemd lost the service"
    ///    failure modes.
    ///
    /// Never panics: per-app failures are absorbed into `ReconcileReport`
    /// by the orchestrator, and companion failures are logged but never
    /// propagated.
    pub async fn run_forever(self) {
        let wait_start = Instant::now();
        while self.wait_for_recovery && !crate::crash_recovery::is_recovery_complete() {
            if wait_start.elapsed() > Duration::from_secs(1800) {
                tracing::warn!("boot reconciler: boot recovery did not complete within 30 minutes, starting anyway");
                break;
            }
            tokio::select! {
                _ = time::sleep(Duration::from_secs(5)) => {}
                _ = self.shutdown.notified() => {
                    tracing::info!("boot reconciler: shutdown requested before recovery completed");
                    return;
                }
            }
        }

        // Companion self-heal runs on its OWN cadence, decoupled from the
        // per-app reconcile pass. On a heavily loaded node `reconcile_existing`
        // over dozens of apps can take well over a minute, which would delay a
        // companion-unit repair (deleted/lost unit file) past any reasonable
        // safety window. Detecting + rewriting a companion unit is cheap, so it
        // gets a dedicated `interval` loop. The handle is aborted when the main
        // loop exits (shutdown uses `notify_one`, so we must NOT add a second
        // waiter on `self.shutdown` — it would steal the single wake permit).
        let companion_handle = if self.companion_stage {
            let orchestrator = self.orchestrator.clone();
            let interval = self.interval;
            let data_dir = orchestrator.data_dir().to_path_buf();
            Some(tokio::spawn(async move {
                let mut failure_rounds: u32 = 0;
                loop {
                    // `installed_app_ids`, NOT `manifest_ids`: a manifest exists
                    // on disk for every *available* app, so driving companion
                    // provisioning from it stood up a UI for apps nobody had
                    // installed and self-healed it forever (archi-dev-box ran
                    // archy-fedimint-ui and archy-lnd-ui with no fedimint and no
                    // lnd container present — the Guardian UI served its wait
                    // page with nothing behind it, reported as "fedimint
                    // installs but does not work"). `None` means the container
                    // listing failed: skip the whole stage rather than reap
                    // every companion on a transient probe error.
                    let Some(installed) = orchestrator.installed_app_ids().await else {
                        tracing::warn!(
                            "companion reconcile: cannot determine installed apps, skipping this pass"
                        );
                        time::sleep(interval).await;
                        continue;
                    };
                    let failures = crate::container::companion::reconcile(&installed).await;
                    // Reaper, RE-WIRED 2026-08-10 — driven by the DURABLE
                    // installed-apps registry, never by runtime inference.
                    //
                    // History: this call was unwired on 2026-08-08 after it
                    // removed archy-bitcoin-ui and archy-lnd-ui for apps that
                    // WERE installed. Not a logic error — the inputs lied:
                    // `installed_app_ids` infers installation from runtime
                    // state (containers present + running-containers.json),
                    // and the clean-exit vanishing bug falsified both signals
                    // at once. The unwire commit set the re-wire bar: a
                    // durable record of "this app is installed".
                    //
                    // That record now exists — installed-apps.json, written on
                    // install, cleared on deliberate uninstall, backfilled at
                    // boot from demonstrably-present containers, and immune to
                    // container absence by construction (89b03c47 holds
                    // entries while a container is gone). A vanished backend
                    // no longer looks uninstalled, so the failure mode that
                    // burned archi-dev-box cannot recur through this path.
                    //
                    // `None` = the registry could not be read (missing or
                    // corrupt) — which is "I could not look", NOT "nothing is
                    // installed". The reaper stays idle in that case; the
                    // runtime-derived `installed` set above is deliberately
                    // NOT used as a fallback (it is exactly the input class
                    // that caused the 2026-08-08 incident). ORPHAN_GRACE still
                    // applies on top: a companion must be orphaned for the
                    // full grace period before it is touched.
                    if let Some(durable) =
                        crate::crash_recovery::load_installed_apps_if_recorded(&data_dir).await
                    {
                        let durable: Vec<String> = durable.into_iter().collect();
                        for (companion, err) in
                            crate::container::companion::reap_orphans(&durable).await
                        {
                            tracing::warn!(
                                companion = %companion,
                                error = %err,
                                "companion reap failed"
                            );
                        }
                    }
                    for (companion, err) in &failures {
                        tracing::warn!(
                            companion = %companion,
                            error = %err,
                            "companion reconcile failed"
                        );
                    }
                    // A failed repair can involve registry pulls and full
                    // image builds; retrying every 30s hammered unreachable
                    // registries ~174×/image/day on an offline node
                    // (a test node log sweep, 2026-07-22). Back off
                    // exponentially while rounds keep failing — 30s doubling
                    // to a 1h cap — and reset the moment a round is clean.
                    failure_rounds = if failures.is_empty() {
                        0
                    } else {
                        failure_rounds.saturating_add(1)
                    };
                    let backoff = interval
                        .saturating_mul(2u32.saturating_pow(failure_rounds.min(7)))
                        .min(Duration::from_secs(3600));
                    time::sleep(backoff).await;
                }
            }))
        } else {
            None
        };

        // Initial pass: no delay.
        self.tick().await;

        loop {
            let deadline = Instant::now() + self.interval;
            tokio::select! {
                _ = time::sleep_until(deadline) => {
                    self.tick().await;
                }
                _ = self.shutdown.notified() => {
                    tracing::info!("boot reconciler: shutdown requested, exiting loop");
                    break;
                }
            }
        }

        if let Some(handle) = companion_handle {
            handle.abort();
        }
    }

    async fn tick(&self) {
        let report = self.orchestrator.reconcile_existing().await;
        Self::log_report(&report);
    }

    fn log_report(report: &ReconcileReport) {
        for (app_id, action) in &report.actions {
            tracing::debug!(app_id = %app_id, action = ?action, "reconcile action");
        }
        for (app_id, err) in &report.failures {
            tracing::warn!(app_id = %app_id, error = %err, "reconcile failure");
        }
        if report.failures.is_empty() {
            tracing::debug!(count = report.actions.len(), "reconcile pass complete");
        } else {
            tracing::warn!(
                ok = report.actions.len(),
                failed = report.failures.len(),
                "reconcile pass completed with failures"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::prod_orchestrator::ProdContainerOrchestrator;
    use anyhow::Result;
    use archipelago_container::{
        AppManifest, BuildConfig, ContainerRuntime as ContainerRuntimeTrait, ContainerState,
        ContainerStatus,
    };
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Mutex as StdMutex;

    /// Instrumented runtime that counts reconcile-loop side effects so tests
    /// can tell exactly how many passes have fired. All containers are
    /// reported as already Running so `reconcile_all` will NoOp — we are only
    /// measuring loop cadence, not install behavior.
    #[derive(Default)]
    struct CountingRuntime {
        /// Number of times get_container_status has been called. Each
        /// reconcile_all pass hits this once per manifest, so with one
        /// manifest this equals the number of reconcile passes.
        status_calls: StdMutex<u32>,
        running: StdMutex<HashMap<String, ContainerState>>,
    }

    impl CountingRuntime {
        fn new_with(names: &[&str]) -> Self {
            let me = Self::default();
            let mut m = me.running.lock().unwrap();
            for n in names {
                m.insert((*n).to_string(), ContainerState::Running);
            }
            drop(m);
            me
        }
        fn status_call_count(&self) -> u32 {
            *self.status_calls.lock().unwrap()
        }
    }

    #[async_trait]
    impl ContainerRuntimeTrait for CountingRuntime {
        async fn pull_image(&self, _: &str, _: Option<&str>) -> Result<()> {
            Ok(())
        }
        async fn create_container(&self, _: &AppManifest, name: &str, _: u16) -> Result<String> {
            Ok(name.to_string())
        }
        async fn start_container(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn stop_container(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn remove_container(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn get_container_status(&self, name: &str) -> Result<ContainerStatus> {
            *self.status_calls.lock().unwrap() += 1;
            let state = self
                .running
                .lock()
                .unwrap()
                .get(name)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("not found: {name}"))?;
            Ok(ContainerStatus {
                id: format!("id-{name}"),
                name: name.to_string(),
                state,
                health: None,
                exit_code: None,
                started_at: None,
                image: "test".into(),
                created: "now".into(),
                ports: vec![],
                lan_address: None,
            })
        }
        async fn get_container_logs(&self, _: &str, _: u32) -> Result<Vec<String>> {
            Ok(vec![])
        }
        async fn list_containers(&self) -> Result<Vec<ContainerStatus>> {
            Ok(vec![])
        }
        async fn image_exists(&self, _: &str) -> Result<bool> {
            Ok(true)
        }
        async fn build_image(&self, _: &BuildConfig) -> Result<()> {
            Ok(())
        }
    }

    fn pull_manifest(id: &str, image: &str) -> AppManifest {
        let yaml = format!(
            "app:\n  id: {id}\n  name: {id}\n  version: 1.0.0\n  container:\n    image: {image}\n"
        );
        AppManifest::parse(&yaml).unwrap()
    }

    async fn orch_with_one_running_manifest(
        rt: Arc<CountingRuntime>,
    ) -> Arc<ProdContainerOrchestrator> {
        let mut orch =
            ProdContainerOrchestrator::with_runtime(rt, PathBuf::from("/nonexistent-for-tests"));
        let tmp = tempfile::tempdir().unwrap().keep();
        orch.set_data_dir(tmp);
        orch.set_disk_gb_for_test(2_000);
        let orch = Arc::new(orch);
        orch.insert_manifest_for_test(
            pull_manifest("test-app", "docker.io/example/test-app:1"),
            PathBuf::from("/tmp/test-app"),
        )
        .await;
        orch
    }

    async fn wait_for_status_calls(rt: &CountingRuntime, expected: u32) -> u32 {
        for _ in 0..1000 {
            let count = rt.status_call_count();
            if count >= expected {
                return count;
            }
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        rt.status_call_count()
    }

    #[tokio::test]
    async fn initial_pass_fires_immediately() {
        let rt = Arc::new(CountingRuntime::new_with(&["test-app"]));
        let orch = orch_with_one_running_manifest(rt.clone()).await;
        let shutdown = Arc::new(Notify::new());
        let reconciler =
            BootReconciler::new(orch.clone(), Duration::from_millis(50), shutdown.clone())
                .without_companion_stage();
        let handle = tokio::spawn(reconciler.run_forever());

        // We expect exactly one reconcile pass to have run by now (the initial),
        // NOT a second one (the 30s sleep hasn't elapsed in paused time).
        assert_eq!(
            wait_for_status_calls(&rt, 1).await,
            1,
            "initial pass should fire once"
        );

        shutdown.notify_one();
        tokio::task::yield_now().await;
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn second_pass_fires_after_interval() {
        let rt = Arc::new(CountingRuntime::new_with(&["test-app"]));
        let orch = orch_with_one_running_manifest(rt.clone()).await;
        let shutdown = Arc::new(Notify::new());
        let reconciler =
            BootReconciler::new(orch.clone(), Duration::from_millis(10), shutdown.clone())
                .without_companion_stage();
        let handle = tokio::spawn(reconciler.run_forever());

        assert_eq!(wait_for_status_calls(&rt, 1).await, 1);

        tokio::time::sleep(Duration::from_millis(20)).await;
        let count = wait_for_status_calls(&rt, 2).await;

        assert!(
            count >= 2,
            "a second reconcile pass should fire after one interval"
        );

        shutdown.notify_one();
        tokio::task::yield_now().await;
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn shutdown_terminates_loop() {
        let rt = Arc::new(CountingRuntime::new_with(&["test-app"]));
        let orch = orch_with_one_running_manifest(rt.clone()).await;
        let shutdown = Arc::new(Notify::new());
        let reconciler =
            BootReconciler::new(orch.clone(), Duration::from_millis(50), shutdown.clone())
                .without_companion_stage();
        let handle = tokio::spawn(reconciler.run_forever());
        wait_for_status_calls(&rt, 1).await;

        shutdown.notify_one();
        let result = tokio::time::timeout(Duration::from_secs(5), handle).await;
        assert!(result.is_ok(), "reconciler did not exit after shutdown");
    }

    #[tokio::test]
    async fn failure_in_one_pass_does_not_stop_loop() {
        // Manifest references a container the runtime does not have AND
        // cannot create (no install path — install_fresh will also fail to
        // pull, since CountingRuntime::pull_image returns Ok but the
        // manifest's referenced container stays uncreated). In practice
        // reconcile_all will observe the missing container, install_fresh
        // will run, and the next pass will see a new state. We care about
        // "loop keeps ticking even when the report has actions".
        let rt = Arc::new(CountingRuntime::default());
        let mut orch = ProdContainerOrchestrator::with_runtime(
            rt.clone(),
            PathBuf::from("/nonexistent-for-tests"),
        );
        let tmp = tempfile::tempdir().unwrap().keep();
        orch.set_data_dir(tmp);
        orch.set_disk_gb_for_test(2_000);
        let orch = Arc::new(orch);
        orch.insert_manifest_for_test(
            pull_manifest("test-app", "docker.io/example/test-app:1"),
            PathBuf::from("/tmp/test-app"),
        )
        .await;
        let shutdown = Arc::new(Notify::new());
        let reconciler =
            BootReconciler::new(orch.clone(), Duration::from_millis(10), shutdown.clone())
                .without_companion_stage();
        let handle = tokio::spawn(reconciler.run_forever());

        let first = wait_for_status_calls(&rt, 1).await;
        assert!(first >= 1, "initial pass should have touched the runtime");

        tokio::time::sleep(Duration::from_millis(20)).await;
        let second = wait_for_status_calls(&rt, first + 1).await;
        assert!(
            second > first,
            "loop should have fired a second pass after the interval"
        );

        shutdown.notify_one();
        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
    }
}
