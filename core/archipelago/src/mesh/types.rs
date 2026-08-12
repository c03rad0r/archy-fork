// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Shared types for mesh networking subsystem.

use serde::{Deserialize, Serialize};

/// Device firmware type, detected via protocol handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Meshcore,
    Meshtastic,
    /// A Reticulum (RNS/LXMF) RNode, bridged via the host-supervised
    /// `reticulum-daemon` over its Unix-socket RPC — not driven in-process
    /// like the other two. See `mesh/reticulum.rs`.
    Reticulum,
    Unknown,
}

impl std::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Meshcore => write!(f, "meshcore"),
            Self::Meshtastic => write!(f, "meshtastic"),
            Self::Reticulum => write!(f, "reticulum"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Optional plain-TCP Reticulum interface, radio-less alternative to the
/// serial-RNode path. Dev/verification surface for now (e.g. proving
/// interop with Aurora's `RnsTcpInterface`/`RnsTcpServerInterface`, which is
/// its default connectivity mode) — not exposed through `mesh.configure`/the
/// frontend. `Server` is hard-gated to loopback in both the daemon and Rust
/// (`reticulum::is_loopback_host`); WAN/LAN exposure is a separate, deliberate
/// future decision (archy is otherwise Tor-first for inter-node traffic).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ReticulumTcpConfig {
    /// Bind a `TCPServerInterface`. `bind` is `host:port`, host must be
    /// loopback (127.0.0.1/::1/localhost).
    Server { bind: String },
    /// Dial one or more `TCPClientInterface` targets (`host:port`).
    Client { connect: Vec<String> },
}

/// The per-message transport pill label for a radio-delivered message: the
/// active device's own name, since one session owns exactly one device.
/// Federation sends/receives are labelled "fips"/"tor" elsewhere — this only
/// covers the radio-class transports.
pub fn radio_transport_label(device_type: DeviceType) -> &'static str {
    match device_type {
        DeviceType::Meshcore => "meshcore",
        DeviceType::Meshtastic => "meshtastic",
        DeviceType::Reticulum => "reticulum",
        DeviceType::Unknown => "lora",
    }
}

/// A peer discovered via mesh radio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshPeer {
    /// Meshcore contact ID (uint32).
    pub contact_id: u32,
    /// Advertised name on the mesh network.
    pub advert_name: String,
    /// Archipelago DID (did:key:z...) if identity was received.
    pub did: Option<String>,
    /// Routing key hex. For a radio (meshcore) peer this is the firmware
    /// contact public key used to address outbound DMs; for a federation-
    /// seeded peer it is the archipelago ed25519 key. Used for delivery, NOT
    /// for authentication — see `arch_pubkey_hex`.
    pub pubkey_hex: Option<String>,
    /// Verified archipelago ed25519 identity key hex, bound from a signed
    /// identity advert (`handle_identity_received`) or federation seeding.
    /// Unlike `pubkey_hex`, this is NEVER overwritten by `refresh_contacts`
    /// with the firmware routing key, so it stays stable for the `!ai` auth
    /// gate, envelope signature verification, and federation-trust matching.
    #[serde(default)]
    pub arch_pubkey_hex: Option<String>,
    /// X25519 public key (32 bytes) for key agreement.
    #[serde(skip)]
    pub x25519_pubkey: Option<[u8; 32]>,
    /// Last received signal strength (dBm).
    pub rssi: Option<i16>,
    /// Signal-to-noise ratio.
    pub snr: Option<f32>,
    /// When we last heard from this peer.
    pub last_heard: String,
    /// Number of hops to reach this peer.
    pub hops: u8,
    /// Firmware advert timestamp (unix secs) of the contact's last advert, or
    /// 0 if unknown. Used to gauge reachability/recency in the UI.
    #[serde(default)]
    pub last_advert: u32,
    /// Best-effort "currently reachable" flag: the radio has a route to this
    /// contact (or it's a federation/identity peer reachable off-radio). A
    /// contact with no path and no recent advert is shown as unreachable.
    #[serde(default)]
    pub reachable: bool,
    /// Whether DMs to/from this peer are end-to-end (PKI / Curve25519) encrypted.
    /// Set for a Meshtastic peer once we know its real NodeInfo public key (the
    /// firmware then PKC-encrypts directed DMs), so the send path can show the
    /// E2E pill on a Sent DM to a PKC-capable stock peer, not only archy peers.
    #[serde(default)]
    pub pkc_capable: bool,
    /// Last known position (degrees), from a Meshtastic `POSITION_APP`
    /// broadcast. `None` until the peer has shared one.
    #[serde(default)]
    pub lat: Option<f64>,
    #[serde(default)]
    pub lon: Option<f64>,
    /// This peer's advertised Lightning connection URI (`pubkey@host:port`).
    ///
    /// Set ONLY from a received `LightningInfo` advertisement (or federation
    /// seeding in a later plan) — never inferred, never defaulted. `None` means
    /// "this peer has not told us it runs Lightning", which is what the
    /// channel-open picker uses to decide whether to offer it as a request
    /// target at all.
    ///
    /// The advertisement is unauthenticated RF input, so this is a *request*
    /// target the operator chooses to act on, not a trusted identity. It is
    /// stored against the peer's authenticating key (`identity_pubkey_hex()`),
    /// never the firmware routing key (T-01-11).
    #[serde(default)]
    pub lightning_uri: Option<String>,
}

impl MeshPeer {
    /// The key to use when AUTHENTICATING this peer (`!ai` trust/allowlist,
    /// envelope signature verification): the verified archipelago identity key
    /// if one is bound, otherwise the routing key. Never use the firmware
    /// routing key for auth when an archipelago identity is known — a radio
    /// peer's firmware key won't match its `nodes.json` archipelago key.
    pub fn identity_pubkey_hex(&self) -> Option<&str> {
        self.arch_pubkey_hex
            .as_deref()
            .or(self.pubkey_hex.as_deref())
    }
}

/// Direction of a mesh message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageDirection {
    Sent,
    Received,
}

/// A mesh message (sent or received).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshMessage {
    pub id: u64,
    pub direction: MessageDirection,
    /// Meshcore contact ID of the peer.
    pub peer_contact_id: u32,
    /// Peer name (for display).
    pub peer_name: Option<String>,
    /// Human-readable rendering — for text messages this is the raw text,
    /// for typed messages a short summary used as a fallback in lists.
    pub plaintext: String,
    pub timestamp: String,
    /// Whether delivery was confirmed via ACK.
    pub delivered: bool,
    /// Whether the message was end-to-end encrypted.
    pub encrypted: bool,
    /// How this message actually traveled, for the per-message transport pill:
    /// "lora" (mesh radio), "fips", or "tor". `None` until known (a Sent
    /// federation message is finalized once the background send resolves the
    /// transport). Surfaced in the UI beside the E2E badge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    /// Typed-envelope label ("text", "invoice", "alert", "coordinate", ...).
    #[serde(default = "default_message_type")]
    pub message_type: String,
    /// Structured payload as JSON — populated for non-text typed messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typed_payload: Option<serde_json::Value>,
    /// Hex-encoded sender pubkey. On Received: the peer's mesh public key
    /// (or first 6 bytes / full key as available). On Sent: None today,
    /// populated later when we own the key. Combined with `sender_seq`
    /// this forms a stable cross-transport MessageKey that reactions,
    /// replies, edits, and read-receipts can reference without relying
    /// on the local `id` field (which is only meaningful to one node).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_pubkey: Option<String>,
    /// Per-sender monotonic sequence from the typed envelope. Paired with
    /// `sender_pubkey` to form the stable MessageKey.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_seq: Option<u64>,
}

fn default_message_type() -> String {
    "text".to_string()
}

/// Overall mesh subsystem status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshStatus {
    pub enabled: bool,
    pub device_type: DeviceType,
    pub device_path: Option<String>,
    pub device_connected: bool,
    pub firmware_version: Option<String>,
    pub self_node_id: Option<u32>,
    pub self_advert_name: Option<String>,
    pub peer_count: usize,
    pub channel_name: String,
    pub messages_sent: u64,
    pub messages_received: u64,
    /// Operator-configured LoRa region (e.g. "EU_868"), for the Device tab
    /// (#8) — composed in by `MeshService::status()` from `MeshConfig`, not
    /// part of the live session state itself.
    #[serde(default)]
    pub region: Option<String>,
}

/// Information returned from device during initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub firmware_version: String,
    pub node_id: u32,
    pub max_contacts: u16,
    pub device_type: DeviceType,
}

/// Events emitted by the mesh listener for other components to consume.
#[derive(Debug, Clone)]
pub enum MeshEvent {
    DeviceConnected(DeviceInfo),
    DeviceDisconnected,
    PeerDiscovered(MeshPeer),
    PeerUpdated(MeshPeer),
    MessageReceived(MeshMessage),
    MessageDelivered {
        message_id: u64,
    },
    IdentityReceived {
        contact_id: u32,
        did: String,
        pubkey_hex: String,
        x25519_pubkey: [u8; 32],
    },
    /// Block header received from an internet-connected mesh peer.
    BlockHeaderReceived {
        height: u64,
        hash: String,
    },
    /// Emergency or dead-man alert received from a peer.
    AlertReceived {
        alert_type: String,
        message: String,
        from_contact_id: u32,
    },
    /// TX relay completed (response received from internet peer).
    TxRelayCompleted {
        request_id: u64,
        txid: Option<String>,
        error: Option<String>,
    },
    /// Lightning relay completed (response received from internet peer).
    LightningRelayCompleted {
        request_id: u64,
        payment_hash: Option<String>,
        error: Option<String>,
    },
    /// An AI query arrived from a peer and was accepted for answering (#50).
    AssistQueryReceived {
        from_contact_id: u32,
        prompt: String,
    },
    /// A local-AI answer finished sending back to the asker (or failed) (#50).
    AssistResponseReady {
        req_id: u64,
        to_contact_id: u32,
        error: Option<String>,
    },
    /// A local-AI answer to a `!ai`-in-chat query, to be delivered back into
    /// the 1:1 thread via the transport-aware `MeshService::send_message`
    /// (Tor for federation peers, LoRa for radio peers). The mesh listener
    /// emits this because it can't route over federation itself — the signing
    /// key and Tor client live on MeshService. Consumed at the server layer.
    AssistChatReply {
        contact_id: u32,
        text: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(arch: Option<&str>, routing: Option<&str>) -> MeshPeer {
        MeshPeer {
            contact_id: 1,
            advert_name: "Test".into(),
            did: None,
            pubkey_hex: routing.map(|s| s.to_string()),
            arch_pubkey_hex: arch.map(|s| s.to_string()),
            x25519_pubkey: None,
            rssi: None,
            snr: None,
            last_heard: String::new(),
            hops: 0,
            last_advert: 0,
            reachable: false,
            pkc_capable: false,
            lat: None,
            lon: None,
            lightning_uri: None,
        }
    }

    #[test]
    fn identity_prefers_bound_archipelago_key_over_firmware_routing_key() {
        // A radio peer that sent an identity advert: routing key is the firmware
        // contact key, but auth must use the bound archipelago key.
        let p = peer(Some("archkey"), Some("firmwarekey"));
        assert_eq!(p.identity_pubkey_hex(), Some("archkey"));
    }

    #[test]
    fn identity_falls_back_to_routing_key_when_no_advert() {
        // A plain peer with no archipelago identity bound: fall back to whatever
        // key we have (federation peers carry the arch key in pubkey_hex).
        let p = peer(None, Some("firmwarekey"));
        assert_eq!(p.identity_pubkey_hex(), Some("firmwarekey"));
        assert_eq!(peer(None, None).identity_pubkey_hex(), None);
    }

    #[test]
    fn refresh_style_routing_update_does_not_change_identity() {
        // Simulates refresh_contacts: pubkey_hex (routing) is rewritten to a new
        // firmware key while arch_pubkey_hex (identity) is preserved.
        let mut p = peer(Some("archkey"), Some("firmware-old"));
        p.pubkey_hex = Some("firmware-new".into());
        assert_eq!(p.identity_pubkey_hex(), Some("archkey"));
    }
}
