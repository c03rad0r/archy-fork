//! Async wrappers for `package.install`, `package.uninstall`, `package.update`.
//!
//! The inner `handle_package_*` functions are large (install is 480 lines with
//! the stack dispatchers, update is 300, uninstall is 200) and do their own
//! fine-grained progress tracking via `install_progress` and `uninstall_stage`.
//! We wrap them rather than refactor them.
//!
//! Each wrapper:
//!   1. Parses + validates the RPC params (cheap, synchronous). Errors here
//!      return immediately to the caller before any state change.
//!   2. Flips the package state to the transitional variant
//!      (`Installing` / `Removing` / `Updating`) so the UI sees it on the
//!      next WebSocket push (before the RPC response even lands).
//!   3. `tokio::spawn`s a background task that invokes the existing
//!      `handle_package_*` method on the Arc-held self.
//!   4. On task success: no state change needed — the inner handler has
//!      already written the terminal state (Running for install/update, or
//!      removed the entry for uninstall).
//!   5. On task failure: revert state to the pre-transition value (or delete
//!      the entry for install, since there was no pre-state), write a line
//!      to the persistent install log, and clear any stale progress fields.
//!   6. Returns `{ "status": "installing" }` etc. immediately.
//!
//! The server package-scan loop's `merge_preserving_transitional` helper
//! already knows to preserve `Installing` / `Removing` / `Updating` between
//! scans, so live progress updates broadcast from inside the spawned task
//! reach the UI correctly.

use super::install::install_log;
use crate::api::rpc::RpcHandler;
use crate::data_model::PackageState;
use crate::state::StateManager;
use anyhow::Result;
use std::sync::Arc;
use tracing::{error, info, warn};

impl RpcHandler {
    /// Async wrapper for `package.install`. Returns `{ "status": "installing" }`
    /// immediately after flipping state to `Installing` and spawning the
    /// actual install pipeline. On failure, removes the package entry from
    /// state so the UI reverts to "not installed".
    pub(in crate::api::rpc) async fn spawn_package_install(
        self: Arc<Self>,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // Extract + validate package_id synchronously so bad params fail
        // fast without touching state.
        let params_val = params
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let package_id = params_val
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing package id"))?
            .to_string();
        super::validation::validate_app_id(&package_id)?;
        super::dependencies::check_bitcoin_pruning_compatibility(&package_id).await?;
        super::dependencies::check_cuprate_disk_compatibility(&package_id).await?;

        // Reject if already in a transitional lifecycle (prevents double-click
        // queuing two installs on the same package).
        {
            let (data, _) = self.state_manager.get_snapshot().await;
            if let Some(entry) = data.package_data.get(&package_id) {
                if matches!(
                    entry.state,
                    PackageState::Installing | PackageState::Removing | PackageState::Updating
                ) {
                    return Err(anyhow::anyhow!(
                        "{} is already {:?}",
                        package_id,
                        entry.state
                    ));
                }
            }
        }

        // Flip state to Installing BEFORE the spawn so the first WebSocket
        // push carries the transitional state. Uses the same
        // `create_installing_entry` path the inner handler would use once
        // it starts pulling, so the UI sees a consistent shape.
        flip_to_installing(&self.state_manager, &package_id).await;

        install_log(&format!("INSTALL SPAWN: {}", package_id)).await;

        let handler = Arc::clone(&self);
        let package_id_spawn = package_id.clone();
        tokio::spawn(async move {
            match handler.handle_package_install(params).await {
                Ok(_) => {
                    info!("package.install {}: complete", package_id_spawn);
                    for id in [&package_id_spawn, &format!("archy-{}", package_id_spawn)] {
                        crate::crash_recovery::clear_user_uninstalled(&handler.config.data_dir, id)
                            .await;
                    }
                    crate::crash_recovery::mark_installed(
                        &handler.config.data_dir,
                        &package_id_spawn,
                    )
                    .await;
                    // The install pipeline has verified the container is up
                    // and healthy (see install.rs post-start exit check).
                    // Kick the scanner first so the fresh manifest (with
                    // `interfaces.main.ui` from the live port binding) lands
                    // BEFORE we flip to Running — without this the Launch
                    // button is missing for up to 60s after a successful
                    // install, because the skeletal install-time manifest
                    // has `interfaces: None`.
                    kick_scanner_and_wait(&handler).await;
                    // We MUST explicitly transition out of Installing here:
                    // `merge_preserving_transitional` in the package-scan
                    // loop treats Installing as RPC-owned and refuses to
                    // let the scanner overwrite it with the observed
                    // Running state. Without this write, the entry stays
                    // stuck at Installing forever.
                    set_package_state(
                        &handler.state_manager,
                        &package_id_spawn,
                        PackageState::Running,
                    )
                    .await;
                    handler.clear_install_progress(&package_id_spawn).await;
                    // Auto-expose the app over Tor (best-effort, detached) —
                    // every installed app gets its .onion without a manual
                    // "Add Service" step.
                    let tor_handler = Arc::clone(&handler);
                    let tor_app = package_id_spawn.clone();
                    tokio::spawn(async move {
                        tor_handler.auto_add_tor_service(&tor_app).await;
                    });
                }
                Err(e) => {
                    error!("package.install {} failed: {:#}", package_id_spawn, e);
                    install_log(&format!("INSTALL FAIL: {} — {:#}", package_id_spawn, e)).await;
                    // handle_package_install saves the catalog-provided
                    // dynamic app config to /var/lib/archipelago/app-configs
                    // BEFORE the install pipeline runs, so a failure can
                    // strand that file (and the optimistic state entry) with
                    // no container behind it. Probe once here; both cleanup
                    // branches below only fire when the app has no footprint.
                    // A retry re-saves the config (the frontend sends
                    // containerConfig on every install), so removal is safe.
                    let left_container =
                        failed_install_left_container(&handler, &package_id_spawn).await;
                    // Dependency-gate rejections happen BEFORE any resource
                    // (container/image/data dir) exists for this package, so
                    // keeping the optimistic entry would leave a phantom
                    // "Stopped" tile whose Start fails with `no such object`
                    // (the log-confirmed LND fresh-install failure). Remove
                    // the entry so the card reverts to installable, and
                    // surface the reason as a notification instead.
                    if let Some(gate) = e.downcast_ref::<super::dependencies::DependencyGateError>()
                    {
                        if !left_container {
                            remove_dynamic_app_config(&package_id_spawn).await;
                        }
                        remove_entry_with_notification(
                            &handler,
                            &package_id_spawn,
                            "install-deps",
                            &gate.to_string(),
                        )
                        .await;
                        return;
                    }
                    // A failed install that left NO container behind has no
                    // real footprint either — keeping the entry would leave
                    // the same phantom "Stopped" tile in My Apps (and the
                    // scanner-side absence eviction takes 3 scans to catch
                    // it). Remove the saved config + entry and surface the
                    // failure as a notification, exactly like the gate case.
                    if !left_container {
                        remove_dynamic_app_config(&package_id_spawn).await;
                        remove_entry_with_notification(
                            &handler,
                            &package_id_spawn,
                            "install-failed",
                            &format!("Install failed: {:#}", e),
                        )
                        .await;
                        return;
                    }
                    // A container exists (crash-after-start kept for
                    // visibility, retry over an existing install, upgrade) —
                    // don't remove the entry, that's what made the card
                    // vanish from My Apps mid-install / between retry-loop
                    // attempts (e.g. tailscale's entrypoint failure). Leave
                    // the entry visible with state=Stopped + the install
                    // error in install_progress.message so the user can see
                    // what went wrong and decide whether to retry or
                    // uninstall. clear_install_progress would erase the
                    // message, so we set it explicitly here instead. The
                    // phase is cleared (None) so no stale InstallPhase
                    // lingers on the card.
                    let err_msg = format!("Install failed: {:#}", e);
                    handler
                        .state_manager
                        .mutate_data(|data| {
                            if let Some(entry) = data.package_data.get_mut(&package_id_spawn) {
                                entry.state = PackageState::Stopped;
                                entry.install_progress = Some(crate::data_model::InstallProgress {
                                    size: 0,
                                    downloaded: 0,
                                    phase: None,
                                    message: Some(err_msg),
                                });
                            }
                        })
                        .await;
                }
            }
        });

        Ok(serde_json::json!({
            "status": "installing",
            "package_id": package_id,
        }))
    }

    /// Async wrapper for `package.uninstall`. Returns `{ "status": "removing" }`
    /// immediately. State stays `Removing` until the inner handler finishes
    /// (including the `sudo rm -rf` of app data, which can take minutes for
    /// bitcoin-core's chainstate). On failure, reverts to the pre-transition
    /// state (usually Running or Stopped) so the user can retry.
    pub(in crate::api::rpc) async fn spawn_package_uninstall(
        self: Arc<Self>,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params_val = params
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let package_id = params_val
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing package id"))?
            .to_string();
        super::validation::validate_app_id(&package_id)?;

        // Reject if already in a transitional lifecycle.
        {
            let (data, _) = self.state_manager.get_snapshot().await;
            if let Some(entry) = data.package_data.get(&package_id) {
                if matches!(
                    entry.state,
                    PackageState::Installing | PackageState::Removing | PackageState::Updating
                ) {
                    return Err(anyhow::anyhow!(
                        "{} is already {:?}",
                        package_id,
                        entry.state
                    ));
                }
            }
        }

        let pre_state =
            flip_package_state(&self.state_manager, &package_id, PackageState::Removing).await;

        install_log(&format!("UNINSTALL SPAWN: {}", package_id)).await;

        let handler = Arc::clone(&self);
        let package_id_spawn = package_id.clone();
        tokio::spawn(async move {
            match handler.handle_package_uninstall(params).await {
                Ok(_) => {
                    info!("package.uninstall {}: complete", package_id_spawn);
                    for id in [&package_id_spawn, &format!("archy-{}", package_id_spawn)] {
                        crate::crash_recovery::mark_user_uninstalled(&handler.config.data_dir, id)
                            .await;
                        crate::crash_recovery::clear_installed(&handler.config.data_dir, id).await;
                    }
                    // Inner handler already removed the package entry on
                    // success. Nothing more to do here.
                }
                Err(e) => {
                    error!("package.uninstall {} failed: {:#}", package_id_spawn, e);
                    install_log(&format!("UNINSTALL FAIL: {} — {:#}", package_id_spawn, e)).await;
                    // Revert to pre-transition state so the user can retry.
                    // Also clear any stale uninstall_stage label.
                    if let Some(prev) = pre_state {
                        set_package_state_and_clear_uninstall_stage(
                            &handler.state_manager,
                            &package_id_spawn,
                            prev,
                        )
                        .await;
                    }
                }
            }
        });

        Ok(serde_json::json!({
            "status": "removing",
            "package_id": package_id,
        }))
    }

    /// Async wrapper for `package.update`. Returns `{ "status": "updating" }`
    /// immediately. The inner handler already manages its own rollback on
    /// failure (restarts old containers); this wrapper just flips state and
    /// spawns.
    pub(in crate::api::rpc) async fn spawn_package_update(
        self: Arc<Self>,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params_val = params
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let package_id = params_val
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing package id"))?
            .to_string();
        super::validation::validate_app_id(&package_id)?;
        // Update is stop → pull → remove → recreate, i.e. a fresh start by
        // another name: on a disk that shrank since install it would resume
        // cuprate's unprunable sync unchecked. Same gate as install and
        // start, run BEFORE the Updating flip so a refusal leaves the app
        // cleanly in its previous state.
        super::dependencies::check_cuprate_disk_compatibility(&package_id).await?;

        // Reject if already in a transitional lifecycle.
        {
            let (data, _) = self.state_manager.get_snapshot().await;
            if let Some(entry) = data.package_data.get(&package_id) {
                if matches!(
                    entry.state,
                    PackageState::Installing | PackageState::Removing | PackageState::Updating
                ) {
                    return Err(anyhow::anyhow!(
                        "{} is already {:?}",
                        package_id,
                        entry.state
                    ));
                }
            }
        }

        // The inner handler flips state to Updating itself, but we do it
        // here too so the transitional state lands before the spawn yields.
        let pre_state =
            flip_package_state(&self.state_manager, &package_id, PackageState::Updating).await;

        install_log(&format!("UPDATE SPAWN: {}", package_id)).await;

        let handler = Arc::clone(&self);
        let package_id_spawn = package_id.clone();
        tokio::spawn(async move {
            match handler.handle_package_update(params).await {
                Ok(result) => {
                    info!("package.update {}: complete", package_id_spawn);
                    // Same reasoning as install: the merge_preserving_transitional
                    // helper treats Updating as RPC-owned, so we MUST write the
                    // terminal Running state ourselves or the entry will stay
                    // stuck at Updating forever. The update pipeline has
                    // already verified the new container is running via its
                    // post-recreate check.
                    // Kick the scanner first so any manifest changes from the
                    // new image version (interfaces, ports, etc.) land before
                    // we flip to Running.
                    kick_scanner_and_wait(&handler).await;
                    set_package_state(
                        &handler.state_manager,
                        &package_id_spawn,
                        if result.get("status").and_then(|v| v.as_str()) == Some("up-to-date") {
                            pre_state.clone().unwrap_or(PackageState::Running)
                        } else {
                            PackageState::Running
                        },
                    )
                    .await;
                }
                Err(e) => {
                    error!("package.update {} failed: {:#}", package_id_spawn, e);
                    install_log(&format!("UPDATE FAIL: {} — {:#}", package_id_spawn, e)).await;
                    // Inner handler already ran rollback_update + cleared
                    // update state, but be defensive: revert to pre-state
                    // in case the inner flow died before its cleanup.
                    if let Some(prev) = pre_state {
                        set_package_state(&handler.state_manager, &package_id_spawn, prev).await;
                    }
                }
            }
        });

        Ok(serde_json::json!({
            "status": "updating",
            "package_id": package_id,
        }))
    }
}

// ---------------------------------------------------------------------------
// State-manager helpers (free fns, usable from inside spawned tasks)
// ---------------------------------------------------------------------------

/// Create or update the entry for this package with `Installing` state.
/// Matches what the inner handler's `set_install_progress` would do on first
/// call, but fires before the spawn so the UI sees it immediately.
async fn flip_to_installing(state_manager: &StateManager, package_id: &str) {
    use crate::data_model::{Description, Manifest, PackageDataEntry, StaticFiles};
    state_manager
        .mutate_data(|data| {
            let entry = data
                .package_data
                .entry(package_id.to_string())
                .or_insert_with(|| PackageDataEntry {
                    ui_ready: None,
                    state: PackageState::Installing,
                    health: None,
                    exit_code: None,
                    static_files: StaticFiles {
                        license: String::new(),
                        instructions: String::new(),
                        // Leave icon empty during the transient Installing window:
                        // hardcoding `<id>.png` is wrong for ~half our apps (many use
                        // `.svg` / `.webp`), producing a broken-image flicker until
                        // the scanner refreshes the entry. The frontend's `icon`
                        // computed falls through to `curatedMap.get(id)?.icon` which
                        // has the correct extensions for known apps.
                        icon: String::new(),
                    },
                    manifest: Manifest {
                        id: package_id.to_string(),
                        title: package_id.to_string(),
                        version: String::new(),
                        description: Description {
                            short: "Installing...".to_string(),
                            long: String::new(),
                        },
                        release_notes: String::new(),
                        license: String::new(),
                        wrapper_repo: String::new(),
                        upstream_repo: String::new(),
                        support_site: String::new(),
                        marketing_site: String::new(),
                        donation_url: None,
                        author: None,
                        website: None,
                        interfaces: None,
                        tier: None,
                    },
                    installed: None,
                    install_progress: None,
                    uninstall_stage: None,
                    available_update: None,
                });
            entry.ui_ready = Some(false);
            entry.state = PackageState::Installing;
        })
        .await;
}

/// True when the failed install still has a real footprint: any container
/// belonging to `package_id` exists (any state — created/exited count too;
/// the install-crash path deliberately keeps the exited container visible),
/// or the app carries a user-stopped marker (Quadlet units run with `--rm`,
/// so a cleanly user-stopped app legitimately has no podman record). Errors
/// from the podman probe count as "exists" — never clean up on an uncertain
/// reading.
async fn failed_install_left_container(handler: &RpcHandler, package_id: &str) -> bool {
    if crate::crash_recovery::load_user_stopped(&handler.config.data_dir)
        .await
        .contains(package_id)
    {
        return true;
    }
    match super::config::get_containers_for_app(package_id).await {
        Ok(containers) => !containers.is_empty(),
        Err(e) => {
            warn!(
                "install cleanup {}: container probe failed ({:#}); keeping saved config",
                package_id, e
            );
            true
        }
    }
}

/// Remove the catalog-provided dynamic app config that
/// `handle_package_install` saved before the pipeline ran (mirror of the
/// write in install.rs). Only called when the app has no container — for an
/// existing install (retry/upgrade) the file is still the app's live runtime
/// config and must be kept.
async fn remove_dynamic_app_config(package_id: &str) {
    let config_path = format!("/var/lib/archipelago/app-configs/{}.json", package_id);
    match tokio::fs::remove_file(&config_path).await {
        Ok(()) => info!(
            "Removed dynamic app config for {} after failed install (no container)",
            package_id
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => warn!(
            "Failed to remove dynamic app config for {}: {}",
            package_id, e
        ),
    }
}

/// Remove the package's optimistic state entry (clearing any pending install
/// phase with it) so the card reverts to installable, and surface the failure
/// reason as an error notification instead.
async fn remove_entry_with_notification(
    handler: &RpcHandler,
    package_id: &str,
    id_prefix: &str,
    message: &str,
) {
    handler
        .state_manager
        .mutate_data(|data| {
            data.package_data.remove(package_id);
            data.notifications.push(crate::data_model::Notification {
                id: format!("{id_prefix}-{package_id}"),
                level: crate::data_model::NotificationLevel::Error,
                title: format!("Could not install {package_id}"),
                message: message.to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                app_id: Some(package_id.to_string()),
            });
            while data.notifications.len() > 20 {
                data.notifications.remove(0);
            }
        })
        .await;
}

/// Flip an existing entry's state and return the pre-flip value (or None if
/// no entry existed). Used for revert-on-failure.
async fn flip_package_state(
    state_manager: &StateManager,
    package_id: &str,
    new_state: PackageState,
) -> Option<PackageState> {
    state_manager
        .mutate_data(|data| {
            let prev = data.package_data.get(package_id).map(|e| e.state.clone());
            if let Some(entry) = data.package_data.get_mut(package_id) {
                if new_state != PackageState::Running {
                    entry.ui_ready = Some(false);
                }
                entry.state = new_state;
            } else {
                warn!(
                    "flip_package_state: no entry for {} — cannot flip",
                    package_id
                );
            }
            prev
        })
        .await
}

/// Set state unconditionally (no-op if entry no longer exists).
async fn set_package_state(
    state_manager: &StateManager,
    package_id: &str,
    new_state: PackageState,
) {
    state_manager
        .mutate_data(|data| {
            if let Some(entry) = data.package_data.get_mut(package_id) {
                if entry.state != new_state {
                    if new_state != PackageState::Running {
                        entry.ui_ready = Some(false);
                    }
                    entry.state = new_state;
                }
            }
        })
        .await
}

/// Set state and clear the uninstall_stage label. Used when an uninstall
/// fails and we revert — the user doesn't want a stale "Removing app data"
/// message sitting on a Running entry.
async fn set_package_state_and_clear_uninstall_stage(
    state_manager: &StateManager,
    package_id: &str,
    new_state: PackageState,
) {
    state_manager
        .mutate_data(|data| {
            if let Some(entry) = data.package_data.get_mut(package_id) {
                if new_state != PackageState::Running {
                    entry.ui_ready = Some(false);
                }
                entry.state = new_state;
                entry.uninstall_stage = None;
            }
        })
        .await
}

/// Kick the container scanner to run immediately and wait for it to finish
/// (with a 2s timeout). Used by install/update success paths so the fresh
/// manifest — with `interfaces.main.ui` populated from the now-running
/// container's port binding — lands BEFORE we flip state to Running.
///
/// Without this, the frontend sees `state = running` but the skeletal
/// install-time manifest (interfaces = None), and hides the Launch button
/// for up to the full 60s scan interval.
///
/// The scan merges via `merge_preserving_transitional`, which keeps
/// state = Installing (we haven't flipped yet) while taking the fresh
/// manifest. After this returns, the caller writes Running on top of the
/// now-populated manifest.
async fn kick_scanner_and_wait(handler: &RpcHandler) {
    let mut rx = handler.scan_tick.subscribe();
    let start = *rx.borrow_and_update();
    handler.scan_kick.notify_one();
    // 2s is well above a typical podman scan (~200ms on .228, ~500ms worst
    // case). If it times out we proceed anyway — the next 60s scan will
    // self-heal and the worst case is the pre-fix behavior (Launch button
    // appears a bit late).
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while *rx.borrow_and_update() == start {
            if rx.changed().await.is_err() {
                break;
            }
        }
    })
    .await;
}
