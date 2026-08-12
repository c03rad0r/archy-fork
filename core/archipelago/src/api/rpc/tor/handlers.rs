use super::*;
use crate::api::rpc::RpcHandler;
use std::time::{SystemTime, UNIX_EPOCH};

impl RpcHandler {
    /// List all configured hidden services with their .onion addresses.
    /// Services for known-but-uninstalled apps are hidden (issue #79).
    pub(in crate::api::rpc) async fn handle_tor_list_services(&self) -> Result<serde_json::Value> {
        let config_dir = self.config.data_dir.join("tor-config");
        let (data, _) = self.state_manager.get_snapshot().await;
        let mut apps = AppInstallState {
            known: Default::default(),
            installed: Default::default(),
        };
        for (id, pkg) in &data.package_data {
            apps.known.insert(id.clone());
            if pkg.installed.is_some() {
                apps.installed.insert(id.clone());
            }
        }
        let services = list_services(&config_dir, Some(&apps)).await?;
        let tor_running = check_tor_running().await;
        Ok(serde_json::json!({ "services": services, "tor_running": tor_running }))
    }

    /// Create a new hidden service for a given local port.
    pub(in crate::api::rpc) async fn handle_tor_create_service(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing name"))?;
        let raw_port = params
            .get("local_port")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u16;
        let remote_port = params
            .get("remote_port")
            .and_then(|v| v.as_u64())
            .map(|v| v as u16);

        validate_service_name(name)?;

        let local_port = if raw_port == 0 {
            self.resolve_app_local_port(name).await.ok_or_else(|| {
                anyhow::anyhow!(
                    "No local web port found for '{}' — the app isn't running or exposes no UI port; specify local_port manually",
                    name
                )
            })?
        } else {
            raw_port
        };

        let config_dir = self.config.data_dir.join("tor-config");
        let mut config = load_services_config(&config_dir).await;
        if config.services.iter().any(|s| s.name == name) {
            return Err(anyhow::anyhow!("Service '{}' already exists", name));
        }

        let is_proto = is_protocol_service(name);
        config.services.push(TorServiceEntry {
            name: name.to_string(),
            local_port,
            remote_port,
            unauthenticated: is_proto,
            enabled: true,
        });
        save_services_config(&config_dir, &config).await?;

        regenerate_torrc(&config).await?;
        restart_tor().await?;

        let onion = wait_for_hostname(name, 60).await;
        if let Some(ref addr) = onion {
            sync_single_hostname(name, addr).await;
        }

        info!(
            service = name,
            port = local_port,
            "Created Tor hidden service"
        );
        Ok(serde_json::json!({
            "created": true,
            "name": name,
            "onion_address": onion,
        }))
    }

    /// Delete a hidden service.
    pub(in crate::api::rpc) async fn handle_tor_delete_service(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing name"))?;

        validate_service_name(name)?;

        if name == "archipelago" {
            return Err(anyhow::anyhow!("Cannot delete the node's own Tor service"));
        }

        let config_dir = self.config.data_dir.join("tor-config");
        let mut config = load_services_config(&config_dir).await;
        let before = config.services.len();
        config.services.retain(|s| s.name != name);
        if config.services.len() == before {
            return Err(anyhow::anyhow!("Service '{}' not found", name));
        }
        save_services_config(&config_dir, &config).await?;

        delete_hidden_service_dir(name).await;

        regenerate_torrc(&config).await?;
        restart_tor().await?;

        info!(service = name, "Deleted Tor hidden service");
        Ok(serde_json::json!({ "deleted": true, "name": name }))
    }

    /// Get the .onion address for a specific service.
    pub(in crate::api::rpc) async fn handle_tor_get_onion_address(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing name"))?;

        validate_service_name(name)?;

        let onion = read_onion_address(name).await;
        Ok(serde_json::json!({ "name": name, "onion_address": onion }))
    }

    /// Rotate a hidden service's .onion address by generating a new keypair.
    pub(in crate::api::rpc) async fn handle_tor_rotate_service(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing name"))?;

        validate_service_name(name)?;

        let old_onion = read_onion_address(name).await;
        if old_onion.is_none() {
            return Err(anyhow::anyhow!(
                "Service '{}' has no .onion address to rotate",
                name
            ));
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        rename_hidden_service_dir(name, timestamp).await;

        info!(
            service = name,
            old_onion = ?old_onion,
            "Renamed old Tor service dir — restarting Tor to generate new keypair"
        );

        restart_tor().await?;

        let new_onion = wait_for_hostname(name, 60).await;

        if let Some(ref new_addr) = new_onion {
            sync_single_hostname(name, new_addr).await;
        }

        let old_name = format!("{}_old_{}", name, timestamp);
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
            info!(old_dir = %old_name, "Transition period elapsed — deleting old Tor service dir");
            delete_hidden_service_dir(&old_name).await;
        });

        if let Some(ref new_addr) = new_onion {
            let data_dir = self.config.data_dir.clone();
            let tor_proxy = self.config.nostr_tor_proxy.clone();
            let new_addr_clone = new_addr.clone();
            let old_onion_clone = old_onion.clone();
            tokio::spawn(async move {
                notify_federation_peers_address_change(
                    &data_dir,
                    &new_addr_clone,
                    old_onion_clone.as_deref(),
                    tor_proxy.as_deref(),
                )
                .await;
            });
        }

        Ok(serde_json::json!({
            "rotated": true,
            "name": name,
            "old_onion": old_onion,
            "new_onion": new_onion,
        }))
    }

    /// Clean up expired rotated service directories past the transition period.
    pub(in crate::api::rpc) async fn handle_tor_cleanup_rotated(
        &self,
    ) -> Result<serde_json::Value> {
        let base = detect_hidden_service_base();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut cleaned = Vec::new();
        if let Ok(mut entries) = tokio::fs::read_dir(&base).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.contains("_old_") {
                    continue;
                }
                if let Some(ts_str) = name.rsplit('_').next() {
                    if let Ok(ts) = ts_str.parse::<u64>() {
                        if now - ts > ROTATION_TRANSITION_SECS {
                            let path = entry.path();
                            let status = tokio::process::Command::new("sudo")
                                .args(["rm", "-rf", &path.to_string_lossy()])
                                .status()
                                .await;
                            if status.map(|s| s.success()).unwrap_or(false) {
                                info!(dir = %name, "Cleaned up expired rotated Tor service");
                                cleaned.push(name);
                            } else {
                                warn!(dir = %name, "Failed to clean up rotated Tor service");
                            }
                        }
                    }
                }
            }
        }

        Ok(serde_json::json!({ "cleaned": cleaned, "count": cleaned.len() }))
    }

    /// Toggle Tor access for a specific app (enable/disable).
    pub(in crate::api::rpc) async fn handle_tor_toggle_app(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let app_id = params
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing app_id"))?;

        validate_service_name(app_id)?;

        let enabled = params
            .get("enabled")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("Missing enabled (bool)"))?;

        let config_dir = self.config.data_dir.join("tor-config");
        let mut config = load_services_config(&config_dir).await;

        let found = config.services.iter_mut().find(|s| s.name == app_id);
        match found {
            Some(entry) => {
                if entry.enabled == enabled {
                    return Ok(serde_json::json!({
                        "app_id": app_id,
                        "enabled": enabled,
                        "changed": false,
                    }));
                }
                entry.enabled = enabled;
            }
            None => {
                if !enabled {
                    return Ok(serde_json::json!({
                        "app_id": app_id,
                        "enabled": false,
                        "changed": false,
                    }));
                }
                let port = self.resolve_app_local_port(app_id).await.ok_or_else(|| {
                    anyhow::anyhow!(
                        "No local web port found for '{}' — the app isn't running or exposes no UI port",
                        app_id
                    )
                })?;
                let is_proto = is_protocol_service(app_id);
                config.services.push(TorServiceEntry {
                    name: app_id.to_string(),
                    local_port: port,
                    remote_port: None,
                    unauthenticated: is_proto,
                    enabled: true,
                });
            }
        }
        save_services_config(&config_dir, &config).await?;

        if !enabled {
            delete_hidden_service_dir(app_id).await;
            info!(
                app = app_id,
                "Disabled Tor access — removed hidden service dir"
            );
        }

        regenerate_torrc(&config).await?;
        restart_tor().await?;

        let new_onion = if enabled {
            let onion = wait_for_hostname(app_id, 60).await;
            if let Some(ref addr) = onion {
                sync_single_hostname(app_id, addr).await;
            }
            onion
        } else {
            let hostnames_dir = self.config.data_dir.join("tor-hostnames");
            let _ = tokio::fs::remove_file(hostnames_dir.join(app_id)).await;
            None
        };

        Ok(serde_json::json!({
            "app_id": app_id,
            "enabled": enabled,
            "changed": true,
            "onion_address": new_onion,
        }))
    }

    /// The host-local port Tor should forward to for an app: the static map
    /// first (protocol apps like bitcoin must expose 8333, not a UI port),
    /// then the live launch address the scanner derived from the app's
    /// published container ports — so any manifest-driven app works without
    /// a per-app entry.
    pub(in crate::api::rpc) async fn resolve_app_local_port(&self, name: &str) -> Option<u16> {
        let known = known_service_port(name);
        if known != 0 {
            return Some(known);
        }
        let (data, _) = self.state_manager.get_snapshot().await;
        let lan = data
            .package_data
            .get(name)?
            .installed
            .as_ref()?
            .interface_addresses
            .get("main")?
            .lan_address
            .clone()?;
        crate::api::rpc::container::port_from_url(&lan)
    }

    /// Best-effort auto-exposure of a freshly installed app as a Tor hidden
    /// service. Skips protocol services (bitcoin/lnd keep their explicit
    /// flows), the node's own service, the credential-bearing UI proxies in
    /// [`never_auto_onioned`], apps that already have one, and apps with no
    /// resolvable web port. Runs detached after install — it never fails the
    /// caller, it only logs.
    pub(in crate::api::rpc) async fn auto_add_tor_service(&self, app_id: &str) {
        if app_id == "archipelago" || is_protocol_service(app_id) {
            return;
        }
        if never_auto_onioned(app_id) {
            info!(
                app = app_id,
                "Skipping auto Tor service — this app fronts wallet/node RPC and \
                 must not gain a global onion as a side effect of being installed"
            );
            return;
        }
        let config_dir = self.config.data_dir.join("tor-config");
        // The scanner may still be deriving the launch address on slower
        // nodes; retry for up to ~5 minutes before giving up quietly.
        for _ in 0..10u32 {
            let config = load_services_config(&config_dir).await;
            if config.services.iter().any(|s| s.name == app_id) {
                return;
            }
            if let Some(port) = self.resolve_app_local_port(app_id).await {
                let params = serde_json::json!({ "name": app_id, "local_port": port });
                match self.handle_tor_create_service(Some(params)).await {
                    Ok(v) => info!(
                        app = app_id,
                        port,
                        onion = ?v.get("onion_address"),
                        "Auto-created Tor hidden service after install"
                    ),
                    Err(e) => warn!(app = app_id, "Auto Tor service creation failed: {}", e),
                }
                return;
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        }
        debug!(
            app = app_id,
            "No web port resolved — skipping auto Tor service"
        );
    }

    /// Restart Tor daemon (system or container).
    pub(in crate::api::rpc) async fn handle_tor_restart(&self) -> Result<serde_json::Value> {
        info!("Manual Tor restart requested");

        let config_dir = self.config.data_dir.join("tor-config");
        let config = load_services_config(&config_dir).await;
        regenerate_torrc(&config).await?;

        restart_tor().await?;

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        sync_all_hostname_copies(&config).await;

        let running = check_tor_running().await;
        Ok(serde_json::json!({ "restarted": true, "tor_running": running }))
    }
}
