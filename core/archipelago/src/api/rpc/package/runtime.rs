use super::config::{
    get_app_capabilities, get_containers_for_app, get_data_dirs_for_app, get_health_check_args,
    get_memory_limit, is_valid_docker_image,
};
use super::dependencies::{ordered_containers_for_start, startup_order};
use super::install::install_log;
use super::validation::validate_app_id;
use crate::api::rpc::RpcHandler;
use crate::data_model::PackageState;
use anyhow::{Context, Result};
use archipelago_container::AppManifest;
use std::path::Path;
use std::process::Output;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::warn;

const PODMAN_CONTROL_TIMEOUT: Duration = Duration::from_secs(30);
const PODMAN_LOG_TIMEOUT: Duration = Duration::from_secs(15);

/// Per-container graceful shutdown timeout in seconds.
/// Bitcoin Core needs 600s to flush UTXO set, LND 330s for channel state,
/// indexers 300s for index flush, databases 120s for WAL/transaction commit.
///
/// MIRRORS `archipelago_container::runtime::stop_grace_secs_for` (which returns
/// `u64` and is the canonical table used by the orchestrator stop path). This
/// `&str` variant exists for the legacy `podman stop -t <s>` call sites here —
/// keep the two tables in sync until those are migrated to the orchestrator.
pub fn stop_timeout_secs(container_name: &str) -> &'static str {
    let id = container_name
        .strip_prefix("archy-")
        .unwrap_or(container_name);
    match id {
        "bitcoin-knots" | "bitcoin-core" | "bitcoin" => "600",
        "lnd" => "330",
        "electrumx" | "electrs" | "mempool-electrs" => "300",
        "btcpay-db" | "mempool-db" | "penpot-postgres" | "immich_postgres" | "nextcloud-db"
        | "endurain-db" => "120",
        "btcpay-server" | "nbxplorer" | "fedimint" | "fedimint-gateway" => "60",
        _ => "30",
    }
}

impl RpcHandler {
    /// Start a package: start all containers in dependency order.
    ///
    /// Returns immediately with `{ "status": "starting" }` after flipping
    /// the package state to `Starting` in the StateManager. The actual
    /// podman-start sequence + post-start exit verification runs in a
    /// background task. On success the state becomes `Running`; on error
    /// it reverts to the pre-transition state.
    pub(in crate::api::rpc) async fn handle_package_start(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let package_id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing package id"))?;
        validate_app_id(package_id)?;
        // A cuprate node that starts on a too-small disk fills it and takes
        // Archipelago down with it (no upstream pruning — see
        // dependencies::check_cuprate_disk_compatibility). Fail the start
        // before clearing user-stopped or flipping state, so the app stays
        // cleanly stopped and the error carries the actionable message.
        super::dependencies::check_cuprate_disk_compatibility(package_id).await?;

        let to_start = if self.orchestrator.is_some() && uses_single_orchestrator_app(package_id) {
            vec![orchestrator_app_id(package_id).to_string()]
        } else if let Some(members) =
            orchestrator_stack_members(self.orchestrator.is_some(), package_id)
        {
            members
        } else {
            ordered_containers_for_start(package_id).await?
        };
        if to_start.is_empty() {
            tracing::warn!("package.start {}: no containers found", package_id);
            return Err(anyhow::anyhow!("No containers found for {}", package_id));
        }

        // Clear user-stopped flag — user explicitly started this app.
        // Must happen BEFORE the spawn (ordering contract with crash recovery).
        crate::crash_recovery::clear_user_stopped(&self.config.data_dir, package_id).await;
        for name in &to_start {
            crate::crash_recovery::clear_user_stopped(&self.config.data_dir, name).await;
        }

        let package_id_owned = package_id.to_string();
        let companion_app_id = package_id_owned.clone();
        let orchestrator = self.orchestrator.clone();
        let state_manager = Arc::clone(&self.state_manager);
        let pre_state =
            flip_package_state(&state_manager, &package_id_owned, PackageState::Starting).await;

        install_log(&format!(
            "START: {} (containers: {:?})",
            package_id_owned, to_start
        ))
        .await;

        let op_lock = app_op_lock(package_id);
        let data_dir = self.config.data_dir.clone();
        tokio::spawn(async move {
            let _op_guard = op_lock.lock().await;
            let result = if let Some(orchestrator) = orchestrator.as_ref() {
                do_orchestrator_package_start(orchestrator.as_ref(), &to_start).await
            } else {
                do_package_start(&to_start).await
            };
            match result {
                Ok(()) => {
                    reconcile_companions_for(&companion_app_id).await;
                    set_package_state(&state_manager, &package_id_owned, PackageState::Running)
                        .await;
                    cascade_restart_address_caching_dependents(
                        orchestrator.as_ref(),
                        &state_manager,
                        &data_dir,
                        &package_id_owned,
                    )
                    .await;
                }
                Err(e) => {
                    tracing::error!("package.start {} failed: {:#}", package_id_owned, e);
                    install_log(&format!("START FAIL: {} — {:#}", package_id_owned, e)).await;
                    if let Some(prev) = pre_state {
                        set_package_state(&state_manager, &package_id_owned, prev).await;
                    } else {
                        warn!(
                            "package.start {}: no pre-state recorded; relying on next scan",
                            package_id_owned
                        );
                    }
                }
            }
        });

        Ok(serde_json::json!({ "status": "starting" }))
    }

    /// Stop a package: mark as user-stopped and stop all containers.
    ///
    /// Returns immediately with `{ "status": "stopping" }`. podman stop
    /// (up to 600s for bitcoin-core) runs in the background.
    pub(in crate::api::rpc) async fn handle_package_stop(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let package_id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing package id"))?;
        validate_app_id(package_id)?;

        let single_orchestrator_app =
            self.orchestrator.is_some() && uses_single_orchestrator_app(package_id);
        let mut containers = if single_orchestrator_app {
            vec![orchestrator_app_id(package_id).to_string()]
        } else {
            get_containers_for_app(package_id).await?
        };
        if !single_orchestrator_app {
            containers.reverse();
        }
        if containers.is_empty() {
            tracing::warn!("package.stop {}: no containers found", package_id);
            return Err(anyhow::anyhow!("No containers found for {}", package_id));
        }

        // Orchestrator-managed stacks are stopped via member APP ids (reverse
        // start order), never live container names: orchestrator.stop(app_id)
        // stops the member's quadlet .service (systemd removes the --rm
        // container), while a container NAME falls through the unknown-app-id
        // fallback to a raw `podman stop` that races systemd over the unit
        // (immich, gate 2026-07-09).
        let to_stop_ids = if !single_orchestrator_app {
            orchestrator_stack_members(self.orchestrator.is_some(), package_id).map(
                |mut members| {
                    members.reverse();
                    members
                },
            )
        } else {
            None
        };

        // Mark as user-stopped BEFORE the spawn so health monitor and
        // crash recovery don't auto-restart mid-flight. Ordering is
        // load-bearing — see runtime.rs:145-148 original note.
        crate::crash_recovery::mark_user_stopped(&self.config.data_dir, package_id).await;
        for name in &containers {
            crate::crash_recovery::mark_user_stopped(&self.config.data_dir, name).await;
        }
        // Stack members are marked under their app ids too, so the reconcile
        // guard holds for members whose container is currently absent (a live
        // container list can't name those).
        if let Some(ids) = &to_stop_ids {
            for id in ids {
                crate::crash_recovery::mark_user_stopped(&self.config.data_dir, id).await;
            }
        }

        let package_id_owned = package_id.to_string();
        let to_stop = to_stop_ids.unwrap_or_else(|| containers.clone());
        let orchestrator = self.orchestrator.clone();
        let state_manager = Arc::clone(&self.state_manager);
        let pre_state =
            flip_package_state(&state_manager, &package_id_owned, PackageState::Stopping).await;

        install_log(&format!(
            "STOP: {} (containers: {:?})",
            package_id_owned, containers
        ))
        .await;

        let op_lock = app_op_lock(package_id);
        tokio::spawn(async move {
            let _op_guard = op_lock.lock().await;
            let result = if let Some(orchestrator) = orchestrator.as_ref() {
                do_orchestrator_package_stop(orchestrator.as_ref(), &to_stop).await
            } else {
                do_package_stop(&containers).await
            };
            match result {
                Ok(()) => {
                    set_package_state(&state_manager, &package_id_owned, PackageState::Stopped)
                        .await;
                }
                Err(e) => {
                    tracing::error!("package.stop {} failed: {:#}", package_id_owned, e);
                    install_log(&format!("STOP FAIL: {} — {:#}", package_id_owned, e)).await;
                    if let Some(prev) = pre_state {
                        set_package_state(&state_manager, &package_id_owned, prev).await;
                    }
                }
            }
        });

        Ok(serde_json::json!({ "status": "stopping" }))
    }

    /// Restart a package: restart all containers.
    ///
    /// Returns immediately with `{ "status": "restarting" }`. The restart
    /// (up to 600s per container for bitcoin-core) runs in the background.
    pub(in crate::api::rpc) async fn handle_package_restart(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let package_id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing package id"))?;
        validate_app_id(package_id)?;

        let single_orchestrator_app =
            self.orchestrator.is_some() && uses_single_orchestrator_app(package_id);
        let mut containers = if single_orchestrator_app {
            vec![orchestrator_app_id(package_id).to_string()]
        } else {
            get_containers_for_app(package_id).await?
        };
        if containers.is_empty() {
            // A stack whose containers were all removed (quadlet stop) can
            // still be restarted: resurrect it member-by-member, same as
            // package.start's no-live-containers fallback.
            let members = super::dependencies::stack_member_app_ids(package_id);
            if members.is_empty() {
                tracing::warn!("package.restart {}: no containers found", package_id);
                return Err(anyhow::anyhow!("No containers found for {}", package_id));
            }
            containers = members.iter().map(|s| s.to_string()).collect();
        }

        // Restart does not mark user-stopped; user wants the app to keep
        // running. Clear any lingering marker so downstream layers don't
        // interpret the brief podman stop as user intent.
        crate::crash_recovery::clear_user_stopped(&self.config.data_dir, package_id).await;
        for name in &containers {
            crate::crash_recovery::clear_user_stopped(&self.config.data_dir, name).await;
        }

        let package_id_owned = package_id.to_string();
        let companion_app_id = package_id_owned.clone();
        let to_restart = if single_orchestrator_app {
            vec![orchestrator_app_id(package_id).to_string()]
        } else if let Some(members) =
            orchestrator_stack_members(self.orchestrator.is_some(), package_id)
        {
            // Restart stacks via member APP ids: restarting by live container
            // name podman-stops the quadlet container (systemd --rm removes
            // it) and the start half then finds no such container — a 5-min
            // outage + RESTART FAIL on immich (gate 2026-07-09).
            members
        } else {
            ordered_containers_for_start(package_id).await?
        };
        let state_manager = Arc::clone(&self.state_manager);
        let orchestrator = self.orchestrator.clone();
        let pre_state =
            flip_package_state(&state_manager, &package_id_owned, PackageState::Restarting).await;

        install_log(&format!(
            "RESTART: {} (containers: {:?})",
            package_id_owned, containers
        ))
        .await;

        let op_lock = app_op_lock(package_id);
        let data_dir = self.config.data_dir.clone();
        tokio::spawn(async move {
            let _op_guard = op_lock.lock().await;
            let result = if let Some(orchestrator) = orchestrator.as_ref() {
                do_orchestrator_package_restart(orchestrator.as_ref(), &to_restart).await
            } else {
                do_package_restart(&containers).await
            };
            match result {
                Ok(()) => {
                    reconcile_companions_for(&companion_app_id).await;
                    set_package_state(&state_manager, &package_id_owned, PackageState::Running)
                        .await;
                    cascade_restart_address_caching_dependents(
                        orchestrator.as_ref(),
                        &state_manager,
                        &data_dir,
                        &package_id_owned,
                    )
                    .await;
                }
                Err(e) => {
                    tracing::error!("package.restart {} failed: {:#}", package_id_owned, e);
                    install_log(&format!("RESTART FAIL: {} — {:#}", package_id_owned, e)).await;
                    if let Some(prev) = pre_state {
                        set_package_state(&state_manager, &package_id_owned, prev).await;
                    }
                }
            }
        });

        Ok(serde_json::json!({ "status": "restarting" }))
    }

    /// Uninstall a package: stop and remove all related containers, clean data.
    pub(in crate::api::rpc) async fn handle_package_uninstall(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let package_id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing package id"))?;
        validate_app_id(package_id)?;
        let preserve_data = params
            .get("preserve_data")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Disable + remove Quadlet companion units BEFORE the rm loop.
        // Otherwise systemd's Restart=always will respawn each companion
        // within ~10s of `podman rm`, leaving them orphaned post-uninstall.
        crate::container::companion::remove_for(package_id).await;

        // Keep the production reconciler from recreating an app immediately
        // after uninstall. The reconciler owns a manifest map independent of
        // podman state, so a raw `podman rm` alone is not enough.
        if let Some(orchestrator) = &self.orchestrator {
            let mut teardown_errors = Vec::new();
            for app_id in orchestrator_uninstall_app_ids(package_id) {
                if let Err(err) = orchestrator.remove(&app_id, preserve_data).await {
                    teardown_errors.push(format!("{app_id}: {err:#}"));
                }
            }
            if !teardown_errors.is_empty() {
                return Err(anyhow::anyhow!(
                    "Uninstall {} aborted: failed to remove declarative app unit(s): {}",
                    package_id,
                    teardown_errors.join("; ")
                ));
            }
        }

        let containers_to_remove = get_containers_for_app(package_id).await?;
        if containers_to_remove.is_empty() {
            tracing::warn!("Uninstall {}: no containers found", package_id);
        }
        let total = containers_to_remove.len();

        let mut stopped = 0u32;
        let mut removed = 0u32;
        // Two distinct failure classes, kept separate so they don't get
        // conflated (the old single `errors` vec did, which caused the "ghost in
        // My Apps" bug): `container_errors` means a container could NOT be
        // removed (force-rm failed too) — the app is genuinely still present, so
        // we keep its state entry and surface a hard error. `cleanup_errors`
        // means volume/network/data-dir teardown left residue — the containers
        // are already gone, so the app IS uninstalled and MUST disappear from My
        // Apps; the residue is logged but never ghosts the app.
        let mut container_errors: Vec<String> = Vec::new();
        let mut cleanup_errors: Vec<String> = Vec::new();

        self.set_uninstall_stage(
            package_id,
            &if total > 0 {
                format!("Stopping containers (0/{})", total)
            } else {
                "Cleaning up".to_string()
            },
        )
        .await;

        for (i, name) in containers_to_remove.iter().enumerate() {
            self.set_uninstall_stage(
                package_id,
                &format!("Stopping containers ({}/{})", i + 1, total),
            )
            .await;
            tracing::info!("Uninstall {}: stopping container {}", package_id, name);
            let stop_out = podman_control(&["stop", "-t", stop_timeout_secs(name), name]).await;
            match stop_out {
                Ok(o) if o.status.success() => stopped += 1,
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    tracing::warn!(
                        "Uninstall {}: stop {} failed: {}",
                        package_id,
                        name,
                        stderr.trim()
                    );
                }
                Err(e) => {
                    tracing::warn!("Uninstall {}: stop {} error: {}", package_id, name, e);
                }
            }

            // Remove container (without -f to respect graceful shutdown above)
            tracing::info!("Uninstall {}: removing container {}", package_id, name);
            let rm_out = podman_control(&["rm", name]).await;
            match rm_out {
                Ok(o) if o.status.success() => removed += 1,
                Ok(o) => {
                    // If normal rm fails (e.g., still running/stopping/removing),
                    // force with targeted cleanup fallbacks. This is deliberately
                    // container-scoped; never prune the store during uninstall.
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    tracing::warn!(
                        "Uninstall {}: rm {} failed ({}), trying force",
                        package_id,
                        name,
                        stderr.trim()
                    );
                    match force_remove_runtime_container(name).await {
                        Ok(()) => removed += 1,
                        Err(e) => {
                            let msg =
                                format!("Failed to remove {}: {}; {}", name, stderr.trim(), e);
                            tracing::error!("Uninstall {}: {}", package_id, msg);
                            container_errors.push(msg);
                        }
                    }
                }
                Err(e) => match force_remove_runtime_container(name).await {
                    Ok(()) => removed += 1,
                    Err(force_err) => {
                        let msg = format!("Failed to remove {}: {}; {}", name, e, force_err);
                        tracing::error!("Uninstall {}: {}", package_id, msg);
                        container_errors.push(msg);
                    }
                },
            }
        }

        // A container that survived even force-remove means the app is NOT
        // actually uninstalled — keep its state entry and fail so the spawned
        // task reverts it to its prior state (and the user can retry), rather
        // than orphaning a live container that's missing from My Apps.
        if !container_errors.is_empty() {
            tracing::error!(
                "Uninstall {}: containers could not be removed: {:?}",
                package_id,
                container_errors
            );
            return Err(anyhow::anyhow!(
                "Uninstall {} failed: {}",
                package_id,
                container_errors.join("; ")
            ));
        }

        // Containers are gone → the app is uninstalled. Remove its state entry
        // NOW, before the (possibly slow, possibly fallible) volume/data
        // teardown below, so My Apps updates immediately and a residue failure
        // can never leave a ghost. Reinstall/scan no longer see a stale entry.
        self.remove_package_state_entry(package_id).await;

        self.set_uninstall_stage(package_id, "Cleaning up volumes")
            .await;
        // Avoid global Podman volume prune on production nodes: store-wide
        // Podman cleanup commands can block app health under load. App data is
        // removed explicitly below when preserve_data=false.
        tracing::info!(
            package_id = %package_id,
            "Skipping global podman volume prune during uninstall"
        );

        // Clean up app-specific networks (only if no other containers use them)
        let app_networks: Vec<&str> = match package_id {
            "immich" | "immich_server" => vec!["immich-net"],
            "penpot" | "penpot-frontend" => vec!["penpot-net"],
            "indeedhub" | "indeedhub-api" => vec!["indeedhub-net"],
            _ => vec![],
        };
        for net in &app_networks {
            let _ = podman_control(&["network", "rm", net]).await;
        }

        // Release port allocation
        {
            let mut allocator = self.port_allocator.lock().await;
            let _ = allocator.release(package_id).await;
            if package_id == "nginx-proxy-manager" {
                let _ = allocator.release("nginx-proxy-manager-http").await;
                let _ = allocator.release("nginx-proxy-manager-https").await;
            }
        }

        // Clean data directories unless preserve_data
        if !preserve_data {
            self.set_uninstall_stage(package_id, "Removing app data")
                .await;
            let data_dirs = get_data_dirs_for_app(package_id);
            for dir in &data_dirs {
                tracing::info!("Uninstall {}: removing data {}", package_id, dir);
                let rm_out = tokio::process::Command::new("sudo")
                    .args(["rm", "-rf", dir])
                    .output()
                    .await;
                match rm_out {
                    Ok(o) if !o.status.success() => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        let msg = format!("Failed to remove data {}: {}", dir, stderr.trim());
                        tracing::error!("Uninstall {}: {}", package_id, msg);
                        cleanup_errors.push(msg);
                    }
                    Err(e) => {
                        let msg = format!("Failed to remove data {}: {}", dir, e);
                        tracing::error!("Uninstall {}: {}", package_id, msg);
                        cleanup_errors.push(msg);
                    }
                    _ => {}
                }
            }
        }

        // The app is already gone from My Apps (entry removed above). Residual
        // volume/data cleanup failures are logged but NEVER ghost the app — a
        // reinstall and the next uninstall both tolerate leftover dirs.
        if !cleanup_errors.is_empty() {
            tracing::error!(
                "Uninstall {} removed but left cleanup residue: {:?}",
                package_id,
                cleanup_errors
            );
        }

        tracing::info!(
            "Uninstall {} complete: stopped={}, removed={}, cleanup_errors={}",
            package_id,
            stopped,
            removed,
            cleanup_errors.len()
        );

        Ok(serde_json::json!({
            "status": "uninstalled",
            "stopped": stopped,
            "removed": removed,
            "cleanup_warnings": cleanup_errors,
        }))
    }

    /// Remove a package's entry (and any alias keys) from persisted state so it
    /// disappears from My Apps immediately, without waiting for the scanner's
    /// absence threshold (3 scans × 60s). Called as soon as an uninstall has
    /// removed the app's containers — before the slower volume/data teardown —
    /// so a residue failure can never leave a ghost entry behind.
    async fn remove_package_state_entry(&self, package_id: &str) {
        let (mut data, _rev) = self.state_manager.get_snapshot().await;
        let before = data.package_data.len();
        data.package_data.remove(package_id);
        // Also remove any alias keys (e.g. "bitcoin-knots" vs "bitcoin").
        let aliases: Vec<String> = data
            .package_data
            .keys()
            .filter(|k| {
                super::config::all_container_names(package_id)
                    .iter()
                    .any(|c| c.strip_prefix("archy-").unwrap_or(c) == k.as_str())
            })
            .cloned()
            .collect();
        for alias in &aliases {
            data.package_data.remove(alias);
        }
        if data.package_data.len() < before {
            self.state_manager.update_data(data).await;
        }
    }

    /// Start a bundled app (create container from pre-loaded image if needed).
    pub(in crate::api::rpc) async fn handle_bundled_app_start(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let app_id = params
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing app_id"))?;
        validate_app_id(app_id)?;
        let image = params
            .get("image")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing image"))?;
        if !is_valid_docker_image(image) {
            return Err(anyhow::anyhow!("Invalid Docker image format"));
        }
        let ports = params
            .get("ports")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing ports"))?;
        let volumes = params
            .get("volumes")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing volumes"))?;

        let check_output = podman_control(&[
            "ps",
            "-a",
            "--format",
            "{{.Names}}",
            "--filter",
            &format!("name={}", app_id),
        ])
        .await
        .context("Failed to check container")?;

        let existing = String::from_utf8_lossy(&check_output.stdout);

        if existing.trim().is_empty() {
            let mut cmd = tokio::process::Command::new("podman");
            cmd.args(["run", "-d", "--name", app_id]);

            for port in ports {
                if let (Some(host), Some(container)) = (
                    port.get("host").and_then(|v| v.as_u64()),
                    port.get("container").and_then(|v| v.as_u64()),
                ) {
                    cmd.arg("-p").arg(format!("{}:{}", host, container));
                }
            }

            for volume in volumes {
                if let (Some(host), Some(container)) = (
                    volume.get("host").and_then(|v| v.as_str()),
                    volume.get("container").and_then(|v| v.as_str()),
                ) {
                    // Validate host path: must be under /var/lib/archipelago/
                    if !host.starts_with("/var/lib/archipelago/")
                        || host.contains("..")
                        || host.contains('\0')
                    {
                        return Err(anyhow::anyhow!(
                            "Volume host path must be under /var/lib/archipelago/ \
                             and cannot contain path traversal"
                        ));
                    }
                    if container.contains("..") || container.contains('\0') {
                        return Err(anyhow::anyhow!("Invalid container mount path"));
                    }
                    let _ = tokio::process::Command::new("sudo")
                        .args(["mkdir", "-p", host])
                        .output()
                        .await;
                    cmd.arg("-v").arg(format!("{}:{}", host, container));
                }
            }

            cmd.arg(image);

            let output = command_with_timeout(cmd, PODMAN_CONTROL_TIMEOUT, "podman run").await?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("Failed to create container: {}", stderr));
            }
        } else {
            let output = podman_control(&["start", app_id])
                .await
                .context("Failed to start container")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("Failed to start container: {}", stderr));
            }
        }

        Ok(serde_json::json!({ "status": "started", "app_id": app_id }))
    }

    /// Stop a bundled app.
    pub(in crate::api::rpc) async fn handle_bundled_app_stop(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let app_id = params
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing app_id"))?;
        validate_app_id(app_id)?;

        let output = podman_control(&["stop", "-t", stop_timeout_secs(app_id), app_id])
            .await
            .context("Failed to stop container")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to stop container: {}", stderr));
        }

        Ok(serde_json::json!({ "status": "stopped", "app_id": app_id }))
    }
}

// ---------------------------------------------------------------------------
// Background workers for async package lifecycle RPCs.
//
// Extracted from the pre-async RPC handlers so the transitional state is
// visible to the UI immediately. Each worker is pure IO over podman + the
// crash_recovery helpers — no StateManager access here so we don't need
// a handler reference. The caller does state flipping before/after.
// ---------------------------------------------------------------------------

/// Start containers in dependency order. Includes the post-start 3s wait +
/// exit-check verification from the original synchronous handler (critical
/// for catching "podman start succeeded but container immediately exited"
/// failure modes).
async fn do_package_start(to_start: &[String]) -> Result<()> {
    let mut errors = Vec::new();
    for (i, name) in to_start.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        if let Err(e) = ensure_startable_container_state(name).await {
            tracing::error!(container = %name, error = %e, "container is not startable");
            errors.push(format!("{}: {}", name, e));
            continue;
        }
        match inspect_runtime_container_state(name).await {
            Ok(Some(state)) if state == "running" => {
                tracing::debug!(container = %name, "container already running during package start");
                continue;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(container = %name, error = %e, "failed to re-inspect before package start")
            }
        }
        repair_before_package_start(name).await;
        wait_before_package_start(name).await;
        tracing::info!("Starting container: {}", name);
        let out = podman_start_container(name)
            .await
            .context(format!("Failed to exec podman start {}", name))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            tracing::error!("Failed to start {}: {}", name, stderr);
            cleanup_start_conflict(name, &stderr).await;
            install_log(&format!("START FAIL: {} — {}", name, stderr)).await;
            errors.push(format!("{}: {}", name, stderr));
        }
    }
    if !errors.is_empty() {
        return Err(anyhow::anyhow!("Start failed: {}", errors.join("; ")));
    }

    // Post-start exit verification (podman start can succeed even if the
    // container exits immediately after).
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    for name in to_start {
        let status = podman_control(&["inspect", name, "--format", "{{.State.Status}}"]).await;
        if let Ok(o) = status {
            let state = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if state == "exited" {
                let logs =
                    podman_with_timeout(&["logs", "--tail", "5", name], PODMAN_LOG_TIMEOUT).await;
                let log_text = logs
                    .map(|o| {
                        let combined = format!(
                            "{}{}",
                            String::from_utf8_lossy(&o.stdout),
                            String::from_utf8_lossy(&o.stderr)
                        );
                        combined.chars().take(200).collect::<String>()
                    })
                    .unwrap_or_default();
                tracing::error!("Container {} exited after start: {}", name, log_text);
                install_log(&format!("START EXITED: {} — {}", name, log_text)).await;
                errors.push(format!("{}: exited after start", name));
            }
        }
    }
    if !errors.is_empty() {
        return Err(anyhow::anyhow!(
            "Containers exited after start: {}",
            errors.join("; ")
        ));
    }

    for name in to_start {
        ensure_runtime_host_port_listener(name).await?;
    }
    if to_start.iter().any(|name| name == "indeedhub") {
        super::install::patch_indeedhub_nostr_provider().await;
    }
    Ok(())
}

async fn do_orchestrator_package_start(
    orchestrator: &dyn crate::container::traits::ContainerOrchestrator,
    to_start: &[String],
) -> Result<()> {
    let mut errors = Vec::new();
    for (i, name) in to_start.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        repair_before_package_start(name).await;
        wait_before_package_start(name).await;
        match orchestrator.start(name).await {
            Ok(()) => wait_after_orchestrator_start(name).await,
            Err(e) if is_unknown_app_id_error(&e) => {
                // Collect instead of `?`: aborting here skipped every later
                // stack member (mempool gate flakes #73/#74 — see
                // order_present_containers).
                if let Err(e) = do_package_start(&[name.clone()]).await {
                    tracing::error!(container = %name, error = %e, "fallback container start failed");
                    errors.push(format!("{}: {:#}", name, e));
                }
            }
            Err(e) => {
                tracing::error!(container = %name, error = %e, "orchestrator start failed");
                install_log(&format!("START FAIL: {} — {:#}", name, e)).await;
                errors.push(format!("{}: {:#}", name, e));
            }
        }
    }
    if !errors.is_empty() {
        Err(anyhow::anyhow!("Start failed: {}", errors.join("; ")))
    } else {
        for name in to_start {
            ensure_runtime_host_port_listener(name).await?;
        }
        Ok(())
    }
}

async fn wait_after_orchestrator_start(name: &str) {
    let delay = match name {
        "archy-btcpay-db" => 5,
        "archy-nbxplorer" => 8,
        _ => 0,
    };
    if delay > 0 {
        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
    }
}

async fn podman_control(args: &[&str]) -> Result<Output> {
    podman_with_timeout(args, podman_control_timeout(args)).await
}

async fn force_remove_runtime_container(container_name: &str) -> Result<()> {
    for args in [
        vec!["rm", "-f", container_name],
        vec!["rm", "-f", "--time", "0", container_name],
    ] {
        let output = podman_control(&args).await?;
        if output.status.success()
            || is_missing_container_error(&String::from_utf8_lossy(&output.stderr))
        {
            return Ok(());
        }
    }

    let _ = podman_control(&["container", "cleanup", container_name]).await;
    let output = podman_control(&["rm", "-f", container_name]).await?;
    if output.status.success()
        || is_missing_container_error(&String::from_utf8_lossy(&output.stderr))
    {
        return Ok(());
    }

    Err(anyhow::anyhow!(
        "force remove failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

async fn force_stop_runtime_container(container_name: &str) -> Result<()> {
    for args in [
        vec!["stop", "-t", "0", container_name],
        vec!["kill", container_name],
    ] {
        let output = podman_control(&args).await?;
        if output.status.success()
            || is_missing_container_error(&String::from_utf8_lossy(&output.stderr))
        {
            return Ok(());
        }
    }

    for _ in 0..15 {
        match inspect_runtime_container_state(container_name).await? {
            None => return Ok(()),
            Some(state) if matches!(state.as_str(), "exited" | "stopped" | "configured") => {
                return Ok(())
            }
            Some(_) => tokio::time::sleep(Duration::from_secs(2)).await,
        }
    }

    Err(anyhow::anyhow!(
        "container did not reach stopped state after force stop"
    ))
}

async fn ensure_startable_container_state(container_name: &str) -> Result<()> {
    let Some(state) = inspect_runtime_container_state(container_name).await? else {
        return Ok(());
    };

    match state.as_str() {
        "configured" | "created" | "exited" | "stopped" | "running" | "paused" => Ok(()),
        "removing" => {
            wait_for_container_absent_or_startable(container_name, Duration::from_secs(60)).await
        }
        other => Err(anyhow::anyhow!(
            "container is in unsupported state before start: {}",
            other
        )),
    }
}

async fn wait_for_container_absent_or_startable(
    container_name: &str,
    timeout: Duration,
) -> Result<()> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match inspect_runtime_container_state(container_name).await? {
            None => return Ok(()),
            Some(state)
                if matches!(
                    state.as_str(),
                    "configured" | "created" | "exited" | "stopped" | "running" | "paused"
                ) =>
            {
                return Ok(())
            }
            Some(state) if state == "removing" && std::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Some(state) if state == "removing" => {
                force_remove_runtime_container(container_name).await?;
                return Ok(());
            }
            Some(state) => {
                return Err(anyhow::anyhow!(
                    "container is in unsupported state before start: {}",
                    state
                ))
            }
        }
    }
}

async fn inspect_runtime_container_state(container_name: &str) -> Result<Option<String>> {
    let output = podman_with_timeout(
        &["inspect", container_name, "--format", "{{.State.Status}}"],
        Duration::from_secs(10),
    )
    .await?;
    if output.status.success() {
        return Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if is_missing_container_error(&stderr) {
        Ok(None)
    } else {
        Err(anyhow::anyhow!("inspect failed: {}", stderr.trim()))
    }
}

fn is_missing_container_error(stderr: &str) -> bool {
    stderr.contains("no such container")
        || stderr.contains("no container with name")
        // podman 5.x `inspect` phrasing: `Error: no such object: "name"`
        || stderr.contains("no such object")
        || stderr.contains("does not exist")
        || stderr.contains("not found")
}

fn podman_control_timeout(args: &[&str]) -> Duration {
    args.windows(2)
        .find_map(|pair| {
            (pair[0] == "-t")
                .then(|| pair[1].parse::<u64>().ok())
                .flatten()
        })
        .map(|secs| Duration::from_secs(secs.saturating_add(30)))
        .unwrap_or(PODMAN_CONTROL_TIMEOUT)
}

fn podman_start_timeout(container_name: &str) -> Duration {
    match container_name {
        "immich_server" | "netbird-server" => Duration::from_secs(120),
        _ => PODMAN_CONTROL_TIMEOUT,
    }
}

async fn podman_with_timeout(args: &[&str], timeout: Duration) -> Result<Output> {
    let mut cmd = tokio::process::Command::new("podman");
    cmd.args(args);
    command_with_timeout(cmd, timeout, &format!("podman {}", args.join(" "))).await
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
        .with_context(|| format!("Failed to exec {}", description))
}

async fn podman_start_container(container_name: &str) -> Result<Output> {
    if !runtime_host_ports(container_name).is_empty() {
        let mut cmd = tokio::process::Command::new("systemd-run");
        cmd.args([
            "--user",
            "--scope",
            "--quiet",
            "--collect",
            "podman",
            "start",
        ])
        .arg(container_name);
        let scoped = command_with_timeout(
            cmd,
            podman_start_timeout(container_name),
            &format!("systemd-run --user --scope podman start {container_name}"),
        )
        .await;
        if scoped.as_ref().is_ok_and(|out| out.status.success()) {
            return scoped;
        }
        if let Err(err) = &scoped {
            tracing::warn!(
                container = %container_name,
                error = %err,
                "scoped podman start failed; falling back to direct podman start"
            );
        }
    }
    podman_with_timeout(
        &["start", container_name],
        podman_start_timeout(container_name),
    )
    .await
}

async fn do_orchestrator_package_stop(
    orchestrator: &dyn crate::container::traits::ContainerOrchestrator,
    containers: &[String],
) -> Result<()> {
    let mut errors = Vec::new();
    for name in containers {
        match orchestrator.stop(name).await {
            Ok(()) => {}
            Err(e) if is_unknown_app_id_error(&e) => {
                if let Err(e) = do_package_stop(&[name.clone()]).await {
                    errors.push(format!("{}: {:#}", name, e));
                }
            }
            // A member whose container is already gone IS stopped: quadlet
            // stop removes the --rm container before the orchestrator's
            // defensive `podman stop` fallback probes it, and a stack member
            // can be absent outright (stopped earlier, or never revived).
            Err(e) if is_missing_container_error(&format!("{:#}", e)) => {
                tracing::debug!(container = %name, error = %e, "stop: container already absent — treating as stopped");
            }
            Err(e) => {
                tracing::error!(container = %name, error = %e, "orchestrator stop failed");
                errors.push(format!("{}: {:#}", name, e));
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Stop failed: {}", errors.join("; ")))
    }
}

fn orchestrator_app_id(package_id: &str) -> &str {
    match package_id {
        "electrs" | "mempool-electrs" => "electrumx",
        "home-assistant" => "homeassistant",
        _ => package_id,
    }
}

/// Member APP ids (start order) for an orchestrator-managed stack, or None
/// when there's no orchestrator / the id isn't a known stack. Lifecycle ops
/// on stacks must address members by app id so the orchestrator drives the
/// quadlet .service; addressing live container names falls through the
/// unknown-app-id fallback to raw podman stop/start, which races systemd's
/// --rm cleanup (the container is removed on stop, the start half then finds
/// nothing — immich restart, gate 2026-07-09).
fn orchestrator_stack_members(has_orchestrator: bool, package_id: &str) -> Option<Vec<String>> {
    if !has_orchestrator {
        return None;
    }
    let members = super::dependencies::stack_member_app_ids(package_id);
    if members.is_empty() {
        None
    } else {
        Some(members.iter().map(|s| s.to_string()).collect())
    }
}

/// See crate::app_ops::address_caching_dependents — shared with the
/// reconciler, which cascades after its own recreates.
fn address_caching_dependents(package_id: &str) -> &'static [&'static str] {
    crate::app_ops::address_caching_dependents(package_id)
}

/// After a backend start/restart lands, bounce its address-caching dependents
/// (see above). Runs inside the backend worker's op-lock scope and takes each
/// dependent's own op lock — lock order is always backend → dependent, and no
/// dependent op takes a backend lock, so the nesting cannot deadlock.
async fn cascade_restart_address_caching_dependents(
    orchestrator: Option<&Arc<dyn crate::container::traits::ContainerOrchestrator>>,
    state_manager: &crate::state::StateManager,
    data_dir: &std::path::Path,
    backend_id: &str,
) {
    for dep in address_caching_dependents(backend_id) {
        // A user-stopped or not-running dependent holds no live connection —
        // starting it here would override an explicit operator decision.
        if crate::crash_recovery::load_user_stopped(data_dir)
            .await
            .contains(*dep)
        {
            continue;
        }
        let running = match podman_control(&["ps", "--format", "{{.Names}}"]).await {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.trim() == *dep),
            _ => false,
        };
        if !running {
            continue;
        }
        install_log(&format!(
            "CASCADE RESTART: {dep} (backend {backend_id} restarted; {dep} caches its resolved address)"
        ))
        .await;
        let op_lock = app_op_lock(dep);
        let _op_guard = op_lock.lock().await;
        let prev = flip_package_state(state_manager, dep, PackageState::Restarting).await;
        let target = vec![dep.to_string()];
        let result = if let Some(orch) = orchestrator {
            do_orchestrator_package_restart(orch.as_ref(), &target).await
        } else {
            do_package_restart(&target).await
        };
        match result {
            Ok(()) => {
                reconcile_companions_for(dep).await;
                set_package_state(state_manager, dep, PackageState::Running).await;
            }
            Err(e) => {
                tracing::error!(dependent = %dep, backend = %backend_id, error = %e, "cascade restart failed");
                install_log(&format!("CASCADE RESTART FAIL: {dep} — {e:#}")).await;
                if let Some(prev) = prev {
                    set_package_state(state_manager, dep, prev).await;
                }
            }
        }
    }
}

/// Per-app lifecycle-operation locks. package.start/stop/restart reply
/// immediately and run their multi-container sequences in spawned tasks;
/// unserialized, back-to-back RPCs interleave those sequences (gate run D,
/// 2026-07-09: package.start's member bring-up raced the still-running
/// package.stop's member shutdown — archy-mempool-db was stopped 2 seconds
/// after it started and the mempool stack finished with zero containers).
/// Workers take the app's lock as their first await; tokio's Mutex is fair
/// (FIFO), so queued operations run in RPC arrival order and the final
/// state matches the last request.
/// Registry lives in crate::app_ops so background actors (reconciler) can
/// probe in-flight ops without an api ↔ container dependency cycle.
fn app_op_lock(package_id: &str) -> Arc<tokio::sync::Mutex<()>> {
    crate::app_ops::op_lock(orchestrator_app_id(package_id))
}

fn uses_single_orchestrator_app(package_id: &str) -> bool {
    startup_order(package_id).is_empty()
        && matches!(
            package_id,
            "bitcoin-ui"
                | "electrs-ui"
                | "lnd-ui"
                | "bitcoin-core"
                | "bitcoin-knots"
                | "lnd"
                | "fedimint"
                | "fedimint-gateway"
                | "filebrowser"
                | "electrumx"
                | "electrs"
                | "mempool-electrs"
                | "homeassistant"
                | "home-assistant"
                | "nextcloud"
                | "vaultwarden"
                | "jellyfin"
                | "photoprism"
                | "uptime-kuma"
                | "gitea"
                | "portainer"
                | "meshtastic"
                | "botfights"
        )
}

async fn do_orchestrator_package_restart(
    orchestrator: &dyn crate::container::traits::ContainerOrchestrator,
    to_restart: &[String],
) -> Result<()> {
    do_orchestrator_package_stop(orchestrator, to_restart).await?;
    do_orchestrator_package_start(orchestrator, to_restart).await
}

/// Stop all containers with their per-container graceful-shutdown timeout.
async fn do_package_stop(containers: &[String]) -> Result<()> {
    let mut errors = Vec::new();
    for name in containers {
        match inspect_runtime_container_state(name).await {
            Ok(None) => {
                tracing::debug!(container = %name, "container already absent during stop");
                continue;
            }
            Ok(Some(state)) if matches!(state.as_str(), "exited" | "stopped" | "configured") => {
                tracing::debug!(container = %name, state = %state, "container already stopped");
                continue;
            }
            Ok(Some(_)) => {}
            Err(e) => {
                tracing::warn!(container = %name, error = %e, "failed to inspect before stop")
            }
        }
        tracing::info!(
            "Stopping container: {} (timeout: {}s)",
            name,
            stop_timeout_secs(name)
        );
        let out = match podman_control(&["stop", "-t", stop_timeout_secs(name), name]).await {
            Ok(out) => out,
            Err(e) => {
                tracing::warn!(
                        container = %name,
                        error = %e,
                    "podman stop errored, trying force stop"
                );
                match force_stop_runtime_container(name).await {
                    Ok(()) => {
                        tracing::info!(container = %name, "force stop after stop error succeeded");
                        continue;
                    }
                    Err(force_err) => {
                        tracing::error!(
                            "Failed to stop {}: {}; force stop failed: {}",
                            name,
                            e,
                            force_err
                        );
                        errors.push(format!("{}: {}; force stop failed: {}", name, e, force_err));
                        continue;
                    }
                }
            }
        };
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            if is_missing_companion_ok(name, &stderr) {
                tracing::debug!(container = %name, "companion already absent during stop");
                continue;
            }
            tracing::warn!("Failed to stop {}: {}, trying force stop", name, stderr);
            match force_stop_runtime_container(name).await {
                Ok(()) => {
                    tracing::info!(container = %name, "force stop after stop failure succeeded")
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to stop {}: {}; force stop failed: {}",
                        name,
                        stderr,
                        e
                    );
                    errors.push(format!("{}: {}; force stop failed: {}", name, stderr, e));
                }
            }
        }
    }
    if !errors.is_empty() {
        return Err(anyhow::anyhow!("Stop failed: {}", errors.join("; ")));
    }
    Ok(())
}

/// Restart via `podman restart`, falling back to stop+start when restart
/// fails (rootless podman loopback issues).
async fn do_package_restart(containers: &[String]) -> Result<()> {
    let mut errors = Vec::new();
    for name in containers {
        tracing::info!("Restarting container: {}", name);
        repair_before_package_start(name).await;
        wait_before_package_start(name).await;
        let out = podman_control(&["restart", "-t", stop_timeout_secs(name), name])
            .await
            .context(format!("Failed to exec podman restart {}", name))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            if is_missing_companion_ok(name, &stderr) {
                tracing::debug!(container = %name, "companion absent during restart; reconcile will recreate it");
                continue;
            }
            tracing::warn!(
                "podman restart {} failed: {}, trying stop+start",
                name,
                stderr
            );
            // Fallback: stop then start
            let _ = podman_control(&["stop", "-t", stop_timeout_secs(name), name]).await;
            wait_before_package_start(name).await;
            let start_out = podman_start_container(name)
                .await
                .context(format!("Failed to exec podman start {}", name))?;
            if !start_out.status.success() {
                let start_err = String::from_utf8_lossy(&start_out.stderr)
                    .trim()
                    .to_string();
                cleanup_start_conflict(name, &start_err).await;
                if is_missing_companion_ok(name, &start_err) {
                    tracing::debug!(container = %name, "companion absent during restart fallback; reconcile will recreate it");
                    continue;
                }
                tracing::error!("stop+start {} also failed: {}", name, start_err);
                errors.push(format!("{}: {}", name, start_err));
            } else {
                tracing::info!("Restarted {} via stop+start fallback", name);
            }
        }
        ensure_runtime_host_port_listener(name).await?;
    }
    if containers.iter().any(|name| name == "indeedhub") {
        super::install::patch_indeedhub_nostr_provider().await;
    }
    if !errors.is_empty() {
        return Err(anyhow::anyhow!("Restart failed: {}", errors.join("; ")));
    }
    Ok(())
}

fn is_unknown_app_id_error(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|cause| cause.to_string().contains("unknown app_id"))
}

async fn repair_before_package_start(container_name: &str) {
    match container_name {
        "btcpay-server" | "archy-nbxplorer" => repair_btcpay_dirs().await,
        "indeedhub" => repair_indeedhub_network().await,
        "immich_server" => repair_immich_dirs().await,
        "netbird" => repair_netbird_network().await,
        "grafana" => {
            repair_grafana_dirs().await;
        }
        "nextcloud" => {
            repair_nextcloud_dirs().await;
        }
        "nginx-proxy-manager" => repair_nginx_proxy_manager_container().await,
        _ => {}
    }
    // Reap this app's ghost containers before anything tries to start it.
    // A ghost (process tree alive, podman record gone) still owns the
    // published port and the data-dir file locks, so the replacement either
    // fails to bind (`address already in use`) or starts and dies on the
    // lock — and `Restart=always` loops it there forever. Ordered before
    // the port cleanup below: killing the owner is what actually frees the
    // port, and the port sweep alone cannot tell a ghost from a live app.
    crate::container::ghost_reaper::reap_for_app(container_name).await;
    cleanup_runtime_host_ports(container_name).await;
}

async fn wait_before_package_start(container_name: &str) {
    match container_name {
        "indeedhub" => wait_for_indeedhub_dependency_dns().await,
        "immich_server" => wait_for_immich_dependencies().await,
        "netbird" => wait_for_netbird_dependency_dns().await,
        _ => {}
    }
}

async fn wait_for_indeedhub_dependency_dns() {
    for _ in 0..30 {
        if indeedhub_frontend_dependencies_running().await {
            super::stacks::repair_indeedhub_network_aliases().await;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    for _ in 0..30 {
        let ready = podman_with_timeout(
            &["exec", "indeedhub-minio", "getent", "hosts", "minio"],
            Duration::from_secs(5),
        )
        .await
        .map(|out| out.status.success())
        .unwrap_or(false);
        if ready {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn indeedhub_frontend_dependencies_running() -> bool {
    for container in ["indeedhub-minio", "indeedhub-redis", "indeedhub-api"] {
        if !container_is_running(container).await {
            return false;
        }
    }
    true
}

async fn container_is_running(container: &str) -> bool {
    let Ok(output) = podman_with_timeout(
        &["inspect", container, "--format", "{{.State.Status}}"],
        Duration::from_secs(5),
    )
    .await
    else {
        return false;
    };

    output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "running"
}

async fn wait_for_netbird_dependency_dns() {
    for _ in 0..30 {
        if container_is_running("netbird-server").await
            && container_is_running("netbird-dashboard").await
        {
            super::stacks::repair_netbird_network_aliases().await;
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            return;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn wait_for_immich_dependencies() {
    for _ in 0..60 {
        if immich_postgres_ready().await && immich_redis_ready().await {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn immich_postgres_ready() -> bool {
    if container_health_is_healthy("immich_postgres").await {
        return true;
    }

    let Ok(output) = podman_with_timeout(
        &[
            "exec",
            "immich_postgres",
            "pg_isready",
            "-U",
            "postgres",
            "-d",
            "immich",
        ],
        Duration::from_secs(5),
    )
    .await
    else {
        return false;
    };
    output.status.success()
}

async fn immich_redis_ready() -> bool {
    if container_health_is_healthy("immich_redis").await {
        return true;
    }

    let Ok(output) = podman_with_timeout(
        &["exec", "immich_redis", "valkey-cli", "ping"],
        Duration::from_secs(5),
    )
    .await
    else {
        return false;
    };
    output.status.success() && String::from_utf8_lossy(&output.stdout).contains("PONG")
}

async fn container_health_is_healthy(container: &str) -> bool {
    let Ok(output) = podman_with_timeout(
        &[
            "inspect",
            container,
            "--format",
            "{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}",
        ],
        Duration::from_secs(5),
    )
    .await
    else {
        return false;
    };

    output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "healthy"
}

async fn repair_netbird_network() {
    super::stacks::repair_netbird_network_aliases().await;
}

async fn repair_nginx_proxy_manager_container() {
    repair_nginx_proxy_manager_dirs().await;
    if !nginx_proxy_manager_has_legacy_admin_port().await {
        cleanup_nginx_proxy_manager_ports().await;
        return;
    }

    install_log(
        "START REPAIR: nginx-proxy-manager - recreating stale container using host port 8081",
    )
    .await;
    let _ = podman_control(&["rm", "-f", "nginx-proxy-manager"]).await;
    cleanup_nginx_proxy_manager_ports().await;
    if let Err(err) = recreate_nginx_proxy_manager_container().await {
        tracing::warn!(error = %err, "failed to recreate stale nginx-proxy-manager container");
    }
}

async fn repair_nginx_proxy_manager_dirs() {
    let _ = tokio::process::Command::new("sudo")
        .args([
            "mkdir",
            "-p",
            "/var/lib/archipelago/nginx-proxy-manager/data/letsencrypt-acme-challenge/.well-known/acme-challenge",
            "/var/lib/archipelago/nginx-proxy-manager/letsencrypt",
        ])
        .output()
        .await;
    let _ = tokio::process::Command::new("sudo")
        .args([
            "chown",
            "-R",
            "1000:1000",
            "/var/lib/archipelago/nginx-proxy-manager",
        ])
        .output()
        .await;
}

async fn nginx_proxy_manager_has_legacy_admin_port() -> bool {
    if let Ok(output) = podman_control(&["port", "nginx-proxy-manager", "81/tcp"]).await {
        if output.status.success()
            && String::from_utf8_lossy(&output.stdout).lines().any(|line| {
                line.rsplit(':')
                    .next()
                    .is_some_and(|port| port.trim() == "81")
            })
        {
            return true;
        }
    }

    let Ok(output) = podman_control(&[
        "ps",
        "-a",
        "--format",
        "{{.Ports}}",
        "--filter",
        "name=^nginx-proxy-manager$",
    ])
    .await
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let ports = String::from_utf8_lossy(&output.stdout);
    ports.contains(":81->81/tcp") || ports.contains(":8443->443/tcp")
}

async fn recreate_nginx_proxy_manager_container() -> Result<()> {
    tokio::process::Command::new("sudo")
        .args([
            "mkdir",
            "-p",
            "/var/lib/archipelago/nginx-proxy-manager/data/letsencrypt-acme-challenge/.well-known/acme-challenge",
            "/var/lib/archipelago/nginx-proxy-manager/letsencrypt",
        ])
        .output()
        .await
        .context("failed to create nginx-proxy-manager data directories")?;
    let _ = tokio::process::Command::new("sudo")
        .args([
            "chown",
            "-R",
            "1000:1000",
            "/var/lib/archipelago/nginx-proxy-manager",
        ])
        .output()
        .await;

    let image = crate::container::image_versions::pinned_image_for_app("nginx-proxy-manager")
        .unwrap_or_else(|| "docker.io/jc21/nginx-proxy-manager:latest".to_string());
    let mut args = vec![
        "run".to_string(),
        "-d".to_string(),
        "--name".to_string(),
        "nginx-proxy-manager".to_string(),
        "--restart=unless-stopped".to_string(),
        "--network=slirp4netns:allow_host_loopback=true".to_string(),
        "--cap-drop=ALL".to_string(),
        "--security-opt=no-new-privileges:true".to_string(),
        "--pids-limit=4096".to_string(),
    ];
    args.extend(get_app_capabilities("nginx-proxy-manager"));
    args.extend([
        "-p".to_string(),
        "8081:81".to_string(),
        "-p".to_string(),
        "8084:80".to_string(),
        "-p".to_string(),
        "8444:443".to_string(),
        "-v".to_string(),
        "/var/lib/archipelago/nginx-proxy-manager/data:/data".to_string(),
        "-v".to_string(),
        "/var/lib/archipelago/nginx-proxy-manager/letsencrypt:/etc/letsencrypt".to_string(),
        "--memory".to_string(),
        get_memory_limit("nginx-proxy-manager").to_string(),
        "--cpus=2".to_string(),
    ]);
    args.extend(get_health_check_args("nginx-proxy-manager", ""));
    args.push(image);

    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = podman_control(&refs).await?;
    if !output.status.success() {
        anyhow::bail!(
            "podman run nginx-proxy-manager failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

async fn ensure_runtime_host_port_listener(container_name: &str) -> Result<()> {
    let Some(port) = runtime_host_ports(container_name).into_iter().next() else {
        return Ok(());
    };

    if wait_for_runtime_host_port(container_name, port, 10).await {
        return Ok(());
    }

    install_log(&format!(
        "START REPAIR: {} — host port {} missing after start; restarting container",
        container_name, port
    ))
    .await;
    let output = podman_control(&["restart", container_name])
        .await
        .context("failed to restart container after missing host port")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "Container {} host port {} was not listening and restart failed: {}",
            container_name,
            port,
            stderr.trim()
        ));
    }

    if wait_for_runtime_host_port(container_name, port, 60).await {
        install_log(&format!(
            "START REPAIR OK: {} — host port {} is listening after restart",
            container_name, port
        ))
        .await;
        return Ok(());
    }

    Err(anyhow::anyhow!(
        "Container {} is running but host port {} is not listening",
        container_name,
        port
    ))
}

fn runtime_host_ports(container_name: &str) -> Vec<u16> {
    let manifest_ports = manifest_host_ports(container_name);
    if !manifest_ports.is_empty() {
        return with_legacy_extra_ports(container_name, manifest_ports);
    }

    let ports = match container_name {
        "grafana" => vec![3000],
        "homeassistant" | "home-assistant" => vec![8123],
        "jellyfin" => vec![8096],
        "searxng" => vec![8888],
        "uptime-kuma" => vec![3002],
        "vaultwarden" => vec![8082],
        "gitea" => vec![3001, 2222, 3000],
        "nextcloud" => vec![8085],
        "nginx-proxy-manager" => vec![8081, 8084, 8444],
        _ => Vec::new(),
    };
    ports
}

fn with_legacy_extra_ports(container_name: &str, mut ports: Vec<u16>) -> Vec<u16> {
    if container_name == "gitea" && !ports.contains(&3000) {
        ports.push(3000);
    }
    if container_name == "nginx-proxy-manager" {
        for port in [8084, 8444] {
            if !ports.contains(&port) {
                ports.push(port);
            }
        }
    }
    ports
}

fn manifest_host_ports(container_name: &str) -> Vec<u16> {
    for apps_dir in manifest_apps_dirs() {
        let Ok(entries) = std::fs::read_dir(apps_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path().join("manifest.yml");
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(manifest) = AppManifest::parse(&contents) else {
                continue;
            };
            if manifest_container_name(&manifest) == container_name {
                return manifest.app.ports.iter().map(|p| p.host).collect();
            }
        }
    }
    Vec::new()
}

pub(super) fn manifest_apps_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        dirs.push(Path::new(&manifest_dir).join("../../apps"));
    }
    dirs.extend([
        Path::new("apps").to_path_buf(),
        Path::new("/opt/archipelago/apps").to_path_buf(),
        Path::new("/opt/archipelago/web-ui/archipelago-runtime/apps").to_path_buf(),
    ]);
    dirs
}

fn manifest_container_name(manifest: &AppManifest) -> String {
    if let Some(v) = manifest.app.extensions.get("container_name") {
        if let Some(s) = v.as_str() {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    match manifest.app.id.as_str() {
        "bitcoin-ui" | "electrs-ui" | "lnd-ui" => format!("archy-{}", manifest.app.id),
        id => id.to_string(),
    }
}

async fn wait_for_runtime_host_port(container_name: &str, port: u16, timeout_secs: u64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        let ready = match container_name {
            "uptime-kuma" => http_host_port_ready(port, "/").await,
            _ => tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_ok(),
        };
        if ready {
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

async fn repair_btcpay_dirs() {
    let _ = tokio::process::Command::new("sudo")
        .args([
            "mkdir",
            "-p",
            "/var/lib/archipelago/btcpay/Main",
            "/var/lib/archipelago/nbxplorer/Main",
        ])
        .output()
        .await;
    for dir in [
        "/var/lib/archipelago/btcpay",
        "/var/lib/archipelago/nbxplorer",
    ] {
        let _ = tokio::process::Command::new("sudo")
            .args(["chown", "-R", "1000:1000", dir])
            .output()
            .await;
    }
    repair_btcpay_database_password().await;
}

async fn repair_grafana_dirs() {
    let _ = tokio::process::Command::new("sudo")
        .args(["mkdir", "-p", "/var/lib/archipelago/grafana"])
        .output()
        .await;
    let podman_chown = podman_control(&[
        "unshare",
        "chown",
        "-R",
        "472:472",
        "/var/lib/archipelago/grafana",
    ])
    .await;
    if !podman_chown.as_ref().is_ok_and(|o| o.status.success()) {
        let _ = tokio::process::Command::new("sudo")
            .args([
                "chown",
                "-R",
                "100471:100471",
                "/var/lib/archipelago/grafana",
            ])
            .output()
            .await;
    }
}

async fn repair_nextcloud_dirs() {
    let _ = tokio::process::Command::new("sudo")
        .args(["mkdir", "-p", "/var/lib/archipelago/nextcloud"])
        .output()
        .await;
    let podman_chown = podman_control(&[
        "unshare",
        "chown",
        "-R",
        "33:33",
        "/var/lib/archipelago/nextcloud",
    ])
    .await;
    if !podman_chown.as_ref().is_ok_and(|o| o.status.success()) {
        let _ = tokio::process::Command::new("sudo")
            .args([
                "chown",
                "-R",
                "100032:100032",
                "/var/lib/archipelago/nextcloud",
            ])
            .output()
            .await;
    }
}

async fn repair_immich_dirs() {
    let _ = tokio::process::Command::new("sudo")
        .args(["mkdir", "-p", "/var/lib/archipelago/immich"])
        .output()
        .await;
    let podman_chown = podman_control(&[
        "unshare",
        "chown",
        "-R",
        "0:0",
        "/var/lib/archipelago/immich",
    ])
    .await;
    if !podman_chown.as_ref().is_ok_and(|o| o.status.success()) {
        let _ = tokio::process::Command::new("sudo")
            .args(["chown", "-R", "1000:1000", "/var/lib/archipelago/immich"])
            .output()
            .await;
    }
}

async fn repair_btcpay_database_password() {
    let Ok(db_pass) =
        tokio::fs::read_to_string("/var/lib/archipelago/secrets/btcpay-db-password").await
    else {
        return;
    };
    let db_pass = db_pass.trim();
    if db_pass.is_empty() {
        return;
    }

    let _ = podman_control(&["start", "archy-btcpay-db"]).await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let escaped = db_pass.replace('\'', "''");
    let sql = format!("ALTER USER btcpay WITH PASSWORD '{}';", escaped);
    let _ = podman_control(&[
        "exec",
        "archy-btcpay-db",
        "psql",
        "-U",
        "btcpay",
        "-d",
        "btcpay",
        "-c",
        &sql,
    ])
    .await;
    let _ = podman_control(&[
        "exec",
        "archy-btcpay-db",
        "createdb",
        "-U",
        "btcpay",
        "nbxplorer",
    ])
    .await;
}

async fn repair_indeedhub_network() {
    super::stacks::repair_indeedhub_network_aliases().await;
}

async fn cleanup_start_conflict(container_name: &str, stderr: &str) {
    if !stderr.contains("address already in use") && !stderr.contains("pasta failed") {
        return;
    }

    let ports = runtime_host_ports(container_name);
    if !ports.is_empty() {
        cleanup_ports(&ports).await;
        return;
    }
}

async fn cleanup_runtime_host_ports(container_name: &str) {
    let ports = runtime_host_ports(container_name);
    if !ports.is_empty() {
        cleanup_ports(&ports).await;
    }
}

async fn cleanup_nginx_proxy_manager_ports() {
    cleanup_ports(&[8081, 8084, 8444]).await;
}

async fn cleanup_ports(ports: &[u16]) {
    for port in ports {
        cleanup_stale_pasta_port(&port.to_string()).await;
    }
}

async fn cleanup_stale_pasta_port(port: &str) {
    let kill_listener = format!(
        "ss -ltnp 'sport = :{}' 2>/dev/null | sed -n 's/.*pid=\\([0-9]*\\).*/\\1/p' | xargs -r kill 2>/dev/null || true",
        port
    );
    let _ = tokio::process::Command::new("sh")
        .args(["-c", &kill_listener])
        .output()
        .await;

    let pattern = format!("pasta.*{}", port);
    let _ = tokio::process::Command::new("pkill")
        .args(["-f", &pattern])
        .output()
        .await;
    let pattern = format!("rootlessport.*{}", port);
    let _ = tokio::process::Command::new("pkill")
        .args(["-f", &pattern])
        .output()
        .await;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}

pub(super) fn is_missing_companion_ok(name: &str, stderr: &str) -> bool {
    matches!(
        name,
        "archy-bitcoin-ui" | "archy-lnd-ui" | "archy-electrs-ui"
    ) && stderr.contains("no container with name or ID")
}

/// Flip the primary package entry's state and return the pre-transition
/// state for revert on error. Mirrors `transitional::flip_to_transitional`
/// but lives here because the package path keys by `package_id` (which may
/// differ from the container name used by orchestrator-level entries).
async fn flip_package_state(
    state_manager: &crate::state::StateManager,
    package_id: &str,
    transitional: PackageState,
) -> Option<PackageState> {
    let (mut data, _) = state_manager.get_snapshot().await;
    let prev = data.package_data.get(package_id).map(|e| e.state.clone());
    if let Some(entry) = data.package_data.get_mut(package_id) {
        entry.state = transitional;
        state_manager.update_data(data).await;
    }
    prev
}

/// Write the package entry's final state. No-op if the entry has since
/// been removed (uninstall race).
async fn set_package_state(
    state_manager: &crate::state::StateManager,
    package_id: &str,
    new_state: PackageState,
) {
    let (mut data, _) = state_manager.get_snapshot().await;
    if let Some(entry) = data.package_data.get_mut(package_id) {
        if entry.state != new_state {
            entry.state = new_state;
            state_manager.update_data(data).await;
        }
    }
}

pub(super) async fn reconcile_companions_for(package_id: &str) {
    let app_ids = match package_id {
        "bitcoin" | "bitcoin-core" => vec!["bitcoin-core".to_string(), "bitcoin-ui".to_string()],
        "bitcoin-knots" => vec!["bitcoin-knots".to_string(), "bitcoin-ui".to_string()],
        "lnd" => vec!["lnd".to_string(), "lnd-ui".to_string()],
        "electrumx" | "electrs" | "mempool-electrs" => {
            vec!["electrumx".to_string(), "electrs-ui".to_string()]
        }
        _ => return,
    };
    for (companion, err) in crate::container::companion::reconcile(&app_ids).await {
        tracing::warn!(companion = %companion, error = %err, "companion reconcile failed");
    }
}

pub(super) fn orchestrator_uninstall_app_ids(package_id: &str) -> Vec<String> {
    match package_id {
        "bitcoin" | "bitcoin-core" => vec!["bitcoin-core".into(), "bitcoin-ui".into()],
        "bitcoin-knots" => vec!["bitcoin-knots".into(), "bitcoin-ui".into()],
        "lnd" => vec!["lnd".into(), "lnd-ui".into()],
        "electrumx" | "electrs" | "mempool-electrs" => {
            vec!["electrumx".into(), "electrs-ui".into()]
        }
        "mempool" | "mempool-web" => vec![
            "mempool-api".into(),
            "archy-mempool-web".into(),
            "archy-mempool-db".into(),
        ],
        "btcpay-server" | "btcpayserver" | "btcpay" => vec![
            "btcpay-server".into(),
            "archy-nbxplorer".into(),
            "archy-btcpay-db".into(),
        ],
        "fedimint" => vec!["fedimint".into(), "fedimint-gateway".into()],
        // IndeedHub: 7-container stack; same rationale as immich below —
        // without this, uninstalling "indeedhub" leaves the six companions
        // enabled for the reconciler to resurrect forever.
        "indeedhub" => vec![
            "indeedhub-postgres".into(),
            "indeedhub-redis".into(),
            "indeedhub-minio".into(),
            "indeedhub-relay".into(),
            "indeedhub-api".into(),
            "indeedhub-ffmpeg".into(),
            "indeedhub".into(),
        ],
        // Immich: multi-container stack, mirrors `immich_stack_app_ids` in
        // stacks.rs. Without this, uninstalling "immich" only disabled the
        // orchestrator-tracked "immich" app_id — "immich-postgres" and
        // "immich-redis" stayed enabled, so the boot reconciler kept
        // restarting their leftover stopped containers forever after the
        // generic uninstall path stopped them (`.198`, 2026-07-01).
        "immich" => vec![
            "immich-postgres".into(),
            "immich-redis".into(),
            "immich".into(),
        ],
        _ => vec![package_id.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_container_classifier_covers_podman5_phrasings() {
        // Regression (.228 gate 2026-07-08): podman 5.x `inspect` on a missing
        // container says `Error: no such object: "mempool"` — the classifier
        // missed it, so a plain missing container surfaced as a hard inspect
        // error and package.start aborted instead of proceeding to recreate.
        assert!(is_missing_container_error(
            "Error: no such object: \"mempool\""
        ));
        assert!(is_missing_container_error(
            "Error: no such container mempool"
        ));
        assert!(is_missing_container_error(
            "Error: no container with name or id \"x\" found"
        ));
        assert!(!is_missing_container_error("Error: OCI runtime error"));
    }

    #[test]
    fn single_app_uninstall_targets_its_declarative_unit() {
        assert_eq!(orchestrator_uninstall_app_ids("cuprate"), vec!["cuprate"]);
    }

    #[test]
    fn runtime_host_ports_are_manifest_derived_for_public_apps() {
        assert_eq!(runtime_host_ports("photoprism"), vec![2342]);
        assert_eq!(runtime_host_ports("jellyfin"), vec![8096]);
        assert_eq!(runtime_host_ports("uptime-kuma"), vec![3002]);
    }

    #[test]
    fn runtime_host_ports_preserve_legacy_extra_ports() {
        assert_eq!(runtime_host_ports("gitea"), vec![3001, 2222, 3000]);
    }

    #[test]
    fn indeedhub_uninstall_covers_every_sibling_orchestrator_app_id() {
        let ids = orchestrator_uninstall_app_ids("indeedhub");
        for expected in [
            "indeedhub-postgres",
            "indeedhub-redis",
            "indeedhub-minio",
            "indeedhub-relay",
            "indeedhub-api",
            "indeedhub-ffmpeg",
            "indeedhub",
        ] {
            assert!(
                ids.iter().any(|id| id == expected),
                "missing {expected} in {ids:?}"
            );
        }
    }

    #[test]
    fn immich_uninstall_covers_every_sibling_orchestrator_app_id() {
        // Regression: uninstalling "immich" used to only disable the
        // "immich" app_id itself, leaving immich-postgres/immich-redis
        // enabled — the boot reconciler kept restarting their leftover
        // stopped containers forever (.198, 2026-07-01).
        let ids = orchestrator_uninstall_app_ids("immich");
        for expected in ["immich-postgres", "immich-redis", "immich"] {
            assert!(
                ids.iter().any(|id| id == expected),
                "missing {expected} in {ids:?}"
            );
        }
    }
}
