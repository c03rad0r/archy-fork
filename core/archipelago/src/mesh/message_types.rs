// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Typed message envelope for mesh communication.
//!
//! Wraps all mesh messages in a CBOR envelope with type discrimination,
//! enabling different message kinds (TEXT, ALERT, INVOICE, COORDINATE, etc.)
//! over the same encrypted channel.
//!
//! Wire format: `[0x02: typed_marker] [CBOR envelope]`
//! The 0x02 prefix distinguishes typed messages from plain text (0x00)
//! and identity broadcasts (0x01 / ARCHY:2/3).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Wire prefix for typed messages.
pub const TYPED_MESSAGE_MARKER: u8 = 0x02;

/// Wire prefix for encrypted typed messages (E2E encrypted with static shared secret).
/// Format: [0xEE] [nonce: 12 bytes] [ciphertext + auth tag]
pub const ENCRYPTED_TYPED_MARKER: u8 = 0xEE;

/// Wire prefix for Double Ratchet encrypted typed messages (forward secrecy).
/// Format: [0xDD] [RatchetHeader: 40 bytes] [nonce: 12] [ciphertext] [tag: 16]
pub const RATCHET_TYPED_MARKER: u8 = 0xDD;

/// Message type discriminator.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeshMessageType {
    Text = 0,
    Alert = 1,
    Invoice = 2,
    PsbtHash = 3,
    Coordinate = 4,
    PrekeyBundle = 5,
    SessionInit = 6,
    BlockHeader = 7,
    TxRelay = 8,
    TxRelayResponse = 9,
    LightningRelay = 10,
    LightningRelayResponse = 11,
    /// Confirmation update for a relayed transaction (1, 2, 3 confs).
    TxConfirmation = 12,
    /// Reply to an earlier message, targeted by MessageKey.
    Reply = 13,
    /// Emoji reaction on an earlier message, targeted by MessageKey.
    Reaction = 14,
    /// Read receipt — "I've seen everything up to and including this MessageKey."
    ReadReceipt = 15,
    /// Forwarded message — wraps the original sender+timestamp+body so the
    /// receiver can render "Forwarded from <name>" above the original content.
    Forward = 16,
    /// In-place edit of an earlier message (text replacement). UI shows "edited".
    Edit = 17,
    /// Tombstone for a prior message — peer already has the bytes, we just
    /// mark it deleted locally. "🗑 message deleted" in the UI.
    Delete = 18,
    /// Attachment/file reference: CID of a blob held by the sender, fetched out-of-band.
    ContentRef = 19,
    /// Periodic heartbeat advertising presence + last-activity epoch.
    Presence = 20,
    /// Channel membership announcement — used by the Phase 5 channels/groups feature.
    ChannelInvite = 21,
    /// Shareable contact card — advertises a federation node (did, onion, pubkey).
    /// Lets the receiver one-click-federate with that node.
    ContactCard = 22,
    /// Inline attachment: file bytes carried directly in the envelope. Used
    /// when the file is small enough to chunk over LoRa (<~2.3 KB after
    /// MCIIXXTT framing) and the peer has no Tor path. Recipient writes the
    /// bytes to its local BlobStore on reassembly.
    ContentInline = 23,
    /// "Ask the node's AI" — a prompt to be answered by the receiving node's
    /// local LLM (issue #50). Gated by the assistant config + trust policy.
    AssistQuery = 24,
    /// Reply to an AssistQuery — a chunk of the LLM's answer, addressed back to
    /// the asker by `req_id`. Long answers span multiple chunks (`seq`/`done`).
    AssistResponse = 25,
    /// "I run Lightning, and this is how to reach me" — advertises the sender's
    /// LND connection URI so the recipient can offer it as a channel-open
    /// target.
    ///
    /// Only ever sent on an explicit operator action against a chosen peer
    /// (`mesh.send-lightning-info`). It is never auto-broadcast to contacts in
    /// range, and a received advertisement is never re-advertised onward — the
    /// URI is this node's payment endpoint, and who learns it is the operator's
    /// choice (T-01-13).
    LightningInfo = 26,
}

impl MeshMessageType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Text),
            1 => Some(Self::Alert),
            2 => Some(Self::Invoice),
            3 => Some(Self::PsbtHash),
            4 => Some(Self::Coordinate),
            5 => Some(Self::PrekeyBundle),
            6 => Some(Self::SessionInit),
            7 => Some(Self::BlockHeader),
            8 => Some(Self::TxRelay),
            9 => Some(Self::TxRelayResponse),
            10 => Some(Self::LightningRelay),
            11 => Some(Self::LightningRelayResponse),
            12 => Some(Self::TxConfirmation),
            13 => Some(Self::Reply),
            14 => Some(Self::Reaction),
            15 => Some(Self::ReadReceipt),
            16 => Some(Self::Forward),
            17 => Some(Self::Edit),
            18 => Some(Self::Delete),
            19 => Some(Self::ContentRef),
            20 => Some(Self::Presence),
            21 => Some(Self::ChannelInvite),
            22 => Some(Self::ContactCard),
            23 => Some(Self::ContentInline),
            24 => Some(Self::AssistQuery),
            25 => Some(Self::AssistResponse),
            26 => Some(Self::LightningInfo),
            _ => None,
        }
    }

    /// Inverse of `label()` — used by the Forward path when it needs to
    /// re-encode a stored message whose original `MeshMessageType` we only
    /// kept as a string label in `MeshMessage.message_type`.
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "text" => Some(Self::Text),
            "alert" => Some(Self::Alert),
            "invoice" => Some(Self::Invoice),
            "psbt_hash" => Some(Self::PsbtHash),
            "coordinate" => Some(Self::Coordinate),
            "prekey_bundle" => Some(Self::PrekeyBundle),
            "session_init" => Some(Self::SessionInit),
            "block_header" => Some(Self::BlockHeader),
            "tx_relay" => Some(Self::TxRelay),
            "tx_relay_response" => Some(Self::TxRelayResponse),
            "lightning_relay" => Some(Self::LightningRelay),
            "lightning_relay_response" => Some(Self::LightningRelayResponse),
            "tx_confirmation" => Some(Self::TxConfirmation),
            "reply" => Some(Self::Reply),
            "reaction" => Some(Self::Reaction),
            "read_receipt" => Some(Self::ReadReceipt),
            "forward" => Some(Self::Forward),
            "edit" => Some(Self::Edit),
            "delete" => Some(Self::Delete),
            "content_ref" => Some(Self::ContentRef),
            "presence" => Some(Self::Presence),
            "channel_invite" => Some(Self::ChannelInvite),
            "contact_card" => Some(Self::ContactCard),
            "content_inline" => Some(Self::ContentInline),
            "assist_query" => Some(Self::AssistQuery),
            "assist_response" => Some(Self::AssistResponse),
            "lightning_info" => Some(Self::LightningInfo),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Alert => "alert",
            Self::Invoice => "invoice",
            Self::PsbtHash => "psbt_hash",
            Self::Coordinate => "coordinate",
            Self::PrekeyBundle => "prekey_bundle",
            Self::SessionInit => "session_init",
            Self::BlockHeader => "block_header",
            Self::TxRelay => "tx_relay",
            Self::TxRelayResponse => "tx_relay_response",
            Self::LightningRelay => "lightning_relay",
            Self::LightningRelayResponse => "lightning_relay_response",
            Self::TxConfirmation => "tx_confirmation",
            Self::Reply => "reply",
            Self::Reaction => "reaction",
            Self::ReadReceipt => "read_receipt",
            Self::Forward => "forward",
            Self::Edit => "edit",
            Self::Delete => "delete",
            Self::ContentRef => "content_ref",
            Self::Presence => "presence",
            Self::ChannelInvite => "channel_invite",
            Self::ContactCard => "contact_card",
            Self::ContentInline => "content_inline",
            Self::AssistQuery => "assist_query",
            Self::AssistResponse => "assist_response",
            Self::LightningInfo => "lightning_info",
        }
    }
}

/// Cross-transport stable identity for any mesh message. `sender_pubkey` is
/// the hex Ed25519 key of the originator; `sender_seq` is that sender's
/// per-peer (or per-channel) monotonic counter at send time. Receivers use
/// this pair — not the local u64 `id` — when addressing replies, reactions,
/// reads, edits, or deletes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageKey {
    pub sender_pubkey: String,
    pub sender_seq: u64,
}

// ─── Wire Envelope ──────────────────────────────────────────────────────

/// CBOR wire envelope wrapping any typed message.
///
/// `v`/`sig` MUST use `compact_bytes`/`compact_bytes_opt` — this is the
/// envelope EVERY message type wraps its payload in, so plain derived
/// `Vec<u8>` encoding here (one CBOR integer per byte instead of a native
/// byte string) bloats every single message on the wire, not just
/// attachments. Root-caused live: a small ReadReceipt (tiny inner payload)
/// crossed the 140-byte single-frame threshold purely from this envelope's
/// own array-of-ints tax on `v`, triggering MC-chunked send to a Reticulum
/// peer whose chunks then failed to reassemble — surfaced as raw
/// `MC000...` fragments in the chat instead of a receipt. Fix this here,
/// not just on individual payload structs like `ContentInlinePayload`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedEnvelope {
    /// Message type.
    pub t: u8,
    /// Payload bytes (type-specific CBOR or raw data).
    #[serde(with = "compact_bytes")]
    pub v: Vec<u8>,
    /// Unix timestamp (seconds since epoch).
    pub ts: u32,
    /// Optional Ed25519 signature of (t || v || ts_bytes) — for signed messages.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "compact_bytes_opt"
    )]
    pub sig: Option<Vec<u8>>,
    /// Message sequence number (per-sender, monotonically increasing).
    #[serde(default)]
    pub seq: u64,
}

impl TypedEnvelope {
    /// Create an unsigned envelope.
    pub fn new(msg_type: MeshMessageType, payload: Vec<u8>) -> Self {
        let ts = chrono::Utc::now().timestamp() as u32;
        Self {
            t: msg_type as u8,
            v: payload,
            ts,
            sig: None,
            seq: 0,
        }
    }

    /// Create a signed envelope (for ALERT, BlockHeader).
    pub fn new_signed(
        msg_type: MeshMessageType,
        payload: Vec<u8>,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Self {
        use ed25519_dalek::Signer;
        let ts = chrono::Utc::now().timestamp() as u32;

        // Sign: type byte || payload || timestamp (4 bytes LE)
        let mut sign_data = Vec::with_capacity(1 + payload.len() + 4);
        sign_data.push(msg_type as u8);
        sign_data.extend_from_slice(&payload);
        sign_data.extend_from_slice(&ts.to_le_bytes());
        let signature = signing_key.sign(&sign_data);

        Self {
            t: msg_type as u8,
            v: payload,
            ts,
            sig: Some(signature.to_bytes().to_vec()),
            seq: 0,
        }
    }

    /// Signing preimage v2: binds the anti-replay `seq` so a radio MITM
    /// can't reorder/replay a signed message under a different sequence
    /// number. v1 (legacy) covers only (t, v, ts).
    fn signing_preimage_v2(&self) -> Vec<u8> {
        let mut sign_data = Vec::with_capacity(1 + self.v.len() + 4 + 8);
        sign_data.push(self.t);
        sign_data.extend_from_slice(&self.v);
        sign_data.extend_from_slice(&self.ts.to_le_bytes());
        sign_data.extend_from_slice(&self.seq.to_le_bytes());
        sign_data
    }

    fn signing_preimage_v1(&self) -> Vec<u8> {
        let mut sign_data = Vec::with_capacity(1 + self.v.len() + 4);
        sign_data.push(self.t);
        sign_data.extend_from_slice(&self.v);
        sign_data.extend_from_slice(&self.ts.to_le_bytes());
        sign_data
    }

    /// Verify signature if present. Accepts the seq-binding v2 preimage OR
    /// the legacy (t, v, ts) preimage — senders still emit v1 until the
    /// whole fleet verifies v2 (receivers hard-drop bad signatures, so
    /// flipping the send side first would break mixed-fleet alerts). The
    /// seq-tampering window closes only when the v1 arm is removed.
    pub fn verify_signature(&self, verifying_key: &ed25519_dalek::VerifyingKey) -> Result<bool> {
        let Some(sig_bytes) = &self.sig else {
            return Ok(false);
        };
        let signature =
            ed25519_dalek::Signature::from_slice(sig_bytes).context("Invalid signature bytes")?;

        if verifying_key
            .verify_strict(&self.signing_preimage_v2(), &signature)
            .is_ok()
        {
            return Ok(true);
        }
        verifying_key
            .verify_strict(&self.signing_preimage_v1(), &signature)
            .context("Signature verification failed")?;
        Ok(true)
    }

    /// Get the message type.
    pub fn message_type(&self) -> Option<MeshMessageType> {
        MeshMessageType::from_u8(self.t)
    }

    /// Set the outbound sequence number. Called by the send path after the
    /// target's counter has been incremented. Safe to call AFTER `new_signed`
    /// because the v1 signature covers `(t, v, ts)` — not `seq`. Once the
    /// fleet is on a build whose `verify_signature` accepts the v2 preimage,
    /// flip senders to sign AFTER seq allocation via `signed_with_seq`.
    pub fn with_seq(mut self, seq: u64) -> Self {
        self.seq = seq;
        self
    }

    /// v2 sender path (NOT yet wired — see `verify_signature` for the fleet
    /// migration order): set seq first, then sign binding it.
    #[allow(dead_code)]
    pub fn signed_with_seq(mut self, seq: u64, signing_key: &ed25519_dalek::SigningKey) -> Self {
        use ed25519_dalek::Signer;
        self.seq = seq;
        let signature = signing_key.sign(&self.signing_preimage_v2());
        self.sig = Some(signature.to_bytes().to_vec());
        self
    }

    /// Encode to wire format: [0x02] [CBOR envelope].
    pub fn to_wire(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        buf.push(TYPED_MESSAGE_MARKER);
        ciborium::into_writer(self, &mut buf).context("CBOR encode failed")?;
        Ok(buf)
    }

    /// Decode from wire format.
    pub fn from_wire(data: &[u8]) -> Result<Self> {
        if data.is_empty() || data[0] != TYPED_MESSAGE_MARKER {
            anyhow::bail!("Not a typed message (expected 0x02 prefix)");
        }
        ciborium::from_reader(&data[1..]).context("CBOR decode failed")
    }

    /// Check if raw bytes are a typed message.
    pub fn is_typed(data: &[u8]) -> bool {
        !data.is_empty() && data[0] == TYPED_MESSAGE_MARKER
    }
}

// ─── Payload Types ──────────────────────────────────────────────────────

/// Alert severity / type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertType {
    Emergency,
    Status,
    DeadMan,
    BlockHeader,
}

/// GPS coordinate stored as integer microdegrees (no floating point).
/// 1 microdegree ≈ 0.11 meters at the equator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coordinate {
    /// Latitude in microdegrees (degrees × 1_000_000).
    pub lat: i32,
    /// Longitude in microdegrees (degrees × 1_000_000).
    pub lng: i32,
    /// Optional human-readable label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Coordinate {
    /// Create from floating-point degrees (convenience for UI layer).
    pub fn from_degrees(lat: f64, lng: f64, label: Option<String>) -> Self {
        Self {
            lat: (lat * 1_000_000.0) as i32,
            lng: (lng * 1_000_000.0) as i32,
            label,
        }
    }

    /// Convert to floating-point degrees (for display only).
    pub fn lat_degrees(&self) -> f64 {
        self.lat as f64 / 1_000_000.0
    }

    pub fn lng_degrees(&self) -> f64 {
        self.lng as f64 / 1_000_000.0
    }
}

/// Alert payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertPayload {
    pub alert_type: AlertType,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinate: Option<Coordinate>,
}

/// Lightning invoice payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoicePayload {
    pub bolt11: String,
    /// Amount in satoshis — always u64, never float.
    pub amount_sats: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    /// Payment hash hex (for tracking).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_hash: Option<String>,
}

/// PSBT coordination payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PsbtHashPayload {
    /// SHA-256 hash of the PSBT (hex).
    pub psbt_hash: String,
    pub description: String,
    /// Amount in satoshis.
    pub amount_sats: u64,
}

/// Block header announcement (from internet-connected to mesh-only peers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeaderPayload {
    pub height: u64,
    /// Block hash (64 hex chars).
    pub hash: String,
    /// Previous block hash for chain continuity.
    pub prev_hash: String,
    /// Block timestamp.
    pub timestamp: u32,
    /// Announced by DID.
    pub announced_by: String,
}

/// Transaction relay request (mesh-only → internet peer).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxRelayPayload {
    pub tx_hex: String,
    pub request_id: u64,
}

/// Transaction relay response (internet peer → mesh-only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxRelayResponsePayload {
    pub request_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Machine-readable error code: bitcoin_unreachable, bitcoin_syncing, tx_rejected, rpc_parse_error
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

/// Lightning invoice relay request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightningRelayPayload {
    pub bolt11: String,
    pub amount_sats: u64,
    pub request_id: u64,
}

/// "Ask the node's AI" request (issue #50). Sent to a peer running a local
/// LLM; answered with one or more `AssistResponsePayload` chunks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistQueryPayload {
    /// Asker-chosen id correlating the query with its response chunks.
    pub req_id: u64,
    /// The natural-language prompt.
    pub prompt: String,
    /// Optional model override; falls back to the responder's configured model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// One chunk of an AI answer, addressed back to the asker by `req_id`.
/// Airtime is scarce, so long answers are capped and split into ordered chunks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistResponsePayload {
    pub req_id: u64,
    /// This chunk's text.
    pub text: String,
    /// 0-based chunk index.
    #[serde(default)]
    pub seq: u16,
    /// True on the final chunk.
    #[serde(default)]
    pub done: bool,
    /// Set instead of `text` when the query failed (model unreachable, denied…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Lightning relay response (proof of payment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightningRelayResponsePayload {
    pub request_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_hash: Option<String>,
    /// Preimage as proof of payment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preimage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Reply to an earlier message. `target` is the MessageKey the reply points
/// at; `text` is the reply body. Receivers render this as a "replying to"
/// card above the text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyPayload {
    pub target: MessageKey,
    pub text: String,
}

/// Emoji reaction on an earlier message. `target` identifies the reacted-to
/// message; `emoji` is a short unicode string (typically 1–4 code points).
/// An empty emoji is treated as "clear my reaction" on the target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionPayload {
    pub target: MessageKey,
    pub emoji: String,
}

/// `Option<Vec<u8>>` <-> base64 string, for fields that need to survive a JSON
/// round-trip to the frontend readably (plain serde would emit/expect a JSON
/// array of numbers for `Vec<u8>`, which isn't what `data:` URLs want). CBOR
/// wire encoding pays a small (~33%) size tax for this on `thumb_bytes`
/// specifically — negligible given thumbnails are capped at ~60 bytes.
mod base64_opt_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(bytes) => s.serialize_str(&STANDARD.encode(bytes)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let opt: Option<String> = Option::deserialize(d)?;
        opt.map(|s| STANDARD.decode(&s).map_err(serde::de::Error::custom))
            .transpose()
    }
}

/// Content/attachment reference: points at a blob held by the sender that
/// recipients fetch out-of-band via `GET {sender_onion}/blob/{cid}?cap=..&exp=..&peer=..`.
/// Thumb bytes (≤60B) may be inlined for immediate display; full blob is lazy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentRefPayload {
    pub cid: String,
    pub size: u64,
    pub mime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "base64_opt_bytes"
    )]
    pub thumb_bytes: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Sender's onion host (no scheme, no trailing slash) — `<host>.onion`.
    pub sender_onion: String,
    /// HMAC capability token scoped to (cid, recipient_pubkey, exp).
    pub cap_token: String,
    /// Capability expiry (unix seconds).
    pub cap_exp: u64,
}

/// Serde's blanket `Serialize`/`Deserialize` for `Vec<u8>` goes through
/// `serialize_seq`/one CBOR integer per byte, NOT CBOR's native byte-string
/// type — measured ~3.5x wire bloat on a real attachment send (4746 raw
/// bytes -> 16638-byte CBOR envelope) before this fix. `serialize_bytes`
/// maps to CBOR major type 2 (compact byte string) instead. Only apply this
/// to fields that never need JSON round-tripping to the frontend (this one
/// is CBOR-wire-only — the frontend gets `cid`/`size`/`mime` metadata built
/// by hand, never the raw bytes, see typed_messages.rs's `typed_json`).
mod compact_bytes {
    use serde::{Deserializer, Serializer};
    use std::fmt;

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }

    struct BytesVisitor;
    impl<'de> serde::de::Visitor<'de> for BytesVisitor {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a byte string")
        }
        fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Vec<u8>, E> {
            Ok(v.to_vec())
        }
        fn visit_borrowed_bytes<E: serde::de::Error>(self, v: &'de [u8]) -> Result<Vec<u8>, E> {
            Ok(v.to_vec())
        }
        fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Vec<u8>, E> {
            Ok(v)
        }
        // ciborium's non-self-describing byte-string decode path visits a
        // seq of u8 in some configurations rather than calling visit_bytes
        // directly — accept that too so this is robust to the reader mode.
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u8>, A::Error> {
            let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            while let Some(byte) = seq.next_element::<u8>()? {
                out.push(byte);
            }
            Ok(out)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        // NOT deserialize_bytes: ciborium's deserialize_bytes only succeeds
        // when the byte string fits its small internal scratch buffer —
        // anything bigger (any real attachment) falls through to an
        // "invalid type: bytes, expected bytes" error despite the CBOR
        // header being genuinely Bytes. deserialize_byte_buf streams
        // segments into an unbounded Vec instead (confirmed against
        // ciborium 0.2.2's de/mod.rs — deserialize_bytes's `Header::Bytes(Some(len))
        // if len <= self.scratch.len()` guard vs deserialize_byte_buf's
        // unconditional `Header::Bytes(len)` streaming path).
        d.deserialize_byte_buf(BytesVisitor)
    }
}

/// `Option<Vec<u8>>` variant of `compact_bytes` — for wire-only optional byte
/// fields (e.g. `TypedEnvelope.sig`) that never need JSON round-tripping.
/// Not the same as `base64_opt_bytes` below, which exists specifically
/// because `ContentRefPayload.thumb_bytes` DOES need a JSON-friendly (string)
/// form for the frontend's `data:` URL — this one stays fully binary.
mod compact_bytes_opt {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(bytes) => s.serialize_bytes(bytes),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        #[derive(Deserialize)]
        struct Wrapper(#[serde(with = "super::compact_bytes")] Vec<u8>);
        let opt: Option<Wrapper> = Option::deserialize(d)?;
        Ok(opt.map(|w| w.0))
    }
}

/// Inline attachment payload — file bytes carried directly in the envelope.
/// Used when the file is small enough to chunk over LoRa and the peer has no
/// Tor path. Receiver writes `bytes` to its local BlobStore on reassembly
/// and renders it as a normal attachment card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentInlinePayload {
    pub mime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(with = "compact_bytes")]
    pub bytes: Vec<u8>,
}

/// Read receipt — "I have seen every message from this sender up to and
/// including `up_to`." Receivers apply this to fold the ✓✓ "seen" marker
/// onto all local messages whose MessageKey ≤ `up_to` for that pubkey.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadReceiptPayload {
    pub up_to: MessageKey,
}

/// Forwarded message. Carries the original sender's pubkey/seq/timestamp and
/// an optional human display name alongside the re-broadcast body so the UI
/// can render "Forwarded from <name>" above the original content. `body_type`
/// + `body` hold the re-serialized original payload bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardPayload {
    pub orig: MessageKey,
    pub orig_ts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orig_name: Option<String>,
    /// Original MeshMessageType (u8) — lets the receiver render the inner
    /// content using the same renderer as the original.
    pub body_type: u8,
    pub body: Vec<u8>,
}

/// In-place edit of an earlier message. `target` must have originated from
/// the sender (enforced on receive). `new_text` replaces plaintext; UI
/// appends an "edited" marker using `edited_at`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditPayload {
    pub target: MessageKey,
    pub new_text: String,
    pub edited_at: u32,
}

/// Tombstone for a prior message by the same sender. Receivers replace the
/// target's displayed content with a "🗑 message deleted" marker locally.
/// True removal is impossible (peers already have the bytes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletePayload {
    pub target: MessageKey,
}

/// Presence heartbeat — sent periodically so peers know you're online.
/// `status` is a short tag ("online", "away", "dnd"); `last_active` is epoch secs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresencePayload {
    pub status: String,
    pub last_active: u32,
}

/// LightningInfo — the sender's LND connection URI, so the recipient can offer
/// it as a channel-open target.
///
/// `uri` is the standard `pubkey@host:port` form (the `:port` suffix is
/// optional). `alias` is the node's human-readable name, carried so the picker
/// can label the entry without a second round trip; it is advisory and
/// unverified, so it must never be used as an identity.
///
/// Kept to two fields on purpose: this rides LoRa, where every byte is paid for
/// on air.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightningInfoPayload {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

/// Is this a well-formed Lightning connection URI (`pubkey@host` or
/// `pubkey@host:port`)?
///
/// Validated before anything is stored, because this arrives over unauthenticated
/// RF: the pubkey part must be 66 hexadecimal characters (a compressed secp256k1
/// key, the same rule `lnd.openchannel` enforces) and the host part must be
/// non-empty. Deliberately does NOT resolve or dial the host — that would turn a
/// received advertisement into an outbound connection an attacker chose.
pub fn is_valid_lightning_uri(uri: &str) -> bool {
    // Exactly one '@': "a@b@c" must not pass by splitting on the first one.
    let mut parts = uri.split('@');
    let (Some(pubkey), Some(host), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    if pubkey.len() != 66 || !pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    // Strip an optional :port and require the remaining host to be non-empty.
    // rsplit_once so IPv6-ish hosts don't lose their body to the first colon.
    let host_only = match host.rsplit_once(':') {
        Some((h, port)) => {
            if port.is_empty() || !port.chars().all(|c| c.is_ascii_digit()) {
                return false;
            }
            if port.parse::<u16>().is_err() {
                return false;
            }
            h
        }
        None => host,
    };
    !host_only.is_empty() && !host_only.contains(char::is_whitespace)
}

/// ChannelInvite — advertise/invite a peer to join a channel. `key` is an
/// optional base64 pre-shared secret; absent `key` means public.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInvitePayload {
    pub channel: u8,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// Transaction confirmation update (relay node → originator).
/// Sent after each new confirmation (1, 2, 3) until fully confirmed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxConfirmationPayload {
    pub request_id: u64,
    pub txid: String,
    /// Number of confirmations (1, 2, 3).
    pub confirmations: u32,
    /// Block height where the transaction was included.
    pub block_height: u64,
}

// ─── Helpers ────────────────────────────────────────────────────────────

/// Encode a payload type to CBOR bytes.
pub fn encode_payload<T: Serialize>(payload: &T) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    ciborium::into_writer(payload, &mut buf).context("CBOR payload encode failed")?;
    Ok(buf)
}

/// Decode a payload type from CBOR bytes.
pub fn decode_payload<T: for<'a> Deserialize<'a>>(data: &[u8]) -> Result<T> {
    ciborium::from_reader(data).context("CBOR payload decode failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_envelope_of_a_small_payload_stays_under_single_frame_budget() {
        // Regression test: a ReadReceipt (tiny inner payload — one MessageKey)
        // wrapped in TypedEnvelope crossed the 140-byte single-LoRa-frame
        // threshold purely from the OUTER envelope's own `v: Vec<u8>` field
        // using array-of-ints CBOR encoding, live-observed forcing an
        // unnecessary MC-chunked send whose chunks then failed to reassemble
        // over Reticulum (surfaced as raw `MC000...` garbage in the chat).
        let receipt = ReadReceiptPayload {
            up_to: MessageKey {
                sender_pubkey: "b550de818bb907047aad60d368668b3815ce2fcb9fc35d8040bb21c5c6217ccc"
                    .to_string(),
                sender_seq: 42,
            },
        };
        let payload = encode_payload(&receipt).unwrap();
        let envelope = TypedEnvelope::new(MeshMessageType::ReadReceipt, payload).with_seq(1);
        let wire = envelope.to_wire().unwrap();
        assert!(
            wire.len() < 140,
            "a ReadReceipt envelope should fit one LoRa frame (<140B), got {} bytes — \
             TypedEnvelope.v is bloating again",
            wire.len()
        );
        let decoded = TypedEnvelope::from_wire(&wire).unwrap();
        let decoded_receipt: ReadReceiptPayload = decode_payload(&decoded.v).unwrap();
        assert_eq!(decoded_receipt.up_to, receipt.up_to);
    }

    #[test]
    fn content_inline_bytes_use_compact_cbor_encoding() {
        // Regression test: Vec<u8> without #[serde(with = "compact_bytes")]
        // serializes as one CBOR integer per byte (~3.5x bloat, measured on
        // a real send: 4746 raw bytes -> 16638-byte wire envelope). Compact
        // encoding should stay close to the raw size, not balloon with it.
        let raw = vec![0xABu8; 4746];
        let payload = ContentInlinePayload {
            mime: "image/jpeg".to_string(),
            filename: None,
            caption: None,
            bytes: raw.clone(),
        };
        let encoded = encode_payload(&payload).unwrap();
        assert!(
            encoded.len() < raw.len() + 200,
            "expected compact encoding close to {} raw bytes, got {} wire bytes",
            raw.len(),
            encoded.len()
        );
        let decoded: ContentInlinePayload = decode_payload(&encoded).unwrap();
        assert_eq!(decoded.bytes, raw);
    }

    #[test]
    fn test_typed_envelope_wire_roundtrip() {
        let envelope = TypedEnvelope::new(MeshMessageType::Text, b"hello mesh".to_vec());
        let wire = envelope.to_wire().unwrap();
        assert_eq!(wire[0], TYPED_MESSAGE_MARKER);

        let decoded = TypedEnvelope::from_wire(&wire).unwrap();
        assert_eq!(decoded.t, MeshMessageType::Text as u8);
        assert_eq!(decoded.v, b"hello mesh");
    }

    #[test]
    fn test_signed_envelope() {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;

        let key = SigningKey::generate(&mut OsRng);
        let envelope = TypedEnvelope::new_signed(
            MeshMessageType::Alert,
            b"emergency broadcast".to_vec(),
            &key,
        );

        assert!(envelope.sig.is_some());
        assert!(envelope.verify_signature(&key.verifying_key()).unwrap());
    }

    #[test]
    fn test_tampered_signature_fails() {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;

        let key = SigningKey::generate(&mut OsRng);
        let mut envelope =
            TypedEnvelope::new_signed(MeshMessageType::Alert, b"test".to_vec(), &key);
        // Tamper with payload
        envelope.v = b"tampered".to_vec();
        assert!(envelope.verify_signature(&key.verifying_key()).is_err());
    }

    #[test]
    fn test_v2_seq_bound_signature() {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;

        let key = SigningKey::generate(&mut OsRng);
        let envelope =
            TypedEnvelope::new(MeshMessageType::Alert, b"test".to_vec()).signed_with_seq(42, &key);
        assert!(envelope.verify_signature(&key.verifying_key()).unwrap());

        // v2 binds seq: replaying the signed envelope under a different
        // sequence number must fail verification.
        let mut replayed = envelope.clone();
        replayed.seq = 43;
        assert!(replayed.verify_signature(&key.verifying_key()).is_err());
    }

    #[test]
    fn test_v1_signature_survives_seq_set_after_signing() {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;

        // Mixed-fleet compatibility: current senders sign first (v1
        // preimage, no seq) and allocate seq afterwards; verify must still
        // accept that.
        let key = SigningKey::generate(&mut OsRng);
        let envelope =
            TypedEnvelope::new_signed(MeshMessageType::Alert, b"test".to_vec(), &key).with_seq(7);
        assert!(envelope.verify_signature(&key.verifying_key()).unwrap());
    }

    #[test]
    fn test_invoice_payload_roundtrip() {
        let invoice = InvoicePayload {
            bolt11: "lnbc500n1pjtest...".to_string(),
            amount_sats: 50000,
            memo: Some("Pizza money".to_string()),
            payment_hash: None,
        };
        let encoded = encode_payload(&invoice).unwrap();
        let decoded: InvoicePayload = decode_payload(&encoded).unwrap();
        assert_eq!(decoded.amount_sats, 50000);
        assert_eq!(decoded.memo, Some("Pizza money".to_string()));
    }

    #[test]
    fn test_coordinate_microdegrees() {
        let coord = Coordinate::from_degrees(51.5074, -0.1278, Some("London".to_string()));
        assert_eq!(coord.lat, 51507400);
        assert_eq!(coord.lng, -127800);
        assert!((coord.lat_degrees() - 51.5074).abs() < 0.001);
        assert!((coord.lng_degrees() - (-0.1278)).abs() < 0.001);
    }

    #[test]
    fn test_alert_payload_roundtrip() {
        let alert = AlertPayload {
            alert_type: AlertType::DeadMan,
            message: "Node unresponsive for 6h".to_string(),
            coordinate: Some(Coordinate::from_degrees(
                30.2672,
                -97.7431,
                Some("Austin".to_string()),
            )),
        };
        let encoded = encode_payload(&alert).unwrap();
        let decoded: AlertPayload = decode_payload(&encoded).unwrap();
        assert_eq!(decoded.alert_type, AlertType::DeadMan);
        assert!(decoded.coordinate.is_some());
    }

    #[test]
    fn test_is_typed() {
        assert!(TypedEnvelope::is_typed(&[0x02, 0x01]));
        assert!(!TypedEnvelope::is_typed(&[0x00, 0x01]));
        assert!(!TypedEnvelope::is_typed(&[]));
    }

    #[test]
    fn test_message_type_label() {
        assert_eq!(MeshMessageType::Invoice.label(), "invoice");
        assert_eq!(MeshMessageType::Alert.label(), "alert");
    }

    #[test]
    fn test_block_header_payload() {
        let header = BlockHeaderPayload {
            height: 890412,
            hash: "00000000000000000001abc".to_string(),
            prev_hash: "00000000000000000001aab".to_string(),
            timestamp: 1710633600,
            announced_by: "did:key:z6MkTest".to_string(),
        };
        let encoded = encode_payload(&header).unwrap();
        let decoded: BlockHeaderPayload = decode_payload(&encoded).unwrap();
        assert_eq!(decoded.height, 890412);
    }

    const LN_PUBKEY: &str = "03a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";

    #[test]
    fn lightning_info_round_trips_through_u8_label_and_back() {
        assert_eq!(
            MeshMessageType::from_u8(26),
            Some(MeshMessageType::LightningInfo)
        );
        assert_eq!(MeshMessageType::LightningInfo.label(), "lightning_info");
        assert_eq!(
            MeshMessageType::from_label("lightning_info"),
            Some(MeshMessageType::LightningInfo)
        );
        // The discriminant is additive: 26 was unused before, so a deployed peer
        // that predates this simply fails to decode it rather than mis-decoding
        // it as some other type.
        assert_eq!(MeshMessageType::LightningInfo as u8, 26);
    }

    #[test]
    fn lightning_info_payload_round_trips_over_the_wire() {
        let payload = LightningInfoPayload {
            uri: format!("{LN_PUBKEY}@1.2.3.4:9735"),
            alias: Some("archy".into()),
        };
        let encoded = encode_payload(&payload).unwrap();
        let decoded: LightningInfoPayload = decode_payload(&encoded).unwrap();
        assert_eq!(decoded.uri, payload.uri);
        assert_eq!(decoded.alias.as_deref(), Some("archy"));
    }

    #[test]
    fn lightning_info_payload_alias_is_optional_and_omitted_on_the_wire() {
        let payload = LightningInfoPayload {
            uri: format!("{LN_PUBKEY}@1.2.3.4:9735"),
            alias: None,
        };
        let encoded = encode_payload(&payload).unwrap();
        let decoded: LightningInfoPayload = decode_payload(&encoded).unwrap();
        assert!(decoded.alias.is_none());
        // skip_serializing_if keeps the absent alias off the air entirely.
        let with_alias = encode_payload(&LightningInfoPayload {
            uri: format!("{LN_PUBKEY}@1.2.3.4:9735"),
            alias: Some("archy".into()),
        })
        .unwrap();
        assert!(
            encoded.len() < with_alias.len(),
            "an absent alias must cost fewer bytes on air, not the same"
        );
    }

    #[test]
    fn valid_lightning_uris_are_accepted() {
        assert!(is_valid_lightning_uri(&format!("{LN_PUBKEY}@1.2.3.4:9735")));
        assert!(is_valid_lightning_uri(&format!(
            "{LN_PUBKEY}@example.com:9735"
        )));
        // The :port suffix is optional.
        assert!(is_valid_lightning_uri(&format!("{LN_PUBKEY}@1.2.3.4")));
        assert!(is_valid_lightning_uri(&format!(
            "{LN_PUBKEY}@abcdefghij.onion:9735"
        )));
    }

    #[test]
    fn malformed_lightning_uris_are_rejected() {
        let bad = [
            "".to_string(),
            "no-at-sign".to_string(),
            LN_PUBKEY.to_string(),                         // no host at all
            format!("{LN_PUBKEY}@"),                       // empty host
            "@1.2.3.4:9735".to_string(),                   // no pubkey
            format!("{}@1.2.3.4:9735", &LN_PUBKEY[..65]),  // pubkey too short
            format!("{}z@1.2.3.4:9735", &LN_PUBKEY[..65]), // pubkey not hex
            format!("{LN_PUBKEY}@1.2.3.4:99999"),          // port out of u16 range
            format!("{LN_PUBKEY}@1.2.3.4:"),               // empty port
            format!("{LN_PUBKEY}@1.2.3.4:http"),           // non-numeric port
            format!("{LN_PUBKEY}@host with spaces:9735"),
            format!("{LN_PUBKEY}@a@b:9735"), // two '@'
        ];
        for uri in bad {
            assert!(
                !is_valid_lightning_uri(&uri),
                "must reject malformed URI {uri:?}"
            );
        }
    }
}
