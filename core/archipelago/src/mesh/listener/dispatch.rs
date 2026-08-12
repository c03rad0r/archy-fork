//! Typed message dispatch — routes TypedEnvelope messages to type-specific handlers.

use super::super::message_types::{self, MeshMessageType, TypedEnvelope};
use super::super::types::*;
use super::bitcoin::handle_tx_relay_broadcast;
use super::MeshState;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Store a typed message with a type label and structured JSON payload for UI rendering.
async fn store_typed_message(
    state: &Arc<MeshState>,
    contact_id: u32,
    peer_name: &str,
    display_text: &str,
    type_label: &str,
    typed_payload: Option<serde_json::Value>,
    sender_seq: Option<u64>,
) {
    let msg_id = state.next_id().await;
    // Populate stable MessageKey components from the peer record so the UI
    // can dedupe and target replies/reactions across transports.
    let sender_pubkey = state
        .peers
        .read()
        .await
        .get(&contact_id)
        .and_then(|p| p.pubkey_hex.clone());
    let msg = MeshMessage {
        id: msg_id,
        direction: MessageDirection::Received,
        peer_contact_id: contact_id,
        peer_name: Some(peer_name.to_string()),
        plaintext: display_text.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        delivered: true,
        // transport + E2E are stamped post-dispatch by
        // handle_typed_envelope_direct, which alone knows the receive transport.
        encrypted: false,
        transport: None,
        message_type: type_label.to_string(),
        typed_payload,
        sender_pubkey,
        sender_seq,
    };
    state.store_message(msg.clone()).await;
    state.status.write().await.messages_received += 1;
    let _ = state.event_tx.send(MeshEvent::MessageReceived(msg));
}

/// Serialize a decoded payload to JSON for the UI layer.
/// Falls back to `None` on serialization failure (shouldn't happen for our serde types).
fn payload_to_json<T: serde::Serialize>(v: &T) -> Option<serde_json::Value> {
    serde_json::to_value(v).ok()
}

/// Handle a typed message envelope (0x02 prefix).
/// Dispatches to type-specific handlers: BlockHeader, Alert, TxRelay, etc.
pub(super) async fn handle_typed_message(
    payload: &[u8],
    sender_contact_id: u32,
    sender_name: &str,
    state: &Arc<MeshState>,
) {
    let envelope = match TypedEnvelope::from_wire(payload) {
        Ok(e) => e,
        Err(e) => {
            warn!(
                payload_len = payload.len(),
                first_bytes = %hex::encode(&payload[..payload.len().min(16)]),
                "Failed to decode typed envelope: {}", e
            );
            return;
        }
    };
    // Radio-delivered → the active device's transport label ("lora" or
    // "reticulum"). Stamp after dispatch (see stamp helper).
    let before = max_message_id(state).await;
    handle_typed_envelope_direct(state, sender_contact_id, sender_name, envelope).await;
    let radio_transport = radio_transport_label(state.status.read().await.device_type);
    stamp_received_transport(state, sender_contact_id, before, radio_transport, false).await;
}

/// Highest stored message id right now. Paired with `stamp_received_transport`
/// to identify messages a dispatch call just stored (ids are monotonic).
pub(crate) async fn max_message_id(state: &Arc<MeshState>) -> u64 {
    state
        .messages
        .read()
        .await
        .iter()
        .map(|m| m.id)
        .max()
        .unwrap_or(0)
}

/// Stamp the per-message transport pill (and E2E flag) onto every RECEIVED
/// message from `sender_contact_id` stored since `after_id` — i.e. the ones the
/// just-completed `handle_typed_envelope_direct` produced. This is how both the
/// radio path ("lora") and the federation path ("fips"/"tor") tag inbound
/// messages without threading transport through all 20 typed-dispatch sites.
/// `encrypted` only ever sets the flag true (a federation envelope is E2E),
/// never clears a true set elsewhere.
pub(crate) async fn stamp_received_transport(
    state: &Arc<MeshState>,
    sender_contact_id: u32,
    after_id: u64,
    transport: &str,
    encrypted: bool,
) {
    let mut messages = state.messages.write().await;
    for m in messages.iter_mut() {
        if m.id > after_id
            && matches!(m.direction, MessageDirection::Received)
            && m.peer_contact_id == sender_contact_id
        {
            if m.transport.is_none() {
                m.transport = Some(transport.to_string());
            }
            if encrypted {
                m.encrypted = true;
            }
        }
    }
}

/// Mark every RECEIVED message stored since `after_id` as end-to-end encrypted.
/// Used by the session loop to stamp the E2E pill on a meshtastic frame the radio
/// reported PKI-encrypted (the synthetic frame can't carry that flag, and the
/// typed-dispatch store path defaults `encrypted` to false). One inbound frame
/// yields at most one received message, so no sender filter is needed.
pub(crate) async fn stamp_received_encrypted(state: &Arc<MeshState>, after_id: u64) {
    let mut messages = state.messages.write().await;
    for m in messages.iter_mut() {
        if m.id > after_id && matches!(m.direction, MessageDirection::Received) {
            m.encrypted = true;
        }
    }
}

/// Dispatch a pre-decoded TypedEnvelope. Shared between the radio receive
/// path (handle_typed_message above) and the federation receive path
/// (MeshService::inject_typed_from_federation) so both transports land the
/// same payload in the same chat store.
pub(crate) async fn handle_typed_envelope_direct(
    state: &Arc<MeshState>,
    sender_contact_id: u32,
    sender_name: &str,
    envelope: TypedEnvelope,
) {
    // Verify the envelope signature (if present) against the sender's known
    // Ed25519 key, and record whether the sender is cryptographically
    // authenticated. A federation peer (synthetic high-half contact_id) arrived
    // over the Tor relay, which verifies the sender signature upstream before
    // injecting here, so it counts as authenticated. This flag gates the
    // identity-based `!ai` allows (allowlist / federation-trust) downstream.
    let mut authenticated = sender_contact_id >= crate::mesh::FEDERATION_CONTACT_ID_BASE;
    if envelope.sig.is_some() {
        let peer_pubkey = state
            .peers
            .read()
            .await
            .get(&sender_contact_id)
            // Verify against the bound archipelago identity key, not the
            // firmware routing key — only the former is what the peer signs with.
            .and_then(|p| p.identity_pubkey_hex())
            .and_then(|hex_str| hex::decode(hex_str).ok())
            .and_then(|bytes| {
                if bytes.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    ed25519_dalek::VerifyingKey::from_bytes(&arr).ok()
                } else {
                    None
                }
            });
        if let Some(vk) = peer_pubkey {
            match envelope.verify_signature(&vk) {
                Ok(true) => {
                    authenticated = true;
                }
                Ok(false) => {
                    warn!(
                        peer = sender_contact_id,
                        "Dropping message with invalid signature"
                    );
                    return;
                }
                Err(e) => {
                    warn!(
                        peer = sender_contact_id,
                        "Signature verification error: {}", e
                    );
                    return;
                }
            }
        }
    }

    let msg_type = envelope.message_type();
    let type_label = msg_type.map(|t| t.label()).unwrap_or("unknown");
    info!(
        msg_type = type_label,
        from = sender_contact_id,
        "Received typed mesh message"
    );

    match msg_type {
        Some(MeshMessageType::BlockHeader) => {
            dispatch_block_header(&envelope, sender_contact_id, sender_name, state).await;
        }

        Some(MeshMessageType::Alert) => {
            match message_types::decode_payload::<message_types::AlertPayload>(&envelope.v) {
                Ok(alert) => {
                    let alert_type_str = format!("{:?}", alert.alert_type).to_lowercase();
                    info!(
                        alert_type = %alert_type_str,
                        from = sender_contact_id,
                        "Alert received via mesh: {}",
                        alert.message
                    );
                    let json = payload_to_json(&alert);
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &alert.message,
                        "alert",
                        json,
                        Some(envelope.seq),
                    )
                    .await;
                    let _ = state.event_tx.send(MeshEvent::AlertReceived {
                        alert_type: alert_type_str,
                        message: alert.message,
                        from_contact_id: sender_contact_id,
                    });
                }
                Err(e) => warn!("Failed to decode alert payload: {}", e),
            }
        }

        Some(MeshMessageType::TxRelay) => {
            match message_types::decode_payload::<message_types::TxRelayPayload>(&envelope.v) {
                Ok(relay) => {
                    // Validate transaction before relaying
                    if !super::super::bitcoin_relay::validate_raw_transaction(&relay.tx_hex) {
                        warn!(peer = sender_contact_id, "Rejected invalid TX relay");
                        return;
                    }
                    info!(
                        request_id = relay.request_id,
                        tx_len = relay.tx_hex.len(),
                        "TX relay request received — broadcasting to Bitcoin network"
                    );
                    let json = payload_to_json(&relay);
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &format!(
                            "TX relay request #{} ({} hex chars)",
                            relay.request_id,
                            relay.tx_hex.len()
                        ),
                        "tx_relay",
                        json,
                        Some(envelope.seq),
                    )
                    .await;

                    // Spawn async task to broadcast via Bitcoin RPC and track confirmations
                    let relay_state = Arc::clone(state);
                    let relay_contact = sender_contact_id;
                    tokio::spawn(async move {
                        handle_tx_relay_broadcast(relay, relay_contact, &relay_state).await;
                    });
                }
                Err(e) => warn!("Failed to decode TX relay payload: {}", e),
            }
        }

        Some(MeshMessageType::TxRelayResponse) => {
            dispatch_tx_relay_response(&envelope, sender_contact_id, sender_name, state).await;
        }

        Some(MeshMessageType::LightningRelay) => {
            match message_types::decode_payload::<message_types::LightningRelayPayload>(&envelope.v)
            {
                Ok(relay) => {
                    info!(
                        request_id = relay.request_id,
                        amount_sats = relay.amount_sats,
                        "Lightning relay request received"
                    );
                    let json = payload_to_json(&relay);
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &format!("Lightning relay: {} sats", relay.amount_sats),
                        "lightning_relay",
                        json,
                        Some(envelope.seq),
                    )
                    .await;
                    // Will be wired to LND in Week 9
                    let _ = state.event_tx.send(MeshEvent::LightningRelayCompleted {
                        request_id: relay.request_id,
                        payment_hash: None,
                        error: Some("Lightning relay processing not yet wired".to_string()),
                    });
                }
                Err(e) => warn!("Failed to decode Lightning relay payload: {}", e),
            }
        }

        Some(MeshMessageType::LightningRelayResponse) => {
            match message_types::decode_payload::<message_types::LightningRelayResponsePayload>(
                &envelope.v,
            ) {
                Ok(resp) => {
                    let status = if resp.payment_hash.is_some() {
                        "paid"
                    } else {
                        "failed"
                    };
                    info!(
                        request_id = resp.request_id,
                        status, "Lightning relay response"
                    );
                    let text = if let Some(ref hash) = resp.payment_hash {
                        format!("Lightning paid! hash: {}...", &hash[..16.min(hash.len())])
                    } else {
                        format!(
                            "Lightning failed: {}",
                            resp.error.as_deref().unwrap_or("unknown")
                        )
                    };
                    let json = payload_to_json(&resp);
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &text,
                        "lightning_relay_response",
                        json,
                        Some(envelope.seq),
                    )
                    .await;
                    let _ = state.event_tx.send(MeshEvent::LightningRelayCompleted {
                        request_id: resp.request_id,
                        payment_hash: resp.payment_hash,
                        error: resp.error,
                    });
                }
                Err(e) => warn!("Failed to decode Lightning relay response: {}", e),
            }
        }

        Some(MeshMessageType::Invoice) => {
            match message_types::decode_payload::<message_types::InvoicePayload>(&envelope.v) {
                Ok(invoice) => {
                    let text = format!(
                        "Invoice: {} sats{}",
                        invoice.amount_sats,
                        invoice
                            .memo
                            .as_ref()
                            .map(|m| format!(" — {}", m))
                            .unwrap_or_default()
                    );
                    let json = payload_to_json(&invoice);
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &text,
                        "invoice",
                        json,
                        Some(envelope.seq),
                    )
                    .await;
                }
                Err(e) => warn!("Failed to decode invoice payload: {}", e),
            }
        }

        Some(MeshMessageType::Coordinate) => {
            match message_types::decode_payload::<message_types::Coordinate>(&envelope.v) {
                Ok(coord) => {
                    let text = format!(
                        "Location: {:.6}, {:.6}{}",
                        coord.lat_degrees(),
                        coord.lng_degrees(),
                        coord
                            .label
                            .as_ref()
                            .map(|l| format!(" ({})", l))
                            .unwrap_or_default()
                    );
                    let json = payload_to_json(&coord);
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &text,
                        "coordinate",
                        json,
                        Some(envelope.seq),
                    )
                    .await;
                }
                Err(e) => warn!("Failed to decode coordinate payload: {}", e),
            }
        }

        Some(MeshMessageType::TxConfirmation) => {
            dispatch_tx_confirmation(&envelope, sender_contact_id, sender_name, state).await;
        }

        Some(MeshMessageType::Reply) => {
            match message_types::decode_payload::<message_types::ReplyPayload>(&envelope.v) {
                Ok(reply) => {
                    let json = payload_to_json(&reply);
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &reply.text,
                        "reply",
                        json,
                        Some(envelope.seq),
                    )
                    .await;
                }
                Err(e) => warn!("Failed to decode reply payload: {}", e),
            }
        }

        Some(MeshMessageType::Reaction) => {
            match message_types::decode_payload::<message_types::ReactionPayload>(&envelope.v) {
                Ok(reaction) => {
                    let display = if reaction.emoji.is_empty() {
                        "(reaction cleared)".to_string()
                    } else {
                        reaction.emoji.clone()
                    };
                    let json = payload_to_json(&reaction);
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &display,
                        "reaction",
                        json,
                        Some(envelope.seq),
                    )
                    .await;
                }
                Err(e) => warn!("Failed to decode reaction payload: {}", e),
            }
        }

        Some(MeshMessageType::Presence) => {
            match message_types::decode_payload::<message_types::PresencePayload>(&envelope.v) {
                Ok(p) => {
                    let sender_pubkey = state
                        .peers
                        .read()
                        .await
                        .get(&sender_contact_id)
                        .and_then(|peer| peer.pubkey_hex.clone());
                    if let Some(pk) = sender_pubkey {
                        let now = chrono::Utc::now().timestamp() as u64;
                        state
                            .presence
                            .write()
                            .await
                            .insert(pk, (p.status, p.last_active, now));
                    }
                }
                Err(e) => warn!("Failed to decode presence payload: {}", e),
            }
        }

        Some(MeshMessageType::LightningInfo) => {
            match message_types::decode_payload::<message_types::LightningInfoPayload>(&envelope.v)
            {
                Ok(info) => {
                    // Validate BEFORE touching stored state. This arrives over
                    // unauthenticated RF, and a peer that has already given us a
                    // good URI must not lose it to a later malformed one —
                    // otherwise anyone in range could blank out a real peer's
                    // entry in the channel-open picker (T-01-12).
                    if !message_types::is_valid_lightning_uri(&info.uri) {
                        warn!(
                            "Rejecting malformed lightning_info URI from contact {} — \
                             keeping any previously stored URI",
                            sender_contact_id
                        );
                        return;
                    }
                    let mut peers = state.peers.write().await;
                    if let Some(peer) = peers.get_mut(&sender_contact_id) {
                        // Newest advertisement wins: a node that moves host or
                        // rotates its port re-advertises, and the stale entry
                        // would just fail to dial. Overwrite, never accumulate.
                        peer.lightning_uri = Some(info.uri);
                    } else {
                        warn!(
                            "lightning_info from unknown contact {} — dropped (a peer record is \
                             created by identity/contact discovery, not by this advertisement)",
                            sender_contact_id
                        );
                    }
                }
                Err(e) => warn!("Failed to decode lightning_info payload: {}", e),
            }
        }

        Some(MeshMessageType::ChannelInvite) => {
            match message_types::decode_payload::<message_types::ChannelInvitePayload>(&envelope.v)
            {
                Ok(inv) => {
                    let display = format!("Invited to channel {} ({})", inv.channel, inv.name);
                    let json = payload_to_json(&inv);
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &display,
                        "channel_invite",
                        json,
                        Some(envelope.seq),
                    )
                    .await;
                }
                Err(e) => warn!("Failed to decode channel_invite payload: {}", e),
            }
        }

        Some(MeshMessageType::PsbtHash) => {
            match message_types::decode_payload::<message_types::PsbtHashPayload>(&envelope.v) {
                Ok(psbt) => {
                    let display = format!("PSBT {} sats — {}", psbt.amount_sats, psbt.description);
                    let json = payload_to_json(&psbt);
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &display,
                        "psbt_hash",
                        json,
                        Some(envelope.seq),
                    )
                    .await;
                }
                Err(e) => warn!("Failed to decode psbt_hash payload: {}", e),
            }
        }

        Some(MeshMessageType::ReadReceipt) => {
            match message_types::decode_payload::<message_types::ReadReceiptPayload>(&envelope.v) {
                Ok(receipt) => {
                    // Roll the ✓✓ delivered marker forward on every Sent
                    // message whose MessageKey ≤ (receipt.up_to.sender_pubkey,
                    // receipt.up_to.sender_seq) addressed to this peer. The
                    // pubkey in `up_to` is OUR pubkey (the peer echoing back
                    // what they've read), so match on sender_seq only within
                    // own-Sent records for this peer_contact_id.
                    let mut messages = state.messages.write().await;
                    for m in messages.iter_mut() {
                        if matches!(m.direction, MessageDirection::Sent)
                            && m.peer_contact_id == sender_contact_id
                            && m.sender_seq
                                .map(|s| s <= receipt.up_to.sender_seq)
                                .unwrap_or(false)
                        {
                            m.delivered = true;
                        }
                    }
                    drop(messages);
                    info!(
                        peer = sender_contact_id,
                        up_to = receipt.up_to.sender_seq,
                        "Applied read receipt"
                    );
                }
                Err(e) => warn!("Failed to decode read_receipt payload: {}", e),
            }
        }

        Some(MeshMessageType::Forward) => {
            match message_types::decode_payload::<message_types::ForwardPayload>(&envelope.v) {
                Ok(fwd) => {
                    let name = fwd.orig_name.as_deref().unwrap_or("someone");
                    let text = format!("Forwarded from {}", name);
                    let json = payload_to_json(&fwd);
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &text,
                        "forward",
                        json,
                        Some(envelope.seq),
                    )
                    .await;
                }
                Err(e) => warn!("Failed to decode forward payload: {}", e),
            }
        }

        Some(MeshMessageType::Edit) => {
            match message_types::decode_payload::<message_types::EditPayload>(&envelope.v) {
                Ok(edit) => {
                    // Only accept edits from the pubkey the target claims to
                    // come from — prevents peer A editing a message authored
                    // by peer B.
                    let sender_pubkey = state
                        .peers
                        .read()
                        .await
                        .get(&sender_contact_id)
                        .and_then(|p| p.pubkey_hex.clone());
                    if sender_pubkey.as_deref() != Some(edit.target.sender_pubkey.as_str()) {
                        warn!(
                            peer = sender_contact_id,
                            "Rejecting edit — sender pubkey does not match target"
                        );
                    } else {
                        let mut messages = state.messages.write().await;
                        for m in messages.iter_mut() {
                            if m.sender_pubkey.as_deref()
                                == Some(edit.target.sender_pubkey.as_str())
                                && m.sender_seq == Some(edit.target.sender_seq)
                            {
                                m.plaintext = edit.new_text.clone();
                                let mut obj = match m.typed_payload.take() {
                                    Some(serde_json::Value::Object(o)) => o,
                                    _ => serde_json::Map::new(),
                                };
                                obj.insert(
                                    "edited_at".to_string(),
                                    serde_json::json!(edit.edited_at),
                                );
                                obj.insert("text".to_string(), serde_json::json!(edit.new_text));
                                m.typed_payload = Some(serde_json::Value::Object(obj));
                                break;
                            }
                        }
                    }
                }
                Err(e) => warn!("Failed to decode edit payload: {}", e),
            }
        }

        Some(MeshMessageType::Delete) => {
            match message_types::decode_payload::<message_types::DeletePayload>(&envelope.v) {
                Ok(del) => {
                    let sender_pubkey = state
                        .peers
                        .read()
                        .await
                        .get(&sender_contact_id)
                        .and_then(|p| p.pubkey_hex.clone());
                    if sender_pubkey.as_deref() != Some(del.target.sender_pubkey.as_str()) {
                        warn!(
                            peer = sender_contact_id,
                            "Rejecting delete — sender pubkey does not match target"
                        );
                    } else {
                        let mut messages = state.messages.write().await;
                        for m in messages.iter_mut() {
                            if m.sender_pubkey.as_deref() == Some(del.target.sender_pubkey.as_str())
                                && m.sender_seq == Some(del.target.sender_seq)
                            {
                                m.plaintext = "🗑 message deleted".to_string();
                                m.typed_payload = Some(serde_json::json!({ "deleted": true }));
                                m.message_type = "delete".to_string();
                                break;
                            }
                        }
                    }
                }
                Err(e) => warn!("Failed to decode delete payload: {}", e),
            }
        }

        Some(MeshMessageType::ContentInline) => {
            match message_types::decode_payload::<message_types::ContentInlinePayload>(&envelope.v)
            {
                Ok(content) => {
                    let text = match (&content.filename, &content.caption) {
                        (Some(fname), Some(c)) => format!("📎 {} — {}", fname, c),
                        (Some(fname), None) => format!("📎 {}", fname),
                        (None, Some(c)) => format!("📎 {}", c),
                        (None, None) => {
                            format!("📎 {} ({} bytes)", content.mime, content.bytes.len())
                        }
                    };
                    // Write bytes to local BlobStore so the standard
                    // content_ref renderer can display it via /blob/<cid>.
                    let mut cid_opt: Option<String> = None;
                    let mut size: u64 = content.bytes.len() as u64;
                    if let Some(store) = state.blob_store.read().await.as_ref().cloned() {
                        match store
                            .put(
                                &content.bytes,
                                &content.mime,
                                content.filename.clone(),
                                None,
                                false,
                            )
                            .await
                        {
                            Ok(meta) => {
                                cid_opt = Some(meta.cid);
                                size = meta.size;
                            }
                            Err(e) => warn!("Failed to persist inline attachment: {}", e),
                        }
                    } else {
                        warn!("Blob store not set on mesh state; inline attachment not persisted");
                    }
                    let json = serde_json::json!({
                        "cid": cid_opt,
                        "size": size,
                        "mime": content.mime,
                        "filename": content.filename,
                        "caption": content.caption,
                        "inline": true,
                    });
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &text,
                        "content_ref",
                        Some(json),
                        Some(envelope.seq),
                    )
                    .await;
                }
                Err(e) => warn!("Failed to decode content_inline payload: {}", e),
            }
        }

        Some(MeshMessageType::ContentRef) => {
            match message_types::decode_payload::<message_types::ContentRefPayload>(&envelope.v) {
                Ok(content) => {
                    let text = match (&content.filename, &content.caption) {
                        (Some(fname), Some(c)) => format!("📎 {} — {}", fname, c),
                        (Some(fname), None) => format!("📎 {}", fname),
                        (None, Some(c)) => format!("📎 {}", c),
                        (None, None) => format!("📎 {} ({} bytes)", content.mime, content.size),
                    };
                    let json = payload_to_json(&content);
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &text,
                        "content_ref",
                        json,
                        Some(envelope.seq),
                    )
                    .await;
                }
                Err(e) => warn!("Failed to decode content_ref payload: {}", e),
            }
        }

        Some(MeshMessageType::Text) => {
            // Typed text arrives with a `sender_seq` in the envelope, so we
            // can store it with a full MessageKey instead of the bare
            // plain-text path — that's what makes replies and reactions
            // addressable against text bubbles.
            let text = String::from_utf8_lossy(&envelope.v).to_string();
            store_typed_message(
                state,
                sender_contact_id,
                sender_name,
                &text,
                "text",
                None,
                Some(envelope.seq),
            )
            .await;

            // `!archy [sub]` typed in the normal 1:1 chat — deterministic node
            // status, answered from local caches with no model in the loop, so
            // it stays available with the assistant off. The trust gate is the
            // same `is_sender_allowed` the assistant uses.
            if let Some(rest) = super::node_cmd::strip_archy_trigger(&text) {
                let req_id = state.next_id().await;
                let rest = rest.to_string();
                let name = sender_name.to_string();
                let cid = sender_contact_id;
                let st = Arc::clone(state);
                tokio::spawn(async move {
                    super::node_cmd::run_node_cmd(
                        rest,
                        req_id,
                        cid,
                        name,
                        authenticated,
                        super::assist::AssistReply::ChatText { contact_id: cid },
                        st,
                    )
                    .await;
                });
            }
            // Mesh-AI assistant (issue #50): a `!ai`/`!ask <question>` typed in
            // the normal 1:1 chat triggers this node's assistant, with the
            // answer sent back as a chat bubble in the same thread. The typed
            // DM carries the peer's federation identity (via sender_contact_id),
            // so the `trusted_only` gate in run_assist resolves correctly —
            // unlike the bare channel-text path, which only knows the radio key.
            else if state.assistant.read().await.enabled {
                if let Some(prompt) = super::decode::strip_ai_trigger(&text) {
                    if !prompt.is_empty() {
                        let req_id = state.next_id().await;
                        let prompt = prompt.to_string();
                        let name = sender_name.to_string();
                        let cid = sender_contact_id;
                        let st = Arc::clone(state);
                        tokio::spawn(async move {
                            super::assist::run_assist(
                                prompt,
                                None,
                                req_id,
                                cid,
                                name,
                                authenticated,
                                super::assist::AssistReply::ChatText { contact_id: cid },
                                st,
                            )
                            .await;
                        });
                    }
                }
            }
        }

        Some(MeshMessageType::AssistQuery) => {
            match message_types::decode_payload::<message_types::AssistQueryPayload>(&envelope.v) {
                Ok(query) => {
                    if !state.assistant.read().await.enabled {
                        debug!(
                            from = sender_contact_id,
                            "AssistQuery ignored — assistant disabled on this node"
                        );
                        return;
                    }
                    info!(
                        from = sender_contact_id,
                        req_id = query.req_id,
                        "AI query received over mesh"
                    );
                    let json = payload_to_json(&query);
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &query.prompt,
                        "assist_query",
                        json,
                        Some(envelope.seq),
                    )
                    .await;
                    // Run the model + reply off the radio loop. Typed query →
                    // typed chunked reply back to the asking peer.
                    let assist_state = Arc::clone(state);
                    let name = sender_name.to_string();
                    tokio::spawn(async move {
                        super::assist::run_assist(
                            query.prompt,
                            query.model,
                            query.req_id,
                            sender_contact_id,
                            name,
                            authenticated,
                            super::assist::AssistReply::Typed {
                                contact_id: sender_contact_id,
                            },
                            assist_state,
                        )
                        .await;
                    });
                }
                Err(e) => warn!("Failed to decode AssistQuery payload: {}", e),
            }
        }

        Some(MeshMessageType::AssistResponse) => {
            match message_types::decode_payload::<message_types::AssistResponsePayload>(&envelope.v)
            {
                Ok(resp) => {
                    let display = resp
                        .error
                        .clone()
                        .map(|e| format!("AI error: {e}"))
                        .unwrap_or_else(|| resp.text.clone());
                    let json = payload_to_json(&resp);
                    store_typed_message(
                        state,
                        sender_contact_id,
                        sender_name,
                        &display,
                        "assist_response",
                        json,
                        Some(envelope.seq),
                    )
                    .await;
                }
                Err(e) => warn!("Failed to decode AssistResponse payload: {}", e),
            }
        }

        _ => {
            debug!(
                msg_type = ?msg_type,
                "Unhandled typed message type"
            );
        }
    }
}

/// Dispatch a BlockHeader typed message.
async fn dispatch_block_header(
    envelope: &TypedEnvelope,
    sender_contact_id: u32,
    sender_name: &str,
    state: &Arc<MeshState>,
) {
    // Respect the receive toggle (issue #28): nodes can opt out of inbound headers.
    if !state.receive_block_headers {
        return;
    }
    // Compact binary format: height(8) + hash(32) + timestamp(4)
    match super::super::bitcoin_relay::decode_compact_block_header(&envelope.v) {
        Ok((height, hash_hex, timestamp)) => {
            // Validate header before accepting
            let last_known = state.block_header_cache.latest_height().await;
            if !super::super::bitcoin_relay::validate_block_header(
                height, &hash_hex, timestamp, last_known,
            ) {
                warn!(
                    peer = sender_contact_id,
                    height, "Rejected invalid block header"
                );
                return;
            }

            info!(
                height,
                hash = %hash_hex,
                "Block header received via mesh"
            );

            // Store in block header cache for the Off-Grid Bitcoin panel
            let header_payload = message_types::BlockHeaderPayload {
                height,
                hash: hash_hex.clone(),
                prev_hash: String::new(),
                timestamp,
                announced_by: sender_name.to_string(),
            };
            let _ = state
                .block_header_cache
                .store_header(header_payload.clone())
                .await;

            let text = format!(
                "Block #{} — {}...{}",
                height,
                &hash_hex[..8.min(hash_hex.len())],
                &hash_hex[hash_hex.len().saturating_sub(8)..]
            );
            let json = payload_to_json(&header_payload);
            store_typed_message(
                state,
                sender_contact_id,
                sender_name,
                &text,
                "block_header",
                json,
                Some(envelope.seq),
            )
            .await;
            let _ = state.event_tx.send(MeshEvent::BlockHeaderReceived {
                height,
                hash: hash_hex,
            });
        }
        Err(e) => warn!("Failed to decode block header: {}", e),
    }
}

/// Dispatch a TxRelayResponse typed message.
async fn dispatch_tx_relay_response(
    envelope: &TypedEnvelope,
    sender_contact_id: u32,
    sender_name: &str,
    state: &Arc<MeshState>,
) {
    match message_types::decode_payload::<message_types::TxRelayResponsePayload>(&envelope.v) {
        Ok(resp) => {
            let status = if resp.txid.is_some() {
                "confirmed"
            } else {
                "failed"
            };
            info!(
                request_id = resp.request_id,
                status,
                error_code = resp.error_code.as_deref().unwrap_or("none"),
                "TX relay response received"
            );
            let text = if let Some(ref txid) = resp.txid {
                format!(
                    "TX relayed! txid: {}...{}",
                    &txid[..8.min(txid.len())],
                    &txid[txid.len().saturating_sub(8)..]
                )
            } else if let Some(ref code) = resp.error_code {
                format!(
                    "TX relay failed [{}]: {}",
                    code,
                    resp.error.as_deref().unwrap_or("unknown")
                )
            } else {
                format!(
                    "TX relay failed: {}",
                    resp.error.as_deref().unwrap_or("unknown")
                )
            };
            let json = payload_to_json(&resp);
            store_typed_message(
                state,
                sender_contact_id,
                sender_name,
                &text,
                "tx_relay_response",
                json,
                Some(envelope.seq),
            )
            .await;
            // Store result for frontend polling
            if let Some(ref tracker) = state.relay_tracker {
                tracker
                    .store_result(super::super::bitcoin_relay::RelayResult {
                        request_id: resp.request_id,
                        txid: resp.txid.clone(),
                        error: resp.error.clone(),
                        error_code: resp.error_code.clone(),
                        completed_at: chrono::Utc::now().to_rfc3339(),
                    })
                    .await;
            }
            let _ = state.event_tx.send(MeshEvent::TxRelayCompleted {
                request_id: resp.request_id,
                txid: resp.txid,
                error: resp.error,
            });
        }
        Err(e) => warn!("Failed to decode TX relay response: {}", e),
    }
}

/// Dispatch a TxConfirmation typed message.
async fn dispatch_tx_confirmation(
    envelope: &TypedEnvelope,
    sender_contact_id: u32,
    sender_name: &str,
    state: &Arc<MeshState>,
) {
    match message_types::decode_payload::<message_types::TxConfirmationPayload>(&envelope.v) {
        Ok(conf) => {
            let status_text = if conf.confirmations >= 3 {
                format!(
                    "TX {} confirmed ({}/3) at block #{}",
                    &conf.txid[..12.min(conf.txid.len())],
                    conf.confirmations,
                    conf.block_height
                )
            } else {
                format!(
                    "TX {} — {}/3 confirmations (block #{})",
                    &conf.txid[..12.min(conf.txid.len())],
                    conf.confirmations,
                    conf.block_height
                )
            };
            info!(
                txid = %conf.txid,
                confirmations = conf.confirmations,
                block_height = conf.block_height,
                "TX confirmation update received"
            );
            let json = payload_to_json(&conf);
            store_typed_message(
                state,
                sender_contact_id,
                sender_name,
                &status_text,
                "tx_confirmation",
                json,
                Some(envelope.seq),
            )
            .await;
            // Store confirmation for frontend polling
            if let Some(ref tracker) = state.relay_tracker {
                tracker
                    .store_result(super::super::bitcoin_relay::RelayResult {
                        request_id: conf.request_id,
                        txid: Some(conf.txid.clone()),
                        error: None,
                        error_code: None,
                        completed_at: chrono::Utc::now().to_rfc3339(),
                    })
                    .await;
            }
            let _ = state.event_tx.send(MeshEvent::TxRelayCompleted {
                request_id: conf.request_id,
                txid: Some(conf.txid),
                error: None,
            });
        }
        Err(e) => warn!("Failed to decode TX confirmation: {}", e),
    }
}
