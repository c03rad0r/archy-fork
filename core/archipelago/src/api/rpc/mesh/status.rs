use super::super::RpcHandler;
use crate::mesh;
use anyhow::Result;
use tracing::warn;

impl RpcHandler {
    /// mesh.status — Get mesh radio status, device info, and peer count.
    pub(in crate::api::rpc) async fn handle_mesh_status(&self) -> Result<serde_json::Value> {
        // Block-header send/receive prefs live in MeshConfig; surface them in
        // status so the UI toggles (issue #28) can show the persisted state.
        let config = mesh::load_config(&self.config.data_dir).await?;
        let service = self.mesh_service.read().await;
        let mut value = if let Some(svc) = service.as_ref() {
            let status = svc.status().await;
            serde_json::to_value(status)?
        } else {
            // No service running — return basic config + device detection
            let devices = mesh::detect_devices().await;
            serde_json::json!({
                "enabled": config.enabled,
                "device_connected": false,
                "device_type": "unknown",
                "device_path": config.device_path,
                "channel_name": config.channel_name.clone().unwrap_or_else(|| "archipelago".to_string()),
                "detected_devices": devices,
                "peer_count": 0,
                "messages_sent": 0,
                "messages_received": 0,
            })
        };
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "announce_block_headers".into(),
                config.announce_block_headers.into(),
            );
            obj.insert(
                "receive_block_headers".into(),
                config.receive_block_headers.into(),
            );
            // Persisted config values the settings UI edits (distinct from the
            // live radio-reported `region`): the configured LoRa region and
            // the firmware pin ("meshcore"|"meshtastic"|"reticulum"|null=auto).
            obj.insert("lora_region".into(), config.lora_region.clone().into());
            obj.insert(
                "lora_radio_params".into(),
                serde_json::to_value(config.lora_radio_params).unwrap_or_default(),
            );
            obj.insert(
                "device_kind".into(),
                config
                    .device_kind
                    .map(|k| k.to_string().to_lowercase())
                    .into(),
            );
            // Hot-swap "keep as is" state: false = archipelago never writes
            // config to the radio (see MeshConfig::manage_radio).
            obj.insert("manage_radio".into(), config.manage_radio.into());
            // USB identity per detected port so the setup modal can show the
            // actual board (product string on native-USB boards, vid:pid as
            // the fallback for bridge chips).
            obj.insert(
                "detected_device_info".into(),
                serde_json::to_value(mesh::detect_devices_info().await).unwrap_or_default(),
            );
            // Raw serial-device presence, in BOTH branches. MeshStatus has no
            // such field, so while the service was running the UI couldn't
            // tell "no radio plugged in" from "radio present but the session
            // can't open it yet" — both looked like device_connected=false.
            if !obj.contains_key("detected_devices") {
                let devices = mesh::detect_devices().await;
                obj.insert("device_present".into(), (!devices.is_empty()).into());
                obj.insert("detected_devices".into(), devices.into());
            } else {
                let present = obj
                    .get("detected_devices")
                    .and_then(|v| v.as_array())
                    .is_some_and(|a| !a.is_empty());
                obj.insert("device_present".into(), present.into());
            }
        }
        Ok(value)
    }

    /// mesh.probe-device — Identify the firmware on a detected serial port and
    /// read its current configuration WITHOUT provisioning it. Powers the
    /// hot-swap "device detected" modal's current-details view. The path must
    /// be one of the currently detected candidate ports (no arbitrary device
    /// paths), and probing the live session's own port is refused.
    pub(in crate::api::rpc) async fn handle_mesh_probe_device(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let path = params
            .as_ref()
            .and_then(|p| p.get("path"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing path"))?
            .to_string();
        let detected = mesh::detect_devices().await;
        anyhow::ensure!(
            detected.iter().any(|d| d == &path),
            "{path} is not a detected mesh-radio candidate port"
        );
        // Refuse to probe while a firmware flash is in flight. Confirmed
        // live 2026-07-23: esptool ("multiple access on port?") and
        // rnodeconf (OSError Errno 71 Protocol error on an RTS ioctl) both
        // failed with symptoms consistent with a second process holding the
        // same serial fd — the flash subprocess runs for minutes outside
        // our own async runtime, so nothing previously stopped a concurrent
        // `mesh.probe-device` call (e.g. the hot-swap modal's own re-probe)
        // from opening the identical port at the same time and corrupting
        // both operations' handshakes.
        if let Some(job) = self.flash_job.read().await.as_ref() {
            anyhow::ensure!(
                job.snapshot().await.done,
                "A firmware flash is in progress — refusing to probe the serial port until it finishes"
            );
        }
        // Only hold the mesh_service lock long enough for the quick
        // active-path guard check — NEVER across the actual probe, which
        // can take 15-60s across its internal collision retries. Confirmed
        // live 2026-07-23: holding this read lock for the full probe starved
        // a concurrent firmware-flash job's MeshService::stop() (which needs
        // the write lock) well past its own bounded timeout, surfacing as
        // "Mesh listener did not release the serial port" even though
        // stop() itself was fast.
        {
            let service = self.mesh_service.read().await;
            if let Some(svc) = service.as_ref() {
                svc.ensure_probe_allowed(&path).await?;
            }
        }
        let probe = mesh::listener::probe_device(&path).await?;
        Ok(serde_json::to_value(probe)?)
    }

    /// mesh.refresh — Actively refresh discovery state: re-query the radio's
    /// contact table and re-announce ourselves so quiet-but-alive neighbours
    /// answer. This is what the UI's Refresh button calls — before it existed
    /// the button only re-read server caches and never touched the radio.
    pub(in crate::api::rpc) async fn handle_mesh_refresh(&self) -> Result<serde_json::Value> {
        let service = self.mesh_service.read().await;
        let Some(svc) = service.as_ref() else {
            return Ok(serde_json::json!({ "refreshed": false, "device_connected": false }));
        };
        let status = svc.status().await;
        if status.device_connected {
            let state = svc.shared_state();
            let _ = state
                .send_cmd(crate::mesh::listener::MeshCommand::RefreshContacts)
                .await;
        }
        Ok(serde_json::json!({
            "refreshed": status.device_connected,
            "device_connected": status.device_connected,
        }))
    }

    /// mesh.peers — List discovered mesh peers.
    pub(in crate::api::rpc) async fn handle_mesh_peers(&self) -> Result<serde_json::Value> {
        let service = self.mesh_service.read().await;
        if let Some(svc) = service.as_ref() {
            let peers = svc.peers().await;
            Ok(serde_json::json!({
                "peers": peers,
                "count": peers.len(),
            }))
        } else {
            Ok(serde_json::json!({
                "peers": [],
                "count": 0,
            }))
        }
    }

    /// mesh.messages — Get recent mesh message history.
    pub(in crate::api::rpc) async fn handle_mesh_messages(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let limit = params
            .as_ref()
            .and_then(|p| p.get("limit"))
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);

        let service = self.mesh_service.read().await;
        if let Some(svc) = service.as_ref() {
            let messages = svc.messages(limit).await;
            Ok(serde_json::json!({
                "messages": messages,
                "count": messages.len(),
            }))
        } else {
            Ok(serde_json::json!({
                "messages": [],
                "count": 0,
            }))
        }
    }

    /// conversations.list — Unified inbox across mesh peers, mesh channels,
    /// and federation nodes. Each conversation returns its latest message
    /// timestamp + snippet + transport tag so the UI can render one sorted list.
    pub(in crate::api::rpc) async fn handle_conversations_list(
        &self,
        _params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let mut conversations: Vec<serde_json::Value> = Vec::new();
        let service = self.mesh_service.read().await;
        if let Some(svc) = service.as_ref() {
            let peers = svc.peers().await;
            let messages = svc.messages(None).await;
            // Collapse radio/federation twins into one conversation per identity
            // so a node reachable both ways shows once, with its messages unioned
            // across both twin contact_ids (#12).
            let groups = mesh::group_peer_twins(&peers);
            for group in &groups {
                let peer = &group.canonical;
                // Newest message across ALL twin contact_ids in this group.
                let last = messages
                    .iter()
                    .rev()
                    .find(|m| group.contact_ids.contains(&m.peer_contact_id));
                let is_federation = peer.contact_id & 0x8000_0000 != 0;
                conversations.push(serde_json::json!({
                    "id": format!("{}:{}", if is_federation { "federation" } else { "mesh" }, peer.contact_id),
                    "transport": if is_federation { "federation" } else { "mesh" },
                    "contact_id": peer.contact_id,
                    "name": peer.advert_name,
                    "pubkey": peer.pubkey_hex,
                    "last_text": last.map(|m| m.plaintext.clone()),
                    "last_timestamp": last.map(|m| m.timestamp.clone()),
                    "last_direction": last.map(|m| format!("{:?}", m.direction).to_lowercase()),
                }));
            }
            // Channel 0 ("Archipelago") as a synthetic conversation.
            let channel_last = messages
                .iter()
                .rev()
                .find(|m| m.message_type == "text" && m.peer_contact_id == 0);
            conversations.push(serde_json::json!({
                "id": "channel:0",
                "transport": "channel",
                "channel": 0,
                "name": "Archipelago",
                "last_text": channel_last.map(|m| m.plaintext.clone()),
                "last_timestamp": channel_last.map(|m| m.timestamp.clone()),
            }));
        }
        // Sort by last_timestamp desc (missing timestamps sink).
        conversations.sort_by(|a, b| {
            let at = a
                .get("last_timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let bt = b
                .get("last_timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            bt.cmp(at)
        });
        Ok(serde_json::json!({ "conversations": conversations }))
    }

    /// conversations.messages — Return messages for a ConversationId string
    /// (format: `mesh:<contact_id>` | `federation:<contact_id>` | `channel:<u8>`).
    pub(in crate::api::rpc) async fn handle_conversations_messages(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let id = params["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?;
        let (kind, rest) = id
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("Invalid conversation id"))?;
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let all = svc.messages(None).await;
        let filtered: Vec<_> = match kind {
            "mesh" | "federation" => {
                let contact_id: u32 = rest.parse().unwrap_or(0);
                // Resolve this id's twin group and union messages across all of
                // its contact_ids, so opening either twin shows the full thread
                // (federation-injected + radio messages) (#12).
                let ids: Vec<u32> = mesh::group_peer_twins(&svc.peers().await)
                    .into_iter()
                    .find(|g| g.contact_ids.contains(&contact_id))
                    .map(|g| g.contact_ids)
                    .unwrap_or_else(|| vec![contact_id]);
                all.into_iter()
                    .filter(|m| ids.contains(&m.peer_contact_id))
                    .collect()
            }
            "channel" => {
                // For now the channel bucket keeps contact_id = 0.
                all.into_iter().filter(|m| m.peer_contact_id == 0).collect()
            }
            _ => Vec::new(),
        };
        Ok(serde_json::json!({ "messages": filtered }))
    }

    /// mesh.debug-dump — Full in-memory state snapshot for debugging.
    /// Returns peers, all messages, status, shared-secret peer ids, encrypt_relay
    /// flag, and stego mode. Intended for smoke tests and bug investigation.
    pub(in crate::api::rpc) async fn handle_mesh_debug_dump(&self) -> Result<serde_json::Value> {
        let service = self.mesh_service.read().await;
        if let Some(svc) = service.as_ref() {
            Ok(svc.debug_dump().await)
        } else {
            Ok(serde_json::json!({ "running": false }))
        }
    }

    /// mesh.session-status — Get ratchet session info for a peer.
    pub(in crate::api::rpc) async fn handle_mesh_session_status(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;

        // Look up peer DID from mesh service
        let service = self.mesh_service.read().await;
        let peer_did = if let Some(svc) = service.as_ref() {
            let peers = svc.peers().await;
            peers
                .iter()
                .find(|p| p.contact_id == contact_id)
                .and_then(|p| p.did.clone())
        } else {
            None
        };

        if let Some(did) = peer_did {
            let session_mgr = mesh::session::SessionManager::new(&self.config.data_dir);
            if let Some(info) = session_mgr.session_info(&did).await {
                Ok(serde_json::json!({
                    "has_session": info.has_session,
                    "forward_secrecy": info.forward_secrecy,
                    "message_count": info.message_count,
                    "ratchet_generation": info.ratchet_generation,
                    "peer_did": did,
                }))
            } else {
                Ok(serde_json::json!({
                    "has_session": false,
                    "forward_secrecy": false,
                    "message_count": 0,
                    "ratchet_generation": 0,
                    "peer_did": did,
                }))
            }
        } else {
            Ok(serde_json::json!({
                "has_session": false,
                "forward_secrecy": false,
                "message_count": 0,
                "ratchet_generation": 0,
                "peer_did": null,
            }))
        }
    }

    /// mesh.clear-all — Nuclear reset: wipe all mesh state files and restart
    /// the service for a completely clean slate.
    pub(in crate::api::rpc) async fn handle_mesh_clear_all(&self) -> Result<serde_json::Value> {
        let data_dir = self.config.data_dir.clone();
        // Delete all mesh state files
        for filename in &[
            "messages.json",
            "mesh-contacts.json",
            "sessions.json",
            "mesh-outbox.json",
        ] {
            let _ = tokio::fs::remove_file(data_dir.join(filename)).await;
        }
        // Clear in-memory state
        let service = self.mesh_service.read().await;
        if let Some(svc) = service.as_ref() {
            let state = svc.state();

            // NOTE: `clear-all` intentionally does NOT build a radio-contact
            // blocklist. Permanently ignoring firmware contacts meant a cleared
            // peer could never return even when it re-advertised (it also broke
            // re-pairing a phone after a clear). Real per-contact blocking will
            // be a separate, explicit feature. Here we just wipe the app-side
            // view and ALSO clear any blocklist left over from older builds, so
            // previously-hidden contacts can re-appear when next heard. The
            // firmware's own contact table is the source of truth on refresh.
            {
                let mut set = state.radio_contact_blocklist.write().await;
                set.clear();
            }
            if let Err(e) = crate::mesh::save_ignored_radio_contacts(&data_dir, &[]).await {
                warn!("Failed to persist cleared radio-contact blocklist: {e:#}");
            }

            // Actually DELETE each radio contact from the firmware table (via
            // CMD_REMOVE_CONTACT) so wiped peers don't just reappear on the next
            // refresh. They come back only when they re-advertise (reachable).
            // Federation-synthetic peers (high contact_id bit) aren't firmware
            // contacts, so skip those.
            let firmware_pubkeys: Vec<[u8; 32]> = state
                .peers
                .read()
                .await
                .values()
                .filter(|p| p.contact_id & 0x8000_0000 == 0)
                .filter_map(|p| p.pubkey_hex.as_deref())
                .filter_map(|h| hex::decode(h).ok())
                .filter(|b| b.len() == 32)
                .map(|b| {
                    let mut k = [0u8; 32];
                    k.copy_from_slice(&b);
                    k
                })
                .collect();
            for pk in firmware_pubkeys {
                let _ = state
                    .send_cmd(crate::mesh::listener::MeshCommand::RemoveContact { pubkey: pk })
                    .await;
            }

            state.peers.write().await.clear();
            state.messages.write().await.clear();
            state.contacts.write().await.clear();
            state.presence.write().await.clear();
            state.chunk_buffer.write().await.clear();
            state.shared_secrets.write().await.clear();
            // Re-seed federation peers
            crate::mesh::seed_federation_peers_into_mesh(state, &data_dir).await;
            // Trigger a contact refresh from the radio device
            let _ = state
                .send_cmd(crate::mesh::listener::MeshCommand::RefreshContacts)
                .await;
        }
        Ok(serde_json::json!({ "status": "cleared" }))
    }
}
