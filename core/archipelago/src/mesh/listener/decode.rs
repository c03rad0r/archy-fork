//! Message decoding: base64, encryption, chunk reassembly, peer resolution.

use super::super::crypto;
use super::super::message_types::{self, TypedEnvelope};
use super::super::types::*;
use super::MeshState;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Try to base64-decode payload and check if the result is a typed envelope.
/// Handles: plain typed (0x02), steganographic (0xAA), and encrypted (0xEE).
/// Returns the decoded bytes if it's a valid base64-encoded TypedEnvelope.
pub(super) fn try_base64_typed(payload: &[u8]) -> Option<Vec<u8>> {
    use base64::Engine;
    if payload.is_empty() || payload[0] == message_types::TYPED_MESSAGE_MARKER {
        return None;
    }
    let text = std::str::from_utf8(payload).ok()?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .ok()?;
    unwrap_wire_layers(&decoded)
}

/// Try to base64-decode and decrypt an encrypted typed message.
/// Handles the common case where encrypted messages arrive as base64 text.
pub(super) async fn try_decrypt_base64(
    payload: &[u8],
    sender_contact_id: u32,
    state: &Arc<MeshState>,
) -> Option<Vec<u8>> {
    use base64::Engine;
    let text = std::str::from_utf8(payload).ok()?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .ok()?;
    if decoded.first() != Some(&message_types::ENCRYPTED_TYPED_MARKER) {
        return None;
    }
    let secrets = state.shared_secrets.read().await;
    try_decrypt_typed(&decoded, sender_contact_id, &secrets)
}

/// Try to decrypt a Double Ratchet encrypted message (0xDD prefix).
/// Format: [0xDD] [RatchetHeader(40) + nonce(12) + ciphertext + tag(16)]
/// Returns the decrypted typed wire bytes ([0x02][CBOR]) if successful.
async fn try_decrypt_ratchet(
    decoded: &[u8],
    sender_contact_id: u32,
    state: &Arc<MeshState>,
) -> Option<Vec<u8>> {
    if decoded.first() != Some(&message_types::RATCHET_TYPED_MARKER) {
        return None;
    }
    let ratchet_bytes = &decoded[1..]; // skip 0xDD marker

    let ratchet_msg = match super::super::ratchet::RatchetMessage::from_bytes(ratchet_bytes) {
        Ok(msg) => msg,
        Err(e) => {
            warn!(
                contact_id = sender_contact_id,
                "Failed to parse ratchet message: {}", e
            );
            return None;
        }
    };

    // Look up peer DID for session manager
    let peer_did = state
        .peers
        .read()
        .await
        .get(&sender_contact_id)
        .and_then(|p| p.did.clone())?;

    match state
        .session_manager
        .decrypt_from_peer(&peer_did, &ratchet_msg)
        .await
    {
        Ok(plaintext) => {
            debug!(contact_id = sender_contact_id, did = %peer_did, "Decrypted ratchet message (0xDD)");
            // The plaintext should be the original [0x02][CBOR] typed wire
            if TypedEnvelope::is_typed(&plaintext) {
                Some(plaintext)
            } else {
                // Could be nested stego -> typed
                unwrap_wire_layers(&plaintext)
            }
        }
        Err(e) => {
            warn!(
                contact_id = sender_contact_id,
                "Ratchet decrypt failed: {}", e
            );
            None
        }
    }
}

/// Try to base64-decode and decrypt a ratchet-encrypted message.
/// Handles the case where ratchet messages arrive as base64 text.
pub(super) async fn try_decrypt_ratchet_base64(
    payload: &[u8],
    sender_contact_id: u32,
    state: &Arc<MeshState>,
) -> Option<Vec<u8>> {
    use base64::Engine;
    let text = std::str::from_utf8(payload).ok()?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .ok()?;
    if decoded.first() != Some(&message_types::RATCHET_TYPED_MARKER) {
        return None;
    }
    try_decrypt_ratchet(&decoded, sender_contact_id, state).await
}

/// Unwrap wire layers: encrypted (0xEE) -> stego (0xAA) -> typed (0x02).
/// Returns None if decoding fails at any layer (caller should use shared_secrets variant).
fn unwrap_wire_layers(decoded: &[u8]) -> Option<Vec<u8>> {
    // Check for steganographic frame (0xAA prefix) — unwrap to typed envelope
    if decoded.first() == Some(&super::super::steganography::STEGO_MARKER) {
        match super::super::steganography::decode_typed_wire(decoded) {
            Ok(typed_wire) => return Some(typed_wire),
            Err(_) => return None,
        }
    }
    if TypedEnvelope::is_typed(decoded) {
        Some(decoded.to_vec())
    } else {
        None
    }
}

/// Try to decrypt an encrypted typed message (0xEE prefix) using known shared secrets.
/// Format: [0xEE] [nonce: 12] [ciphertext + tag: 16]
fn try_decrypt_typed(
    decoded: &[u8],
    sender_contact_id: u32,
    shared_secrets: &HashMap<u32, [u8; 32]>,
) -> Option<Vec<u8>> {
    if decoded.first() != Some(&message_types::ENCRYPTED_TYPED_MARKER) {
        return None;
    }
    let ciphertext = &decoded[1..]; // skip 0xEE marker

    // Try sender's shared secret first (most likely)
    if let Some(secret) = shared_secrets.get(&sender_contact_id) {
        if let Ok(plaintext) = crypto::decrypt(secret, ciphertext) {
            return unwrap_wire_layers(&plaintext);
        }
    }

    // Fallback: try all known shared secrets (in case contact_id mapping is stale)
    for (cid, secret) in shared_secrets {
        if *cid == sender_contact_id {
            continue;
        } // already tried
        if let Ok(plaintext) = crypto::decrypt(secret, ciphertext) {
            return unwrap_wire_layers(&plaintext);
        }
    }

    None
}

/// Cheap structural check: does this payload look like an `MCxxyyzz…`
/// chunk frame? Used by the receive dispatcher to decide whether a `None`
/// from `try_chunk_reassemble` means "not a chunk" (fall through to the
/// plaintext store) or "chunk buffered, waiting for more frames" (do not
/// store anything yet — partial chunks must never be persisted as their
/// own messages, which was the cause of the raw `MC0301…` bubbles users
/// were seeing in chat).
pub(super) fn is_mc_chunk_frame(payload: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(payload) else {
        return false;
    };
    if !text.starts_with("MC") || text.len() < 8 {
        return false;
    }
    u8::from_str_radix(&text[2..4], 16).is_ok()
        && u8::from_str_radix(&text[4..6], 16).is_ok()
        && u8::from_str_radix(&text[6..8], 16).is_ok()
}

/// Check if payload is a mesh chunk ("MC" prefix) and try to reassemble.
/// Format: MC{msg_id:2hex}{chunk_idx:2hex}{total:2hex}{base64_data}
/// Returns Some(decoded_bytes) when all chunks have arrived.
pub(super) async fn try_chunk_reassemble(
    payload: &[u8],
    sender_contact_id: u32,
    state: &Arc<MeshState>,
) -> Option<Vec<u8>> {
    use base64::Engine;
    let text = std::str::from_utf8(payload).ok()?;
    if !text.starts_with("MC") || text.len() < 8 {
        return None;
    }

    let msg_id = u8::from_str_radix(&text[2..4], 16).ok()?;
    let chunk_idx = u8::from_str_radix(&text[4..6], 16).ok()?;
    let total = u8::from_str_radix(&text[6..8], 16).ok()?;
    let chunk_data = &text[8..];

    if total == 0 || total > 20 {
        return None; // sanity check
    }

    let key = (sender_contact_id, msg_id);
    let mut buffer = state.chunk_buffer.write().await;

    // Clean up stale entries (>120s old)
    buffer.retain(|_, v| v.created.elapsed().as_secs() < 120);

    let assembly = buffer.entry(key).or_insert_with(|| super::ChunkAssembly {
        chunks: HashMap::new(),
        total,
        created: std::time::Instant::now(),
    });

    assembly.chunks.insert(chunk_idx, chunk_data.to_string());
    assembly.total = total; // update in case first chunk had it wrong

    debug!(
        msg_id,
        chunk_idx,
        total,
        received = assembly.chunks.len(),
        "Chunk received"
    );

    // Check if we have all chunks
    if assembly.chunks.len() < total as usize {
        return None;
    }

    // All chunks received — reassemble in order
    let mut combined = String::new();
    for i in 0..total {
        match assembly.chunks.get(&i) {
            Some(data) => combined.push_str(data),
            None => {
                warn!(msg_id, missing = i, "Chunk missing during reassembly");
                return None;
            }
        }
    }

    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&combined) {
        // Check for ratchet-encrypted frame (0xDD) — decrypt then unwrap
        if decoded.first() == Some(&message_types::RATCHET_TYPED_MARKER) {
            // Must drop buffer lock before calling async try_decrypt_ratchet
            let decoded_clone = decoded.clone();
            drop(buffer);
            if let Some(typed_wire) =
                try_decrypt_ratchet(&decoded_clone, sender_contact_id, state).await
            {
                info!(
                    msg_id,
                    chunks = total,
                    total_len = typed_wire.len(),
                    "Reassembled ratchet-encrypted chunked message"
                );
                state.chunk_buffer.write().await.remove(&key);
                return Some(typed_wire);
            }
            buffer = state.chunk_buffer.write().await;
        }
        // Check for static-encrypted frame (0xEE) — decrypt then unwrap
        if decoded.first() == Some(&message_types::ENCRYPTED_TYPED_MARKER) {
            let secrets = state.shared_secrets.read().await;
            if let Some(typed_wire) = try_decrypt_typed(&decoded, sender_contact_id, &secrets) {
                info!(
                    msg_id,
                    chunks = total,
                    total_len = typed_wire.len(),
                    "Reassembled encrypted chunked message"
                );
                buffer.remove(&key);
                return Some(typed_wire);
            }
        }
        // Check for stego frame — unwrap to typed envelope
        if decoded.first() == Some(&super::super::steganography::STEGO_MARKER) {
            if let Ok(typed_wire) = super::super::steganography::decode_typed_wire(&decoded) {
                info!(
                    msg_id,
                    chunks = total,
                    total_len = typed_wire.len(),
                    "Reassembled stego chunked message"
                );
                buffer.remove(&key);
                return Some(typed_wire);
            }
        }
        if TypedEnvelope::is_typed(&decoded) {
            info!(
                msg_id,
                chunks = total,
                total_len = decoded.len(),
                "Reassembled chunked message"
            );
            buffer.remove(&key);
            return Some(decoded);
        }
    }

    warn!(msg_id, "All chunks received but decode failed");
    buffer.remove(&key);
    None
}

/// Look up a peer by pubkey hex prefix. Returns (contact_id, display_name).
pub(super) async fn resolve_peer(state: &Arc<MeshState>, sender_prefix: &str) -> (u32, String) {
    {
        let peers = state.peers.read().await;
        if let Some(peer) = peers.values().find(|p| {
            p.pubkey_hex
                .as_ref()
                .map(|k| k.starts_with(sender_prefix))
                .unwrap_or(false)
        }) {
            return (peer.contact_id, peer.advert_name.clone());
        }
    }

    if let Some((node_num, pubkey_hex, name)) = meshtastic_peer_from_prefix(sender_prefix) {
        let peer = MeshPeer {
            contact_id: node_num,
            advert_name: name.clone(),
            did: None,
            pubkey_hex: Some(pubkey_hex),
            arch_pubkey_hex: None,
            x25519_pubkey: None,
            rssi: None,
            snr: None,
            last_heard: chrono::Utc::now().to_rfc3339(),
            hops: 0xff,
            last_advert: 0,
            reachable: true,
            // Stamped fresh from `peer_pubkeys` in `get_contacts` once a real
            // contact refresh runs; unknown at synthesis time here.
            pkc_capable: false,
            lat: None,
            lon: None,
            // Set only by an explicit LightningInfo advert, never inferred.
            lightning_uri: None,
        };
        let is_new = {
            let mut peers = state.peers.write().await;
            peers.insert(node_num, peer.clone()).is_none()
        };
        state.update_peer_count().await;
        let _ = state.event_tx.send(if is_new {
            MeshEvent::PeerDiscovered(peer)
        } else {
            MeshEvent::PeerUpdated(peer)
        });
        return (node_num, name);
    }

    (0, sender_prefix.to_string())
}

fn meshtastic_peer_from_prefix(sender_prefix: &str) -> Option<(u32, String, String)> {
    if sender_prefix.len() < 12 {
        return None;
    }
    let bytes = hex::decode(&sender_prefix[..12]).ok()?;
    if bytes.len() != 6 || bytes[4] != b'm' || bytes[5] != b'e' {
        return None;
    }
    let node_num = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if node_num == 0 || node_num == u32::MAX {
        return None;
    }
    let mut full_key = [0u8; 32];
    full_key[..4].copy_from_slice(&node_num.to_le_bytes());
    full_key[4..15].copy_from_slice(b"meshtastic:");
    let name = format!("Meshtastic !{:08x}", node_num);
    Some((node_num, hex::encode(full_key), name))
}

/// Stamp the SNR carried in a Meshcore v3 contact-message frame onto the
/// sender's peer record so the signal-bars indicator has real data (Meshcore
/// has no per-packet RSSI like Meshtastic, only this 1-byte SNR — see
/// `protocol::parse_contact_msg_v3_raw`).
pub(super) async fn update_peer_snr(state: &Arc<MeshState>, contact_id: u32, snr: f32) {
    let mut peers = state.peers.write().await;
    if let Some(peer) = peers.get_mut(&contact_id) {
        peer.snr = Some(snr);
    }
}

/// Store a plain-text (non-typed) message and emit an event.
pub(super) async fn store_plain_message(
    state: &Arc<MeshState>,
    contact_id: u32,
    peer_name: &str,
    text: &str,
) {
    store_plain_message_with_encryption(state, contact_id, peer_name, text, false).await;
}

pub(super) async fn store_plain_message_with_encryption(
    state: &Arc<MeshState>,
    contact_id: u32,
    peer_name: &str,
    text: &str,
    encrypted: bool,
) {
    let msg_id = state.next_id().await;
    let radio_transport = radio_transport_label(state.status.read().await.device_type);
    let msg = MeshMessage {
        id: msg_id,
        direction: MessageDirection::Received,
        peer_contact_id: contact_id,
        peer_name: Some(peer_name.to_string()),
        plaintext: text.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        delivered: true,
        encrypted,
        transport: Some(radio_transport.to_string()),
        message_type: "text".to_string(),
        typed_payload: None,
        sender_pubkey: None,
        sender_seq: None,
    };
    state.store_message(msg.clone()).await;
    state.status.write().await.messages_received += 1;
    let _ = state.event_tx.send(MeshEvent::MessageReceived(msg));

    // Where a plain-text answer goes: a private NATIVE DM to the asker whenever
    // we know its radio pubkey (so it does NOT land on the public channel and a
    // stock meshcore client can read it); we only fall back to a channel reply
    // if the sender has no resolvable pubkey (rare).
    let plain_reply = || async {
        let peers = state.peers.read().await;
        peers
            .get(&contact_id)
            .and_then(|p| p.pubkey_hex.clone())
            .filter(|h| h.len() >= 12)
            .and_then(|h| hex::decode(&h[..12]).ok())
            .filter(|b| b.len() == 6)
            .map(|b| {
                let mut pre = [0u8; 6];
                pre.copy_from_slice(&b);
                super::assist::AssistReply::RadioDm { dest_prefix: pre }
            })
            .unwrap_or(super::assist::AssistReply::ChannelText { channel: 0 })
    };

    // `!archy [sub]` — node status straight from this node's caches. No model is
    // involved, so it stays available with the AI assistant switched off; the
    // trust gate in run_node_cmd still applies.
    if let Some(rest) = super::node_cmd::strip_archy_trigger(text) {
        let reply = plain_reply().await;
        let req_id = state.next_id().await;
        let rest = rest.to_string();
        let name = peer_name.to_string();
        let st = Arc::clone(state);
        tokio::spawn(async move {
            // Bare plain-text carries no signature — not authenticated.
            super::node_cmd::run_node_cmd(rest, req_id, contact_id, name, false, reply, st).await;
        });
    }
    // Mesh-AI assistant (issue #50): a plain `!ai`/`!ask <question>` is answered
    // by this node's local model when the assistant is on. The trust/rate gate
    // lives in run_assist.
    else if state.assistant.read().await.enabled {
        if let Some(prompt) = strip_ai_trigger(text) {
            if !prompt.is_empty() {
                let reply = plain_reply().await;
                let req_id = state.next_id().await;
                let prompt = prompt.to_string();
                let name = peer_name.to_string();
                let st = Arc::clone(state);
                tokio::spawn(async move {
                    // A bare plain-text channel `!ai` carries no signature, so it
                    // is NOT authenticated — under trusted_only it'll be denied,
                    // and it can only be answered under the "anyone" policy.
                    super::assist::run_assist(
                        prompt, None, req_id, contact_id, name, false, reply, st,
                    )
                    .await;
                });
            }
        }
    }
}

/// Recognise a `!ai`/`!ask ` command prefix (case-insensitive) and return the
/// trimmed question after it, or `None` if the text isn't an AI command.
pub(super) fn strip_ai_trigger(text: &str) -> Option<&str> {
    let t = text.trim_start();
    for p in ["!ai ", "!ask "] {
        if t.len() >= p.len() && t[..p.len()].eq_ignore_ascii_case(p) {
            return Some(t[p.len()..].trim());
        }
    }
    None
}

/// Handle a received identity broadcast from a peer.
#[allow(dead_code)]
pub(super) async fn handle_identity_received(
    contact_id: u32,
    // None = signal strength unknown at this layer (identity adverts arrive
    // through the transport-agnostic channel path, which carries no phy
    // stats). The periodic refresh_contacts pass fills in the real value for
    // transports that report one; hardcoding 0 here made every discovery
    // read as "0 dBm" — indistinguishable from a real (if implausible)
    // reading and from "no radio at all" (2026-08-16).
    rssi: Option<i16>,
    did: &str,
    ed_pubkey_hex: &str,
    x25519_pubkey_hex: &str,
    state: &Arc<MeshState>,
    our_x25519_secret: &[u8; 32],
) {
    info!(
        contact_id,
        did = %did,
        rssi = ?rssi,
        "Archipelago peer discovered over mesh"
    );

    // Verify Ed25519 public key is valid
    let ed_pubkey_bytes = match hex::decode(ed_pubkey_hex) {
        Ok(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            arr
        }
        _ => {
            warn!(contact_id, "Rejecting identity: invalid Ed25519 public key");
            return;
        }
    };
    if ed25519_dalek::VerifyingKey::from_bytes(&ed_pubkey_bytes).is_err() {
        warn!(
            contact_id,
            "Rejecting identity: Ed25519 key is not a valid curve point"
        );
        return;
    }

    // Verify X25519 public key is consistent with Ed25519 key
    let expected_x25519 = match crypto::ed25519_pubkey_to_x25519(&ed_pubkey_bytes) {
        Ok(k) => k,
        Err(e) => {
            warn!(
                contact_id,
                "Rejecting identity: cannot derive X25519 from Ed25519: {}", e
            );
            return;
        }
    };

    // Decode X25519 public key
    let x25519_bytes = match hex::decode(x25519_pubkey_hex) {
        Ok(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            arr
        }
        _ => {
            warn!(contact_id, "Rejecting identity: invalid X25519 public key");
            return;
        }
    };

    if x25519_bytes != expected_x25519 {
        warn!(contact_id, did = %did, "Rejecting identity: X25519 key does not match Ed25519 key");
        return;
    }

    // Derive shared secret for encrypted messaging
    let shared_secret = crypto::x25519_shared_secret(our_x25519_secret, &x25519_bytes);
    state
        .shared_secrets
        .write()
        .await
        .insert(contact_id, shared_secret);

    // Update peer record
    let mut peer = MeshPeer {
        contact_id,
        // .get(): a malformed DID shorter than the "did:key:" prefix must
        // not panic the listener on a radio-supplied string.
        advert_name: format!("Archy-{}", did.get(8..16.min(did.len())).unwrap_or(did)),
        did: Some(did.to_string()),
        pubkey_hex: Some(ed_pubkey_hex.to_string()),
        // The advert signature was verified above, so this is an authenticated
        // archipelago identity. Bind it separately so a later refresh_contacts
        // (which rewrites pubkey_hex to the firmware routing key) can't drop it.
        arch_pubkey_hex: Some(ed_pubkey_hex.to_string()),
        x25519_pubkey: Some(x25519_bytes),
        rssi,
        snr: None,
        last_heard: chrono::Utc::now().to_rfc3339(),
        hops: 0,
        last_advert: 0,
        // We just heard this peer's identity advert, so it's reachable.
        reachable: true,
        // PKC capability is tracked by the radio driver's get_contacts(), not
        // known at identity-advert time.
        pkc_capable: false,
        lat: None,
        lon: None,
        // Set only by an explicit LightningInfo advert, never inferred.
        lightning_uri: None,
    };

    let is_new = {
        let mut peers = state.peers.write().await;
        let is_new = !peers.contains_key(&contact_id);
        if let Some(existing) = peers.get(&contact_id) {
            // This id is shared with the federation-seeded row for the same
            // node (that's the point — identity adverts MERGE, not duplicate).
            // The wholesale insert below must not stomp the federation row's
            // real node name with our synthetic "Archy-…" placeholder — with
            // Reticulum re-emitting identity adverts every announce tick,
            // that renamed every federated contact once a minute. Same for a
            // known position: keep it rather than nulling it out.
            if !existing.advert_name.trim().is_empty()
                && !existing.advert_name.starts_with("Archy-")
            {
                peer.advert_name = existing.advert_name.clone();
            }
            if peer.lat.is_none() {
                peer.lat = existing.lat;
                peer.lon = existing.lon;
            }
            // Same hazard as the name and position above: an identity advert
            // carries no Lightning datum, and Reticulum re-emits one every
            // announce tick, so a wholesale insert would drop a peer out of the
            // channel-open picker about once a minute. The URI comes only from
            // an explicit LightningInfo advert — preserve it.
            if peer.lightning_uri.is_none() {
                peer.lightning_uri = existing.lightning_uri.clone();
            }
        }
        peers.insert(contact_id, peer.clone());
        is_new
    };
    state.update_peer_count().await;

    let event = if is_new {
        MeshEvent::PeerDiscovered(peer)
    } else {
        MeshEvent::PeerUpdated(peer)
    };
    let _ = state.event_tx.send(event);
    let _ = state.event_tx.send(MeshEvent::IdentityReceived {
        contact_id,
        did: did.to_string(),
        pubkey_hex: ed_pubkey_hex.to_string(),
        x25519_pubkey: x25519_bytes,
    });
}

/// Handle a received message (direct or channel).
#[allow(dead_code)]
pub(super) async fn handle_received_message(
    contact_id: u32,
    payload: &[u8],
    rssi: i16,
    is_channel: bool,
    state: &Arc<MeshState>,
    _our_x25519_secret: &[u8; 32],
) {
    // Try to decrypt if we have a shared secret for this contact
    let shared_secrets = state.shared_secrets.read().await;
    let (plaintext, encrypted) = if let Some(secret) = shared_secrets.get(&contact_id) {
        match crypto::decrypt(secret, payload) {
            Ok(pt) => (String::from_utf8_lossy(&pt).to_string(), true),
            Err(_) => {
                // Not encrypted or wrong key — treat as plaintext
                (String::from_utf8_lossy(payload).to_string(), false)
            }
        }
    } else {
        (String::from_utf8_lossy(payload).to_string(), false)
    };
    drop(shared_secrets);

    // Update peer last_heard
    {
        let mut peers = state.peers.write().await;
        if let Some(peer) = peers.get_mut(&contact_id) {
            peer.last_heard = chrono::Utc::now().to_rfc3339();
            peer.rssi = Some(rssi);
        }
    }

    let peer_name = state
        .peers
        .read()
        .await
        .get(&contact_id)
        .map(|p| p.advert_name.clone());

    let msg_id = state.next_id().await;
    let radio_transport = radio_transport_label(state.status.read().await.device_type);
    let msg = MeshMessage {
        id: msg_id,
        direction: MessageDirection::Received,
        peer_contact_id: contact_id,
        peer_name,
        plaintext: plaintext.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        delivered: true,
        encrypted,
        transport: Some(radio_transport.to_string()),
        message_type: "text".to_string(),
        typed_payload: None,
        sender_pubkey: None,
        sender_seq: None,
    };

    state.store_message(msg.clone()).await;
    {
        let mut status = state.status.write().await;
        status.messages_received += 1;
    }

    info!(
        contact_id,
        encrypted,
        channel = is_channel,
        "Received mesh message"
    );

    let _ = state.event_tx.send(MeshEvent::MessageReceived(msg));
}
