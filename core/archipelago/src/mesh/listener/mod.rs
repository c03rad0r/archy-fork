//! Background mesh listener task.
//!
//! Runs as a long-lived tokio task that:
//! - Maintains the serial connection to the Meshcore device
//! - Reads incoming frames and dispatches events
//! - Periodically broadcasts our identity advertisement
//! - Reconnects on device disconnect
//! - Manages peer cache and message store

mod assist;
mod bitcoin;
mod decode;
pub(crate) mod dispatch;
mod frames;
mod node_cmd;
mod session;

pub(crate) use session::{probe_device, DeviceProbe};

use super::types::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{error, info, warn};

/// How often to broadcast our identity advertisement (seconds).
const ADVERT_INTERVAL: Duration = Duration::from_secs(60);

/// How often to poll for queued messages when no push notifications.
const SYNC_INTERVAL: Duration = Duration::from_secs(10);

/// Backlog #12 (provisioning robustness): if we haven't successfully received
/// ANY frame in this long, treat the serial link as stalled and force a
/// reconnect — the write-side `consecutive_write_failures` counter is blind
/// to a receive-only stall (writes can keep succeeding while the radio's
/// stopped streaming inbound, e.g. the FROM_RADIO_REBOOTED-without-recovery
/// case meshtastic.rs already has a targeted, immediate fix for — this
/// watchdog is just the backstop for a device that goes silent WITHOUT
/// emitting that notification).
///
/// 5 minutes was originally chosen on the (wrong) assumption that the 60s
/// advert / 10s sync cadence implies *received* traffic — those are our own
/// OUTBOUND cadences and say nothing about what peers send us. A quiet mesh
/// (no peer transmitting, or Reticulum/LXMF's point-to-point store-and-
/// forward model with no broadcast echo) can be legitimately RX-silent for
/// long stretches with the link perfectly healthy; at 300s this forced a
/// full auto-detect reconnect (visible in the UI as "Connecting…") every
/// ~5 minutes on otherwise-idle nodes. 30 minutes still catches a wedged
/// device in reasonable time without false-triggering on normal mesh quiet.
const RX_STALL_TIMEOUT: Duration = Duration::from_secs(1800);

/// Maximum stored messages (circular buffer).
const MAX_MESSAGES: usize = 100;

/// On-disk message-history file under `data_dir/` (written by
/// `spawn_message_persister`, restored by `load_persisted_messages`).
const MESSAGES_FILE: &str = "mesh-messages.json";

/// Serialized form of the persisted history. `send_seqs` rides along because
/// the per-target outbound sequence counters must survive restarts too:
/// receivers dedup on (sender_pubkey, sender_seq), so a node that reboots and
/// starts counting from 1 again has its first messages silently dropped by
/// every peer that already saw those sequence numbers.
#[derive(serde::Serialize, serde::Deserialize)]
struct PersistedMessages {
    messages: Vec<MeshMessage>,
    #[serde(default)]
    send_seqs: HashMap<u32, u64>,
}

/// Check if two ISO8601 timestamps are within N seconds of each other.
fn within_seconds_iso(ts1: &str, ts2: &str, secs: i64) -> bool {
    use chrono::DateTime;
    let a = DateTime::parse_from_rfc3339(ts1).ok();
    let b = DateTime::parse_from_rfc3339(ts2).ok();
    match (a, b) {
        (Some(a), Some(b)) => (a - b).num_seconds().unsigned_abs() < secs as u64,
        _ => false,
    }
}

/// Initial delay before reconnection attempt after device disconnect.
const RECONNECT_DELAY_INIT: Duration = Duration::from_secs(5);

/// Maximum reconnect delay (cap for exponential backoff).
const RECONNECT_DELAY_MAX: Duration = Duration::from_secs(60);

/// Minimum time a session must run before we trust it enough to reset
/// backoff to the minimum. Without this gate, a device that connects then
/// fails again within a couple of seconds (e.g. mid-boot-loop) never backs
/// off — every retry immediately re-opens the port, which toggles DTR/RTS
/// (resets many ESP32 boards' MCU on native-USB and CP2102/CH340
/// auto-reset-circuit boards alike), turning a device that's merely
/// unstable into a self-sustaining boot loop that outlasts whatever
/// triggered the original instability. Confirmed live 2026-07-23: a Heltec
/// V3 stuck retrying every ~5-15s for 5+ minutes after a failed firmware
/// flash left it in a marginal state.
const STABLE_SESSION_THRESHOLD: Duration = Duration::from_secs(20);

/// Number of consecutive write failures before we consider the device dead
/// and trigger a reconnection cycle.
const MAX_CONSECUTIVE_WRITE_FAILURES: u32 = 3;

/// Command sent from MeshService to the listener task (which owns the serial port).
pub enum MeshCommand {
    SendText {
        dest_pubkey_prefix: [u8; 6],
        payload: Vec<u8>,
    },
    /// Send pre-encoded binary (TypedEnvelope wire bytes) to a peer.
    SendRaw {
        dest_pubkey_prefix: [u8; 6],
        payload: Vec<u8>,
    },
    /// Send pre-encoded binary over a dedicated Reticulum RNS Resource
    /// transfer instead of the small inline-chunk path — Reticulum-only, see
    /// `MeshRadioDevice::send_resource`. Used for large attachments
    /// (compressed photos, voice messages) that exceed the small-message cap
    /// but fit a sane LoRa-Resource budget; routing decision is made by the
    /// RPC layer (`mesh.transport-advice`'s `"resource-mesh"` tier).
    SendResource {
        dest_pubkey_prefix: [u8; 6],
        payload: Vec<u8>,
    },
    /// Native LXMF `FIELD_IMAGE` send — Reticulum-only, for a stock
    /// (non-archy) peer that can't decode our typed envelope. See
    /// `MeshRadioDevice::send_native_image`.
    SendNativeImage {
        dest_pubkey_prefix: [u8; 6],
        mime: String,
        bytes: Vec<u8>,
        caption: Option<String>,
    },
    /// Send PLAIN text as one or more native meshcore DMs to a stock client
    /// (e.g. a phone). Long text is split into multiple readable plain messages
    /// — never MC-chunked — because stock clients can't reassemble archy's
    /// chunk framing. Used for chat/AI replies to non-archipelago contacts.
    SendNativeText {
        dest_pubkey_prefix: [u8; 6],
        payload: Vec<u8>,
    },
    /// Broadcast pre-encoded binary on a mesh channel.
    BroadcastChannel {
        channel: u8,
        payload: Vec<u8>,
    },
    SendAdvert,
    /// Reboot the locally-connected radio firmware to recover a wedged /
    /// RX-deaf radio. Meshtastic: firmware reboot command. Reticulum: the
    /// sidecar daemon is restarted (radio re-detected + reconfigured).
    /// MeshCore: unsupported, and says so. `reply` (when present) carries
    /// the real outcome to the RPC caller — the buttons used to be
    /// fire-and-forget `warn!`s, i.e. no feedback ever reached the UI
    /// (operator, 2026-08-06).
    RebootRadio {
        seconds: i64,
        reply: Option<tokio::sync::oneshot::Sender<Result<String, String>>>,
    },
    /// Query the live RNode radio state (Reticulum-only): the sidecar's
    /// radio-confirmed parameters, for the LoRa settings panel's current
    /// values + apply read-back.
    QueryRadioState {
        reply: tokio::sync::oneshot::Sender<Result<serde_json::Value, String>>,
    },
    /// Re-fetch contact list from the radio device.
    RefreshContacts,
    /// Delete a contact from the firmware table (clear-all / unreachable wipe).
    RemoveContact {
        pubkey: [u8; 32],
    },
    /// Import/add a heard advert as a firmware contact so it shows up without
    /// needing a flood advert. Name may be empty (firmware fills from advert).
    AddContact {
        pubkey: [u8; 32],
        name: String,
    },
}

/// Shared state for the mesh listener, accessible from RPC handlers.
pub struct MeshState {
    pub peers: RwLock<HashMap<u32, MeshPeer>>,
    pub messages: RwLock<VecDeque<MeshMessage>>,
    pub shared_secrets: RwLock<HashMap<u32, [u8; 32]>>,
    pub status: RwLock<MeshStatus>,
    pub event_tx: broadcast::Sender<MeshEvent>,
    /// Command channel sender. Wrapped in RwLock so `MeshService::stop()`
    /// can swap it for a fresh channel when the listener task drains the
    /// old receiver — without this, a disable→enable cycle fails with
    /// "Command channel already consumed" on the second start().
    pub cmd_tx: RwLock<mpsc::Sender<MeshCommand>>,
    next_message_id: RwLock<u64>,
    /// Per-contact outbound sequence counter. Increments on every typed
    /// envelope we send to a given peer so the receiver (and anyone else
    /// forwarding/reacting) can build a stable MessageKey = (our_pubkey, seq).
    /// Channel broadcasts use contact_id = 0 as a shared counter.
    next_send_seq: RwLock<HashMap<u32, u64>>,
    /// Block header cache — populated when receiving headers from internet-connected peers.
    pub block_header_cache: Arc<super::bitcoin_relay::BlockHeaderCache>,
    /// Relay tracker — stores completed relay results for frontend polling.
    pub relay_tracker: Option<Arc<super::bitcoin_relay::RelayTracker>>,
    /// Steganography mode for outgoing/incoming messages.
    pub stego_mode: super::steganography::SteganographyMode,
    /// Chunk reassembly buffer for multi-frame messages.
    pub(crate) chunk_buffer: RwLock<HashMap<(u32, u8), ChunkAssembly>>,
    /// Double Ratchet session manager for forward-secret encryption.
    pub session_manager: Arc<super::session::SessionManager>,
    /// Whether to encrypt directed relay messages (config toggle for rollback).
    pub encrypt_relay: bool,
    /// Whether to accept inbound Bitcoin block headers from peers (issue #28).
    pub receive_block_headers: bool,
    /// Last-seen presence heartbeats per peer pubkey hex: (status, last_active_epoch, received_at).
    pub presence: RwLock<HashMap<String, (String, u32, u64)>>,
    /// Contacts store — alias/notes/pinned/blocked per peer pubkey hex.
    pub contacts: RwLock<HashMap<String, ContactEntry>>,
    /// Our archipelago ed25519 public key (hex). Used by outbound DM-via-channel
    /// to embed a sender prefix on the wire so receivers can attribute inbound
    /// messages to the correct contact_id even when multiple `Archy-*` peers
    /// share the LoRa channel.
    pub our_ed_pubkey_hex: String,
    /// Shared blob store for writing received inline attachments. Populated
    /// by `RpcHandler` after startup so the mesh listener can persist inline
    /// file bytes into the same store the HTTP layer serves.
    pub blob_store: RwLock<Option<Arc<crate::blobs::BlobStore>>>,
    /// The assistant's shared tool loop, reached through the same RpcHandler
    /// every caller dispatches on. Populated by the server after startup
    /// (same pattern as blob_store). `None` on early boot → the legacy
    /// bare-LLM path in assist.rs answers instead.
    pub assistant_handler: RwLock<Option<Arc<crate::api::rpc::RpcHandler>>>,
    /// Firmware-pubkey-hex of radio contacts the user has chosen to ignore
    /// (via mesh.clear-all). `refresh_contacts` skips any device contact
    /// whose pubkey is in this set, preventing the meshcore firmware's
    /// persistent contact table from regenerating rows the user just
    /// wiped. Persisted to `mesh-ignored-radio-contacts.json`.
    pub radio_contact_blocklist: RwLock<HashSet<String>>,
    /// Mesh-AI assistant settings (issue #50): whether this node answers
    /// AssistQuery messages with its local LLM, and who may ask. Live-updatable
    /// so the UI toggle applies without restarting the listener.
    pub assistant: RwLock<AssistantConfig>,
    /// Data dir — lets dispatch handlers reach disk-backed stores (e.g. the
    /// federation trust list used to gate AI queries) without threading a path
    /// through every call.
    pub data_dir: std::path::PathBuf,
    /// Contact-ids with an AI query currently being answered. Caps each asker to
    /// one in-flight query so a peer can't flood the node's compute / airtime.
    pub assist_inflight: RwLock<HashSet<u32>>,
    /// Recently-denied `!ai` askers (newest first, capped). When `trusted_only`
    /// rejects a sender — typically a radio (meshcore) device that presents a
    /// firmware key rather than an archipelago DID — we record who tried so the
    /// UI can surface them and let the operator one-click allow their key.
    /// Silent on the wire (no airtime spent), visible to the operator here.
    pub assist_denied: RwLock<VecDeque<DeniedAsker>>,
}

/// A `!ai` asker that the assistant policy turned away. Surfaced to the UI so
/// the operator can add their key to the allowlist without hunting the journal.
#[derive(Debug, Clone, Serialize)]
pub struct DeniedAsker {
    /// Meshcore contact id of the asker.
    pub contact_id: u32,
    /// Best-known display name (advert name) at denial time.
    pub name: String,
    /// The asker's ed25519 pubkey hex, if known. `None` for a raw radio device
    /// that hasn't advertised an archipelago key — such a sender can only be
    /// admitted by switching the policy to "anyone", not via the allowlist.
    pub pubkey_hex: Option<String>,
    /// ISO-8601 timestamp of the (most recent) denial.
    pub at: String,
}

/// Mesh-AI assistant configuration, snapshotted from `MeshConfig` at startup.
#[derive(Debug, Clone)]
pub struct AssistantConfig {
    /// Answer AssistQuery messages with the local LLM.
    pub enabled: bool,
    /// Model to use; None → the backend's built-in default.
    pub model: Option<String>,
    /// Restrict asking to federation-Trusted peers (vs. anyone on the mesh).
    pub trusted_only: bool,
    /// AI backend: "claude" (shared proxy token) or "ollama" (local model).
    pub backend: String,
    /// Per-contact allowlist (ed25519 pubkey hex) permitted to use `!ai`
    /// regardless of `trusted_only`. Empty → only the `trusted_only` policy
    /// applies. A user-blocked contact is always denied even if listed here.
    pub allowed_contacts: Vec<String>,
}

/// Contact metadata kept alongside MeshState.peers. Pinned contacts sort to
/// the top of the chat list, blocked ones are filtered out of notifications.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContactEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub blocked: bool,
}

/// In-progress chunk reassembly for a multi-frame message.
pub(crate) struct ChunkAssembly {
    chunks: HashMap<u8, String>,
    total: u8,
    created: std::time::Instant,
}

impl MeshState {
    pub fn new(
        channel_name: &str,
        block_header_cache: Arc<super::bitcoin_relay::BlockHeaderCache>,
        relay_tracker: Option<Arc<super::bitcoin_relay::RelayTracker>>,
        stego_mode: super::steganography::SteganographyMode,
        encrypt_relay: bool,
        receive_block_headers: bool,
        session_manager: Arc<super::session::SessionManager>,
        our_ed_pubkey_hex: String,
        assistant: AssistantConfig,
        data_dir: std::path::PathBuf,
    ) -> (
        Arc<Self>,
        broadcast::Receiver<MeshEvent>,
        mpsc::Receiver<MeshCommand>,
    ) {
        let (tx, rx) = broadcast::channel(64);
        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        let state = Arc::new(Self {
            peers: RwLock::new(HashMap::new()),
            messages: RwLock::new(VecDeque::new()),
            shared_secrets: RwLock::new(HashMap::new()),
            cmd_tx: RwLock::new(cmd_tx),
            status: RwLock::new(MeshStatus {
                enabled: true,
                device_type: DeviceType::Unknown,
                device_path: None,
                device_connected: false,
                firmware_version: None,
                self_node_id: None,
                self_advert_name: None,
                peer_count: 0,
                channel_name: channel_name.to_string(),
                messages_sent: 0,
                messages_received: 0,
                region: None,
            }),
            event_tx: tx,
            next_message_id: RwLock::new(1),
            next_send_seq: RwLock::new(HashMap::new()),
            block_header_cache,
            relay_tracker,
            stego_mode,
            chunk_buffer: RwLock::new(HashMap::new()),
            session_manager,
            encrypt_relay,
            receive_block_headers,
            presence: RwLock::new(HashMap::new()),
            contacts: RwLock::new(HashMap::new()),
            our_ed_pubkey_hex,
            blob_store: RwLock::new(None),
            assistant_handler: RwLock::new(None),
            radio_contact_blocklist: RwLock::new(HashSet::new()),
            assistant: RwLock::new(assistant),
            data_dir,
            assist_inflight: RwLock::new(HashSet::new()),
            assist_denied: RwLock::new(VecDeque::new()),
        });
        (state, rx, cmd_rx)
    }

    /// Send a command to the listener. Reads the current sender from the
    /// RwLock and clones for the async send. Returns the mpsc SendError so
    /// callers can treat a dead listener as "mesh not running".
    pub async fn send_cmd(
        &self,
        cmd: MeshCommand,
    ) -> Result<(), mpsc::error::SendError<MeshCommand>> {
        let tx = self.cmd_tx.read().await.clone();
        tx.send(cmd).await
    }

    pub async fn next_id(&self) -> u64 {
        let mut id = self.next_message_id.write().await;
        let current = *id;
        *id += 1;
        current
    }

    /// Allocate the next outbound sequence number for a given target
    /// (contact_id for direct peers, 0 for channel broadcasts). Monotonic
    /// per target, starts at 1.
    pub async fn next_send_seq(&self, target: u32) -> u64 {
        let mut map = self.next_send_seq.write().await;
        let slot = map.entry(target).or_insert(0);
        *slot += 1;
        *slot
    }

    pub async fn store_message(&self, msg: MeshMessage) {
        let mut messages = self.messages.write().await;
        // Deduplicate RECEIVED messages only — a Sent record is the user's
        // own action and must ALWAYS be shown, even when the display text
        // collides with an earlier one (e.g. two 👍 reactions to different
        // targets, or "ok" reply twice in a row).
        //
        // Dedup runs THREE checks, any match drops the incoming message:
        //   (a) (sender_pubkey, sender_seq) — exact MessageKey match
        //   (b) (sender_seq, plaintext, 120s) — cross-transport match when
        //       the same envelope arrives via radio and federation: radio
        //       populates sender_pubkey from the firmware key, federation
        //       populates it from the archipelago ed25519 key, so (a) misses
        //       but the seq+text still uniquely identifies the envelope
        //   (c) (peer_contact_id, plaintext, 30s) — legacy plain-text frames
        //       without a sender_seq at all
        if matches!(msg.direction, MessageDirection::Received) {
            let has_seq = msg.sender_seq.is_some();
            let key_match = has_seq
                && msg.sender_pubkey.is_some()
                && messages.iter().rev().take(40).any(|m| {
                    matches!(m.direction, MessageDirection::Received)
                        && m.sender_pubkey == msg.sender_pubkey
                        && m.sender_seq == msg.sender_seq
                });
            let cross_transport_match = has_seq
                && messages.iter().rev().take(40).any(|m| {
                    matches!(m.direction, MessageDirection::Received)
                        && m.sender_seq == msg.sender_seq
                        && m.plaintext == msg.plaintext
                        && within_seconds_iso(&m.timestamp, &msg.timestamp, 120)
                });
            let legacy_match = !has_seq
                && messages.iter().rev().take(20).any(|m| {
                    matches!(m.direction, MessageDirection::Received)
                        && m.peer_contact_id == msg.peer_contact_id
                        && m.plaintext == msg.plaintext
                        && within_seconds_iso(&m.timestamp, &msg.timestamp, 30)
                });
            if key_match || cross_transport_match || legacy_match {
                return;
            }
        }
        messages.push_back(msg);
        if messages.len() > MAX_MESSAGES {
            messages.pop_front();
        }
    }

    pub(crate) async fn update_peer_count(&self) {
        let count = self.peers.read().await.len();
        self.status.write().await.peer_count = count;
    }

    /// Restore the persisted message history + outbound sequence counters.
    /// Called once at service startup, before the listener spawns. Missing
    /// file (fresh node) is normal; a corrupt file is logged and skipped
    /// rather than blocking mesh startup.
    pub async fn load_persisted_messages(&self) {
        let path = self.data_dir.join(MESSAGES_FILE);
        let bytes = match tokio::fs::read(&path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                warn!("mesh: reading {} failed: {e}", path.display());
                return;
            }
        };
        let persisted: PersistedMessages = match serde_json::from_slice(&bytes) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "mesh: parsing {} failed (skipping restore): {e}",
                    path.display()
                );
                return;
            }
        };
        let count = persisted.messages.len();
        let max_id = persisted.messages.iter().map(|m| m.id).max().unwrap_or(0);
        *self.messages.write().await = persisted.messages.into();
        if !persisted.send_seqs.is_empty() {
            *self.next_send_seq.write().await = persisted.send_seqs;
        }
        {
            let mut id = self.next_message_id.write().await;
            if *id <= max_id {
                *id = max_id + 1;
            }
        }
        info!(
            "mesh: restored {count} persisted messages (next id {})",
            max_id + 1
        );
    }
}

/// Persist the message history on a low-rate timer instead of hooking every
/// mutation path (store, delivered/transport/encrypted stamps, edits, deletes,
/// read-receipt prunes — and whatever gets added next). Serializing ≤100
/// messages every few seconds is trivial; the write is skipped when nothing
/// changed, and at most the last few seconds of history are lost on a hard
/// kill — versus the entire history, which is what a restart cost before this
/// existed. Atomic write-then-rename, 0600 (DM plaintext lives in this file).
pub fn spawn_message_persister(state: Arc<MeshState>) {
    tokio::spawn(async move {
        use std::os::unix::fs::PermissionsExt;
        let path = state.data_dir.join(MESSAGES_FILE);
        let tmp = path.with_extension("json.tmp");
        let mut last_written: Option<Vec<u8>> = None;
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let snapshot = PersistedMessages {
                messages: state.messages.read().await.iter().cloned().collect(),
                send_seqs: state.next_send_seq.read().await.clone(),
            };
            let json = match serde_json::to_vec_pretty(&snapshot) {
                Ok(j) => j,
                Err(e) => {
                    warn!("mesh: serializing message history failed: {e}");
                    continue;
                }
            };
            if last_written.as_deref() == Some(json.as_slice()) {
                continue;
            }
            if let Err(e) = tokio::fs::write(&tmp, &json).await {
                warn!("mesh: writing {} failed: {e}", tmp.display());
                continue;
            }
            let perms = std::fs::Permissions::from_mode(0o600);
            if let Err(e) = tokio::fs::set_permissions(&tmp, perms).await {
                warn!("mesh: chmod {} failed: {e}", tmp.display());
            }
            if let Err(e) = tokio::fs::rename(&tmp, &path).await {
                warn!(
                    "mesh: renaming {} -> {} failed: {e}",
                    tmp.display(),
                    path.display()
                );
                continue;
            }
            last_written = Some(json);
        }
    });
}

/// Spawn the background mesh listener task.
///
/// This task manages the full lifecycle:
/// 1. Detect and connect to Meshcore device
/// 2. Initialize and set advert name
/// 3. Main loop: read frames, dispatch events, periodic adverts
/// 4. Reconnect on disconnect
pub fn spawn_mesh_listener(
    state: Arc<MeshState>,
    data_dir: std::path::PathBuf,
    device_path: Option<String>,
    our_did: String,
    our_ed_pubkey_hex: String,
    our_x25519_secret: [u8; 32],
    our_x25519_pubkey_hex: String,
    server_name: Option<String>,
    lora_region: Option<String>,
    lora_radio_params: Option<super::LoraRadioParams>,
    channel_name: Option<String>,
    device_kind: Option<super::types::DeviceType>,
    reticulum_tcp: Option<super::types::ReticulumTcpConfig>,
    manage_radio: bool,
    shutdown: tokio::sync::watch::Receiver<bool>,
    cmd_rx: mpsc::Receiver<MeshCommand>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut shutdown = shutdown;
        let mut cmd_rx = cmd_rx;
        let mut reconnect_delay = RECONNECT_DELAY_INIT;
        // Mutable so a successful auto-detect can pin the firmware kind for
        // the rest of this listener's lifetime — see the pin-on-first-success
        // block below for why.
        let mut device_kind = device_kind;
        // Backlog #12 hot-swap re-binding: each run_mesh_session call already
        // builds a fresh device struct (contacts/current_region/etc. all
        // start empty), so per-device session state is naturally isolated
        // across reconnects — there's no stale in-memory state to clear here.
        // What's worth doing is detecting when the *physical radio itself*
        // changed (a genuine hot-swap, not just the same radio reconnecting)
        // so it's visible in logs rather than silently treated the same as
        // an ordinary reconnect.
        let mut last_self_node_id: Option<u32> = None;
        loop {
            if *shutdown.borrow() {
                info!("Mesh listener shutting down");
                return;
            }

            let session_start = std::time::Instant::now();
            match session::run_mesh_session(
                &state,
                &data_dir,
                device_path.as_deref(),
                &our_did,
                &our_ed_pubkey_hex,
                &our_x25519_secret,
                &our_x25519_pubkey_hex,
                server_name.as_deref(),
                lora_region.as_deref(),
                lora_radio_params,
                channel_name.as_deref(),
                device_kind,
                reticulum_tcp.clone(),
                manage_radio,
                &mut shutdown,
                &mut cmd_rx,
            )
            .await
            {
                Ok(()) => {
                    info!("Mesh session ended cleanly");
                    // Only trust a session that actually ran for a while —
                    // see STABLE_SESSION_THRESHOLD's doc comment.
                    if session_start.elapsed() >= STABLE_SESSION_THRESHOLD {
                        reconnect_delay = RECONNECT_DELAY_INIT;
                    }
                }
                Err(e) => {
                    if session_start.elapsed() >= STABLE_SESSION_THRESHOLD {
                        reconnect_delay = RECONNECT_DELAY_INIT;
                    }
                    error!("Mesh session error: {} (retry in {:?})", e, reconnect_delay);
                }
            }

            // Hot-swap detection: compare this session's self_node_id against
            // the last one we saw. A change means the physical radio itself
            // was swapped (not just a reconnect of the same board).
            {
                let current_self_node_id = state.status.read().await.self_node_id;
                if let (Some(prev), Some(cur)) = (last_self_node_id, current_self_node_id) {
                    if prev != cur {
                        info!(
                            previous_node_id = prev,
                            new_node_id = cur,
                            "Local mesh radio identity changed — treating as a hot-swapped device"
                        );
                    }
                }
                if current_self_node_id.is_some() {
                    last_self_node_id = current_self_node_id;
                }
            }

            // Pin the firmware kind after the first successful auto-detect.
            // Confirmed live 2026-07-23: with device_kind left unpinned (e.g.
            // after clearing a stale pin), EVERY reconnect re-runs the full
            // Reticulum→Meshcore→Meshtastic auto-detect cascade — each
            // candidate past the first does its own open() with the DTR/RTS
            // reset both boards need, so a device correctly identified as
            // Meshtastic still gets reset once for the failed Meshcore
            // attempt before Meshtastic's own open() resets it again. That
            // doubled the reset count on every single reconnect indefinitely,
            // not just during initial detection. Once auto-detect has
            // identified the device this listener is actually talking to,
            // there's no reason to keep guessing on subsequent reconnects —
            // pin it, both in this task's own loop (takes effect
            // immediately) and on disk (survives a service restart). A
            // genuine hot-swap to different firmware is still handled: the
            // setup modal's `mesh.probe-device` always re-probes unpinned,
            // and the flash flow already clears this pin on its own.
            if device_kind.is_none() {
                let detected = state.status.read().await.device_type;
                if detected != super::types::DeviceType::Unknown {
                    device_kind = Some(detected);
                    match super::load_config(&data_dir).await {
                        Ok(mut cfg) if cfg.device_kind.is_none() => {
                            cfg.device_kind = Some(detected);
                            if let Err(e) = super::save_config(&data_dir, &cfg).await {
                                warn!("Failed to persist auto-detected device_kind: {}", e);
                            } else {
                                info!(
                                    kind = %detected,
                                    "Pinned auto-detected firmware kind to avoid repeated multi-protocol resets on reconnect"
                                );
                            }
                        }
                        Ok(_) => {}
                        Err(e) => warn!("Failed to load mesh config to persist device_kind: {}", e),
                    }
                }
            }

            // Update status to disconnected. device_type/firmware_version are
            // reset too — they were previously left holding the LAST radio's
            // identity, so after a hot-swap the UI showed the old firmware
            // ("meshcore, disconnected") while a different stick sat in the
            // port. Unknown-until-next-successful-connect is the honest state.
            {
                let mut status = state.status.write().await;
                status.device_connected = false;
                status.device_path = None;
                status.device_type = super::types::DeviceType::Unknown;
                status.firmware_version = None;
            }
            let _ = state.event_tx.send(MeshEvent::DeviceDisconnected);

            // Wait before reconnecting (exponential backoff)
            tokio::select! {
                _ = tokio::time::sleep(reconnect_delay) => {},
                _ = shutdown.changed() => {
                    if *shutdown.borrow() { return; }
                },
            }

            // Increase backoff for next failure, cap at max
            reconnect_delay = (reconnect_delay * 2).min(RECONNECT_DELAY_MAX);
        }
    })
}
