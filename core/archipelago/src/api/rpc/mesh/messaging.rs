use super::super::RpcHandler;
use crate::mesh;
use anyhow::Result;
use std::sync::Arc;
use tracing::info;

impl RpcHandler {
    /// mesh.send — Send an encrypted message to a mesh peer.
    pub(in crate::api::rpc) async fn handle_mesh_send(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let contact_id = params
            .get("contact_id")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;

        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing message"))?;

        if message.is_empty() {
            anyhow::bail!("Message cannot be empty");
        }

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running. Enable mesh first."))?;

        let msg = svc.send_message(contact_id, message).await?;
        info!(contact_id, encrypted = msg.encrypted, "Sent mesh message");

        Ok(serde_json::json!({
            "sent": true,
            "message_id": msg.id,
            "encrypted": msg.encrypted,
        }))
    }

    /// mesh.send-channel — Send a text message to a mesh channel (broadcast).
    pub(in crate::api::rpc) async fn handle_mesh_send_channel(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let channel = params.get("channel").and_then(|v| v.as_u64()).unwrap_or(0) as u8;

        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing message"))?;

        if message.is_empty() {
            anyhow::bail!("Message cannot be empty");
        }

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running. Enable mesh first."))?;

        let msg = svc.send_channel_message(channel, message).await?;
        info!(channel, "Sent mesh channel message");

        Ok(serde_json::json!({
            "sent": true,
            "message_id": msg.id,
            "channel": channel,
        }))
    }

    /// mesh.broadcast — Broadcast our node identity over mesh.
    pub(in crate::api::rpc) async fn handle_mesh_broadcast(&self) -> Result<serde_json::Value> {
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running. Enable mesh first."))?;

        svc.broadcast_identity().await?;
        info!("Broadcast identity over mesh");

        Ok(serde_json::json!({ "broadcast": true }))
    }

    /// mesh.reboot-radio — Reboot the locally-connected radio firmware to
    /// recover a wedged / RX-deaf radio. Optional `seconds` delay (default 2).
    pub(in crate::api::rpc) async fn handle_mesh_reboot_radio(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let seconds = params
            .as_ref()
            .and_then(|p| p.get("seconds"))
            .and_then(|v| v.as_i64())
            .unwrap_or(2);

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running. Enable mesh first."))?;

        let message = svc.reboot_radio(seconds).await?;
        info!(seconds, "Mesh radio reboot requested via RPC");

        Ok(serde_json::json!({ "reboot": true, "seconds": seconds, "message": message }))
    }

    /// mesh.rnode-config — persisted RF settings + the live radio state
    /// (radio-confirmed values) for the LoRa settings panel. `live` is best-
    /// effort: null with `live_error` when no Reticulum radio is connected.
    pub(in crate::api::rpc) async fn handle_mesh_rnode_config(&self) -> Result<serde_json::Value> {
        let settings = mesh::rnode_settings::RNodeRfSettings::load(&self.config.data_dir).await;
        let (live, live_error) = match self.mesh_service.read().await.as_ref() {
            Some(svc) => match svc.radio_state().await {
                Ok(state) => (Some(state), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            },
            None => (None, Some("Mesh service not running".to_string())),
        };
        Ok(serde_json::json!({
            "settings": settings,
            "live": live,
            "live_error": live_error,
        }))
    }

    /// mesh.rnode-config-apply — validate + persist the RF settings, restart
    /// the radio daemon so they take effect, then read back the radio-
    /// confirmed values as proof. Returns { applied, live, message }; a
    /// failed read-back still reports the persisted settings with a clear
    /// message instead of pretending success.
    pub(in crate::api::rpc) async fn handle_mesh_rnode_config_apply(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let settings: mesh::rnode_settings::RNodeRfSettings = serde_json::from_value(
            params
                .get("settings")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing 'settings'"))?,
        )
        .map_err(|e| anyhow::anyhow!("Invalid settings: {e}"))?;
        settings.validate()?;
        settings.save(&self.config.data_dir).await?;
        info!(?settings, "RNode RF settings persisted");

        // Restart the radio daemon so the new args apply. No radio connected
        // is fine — the settings apply on the next connect.
        let service = self.mesh_service.read().await;
        let Some(svc) = service.as_ref() else {
            return Ok(serde_json::json!({
                "applied": false,
                "message": "Settings saved. They apply when the mesh service next connects to the radio.",
            }));
        };
        if let Err(e) = svc.reboot_radio(2).await {
            return Ok(serde_json::json!({
                "applied": false,
                "message": format!(
                    "Settings saved, but the radio daemon restart failed: {e:#}. \
                     They apply on the next reconnect."
                ),
            }));
        }

        // Read-back: poll until the respawned daemon reports the radio online
        // with our applied values (the respawn re-detects the RNode, ~15s).
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(45);
        let mut last_live = None;
        while tokio::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if let Ok(state) = svc.radio_state().await {
                let online = state
                    .get("online")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                last_live = Some(state);
                if online {
                    break;
                }
            }
        }
        match last_live {
            Some(live) => {
                let confirmed = live
                    .get("r_frequency")
                    .and_then(|v| v.as_u64())
                    .map(|f| f == settings.frequency)
                    .unwrap_or(false);
                Ok(serde_json::json!({
                    "applied": true,
                    "confirmed": confirmed,
                    "live": live,
                    "message": if confirmed {
                        "The radio confirmed it is now using the applied settings."
                    } else {
                        "Settings applied and the daemon restarted; the radio has not \
                         confirmed the new values yet — recheck in a few seconds."
                    },
                }))
            }
            None => Ok(serde_json::json!({
                "applied": true,
                "confirmed": false,
                "live": null,
                "message": "Settings applied and the daemon restarted, but it has not \
                            reported the radio state yet — recheck in a few seconds.",
            })),
        }
    }

    /// mesh.configure — Enable/disable mesh and set device path.
    pub(in crate::api::rpc) async fn handle_mesh_configure(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        if let Some(job) = self.flash_job.read().await.as_ref() {
            anyhow::ensure!(job.snapshot().await.done,
                "A firmware flash is in progress; wait before reconnecting or changing radio settings");
        }

        let mut config = mesh::load_config(&self.config.data_dir).await?;

        if let Some(enabled) = params.get("enabled").and_then(|v| v.as_bool()) {
            config.enabled = enabled;
        }
        if let Some(device) = params.get("device_path").and_then(|v| v.as_str()) {
            config.device_path = Some(device.to_string());
        }
        if let Some(channel) = params.get("channel_name").and_then(|v| v.as_str()) {
            config.channel_name = Some(channel.to_string());
        }
        if let Some(broadcast) = params.get("broadcast_identity").and_then(|v| v.as_bool()) {
            config.broadcast_identity = broadcast;
        }
        if let Some(name) = params.get("advert_name").and_then(|v| v.as_str()) {
            // Empty clears the custom mesh name (falls back to the server
            // name) — without this, a name could be set but never unset.
            let trimmed = name.trim();
            config.advert_name = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        if let Some(announce) = params
            .get("announce_block_headers")
            .and_then(|v| v.as_bool())
        {
            config.announce_block_headers = announce;
        }
        if let Some(receive) = params
            .get("receive_block_headers")
            .and_then(|v| v.as_bool())
        {
            config.receive_block_headers = receive;
        }
        // LoRa region (Meshtastic): validated against the driver's region
        // table so a typo can't be persisted and silently ignored on connect.
        // Empty string clears the setting (radio keeps/uses its own region).
        if let Some(region) = params.get("lora_region").and_then(|v| v.as_str()) {
            let trimmed = region.trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("unset") {
                config.lora_region = None;
            } else if mesh::meshtastic_region_is_valid(trimmed) {
                config.lora_region = Some(trimmed.to_uppercase());
            } else {
                anyhow::bail!("Unknown LoRa region: {trimmed}");
            }
        }
        // Meshcore LoRa PHY params (freq/bw/sf/cr, firmware field units — see
        // mesh::LoraRadioParams). Validated against the firmware's accepted
        // ranges here so a bad value errors at the API instead of being sent
        // to the radio and rejected on-device. `null` clears the setting.
        if let Some(rp) = params.get("lora_radio_params") {
            if rp.is_null() {
                config.lora_radio_params = None;
            } else {
                let parsed: mesh::LoraRadioParams = serde_json::from_value(rp.clone())
                    .map_err(|e| anyhow::anyhow!("Invalid lora_radio_params: {e}"))?;
                anyhow::ensure!(
                    (150_000..=2_500_000).contains(&parsed.freq_khz),
                    "freq_khz out of range (150000..=2500000)"
                );
                anyhow::ensure!(
                    (7_000..=500_000).contains(&parsed.bw_hz),
                    "bw_hz out of range (7000..=500000)"
                );
                anyhow::ensure!((5..=12).contains(&parsed.sf), "sf out of range (5..=12)");
                anyhow::ensure!((5..=8).contains(&parsed.cr), "cr out of range (5..=8)");
                config.lora_radio_params = Some(parsed);
            }
        }
        // Hot-swap "keep as is": false = never write config to the radio
        // (region/channel/PHY/advert name) — use it exactly as flashed.
        if let Some(manage) = params.get("manage_radio").and_then(|v| v.as_bool()) {
            config.manage_radio = manage;
        }
        // Firmware pin: probe only the named firmware on the port ("auto"/""
        // clears the pin and restores strict-probe auto-detect).
        if let Some(kind) = params.get("device_kind").and_then(|v| v.as_str()) {
            config.device_kind = match kind.trim().to_lowercase().as_str() {
                "" | "auto" => None,
                "meshcore" => Some(mesh::types::DeviceType::Meshcore),
                "meshtastic" => Some(mesh::types::DeviceType::Meshtastic),
                "reticulum" | "rnode" => Some(mesh::types::DeviceType::Reticulum),
                other => anyhow::bail!(
                    "Unknown device_kind: {other} (expected auto|meshcore|meshtastic|reticulum)"
                ),
            };
        }

        mesh::save_config(&self.config.data_dir, &config).await?;

        // Apply to the running service in the background: configure() may
        // stop+start the listener (config changes now restart the session so
        // they actually take effect), and that can take seconds when the old
        // session is mid-probe. Holding the service write-lock for that long
        // inside this handler stalled every concurrent mesh.status/mesh.peers
        // poll behind it — the UI froze and nginx surfaced 502s. The config is
        // already persisted above; the UI observes progress via mesh.status.
        {
            let service_arc = Arc::clone(&self.mesh_service);
            let config_for_apply = config.clone();
            let flash_jobs = Arc::clone(&self.flash_job);
            tokio::spawn(async move {
                // Serialize against flash registration. If a flash started
                // after this RPC saved settings, its completion applies them.
                let flash_guard = flash_jobs.read().await;
                if let Some(job) = flash_guard.as_ref() {
                    if !job.snapshot().await.done {
                        return;
                    }
                }
                let mut service = service_arc.write().await;
                if let Some(svc) = service.as_mut() {
                    if let Err(e) = svc.configure(config_for_apply).await {
                        tracing::error!("Applying mesh config to running service failed: {e:#}");
                    }
                }
            });
        }

        info!("Mesh config updated");
        Ok(serde_json::json!({
            "configured": true,
            "enabled": config.enabled,
            "device_path": config.device_path,
            "announce_block_headers": config.announce_block_headers,
            "receive_block_headers": config.receive_block_headers,
            "lora_region": config.lora_region,
            "device_kind": config.device_kind.map(|k| k.to_string()),
        }))
    }
}
