use super::super::RpcHandler;
use crate::mesh;
use crate::mesh::message_types::Coordinate;
use anyhow::Result;
use tracing::info;

impl RpcHandler {
    /// mesh.outbox — List pending store-and-forward messages.
    pub(in crate::api::rpc) async fn handle_mesh_outbox(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let limit = params
            .as_ref()
            .and_then(|p| p["limit"].as_u64())
            .map(|n| n as usize);

        // Check if outbox file exists
        let outbox = mesh::outbox::MeshOutbox::load(&self.config.data_dir).await?;
        let messages = outbox.list(limit).await;
        let count = outbox.count().await;

        Ok(serde_json::json!({
            "messages": messages.iter().map(|m| serde_json::json!({
                "id": m.id,
                "dest_did": m.dest_did,
                "from_did": m.from_did,
                "created_at": m.created_at,
                "ttl_secs": m.ttl_secs,
                "retry_count": m.retry_count,
                "relay_hops": m.relay_hops,
                "expired": m.is_expired(),
            })).collect::<Vec<_>>(),
            "count": count,
        }))
    }

    /// mesh.deadman-status — Get dead man's switch status.
    pub(in crate::api::rpc) async fn handle_mesh_deadman_status(
        &self,
    ) -> Result<serde_json::Value> {
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        let status = svc.dead_man_switch.status().await;
        Ok(serde_json::to_value(status)?)
    }

    /// mesh.deadman-configure — Configure the dead man's switch.
    pub(in crate::api::rpc) async fn handle_mesh_deadman_configure(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        let mut config = svc.dead_man_switch.get_config().await;

        if let Some(enabled) = params.get("enabled").and_then(|v| v.as_bool()) {
            config.dead_man_enabled = enabled;
        }
        if let Some(interval) = params.get("interval_secs").and_then(|v| v.as_u64()) {
            if interval < 60 {
                anyhow::bail!("Interval must be at least 60 seconds");
            }
            config.dead_man_interval_secs = interval;
        }
        if let (Some(lat), Some(lng)) = (
            params.get("lat").and_then(|v| v.as_f64()),
            params.get("lng").and_then(|v| v.as_f64()),
        ) {
            let label = params
                .get("label")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            config.last_gps = Some(Coordinate::from_degrees(lat, lng, label));
        }
        if let Some(contacts) = params.get("contacts").and_then(|v| v.as_array()) {
            config.emergency_contacts = contacts
                .iter()
                .filter_map(|c| c.as_str().map(|s| s.to_string()))
                .collect();
        }
        if let Some(msg) = params.get("custom_message").and_then(|v| v.as_str()) {
            config.custom_message = Some(msg.to_string());
        }
        if let Some(auto_gps) = params.get("auto_gps").and_then(|v| v.as_bool()) {
            config.auto_include_gps = auto_gps;
        }

        svc.dead_man_switch.configure(config).await?;
        // Reset timer on configure
        svc.dead_man_switch.check_in().await;

        let status = svc.dead_man_switch.status().await;
        info!("Dead man's switch configured");
        Ok(serde_json::to_value(status)?)
    }

    /// mesh.deadman-checkin — Heartbeat to reset the dead man's switch timer.
    pub(in crate::api::rpc) async fn handle_mesh_deadman_checkin(
        &self,
    ) -> Result<serde_json::Value> {
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        svc.dead_man_check_in().await;
        let remaining = svc.dead_man_switch.time_remaining_secs().await;

        Ok(serde_json::json!({
            "checked_in": true,
            "time_remaining_secs": remaining,
        }))
    }

    /// mesh.rotate-prekeys — Force prekey rotation for X3DH.
    pub(in crate::api::rpc) async fn handle_mesh_rotate_prekeys(
        &self,
    ) -> Result<serde_json::Value> {
        // Load identity signing key
        let identity_dir = self.config.data_dir.join("identity");
        let node_key_path = identity_dir.join("node_key");
        let key_bytes = tokio::fs::read(&node_key_path)
            .await
            .map_err(|_| anyhow::anyhow!("Node identity not found"))?;
        if key_bytes.len() != 32 {
            anyhow::bail!("Invalid node key");
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&key_bytes);
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);

        // Generate new prekey bundle
        let (bundle, _secrets) = mesh::x3dh::generate_prekey_bundle(&signing_key, 10)?;

        // Save bundle for distribution
        let bundle_bytes = mesh::x3dh::encode_bundle(&bundle)?;
        let prekey_dir = self.config.data_dir.join("prekeys");
        tokio::fs::create_dir_all(&prekey_dir).await?;
        tokio::fs::write(prekey_dir.join("bundle.cbor"), &bundle_bytes).await?;

        info!(
            one_time_keys = bundle.one_time_prekeys.len(),
            "Prekey bundle rotated"
        );
        Ok(serde_json::json!({
            "rotated": true,
            "signed_prekey_id": bundle.signed_prekey.id,
            "one_time_prekeys": bundle.one_time_prekeys.len(),
        }))
    }

    /// mesh.test-send — Send test payloads of various sizes to diagnose radio link.
    /// Sends plain text markers that the receiver can count.
    pub(in crate::api::rpc) async fn handle_mesh_test_send(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;

        // Test modes: "ping" (small), "medium" (80 bytes), "large" (150 bytes), "chunked" (400 bytes)
        let mode = params["mode"].as_str().unwrap_or("ping");
        let count = params["count"].as_u64().unwrap_or(3) as usize;

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        let mut sent = 0usize;
        let test_id = chrono::Utc::now().timestamp() as u32;

        for i in 0..count {
            let payload = match mode {
                "ping" => format!("MESHTEST:{}:{}:PING", test_id, i),
                "medium" => format!("MESHTEST:{}:{}:{}", test_id, i, "X".repeat(60)),
                "large" => format!("MESHTEST:{}:{}:{}", test_id, i, "X".repeat(130)),
                "chunked" => {
                    // Send a TypedEnvelope that requires chunking (>140 base64 chars)
                    let fake_tx = "0".repeat(400); // simulates TX hex
                    let wire = crate::mesh::bitcoin_relay::build_tx_relay_request(
                        &fake_tx,
                        test_id as u64 + i as u64,
                    )?;
                    // Send via SendRaw which handles base64 + chunking
                    let peers = svc.peers().await;
                    if let Some(peer) = peers.iter().find(|p| p.contact_id == contact_id) {
                        if let Some(ref pk) = peer.pubkey_hex {
                            if let Ok(pk_bytes) = hex::decode(pk) {
                                if pk_bytes.len() >= 6 {
                                    let mut prefix = [0u8; 6];
                                    prefix.copy_from_slice(&pk_bytes[..6]);
                                    let _ = svc
                                        .shared_state()
                                        .send_cmd(crate::mesh::listener::MeshCommand::SendRaw {
                                            dest_pubkey_prefix: prefix,
                                            payload: wire,
                                        })
                                        .await;
                                    sent += 1;
                                }
                            }
                        }
                    }
                    // Delay between chunked sends
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    continue;
                }
                _ => format!("MESHTEST:{}:{}:UNKNOWN", test_id, i),
            };

            // Send as plain text for ping/medium/large
            let _msg = svc.send_message(contact_id, &payload).await?;
            sent += 1;
            info!(
                test_id,
                seq = i,
                mode,
                len = payload.len(),
                "Test message sent"
            );

            // Small delay between sends
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        }

        Ok(serde_json::json!({
            "test_id": test_id,
            "mode": mode,
            "sent": sent,
            "count": count,
        }))
    }
}
