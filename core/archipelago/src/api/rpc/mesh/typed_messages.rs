use super::super::RpcHandler;
use crate::blobs::DEFAULT_CAP_TTL_SECS;
use crate::mesh::message_types::{
    self, AlertPayload, AlertType, ChannelInvitePayload, ContentInlinePayload, ContentRefPayload,
    Coordinate, DeletePayload, EditPayload, ForwardPayload, InvoicePayload, MeshMessageType,
    MessageKey, PsbtHashPayload, ReactionPayload, ReadReceiptPayload, ReplyPayload, TypedEnvelope,
};
use crate::mesh::types::radio_transport_label;
use anyhow::Result;
use tracing::info;

impl RpcHandler {
    /// mesh.send-invoice — Create a Lightning invoice and send bolt11 to mesh peer.
    pub(in crate::api::rpc) async fn handle_mesh_send_invoice(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;
        let amount_sats = params["amount_sats"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing amount_sats"))?;
        let memo = params["memo"].as_str().map(|s| s.to_string());

        // Build invoice payload
        let invoice = InvoicePayload {
            bolt11: format!("lnbc{}n1pjmesh...", amount_sats), // Placeholder — real LND call in Phase 4
            amount_sats,
            memo: memo.clone(),
            payment_hash: None,
        };

        // Send via mesh
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        let seq = svc.next_send_seq(contact_id).await;
        let payload = message_types::encode_payload(&invoice)?;
        let envelope = TypedEnvelope::new(MeshMessageType::Invoice, payload).with_seq(seq);
        let wire = envelope.to_wire()?;

        let display = format!(
            "Invoice: {} sats{}",
            amount_sats,
            memo.as_ref()
                .map(|m| format!(" — {}", m))
                .unwrap_or_default()
        );
        let typed_json = serde_json::to_value(&invoice).ok();
        let msg = svc
            .send_typed_wire(contact_id, wire, "invoice", &display, typed_json, seq)
            .await?;

        info!(contact_id, amount_sats, "Sent invoice over mesh");
        Ok(serde_json::json!({
            "sent": true,
            "message_id": msg.id,
            "amount_sats": amount_sats,
            "bolt11": invoice.bolt11,
        }))
    }

    /// mesh.send-coordinate — Send GPS coordinates to a mesh peer.
    pub(in crate::api::rpc) async fn handle_mesh_send_coordinate(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;
        let lat = params["lat"]
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("Missing lat"))?;
        let lng = params["lng"]
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("Missing lng"))?;
        let label = params["label"].as_str().map(|s| s.to_string());

        let coord = Coordinate::from_degrees(lat, lng, label);

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        let seq = svc.next_send_seq(contact_id).await;
        let payload = message_types::encode_payload(&coord)?;
        let envelope = TypedEnvelope::new(MeshMessageType::Coordinate, payload).with_seq(seq);
        let wire = envelope.to_wire()?;

        let display = format!(
            "Location: {:.6}, {:.6}{}",
            coord.lat_degrees(),
            coord.lng_degrees(),
            coord
                .label
                .as_ref()
                .map(|l| format!(" ({})", l))
                .unwrap_or_default()
        );
        let typed_json = serde_json::to_value(&coord).ok();
        let msg = svc
            .send_typed_wire(contact_id, wire, "coordinate", &display, typed_json, seq)
            .await?;

        info!(contact_id, "Sent coordinate over mesh");
        Ok(serde_json::json!({
            "sent": true,
            "message_id": msg.id,
            "lat": coord.lat,
            "lng": coord.lng,
        }))
    }

    /// mesh.send-alert — Send a signed emergency alert over mesh.
    pub(in crate::api::rpc) async fn handle_mesh_send_alert(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let message = params["message"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing message"))?;
        let alert_type_str = params["alert_type"].as_str().unwrap_or("status");
        let broadcast = params["broadcast"].as_bool().unwrap_or(false);

        let alert_type = match alert_type_str {
            "emergency" => AlertType::Emergency,
            "dead_man" => AlertType::DeadMan,
            _ => AlertType::Status,
        };

        // Optional GPS
        let coordinate =
            if let (Some(lat), Some(lng)) = (params["lat"].as_f64(), params["lng"].as_f64()) {
                Some(Coordinate::from_degrees(lat, lng, None))
            } else {
                None
            };

        let alert = AlertPayload {
            alert_type,
            message: message.to_string(),
            coordinate,
        };

        let payload = message_types::encode_payload(&alert)?;

        // Sign the alert with node identity
        let (_data, _) = self.state_manager.get_snapshot().await;
        let identity_dir = self.config.data_dir.join("identity");
        let node_key_path = identity_dir.join("node_key");

        let unsigned_envelope = if node_key_path.exists() {
            let key_bytes = tokio::fs::read(&node_key_path).await?;
            if key_bytes.len() == 32 {
                let mut seed = [0u8; 32];
                seed.copy_from_slice(&key_bytes);
                let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
                TypedEnvelope::new_signed(MeshMessageType::Alert, payload, &signing_key)
            } else {
                TypedEnvelope::new(MeshMessageType::Alert, payload)
            }
        } else {
            TypedEnvelope::new(MeshMessageType::Alert, payload)
        };

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        let display = alert.message.clone();
        let typed_json = serde_json::to_value(&alert).ok();
        let signed = unsigned_envelope.sig.is_some();
        if broadcast {
            // Channel 0 uses target=0 for seq allocation. Signature covers
            // (t, v, ts) — setting seq afterwards does NOT invalidate it.
            let seq = svc.next_send_seq(0).await;
            let envelope = unsigned_envelope.with_seq(seq);
            let wire = envelope.to_wire()?;
            svc.send_channel_typed_wire(0, wire, "alert", &display, typed_json.clone(), seq)
                .await?;
            info!(alert_type = alert_type_str, "Broadcast alert over mesh");
        } else if let Some(contact_id) = params["contact_id"].as_u64() {
            let contact_id = contact_id as u32;
            let seq = svc.next_send_seq(contact_id).await;
            let envelope = unsigned_envelope.with_seq(seq);
            let wire = envelope.to_wire()?;
            svc.send_typed_wire(contact_id, wire, "alert", &display, typed_json, seq)
                .await?;
            info!(
                contact_id,
                alert_type = alert_type_str,
                "Sent alert to peer"
            );
        } else {
            anyhow::bail!("Must specify contact_id or broadcast: true");
        }

        Ok(serde_json::json!({
            "sent": true,
            "alert_type": alert_type_str,
            "signed": signed,
        }))
    }

    /// mesh.send-content — Send a ContentRef for a locally-stored blob to a mesh peer.
    /// Params: { contact_id, cid, caption? }. Looks up the peer's pubkey from
    /// MeshState, loads BlobMeta, issues a peer-scoped capability, and sends a
    /// typed envelope over the mesh. The recipient fetches the blob out-of-band
    /// via `GET {sender_onion}/blob/{cid}?cap=..&exp=..&peer=..`.
    pub(in crate::api::rpc) async fn handle_mesh_send_content(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;
        let cid = params["cid"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing cid"))?
            .to_string();
        let caption = params["caption"].as_str().map(|s| s.to_string());
        // Optional override: frontend can resolve the peer's federation onion
        // itself (by matching mesh peer name against federation.list-nodes)
        // and pass it in directly. Bypasses the mesh-peer→DID→federation
        // lookup, which currently fails because meshcore adverts don't carry
        // the archipelago master DID.
        let explicit_peer_onion = params["peer_onion"].as_str().map(|s| s.to_string());

        // Pull the shared blob store (set by ApiHandler at startup).
        let blob_store = {
            let guard = self.blob_store.read().await;
            guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Blob store not initialised"))?
                .clone()
        };
        let meta = blob_store.meta(&cid).await?;

        // Need the live mesh service for peer lookup + send.
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        // Resolve peer Ed25519 pubkey + DID from contact_id. The pubkey scopes
        // the capability token; the DID is the stable cross-transport identity
        // used to match the peer against our federation node list (mesh and
        // federation use different ed25519 keys but the same master DID).
        let state = svc.shared_state();
        let (peer_pubkey_hex, peer_did) = {
            let peers = state.peers.read().await;
            let peer = peers
                .get(&contact_id)
                .ok_or_else(|| anyhow::anyhow!("Unknown mesh contact_id {}", contact_id))?;
            let pk = peer
                .pubkey_hex
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Peer has no pubkey yet"))?;
            (pk, peer.did.clone())
        };

        // Our onion (stripped of scheme/trailing slash) for the URL the receiver will hit.
        let (data, _) = self.state_manager.get_snapshot().await;
        let sender_onion = data
            .server_info
            .tor_address
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Tor onion not yet available"))?
            .trim_end_matches('/')
            .to_string();

        let exp = (chrono::Utc::now().timestamp() as u64) + DEFAULT_CAP_TTL_SECS;
        let cap_token = blob_store.issue_capability(&cid, &peer_pubkey_hex, exp);

        let content = ContentRefPayload {
            cid: cid.clone(),
            size: meta.size,
            mime: meta.mime.clone(),
            filename: meta.filename.clone(),
            thumb_bytes: meta.thumb_bytes.clone(),
            caption: caption.clone(),
            sender_onion,
            cap_token,
            cap_exp: exp,
        };

        let seq = svc.next_send_seq(contact_id).await;
        let payload = message_types::encode_payload(&content)?;
        let envelope = TypedEnvelope::new(MeshMessageType::ContentRef, payload).with_seq(seq);
        let wire = envelope.to_wire()?;

        let display = match (&content.filename, &content.caption) {
            (Some(fname), Some(c)) => format!("{} — {}", fname, c),
            (Some(fname), None) => fname.clone(),
            (None, Some(c)) => c.clone(),
            (None, None) => format!("{} ({} bytes)", content.mime, content.size),
        };
        let typed_json = serde_json::to_value(&content).ok();

        // ContentRef envelopes routinely exceed LoRa's ~160-byte per-frame
        // budget (cid alone is 64 hex chars, plus onion + cap). Route via
        // federation when the peer has a known onion; fall back to LoRa
        // only for tiny envelopes that could theoretically fit.
        //
        // Federation peers are pre-seeded into mesh state with their
        // archipelago pubkey as `pubkey_hex` and DID populated — so a direct
        // lookup on either key finds the onion. The `explicit_peer_onion`
        // frontend override and the DID path remain as fallbacks for the
        // transitional case where a mesh-discovered LoRa contact also
        // happens to be federated.
        let federation_onion = if let Some(onion) = explicit_peer_onion {
            Some(onion)
        } else {
            let nodes = crate::federation::load_nodes(&self.config.data_dir)
                .await
                .unwrap_or_default();
            nodes
                .iter()
                .find(|n| n.pubkey == peer_pubkey_hex)
                .map(|n| n.onion.clone())
                .or_else(|| {
                    peer_did.as_ref().and_then(|did| {
                        nodes
                            .iter()
                            .find(|n| &n.did == did)
                            .map(|n| n.onion.clone())
                    })
                })
        };
        let msg = if let Some(onion) = federation_onion {
            svc.send_typed_wire_via_federation(
                contact_id,
                &onion,
                wire,
                "content_ref",
                &display,
                typed_json,
                seq,
            )
            .await?
        } else {
            svc.send_typed_wire(contact_id, wire, "content_ref", &display, typed_json, seq)
                .await?
        };

        info!(contact_id, cid = %cid, size = meta.size, "Sent content_ref over mesh");
        Ok(serde_json::json!({
            "sent": true,
            "message_id": msg.id,
            "cid": cid,
            "size": meta.size,
        }))
    }

    /// mesh.send-content-inline — Carry file bytes directly in a typed envelope.
    /// Params: { contact_id, mime, filename?, caption?, bytes_b64 }. The
    /// underlying `send_typed_wire` auto-chunks via MCIIXXTT framing when the
    /// envelope exceeds the LoRa per-frame budget. Sender also writes the
    /// blob to its own BlobStore so the chat history renders identically to
    /// ContentRef on both sides.
    pub(in crate::api::rpc) async fn handle_mesh_send_content_inline(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;
        let mime = params["mime"]
            .as_str()
            .unwrap_or("application/octet-stream")
            .to_string();
        let filename = params["filename"].as_str().map(|s| s.to_string());
        let caption = params["caption"].as_str().map(|s| s.to_string());
        let bytes_b64 = params["bytes_b64"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing bytes_b64"))?;
        let bytes = B64
            .decode(bytes_b64)
            .map_err(|e| anyhow::anyhow!("Invalid base64: {}", e))?;

        // Hard ceiling matching the chunked-send capacity (~20 chunks * 152
        // b64 chars after MCIIXXTT framing). Anything larger must go via
        // ContentRef over Tor — UNLESS the active device is Reticulum, which
        // can carry up to RETICULUM_RESOURCE_MAX directly over LoRa via a
        // native RNS Resource transfer (keep this ceiling in sync with
        // `mesh.transport-advice`'s `"resource-mesh"` tier, the source of
        // truth the frontend consults before ever reaching this size).
        const INLINE_HARD_MAX: usize = 2300;
        const RETICULUM_RESOURCE_MAX: usize = 2 * 1024 * 1024;

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let device_type = svc.shared_state().status.read().await.device_type;
        let use_resource_transfer = bytes.len() > INLINE_HARD_MAX
            && device_type == crate::mesh::types::DeviceType::Reticulum
            && bytes.len() <= RETICULUM_RESOURCE_MAX;

        if bytes.len() > INLINE_HARD_MAX && !use_resource_transfer {
            anyhow::bail!(
                "Payload {} bytes exceeds inline max {} — use mesh.send-content (ContentRef) instead",
                bytes.len(),
                INLINE_HARD_MAX
            );
        }

        // Mirror to local BlobStore so the Sent record renders the same
        // attachment card as the receiver's.
        let blob_store = {
            let guard = self.blob_store.read().await;
            guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Blob store not initialised"))?
                .clone()
        };
        let meta = blob_store
            .put(&bytes, &mime, filename.clone(), None, false)
            .await?;

        let display = match (&filename, &caption) {
            (Some(f), Some(c)) => format!("📎 {} — {}", f, c),
            (Some(f), None) => format!("📎 {}", f),
            (None, Some(c)) => format!("📎 {}", c),
            (None, None) => format!("📎 {} ({} bytes)", mime, meta.size),
        };
        // Render as a content_ref card on the sender side (UI already knows
        // how to draw it from cid + mime + filename + size) regardless of
        // which wire format actually goes out — this is a local-only mirror.
        let typed_json = serde_json::json!({
            "cid": meta.cid,
            "size": meta.size,
            "mime": mime,
            "filename": filename,
            "caption": caption,
            "inline": true,
        });
        let seq = svc.next_send_seq(contact_id).await;

        // A stock (non-archy) peer can't decode our typed-envelope wire
        // format — send images to them via LXMF's native FIELD_IMAGE
        // instead, so they actually see the photo (Sideband/NomadNet).
        let is_archy = svc.is_archy_peer(contact_id).await;
        let native_image = !is_archy
            && device_type == crate::mesh::types::DeviceType::Reticulum
            && mime.starts_with("image/");

        let msg = if native_image {
            svc.send_native_image(contact_id, &mime, bytes, caption.clone())
                .await?;
            svc.record_sent_typed(
                contact_id,
                "content_ref",
                &display,
                Some(typed_json),
                seq,
                Some(radio_transport_label(device_type).to_string()),
                true, // Reticulum/LXMF is unconditionally E2E on every send
            )
            .await
        } else {
            let content = ContentInlinePayload {
                mime: mime.clone(),
                filename: filename.clone(),
                caption: caption.clone(),
                bytes,
            };
            let payload = message_types::encode_payload(&content)?;
            let envelope =
                TypedEnvelope::new(MeshMessageType::ContentInline, payload).with_seq(seq);
            let wire = envelope.to_wire()?;
            if use_resource_transfer {
                svc.send_content_resource(
                    contact_id,
                    wire,
                    "content_ref",
                    &display,
                    Some(typed_json),
                    seq,
                )
                .await?
            } else {
                svc.send_typed_wire(
                    contact_id,
                    wire,
                    "content_ref",
                    &display,
                    Some(typed_json),
                    seq,
                )
                .await?
            }
        };

        info!(
            contact_id,
            size = meta.size,
            cid = %meta.cid,
            via_resource = use_resource_transfer,
            "Sent content_inline over mesh"
        );
        Ok(serde_json::json!({
            "sent": true,
            "message_id": msg.id,
            "cid": meta.cid,
            "size": meta.size,
        }))
    }

    /// mesh.transport-advice — Recommend how to send an attachment of a given
    /// size to a given peer. Single source of truth for the frontend tier
    /// router. Params: { contact_id, size }. Returns:
    ///   { tier, est_seconds, has_tor, reason }
    /// where tier ∈ "auto-mesh" | "choose" | "tor-only" | "impossible".
    pub(in crate::api::rpc) async fn handle_mesh_transport_advice(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;
        let size = params["size"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing size"))?;

        // Knobs — keep in sync with the frontend modal copy.
        const MESH_AUTO_MAX: u64 = 1024;
        const MESH_HARD_MAX: u64 = 2300;
        // Reticulum-only: above the small inline-chunk cap, a real RNS Resource
        // transfer can still carry the payload directly over LoRa (native
        // chunked transfer with retries) instead of falling back to Tor. Capped
        // well under TOR_LARGE_WARN to keep worst-case LoRa transfer time
        // bounded — comfortably covers the HIGH image preset (512KB target).
        const RETICULUM_RESOURCE_MAX: u64 = 2 * 1024 * 1024;
        const TOR_LARGE_WARN: u64 = 5 * 1024 * 1024;
        // Meshcore/Meshtastic effective LoRa throughput after retries/FEC is much
        // lower than the raw radio bitrate. Reticulum's RNodeInterface reports its
        // real bitrate (e.g. ~3125 bps ≈ 390 B/s observed live), so estimates for it
        // would be wildly pessimistic at the generic 50 B/s figure.
        const LORA_BYTES_PER_SEC_DEFAULT: u64 = 50;
        const LORA_BYTES_PER_SEC_RETICULUM: u64 = 390;

        // Resolve peer Tor reachability via federation node list.
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let state = svc.shared_state();
        let device_type = state.status.read().await.device_type;
        let lora_bytes_per_sec = if device_type == crate::mesh::types::DeviceType::Reticulum {
            LORA_BYTES_PER_SEC_RETICULUM
        } else {
            LORA_BYTES_PER_SEC_DEFAULT
        };
        let (peer_pubkey_hex, peer_did) = {
            let peers = state.peers.read().await;
            match peers.get(&contact_id) {
                Some(p) => (p.pubkey_hex.clone(), p.did.clone()),
                None => (None, None),
            }
        };
        let nodes = crate::federation::load_nodes(&self.config.data_dir)
            .await
            .unwrap_or_default();
        let fed_node = nodes.iter().find(|n| {
            peer_pubkey_hex.as_deref() == Some(n.pubkey.as_str())
                || peer_did.as_deref() == Some(n.did.as_str())
        });
        let has_tor = fed_node.is_some();
        // Distinct FIPS capability so the frontend can offer FIPS as its own
        // pill (not just a generic "Tor") — the dial layer still picks
        // FIPS-first with Tor fallback on the actual send; these are honest
        // capability labels, with `last_transport` saying what worked last.
        let has_fips = fed_node.is_some_and(|n| n.fips_npub.is_some());
        let last_transport = fed_node.and_then(|n| n.last_transport.clone());

        let est_seconds = (size.saturating_add(lora_bytes_per_sec - 1) / lora_bytes_per_sec).max(1);

        let is_reticulum = device_type == crate::mesh::types::DeviceType::Reticulum;
        let (tier, reason) = if size <= MESH_AUTO_MAX {
            ("auto-mesh", "Small enough to send inline over mesh")
        } else if size <= MESH_HARD_MAX {
            if has_tor {
                ("choose", "Fits over mesh (slow) or Tor (instant)")
            } else {
                ("auto-mesh", "No Tor path — sending inline over mesh")
            }
        } else if is_reticulum && size <= RETICULUM_RESOURCE_MAX {
            (
                "resource-mesh",
                "Sending directly over LoRa via a Reticulum resource transfer",
            )
        } else if size <= TOR_LARGE_WARN {
            if has_tor {
                ("tor-only", "Too large for mesh — Tor only")
            } else {
                ("impossible", "Too large for mesh, and peer has no Tor path")
            }
        } else {
            if has_tor {
                ("tor-only", "Large file — receiver fetch may be slow")
            } else {
                ("impossible", "Too large, and peer has no Tor path")
            }
        };

        Ok(serde_json::json!({
            "tier": tier,
            "est_seconds": est_seconds,
            "has_tor": has_tor,
            "has_fips": has_fips,
            "last_transport": last_transport,
            "reason": reason,
            "size": size,
            "mesh_auto_max": MESH_AUTO_MAX,
            "mesh_hard_max": MESH_HARD_MAX,
        }))
    }

    /// mesh.send-reply — Send a text reply targeted at an earlier message.
    /// Params: { contact_id, target_pubkey, target_seq, text }. The target
    /// MessageKey identifies the message being replied to; it does NOT need
    /// to be our peer — cross-transport replies to anyone work.
    pub(in crate::api::rpc) async fn handle_mesh_send_reply(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;
        let target_pubkey = params["target_pubkey"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing target_pubkey"))?
            .to_string();
        let target_seq = params["target_seq"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing target_seq"))?;
        let text = params["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing text"))?
            .to_string();

        let reply = ReplyPayload {
            target: MessageKey {
                sender_pubkey: target_pubkey,
                sender_seq: target_seq,
            },
            text: text.clone(),
        };

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let seq = svc.next_send_seq(contact_id).await;
        let payload = message_types::encode_payload(&reply)?;
        let envelope = TypedEnvelope::new(MeshMessageType::Reply, payload).with_seq(seq);
        let wire = envelope.to_wire()?;
        let typed_json = serde_json::to_value(&reply).ok();
        let msg = svc
            .send_typed_wire(contact_id, wire, "reply", &text, typed_json, seq)
            .await?;
        info!(contact_id, seq, "Sent reply over mesh");
        Ok(serde_json::json!({
            "sent": true,
            "message_id": msg.id,
            "sender_seq": seq,
        }))
    }

    /// mesh.send-reaction — Emoji reaction on an earlier message.
    /// Params: { contact_id, target_pubkey, target_seq, emoji }. An empty
    /// emoji string clears any existing reaction from us on that target.
    pub(in crate::api::rpc) async fn handle_mesh_send_reaction(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;
        let target_pubkey = params["target_pubkey"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing target_pubkey"))?
            .to_string();
        let target_seq = params["target_seq"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing target_seq"))?;
        let emoji = params["emoji"].as_str().unwrap_or("").to_string();

        let reaction = ReactionPayload {
            target: MessageKey {
                sender_pubkey: target_pubkey,
                sender_seq: target_seq,
            },
            emoji: emoji.clone(),
        };

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let seq = svc.next_send_seq(contact_id).await;
        let payload = message_types::encode_payload(&reaction)?;
        let envelope = TypedEnvelope::new(MeshMessageType::Reaction, payload).with_seq(seq);
        let wire = envelope.to_wire()?;
        let display = if emoji.is_empty() {
            "(cleared)".to_string()
        } else {
            emoji.clone()
        };
        let typed_json = serde_json::to_value(&reaction).ok();
        let msg = svc
            .send_typed_wire(contact_id, wire, "reaction", &display, typed_json, seq)
            .await?;
        info!(contact_id, seq, emoji = %emoji, "Sent reaction over mesh");
        Ok(serde_json::json!({
            "sent": true,
            "message_id": msg.id,
            "sender_seq": seq,
        }))
    }

    /// mesh.fetch-content — Fetch a ContentRef blob from the sender's onion and
    /// persist it to our local blob store. Params must include everything the
    /// receiver needs to construct and authorise the URL: `{ cid, sender_onion,
    /// cap_token, cap_exp, mime?, filename? }`. The caller is typically the UI
    /// clicking a content card it just received.
    pub(in crate::api::rpc) async fn handle_mesh_fetch_content(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let cid = params["cid"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing cid"))?
            .to_string();
        let mime_hint = params["mime"]
            .as_str()
            .unwrap_or("application/octet-stream")
            .to_string();
        let filename_hint = params["filename"].as_str().map(|s| s.to_string());

        let blob_store = {
            let guard = self.blob_store.read().await;
            guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Blob store not initialised"))?
                .clone()
        };

        let self_pubkey_hex = {
            let guard = self.self_pubkey_hex.read().await;
            guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Self pubkey not set"))?
                .clone()
        };

        // Short-circuit if we already hold the blob — still issue a fresh
        // self-cap so the UI gets a displayable local URL. Checked BEFORE the
        // sender_onion/cap_token/cap_exp params are required below: an inline
        // ContentInline attachment (mesh.send-content-inline) is written to
        // our own BlobStore the moment it's received/sent (dispatch.rs), so
        // its typed_payload never carries those fields at all — only a
        // ContentRef fetched from a remote peer needs them.
        if blob_store.has(&cid).await {
            let local_exp = (chrono::Utc::now().timestamp() as u64) + DEFAULT_CAP_TTL_SECS;
            let local_cap = blob_store.issue_capability(&cid, &self_pubkey_hex, local_exp);
            let local_url = format!(
                "/blob/{}?cap={}&exp={}&peer={}",
                cid, local_cap, local_exp, self_pubkey_hex
            );
            return Ok(serde_json::json!({
                "fetched": false,
                "cached": true,
                "cid": cid,
                "local_url": local_url,
            }));
        }

        let sender_onion = params["sender_onion"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing sender_onion"))?
            .trim_end_matches('/')
            .to_string();
        let cap_token = params["cap_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing cap_token"))?
            .to_string();
        let cap_exp = params["cap_exp"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing cap_exp"))?;

        // Reach the sender: FIPS preferred when the sender is federated
        // and has advertised a FIPS npub, Tor fallback otherwise.
        // Cap/exp/peer in the query string match what the sender signed in
        // handle_mesh_send_content — signature domain unchanged.
        let onion_bare = sender_onion
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .to_string();
        let path = format!(
            "/blob/{}?cap={}&exp={}&peer={}",
            cid, cap_token, cap_exp, self_pubkey_hex
        );
        let fips_npub =
            crate::federation::fips_npub_for_onion(&self.config.data_dir, &onion_bare).await;

        let (resp, transport) =
            crate::fips::dial::PeerRequest::new(fips_npub.as_deref(), &onion_bare, &path)
                .service(crate::settings::transport::PeerService::MeshFileSharing)
                .timeout(std::time::Duration::from_secs(120))
                .fips_timeout(std::time::Duration::from_secs(8))
                .record_transport(&self.config.data_dir)
                .send_get()
                .await
                .map_err(|e| anyhow::anyhow!("Fetch failed: {}", e))?;
        if !resp.status().is_success() {
            anyhow::bail!("Blob fetch HTTP {} (via {})", resp.status(), transport);
        }
        let mime = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or(mime_hint);
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("Read body failed: {}", e))?;

        let meta = blob_store
            .put(&bytes, &mime, filename_hint, None, false)
            .await?;
        if meta.cid != cid {
            anyhow::bail!("CID mismatch: expected {}, got {}", cid, meta.cid);
        }
        // Self-signed capability so the UI can GET /blob/<cid> from our own
        // node without a second round-trip.
        let local_exp = (chrono::Utc::now().timestamp() as u64) + DEFAULT_CAP_TTL_SECS;
        let local_cap = blob_store.issue_capability(&meta.cid, &self_pubkey_hex, local_exp);
        let local_url = format!(
            "/blob/{}?cap={}&exp={}&peer={}",
            meta.cid, local_cap, local_exp, self_pubkey_hex
        );
        info!(cid = %cid, size = meta.size, transport = %transport, "Fetched content_ref blob");
        Ok(serde_json::json!({
            "fetched": true,
            "cached": false,
            "cid": meta.cid,
            "size": meta.size,
            "mime": meta.mime,
            "local_url": local_url,
        }))
    }

    /// mesh.send-psbt — share a PSBT sign-request with a mesh peer.
    /// Params: `{ contact_id, psbt_hash, description, amount_sats }`. The payload
    /// carries only the SHA-256 hash; actual PSBT bytes travel out-of-band via
    /// a ContentRef or federation if needed.
    pub(in crate::api::rpc) async fn handle_mesh_send_psbt(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;
        let psbt_hash = params["psbt_hash"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing psbt_hash"))?
            .to_string();
        let description = params["description"]
            .as_str()
            .unwrap_or("PSBT sign request")
            .to_string();
        let amount_sats = params["amount_sats"].as_u64().unwrap_or(0);

        let payload = PsbtHashPayload {
            psbt_hash: psbt_hash.clone(),
            description: description.clone(),
            amount_sats,
        };

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let seq = svc.next_send_seq(contact_id).await;
        let body = message_types::encode_payload(&payload)?;
        let envelope = TypedEnvelope::new(MeshMessageType::PsbtHash, body).with_seq(seq);
        let wire = envelope.to_wire()?;
        let display = format!("PSBT {} sats — {}", amount_sats, description);
        let typed_json = serde_json::to_value(&payload).ok();
        let msg = svc
            .send_typed_wire(contact_id, wire, "psbt_hash", &display, typed_json, seq)
            .await?;
        info!(contact_id, psbt_hash = %psbt_hash, "Sent PSBT hash over mesh");
        Ok(serde_json::json!({ "sent": true, "message_id": msg.id, "sender_seq": seq }))
    }

    /// mesh.send-read-receipt — "I've seen everything from `target_pubkey` up to `target_seq`."
    /// Params: `{ contact_id, target_pubkey, target_seq }`. The peer uses this to roll
    /// forward the ✓✓ marker on its local Sent bubbles.
    pub(in crate::api::rpc) async fn handle_mesh_send_read_receipt(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;
        let target_pubkey = params["target_pubkey"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing target_pubkey"))?
            .to_string();
        let target_seq = params["target_seq"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing target_seq"))?;

        let receipt = ReadReceiptPayload {
            up_to: MessageKey {
                sender_pubkey: target_pubkey,
                sender_seq: target_seq,
            },
        };
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        // Read receipts are fired automatically just by viewing a chat (no
        // explicit user action), unlike every other typed send here — so a
        // stock (non-archy) peer that can't decode a TypedEnvelope at all
        // (e.g. a phone running plain Sideband) would otherwise get a raw
        // control envelope shoved at it the moment its message is viewed,
        // surfacing as garbage text right after whatever it just sent.
        if !svc.is_archy_peer(contact_id).await {
            return Ok(serde_json::json!({ "sent": false, "reason": "not an archy peer" }));
        }
        let seq = svc.next_send_seq(contact_id).await;
        let payload = message_types::encode_payload(&receipt)?;
        let envelope = TypedEnvelope::new(MeshMessageType::ReadReceipt, payload).with_seq(seq);
        let wire = envelope.to_wire()?;
        let display = format!("seen ≤ #{}", receipt.up_to.sender_seq);
        let typed_json = serde_json::to_value(&receipt).ok();
        let msg = svc
            .send_typed_wire(contact_id, wire, "read_receipt", &display, typed_json, seq)
            .await?;
        // Read receipts are control envelopes; the receiver uses them to
        // roll the ✓✓ marker forward on the matching outgoing bubble. They
        // must not surface as standalone bubbles in our own chat history.
        svc.drop_message_by_id(msg.id).await;
        info!(contact_id, seq, "Sent read receipt over mesh");
        Ok(serde_json::json!({ "sent": true, "message_id": msg.id, "sender_seq": seq }))
    }

    /// mesh.forward-message — Re-broadcast an existing local message to another peer,
    /// preserving original sender attribution. Params: `{ contact_id, source_message_id }`.
    /// We look up the source by local id, pull its typed_payload (or plaintext for Text),
    /// and wrap it in a Forward envelope with the original MessageKey + timestamp.
    pub(in crate::api::rpc) async fn handle_mesh_forward_message(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;
        let source_id = params["source_message_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing source_message_id"))?;

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        // Pull the source message from MeshState.
        let source = svc
            .find_message_by_id(source_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("Source message {} not found", source_id))?;

        // Forwarding a message without a stable MessageKey is meaningless —
        // the receiver can't attribute it.
        let orig_pubkey = source.sender_pubkey.clone().ok_or_else(|| {
            anyhow::anyhow!("Source message has no sender_pubkey — cannot forward")
        })?;
        let orig_seq = source
            .sender_seq
            .ok_or_else(|| anyhow::anyhow!("Source message has no sender_seq — cannot forward"))?;

        // Re-encode the original body. For typed messages we serialize the
        // existing typed_payload JSON back through CBOR via the original type;
        // for plain text we forward the plaintext as Text.
        let (body_type, body): (u8, Vec<u8>) = match source.typed_payload.as_ref() {
            Some(json) => {
                let type_label = source.message_type.as_str();
                let t =
                    MeshMessageType::from_label(type_label).unwrap_or(MeshMessageType::Text) as u8;
                let mut buf = Vec::new();
                ciborium::into_writer(json, &mut buf)
                    .map_err(|e| anyhow::anyhow!("re-encode body failed: {}", e))?;
                (t, buf)
            }
            None => (
                MeshMessageType::Text as u8,
                source.plaintext.clone().into_bytes(),
            ),
        };

        let forward = ForwardPayload {
            orig: MessageKey {
                sender_pubkey: orig_pubkey,
                sender_seq: orig_seq,
            },
            orig_ts: source
                .timestamp
                .parse::<chrono::DateTime<chrono::Utc>>()
                .map(|dt| dt.timestamp() as u32)
                .unwrap_or(chrono::Utc::now().timestamp() as u32),
            orig_name: source.peer_name.clone(),
            body_type,
            body,
        };

        let seq = svc.next_send_seq(contact_id).await;
        let payload = message_types::encode_payload(&forward)?;
        let envelope = TypedEnvelope::new(MeshMessageType::Forward, payload).with_seq(seq);
        let wire = envelope.to_wire()?;
        let display = format!(
            "Forwarded: {}",
            if source.plaintext.is_empty() {
                "(attachment)"
            } else {
                source.plaintext.as_str()
            }
        );
        let typed_json = serde_json::to_value(&forward).ok();
        let msg = svc
            .send_typed_wire(contact_id, wire, "forward", &display, typed_json, seq)
            .await?;
        info!(contact_id, seq, source_id, "Forwarded message over mesh");
        Ok(serde_json::json!({ "sent": true, "message_id": msg.id, "sender_seq": seq }))
    }

    /// mesh.edit-message — In-place edit of an earlier message's text. The target
    /// must have been sent by this node (own MessageKey). Params:
    /// `{ contact_id, target_seq, new_text }`. `target_pubkey` is implicit (self).
    pub(in crate::api::rpc) async fn handle_mesh_edit_message(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;
        let target_seq = params["target_seq"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing target_seq"))?;
        let new_text = params["new_text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing new_text"))?
            .to_string();

        let self_pubkey = {
            let guard = self.self_pubkey_hex.read().await;
            guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Self pubkey not set"))?
                .clone()
        };

        let edit = EditPayload {
            target: MessageKey {
                sender_pubkey: self_pubkey,
                sender_seq: target_seq,
            },
            new_text: new_text.clone(),
            edited_at: chrono::Utc::now().timestamp() as u32,
        };

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let seq = svc.next_send_seq(contact_id).await;
        let payload = message_types::encode_payload(&edit)?;
        let envelope = TypedEnvelope::new(MeshMessageType::Edit, payload).with_seq(seq);
        let wire = envelope.to_wire()?;
        let typed_json = serde_json::to_value(&edit).ok();
        let msg = svc
            .send_typed_wire(contact_id, wire, "edit", &new_text, typed_json, seq)
            .await?;
        // Edits are control envelopes — they mutate the target bubble in
        // apply_local_edit below, so the standalone Sent record has no UI
        // value and would just clutter the chat.
        svc.drop_message_by_id(msg.id).await;

        // Best-effort: apply the edit to our own local copy too, so the UI
        // updates without waiting for an echo.
        svc.apply_local_edit(target_seq, &new_text, edit.edited_at)
            .await;

        info!(contact_id, seq, target_seq, "Sent edit over mesh");
        Ok(serde_json::json!({ "sent": true, "message_id": msg.id, "sender_seq": seq }))
    }

    /// mesh.delete-message — Tombstone an earlier own-message. Params:
    /// `{ contact_id, target_seq }`. Applied locally immediately; wire form is
    /// informational for peers who already have the bytes.
    pub(in crate::api::rpc) async fn handle_mesh_delete_message(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;
        let target_seq = params["target_seq"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing target_seq"))?;

        let self_pubkey = {
            let guard = self.self_pubkey_hex.read().await;
            guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Self pubkey not set"))?
                .clone()
        };

        let del = DeletePayload {
            target: MessageKey {
                sender_pubkey: self_pubkey,
                sender_seq: target_seq,
            },
        };

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let seq = svc.next_send_seq(contact_id).await;
        let payload = message_types::encode_payload(&del)?;
        let envelope = TypedEnvelope::new(MeshMessageType::Delete, payload).with_seq(seq);
        let wire = envelope.to_wire()?;
        let typed_json = serde_json::to_value(&del).ok();
        let msg = svc
            .send_typed_wire(contact_id, wire, "delete", "(deleted)", typed_json, seq)
            .await?;
        // Delete is a control envelope — apply_local_delete below tombstones
        // the target bubble in place, so the standalone Sent record is just
        // noise in the chat history.
        svc.drop_message_by_id(msg.id).await;

        svc.apply_local_delete(target_seq).await;

        info!(contact_id, seq, target_seq, "Sent delete over mesh");
        Ok(serde_json::json!({ "sent": true, "message_id": msg.id, "sender_seq": seq }))
    }

    /// mesh.broadcast-presence — emit a PresencePayload heartbeat on the
    /// channel so online peers can update their presence table.
    /// Params: `{ channel?, status? }`. Defaults: channel 0, status "online".
    pub(in crate::api::rpc) async fn handle_mesh_broadcast_presence(
        &self,
        _params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // DISABLED: presence broadcasts were spamming the public channel
        // with malformed CBOR bytes (repeaters re-echoed our
        // PresencePayload as plaintext, producing "av�…fstatusfonline…").
        // The RPC stays registered so frontends that still call it don't
        // hard-fail, but it no longer transmits anything.
        Ok(serde_json::json!({ "sent": false, "reason": "presence disabled" }))
    }

    /// mesh.presence-list — return the in-memory presence map (pubkey → status+timestamps).
    pub(in crate::api::rpc) async fn handle_mesh_presence_list(
        &self,
        _params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let state = svc.shared_state();
        let presence = state.presence.read().await;
        let list: Vec<_> = presence
            .iter()
            .map(|(pk, (status, last_active, received_at))| {
                serde_json::json!({
                    "pubkey": pk,
                    "status": status,
                    "last_active": last_active,
                    "received_at": received_at,
                })
            })
            .collect();
        Ok(serde_json::json!({ "presence": list }))
    }

    /// mesh.contacts-list — return the contacts store merged with the peer list.
    pub(in crate::api::rpc) async fn handle_mesh_contacts_list(
        &self,
        _params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let state = svc.shared_state();
        let contacts = state.contacts.read().await;
        let peer_vec: Vec<_> = state.peers.read().await.values().cloned().collect();
        // Collapse radio/federation twins so a node reachable both ways shows as
        // one contact instead of two (#12).
        let groups = crate::mesh::group_peer_twins(&peer_vec);
        let mut out: Vec<serde_json::Value> = Vec::new();
        for group in &groups {
            let peer = &group.canonical;
            if let Some(pk) = peer.pubkey_hex.as_ref() {
                let entry = contacts.get(pk).cloned().unwrap_or_default();
                out.push(serde_json::json!({
                    "pubkey": pk,
                    "contact_id": peer.contact_id,
                    "name": peer.advert_name,
                    "alias": entry.alias,
                    "notes": entry.notes,
                    "pinned": entry.pinned,
                    "blocked": entry.blocked,
                }));
            }
        }
        Ok(serde_json::json!({ "contacts": out }))
    }

    /// mesh.contacts-save — create/update a contact entry (alias/notes/pinned).
    /// Params: `{ pubkey, alias?, notes?, pinned? }`.
    pub(in crate::api::rpc) async fn handle_mesh_contacts_save(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let pubkey = params["pubkey"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing pubkey"))?
            .to_string();
        let alias = params["alias"].as_str().map(|s| s.to_string());
        let notes = params["notes"].as_str().map(|s| s.to_string());
        let pinned = params["pinned"].as_bool();

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let state = svc.shared_state();
        let mut contacts = state.contacts.write().await;
        let entry = contacts.entry(pubkey.clone()).or_default();
        if alias.is_some() {
            entry.alias = alias;
        }
        if notes.is_some() {
            entry.notes = notes;
        }
        if let Some(p) = pinned {
            entry.pinned = p;
        }
        let saved = entry.clone();
        let snapshot = contacts.clone();
        drop(contacts);
        // Persist (encrypted, atomic) so the customisation survives restarts.
        if let Err(e) = crate::mesh::save_mesh_contacts(&self.config.data_dir, &snapshot).await {
            tracing::warn!("failed to persist mesh contacts: {e}");
        }
        Ok(serde_json::json!({
            "saved": true,
            "pubkey": pubkey,
            "alias": saved.alias,
            "notes": saved.notes,
            "pinned": saved.pinned,
            "blocked": saved.blocked,
        }))
    }

    /// mesh.contacts-block — toggle the blocked flag on a contact.
    /// Params: `{ pubkey, blocked }`.
    pub(in crate::api::rpc) async fn handle_mesh_contacts_block(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let pubkey = params["pubkey"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing pubkey"))?
            .to_string();
        let blocked = params["blocked"].as_bool().unwrap_or(true);

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let state = svc.shared_state();
        let mut contacts = state.contacts.write().await;
        let entry = contacts.entry(pubkey.clone()).or_default();
        entry.blocked = blocked;
        let snapshot = contacts.clone();
        drop(contacts);
        if let Err(e) = crate::mesh::save_mesh_contacts(&self.config.data_dir, &snapshot).await {
            tracing::warn!("failed to persist mesh contacts: {e}");
        }
        Ok(serde_json::json!({ "pubkey": pubkey, "blocked": blocked }))
    }

    /// mesh.send-channel-invite — share a channel invite with a direct peer.
    /// Params: `{ contact_id, channel, name, key? }`.
    pub(in crate::api::rpc) async fn handle_mesh_send_channel_invite(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let contact_id = params["contact_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing contact_id"))? as u32;
        let channel = params["channel"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing channel"))? as u8;
        let name = params["name"].as_str().unwrap_or("").to_string();
        let key = params["key"].as_str().map(|s| s.to_string());

        let invite = ChannelInvitePayload {
            channel,
            name: name.clone(),
            key,
        };
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let seq = svc.next_send_seq(contact_id).await;
        let payload = message_types::encode_payload(&invite)?;
        let envelope = TypedEnvelope::new(MeshMessageType::ChannelInvite, payload).with_seq(seq);
        let wire = envelope.to_wire()?;
        let display = format!("Channel invite: {} ({})", channel, name);
        let typed_json = serde_json::to_value(&invite).ok();
        let msg = svc
            .send_typed_wire(
                contact_id,
                wire,
                "channel_invite",
                &display,
                typed_json,
                seq,
            )
            .await?;
        Ok(serde_json::json!({ "sent": true, "message_id": msg.id, "sender_seq": seq }))
    }

    /// mesh.lightning-peers — the meshed peers that have advertised a Lightning
    /// URI, i.e. the "public/other" side of the channel-open picker (FED-05).
    ///
    /// Returns an empty array, never an error, when no peer has advertised —
    /// "nobody yet" is a normal state on a fresh node, not a fault.
    pub(in crate::api::rpc) async fn handle_mesh_lightning_peers(
        &self,
        _params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let state = svc.shared_state();
        let peer_vec: Vec<_> = state.peers.read().await.values().cloned().collect();
        Ok(serde_json::json!({ "peers": build_lightning_peer_list(&peer_vec) }))
    }

    /// mesh.send-lightning-info — advertise THIS node's Lightning URI to one
    /// chosen peer.
    ///
    /// Requires an explicit target. There is deliberately no broadcast form:
    /// this discloses the node's payment endpoint, and who learns it is the
    /// operator's choice, not a side effect of being in radio range (T-01-13).
    pub(in crate::api::rpc) async fn handle_mesh_send_lightning_info(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let contact_id = parse_send_lightning_target(params.as_ref())?;

        // Read our own URI from the lnd.getinfo path. Refuse rather than send an
        // empty advertisement: a peer that stored "" would show us in its picker
        // as a target it can never dial.
        let info = self
            .handle_lnd_getinfo()
            .await
            .map_err(|e| anyhow::anyhow!("Cannot read this node's Lightning info: {e}"))?;
        let uri = info
            .get("uris")
            .and_then(|u| u.as_array())
            .and_then(|a| a.first())
            .and_then(|u| u.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "This node has no advertised Lightning URI to share — LND may be down, or \
                     configured with no externally reachable address"
                )
            })?;
        if !message_types::is_valid_lightning_uri(&uri) {
            return Err(anyhow::anyhow!(
                "This node's own Lightning URI is malformed; refusing to advertise it"
            ));
        }
        let alias = info
            .get("alias")
            .and_then(|a| a.as_str())
            .filter(|a| !a.is_empty())
            .map(|a| a.to_string());

        let payload_struct = message_types::LightningInfoPayload { uri, alias };

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let seq = svc.next_send_seq(contact_id).await;
        let payload = message_types::encode_payload(&payload_struct)?;
        let envelope = TypedEnvelope::new(MeshMessageType::LightningInfo, payload).with_seq(seq);
        let wire = envelope.to_wire()?;
        let typed_json = serde_json::to_value(&payload_struct).ok();
        let msg = svc
            .send_typed_wire(
                contact_id,
                wire,
                "lightning_info",
                "Shared Lightning connection info",
                typed_json,
                seq,
            )
            .await?;
        info!(contact_id, seq, "Sent lightning_info to a chosen mesh peer");
        Ok(serde_json::json!({ "sent": true, "message_id": msg.id, "sender_seq": seq }))
    }
}

/// The required target for `mesh.send-lightning-info`.
///
/// Split out so the "a target is mandatory" contract is testable without a mesh
/// service — that contract is the whole of T-01-13's mitigation, so it should
/// not be provable only by reading the code.
fn parse_send_lightning_target(params: Option<&serde_json::Value>) -> Result<u32> {
    let params =
        params.ok_or_else(|| anyhow::anyhow!("Missing params: a target contact_id is required"))?;
    let contact_id = params["contact_id"].as_u64().ok_or_else(|| {
        anyhow::anyhow!(
            "Missing contact_id: mesh.send-lightning-info requires an explicit target and has no \
             broadcast form"
        )
    })?;
    u32::try_from(contact_id).map_err(|_| anyhow::anyhow!("contact_id out of range"))
}

/// Build the deduplicated, deterministically ordered Lightning-peer list.
///
/// Pure so the dedup and ordering contracts are testable without a mesh
/// service. Ordering matters for a real reason: the picker must not reshuffle
/// between reads, or an operator clicking a row can hit a different node than
/// the one they aimed at.
fn build_lightning_peer_list(peers: &[crate::mesh::types::MeshPeer]) -> Vec<serde_json::Value> {
    use std::collections::HashMap;

    // Dedup by the AUTHENTICATING key, never the firmware routing key: a radio
    // contact and its federation twin are one node and must not be offered
    // twice (T-01-11). Peers with no key at all fall back to contact_id, which
    // is unique per record.
    let mut best: HashMap<String, &crate::mesh::types::MeshPeer> = HashMap::new();
    for peer in peers.iter().filter(|p| p.lightning_uri.is_some()) {
        let key = peer
            .identity_pubkey_hex()
            .map(|k| k.to_ascii_lowercase())
            .unwrap_or_else(|| format!("contact:{}", peer.contact_id));
        best.entry(key)
            .and_modify(|kept| {
                // Newest advertisement wins. last_heard is RFC3339; parse rather
                // than string-compare so a differing offset can't misorder.
                let kept_at = chrono::DateTime::parse_from_rfc3339(&kept.last_heard).ok();
                let this_at = chrono::DateTime::parse_from_rfc3339(&peer.last_heard).ok();
                if this_at >= kept_at {
                    *kept = peer;
                }
            })
            .or_insert(peer);
    }

    let mut out: Vec<&crate::mesh::types::MeshPeer> = best.into_values().collect();
    // Sort by display name, then contact_id as the tiebreak, so two peers
    // sharing a name still have a total order and the list is stable across
    // reads (a HashMap's iteration order is not).
    out.sort_by(|a, b| {
        a.advert_name
            .to_lowercase()
            .cmp(&b.advert_name.to_lowercase())
            .then(a.contact_id.cmp(&b.contact_id))
    });

    out.into_iter()
        .map(|p| {
            serde_json::json!({
                "contact_id": p.contact_id,
                "name": p.advert_name,
                "lightning_uri": p.lightning_uri,
                "last_heard": p.last_heard,
                "reachable": p.reachable,
                "hops": p.hops,
            })
        })
        .collect()
}

#[cfg(test)]
mod lightning_peer_tests {
    use super::*;
    use crate::mesh::types::MeshPeer;

    const URI_A: &str =
        "03a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90@1.2.3.4:9735";
    const URI_B: &str =
        "02ffeeddccbbaa998877665544332211ffeeddccbbaa998877665544332211ffee@5.6.7.8:9735";

    fn peer(contact_id: u32, name: &str, arch: Option<&str>) -> MeshPeer {
        MeshPeer {
            contact_id,
            advert_name: name.into(),
            did: None,
            pubkey_hex: Some(format!("routing{contact_id}")),
            arch_pubkey_hex: arch.map(|s| s.into()),
            x25519_pubkey: None,
            rssi: None,
            snr: None,
            last_heard: "2026-08-02T10:00:00+00:00".into(),
            hops: 1,
            last_advert: 0,
            reachable: true,
            pkc_capable: false,
            lat: None,
            lon: None,
            lightning_uri: None,
        }
    }

    #[test]
    fn no_advertised_peers_is_an_empty_list_not_an_error() {
        assert!(build_lightning_peer_list(&[]).is_empty());
        // Peers exist, but none has advertised Lightning.
        let quiet = vec![peer(1, "a", None), peer(2, "b", None)];
        assert!(build_lightning_peer_list(&quiet).is_empty());
    }

    #[test]
    fn only_peers_that_advertised_are_listed() {
        let mut with = peer(1, "has-lightning", None);
        with.lightning_uri = Some(URI_A.into());
        let list = build_lightning_peer_list(&[with, peer(2, "no-lightning", None)]);

        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["name"], "has-lightning");
        assert_eq!(list[0]["lightning_uri"], URI_A);
    }

    #[test]
    fn a_peer_that_advertised_twice_appears_once_with_the_newer_uri() {
        // Same node seen as two records (radio + federation twin) sharing an
        // authenticating key — the picker must offer it once, not twice.
        let mut older = peer(1, "twin", Some("ARCHKEY"));
        older.lightning_uri = Some(URI_A.into());
        older.last_heard = "2026-08-02T10:00:00+00:00".into();

        let mut newer = peer(2, "twin", Some("archkey")); // case-insensitive match
        newer.lightning_uri = Some(URI_B.into());
        newer.last_heard = "2026-08-02T11:30:00+00:00".into();

        let list = build_lightning_peer_list(&[older.clone(), newer.clone()]);
        assert_eq!(list.len(), 1, "twins must collapse to one entry");
        assert_eq!(list[0]["lightning_uri"], URI_B, "the newer URI must win");

        // Order of the input must not change the outcome.
        let reversed = build_lightning_peer_list(&[newer, older]);
        assert_eq!(reversed[0]["lightning_uri"], URI_B);
    }

    #[test]
    fn a_differing_offset_cannot_misorder_the_newest_advertisement() {
        // 09:30-01:00 is 10:30 UTC — LATER than 10:00Z, though it string-sorts
        // earlier. Parsing rather than string-comparing is what makes this pass.
        let mut utc = peer(1, "twin", Some("k"));
        utc.lightning_uri = Some(URI_A.into());
        utc.last_heard = "2026-08-02T10:00:00+00:00".into();

        let mut offset = peer(2, "twin", Some("k"));
        offset.lightning_uri = Some(URI_B.into());
        offset.last_heard = "2026-08-02T09:30:00-01:00".into();

        let list = build_lightning_peer_list(&[utc, offset]);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["lightning_uri"], URI_B);
    }

    #[test]
    fn ordering_is_stable_and_deterministic_across_reads() {
        let mut peers = Vec::new();
        for (id, name) in [(3, "Zulu"), (1, "alpha"), (2, "Mike"), (9, "alpha")] {
            let mut p = peer(id, name, Some(&format!("key{id}")));
            p.lightning_uri = Some(URI_A.into());
            peers.push(p);
        }

        let first = build_lightning_peer_list(&peers);
        // A HashMap's iteration order is not stable, so run it repeatedly over a
        // shuffled input: the output must be byte-identical every time.
        for _ in 0..8 {
            peers.rotate_left(1);
            assert_eq!(build_lightning_peer_list(&peers), first);
        }

        let names: Vec<_> = first.iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["alpha", "alpha", "Mike", "Zulu"]);
        // Same name -> contact_id breaks the tie, ascending.
        assert_eq!(first[0]["contact_id"], 1);
        assert_eq!(first[1]["contact_id"], 9);
    }

    #[test]
    fn send_requires_an_explicit_target_and_has_no_broadcast_form() {
        assert!(parse_send_lightning_target(None).is_err(), "no params");
        assert!(
            parse_send_lightning_target(Some(&serde_json::json!({}))).is_err(),
            "params without contact_id must be refused, not treated as broadcast"
        );
        assert!(
            parse_send_lightning_target(Some(&serde_json::json!({"broadcast": true}))).is_err(),
            "there is no broadcast escape hatch"
        );
        assert!(
            parse_send_lightning_target(Some(&serde_json::json!({"contact_id": 1u64 << 40})))
                .is_err(),
            "an out-of-range contact_id must error, not silently truncate to another peer"
        );
        assert_eq!(
            parse_send_lightning_target(Some(&serde_json::json!({"contact_id": 42}))).unwrap(),
            42
        );
    }
}
