use super::package::validate_app_id;
use super::transitional::Op;
use super::RpcHandler;
use anyhow::{Context, Result};
use std::time::Duration;

const PODMAN_INSPECT_TIMEOUT: Duration = Duration::from_secs(5);
const PODMAN_PS_TIMEOUT: Duration = Duration::from_secs(5);
const ORCHESTRATOR_HEALTH_TIMEOUT: Duration = Duration::from_secs(5);

impl RpcHandler {
    pub(super) async fn handle_container_install(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // The `container-install { manifest_path }` RPC is a dev-mode convenience
        // that points at an arbitrary YAML on disk. Production install happens via
        // the reconciler (BootReconciler, Step 5) and via the unified
        // ContainerOrchestrator::install(app_id) trait call, which can be exposed
        // through a separate `container-install-by-id` RPC when needed.
        let dev = self.dev_orchestrator.as_ref().ok_or_else(|| {
            anyhow::anyhow!("container-install with manifest_path is only available in dev mode")
        })?;

        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let manifest_path = params
            .get("manifest_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing manifest_path"))?;

        // Validate manifest path: reject traversal, resolve to canonical path
        if manifest_path.contains("..") || manifest_path.contains('\0') {
            return Err(anyhow::anyhow!(
                "Invalid manifest_path: path traversal not allowed"
            ));
        }
        let apps_dir = self.config.data_dir.join("apps");
        let resolved = if std::path::Path::new(manifest_path).is_absolute() {
            std::path::PathBuf::from(manifest_path)
        } else {
            apps_dir.join(manifest_path)
        };
        let canonical = resolved
            .canonicalize()
            .context("Invalid manifest_path: file not found")?;
        if !canonical.starts_with(&apps_dir) {
            return Err(anyhow::anyhow!(
                "Invalid manifest_path: must be under the apps directory"
            ));
        }

        // Load manifest
        let manifest_content = tokio::fs::read_to_string(&canonical)
            .await
            .context("Failed to read manifest file")?;
        let manifest: archipelago_container::AppManifest =
            serde_yaml::from_str(&manifest_content).context("Failed to parse manifest")?;

        let container_name = dev
            .install_container(&manifest, manifest_path)
            .await
            .context("Failed to install container")?;

        Ok(serde_json::json!(container_name))
    }

    pub(super) async fn handle_container_start(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let app_id = params
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing app_id"))?;
        validate_app_id(app_id)?;

        // User explicitly started the app — clear the user-stopped marker so
        // crash recovery / health monitor won't second-guess it. Must happen
        // BEFORE the spawn (see runtime.rs:145-148 for the symmetric stop
        // side and the ordering contract crash recovery depends on).
        crate::crash_recovery::clear_user_stopped(&self.config.data_dir, app_id).await;

        // spawn_transitional returns as soon as the background task is
        // launched (<1s). The UI sees Starting… immediately via WebSocket.
        self.spawn_transitional(Op::Start, app_id.to_string())
            .await?;

        Ok(serde_json::json!({ "status": "starting" }))
    }

    pub(super) async fn handle_container_stop(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let app_id = params
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing app_id"))?;
        validate_app_id(app_id)?;

        // Mark as user-stopped BEFORE the spawn — ordering is load-bearing
        // (crash recovery / health monitor inspect this flag concurrently
        // with the in-flight stop; see runtime.rs:145-148 for the package
        // path that also writes this in the same order).
        crate::crash_recovery::mark_user_stopped(&self.config.data_dir, app_id).await;

        // podman stop -t 600 (bitcoin-core) / -t 330 (lnd) runs in the
        // background; the RPC returns now with "stopping".
        self.spawn_transitional(Op::Stop, app_id.to_string())
            .await?;

        Ok(serde_json::json!({ "status": "stopping" }))
    }

    pub(super) async fn handle_container_restart(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let app_id = params
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing app_id"))?;
        validate_app_id(app_id)?;

        // Restart does not mark user-stopped (the user wants the app to
        // keep running). Clear the marker as a defensive measure in case a
        // prior stop left it set and the restart is intended to revive the
        // normal running state.
        crate::crash_recovery::clear_user_stopped(&self.config.data_dir, app_id).await;

        self.spawn_transitional(Op::Restart, app_id.to_string())
            .await?;

        Ok(serde_json::json!({ "status": "restarting" }))
    }

    pub(super) async fn handle_container_remove(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let orchestrator = self
            .orchestrator
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Container orchestrator not available"))?;

        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let app_id = params
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing app_id"))?;
        validate_app_id(app_id)?;
        let preserve_data = params
            .get("preserve_data")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        orchestrator
            .remove(app_id, preserve_data)
            .await
            .context("Failed to remove container")?;

        Ok(serde_json::json!({ "status": "removed" }))
    }

    pub(super) async fn handle_container_list(&self) -> Result<serde_json::Value> {
        // Use the scanner's cached state for consistency with WebSocket updates.
        // This prevents the container-list RPC from returning different results
        // than the WebSocket-delivered package_data, which caused apps to flicker
        // between "installed" and "not-installed" in the UI.
        let (data, _) = self.state_manager.get_snapshot().await;
        // Apps the user explicitly stopped must read as "stopped" even though a
        // UI companion (electrs-ui, bitcoin-ui, …) keeps serving the launch port:
        // launch_port_reachable() below would otherwise upgrade an exited backend
        // back to "running". The reconcile guard keeps these backends down, so the
        // marker is authoritative here.
        let user_stopped = crate::crash_recovery::load_user_stopped(&self.config.data_dir).await;
        if data.server_info.status_info.containers_scanned && !data.package_data.is_empty() {
            let mut containers = Vec::with_capacity(data.package_data.len());
            for (id, pkg) in &data.package_data {
                // Keep this mapping in sync with the UI's
                // ContainerStatus.state union in
                // neode-ui/src/api/container-client.ts. The UI maps
                // transitional variants to single-button labels
                // (Stopping… / Starting… / Restarting…).
                let mut state = match &pkg.state {
                    crate::data_model::PackageState::Running => "running".to_string(),
                    crate::data_model::PackageState::Stopped => "stopped".to_string(),
                    crate::data_model::PackageState::Exited => "exited".to_string(),
                    crate::data_model::PackageState::Starting => "starting".to_string(),
                    crate::data_model::PackageState::Stopping => "stopping".to_string(),
                    crate::data_model::PackageState::Restarting => "restarting".to_string(),
                    crate::data_model::PackageState::Installing => "installing".to_string(),
                    crate::data_model::PackageState::Installed => "installed".to_string(),
                    crate::data_model::PackageState::Updating => "updating".to_string(),
                    crate::data_model::PackageState::Removing => "removing".to_string(),
                    crate::data_model::PackageState::CreatingBackup => {
                        "creating-backup".to_string()
                    }
                    crate::data_model::PackageState::RestoringBackup => {
                        "restoring-backup".to_string()
                    }
                    crate::data_model::PackageState::BackingUp => "backing-up".to_string(),
                };

                // Scanner backoff preserves cached package_data. Refresh stable
                // states so callers do not see stale `running`/`exited` after
                // health-monitor recovery or Quadlet --rm container removal.
                if user_stopped.contains(id) {
                    // User stopped it → authoritative "stopped". Do NOT let a
                    // still-running UI companion's launch port mark it running.
                    state = "stopped".to_string();
                } else if state == "running" && requires_launch_port_for_health(id) {
                    if !self.cached_reachable_health(id).await?.is_some() {
                        state = live_state_for_app(id)
                            .await
                            .unwrap_or("starting".to_string());
                    }
                } else if should_refresh_cached_state(&state) {
                    if launch_port_reachable(id).await {
                        state = "running".to_string();
                    } else {
                        if let Some(live) = live_state_for_app(id).await {
                            state = live;
                        } else if quadlet_service_active(id).await {
                            state = "starting".to_string();
                        }
                    }
                }

                let lan = pkg
                    .installed
                    .as_ref()
                    .and_then(|i| i.interface_addresses.get("main"))
                    .and_then(|a| a.lan_address.as_deref());
                containers.push(serde_json::json!({
                    "id": id,
                    "name": id,
                    "state": state,
                    "image": "",
                    "created": "",
                    "ports": [],
                    "lan_address": lan,
                }));
            }
            return Ok(serde_json::json!(containers));
        }

        // Fallback: scanner hasn't run yet, query the orchestrator directly.
        if let Some(orchestrator) = &self.orchestrator {
            if let Ok(containers) = orchestrator.list().await {
                if !containers.is_empty() {
                    return Ok(serde_json::to_value(containers)?);
                }
            }
        }

        let output = tokio::process::Command::new("podman")
            .args(["ps", "-a", "--format", "json"])
            .output()
            .await
            .context("Failed to list containers via podman")?;

        if !output.status.success() {
            return Ok(serde_json::json!([]));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return Ok(serde_json::json!([]));
        }

        let podman_containers: Vec<serde_json::Value> =
            serde_json::from_str(&stdout).unwrap_or_else(|_| Vec::new());

        let containers: Vec<serde_json::Value> = podman_containers
            .iter()
            .map(|c| {
                let state = c.get("State").and_then(|v| v.as_str()).unwrap_or("unknown");
                let mapped_state = match state.to_lowercase().as_str() {
                    "running" => "running",
                    "exited" => "exited",
                    "stopped" => "stopped",
                    "created" => "created",
                    "paused" => "paused",
                    _ => "unknown",
                };
                let name = c
                    .get("Names")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let ports: Vec<String> = c
                    .get("Ports")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|p| {
                                let host = p.get("host_port").and_then(|v| v.as_u64())?;
                                let container = p.get("container_port").and_then(|v| v.as_u64())?;
                                let proto =
                                    p.get("protocol").and_then(|v| v.as_str()).unwrap_or("tcp");
                                Some(format!("0.0.0.0:{}->{}/{}", host, container, proto))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                serde_json::json!({
                    "id": c.get("Id").and_then(|v| v.as_str()).unwrap_or(""),
                    "name": name,
                    "state": mapped_state,
                    "image": c.get("Image").and_then(|v| v.as_str()).unwrap_or(""),
                    "created": c.get("Created").and_then(|v| v.as_str()).unwrap_or(""),
                    "ports": ports,
                    "lan_address": serde_json::Value::Null,
                })
            })
            .collect();

        Ok(serde_json::json!(containers))
    }

    pub(super) async fn handle_container_status(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let orchestrator = self
            .orchestrator
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Container orchestrator not available"))?;

        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let app_id = params
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing app_id"))?;
        validate_app_id(app_id)?;

        let mut last_err: Option<anyhow::Error> = None;
        for candidate in status_app_id_candidates(app_id) {
            match orchestrator.status(&candidate).await {
                Ok(status) => return Ok(serde_json::to_value(status)?),
                Err(e) => last_err = Some(e),
            }
        }

        // Fallback for alias drift: query podman directly by likely container
        // names so status checks stay useful during migration.
        for name in status_container_name_candidates(app_id) {
            if let Some(v) = inspect_container_state_value(&name).await {
                return Ok(v);
            }
        }

        if let Some(e) = last_err {
            return Err(e.context("Failed to get container status"));
        }
        Err(anyhow::anyhow!("Failed to get container status"))
    }

    pub(super) async fn handle_container_logs(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let orchestrator = self
            .orchestrator
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Container orchestrator not available"))?;

        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let app_id = params
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing app_id"))?;
        validate_app_id(app_id)?;
        let lines = params.get("lines").and_then(|v| v.as_u64()).unwrap_or(100) as u32;

        let logs = orchestrator
            .logs(app_id, lines)
            .await
            .context("Failed to get container logs")?;

        Ok(serde_json::to_value(logs)?)
    }

    /// Used by HTTP GET /api/container/logs (same logic as container-logs RPC).
    pub async fn get_container_logs_value(
        &self,
        app_id: &str,
        lines: u32,
    ) -> Result<serde_json::Value> {
        let orchestrator = self
            .orchestrator
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Container orchestrator not available"))?;

        let logs = orchestrator
            .logs(app_id, lines)
            .await
            .context("Failed to get container logs")?;

        Ok(serde_json::to_value(logs)?)
    }

    pub(super) async fn handle_container_health(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let orchestrator = self
            .orchestrator
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Container orchestrator not available"))?;

        // If app_id is provided, get health for that app.
        if let Some(params) = params {
            if let Some(app_id) = params.get("app_id").and_then(|v| v.as_str()) {
                if let Some(health) = self.cached_reachable_health(app_id).await? {
                    return Ok(serde_json::json!({ app_id: health }));
                }

                if let Some(health) = self.cached_state_health(app_id).await {
                    return Ok(serde_json::json!({ app_id: health }));
                }

                if requires_launch_port_for_health(app_id) {
                    return Ok(serde_json::json!({ app_id: "starting" }));
                }

                if let Some(health) = self.stack_health(app_id).await? {
                    return Ok(serde_json::json!({ app_id: health }));
                }

                let mut last_err: Option<anyhow::Error> = None;
                for candidate in status_app_id_candidates(app_id) {
                    match tokio::time::timeout(
                        ORCHESTRATOR_HEALTH_TIMEOUT,
                        orchestrator.health(&candidate),
                    )
                    .await
                    {
                        Ok(Ok(health)) => return Ok(serde_json::json!({ app_id: health })),
                        Ok(Err(e)) => last_err = Some(e),
                        Err(_) => {}
                    }
                }
                for name in status_container_name_candidates(app_id) {
                    if let Some(health) = inspect_container_health_value(&name).await {
                        return Ok(serde_json::json!({ app_id: health }));
                    }
                }
                if let Some(e) = last_err {
                    return Err(e.context("Failed to get container health"));
                }
                return Err(anyhow::anyhow!("Failed to get container health"));
            }
        }

        // Otherwise, get health for all containers.
        let containers = orchestrator
            .list()
            .await
            .context("Failed to list containers")?;

        let mut health_map = serde_json::Map::new();
        for container in containers {
            // Map the runtime container name back to the app_id the orchestrator
            // knows about. Dev orchestrator uses `archipelago-<id>-dev`; Prod
            // uses bare `<id>` (or `archy-<id>` for UIs — health() accepts the
            // app_id either way since UI_APP_IDS is centralised).
            let app_id_candidate = container
                .name
                .strip_prefix("archipelago-")
                .and_then(|s| s.strip_suffix("-dev"))
                .or_else(|| container.name.strip_prefix("archy-"))
                .unwrap_or(container.name.as_str());
            match tokio::time::timeout(
                ORCHESTRATOR_HEALTH_TIMEOUT,
                orchestrator.health(app_id_candidate),
            )
            .await
            {
                Ok(Ok(health)) => {
                    health_map.insert(
                        app_id_candidate.to_string(),
                        serde_json::Value::String(health),
                    );
                }
                Ok(Err(_)) | Err(_) => {
                    health_map.insert(
                        app_id_candidate.to_string(),
                        serde_json::Value::String("unknown".to_string()),
                    );
                }
            }
        }

        Ok(serde_json::Value::Object(health_map))
    }

    async fn cached_state_health(&self, app_id: &str) -> Option<&'static str> {
        let (data, _) = self.state_manager.get_snapshot().await;
        let Some(pkg) = data.package_data.get(app_id) else {
            if data.server_info.status_info.containers_scanned {
                return Some("stopped");
            }
            return None;
        };
        match pkg.state {
            crate::data_model::PackageState::Running => None,
            crate::data_model::PackageState::Installing
            | crate::data_model::PackageState::Installed
            | crate::data_model::PackageState::Starting => Some("starting"),
            crate::data_model::PackageState::Stopping
            | crate::data_model::PackageState::Stopped
            | crate::data_model::PackageState::Exited => Some("stopped"),
            crate::data_model::PackageState::Removing => Some("removing"),
            crate::data_model::PackageState::Restarting
            | crate::data_model::PackageState::Updating
            | crate::data_model::PackageState::CreatingBackup
            | crate::data_model::PackageState::RestoringBackup
            | crate::data_model::PackageState::BackingUp => Some("starting"),
        }
    }

    async fn cached_reachable_health(&self, app_id: &str) -> Result<Option<String>> {
        let (data, _) = self.state_manager.get_snapshot().await;
        let pkg = data.package_data.get(app_id);
        if matches!(
            pkg.map(|pkg| &pkg.state),
            Some(crate::data_model::PackageState::Removing)
        ) {
            return Ok(None);
        }

        let url = pkg
            .and_then(|pkg| pkg.installed.as_ref())
            .and_then(|i| i.interface_addresses.get("main"))
            .and_then(|a| a.lan_address.as_deref())
            .map(ToOwned::to_owned)
            .or_else(|| health_probe_url_for_app(app_id));

        let Some(url) = url else {
            return Ok(None);
        };
        if url.starts_with("http://") || url.starts_with("https://") {
            return Ok(http_launch_url_reachable(&url)
                .await
                .then(|| "healthy".to_string()));
        }

        let Some(port) = port_from_url(&url) else {
            return Ok(None);
        };
        Ok(launch_port_reachable_by_port(port)
            .await
            .then(|| "healthy".to_string()))
    }

    async fn stack_health(&self, app_id: &str) -> Result<Option<String>> {
        let Some(members) = stack_health_members(app_id) else {
            return Ok(None);
        };

        let orchestrator = self
            .orchestrator
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Container orchestrator not available"))?;

        let mut saw_starting = false;
        let mut saw_unknown = false;
        for member in members {
            match member_health(orchestrator.as_ref(), member)
                .await
                .as_deref()
            {
                Ok(health) if health == "healthy" => {}
                Ok(health) if health == "starting" => saw_starting = true,
                Ok(health) if health == "unknown" => saw_unknown = true,
                Ok(_) => return Ok(Some("unhealthy".to_string())),
                Err(_) => saw_unknown = true,
            }
        }

        if saw_unknown {
            if let Some(health) = self.cached_reachable_health(app_id).await? {
                return Ok(Some(health));
            }
            Ok(Some("unknown".to_string()))
        } else if saw_starting {
            if let Some(health) = self.cached_reachable_health(app_id).await? {
                return Ok(Some(health));
            }
            Ok(Some("starting".to_string()))
        } else {
            Ok(Some("healthy".to_string()))
        }
    }
}

async fn member_health(
    orchestrator: &dyn crate::container::traits::ContainerOrchestrator,
    app_id: &str,
) -> Result<String> {
    if let Ok(Ok(health)) =
        tokio::time::timeout(ORCHESTRATOR_HEALTH_TIMEOUT, orchestrator.health(app_id)).await
    {
        return Ok(health);
    }
    for name in status_container_name_candidates(app_id) {
        if let Some(health) = inspect_container_health_value(&name).await {
            return Ok(health);
        }
    }
    Ok("unknown".to_string())
}

fn stack_health_members(app_id: &str) -> Option<&'static [&'static str]> {
    match app_id {
        "mempool" | "mempool-web" => {
            Some(&["archy-mempool-db", "mempool-api", "archy-mempool-web"])
        }
        "btcpay-server" | "btcpayserver" | "btcpay" => {
            Some(&["archy-btcpay-db", "archy-nbxplorer", "btcpay-server"])
        }
        "immich" => Some(&["immich_postgres", "immich_redis", "immich_server"]),
        "indeedhub" => Some(&[
            "indeedhub-postgres",
            "indeedhub-redis",
            "indeedhub-minio",
            "indeedhub-relay",
            "indeedhub-api",
            "indeedhub",
        ]),
        _ => None,
    }
}

fn status_app_id_candidates(app_id: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |s: &str| {
        if !out.iter().any(|e: &String| e == s) {
            out.push(s.to_string());
        }
    };

    match app_id {
        "bitcoin-knots" => {
            push("bitcoin-knots");
            push("bitcoin-core");
            push("bitcoin");
        }
        "bitcoin-core" | "bitcoin" => {
            push("bitcoin-core");
            push("bitcoin-knots");
            push("bitcoin");
        }
        "electrs" | "mempool-electrs" => {
            push("electrs");
            push("mempool-electrs");
            push("electrumx");
        }
        "mempool" | "mempool-web" => {
            push("mempool");
            push("archy-mempool-web");
        }
        "immich" => {
            push("immich");
            push("immich_server");
        }
        _ => push(app_id),
    }

    out
}

fn status_container_name_candidates(app_id: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |s: &str| {
        if !out.iter().any(|e: &String| e == s) {
            out.push(s.to_string());
        }
    };

    match app_id {
        "bitcoin-knots" | "bitcoin-core" | "bitcoin" => push("bitcoin-knots"),
        "bitcoin-ui" => push("archy-bitcoin-ui"),
        "lnd-ui" => push("archy-lnd-ui"),
        "electrs-ui" => push("archy-electrs-ui"),
        "electrs" | "mempool-electrs" => push("electrumx"),
        "mempool" | "mempool-web" | "archy-mempool-web" => push("mempool"),
        "immich" => push("immich_server"),
        _ => {}
    }

    push(app_id);
    if let Some(stripped) = app_id.strip_prefix("archy-") {
        push(stripped);
    } else {
        push(&format!("archy-{}", app_id));
    }

    out
}

fn should_refresh_cached_state(state: &str) -> bool {
    matches!(state, "exited" | "stopped" | "stopping")
}

async fn live_state_for_app(app_id: &str) -> Option<String> {
    for name in status_container_name_candidates(app_id) {
        if let Some(live) = inspect_container_state_value(&name).await {
            if let Some(live_state) = live.get("state").and_then(|v| v.as_str()) {
                return Some(live_state.to_string());
            }
        }
    }
    None
}

async fn quadlet_service_active(app_id: &str) -> bool {
    for name in status_container_name_candidates(app_id) {
        let service = format!("{name}.service");
        let mut cmd = tokio::process::Command::new("systemctl");
        cmd.args(["--user", "is-active", "--quiet", &service]);
        cmd.kill_on_drop(true);
        if matches!(
            tokio::time::timeout(Duration::from_secs(2), cmd.status()).await,
            Ok(Ok(status)) if status.success()
        ) {
            return true;
        }
    }
    false
}

fn health_probe_url_for_app(app_id: &str) -> Option<String> {
    let port = match app_id {
        "bitcoin-ui" => 8334,
        "botfights" => 9100,
        "btcpay-server" | "btcpay" | "btcpayserver" => 23000,
        "electrumx" | "electrs" | "mempool-electrs" | "electrs-ui" => 50002,
        "fedimint" | "fedimintd" => 8175,
        "filebrowser" => 8083,
        "gitea" => 3001,
        "grafana" => 3000,
        "homeassistant" | "home-assistant" => 8123,
        "immich" | "immich_server" => 2283,
        "indeedhub" => 7778,
        "jellyfin" => 8096,
        "lnd" | "lnd-ui" => 18083,
        "mempool" | "mempool-web" => 4080,
        "nginx-proxy-manager" => 8081,
        "ollama" => 11434,
        "photoprism" => 2342,
        "portainer" => 9000,
        "searxng" => 8888,
        "tailscale" => 8240,
        "uptime-kuma" => 3002,
        "vaultwarden" => 8082,
        _ => return None,
    };
    Some(format!("http://localhost:{port}"))
}

fn requires_launch_port_for_health(app_id: &str) -> bool {
    matches!(app_id, "fedimint" | "fedimintd" | "fedimint-gateway")
}

async fn launch_port_reachable(app_id: &str) -> bool {
    let Some(port) = health_probe_url_for_app(app_id).and_then(|url| port_from_url(&url)) else {
        return false;
    };
    launch_port_reachable_by_port(port).await
}

async fn launch_port_reachable_by_port(port: u16) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            tokio::net::TcpStream::connect(("127.0.0.1", port)),
        )
        .await,
        Ok(Ok(_))
    )
}

async fn http_launch_url_reachable(url: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return false;
    };
    match client.get(url).send().await {
        Ok(response) => {
            let status = response.status();
            status.is_success() || status.is_redirection()
        }
        Err(_) => false,
    }
}

pub(in crate::api::rpc) fn port_from_url(url: &str) -> Option<u16> {
    let after_colon = url.rsplit_once(':')?.1;
    let port = after_colon
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    port.parse::<u16>().ok()
}

async fn inspect_container_state_value(name: &str) -> Option<serde_json::Value> {
    if let Some(v) = ps_container_state_value(name).await {
        return Some(v);
    }

    let mut cmd = tokio::process::Command::new("podman");
    cmd.args([
        "inspect",
        name,
        "--format",
        "{{.State.Status}} {{.State.Running}} {{if .State.Healthcheck}}{{.State.Healthcheck.Status}}{{else}}none{{end}}",
    ]);
    cmd.kill_on_drop(true);
    let out = tokio::time::timeout(PODMAN_INSPECT_TIMEOUT, cmd.output())
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }

    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if line.is_empty() {
        return None;
    }
    let mut parts = line.split_whitespace();
    let status = parts.next().unwrap_or("unknown");
    let running = parts.next().unwrap_or("false") == "true";
    let health = parts.next().unwrap_or("none");
    Some(serde_json::json!({
        "name": name,
        "status": status,
        "state": status,
        "running": running,
        "health": health,
    }))
}

async fn ps_container_state_value(name: &str) -> Option<serde_json::Value> {
    let mut cmd = tokio::process::Command::new("podman");
    cmd.args([
        "ps",
        "-a",
        "--filter",
        &format!("name={name}"),
        "--format",
        "{{.Names}}|{{.Status}}",
    ]);
    cmd.kill_on_drop(true);
    let out = tokio::time::timeout(PODMAN_PS_TIMEOUT, cmd.output())
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        let mut parts = line.splitn(2, '|');
        let container_name = parts.next().unwrap_or_default();
        if container_name != name {
            continue;
        }
        let status = parts.next().unwrap_or_default();
        let state = state_from_podman_status(status);
        let health = parse_health_from_status(status).unwrap_or("none");
        return Some(serde_json::json!({
            "name": name,
            "status": state,
            "state": state,
            "running": state.eq_ignore_ascii_case("running"),
            "health": health,
        }));
    }
    None
}

fn state_from_podman_status(status: &str) -> &str {
    if status.starts_with("Up ") {
        "running"
    } else if status.starts_with("Exited ") {
        "exited"
    } else if status.starts_with("Created") {
        "created"
    } else if status.starts_with("Stopping") {
        "stopping"
    } else if status.starts_with("Removing") {
        "removing"
    } else {
        "unknown"
    }
}

fn parse_health_from_status(status: &str) -> Option<&str> {
    let start = status.rfind('(')?;
    let end = status.rfind(')')?;
    (start < end).then(|| &status[start + 1..end])
}

async fn inspect_container_health_value(name: &str) -> Option<String> {
    let v = inspect_container_state_value(name).await?;
    if let Some(health) = v.get("health").and_then(|s| s.as_str()) {
        if health != "none" {
            return Some(health.to_string());
        }
    }
    match v.get("state").and_then(|s| s.as_str()).unwrap_or("unknown") {
        "running" => Some("healthy".to_string()),
        "created" => Some("starting".to_string()),
        "paused" => Some("paused".to_string()),
        "stopping" => Some("unhealthy".to_string()),
        "exited" | "stopped" => Some("unhealthy".to_string()),
        other => Some(format!("unknown:{other}")),
    }
}
