//! Bitcoin relay operations: TX broadcast, confirmation tracking, peer messaging.

use super::super::crypto;
use super::super::message_types;
use super::MeshCommand;
use super::MeshState;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Called on an internet-connected node when it receives a TxRelay request.
/// Broadcasts the raw TX to Bitcoin via RPC, sends the txid back, then
/// monitors for 3 confirmations and sends updates back via mesh.
pub(super) async fn handle_tx_relay_broadcast(
    relay: message_types::TxRelayPayload,
    sender_contact_id: u32,
    state: &Arc<MeshState>,
) {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to create HTTP client for TX relay: {}", e);
            return;
        }
    };

    let (rpc_user, rpc_pass) = crate::bitcoin_rpc::bitcoin_rpc_credentials().await;

    // Pre-flight: check if Bitcoin Core is reachable and synced
    if !preflight_check(
        &client,
        &rpc_user,
        &rpc_pass,
        &relay,
        sender_contact_id,
        state,
    )
    .await
    {
        return;
    }

    // Step 1: Broadcast via Bitcoin Core RPC sendrawtransaction
    let txid = match broadcast_transaction(
        &client,
        &rpc_user,
        &rpc_pass,
        &relay,
        sender_contact_id,
        state,
    )
    .await
    {
        Some(id) => id,
        None => return,
    };

    info!(request_id = relay.request_id, txid = %txid, "TX broadcast successful — tracking confirmations");

    // Step 2: Send TxRelayResponse with txid back to originator
    send_tx_relay_response(
        state,
        sender_contact_id,
        relay.request_id,
        Some(&txid),
        None,
        None,
    )
    .await;

    // Step 3: Monitor confirmations (poll every 30s, up to 3 hours)
    track_confirmations(&client, &txid, relay.request_id, sender_contact_id, state).await;
}

/// Pre-flight check: verify Bitcoin Core is reachable and synced.
/// Returns `true` if the node is ready, `false` if an error response was sent.
async fn preflight_check(
    client: &reqwest::Client,
    rpc_user: &str,
    rpc_pass: &str,
    relay: &message_types::TxRelayPayload,
    sender_contact_id: u32,
    state: &Arc<MeshState>,
) -> bool {
    let preflight_body = serde_json::json!({
        "jsonrpc": "1.0",
        "id": "preflight",
        "method": "getblockchaininfo",
        "params": []
    });

    match client
        .post(crate::constants::BITCOIN_RPC_URL)
        .basic_auth(rpc_user, Some(rpc_pass))
        .json(&preflight_body)
        .send()
        .await
    {
        Ok(resp) => {
            if let Ok(rpc_resp) = resp.json::<serde_json::Value>().await {
                if let Some(result) = rpc_resp.get("result") {
                    let ibd = result
                        .get("initialblockdownload")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let progress = result
                        .get("verificationprogress")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    if ibd || progress < 0.999 {
                        let pct = (progress * 100.0) as u32;
                        let msg =
                            format!("Bitcoin node is syncing ({}%) — cannot broadcast yet", pct);
                        warn!(request_id = relay.request_id, "{}", msg);
                        send_tx_relay_response(
                            state,
                            sender_contact_id,
                            relay.request_id,
                            None,
                            Some(&msg),
                            Some("bitcoin_syncing"),
                        )
                        .await;
                        return false;
                    }
                } else if let Some(err) = rpc_resp.get("error").and_then(|e| e.as_object()) {
                    let msg = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("RPC error");
                    warn!(
                        request_id = relay.request_id,
                        "Bitcoin pre-flight failed: {}", msg
                    );
                    send_tx_relay_response(
                        state,
                        sender_contact_id,
                        relay.request_id,
                        None,
                        Some(&format!("Bitcoin node error: {}", msg)),
                        Some("bitcoin_unreachable"),
                    )
                    .await;
                    return false;
                }
            }
        }
        Err(e) => {
            let msg = format!(
                "Bitcoin node unreachable — {}",
                if e.is_connect() {
                    "connection refused (node may be stopped)"
                } else if e.is_timeout() {
                    "connection timed out"
                } else {
                    "network error"
                }
            );
            warn!(request_id = relay.request_id, "Pre-flight: {}: {}", msg, e);
            send_tx_relay_response(
                state,
                sender_contact_id,
                relay.request_id,
                None,
                Some(&msg),
                Some("bitcoin_unreachable"),
            )
            .await;
            return false;
        }
    }
    true
}

/// Broadcast a raw transaction via Bitcoin Core RPC.
/// Returns the txid on success, or None if an error response was sent.
async fn broadcast_transaction(
    client: &reqwest::Client,
    rpc_user: &str,
    rpc_pass: &str,
    relay: &message_types::TxRelayPayload,
    sender_contact_id: u32,
    state: &Arc<MeshState>,
) -> Option<String> {
    let body = serde_json::json!({
        "jsonrpc": "1.0",
        "id": "mesh-relay",
        "method": "sendrawtransaction",
        "params": [relay.tx_hex]
    });

    let txid = match client
        .post(crate::constants::BITCOIN_RPC_URL)
        .basic_auth(rpc_user, Some(rpc_pass))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(rpc_resp) => {
                if let Some(err) = rpc_resp.get("error").and_then(|e| e.as_object()) {
                    let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
                    let msg = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown");
                    let user_msg = match code {
                        -25 => format!("TX already in mempool or confirmed: {}", msg),
                        -26 => format!("TX rejected by mempool policy: {}", msg),
                        -27 => "TX already confirmed in a block".to_string(),
                        _ => format!("Bitcoin rejected TX (code {}): {}", code, msg),
                    };
                    warn!(
                        request_id = relay.request_id,
                        rpc_code = code,
                        "sendrawtransaction: {}",
                        msg
                    );
                    send_tx_relay_response(
                        state,
                        sender_contact_id,
                        relay.request_id,
                        None,
                        Some(&user_msg),
                        Some(&format!("tx_rejected:{}", code)),
                    )
                    .await;
                    return None;
                }
                rpc_resp
                    .get("result")
                    .and_then(|r| r.as_str())
                    .map(|s| s.to_string())
            }
            Err(e) => {
                warn!("Failed to parse Bitcoin RPC response: {}", e);
                send_tx_relay_response(
                    state,
                    sender_contact_id,
                    relay.request_id,
                    None,
                    Some("Failed to parse Bitcoin node response"),
                    Some("rpc_parse_error"),
                )
                .await;
                return None;
            }
        },
        Err(e) => {
            let msg = format!(
                "Bitcoin node unreachable during broadcast — {}",
                if e.is_connect() {
                    "connection refused"
                } else if e.is_timeout() {
                    "timed out"
                } else {
                    "network error"
                }
            );
            warn!("Bitcoin Core RPC unreachable: {}", e);
            send_tx_relay_response(
                state,
                sender_contact_id,
                relay.request_id,
                None,
                Some(&msg),
                Some("bitcoin_unreachable"),
            )
            .await;
            return None;
        }
    };

    if txid.is_none() {
        send_tx_relay_response(
            state,
            sender_contact_id,
            relay.request_id,
            None,
            Some("Bitcoin node returned no transaction ID"),
            Some("rpc_parse_error"),
        )
        .await;
    }
    txid
}

/// Monitor a transaction for confirmations (poll every 30s, up to 3 hours).
async fn track_confirmations(
    client: &reqwest::Client,
    txid: &str,
    request_id: u64,
    sender_contact_id: u32,
    state: &Arc<MeshState>,
) {
    let mut last_reported_confs: u32 = 0;
    for _ in 0..360 {
        tokio::time::sleep(Duration::from_secs(30)).await;

        match check_tx_confirmations(client, txid).await {
            Ok((confs, block_height)) => {
                if confs > last_reported_confs && confs <= 3 {
                    info!(txid = %txid, confirmations = confs, "Sending confirmation update via mesh");
                    send_confirmation_update(
                        state,
                        sender_contact_id,
                        request_id,
                        txid,
                        confs,
                        block_height,
                    )
                    .await;
                    last_reported_confs = confs;
                    if confs >= 3 {
                        info!(txid = %txid, "TX fully confirmed (3/3) — done tracking");
                        return;
                    }
                }
            }
            Err(e) => {
                debug!(txid = %txid, "Confirmation check: {}", e);
            }
        }
    }
}

/// Send a TxRelayResponse back to the originating peer.
async fn send_tx_relay_response(
    state: &Arc<MeshState>,
    dest_contact_id: u32,
    request_id: u64,
    txid: Option<&str>,
    error: Option<&str>,
    error_code: Option<&str>,
) {
    let wire = match super::super::bitcoin_relay::build_tx_relay_response(
        request_id, txid, error, error_code,
    ) {
        Ok(w) => w,
        Err(e) => {
            warn!("Failed to build TX relay response: {}", e);
            return;
        }
    };
    send_to_peer(state, dest_contact_id, wire).await;
}

/// Send a TxConfirmation update to the originator.
async fn send_confirmation_update(
    state: &Arc<MeshState>,
    dest_contact_id: u32,
    request_id: u64,
    txid: &str,
    confirmations: u32,
    block_height: u64,
) {
    let conf = message_types::TxConfirmationPayload {
        request_id,
        txid: txid.to_string(),
        confirmations,
        block_height,
    };
    if let Ok(payload_bytes) = message_types::encode_payload(&conf) {
        let envelope = message_types::TypedEnvelope::new(
            message_types::MeshMessageType::TxConfirmation,
            payload_bytes,
        );
        if let Ok(wire) = envelope.to_wire() {
            send_to_peer(state, dest_contact_id, wire).await;
        }
    }
}

/// Encrypt a typed wire payload for a specific peer.
/// Attempts ratchet encryption first (forward secrecy), falls back to static
/// shared secret, falls back to plaintext if neither is available.
/// Respects the encrypt_relay config toggle for rollback.
async fn encrypt_for_peer(state: &Arc<MeshState>, contact_id: u32, typed_wire: &[u8]) -> Vec<u8> {
    if !state.encrypt_relay {
        return typed_wire.to_vec();
    }

    // Look up peer DID for ratchet session
    let peer_did = state
        .peers
        .read()
        .await
        .get(&contact_id)
        .and_then(|p| p.did.clone());

    // Try ratchet encryption first (forward secrecy)
    if let Some(ref did) = peer_did {
        if state.session_manager.has_session(did).await {
            match state
                .session_manager
                .encrypt_for_peer(did, typed_wire)
                .await
            {
                Ok(ratchet_msg) => {
                    let ratchet_bytes = ratchet_msg.to_bytes();
                    let mut buf = Vec::with_capacity(1 + ratchet_bytes.len());
                    buf.push(message_types::RATCHET_TYPED_MARKER);
                    buf.extend_from_slice(&ratchet_bytes);
                    debug!(contact_id, did = %did, "Encrypted with Double Ratchet (0xDD)");
                    return buf;
                }
                Err(e) => {
                    warn!(contact_id, did = %did, "Ratchet encrypt failed, trying static: {}", e);
                }
            }
        }
    }

    // Fall back to static shared secret (0xEE)
    let secrets = state.shared_secrets.read().await;
    if let Some(secret) = secrets.get(&contact_id) {
        match crypto::encrypt(secret, typed_wire) {
            Ok(ciphertext) => {
                let mut buf = Vec::with_capacity(1 + ciphertext.len());
                buf.push(message_types::ENCRYPTED_TYPED_MARKER);
                buf.extend_from_slice(&ciphertext);
                debug!(contact_id, "Encrypted with static shared secret (0xEE)");
                return buf;
            }
            Err(e) => {
                warn!(
                    contact_id,
                    "Static encrypt failed, sending plaintext: {}", e
                );
            }
        }
    }

    // No encryption available — send plaintext
    debug!(
        contact_id,
        "No encryption available, sending plaintext (0x02)"
    );
    typed_wire.to_vec()
}

/// Send raw wire bytes to a specific peer by contact_id.
/// Encrypts directed messages via ratchet or shared secret when available.
/// Falls back to channel 0 broadcast (plaintext) if peer's pubkey is unknown.
/// `pub(super)` so sibling handlers (e.g. the AI assistant) can reply on the
/// same encrypted, peer-addressed path the relay handlers use.
pub(super) async fn send_to_peer(state: &Arc<MeshState>, contact_id: u32, typed_wire: Vec<u8>) {
    let peers = state.peers.read().await;
    if let Some(peer) = peers.get(&contact_id) {
        if let Some(ref pk) = peer.pubkey_hex {
            if let Ok(pk_bytes) = hex::decode(pk) {
                if pk_bytes.len() >= 6 {
                    let mut prefix = [0u8; 6];
                    prefix.copy_from_slice(&pk_bytes[..6]);
                    drop(peers);
                    // Encrypt for this specific peer before sending
                    let payload = encrypt_for_peer(state, contact_id, &typed_wire).await;
                    let _ = state
                        .send_cmd(MeshCommand::SendRaw {
                            dest_pubkey_prefix: prefix,
                            payload,
                        })
                        .await;
                    return;
                }
            }
        }
    }
    drop(peers);
    // Broadcast fallback — plaintext (no specific peer to encrypt for)
    let _ = state
        .send_cmd(MeshCommand::BroadcastChannel {
            channel: 0,
            payload: typed_wire,
        })
        .await;
}

/// Check transaction confirmation count via Bitcoin Core RPC.
async fn check_tx_confirmations(
    client: &reqwest::Client,
    txid: &str,
) -> anyhow::Result<(u32, u64)> {
    let body = serde_json::json!({
        "jsonrpc": "1.0",
        "id": "mesh-conf",
        "method": "gettransaction",
        "params": [txid]
    });
    let (rpc_user, rpc_pass) = crate::bitcoin_rpc::bitcoin_rpc_credentials().await;
    let resp = client
        .post(crate::constants::BITCOIN_RPC_URL)
        .basic_auth(&rpc_user, Some(&rpc_pass))
        .json(&body)
        .send()
        .await?;
    let rpc_resp: serde_json::Value = resp.json().await?;
    if let Some(result) = rpc_resp.get("result") {
        let confs = result
            .get("confirmations")
            .and_then(|c| c.as_u64())
            .unwrap_or(0) as u32;
        let block_height = result
            .get("blockheight")
            .and_then(|h| h.as_u64())
            .unwrap_or(0);
        Ok((confs, block_height))
    } else {
        anyhow::bail!("gettransaction returned no result")
    }
}
