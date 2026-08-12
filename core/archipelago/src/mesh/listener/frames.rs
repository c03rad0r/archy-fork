//! Inbound frame dispatcher — routes device frames to the appropriate handler.

use super::super::message_types::TypedEnvelope;
use super::super::protocol;
use super::decode::{
    handle_identity_received, is_mc_chunk_frame, resolve_peer, store_plain_message,
    store_plain_message_with_encryption, try_base64_typed, try_chunk_reassemble,
    try_decrypt_base64, try_decrypt_ratchet_base64, update_peer_snr,
};
use super::dispatch::handle_typed_message;
use super::MeshState;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Handle a single inbound frame from the device.
/// Returns `true` if contacts should be refreshed from the device.
pub(super) async fn handle_frame(
    frame: &protocol::InboundFrame,
    state: &Arc<MeshState>,
    our_x25519_secret: &[u8; 32],
) -> bool {
    match frame.code {
        protocol::PUSH_NEW_CONTACT | protocol::PUSH_CONTACT_ADVERT => {
            info!(
                code = frame.code,
                data_len = frame.data.len(),
                "Contact discovery event — refreshing contacts"
            );
            // Auto-import: a PUSH_CONTACT_ADVERT (0x80) carries the 32-byte
            // pubkey of a node we just heard. If it isn't already a contact,
            // add it to the firmware table so it shows up immediately — no
            // flood-advert dance required. (PUSH_NEW_CONTACT/0x8A is already
            // added by the firmware, so we skip it.)
            if frame.code == protocol::PUSH_CONTACT_ADVERT && frame.data.len() >= 32 {
                let mut pubkey = [0u8; 32];
                pubkey.copy_from_slice(&frame.data[..32]);
                let pk_hex = hex::encode(pubkey);
                let known = state
                    .peers
                    .read()
                    .await
                    .values()
                    .any(|p| p.pubkey_hex.as_deref() == Some(pk_hex.as_str()));
                if !known {
                    let _ = state
                        .send_cmd(super::MeshCommand::AddContact {
                            pubkey,
                            name: String::new(),
                        })
                        .await;
                }
            }
            return true; // Signal caller to fetch contacts
        }

        protocol::PUSH_ACK => {
            debug!("Message delivery confirmed");
            // Could track which message was ACKed from frame.data
        }

        protocol::PUSH_MESSAGES_WAITING => {
            info!("Device has messages waiting — will sync");
            return true; // Signal caller to sync immediately
        }

        protocol::RESP_CONTACT_MSG_V3 | protocol::RESP_CONTACT_MSG_V3_E2E => {
            // Direct message received (v3 format) — check for typed envelope first
            match protocol::parse_contact_msg_v3_raw(&frame.data) {
                Ok((sender_prefix, payload, snr)) => {
                    if !payload.is_empty() {
                        let encrypted = frame.code == protocol::RESP_CONTACT_MSG_V3_E2E;
                        let (contact_id, name) = resolve_peer(state, &sender_prefix).await;
                        update_peer_snr(state, contact_id, snr as f32).await;
                        if TypedEnvelope::is_typed(&payload) {
                            handle_typed_message(&payload, contact_id, &name, state).await;
                        } else if let Some(decoded) = try_base64_typed(&payload) {
                            handle_typed_message(&decoded, contact_id, &name, state).await;
                        } else if let Some(decoded) =
                            try_decrypt_ratchet_base64(&payload, contact_id, state).await
                        {
                            handle_typed_message(&decoded, contact_id, &name, state).await;
                        } else if let Some(decoded) =
                            try_decrypt_base64(&payload, contact_id, state).await
                        {
                            handle_typed_message(&decoded, contact_id, &name, state).await;
                        } else if let Some(decoded) =
                            try_chunk_reassemble(&payload, contact_id, state).await
                        {
                            handle_typed_message(&decoded, contact_id, &name, state).await;
                        } else if !payload.starts_with(b"MC") {
                            let text = String::from_utf8_lossy(&payload).to_string();
                            store_plain_message_with_encryption(
                                state, contact_id, &name, &text, encrypted,
                            )
                            .await;
                            info!(from = %sender_prefix, "Received mesh DM (v3)");
                        }
                    }
                }
                Err(e) => warn!("Failed to parse v3 message: {}", e),
            }
        }

        protocol::RESP_CONTACT_MSG => {
            // Direct message received (v1 format)
            match protocol::parse_contact_msg_v1_raw(&frame.data) {
                Ok((sender_prefix, payload)) => {
                    if !payload.is_empty() {
                        let (contact_id, name) = resolve_peer(state, &sender_prefix).await;
                        if TypedEnvelope::is_typed(&payload) {
                            handle_typed_message(&payload, contact_id, &name, state).await;
                        } else if let Some(decoded) = try_base64_typed(&payload) {
                            handle_typed_message(&decoded, contact_id, &name, state).await;
                        } else if let Some(decoded) =
                            try_decrypt_ratchet_base64(&payload, contact_id, state).await
                        {
                            handle_typed_message(&decoded, contact_id, &name, state).await;
                        } else if let Some(decoded) =
                            try_decrypt_base64(&payload, contact_id, state).await
                        {
                            handle_typed_message(&decoded, contact_id, &name, state).await;
                        } else if let Some(decoded) =
                            try_chunk_reassemble(&payload, contact_id, state).await
                        {
                            handle_typed_message(&decoded, contact_id, &name, state).await;
                        } else if !payload.starts_with(b"MC") {
                            let text = String::from_utf8_lossy(&payload).to_string();
                            store_plain_message(state, contact_id, &name, &text).await;
                            info!(from = %sender_prefix, "Received mesh DM (v1)");
                        }
                    }
                }
                Err(e) => warn!("Failed to parse v1 message: {}", e),
            }
        }

        protocol::RESP_CHANNEL_MSG_V3 => {
            // Channel broadcast received (v3) — check for typed envelope
            match protocol::parse_channel_msg_v3_raw(&frame.data) {
                Ok((channel_idx, payload)) => {
                    if !payload.is_empty() {
                        handle_channel_payload(
                            state,
                            channel_idx,
                            &payload,
                            our_x25519_secret,
                            None,
                        )
                        .await;
                    }
                }
                Err(e) => warn!("Failed to parse v3 channel message: {}", e),
            }
        }

        protocol::RESP_CHANNEL_MSG => {
            // Channel broadcast received (v1)
            match protocol::parse_channel_msg_v1_raw(&frame.data) {
                Ok((channel_idx, payload)) => {
                    if !payload.is_empty() {
                        handle_channel_payload(
                            state,
                            channel_idx,
                            &payload,
                            our_x25519_secret,
                            None,
                        )
                        .await;
                    }
                }
                Err(e) => warn!("Failed to parse channel message: {}", e),
            }
        }

        // Synthetic Meshtastic channel broadcast that carries its sender:
        // `[channel_idx: u8][sender_pubkey_prefix: 6 bytes][text…]`. Resolve the
        // sender to a friendly name, then file the message under the channel
        // thread attributed to them — this is what makes the default public
        // LongFast channel actually show inbound traffic (and who sent it).
        protocol::RESP_MESHTASTIC_CHANNEL_TEXT => {
            if frame.data.len() > 7 {
                let channel_idx = frame.data[0];
                let sender_prefix_hex = hex::encode(&frame.data[1..7]);
                let payload = frame.data[7..].to_vec();
                if !payload.is_empty() {
                    let (_cid, name) = resolve_peer(state, &sender_prefix_hex).await;
                    handle_channel_payload(
                        state,
                        channel_idx,
                        &payload,
                        our_x25519_secret,
                        Some(name),
                    )
                    .await;
                }
            }
        }

        protocol::PUSH_LOG_DATA | protocol::PUSH_PATH_UPDATE | protocol::PUSH_RAW_DATA => {
            // Internal device logging/path data — safe to ignore
        }

        _ => {
            if protocol::is_push_notification(frame.code) {
                debug!(code = frame.code, "Unhandled push notification");
            }
        }
    }
    false
}

/// Process a channel-broadcast payload. If the payload carries the
/// DM-via-channel marker and the destination prefix matches any of our
/// local mesh peer pubkeys (or we can't tell), the inner payload is
/// dispatched through the direct-message path so it lands in the right
/// chat. Otherwise it's handled as a normal channel text/typed message.
async fn handle_channel_payload(
    state: &Arc<MeshState>,
    channel_idx: u8,
    payload: &[u8],
    our_x25519_secret: &[u8; 32],
    // When the transport knows who sent this channel broadcast (Meshtastic
    // packets carry the originating node), the plain-text/typed message is filed
    // under the channel thread but attributed to this sender name. Meshcore
    // channel frames carry no sender, so they pass `None` and fall back to a
    // generic "Channel N" label.
    sender_name: Option<String>,
) {
    // DM-via-channel wrapper (text form): the channel text carries an
    // ASCII "@DM:<base64>" token somewhere in the body. We locate the
    // marker anywhere in the payload (the firmware auto-prepends the
    // sender's `"<advert_name>: "` before our bytes, so the marker is
    // not at offset 0), then base64-decode to get `[dest(6)][inner]`.
    // Using a text marker + base64 avoids the C-string NUL truncation
    // that broke the previous raw-byte wrapper on the firmware's side.
    let text_view = std::str::from_utf8(payload).ok();

    // v2 format: `@DM2:` + base64(`[dest(6)][sender_arch(6)][inner…]`).
    // Carries a sender prefix so we can attribute the message to the real
    // sender's contact_id instead of guessing — fixes the long-standing
    // bug where every inbound DM-via-channel was misattributed to whichever
    // `Archy-*` peer happened to have the lowest contact_id in the firmware
    // contact table (the third-device thread on .39/.76).
    if let Some(idx) = text_view.and_then(|t| t.find("@DM2:")) {
        use base64::Engine;
        let b64 = &text_view.unwrap()[idx + 5..];
        let b64 = b64.trim_end_matches(|c: char| c == '\0' || c.is_whitespace());
        match base64::engine::general_purpose::STANDARD.decode(b64) {
            Ok(body) if body.len() >= 12 => {
                let dest_prefix: [u8; 6] = body[..6].try_into().expect("sliced 6 bytes");
                let sender_prefix: [u8; 6] = body[6..12].try_into().expect("sliced 6 bytes");
                let inner_vec = body[12..].to_vec();
                let inner: &[u8] = &inner_vec;
                let addressed_to_us = dest_prefix_is_us(state, &dest_prefix).await;
                if !addressed_to_us {
                    debug!(
                        dest = %hex::encode(dest_prefix),
                        inner_len = inner.len(),
                        "Dropping DM2-via-channel (not for us)"
                    );
                    return;
                }
                info!(
                    dest = %hex::encode(dest_prefix),
                    sender = %hex::encode(sender_prefix),
                    inner_len = inner.len(),
                    channel = channel_idx,
                    "Received DM2 via channel (addressed to us)"
                );
                let (contact_id, name) =
                    resolve_sender_by_arch_prefix(state, &sender_prefix, &dest_prefix).await;
                if TypedEnvelope::is_typed(inner) {
                    handle_typed_message(inner, contact_id, &name, state).await;
                } else if let Some(decoded) = try_base64_typed(inner) {
                    handle_typed_message(&decoded, contact_id, &name, state).await;
                } else if let Some(decoded) = try_chunk_reassemble(inner, contact_id, state).await {
                    if TypedEnvelope::is_typed(&decoded) {
                        handle_typed_message(&decoded, contact_id, &name, state).await;
                    } else {
                        let text = String::from_utf8_lossy(&decoded).to_string();
                        store_plain_message(state, contact_id, &name, &text).await;
                    }
                } else if is_mc_chunk_frame(inner) {
                    // Chunk buffered for reassembly — do not store the raw
                    // `MCxxyyzz…` frame as its own plaintext message. Wait
                    // for the rest of the chunks to arrive.
                    debug!(inner_len = inner.len(), "DM2 chunk buffered");
                } else {
                    let text = String::from_utf8_lossy(inner).to_string();
                    store_plain_message(state, contact_id, &name, &text).await;
                }
                return;
            }
            Ok(_) => debug!("DM2-via-channel b64 decoded too short"),
            Err(e) => debug!("DM2-via-channel b64 decode failed: {}", e),
        }
    }

    if let Some(idx) = text_view.and_then(|t| t.find("@DM:")) {
        use base64::Engine;
        let b64 = &text_view.unwrap()[idx + 4..];
        // Trim any trailing whitespace / NULs that firmware may append.
        let b64 = b64.trim_end_matches(|c: char| c == '\0' || c.is_whitespace());
        match base64::engine::general_purpose::STANDARD.decode(b64) {
            Ok(body) if body.len() >= 6 => {
                let dest_prefix: [u8; 6] = body[..6].try_into().expect("sliced 6 bytes");
                let inner_vec = body[6..].to_vec();
                let inner: &[u8] = &inner_vec;
                let addressed_to_us = dest_prefix_is_us(state, &dest_prefix).await;
                if !addressed_to_us {
                    debug!(
                        dest = %hex::encode(dest_prefix),
                        inner_len = inner.len(),
                        "Dropping DM-via-channel (not for us)"
                    );
                    return;
                }
                info!(
                    dest = %hex::encode(dest_prefix),
                    inner_len = inner.len(),
                    channel = channel_idx,
                    "Received DM via channel (addressed to us)"
                );
                let (contact_id, name) = resolve_counterparty(state, &dest_prefix).await;
                if TypedEnvelope::is_typed(inner) {
                    handle_typed_message(inner, contact_id, &name, state).await;
                } else if let Some(decoded) = try_base64_typed(inner) {
                    handle_typed_message(&decoded, contact_id, &name, state).await;
                } else if let Some(decoded) = try_chunk_reassemble(inner, contact_id, state).await {
                    if TypedEnvelope::is_typed(&decoded) {
                        handle_typed_message(&decoded, contact_id, &name, state).await;
                    } else {
                        let text = String::from_utf8_lossy(&decoded).to_string();
                        store_plain_message(state, contact_id, &name, &text).await;
                    }
                } else if is_mc_chunk_frame(inner) {
                    debug!(inner_len = inner.len(), "DM chunk buffered");
                } else {
                    let text = String::from_utf8_lossy(inner).to_string();
                    store_plain_message(state, contact_id, &name, &text).await;
                }
                return;
            }
            Ok(_) => debug!("DM-via-channel b64 decoded too short"),
            Err(e) => debug!("DM-via-channel b64 decode failed: {}", e),
        }
    }

    // Legacy raw-byte wrapper kept as a defensive no-op.
    if payload.len() >= 7 && payload[0] == protocol::DM_VIA_CHANNEL_MARKER {
        let dest_prefix: [u8; 6] = payload[1..7].try_into().expect("sliced 6 bytes");
        let inner = &payload[7..];

        // If the destination prefix matches a contact we know about that
        // isn't ourselves, forward it (the channel broadcast is shared by
        // everyone but only the intended recipient should treat it as a
        // DM). We compare against our mesh contacts — if the prefix is
        // not one of our known peers AND not our self_node_id, we drop
        // because it's someone else's DM bouncing through the mesh.
        let addressed_to_us = dest_prefix_is_us(state, &dest_prefix).await;
        if !addressed_to_us {
            debug!(
                dest = %hex::encode(dest_prefix),
                inner_len = inner.len(),
                "Dropping DM-via-channel (not for us)"
            );
            return;
        }

        info!(
            dest = %hex::encode(dest_prefix),
            inner_len = inner.len(),
            channel = channel_idx,
            "Received DM via channel (addressed to us)"
        );

        // Treat the inner payload exactly the same as we'd treat a direct
        // unicast frame — resolve the sender from our peer table (we
        // don't know the sender here, so use a synthetic-ish contact_id
        // derived from the first peer whose dest_prefix != us), and
        // dispatch through the typed / base64 / plain-text ladder.
        // Because the wrapped frame doesn't carry the sender prefix, we
        // pick "the other side of the conversation" — there are only two
        // known archipelago peers in the radio neighborhood, so the
        // sender is whoever isn't us. For the typical 2-node setup this
        // is correct. When there are more peers, upper layers (typed
        // envelope sender_pubkey) will carry the real sender identity.
        let (contact_id, name) = resolve_counterparty(state, &dest_prefix).await;
        if TypedEnvelope::is_typed(inner) {
            handle_typed_message(inner, contact_id, &name, state).await;
        } else if let Some(decoded) = try_base64_typed(inner) {
            handle_typed_message(&decoded, contact_id, &name, state).await;
        } else if let Some(decoded) = try_chunk_reassemble(inner, contact_id, state).await {
            // Reassembled a chunked MC-framed payload
            if TypedEnvelope::is_typed(&decoded) {
                handle_typed_message(&decoded, contact_id, &name, state).await;
            } else {
                let text = String::from_utf8_lossy(&decoded).to_string();
                store_plain_message(state, contact_id, &name, &text).await;
            }
        } else {
            let text = String::from_utf8_lossy(inner).to_string();
            store_plain_message(state, contact_id, &name, &text).await;
        }
        return;
    }

    // Archipelago identity broadcast (`ARCHY:`): upsert the sender's real
    // archipelago identity (DID + ed25519 + x25519) so trust-gating and
    // encrypted DMs work over BOTH meshcore and Meshtastic — the latter
    // otherwise only exposes synthetic node keys. Keyed by the archipelago
    // pubkey (federation_peer_contact_id) so it MERGES with the federation-
    // seeded peer instead of creating a duplicate chat thread. Not stored as
    // a chat message.
    if let Ok(text) = std::str::from_utf8(payload) {
        if let Some((did, ed_hex, x_hex)) = super::super::protocol::parse_identity_broadcast(text) {
            // Ignore our own identity echoed back by the radio/channel.
            if ed_hex.eq_ignore_ascii_case(&state.our_ed_pubkey_hex) {
                return;
            }
            let contact_id = super::super::federation_peer_contact_id(&ed_hex);
            handle_identity_received(
                contact_id,
                0,
                &did,
                &ed_hex,
                &x_hex,
                state,
                our_x25519_secret,
            )
            .await;
            return;
        }
    }

    // Regular channel broadcast (not DM-wrapped). File it under the channel
    // thread (contact_id = u32::MAX - idx) but label it with the real sender
    // when the transport gave us one (Meshtastic), so the channel view shows who
    // said what. Meshcore frames have no sender → generic "Channel N".
    let chan_contact_id = u32::MAX - (channel_idx as u32);
    let chan_name = sender_name.unwrap_or_else(|| format!("Channel {}", channel_idx));
    if TypedEnvelope::is_typed(payload) {
        handle_typed_message(payload, chan_contact_id, &chan_name, state).await;
    } else {
        let text = String::from_utf8_lossy(payload).to_string();
        store_plain_message(state, chan_contact_id, &chan_name, &text).await;
        info!(channel = channel_idx, sender = %chan_name, "Received mesh channel message");
    }
}

/// Return true if the given 6-byte pubkey prefix matches our own meshcore
/// firmware pubkey. We don't currently track our own firmware pubkey in
/// state (the SELF_INFO parse only pulls the node_id), so this falls back
/// to "not any of our known peers" — i.e. if the prefix isn't one of the
/// OTHER contacts in our mesh contact table, it must be us. That holds
/// for the typical 2-node-plus-repeaters topology and is good enough to
/// filter out DMs clearly addressed to someone else.
async fn dest_prefix_is_us(state: &Arc<MeshState>, dest_prefix: &[u8; 6]) -> bool {
    let peers = state.peers.read().await;
    for p in peers.values() {
        if let Some(hex_pk) = p.pubkey_hex.as_deref() {
            if hex_pk.len() >= 12 {
                if let Ok(bytes) = hex::decode(&hex_pk[..12]) {
                    if bytes.len() == 6 && bytes[..] == dest_prefix[..] {
                        // It matches a peer we know — so it's NOT for us
                        // (we'd never have a peer row for ourselves).
                        return false;
                    }
                }
            }
        }
    }
    true
}

/// Look up the contact_id for a `@DM2:` sender by matching the 6-byte
/// archipelago ed25519 prefix against `state.peers`. Federation-seeded
/// peers (added by `mesh::upsert_federation_peer` at startup and after
/// every federation mutation) carry the archipelago key in `pubkey_hex`,
/// so the prefix lookup resolves to the unified federation chat thread —
/// which is exactly where we want both LoRa-arrived and Tor-arrived
/// messages from the same peer to land. If no peer matches (sender isn't
/// in our federation list yet), we fall through to `resolve_counterparty`
/// so the message still lands somewhere visible rather than being dropped.
async fn resolve_sender_by_arch_prefix(
    state: &Arc<MeshState>,
    sender_arch_prefix: &[u8; 6],
    dest_prefix: &[u8; 6],
) -> (u32, String) {
    let prefix_hex = hex::encode(sender_arch_prefix);
    let peers = state.peers.read().await;
    if let Some(p) = peers.values().find(|p| {
        p.pubkey_hex
            .as_deref()
            .map(|k| k.starts_with(&prefix_hex))
            .unwrap_or(false)
    }) {
        return (p.contact_id, p.advert_name.clone());
    }
    drop(peers);
    resolve_counterparty(state, dest_prefix).await
}

/// Pick a "counterparty" contact_id when dispatching a DM-via-channel
/// whose sender we don't otherwise know. We look for any archipelago
/// (type-1, "Archy-*") peer in the contact table whose prefix ISN'T the
/// destination — that's "the other side." Falls back to contact_id=0
/// when nothing matches.
async fn resolve_counterparty(state: &Arc<MeshState>, dest_prefix: &[u8; 6]) -> (u32, String) {
    // Collect every `Archy-*` peer whose meshcore pubkey-prefix differs
    // from dest_prefix (dest is ours, so "not dest" = "not us"), then
    // pick the lowest contact_id. HashMap iteration order is randomized,
    // so sorting is required to avoid flapping between peers across
    // receives (which was producing doubled chat threads in the UI).
    let peers = state.peers.read().await;
    let mut candidates: Vec<(u32, String)> = peers
        .values()
        .filter(|p| p.advert_name.starts_with("Archy-"))
        .filter_map(|p| {
            let hex_pk = p.pubkey_hex.as_deref()?;
            if hex_pk.len() < 12 {
                return None;
            }
            let bytes = hex::decode(&hex_pk[..12]).ok()?;
            if bytes.len() == 6 && bytes[..] != dest_prefix[..] {
                Some((p.contact_id, p.advert_name.clone()))
            } else {
                None
            }
        })
        .collect();
    candidates.sort_by_key(|(id, _)| *id);
    candidates
        .into_iter()
        .next()
        .unwrap_or((0, "dm-via-channel".to_string()))
}
