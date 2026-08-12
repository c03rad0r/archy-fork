use super::super::RpcHandler;
use anyhow::Result;
use tracing::info;

impl RpcHandler {
    /// mesh.relay-tx — Send a raw transaction for relay by an internet-connected mesh peer.
    pub(in crate::api::rpc) async fn handle_mesh_relay_tx(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let tx_hex = params["tx_hex"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing tx_hex"))?;

        let relay_mode = params["relay_mode"].as_str().unwrap_or("archy");

        if tx_hex.len() < 20 || tx_hex.len() > 200_000 {
            anyhow::bail!("Invalid tx_hex length");
        }
        // Validate hex
        if hex::decode(tx_hex).is_err() {
            anyhow::bail!("tx_hex is not valid hexadecimal");
        }

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        let request_id = chrono::Utc::now().timestamp() as u64;
        svc.relay_tracker
            .track_tx_relay(request_id, svc.our_did())
            .await;

        let wire = crate::mesh::bitcoin_relay::build_tx_relay_request(tx_hex, request_id)?;

        let mut sent_count = 0u32;

        if relay_mode == "broadcast" {
            // Broadcast mode: send on channel 0 (all mesh nodes relay)
            // Still encrypted — only Archy nodes can decrypt and broadcast the TX
            let shared_state = svc.shared_state();
            let shared_secrets = shared_state.shared_secrets.read().await;

            // Encrypt with first available Archy peer's shared secret
            // (any Archy node that receives it can try decrypting)
            let payload = shared_secrets
                .values()
                .next()
                .and_then(|secret| {
                    crate::mesh::crypto::encrypt(secret, &wire).ok().map(|ct| {
                        let mut encrypted = Vec::with_capacity(1 + ct.len());
                        encrypted.push(crate::mesh::message_types::ENCRYPTED_TYPED_MARKER);
                        encrypted.extend_from_slice(&ct);
                        encrypted
                    })
                })
                .unwrap_or_else(|| wire.clone());
            drop(shared_secrets);

            {
                use base64::Engine;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&payload);
                let _ = shared_state
                    .send_cmd(crate::mesh::listener::MeshCommand::BroadcastChannel {
                        channel: 0,
                        payload: b64.into_bytes(),
                    })
                    .await;
            }
            info!(
                request_id,
                tx_len = tx_hex.len(),
                "TX relay broadcast on mesh channel 0 (encrypted)"
            );
        } else {
            // Archy mode: E2E encrypted per-peer, direct to known Archy nodes
            let peers = svc.peers().await;
            let shared_state = svc.shared_state();
            let shared_secrets = shared_state.shared_secrets.read().await;
            for peer in &peers {
                if !peer.advert_name.starts_with("Archy-") {
                    continue;
                }
                if let Some(ref pk) = peer.pubkey_hex {
                    if let Ok(pk_bytes) = hex::decode(pk) {
                        if pk_bytes.len() >= 6 {
                            let mut prefix = [0u8; 6];
                            prefix.copy_from_slice(&pk_bytes[..6]);

                            let payload = if let Some(secret) = shared_secrets.get(&peer.contact_id)
                            {
                                match crate::mesh::crypto::encrypt(secret, &wire) {
                                    Ok(ciphertext) => {
                                        let mut encrypted =
                                            Vec::with_capacity(1 + ciphertext.len());
                                        encrypted.push(
                                            crate::mesh::message_types::ENCRYPTED_TYPED_MARKER,
                                        );
                                        encrypted.extend_from_slice(&ciphertext);
                                        encrypted
                                    }
                                    Err(_) => wire.clone(),
                                }
                            } else {
                                wire.clone()
                            };

                            let _ = svc
                                .shared_state()
                                .send_cmd(crate::mesh::listener::MeshCommand::SendRaw {
                                    dest_pubkey_prefix: prefix,
                                    payload,
                                })
                                .await;
                            sent_count += 1;
                        }
                    }
                }
            }
            drop(shared_secrets);
            info!(
                request_id,
                tx_len = tx_hex.len(),
                archy_peers = sent_count,
                "TX relay sent to Archy peers (E2E encrypted)"
            );
        }
        Ok(serde_json::json!({
            "request_id": request_id,
            "queued": true,
            "tx_hex_len": tx_hex.len(),
        }))
    }

    /// mesh.relay-status — Check the status of a pending or completed TX relay.
    pub(in crate::api::rpc) async fn handle_mesh_relay_status(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let request_id = params["request_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing request_id"))?;

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        // Check completed results first
        if let Some(result) = svc.relay_tracker.get_result(request_id).await {
            return Ok(serde_json::json!({
                "status": if result.txid.is_some() { "confirmed" } else { "failed" },
                "request_id": result.request_id,
                "txid": result.txid,
                "error": result.error,
                "error_code": result.error_code,
                "completed_at": result.completed_at,
            }));
        }

        // Check if still pending
        if svc.relay_tracker.is_pending(request_id).await {
            return Ok(serde_json::json!({
                "status": "pending",
                "request_id": request_id,
            }));
        }

        // Unknown — either expired or never existed
        Ok(serde_json::json!({
            "status": "unknown",
            "request_id": request_id,
        }))
    }

    /// mesh.block-headers — Get cached block headers received from mesh peers.
    pub(in crate::api::rpc) async fn handle_mesh_block_headers(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let count = params
            .as_ref()
            .and_then(|p| p["count"].as_u64())
            .unwrap_or(10) as usize;

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        let headers = svc.block_header_cache.recent_headers(count).await;
        let latest = svc.block_header_cache.latest_height().await;

        Ok(serde_json::json!({
            "headers": headers.iter().map(|h| serde_json::json!({
                "height": h.height,
                "hash": h.hash,
                "prev_hash": h.prev_hash,
                "timestamp": h.timestamp,
                "announced_by": h.announced_by,
            })).collect::<Vec<_>>(),
            "latest_height": latest,
            "count": headers.len(),
        }))
    }

    /// mesh.relay-lightning — Send a Lightning invoice for payment by an internet-connected peer.
    pub(in crate::api::rpc) async fn handle_mesh_relay_lightning(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let bolt11 = params["bolt11"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing bolt11"))?;
        let amount_sats = params["amount_sats"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing amount_sats"))?;

        if !bolt11.starts_with("lnbc") && !bolt11.starts_with("lntb") {
            anyhow::bail!("Invalid bolt11 invoice — must start with lnbc or lntb");
        }

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        let request_id = chrono::Utc::now().timestamp() as u64;
        svc.relay_tracker
            .track_lightning_relay(request_id, svc.our_did())
            .await;

        let wire = crate::mesh::bitcoin_relay::build_lightning_relay_request(
            bolt11,
            amount_sats,
            request_id,
        )?;

        // Send to Archipelago peers — E2E encrypted per-peer
        let peers = svc.peers().await;
        let shared_state = svc.shared_state();
        let shared_secrets = shared_state.shared_secrets.read().await;
        let mut sent_count = 0u32;
        for peer in &peers {
            if !peer.advert_name.starts_with("Archy-") {
                continue;
            }
            if let Some(ref pk) = peer.pubkey_hex {
                if let Ok(pk_bytes) = hex::decode(pk) {
                    if pk_bytes.len() >= 6 {
                        let mut prefix = [0u8; 6];
                        prefix.copy_from_slice(&pk_bytes[..6]);

                        let payload = if let Some(secret) = shared_secrets.get(&peer.contact_id) {
                            match crate::mesh::crypto::encrypt(secret, &wire) {
                                Ok(ciphertext) => {
                                    let mut encrypted = Vec::with_capacity(1 + ciphertext.len());
                                    encrypted
                                        .push(crate::mesh::message_types::ENCRYPTED_TYPED_MARKER);
                                    encrypted.extend_from_slice(&ciphertext);
                                    encrypted
                                }
                                Err(_) => wire.clone(),
                            }
                        } else {
                            wire.clone()
                        };

                        let _ = svc
                            .shared_state()
                            .send_cmd(crate::mesh::listener::MeshCommand::SendRaw {
                                dest_pubkey_prefix: prefix,
                                payload,
                            })
                            .await;
                        sent_count += 1;
                    }
                }
            }
        }
        drop(shared_secrets);

        info!(
            request_id,
            amount_sats,
            archy_peers = sent_count,
            "Lightning relay sent (E2E encrypted)"
        );
        Ok(serde_json::json!({
            "request_id": request_id,
            "queued": true,
            "amount_sats": amount_sats,
        }))
    }
}
