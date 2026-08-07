//! Mesh networking: LoRa radio integration for offline peer discovery
//! and encrypted messaging between Archipelago nodes.
//!
//! Supports Meshcore firmware via Companion USB and Meshtastic firmware via
//! the Meshtastic serial API on Heltec, T-Beam, RAK WisBlock, Station G2,
//! and other ESP32/nRF52-based LoRa boards.

pub mod alerts;
pub mod bitcoin_relay;
pub mod crypto;
pub mod flash;
pub mod listener;
pub mod meshtastic;
pub mod message_types;
pub mod outbox;
pub mod protocol;
pub mod ratchet;
pub mod reticulum;
pub mod rnode_settings;
pub mod scheduler;
pub mod serial;
pub mod session;
pub mod steganography;
pub mod types;
pub mod x3dh;

pub use types::*;

use alerts::DeadManSwitch;
use anyhow::{Context, Result};
use bitcoin_relay::{BlockHeaderCache, RelayTracker};
use ed25519_dalek::SigningKey;
use listener::MeshState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tokio::sync::watch;
use tracing::{error, info, warn};

const MESH_CONFIG_FILE: &str = "mesh-config.json";

/// How long `MeshService::stop()` waits for the listener task to notice its
/// shutdown signal and exit gracefully before force-aborting it. See
/// `stop()`'s doc comment for the real incident this guards against: without
/// a hard abort fallback, a slow-to-notice listener could be left running
/// forever, orphaned, racing a later independently-started listener on the
/// same serial port.
const LISTENER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);
const MESH_IGNORED_RADIO_FILE: &str = "mesh-ignored-radio-contacts.json";
const MESH_CONTACTS_FILE: &str = "mesh-contacts.json";

/// Derive a stable synthetic `contact_id` for a federation peer from its
/// archipelago ed25519 pubkey. Mesh LoRa contacts use meshcore firmware's
/// own ID space (small ints 0..N), so federation peers are mapped into the
/// high half of u32 space to avoid collision. Both the receive path
/// (`inject_typed_from_federation`) and the startup pre-seed use this
/// formula so they always produce the same id for the same peer.
/// Mesh contacts at or above this id are synthetic federation peers (the high
/// half of the u32 space). Meshcore radio contacts use the firmware's low-int id
/// space, so this bit cleanly distinguishes "arrived over the authenticated
/// federation transport" from "heard over the radio".
pub(crate) const FEDERATION_CONTACT_ID_BASE: u32 = 0x8000_0000;

pub(crate) fn federation_peer_contact_id(archipelago_pubkey_hex: &str) -> u32 {
    let bytes = hex::decode(archipelago_pubkey_hex).unwrap_or_default();
    if bytes.len() < 4 {
        return 0x8000_0001;
    }
    let low = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    0x8000_0000 | (low & 0x7FFF_FFFF)
}

/// Bind radio (LoRa) contacts to their federation twin's archipelago identity.
///
/// The same physical node commonly appears twice in the peer table: a radio
/// contact (low `contact_id`, firmware routing key only, `arch_pubkey_hex ==
/// None`) and a federation peer (high `contact_id`, `arch_pubkey_hex` set). The
/// radio half carries no archipelago identity because identity adverts are no
/// longer broadcast on the public channel (anti-spam), so the `!ai` trust gate
/// and envelope signature verification have no key to check a radio asker
/// against — a `!ai` from a trusted node over LoRa is therefore denied, and the
/// node shows up as two separate contacts.
///
/// We correlate the two halves by exact, case-insensitive `advert_name` and copy
/// the federation peer's `arch_pubkey_hex`/`did`/`x25519` onto the radio peer.
/// This only supplies a CANDIDATE identity key; it does NOT bypass
/// authentication. A radio envelope must still carry an Ed25519 signature that
/// verifies against this bound key (see `handle_typed_envelope_direct`), so a
/// meshcore node merely *named* like a trusted node cannot impersonate it — it
/// cannot produce the signature. The candidate key comes from the authenticated
/// federation handshake (`nodes.json`), never from anything the radio packet
/// claims. Names held by more than one federation peer are treated as ambiguous
/// and skipped so a duplicate name can't bind the wrong identity.
pub(crate) fn bind_federation_twins(peers: &mut std::collections::HashMap<u32, MeshPeer>) {
    // name (lowercased) -> federation identity; `None` marks an ambiguous name
    // (seen on more than one federation peer) which we must not bind.
    type FedIdentity = (String, Option<String>, Option<[u8; 32]>);
    let mut fed_by_name: std::collections::HashMap<String, Option<FedIdentity>> =
        std::collections::HashMap::new();
    for p in peers.values() {
        if p.contact_id < FEDERATION_CONTACT_ID_BASE {
            continue;
        }
        let Some(arch) = p.arch_pubkey_hex.clone() else {
            continue;
        };
        let name = p.advert_name.trim().to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        fed_by_name
            .entry(name)
            .and_modify(|e| *e = None) // a second federation peer with this name → ambiguous
            .or_insert(Some((arch, p.did.clone(), p.x25519_pubkey)));
    }
    if fed_by_name.is_empty() {
        return;
    }
    for p in peers.values_mut() {
        if p.contact_id >= FEDERATION_CONTACT_ID_BASE || p.arch_pubkey_hex.is_some() {
            continue;
        }
        let name = p.advert_name.trim().to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        if let Some(Some((arch, did, x25519))) = fed_by_name.get(&name) {
            p.arch_pubkey_hex = Some(arch.clone());
            if p.did.is_none() {
                p.did = did.clone();
            }
            if p.x25519_pubkey.is_none() {
                p.x25519_pubkey = *x25519;
            }
        }
    }
}

/// One logical contact after collapsing cross-transport twins (see
/// [`group_peer_twins`]). A node reachable both over LoRa and over federation
/// has two `MeshPeer` rows (different `contact_id`s) but is one conversation.
pub(crate) struct PeerGroup {
    /// The peer row the UI should address. The radio twin when one exists (so
    /// `send_typed_wire` stays mesh-first — LoRa if reachable, else federation
    /// via the bound arch key), otherwise the federation row. Gap-healed so its
    /// name / `arch_pubkey_hex` / `did` are populated from whichever twin had
    /// them, and `reachable` is the OR across the group.
    pub canonical: MeshPeer,
    /// Every `contact_id` in the group. The conversation's messages are the
    /// union of those keyed by any of these ids — federation-injected messages
    /// land on the federation twin's id, radio messages on the radio twin's.
    pub contact_ids: Vec<u32>,
}

/// Collapse cross-transport twin peers into one conversation per identity.
///
/// The same node commonly appears twice in the peer table: a radio twin (low
/// `contact_id`, firmware routing key) and a federation twin (high
/// `contact_id`, archipelago key), correlated by [`bind_federation_twins`]
/// which copies `arch_pubkey_hex` onto the radio twin but leaves both rows.
/// Messages are keyed by `peer_contact_id`, so they split across the two ids:
/// a federation-injected message sits on the federation row while the user may
/// open the radio row and see an empty thread (the `.120`→`.89` symptom).
///
/// Group peers by `arch_pubkey_hex` when set, else treat each peer as its own
/// singleton group keyed by `contact_id`. Grouping is done ONLY here at surface
/// time — never re-keyed at bind time — so outbound routing keeps the distinct
/// per-twin `contact_id`s and stays mesh-first. First-seen order is preserved
/// for stable downstream sorting.
pub(crate) fn group_peer_twins(peers: &[MeshPeer]) -> Vec<PeerGroup> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<MeshPeer>> =
        std::collections::HashMap::new();
    for p in peers {
        let key = match p.arch_pubkey_hex.as_deref() {
            Some(arch) if !arch.is_empty() => format!("arch:{}", arch.to_ascii_lowercase()),
            _ => format!("cid:{}", p.contact_id),
        };
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(p.clone());
    }

    let mut out = Vec::with_capacity(order.len());
    for key in order {
        let members = match groups.remove(&key) {
            Some(m) if !m.is_empty() => m,
            _ => continue,
        };
        let contact_ids: Vec<u32> = members.iter().map(|m| m.contact_id).collect();
        // Canonical = the radio twin (lowest id below the federation base) when
        // one exists, else the lowest id overall (a federation-only peer).
        let canonical_src = members
            .iter()
            .filter(|m| m.contact_id < FEDERATION_CONTACT_ID_BASE)
            .min_by_key(|m| m.contact_id)
            .or_else(|| members.iter().min_by_key(|m| m.contact_id))
            .expect("non-empty members");
        let mut canonical = canonical_src.clone();
        // Heal gaps from the twin: a radio row may lack the advert name, arch
        // identity, or did that only the federation row carries.
        if canonical.advert_name.trim().is_empty() {
            if let Some(named) = members.iter().find(|m| !m.advert_name.trim().is_empty()) {
                canonical.advert_name = named.advert_name.clone();
            }
        }
        if canonical.arch_pubkey_hex.is_none() {
            canonical.arch_pubkey_hex = members.iter().find_map(|m| m.arch_pubkey_hex.clone());
        }
        if canonical.did.is_none() {
            canonical.did = members.iter().find_map(|m| m.did.clone());
        }
        // Reachable if ANY twin is reachable (radio path or off-radio federation).
        canonical.reachable = members.iter().any(|m| m.reachable);
        out.push(PeerGroup {
            canonical,
            contact_ids,
        });
    }
    out
}

/// Upsert a mesh peer record representing a federation node so the UI can
/// address it as a chat and `mesh.send-content` can route ContentRef to it.
/// Existing entries (same contact_id) are updated in place, preserving any
/// previously observed radio state (rssi/snr/hops).
pub(crate) async fn upsert_federation_peer(
    state: &Arc<listener::MeshState>,
    archipelago_pubkey_hex: &str,
    did: &str,
    name: Option<&str>,
) -> u32 {
    let contact_id = federation_peer_contact_id(archipelago_pubkey_hex);
    let display_name = name.map(|s| s.to_string()).unwrap_or_else(|| {
        let short = &archipelago_pubkey_hex[..archipelago_pubkey_hex.len().min(8)];
        format!("Archipelago {}", short)
    });
    let mut peers = state.peers.write().await;
    let existing = peers.get(&contact_id).cloned();
    let peer = MeshPeer {
        contact_id,
        advert_name: display_name,
        did: Some(did.to_string()),
        pubkey_hex: Some(archipelago_pubkey_hex.to_string()),
        // Federation peers are authenticated by the Tor relay upstream; their
        // archipelago key is known, so bind it as the identity key too.
        arch_pubkey_hex: Some(archipelago_pubkey_hex.to_string()),
        x25519_pubkey: existing.as_ref().and_then(|p| p.x25519_pubkey),
        rssi: existing.as_ref().and_then(|p| p.rssi),
        snr: existing.as_ref().and_then(|p| p.snr),
        last_heard: chrono::Utc::now().to_rfc3339(),
        hops: existing.as_ref().map(|p| p.hops).unwrap_or(0),
        last_advert: existing.as_ref().map(|p| p.last_advert).unwrap_or(0),
        // Federation peers are reachable off-radio (Tor/FIPS), so always true.
        reachable: true,
        // Off-radio E2E (federation) is handled by the archy-peer path; preserve
        // any radio PKI capability learned for a twinned contact.
        pkc_capable: existing.as_ref().map(|p| p.pkc_capable).unwrap_or(false),
        lat: existing.as_ref().and_then(|p| p.lat),
        lon: existing.as_ref().and_then(|p| p.lon),
        // Federation seeding carries no Lightning datum today; keep whatever an
        // explicit advert already established for this peer.
        lightning_uri: existing.as_ref().and_then(|p| p.lightning_uri.clone()),
    };
    peers.insert(contact_id, peer);
    // A radio twin of this node (same advert_name, no arch identity yet) can now
    // inherit this federation peer's archipelago key — so a signed `!ai`/typed
    // message arriving over LoRa from it authenticates and the duplicate radio
    // contact resolves to the same identity.
    bind_federation_twins(&mut peers);
    drop(peers);
    state.update_peer_count().await;
    contact_id
}

/// Purge a federation peer from all live mesh state and persisted contacts so
/// removing a node (federation.remove-node) also clears its chat contact,
/// thread, and any per-contact customisation — otherwise a stale/renamed node
/// (e.g. an old "Arch HP" entry) lingers in the chat list even after it's gone
/// from `nodes.json` (#2). Keyed by the synthetic `contact_id` for the peer
/// table/messages and by `pubkey_hex` for the pubkey-keyed contacts/presence
/// stores.
pub(crate) async fn purge_federation_peer(
    state: &Arc<listener::MeshState>,
    contact_id: u32,
    pubkey_hex: &str,
    data_dir: &Path,
) {
    state.peers.write().await.remove(&contact_id);
    state.shared_secrets.write().await.remove(&contact_id);
    state
        .messages
        .write()
        .await
        .retain(|m| m.peer_contact_id != contact_id);
    state.presence.write().await.remove(pubkey_hex);
    let mut contacts = state.contacts.write().await;
    if contacts.remove(pubkey_hex).is_some() {
        let snapshot = contacts.clone();
        drop(contacts);
        if let Err(e) = save_mesh_contacts(data_dir, &snapshot).await {
            warn!("Failed to persist mesh contacts after purge: {}", e);
        }
    }
    state.update_peer_count().await;
}

/// Load federation nodes from disk and upsert each as a synthetic mesh peer.
/// Called at MeshService startup so the chat list already contains every
/// known federation node — users can share files to them without first
/// receiving a message.
pub(crate) async fn seed_federation_peers_into_mesh(
    state: &Arc<listener::MeshState>,
    data_dir: &Path,
) {
    let nodes = match crate::federation::load_nodes(data_dir).await {
        Ok(n) => n,
        Err(_) => return,
    };
    // Skip nodes whose onion we've already seeded: the same physical node can
    // linger in the federation list under two dids (see B1/B2). Seeding both
    // would create two chat contacts for one node — one by name+logo and one
    // by raw did. One onion → one mesh contact.
    let mut seen_onions = std::collections::HashSet::new();
    for node in nodes {
        let onion_key = node.onion.trim_end_matches(".onion").to_string();
        if !onion_key.is_empty() && !seen_onions.insert(onion_key) {
            tracing::debug!(did = %node.did, onion = %node.onion, "skipping duplicate federation node (onion already seeded)");
            continue;
        }
        upsert_federation_peer(state, &node.pubkey, &node.did, node.name.as_deref()).await;
    }
}

/// Operator-configured LoRa PHY parameters for a Meshcore radio, in the
/// firmware's own field units: `freq_khz` = MHz×1000 (869618 → 869.618 MHz),
/// `bw_hz` = kHz×1000 (62500 → 62.5 kHz), `sf` 5..=12, `cr` 5..=8. These are
/// region/deployment-specific (e.g. the Portugal preset 869618/62500/8/8) and
/// MUST match every radio on the local mesh — a mismatched radio hears RF
/// energy but demodulates nothing. None (the default) leaves the device's own
/// settings untouched, so nodes outside the configured deployment are never
/// affected.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoraRadioParams {
    pub freq_khz: u32,
    pub bw_hz: u32,
    pub sf: u8,
    pub cr: u8,
}

/// Mesh configuration (persisted to disk).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshConfig {
    pub enabled: bool,
    /// Specific device path, or None for auto-detection.
    #[serde(default)]
    pub device_path: Option<String>,
    /// Channel name for broadcasts.
    #[serde(default)]
    pub channel_name: Option<String>,
    /// Meshtastic LoRa region (e.g. "EU_868", "US", "ANZ"). Fresh-flashed
    /// Meshtastic radios ship region-UNSET and are RF-silent until a region is
    /// set, so archy provisions this region on connect to bring every node onto
    /// the same band automatically (the parity equivalent of a meshcore radio
    /// coming up on its configured band). Ignored for meshcore devices and when
    /// unset/None.
    #[serde(default)]
    pub lora_region: Option<String>,
    /// Meshcore LoRa PHY parameters (freq/bw/sf/cr). Provisioned onto the
    /// radio on connect when set; None leaves the device untouched. Ignored
    /// for Meshtastic (region/preset covers it) and Reticulum.
    #[serde(default)]
    pub lora_radio_params: Option<LoraRadioParams>,
    /// Whether to periodically broadcast our identity.
    #[serde(default)]
    pub broadcast_identity: bool,
    /// Custom advertised name on the mesh network.
    #[serde(default)]
    pub advert_name: Option<String>,
    /// Off-grid mode: disable Tor/internet, route everything via mesh only.
    #[serde(default)]
    pub mesh_only_mode: Option<bool>,
    /// Announce new Bitcoin block headers over mesh (internet-connected nodes only).
    #[serde(default)]
    pub announce_block_headers: bool,
    /// Accept Bitcoin block headers received over mesh from peers. On by default;
    /// turn off to ignore inbound headers (the receive half of issue #28).
    #[serde(default = "default_true")]
    pub receive_block_headers: bool,
    /// Steganographic encoding mode for mesh messages (Normal = disabled).
    #[serde(default)]
    pub steganography_mode: steganography::SteganographyMode,
    /// Encrypt directed relay messages (TX, Lightning, block headers) via ratchet or shared secret.
    /// Set to false to disable encryption for debugging or rollback.
    #[serde(default = "default_true")]
    pub encrypt_relay_messages: bool,
    /// Answer AI queries (AssistQuery) from peers using this node's local LLM
    /// (issue #50). Off by default — the node only becomes a mesh AI on opt-in.
    #[serde(default)]
    pub assistant_enabled: bool,
    /// Ollama model used to answer AI queries. None → the built-in default.
    #[serde(default)]
    pub assistant_model: Option<String>,
    /// When true (default), only federation-Trusted peers may ask; when false,
    /// any peer on the mesh may ask (spends this node's compute + airtime).
    #[serde(default = "default_true")]
    pub assistant_trusted_only: bool,
    /// Which AI backend answers queries: "claude" (the shared Claude proxy
    /// token at secrets/claude-api-key — default for now, works without a
    /// local GPU) or "ollama" (a local model on this node).
    #[serde(default = "default_assistant_backend")]
    pub assistant_backend: String,
    /// Per-contact allowlist (ed25519 pubkey hex) permitted to use `!ai` even
    /// when `assistant_trusted_only` is on and they aren't federation-Trusted.
    #[serde(default)]
    pub assistant_allowed_contacts: Vec<String>,
    /// Pin the expected firmware on `device_path`/auto-detected ports. A
    /// reflashable board (e.g. Heltec V3) can run Meshcore, Meshtastic, or
    /// RNode firmware, so probe order alone is best-effort — set this when an
    /// operator knows which one is plugged in. When `Some`, only that
    /// device's probe runs (no other firmware's init bytes are ever injected
    /// into the port); `None` keeps today's Meshcore→Meshtastic→Reticulum
    /// strict-probe auto-detect.
    #[serde(default)]
    pub device_kind: Option<types::DeviceType>,
    /// Optional plain-TCP Reticulum interface — radio-less dev/verification
    /// alternative to the serial-RNode path (see `types::ReticulumTcpConfig`
    /// doc comment). Not exposed via `mesh.configure`/frontend; hand-edit
    /// this file to use it. `None` (default) preserves today's
    /// serial-only/auto-detect behavior unchanged.
    #[serde(default)]
    pub reticulum_tcp: Option<types::ReticulumTcpConfig>,
    /// Whether archipelago manages the radio's configuration (region, shared
    /// channel, PHY params, advert name) on connect. `true` (default) is
    /// today's behavior. `false` = the hot-swap modal's "keep as is": use the
    /// radio exactly as flashed — no config writes at all.
    #[serde(default = "default_true")]
    pub manage_radio: bool,
}

/// True when `name` is a LoRa region the Meshtastic driver can program
/// (the canonical region list for the mesh settings UI).
pub fn meshtastic_region_is_valid(name: &str) -> bool {
    meshtastic::region_name_to_code(name).is_some()
}

fn default_assistant_backend() -> String {
    "claude".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            device_path: None,
            channel_name: Some("archipelago".to_string()),
            lora_region: None,
            lora_radio_params: None,
            broadcast_identity: true,
            advert_name: None,
            mesh_only_mode: None,
            announce_block_headers: false,
            receive_block_headers: true,
            steganography_mode: steganography::SteganographyMode::Normal,
            encrypt_relay_messages: true,
            assistant_enabled: false,
            assistant_model: None,
            assistant_trusted_only: true,
            assistant_backend: default_assistant_backend(),
            assistant_allowed_contacts: Vec::new(),
            device_kind: None,
            reticulum_tcp: None,
            manage_radio: true,
        }
    }
}

/// Whether a mesh config file has ever been written for this node — lets the
/// boot path distinguish "operator explicitly disabled mesh" (file exists,
/// enabled=false) from "never configured" (no file), which is the only case
/// radio auto-enable should touch.
pub fn config_file_exists(data_dir: &Path) -> bool {
    data_dir.join(MESH_CONFIG_FILE).exists()
}

/// True when `new` differs from `old` in any field a running mesh session
/// captured by value at spawn (device path/kind, advert name, region, PHY
/// params, channel, manage_radio, TCP interface) — i.e. when applying `new`
/// to a live service requires a listener restart. Fields the session reads
/// live from shared state (broadcast flags, assistant settings, steganography
/// mode, …) deliberately don't trigger a restart.
fn session_config_changed(old: &MeshConfig, new: &MeshConfig) -> bool {
    old.device_path != new.device_path
        || old.device_kind != new.device_kind
        || old.advert_name != new.advert_name
        || old.lora_region != new.lora_region
        || old.lora_radio_params != new.lora_radio_params
        || old.channel_name != new.channel_name
        || old.manage_radio != new.manage_radio
        || old.reticulum_tcp != new.reticulum_tcp
}

pub async fn load_config(data_dir: &Path) -> Result<MeshConfig> {
    let path = data_dir.join(MESH_CONFIG_FILE);
    if !path.exists() {
        return Ok(MeshConfig::default());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read mesh config")?;
    let config: MeshConfig = serde_json::from_str(&content).unwrap_or_default();
    Ok(config)
}

pub async fn save_config(data_dir: &Path, config: &MeshConfig) -> Result<()> {
    fs::create_dir_all(data_dir)
        .await
        .context("Failed to create data dir")?;
    let content =
        serde_json::to_string_pretty(config).context("Failed to serialize mesh config")?;
    fs::write(data_dir.join(MESH_CONFIG_FILE), content)
        .await
        .context("Failed to write mesh config")?;
    Ok(())
}

/// Two /dev paths refer to the same serial device if their symlink-resolved
/// targets match (e.g. `/dev/mesh-radio` vs the `/dev/ttyUSBn` it points at).
/// Paths that fail to resolve fall back to a plain string comparison.
async fn same_serial_device(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let ra = fs::canonicalize(a)
        .await
        .unwrap_or_else(|_| PathBuf::from(a));
    let rb = fs::canonicalize(b)
        .await
        .unwrap_or_else(|_| PathBuf::from(b));
    ra == rb
}

pub async fn load_ignored_radio_contacts(data_dir: &Path) -> Vec<String> {
    let path = data_dir.join(MESH_IGNORED_RADIO_FILE);
    if !path.exists() {
        return Vec::new();
    }
    match fs::read_to_string(&path).await {
        Ok(s) => serde_json::from_str::<Vec<String>>(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub async fn save_ignored_radio_contacts(data_dir: &Path, pubkeys: &[String]) -> Result<()> {
    fs::create_dir_all(data_dir).await.ok();
    let content =
        serde_json::to_string_pretty(pubkeys).context("Failed to serialize ignored-radio list")?;
    fs::write(data_dir.join(MESH_IGNORED_RADIO_FILE), content)
        .await
        .context("Failed to write ignored-radio list")?;
    Ok(())
}

/// Load persisted mesh contact customisations (alias / notes / pinned / blocked),
/// decrypting at rest with the node key and migrating any legacy plaintext file.
/// Returns an empty map on any error so a read failure never loses live state.
pub async fn load_mesh_contacts(
    data_dir: &Path,
) -> std::collections::HashMap<String, listener::ContactEntry> {
    let path = data_dir.join(MESH_CONTACTS_FILE);
    let Ok(raw) = fs::read(&path).await else {
        return std::collections::HashMap::new();
    };
    let bytes = if crate::storage_crypto::is_plaintext_json(&raw) {
        raw
    } else {
        match crate::storage_crypto::derive_key(
            data_dir,
            crate::storage_crypto::DOMAIN_MESH_CONTACTS,
        )
        .await
        {
            Ok(k) => match crate::storage_crypto::open(&raw, &k) {
                Ok(p) => p,
                Err(e) => {
                    warn!("mesh contacts: decrypt failed ({e}); keeping in-memory state");
                    return std::collections::HashMap::new();
                }
            },
            Err(_) => return std::collections::HashMap::new(),
        }
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Persist mesh contact customisations, encrypted at rest with the node key and
/// written atomically (temp + rename) so a crash mid-write can't corrupt them.
pub async fn save_mesh_contacts(
    data_dir: &Path,
    contacts: &std::collections::HashMap<String, listener::ContactEntry>,
) -> Result<()> {
    fs::create_dir_all(data_dir).await.ok();
    let content = serde_json::to_vec(contacts).context("Failed to serialize mesh contacts")?;
    let bytes = match crate::storage_crypto::derive_key(
        data_dir,
        crate::storage_crypto::DOMAIN_MESH_CONTACTS,
    )
    .await
    {
        Ok(k) => crate::storage_crypto::seal(&content, &k).unwrap_or(content),
        Err(_) => content, // no key yet (pre-onboarding) → plaintext rather than no-write
    };
    let path = data_dir.join(MESH_CONTACTS_FILE);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &bytes)
        .await
        .context("Failed to write mesh contacts tmp")?;
    fs::rename(&tmp, &path)
        .await
        .context("Failed to rename mesh contacts")?;
    Ok(())
}

/// Detect serial devices that could be mesh radios.
/// Checks both Meshcore (via probe) and legacy Meshtastic paths.
pub async fn detect_devices() -> Vec<String> {
    serial::detect_serial_devices().await
}

/// Detected serial ports with USB identity metadata (for the device-detected
/// setup modal's hardware graphic).
pub async fn detect_devices_info() -> Vec<serial::DetectedDeviceInfo> {
    serial::detect_serial_devices_info().await
}

// ─── MeshService ────────────────────────────────────────────────────────

/// Top-level mesh networking service.
/// Manages the background listener, exposes APIs for RPC handlers.
pub struct MeshService {
    state: Arc<MeshState>,
    config: MeshConfig,
    data_dir: PathBuf,
    shutdown_tx: Option<watch::Sender<bool>>,
    listener_handle: Option<tokio::task::JoinHandle<()>>,
    deadman_handle: Option<tokio::task::JoinHandle<()>>,
    block_announcer_handle: Option<tokio::task::JoinHandle<()>>,
    presence_handle: Option<tokio::task::JoinHandle<()>>,
    scheduler_handle: Option<tokio::task::JoinHandle<()>>,
    cmd_rx: Option<tokio::sync::mpsc::Receiver<listener::MeshCommand>>,
    // Crypto identity for this node
    our_did: String,
    our_ed_pubkey_hex: String,
    our_x25519_secret: [u8; 32],
    our_x25519_pubkey_hex: String,
    signing_key: SigningKey,
    /// Human-readable server name (e.g. "Arch Dev", "ThinkPad") for mesh adverts.
    server_name: Option<String>,
    // Phase 4: off-grid Bitcoin operations
    pub block_header_cache: Arc<BlockHeaderCache>,
    pub relay_tracker: Arc<RelayTracker>,
    pub dead_man_switch: Arc<DeadManSwitch>,
    /// Scheduled / queued outbound mesh messages (issue #50, phase 1.7).
    pub scheduler: Arc<scheduler::MeshScheduler>,
}

impl MeshService {
    /// Create a new MeshService. Does not start the listener yet.
    pub async fn new(
        data_dir: &Path,
        signing_key: &SigningKey,
        did: &str,
        ed_pubkey_hex: &str,
    ) -> Result<Self> {
        let config = load_config(data_dir).await?;
        let channel_name = config
            .channel_name
            .clone()
            .unwrap_or_else(|| "archipelago".to_string());

        let block_header_cache = Arc::new(BlockHeaderCache::new());
        let relay_tracker = Arc::new(RelayTracker::new());
        let session_manager = Arc::new(session::SessionManager::new(data_dir));
        let (state, _rx, cmd_rx) = MeshState::new(
            &channel_name,
            Arc::clone(&block_header_cache),
            Some(Arc::clone(&relay_tracker)),
            config.steganography_mode,
            config.encrypt_relay_messages,
            config.receive_block_headers,
            Arc::clone(&session_manager),
            ed_pubkey_hex.to_string(),
            listener::AssistantConfig {
                enabled: config.assistant_enabled,
                model: config.assistant_model.clone(),
                trusted_only: config.assistant_trusted_only,
                backend: config.assistant_backend.clone(),
                allowed_contacts: config.assistant_allowed_contacts.clone(),
            },
            data_dir.to_path_buf(),
        );

        // Derive X25519 keys from Ed25519 identity
        let x25519_secret = crypto::ed25519_secret_to_x25519(signing_key);
        let x25519_pubkey =
            crypto::ed25519_pubkey_to_x25519(&signing_key.verifying_key().to_bytes())?;
        let x25519_pubkey_hex = hex::encode(x25519_pubkey);
        let dead_man_switch = Arc::new(DeadManSwitch::new(data_dir).await.unwrap_or_else(|e| {
            warn!("Failed to load dead man config (using defaults): {}", e);
            // Fallback: create with defaults (won't persist until configured)
            tokio::runtime::Handle::current()
                .block_on(DeadManSwitch::new(data_dir))
                .expect("DeadManSwitch fallback should succeed")
        }));

        // Pre-seed mesh state with a synthetic peer record for every known
        // federation node. Mesh LoRa discovery and federation have disjoint
        // identity namespaces, so without this step a user can't address a
        // federated peer from the mesh UI until one side receives over the
        // radio — which never happens for nodes that only share Tor.
        seed_federation_peers_into_mesh(&state, data_dir).await;

        // Load the radio-contact blocklist so previously-wiped firmware
        // contacts stay hidden after restart. Without this, meshcore's
        // persistent on-device contact table regenerates the rows on the
        // next refresh_contacts cycle and the user sees stale entries
        // they already cleared.
        {
            let ignored = load_ignored_radio_contacts(data_dir).await;
            let mut set = state.radio_contact_blocklist.write().await;
            for pk in ignored {
                set.insert(pk);
            }
        }

        // Restore persisted contact customisations (alias/notes/pinned/blocked),
        // decrypted with the node key, so they survive restarts.
        {
            let saved = load_mesh_contacts(data_dir).await;
            if !saved.is_empty() {
                let mut contacts = state.contacts.write().await;
                for (pk, entry) in saved {
                    contacts.insert(pk, entry);
                }
            }
        }

        // Restore the message history + outbound send-sequence counters, then
        // keep them persisted. Until 2026-07-22 both were RAM-only: every
        // restart wiped the chat history, and the reset sequence counters made
        // peers drop the first post-reboot messages as (pubkey, seq) replays.
        state.load_persisted_messages().await;
        listener::spawn_message_persister(Arc::clone(&state));

        Ok(Self {
            state,
            config,
            data_dir: data_dir.to_path_buf(),
            shutdown_tx: None,
            listener_handle: None,
            deadman_handle: None,
            block_announcer_handle: None,
            presence_handle: None,
            scheduler_handle: None,
            cmd_rx: Some(cmd_rx),
            our_did: did.to_string(),
            our_ed_pubkey_hex: ed_pubkey_hex.to_string(),
            our_x25519_secret: x25519_secret,
            our_x25519_pubkey_hex: x25519_pubkey_hex,
            signing_key: signing_key.clone(),
            server_name: None,
            block_header_cache,
            relay_tracker,
            dead_man_switch,
            scheduler: Arc::new(scheduler::MeshScheduler::load(data_dir).await),
        })
    }

    /// Set the human-readable server name used in mesh adverts.
    pub fn set_server_name(&mut self, name: Option<String>) {
        self.server_name = name;
    }

    /// Start the background mesh listener. Idempotent: if the listener is
    /// already running, this is a harmless no-op rather than an error —
    /// confirmed live 2026-07-23, a real race between the flash job's own
    /// post-flash restart and a concurrent user "Keep As Is" click (both
    /// legitimately trying to ensure the listener is running) surfaced this
    /// as a user-facing "Mesh listener already running" RPC error. Ensuring
    /// the listener is running is the intent every caller actually has;
    /// whichever caller's start() happens to win the race, the other
    /// finding it already satisfied is success, not failure.
    pub fn start(&mut self) -> Result<()> {
        if self.listener_handle.is_some() {
            return Ok(());
        }

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        self.shutdown_tx = Some(shutdown_tx);

        let cmd_rx = self
            .cmd_rx
            .take()
            .ok_or_else(|| anyhow::anyhow!("Command channel already consumed"))?;

        let handle = listener::spawn_mesh_listener(
            Arc::clone(&self.state),
            self.data_dir.clone(),
            self.config.device_path.clone(),
            self.our_did.clone(),
            self.our_ed_pubkey_hex.clone(),
            self.our_x25519_secret,
            self.our_x25519_pubkey_hex.clone(),
            // The mesh-page "Name on the mesh" (config.advert_name) wins over
            // the server name — it existed as write-only config with no reader
            // until this line, which is why renaming on the Mesh page never
            // changed anything on the air.
            self.config
                .advert_name
                .clone()
                .or_else(|| self.server_name.clone()),
            self.config.lora_region.clone(),
            self.config.lora_radio_params,
            self.config.channel_name.clone(),
            self.config.device_kind,
            self.config.reticulum_tcp.clone(),
            self.config.manage_radio,
            shutdown_rx,
            cmd_rx,
        );
        self.listener_handle = Some(handle);

        // Spawn dead man's switch background checker
        let dms = Arc::clone(&self.dead_man_switch);
        let dms_state = Arc::clone(&self.state);
        let dms_key = self.signing_key.clone();
        let dms_shutdown = self
            .shutdown_tx
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Shutdown channel not initialized"))?
            .subscribe();
        let dms_handle = tokio::spawn(async move {
            let mut shutdown = dms_shutdown;
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            interval.tick().await; // skip first immediate tick
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if dms.is_triggered().await {
                            let was_triggered = *dms.triggered_flag().await;
                            if !was_triggered {
                                error!("Dead man's switch TRIGGERED — broadcasting alert");
                                if let Ok(wire) = dms.build_signed_alert(&dms_key).await {
                                    for ch in [0u8, 1] {
                                        let _ = dms_state.send_cmd(
                                            listener::MeshCommand::BroadcastChannel {
                                                channel: ch,
                                                payload: wire.clone(),
                                            },
                                        ).await;
                                    }
                                }
                                dms.mark_triggered().await;
                            }
                        }
                    }
                    _ = shutdown.changed() => {
                        if *shutdown.borrow() { return; }
                    }
                }
            }
        });
        self.deadman_handle = Some(dms_handle);

        // Scheduled-message task (issue #50, phase 1.7): fires queued messages
        // when due, retrying peer DMs until the peer is back in range.
        let sched = Arc::clone(&self.scheduler);
        let sched_state = Arc::clone(&self.state);
        let sched_shutdown = self
            .shutdown_tx
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Shutdown channel not initialized"))?
            .subscribe();
        let sched_handle = tokio::spawn(async move {
            scheduler::run_scheduler(sched, sched_state, sched_shutdown).await;
        });
        self.scheduler_handle = Some(sched_handle);

        // Spawn block header announcer (internet-connected nodes only)
        if self.config.announce_block_headers {
            let bha_state = Arc::clone(&self.state);
            let bha_cache = Arc::clone(&self.block_header_cache);
            let bha_key = self.signing_key.clone();
            let bha_did = self.our_did.clone();
            let bha_shutdown = self
                .shutdown_tx
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Shutdown channel not initialized"))?
                .subscribe();
            let bha_handle = tokio::spawn(async move {
                let mut shutdown = bha_shutdown;
                let mut interval = tokio::time::interval(Duration::from_secs(30));
                interval.tick().await; // skip first
                let mut last_announced_height: u64 = 0;
                let mut last_announce_at: Option<std::time::Instant> = None;
                let client = match reqwest::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                {
                    Ok(c) => c,
                    Err(e) => {
                        error!("Failed to create HTTP client for block announcer: {}", e);
                        return;
                    }
                };

                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            // Poll Bitcoin Core for latest block
                            match bitcoin_rpc_getblockcount(&client).await {
                                Ok(height) if height > last_announced_height => {
                                    // Advance the tip baseline immediately so a fast Bitcoin
                                    // catch-up (a new block every poll) doesn't re-fire each tick.
                                    last_announced_height = height;
                                    // Throttle: at most one announcement per ~9 min. Real ~10 min
                                    // blocks still propagate, but a rapid catch-up can no longer
                                    // flood the shared LoRa channel.
                                    if last_announce_at
                                        .map(|t| t.elapsed() < Duration::from_secs(540))
                                        .unwrap_or(false)
                                    {
                                        continue;
                                    }
                                    if let Ok(header) = bitcoin_rpc_getblockheader_by_height(&client, height).await {
                                        // Store in cache
                                        let payload = message_types::BlockHeaderPayload {
                                            height,
                                            hash: header.hash.clone(),
                                            prev_hash: header.prev_hash.clone(),
                                            timestamp: header.timestamp,
                                            announced_by: bha_did.clone(),
                                        };
                                        if let Err(e) = bha_cache.store_header(payload).await {
                                            warn!("Failed to persist block-header cache: {e:#}");
                                        }

                                        // Build signed announcement and broadcast
                                        match bitcoin_relay::build_block_header_announcement(
                                            height,
                                            &header.hash,
                                            &header.prev_hash,
                                            header.timestamp,
                                            &bha_did,
                                            &bha_key,
                                        ) {
                                            Ok(wire) => {
                                                // Send to peers — prefer Archy nodes, fall back to all (max 5)
                                                let peers = bha_state.peers.read().await;
                                                let mut sent = 0u32;
                                                let max_peers = 5u32;
                                                // First pass: Archy nodes
                                                for peer in peers.values() {
                                                    if sent >= max_peers { break; }
                                                    if !peer.advert_name.starts_with("Archy-") { continue; }
                                                    if let Some(ref pk) = peer.pubkey_hex {
                                                        if let Ok(pk_bytes) = hex::decode(pk) {
                                                            if pk_bytes.len() >= 6 {
                                                                let mut prefix = [0u8; 6];
                                                                prefix.copy_from_slice(&pk_bytes[..6]);
                                                                let _ = bha_state.send_cmd(
                                                                    listener::MeshCommand::SendRaw {
                                                                        dest_pubkey_prefix: prefix,
                                                                        payload: wire.clone(),
                                                                    },
                                                                ).await;
                                                                sent += 1;
                                                            }
                                                        }
                                                    }
                                                }
                                                // NOTE: intentionally NO fallback to arbitrary
                                                // peers. Block headers go ONLY to known Archy
                                                // (federated) nodes — never to random meshcore
                                                // devices on the shared public channel.
                                                drop(peers);
                                                if sent > 0 {
                                                    last_announce_at = Some(std::time::Instant::now());
                                                    info!(height, hash = %header.hash, peers = sent, "Announced block header to Archy peers");
                                                }
                                            }
                                            Err(e) => warn!("Failed to build block announcement: {}", e),
                                        }
                                    }
                                }
                                Ok(_) => {} // No new block
                                Err(e) => {
                                    // Bitcoin not running or not reachable — that's fine, skip
                                    tracing::debug!("Block poll: {}", e);
                                }
                            }
                        }
                        _ = shutdown.changed() => {
                            if *shutdown.borrow() { return; }
                        }
                    }
                }
            });
            self.block_announcer_handle = Some(bha_handle);
            info!("Block header announcer started");
        }

        // Presence heartbeat broadcaster is DISABLED. The CBOR-encoded
        // PresencePayload was rendering as garbled bytes on peers that
        // didn't understand the typed envelope (e.g. the FreeMadeira
        // repeater echoed it back as plaintext on channel 0), spamming
        // every visible node with "Archy-…: av�…fstatusfonline…" every
        // 120s. Re-enable only after either (a) presence moves to a
        // non-broadcast path or (b) we can guarantee no plain-text-only
        // receivers on the shared channel.
        self.presence_handle = None;

        info!("Mesh service started");
        Ok(())
    }

    /// Stop the background listener and dead man's switch.
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }
        if let Some(mut handle) = self.listener_handle.take() {
            // Bounded wait for graceful shutdown, with a hard abort as
            // fallback — confirmed live 2026-07-23: a caller-side timeout
            // wrapping stop() (mesh::flash's STOP_LISTENER_TIMEOUT) cancelled
            // this await when the listener was slow to notice its shutdown
            // signal (mid multi-candidate probe), but `.take()` above had
            // already cleared `listener_handle` to None — so MeshService
            // believed it was stopped while the task kept running, orphaned
            // (dropping a JoinHandle does not abort the task it points to).
            // A later start() then spawned a second, fully independent
            // listener session racing the orphaned one on the same serial
            // port — neither could ever get a clean response, so every
            // mesh.configure/probe against that device failed indefinitely
            // even though the device itself was fine.
            //
            // Awaiting `&mut handle` (not `handle` by value) is what makes
            // the fallback possible: the Future is polled through the
            // reference, so if the timeout fires, this task's own `handle`
            // binding is still ours to call `.abort()` on afterward —
            // unlike moving `handle` into the timeout future outright, which
            // would drop (and thus orphan) it on timeout with nothing left
            // to abort.
            if tokio::time::timeout(LISTENER_SHUTDOWN_TIMEOUT, &mut handle)
                .await
                .is_err()
            {
                warn!("Mesh listener did not shut down gracefully in time — aborting it");
                handle.abort();
                let _ = handle.await;
            }
        }
        if let Some(handle) = self.deadman_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        if let Some(handle) = self.scheduler_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        if let Some(handle) = self.block_announcer_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        if let Some(handle) = self.presence_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        // Recreate the cmd channel so a subsequent start() has a fresh
        // receiver. The listener task took ownership of the old receiver
        // on its previous run and dropped it when the task ended, so
        // without this swap the next start() hits "Command channel
        // already consumed". Swapping the sender inside MeshState means
        // every Arc holder transparently picks up the new channel.
        let (new_tx, new_rx) = tokio::sync::mpsc::channel(32);
        *self.state.cmd_tx.write().await = new_tx;
        self.cmd_rx = Some(new_rx);
        info!("Mesh service stopped");
    }

    /// Get current mesh status.
    pub async fn status(&self) -> MeshStatus {
        let mut status = self.state.status.read().await.clone();
        // The operator-configured LoRa region isn't part of the live session
        // state (it's config, read once at session start) — compose it in
        // here rather than threading it through the session's shared status
        // writes, for the Device tab (#8) to display.
        status.region = self.config.lora_region.clone();
        status
    }

    /// Get a reference to the shared mesh state.
    pub fn state(&self) -> &Arc<listener::MeshState> {
        &self.state
    }

    /// Get list of discovered peers.
    pub async fn peers(&self) -> Vec<MeshPeer> {
        self.state.peers.read().await.values().cloned().collect()
    }

    /// Refuse to probe the port the live session currently occupies (the
    /// probe would steal the serial port from under the session); a
    /// detected-but-not-connected port is fair game, accepting a benign race
    /// with the reconnect loop (whichever loses just retries). Split out from
    /// the actual probe on purpose — see `probe_device`'s doc comment.
    pub async fn ensure_probe_allowed(&self, path: &str) -> Result<()> {
        let status = self.state.status.read().await;
        if status.device_connected {
            if let Some(active) = status.device_path.as_deref() {
                // Compare canonical paths: /dev/mesh-radio is a symlink to the
                // ttyUSB*/ttyACM* node, and a probe through the alias would
                // still open the very tty the live session is holding.
                let canon = |p: &str| {
                    std::fs::canonicalize(p).unwrap_or_else(|_| std::path::PathBuf::from(p))
                };
                if canon(active) == canon(path) {
                    anyhow::bail!("{path} is the active mesh radio — already connected");
                }
            }
        }
        Ok(())
    }

    /// Get message history.
    pub async fn messages(&self, limit: Option<usize>) -> Vec<MeshMessage> {
        let messages = self.state.messages.read().await;
        let limit = limit.unwrap_or(MAX_MESSAGES_DEFAULT);
        // Return in chronological order (oldest first) — take last N items
        let len = messages.len();
        let skip = len.saturating_sub(limit);
        messages.iter().skip(skip).cloned().collect()
    }

    /// Full in-memory state dump for debugging. Returns peers, all messages,
    /// status, shared-secret peer ids (not the secrets), encrypt_relay flag,
    /// and stego mode. Intended for development/smoke-test use only — don't
    /// call this on a hot path.
    pub async fn debug_dump(&self) -> serde_json::Value {
        let status = self.state.status.read().await.clone();
        let peers: Vec<_> = self.state.peers.read().await.values().cloned().collect();
        let messages: Vec<_> = self.state.messages.read().await.iter().cloned().collect();
        let secret_peer_ids: Vec<u32> = self
            .state
            .shared_secrets
            .read()
            .await
            .keys()
            .copied()
            .collect();
        serde_json::json!({
            "status": status,
            "peers": peers,
            "peer_count": peers.len(),
            "messages": messages,
            "message_count": messages.len(),
            "secret_peer_ids": secret_peer_ids,
            "encrypt_relay": self.state.encrypt_relay,
            "stego_mode": format!("{:?}", self.state.stego_mode),
        })
    }

    /// Resolve a peer's 6-byte public-key prefix for mesh addressing.
    async fn peer_dest_prefix(&self, contact_id: u32) -> Result<[u8; 6]> {
        let peers = self.state.peers.read().await;
        let peer = peers
            .get(&contact_id)
            .ok_or_else(|| anyhow::anyhow!("Peer not found"))?;
        // Cross-transport twin resolution: callers frequently hold the
        // FEDERATION twin's contact_id (the UI's merged conversation row),
        // whose pubkey_hex is the Archipelago ed25519 key — NOT a radio
        // routing key. Sending a Reticulum resource with that prefix fails
        // with "Unknown Reticulum prefix" (observed live 2026-07-28,
        // image-over-LoRa to a merged contact). Route via the radio twin —
        // same arch identity, radio-range id — whose pubkey_hex is the
        // actual over-the-air routing key (RNS dest hash / firmware key).
        let radio_peer = if peer.contact_id >= FEDERATION_CONTACT_ID_BASE {
            peer.arch_pubkey_hex
                .as_deref()
                .and_then(|arch| {
                    peers.values().find(|p| {
                        p.contact_id < FEDERATION_CONTACT_ID_BASE
                            && p.arch_pubkey_hex.as_deref() == Some(arch)
                    })
                })
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Peer is federation-only (no radio twin) — not reachable over the radio"
                    )
                })?
        } else {
            peer
        };
        let pubkey_hex = radio_peer
            .pubkey_hex
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Peer has no public key"))?;
        let pubkey_bytes =
            hex::decode(pubkey_hex).map_err(|_| anyhow::anyhow!("Invalid peer public key"))?;
        if pubkey_bytes.len() < 6 {
            anyhow::bail!("Peer public key too short");
        }
        let mut dest_prefix = [0u8; 6];
        dest_prefix.copy_from_slice(&pubkey_bytes[..6]);
        Ok(dest_prefix)
    }

    /// True if `contact_id` is reachable over the mesh radio right now — the
    /// same peer/twin resolution `peer_dest_prefix` performs, exposed as a
    /// cheap bool so RPC handlers can gate radio-only transports (LXMF
    /// native image, Reticulum resource transfer) without duplicating the
    /// twin-resolution logic. A federation-only contact_id with no matching
    /// radio twin returns false here — offering "resource-mesh" or native
    /// image to such a peer sends it straight into `peer_dest_prefix`'s
    /// "federation-only (no radio twin)" error (picture-send from a
    /// federation-only contact, 2026-08-07).
    pub async fn has_radio_route(&self, contact_id: u32) -> bool {
        self.peer_dest_prefix(contact_id).await.is_ok()
    }

    /// Split an oversized wire payload into MC-framed base64 chunks and send
    /// each via the mesh device. Matches the receive-side reassembly in
    /// `mesh/listener/decode.rs::handle_chunked_frame` (header `MCIIXXTT`,
    /// 20-chunk cap, 152 base64 chars per chunk). The caller must ensure
    /// the peer exists and the device is connected.
    #[allow(dead_code)]
    async fn send_chunked_payload(&self, contact_id: u32, payload: Vec<u8>) -> Result<()> {
        use base64::Engine;
        const HEADER_LEN: usize = 8; // MC + msg_id(2) + chunk_idx(2) + total(2)
        const MAX_CHUNK_B64: usize = protocol::MAX_MESSAGE_LEN - HEADER_LEN;
        const MAX_CHUNKS: u8 = 20;

        let b64 = base64::engine::general_purpose::STANDARD.encode(&payload);
        let total_chunks = b64.len().div_ceil(MAX_CHUNK_B64) as u8;
        if total_chunks == 0 || total_chunks > MAX_CHUNKS {
            anyhow::bail!(
                "Payload too large to chunk: {} bytes → {} chunks (max {})",
                payload.len(),
                total_chunks,
                MAX_CHUNKS
            );
        }

        // Pick a 1-byte msg_id. Use the low 8 bits of the unix nanos; not
        // cryptographically unique but collisions within a 120s reassembly
        // window are astronomically unlikely for normal send rates.
        let msg_id: u8 =
            (chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64 & 0xFF) as u8;

        let dest_prefix = self.peer_dest_prefix(contact_id).await?;
        for chunk_idx in 0..total_chunks {
            let start = chunk_idx as usize * MAX_CHUNK_B64;
            let end = (start + MAX_CHUNK_B64).min(b64.len());
            let chunk = &b64[start..end];
            let frame = format!(
                "MC{:02X}{:02X}{:02X}{}",
                msg_id, chunk_idx, total_chunks, chunk
            );
            self.state
                .send_cmd(listener::MeshCommand::SendText {
                    dest_pubkey_prefix: dest_prefix,
                    payload: frame.into_bytes(),
                })
                .await
                .map_err(|_| anyhow::anyhow!("Mesh listener not running"))?;
        }
        tracing::info!(
            contact_id,
            msg_id,
            chunks = total_chunks,
            bytes = payload.len(),
            "Sent chunked payload over mesh"
        );
        Ok(())
    }

    /// Send raw wire payload bytes to a peer (no Sent-record bookkeeping).
    /// Callers are responsible for storing the MeshMessage record afterwards.
    ///
    /// Oversized payloads (>LoRa per-frame budget) are handled by the lower
    /// `send_dm_via_channel` layer, which base64-encodes + MC-frame-chunks
    /// the bytes into 80-char pieces and reassembles on the receiver. We
    /// must NOT chunk here as well — doing so double-chunks and produces
    /// bytes the receiver can't decode.
    async fn send_raw_payload(&self, contact_id: u32, payload: Vec<u8>) -> Result<()> {
        let status = self.state.status.read().await;
        if !status.device_connected {
            anyhow::bail!("No mesh device connected");
        }
        drop(status);

        let dest_prefix = self.peer_dest_prefix(contact_id).await?;

        self.state
            .send_cmd(listener::MeshCommand::SendText {
                dest_pubkey_prefix: dest_prefix,
                payload,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Mesh listener not running"))?;
        Ok(())
    }

    /// Send a typed envelope wire payload and record a rich Sent MeshMessage.
    /// Used by RPC handlers that transmit invoice/coordinate/alert/etc. so the
    /// UI sees a proper rich Sent card instead of garbage wire-byte plaintext.
    ///
    /// `sender_seq` should match the seq encoded into the TypedEnvelope by the
    /// caller — the RPC handler allocates it via `next_send_seq` before
    /// building the envelope and threads it through here so the local Sent
    /// record carries the same MessageKey the receiver will see.
    pub async fn send_typed_wire(
        &self,
        contact_id: u32,
        wire: Vec<u8>,
        type_label: &str,
        display_text: &str,
        typed_payload: Option<serde_json::Value>,
        sender_seq: u64,
    ) -> Result<MeshMessage> {
        // Federation-synthetic contacts (high bit set) don't exist in the
        // meshcore firmware contact table, so LoRa send would fail at
        // `peer_dest_prefix`. Any envelope larger than the LoRa frame budget
        // also needs the federation path. In both cases we look up the
        // peer's onion (by archipelago pubkey first, then by DID) and POST
        // over Tor; otherwise the send falls through to LoRa.
        let is_federation_synthetic = contact_id & 0x8000_0000 != 0;
        let exceeds_lora = wire.len() > protocol::MAX_MESSAGE_LEN;
        // Mesh-preferred routing with a federation fallback. A normal radio
        // contact is delivered over LoRa (preferred — free, local, no internet).
        // But if that contact is the same node as a federated peer — we know its
        // archipelago identity (`arch_pubkey_hex`) → onion — AND it is NOT
        // currently reachable over the radio (out of LoRa range, e.g. a peer on
        // another continent), route the message over the federation transport
        // (FIPS→Tor) instead of handing it to a radio that physically cannot
        // deliver it. Reachable radio peers stay on the mesh; oversized
        // envelopes (file shares etc.) always take the federation path.
        let radio_federated_unreachable = !is_federation_synthetic && !exceeds_lora && {
            let peers = self.state.peers.read().await;
            peers
                .get(&contact_id)
                .map(|p| !p.reachable && p.arch_pubkey_hex.is_some())
                .unwrap_or(false)
        };
        // Transport policy: LoRa first when it can actually carry the message,
        // then FIPS, then Tor. A federation-synthetic id (what the UI's merged
        // conversation holds) used to ALWAYS take the federation path, even
        // when the very same node was sitting one LoRa hop away — so chats
        // between two radio-equipped nodes silently rode FIPS/Tor. If the
        // federation contact has a REACHABLE radio twin (same archipelago
        // identity, radio-range id) and the payload fits the radio, skip the
        // federation branch: the fall-through LoRa path twin-resolves the
        // routing key via peer_dest_prefix.
        let device_connected = self.state.status.read().await.device_connected;
        let radio_twin_reachable =
            is_federation_synthetic && !exceeds_lora && device_connected && {
                let peers = self.state.peers.read().await;
                peers
                    .get(&contact_id)
                    .and_then(|p| p.arch_pubkey_hex.clone())
                    .map(|arch| {
                        peers.values().any(|p| {
                            p.contact_id < FEDERATION_CONTACT_ID_BASE
                                && p.reachable
                                && p.arch_pubkey_hex.as_deref() == Some(arch.as_str())
                        })
                    })
                    .unwrap_or(false)
            };
        let mesh_only_mode = load_config(&self.data_dir)
            .await
            .ok()
            .and_then(|cfg| cfg.mesh_only_mode)
            .unwrap_or(false);
        if radio_twin_reachable && !mesh_only_mode {
            tracing::info!(
                contact_id,
                bytes = wire.len(),
                "Radio-first routing: federation contact has a reachable radio twin — sending over LoRa"
            );
        }
        if !mesh_only_mode
            && !radio_twin_reachable
            && (is_federation_synthetic || exceeds_lora || radio_federated_unreachable)
        {
            // Resolve the peer's pubkey/did. Prefer the live mesh peer table,
            // but fall back to federation storage for federation-synthetic ids
            // that were never seeded into `state.peers` — e.g. a radio-less
            // node where the mesh device table is empty. Without this fallback
            // chatting a federation contact bails "Unknown federation peer"
            // even though we know its onion from nodes.json.
            let from_table = {
                let peers = self.state.peers.read().await;
                peers.get(&contact_id).map(|p| {
                    // Resolve via the archipelago IDENTITY key (not the firmware
                    // routing key) — that's what matches the peer's onion entry
                    // in nodes.json for the federation lookup below.
                    (
                        p.arch_pubkey_hex.clone().or_else(|| p.pubkey_hex.clone()),
                        p.did.clone(),
                    )
                })
            };
            let (peer_pubkey, peer_did) = match from_table {
                Some(v) => v,
                None if is_federation_synthetic => {
                    let nodes = crate::federation::load_nodes(&self.data_dir)
                        .await
                        .unwrap_or_default();
                    match nodes
                        .iter()
                        .find(|n| federation_peer_contact_id(&n.pubkey) == contact_id)
                    {
                        Some(n) => (Some(n.pubkey.clone()), Some(n.did.clone())),
                        None => anyhow::bail!("Unknown federation peer {}", contact_id),
                    }
                }
                None => (None, None),
            };
            let nodes = crate::federation::load_nodes(&self.data_dir)
                .await
                .unwrap_or_default();
            let onion = peer_pubkey
                .as_ref()
                .and_then(|pk| {
                    nodes
                        .iter()
                        .find(|n| &n.pubkey == pk)
                        .map(|n| n.onion.clone())
                })
                .or_else(|| {
                    peer_did
                        .as_ref()
                        .and_then(|d| nodes.iter().find(|n| &n.did == d).map(|n| n.onion.clone()))
                });
            if let Some(onion) = onion {
                return self
                    .send_typed_wire_via_federation(
                        contact_id,
                        &onion,
                        wire,
                        type_label,
                        display_text,
                        typed_payload,
                        sender_seq,
                    )
                    .await;
            }
            // No federation path — fall through to send_raw_payload, which
            // hands the wire to the lower DM-via-channel layer. That layer
            // (`send_dm_via_channel` in listener/session.rs) handles both
            // single-frame and chunked transmission internally; we must NOT
            // pre-chunk here as well or the receiver sees garbage.
        } else if mesh_only_mode
            && (is_federation_synthetic || exceeds_lora || radio_federated_unreachable)
        {
            tracing::info!(
                contact_id,
                bytes = wire.len(),
                is_federation_synthetic,
                exceeds_lora,
                radio_federated_unreachable,
                "Off-grid mode active; forcing mesh message over LoRa only"
            );
        }
        self.send_raw_payload(contact_id, wire).await?;
        let device_type = self.state.status.read().await.device_type;
        let radio_transport = radio_transport_label(device_type);
        Ok(self
            .record_sent_typed(
                contact_id,
                type_label,
                display_text,
                typed_payload,
                sender_seq,
                Some(radio_transport.to_string()),
                // Archy↔archy typed envelopes over LoRa are identity-signed; the
                // radio E2E flag (meshtastic PKI / meshcore session) isn't
                // threaded to the send side yet, so don't over-claim E2E here —
                // except Reticulum/LXMF, which is unconditionally E2E on every
                // send regardless of peer/session state (see send_message).
                device_type == DeviceType::Reticulum,
            )
            .await)
    }

    /// Send an image via native LXMF `FIELD_IMAGE` instead of our own typed
    /// envelope — for a stock (non-archy) peer that can't decode our CBOR
    /// wire format. Caller (the RPC layer) gates this on
    /// `!is_archy_peer(contact_id)`; low-level "just send the bytes" shape
    /// mirroring `send_raw_payload` — does NOT record a Sent MeshMessage
    /// itself, callers use `record_sent_typed` same as the typed-envelope
    /// paths so the Sent card renders identically regardless of which wire
    /// format actually went out.
    pub async fn send_native_image(
        &self,
        contact_id: u32,
        mime: &str,
        bytes: Vec<u8>,
        caption: Option<String>,
    ) -> Result<()> {
        let status = self.state.status.read().await;
        if !status.device_connected {
            anyhow::bail!("No mesh device connected");
        }
        drop(status);
        let dest_prefix = self.peer_dest_prefix(contact_id).await?;
        self.state
            .send_cmd(listener::MeshCommand::SendNativeImage {
                dest_pubkey_prefix: dest_prefix,
                mime: mime.to_string(),
                bytes,
                caption,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Mesh listener not running"))?;
        Ok(())
    }

    /// Send a typed envelope over a dedicated Reticulum RNS Resource transfer
    /// (`MeshCommand::SendResource`) instead of the small inline-chunk path
    /// `send_typed_wire`/`send_raw_payload` uses. Callers (the `mesh.send-content-inline`
    /// RPC handler) are responsible for only reaching this when the active
    /// device is actually Reticulum and the payload fits the
    /// `RETICULUM_RESOURCE_MAX` budget — see `mesh.transport-advice`'s
    /// `"resource-mesh"` tier, the single source of truth for that decision.
    /// Mirrors `send_typed_wire`'s signature/return shape so RPC call sites
    /// can switch between the two paths without restructuring.
    pub async fn send_content_resource(
        &self,
        contact_id: u32,
        wire: Vec<u8>,
        type_label: &str,
        display_text: &str,
        typed_payload: Option<serde_json::Value>,
        sender_seq: u64,
    ) -> Result<MeshMessage> {
        let status = self.state.status.read().await;
        if !status.device_connected {
            anyhow::bail!("No mesh device connected");
        }
        drop(status);

        let dest_prefix = self.peer_dest_prefix(contact_id).await?;
        self.state
            .send_cmd(listener::MeshCommand::SendResource {
                dest_pubkey_prefix: dest_prefix,
                payload: wire,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Mesh listener not running"))?;

        let device_type = self.state.status.read().await.device_type;
        let radio_transport = radio_transport_label(device_type);
        Ok(self
            .record_sent_typed(
                contact_id,
                type_label,
                display_text,
                typed_payload,
                sender_seq,
                Some(radio_transport.to_string()),
                // Reticulum/LXMF is unconditionally E2E on every send — same
                // reasoning as send_message's native-text path. This method
                // is Reticulum-only by construction (callers gate on
                // device_type before reaching it), so this is never wrong.
                true,
            )
            .await)
    }

    /// Send a typed envelope to a peer via the federation (Tor) path rather
    /// than LoRa. Used when the envelope exceeds LoRa's per-frame budget —
    /// ContentRef is the canonical example, at 400+ bytes with thumb + cap.
    /// The peer's onion must already be in federation storage. Records a
    /// Sent MeshMessage locally on success so the UI gets the rich card.
    ///
    /// This does NOT use chunking and does NOT go through the mesh radio —
    /// it is a straight HTTP POST over Tor to the peer's
    /// `/archipelago/mesh-typed` endpoint.
    ///
    /// The POST itself is fire-and-forget: we record the Sent MeshMessage
    /// synchronously (so the UI sees the bubble immediately) and spawn the
    /// Tor HTTP in the background. Tor circuit setup is 1–5s per envelope
    /// and blocking the RPC on it made `mesh.send` feel laggy — especially
    /// over a held Enter key. Delivery failures still surface via the
    /// absent read-receipt path: `delivered` stays `false` on the Sent
    /// record if the peer never echoes back a receipt.
    pub async fn send_typed_wire_via_federation(
        &self,
        contact_id: u32,
        peer_onion: &str,
        wire: Vec<u8>,
        type_label: &str,
        display_text: &str,
        typed_payload: Option<serde_json::Value>,
        sender_seq: u64,
    ) -> Result<MeshMessage> {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        use ed25519_dalek::Signer;

        // Sign the raw wire bytes so the receiver can attribute the envelope
        // to our pubkey even when it arrives over federation/FIPS/Tor rather
        // than the radio. Signature covers the wire only — the receiver
        // re-hashes.
        let signature = hex::encode(self.signing_key.sign(&wire).to_bytes());
        let wire_b64 = BASE64.encode(&wire);
        let body = serde_json::json!({
            "from_pubkey": self.our_ed_pubkey_hex,
            "from_name": self.our_did,
            "typed_envelope_b64": wire_b64,
            "signature": signature,
        });

        // Record Sent now so the UI bubble appears immediately.
        let msg = self
            .record_sent_typed(
                contact_id,
                type_label,
                display_text,
                typed_payload,
                sender_seq,
                // Transport is finalized below once the background send resolves
                // FIPS vs Tor; mark E2E now — a federation envelope is
                // identity-signed and rides an encrypted transport.
                None,
                true,
            )
            .await;

        // Fire the send in the background. FIPS is preferred when the peer
        // is federated and running fips; Tor is the fallback. Failures are
        // logged but do not propagate — caller already has the Sent
        // MeshMessage and the UI's delivery indicator tracks the receipt.
        let peer_onion_owned = peer_onion.to_string();
        let data_dir_owned = self.data_dir.clone();
        // Finalize the Sent record's transport pill once we know which leg
        // (FIPS/Tor) actually delivered it.
        let state_for_transport = self.state.clone();
        let sent_msg_id = msg.id;
        tokio::spawn(async move {
            let fips_npub =
                crate::federation::fips_npub_for_onion(&data_dir_owned, &peer_onion_owned).await;
            let req = crate::fips::dial::PeerRequest::new(
                fips_npub.as_deref(),
                &peer_onion_owned,
                "/archipelago/mesh-typed",
            )
            .service(crate::settings::transport::PeerService::Messaging)
            .timeout(std::time::Duration::from_secs(120))
            // Fast-fail a FIPS path the peer isn't reachable on (the common case
            // for remote/Tailscale peers that share no FIPS overlay with us) so
            // the Tor fallback delivers the message in ~3-5s instead of the send
            // hanging on FIPS. FIPS-reachable peers connect in <1s and still use
            // it; only an unreachable FIPS path is short-circuited. Matches the
            // federation-sync fix. 8s ≈ the FIPS connect_timeout headroom.
            .fips_timeout(std::time::Duration::from_secs(8));
            match req.send_json(&body).await {
                Ok((resp, transport)) if resp.status().is_success() => {
                    tracing::debug!(contact_id, transport = %transport, "Federation envelope delivered");
                    // Tag the Sent bubble with the leg that delivered it (the
                    // transport pill: "fips" / "tor").
                    let mut messages = state_for_transport.messages.write().await;
                    if let Some(m) = messages.iter_mut().find(|m| m.id == sent_msg_id) {
                        m.transport = Some(transport.to_string());
                    }
                }
                Ok((resp, transport)) => warn!(
                    contact_id,
                    status = %resp.status(),
                    transport = %transport,
                    "Peer rejected federation-routed envelope"
                ),
                Err(e) => warn!(contact_id, "Federation POST failed: {}", e),
            }
        });

        Ok(msg)
    }

    /// Inject a typed envelope received over federation (Tor) into MeshState
    /// as if it had arrived over the mesh radio. Looks up contact_id by
    /// matching pubkey_hex against known mesh peers; falls back to a
    /// synthetic id derived from the pubkey bytes so the UI can still
    /// address the chat (will render as a new "peer" if not in the list).
    pub async fn inject_typed_from_federation(
        &self,
        from_pubkey_hex: &str,
        from_name: Option<&str>,
        wire: Vec<u8>,
    ) -> Result<()> {
        let envelope = crate::mesh::message_types::TypedEnvelope::from_wire(&wire)?;
        // Federation and mesh have disjoint identity namespaces: a LoRa
        // mesh contact carries meshcore's firmware-issued pubkey, not the
        // archipelago ed25519 key. So we cannot rely on matching pubkeys
        // across transports. Instead, every federation peer has a stable
        // synthetic mesh contact_id derived from its archipelago pubkey,
        // and is upserted into mesh state the first time we hear from them.
        let (federation_did, federation_name) = {
            let nodes = crate::federation::load_nodes(&self.data_dir)
                .await
                .unwrap_or_default();
            nodes
                .into_iter()
                .find(|n| n.pubkey == from_pubkey_hex)
                .map(|n| (Some(n.did), n.name))
                .unwrap_or((None, None))
        };
        // If the federation list knows this sender, use its DID; otherwise
        // fall back to `from_name` (the sender's self-reported DID from the
        // envelope body, signature-verified upstream in handle_mesh_typed_relay).
        let effective_did = federation_did
            .or_else(|| from_name.map(|s| s.to_string()))
            .unwrap_or_else(|| {
                format!(
                    "did:unknown:{}",
                    &from_pubkey_hex[..from_pubkey_hex.len().min(16)]
                )
            });
        let display_name = federation_name
            .clone()
            .or_else(|| from_name.map(|s| s.to_string()))
            .unwrap_or_else(|| "federation peer".to_string());
        let contact_id = upsert_federation_peer(
            &self.state,
            from_pubkey_hex,
            &effective_did,
            Some(&display_name),
        )
        .await;
        // The inbound HTTP gives no FIPS-vs-Tor signal, so label the message
        // with the leg most recently used with this peer (federation storage's
        // `last_transport`), defaulting to Tor. Federation envelopes are E2E
        // (identity-signed over an encrypted transport).
        let transport_label = {
            let nodes = crate::federation::load_nodes(&self.data_dir)
                .await
                .unwrap_or_default();
            nodes
                .iter()
                .find(|n| n.pubkey == from_pubkey_hex)
                .and_then(|n| n.last_transport.clone())
                .filter(|t| t == "fips" || t == "tor")
                .unwrap_or_else(|| "tor".to_string())
        };
        let before = listener::dispatch::max_message_id(&self.state).await;
        listener::dispatch::handle_typed_envelope_direct(
            &self.state,
            contact_id,
            &display_name,
            envelope,
        )
        .await;
        listener::dispatch::stamp_received_transport(
            &self.state,
            contact_id,
            before,
            &transport_label,
            true,
        )
        .await;
        Ok(())
    }

    /// Allocate the next outbound seq for a target. Convenience passthrough
    /// to MeshState::next_send_seq; used by RPC handlers before encoding a
    /// TypedEnvelope so the seq on the wire matches the Sent record.
    pub async fn next_send_seq(&self, target: u32) -> u64 {
        self.state.next_send_seq(target).await
    }

    /// Look up a stored MeshMessage by its local `id`. Used by the Forward
    /// RPC to pull an existing record's typed payload for re-encoding.
    pub async fn find_message_by_id(&self, id: u64) -> Option<MeshMessage> {
        let messages = self.state.messages.read().await;
        messages.iter().find(|m| m.id == id).cloned()
    }

    /// Drop a stored MeshMessage by local id. Used after sending control
    /// envelopes (read receipts) so they don't surface as their own
    /// bubbles in the chat history. The wire frame is already on its way;
    /// this just prunes the local Sent record.
    pub async fn drop_message_by_id(&self, id: u64) {
        let mut messages = self.state.messages.write().await;
        messages.retain(|m| m.id != id);
    }

    /// Apply an Edit locally to any own-Sent message matching `sender_seq`
    /// (sender_pubkey is implicit = self). Rewrites `plaintext` and appends
    /// an `edited_at` marker on `typed_payload` so the UI can show "(edited)".
    /// Best-effort: missing target is silently ignored.
    pub async fn apply_local_edit(&self, target_seq: u64, new_text: &str, edited_at: u32) {
        let mut messages = self.state.messages.write().await;
        for m in messages.iter_mut() {
            if m.sender_seq == Some(target_seq)
                && matches!(m.direction, crate::mesh::types::MessageDirection::Sent)
            {
                m.plaintext = new_text.to_string();
                let mut obj = match m.typed_payload.take() {
                    Some(serde_json::Value::Object(o)) => o,
                    _ => serde_json::Map::new(),
                };
                obj.insert("edited_at".to_string(), serde_json::json!(edited_at));
                obj.insert("text".to_string(), serde_json::json!(new_text));
                m.typed_payload = Some(serde_json::Value::Object(obj));
                break;
            }
        }
    }

    /// Apply a Delete tombstone locally to an own-Sent message.
    pub async fn apply_local_delete(&self, target_seq: u64) {
        let mut messages = self.state.messages.write().await;
        for m in messages.iter_mut() {
            if m.sender_seq == Some(target_seq)
                && matches!(m.direction, crate::mesh::types::MessageDirection::Sent)
            {
                m.plaintext = "🗑 message deleted".to_string();
                m.typed_payload = Some(serde_json::json!({ "deleted": true }));
                m.message_type = "delete".to_string();
                break;
            }
        }
    }

    /// Broadcast a typed envelope wire payload on a mesh channel and record a
    /// rich Sent MeshMessage. Bytes are sent directly — do NOT utf8_lossy-encode
    /// binary envelope bytes before handing them here.
    pub async fn send_channel_typed_wire(
        &self,
        channel: u8,
        wire: Vec<u8>,
        type_label: &str,
        display_text: &str,
        typed_payload: Option<serde_json::Value>,
        sender_seq: u64,
    ) -> Result<MeshMessage> {
        let status = self.state.status.read().await;
        if !status.device_connected {
            anyhow::bail!("No mesh device connected");
        }
        drop(status);

        if wire.len() > protocol::MAX_MESSAGE_LEN {
            anyhow::bail!(
                "Message too large for LoRa: {} bytes (max {})",
                wire.len(),
                protocol::MAX_MESSAGE_LEN
            );
        }

        self.state
            .send_cmd(listener::MeshCommand::BroadcastChannel {
                channel,
                payload: wire,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Mesh listener not running"))?;

        let chan_contact_id = u32::MAX - (channel as u32);
        let chan_name = format!("Channel {}", channel);
        let msg_id = self.state.next_id().await;
        let radio_transport = radio_transport_label(self.state.status.read().await.device_type);
        let msg = MeshMessage {
            id: msg_id,
            direction: MessageDirection::Sent,
            peer_contact_id: chan_contact_id,
            peer_name: Some(chan_name),
            plaintext: display_text.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            delivered: false,
            // Channel broadcasts use the shared channel PSK, not per-identity
            // E2E — so not an E2E message, but it does travel over the radio.
            encrypted: false,
            transport: Some(radio_transport.to_string()),
            message_type: type_label.to_string(),
            typed_payload,
            sender_pubkey: Some(self.our_ed_pubkey_hex.clone()),
            sender_seq: Some(sender_seq),
        };
        self.state.store_message(msg.clone()).await;
        {
            let mut status = self.state.status.write().await;
            status.messages_sent += 1;
        }
        Ok(msg)
    }

    /// Send a text message to a peer. Wraps the text in a typed `Text`
    /// envelope (variant 0) with an allocated `sender_seq`, so the resulting
    /// MeshMessage carries a stable MessageKey — this is what makes replies
    /// and reactions addressable against plain text bubbles.
    pub async fn send_message(&self, contact_id: u32, text: &str) -> Result<MeshMessage> {
        use crate::mesh::message_types::{MeshMessageType, TypedEnvelope};
        let seq = self.state.next_send_seq(contact_id).await;
        let device_type = self.state.status.read().await.device_type;
        let archy = self.is_archy_peer(contact_id).await;

        // Transport choice is DEVICE-AWARE so we fix Meshtastic without regressing
        // Meshcore:
        //   • Meshtastic (any peer) → plain text native DM on TEXT_MESSAGE_APP. The
        //     firmware end-to-end (PKC/Curve25519) encrypts a directed DM to any
        //     peer whose public key it knows (archy peers exchange them via
        //     NodeInfo), so it's delivered E2E and shows as chat on every client.
        //     Meshtastic firmware 2.7.x will NOT deliver our opaque binary typed
        //     envelope as a message (PRIVATE_APP is opaque app-data; a base64
        //     envelope overflows one LoRa frame and chunk-fails) — wrapping text
        //     is exactly what silently broke archy↔archy Meshtastic LoRa.
        //   • Meshcore/Reticulum archy peer → keep the rich signed typed envelope.
        //     Meshcore frames are binary-safe (no UTF-8 mangling) and Reticulum/LXMF
        //     is binary-safe and high-capacity too; both carry their own transport
        //     E2E plus our signature for `!ai` auth / seq reply addressing, so the
        //     envelope works there and we must not drop it.
        //   • Meshcore stock client → plain text (can't decode our envelope).
        // Rich typed messages (invoice/coordinate/reaction/…) always use the
        // typed-wire path via `send_typed_wire`; only plain Text is routed here.
        // A federation-synthetic contact ALWAYS takes the typed path, whatever
        // radio (if any) is attached. `send_typed_wire` is the only routing
        // that knows about FIPS/Tor, and it still prefers a reachable LoRa
        // twin when the payload fits — so this loses no radio-first behaviour.
        //
        // Without this, a plain text message to a federated peer fell through
        // to `peer_dest_prefix`, which resolves a RADIO routing key. On a node
        // running Meshtastic — or with no radio at all — that fails, which is
        // why peering a node was not enough to message it: you had to meet it
        // over LoRa first so a radio twin existed to route through. Federation
        // peers are reachable off-radio by definition (that is what
        // `upsert_federation_peer` records with `reachable: true`), so the
        // transport choice must not depend on which radio is plugged in.
        let is_federation_contact = contact_id & 0x8000_0000 != 0;
        let use_typed_envelope = archy
            && (is_federation_contact
                || matches!(device_type, DeviceType::Meshcore | DeviceType::Reticulum));
        if use_typed_envelope {
            // Sign with our archipelago identity so the receiver can authenticate
            // us over LoRa (verifies against our bound `arch_pubkey_hex`). `with_seq`
            // is applied after signing — seq is not covered by the signature.
            let envelope = TypedEnvelope::new_signed(
                MeshMessageType::Text,
                text.as_bytes().to_vec(),
                &self.signing_key,
            )
            .with_seq(seq);
            let wire = envelope.to_wire()?;
            return self
                .send_typed_wire(contact_id, wire, "text", text, None, seq)
                .await;
        }

        let dest_prefix = self.peer_dest_prefix(contact_id).await?;
        self.state
            .send_cmd(listener::MeshCommand::SendNativeText {
                dest_pubkey_prefix: dest_prefix,
                payload: text.as_bytes().to_vec(),
            })
            .await
            .map_err(|_| anyhow::anyhow!("Mesh listener not running"))?;
        // The firmware PKI-encrypts a directed DM to any peer whose key it knows;
        // archy peers always exchange keys, so mark those Sent rows E2E so the
        // pill shows immediately. A non-archy stock peer (e.g. 3ccc) can also be
        // PKC-capable once we've learned its NodeInfo public key — OR that in too
        // so the pill isn't archy-only. (The receiver independently stamps E2E
        // from the radio's `pki_encrypted` flag, so an inbound row is accurate
        // regardless.)
        //
        // Reticulum/LXMF has no such conditional: every send is encrypted to the
        // destination's identity key by the LXMF router itself, archy peer or
        // not — so it's unconditionally E2E rather than gated on `archy`/`pkc_capable`
        // (which is a Meshtastic-only concept; Reticulum contacts never set it).
        let pkc_capable = self.peer_pkc_capable(contact_id).await;
        let encrypted = device_type == DeviceType::Reticulum || archy || pkc_capable;
        Ok(self
            .record_sent_typed(
                contact_id,
                "text",
                text,
                None,
                seq,
                Some(radio_transport_label(device_type).to_string()),
                encrypted,
            )
            .await)
    }

    /// Whether `contact_id` is an archipelago peer (vs a stock meshcore client).
    /// Federation-synthetic ids are always archy; radio contacts count as archy
    /// only once we've learned their archipelago identity (DID or x25519 key,
    /// from federation seeding or an identity exchange). Stock clients have
    /// neither, so we send them plain text rather than typed envelopes.
    pub(crate) async fn is_archy_peer(&self, contact_id: u32) -> bool {
        if contact_id & 0x8000_0000 != 0 {
            return true;
        }
        let peers = self.state.peers.read().await;
        peers
            .get(&contact_id)
            .map(|p| p.did.is_some() || p.x25519_pubkey.is_some())
            .unwrap_or(false)
    }

    /// Whether `contact_id`'s real radio PKI (Curve25519) key is known, so the
    /// firmware delivers a directed DM to it end-to-end encrypted even though
    /// it's not an archipelago peer (e.g. stock Meshtastic peer 3ccc). Stamped
    /// onto `MeshPeer::pkc_capable` by `refresh_contacts` from the driver's
    /// `get_contacts()`.
    async fn peer_pkc_capable(&self, contact_id: u32) -> bool {
        self.state
            .peers
            .read()
            .await
            .get(&contact_id)
            .map(|p| p.pkc_capable)
            .unwrap_or(false)
    }

    /// Record a Sent MeshMessage for a typed envelope that has already been
    /// transmitted by the caller. Used by the RPC layer after sending
    /// invoice/coordinate/alert/etc. so the UI gets a proper rich Sent card
    /// instead of a Sent record containing the raw wire bytes as plaintext.
    pub async fn record_sent_typed(
        &self,
        contact_id: u32,
        type_label: &str,
        display_text: &str,
        typed_payload: Option<serde_json::Value>,
        sender_seq: u64,
        transport: Option<String>,
        encrypted: bool,
    ) -> MeshMessage {
        let msg_id = self.state.next_id().await;
        let peer_name = self
            .state
            .peers
            .read()
            .await
            .get(&contact_id)
            .map(|p| p.advert_name.clone());
        let msg = MeshMessage {
            id: msg_id,
            direction: MessageDirection::Sent,
            peer_contact_id: contact_id,
            peer_name,
            plaintext: display_text.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            delivered: false,
            encrypted,
            transport,
            message_type: type_label.to_string(),
            typed_payload,
            sender_pubkey: Some(self.our_ed_pubkey_hex.clone()),
            sender_seq: Some(sender_seq),
        };
        self.state.store_message(msg.clone()).await;
        {
            let mut status = self.state.status.write().await;
            status.messages_sent += 1;
        }
        msg
    }

    /// Send a message to a mesh channel (broadcast).
    /// Routes through the background listener which owns the serial port.
    pub async fn send_channel_message(&self, channel: u8, text: &str) -> Result<MeshMessage> {
        let status = self.state.status.read().await;
        if !status.device_connected {
            anyhow::bail!("No mesh device connected");
        }
        drop(status);

        let payload = text.as_bytes().to_vec();

        if payload.len() > protocol::MAX_MESSAGE_LEN {
            anyhow::bail!(
                "Message too large for LoRa: {} bytes (max {})",
                payload.len(),
                protocol::MAX_MESSAGE_LEN
            );
        }

        // Send through the listener's command channel
        self.state
            .send_cmd(listener::MeshCommand::BroadcastChannel { channel, payload })
            .await
            .map_err(|_| anyhow::anyhow!("Mesh listener not running"))?;

        let chan_contact_id = u32::MAX - (channel as u32);
        let chan_name = format!("Channel {}", channel);
        let msg_id = self.state.next_id().await;
        let radio_transport = radio_transport_label(self.state.status.read().await.device_type);

        let msg = MeshMessage {
            id: msg_id,
            direction: MessageDirection::Sent,
            peer_contact_id: chan_contact_id,
            peer_name: Some(chan_name),
            plaintext: text.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            delivered: false,
            // Plain channel broadcast over the radio (shared PSK, not E2E).
            encrypted: false,
            transport: Some(radio_transport.to_string()),
            message_type: "text".to_string(),
            typed_payload: None,
            sender_pubkey: None,
            sender_seq: None,
        };

        self.state.store_message(msg.clone()).await;
        {
            let mut status = self.state.status.write().await;
            status.messages_sent += 1;
        }

        Ok(msg)
    }

    /// Broadcast our advertisement over mesh so other nodes can discover us.
    /// Sends an immediate advert via the listener's command channel.
    pub async fn broadcast_identity(&self) -> Result<()> {
        let status = self.state.status.read().await;
        if !status.device_connected {
            anyhow::bail!("No mesh device connected. Check USB connection.");
        }
        drop(status);

        self.state
            .send_cmd(listener::MeshCommand::SendAdvert)
            .await
            .map_err(|_| anyhow::anyhow!("Mesh listener not running"))?;

        info!("Mesh self-advert broadcast triggered");
        Ok(())
    }

    /// Reboot the locally-connected radio firmware to recover a wedged /
    /// RX-deaf radio (one that has stopped hearing the mesh while still able to
    /// transmit). The device reconnects via the listener's reboot→reconnect
    /// loop. `seconds` is the firmware reboot delay.
    pub async fn reboot_radio(&self, seconds: i64) -> Result<String> {
        let status = self.state.status.read().await;
        if !status.device_connected {
            anyhow::bail!("No mesh device connected. Check USB connection.");
        }
        drop(status);

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.state
            .send_cmd(listener::MeshCommand::RebootRadio {
                seconds,
                reply: Some(tx),
            })
            .await
            .map_err(|_| anyhow::anyhow!("Mesh listener not running"))?;

        // The real outcome, not fire-and-forget: the UI shows this string
        // (or the error) instead of pretending success.
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(15), rx)
            .await
            .map_err(|_| anyhow::anyhow!("The radio did not acknowledge the reboot in time"))?
            .map_err(|_| anyhow::anyhow!("Mesh session ended before the reboot completed"))?;
        let message = outcome.map_err(|e| anyhow::anyhow!(e))?;
        info!(seconds, "Mesh radio reboot triggered");
        Ok(message)
    }

    /// Live RNode radio state (Reticulum-only): the sidecar's view of the
    /// interface including the radio-confirmed r_* parameters. The LoRa
    /// settings panel's source for "what is the device actually running".
    pub async fn radio_state(&self) -> Result<serde_json::Value> {
        // Retry across a reconnect window. Applying settings deliberately
        // restarts the radio daemon (~15s), and the session is legitimately
        // absent while it comes back — a single-shot query inside that window
        // reported "the daemon did not answer" for what is a healthy,
        // in-progress restart (operator, 2026-08-06).
        const ATTEMPTS: u32 = 6;
        let mut last_err = anyhow::anyhow!("No mesh device connected. Check USB connection.");
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(4)).await;
            }
            if !self.state.status.read().await.device_connected {
                last_err = anyhow::anyhow!(
                    "The radio is not connected right now — if settings were just applied it \
                     is restarting and comes back within about 20 seconds."
                );
                continue;
            }
            let (tx, rx) = tokio::sync::oneshot::channel();
            if self
                .state
                .send_cmd(listener::MeshCommand::QueryRadioState { reply: tx })
                .await
                .is_err()
            {
                last_err = anyhow::anyhow!("Mesh listener not running");
                continue;
            }
            match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
                Ok(Ok(Ok(state))) => return Ok(state),
                Ok(Ok(Err(e))) => {
                    // A real device-level refusal (e.g. not an RNode radio) —
                    // retrying cannot change it.
                    return Err(anyhow::anyhow!(e));
                }
                Ok(Err(_)) => {
                    last_err =
                        anyhow::anyhow!("Mesh session ended before the state query completed")
                }
                Err(_) => {
                    last_err = anyhow::anyhow!("The radio daemon did not answer the state query")
                }
            }
        }
        Err(last_err)
    }

    /// Current mesh-AI assistant settings (issue #50).
    pub async fn assistant_config(&self) -> listener::AssistantConfig {
        self.state.assistant.read().await.clone()
    }

    /// Recently-denied `!ai` askers (newest first) so the UI can offer to allow
    /// them. Cleared implicitly as new denials rotate older ones out.
    pub async fn assistant_denied_askers(&self) -> Vec<listener::DeniedAsker> {
        self.state
            .assist_denied
            .read()
            .await
            .iter()
            .cloned()
            .collect()
    }

    /// Update the mesh-AI assistant settings live (no listener restart) and
    /// persist them to the mesh config. `model: Some(None)` clears the override
    /// (falls back to the built-in default); `None` leaves a field unchanged.
    pub async fn configure_assistant(
        &self,
        enabled: Option<bool>,
        model: Option<Option<String>>,
        trusted_only: Option<bool>,
        backend: Option<String>,
        allowed_contacts: Option<Vec<String>>,
    ) -> Result<()> {
        {
            let mut a = self.state.assistant.write().await;
            if let Some(e) = enabled {
                a.enabled = e;
            }
            if let Some(m) = model {
                a.model = m;
            }
            if let Some(t) = trusted_only {
                a.trusted_only = t;
            }
            if let Some(b) = backend {
                a.backend = b;
            }
            if let Some(list) = allowed_contacts {
                a.allowed_contacts = list;
            }
        }
        // Persist by updating the on-disk config (the in-memory `self.config`
        // snapshot stays as-is; the live `state.assistant` is the runtime
        // source of truth and is re-seeded from disk on the next start).
        let mut cfg = load_config(&self.data_dir).await.unwrap_or_default();
        {
            let a = self.state.assistant.read().await;
            cfg.assistant_enabled = a.enabled;
            cfg.assistant_model = a.model.clone();
            cfg.assistant_trusted_only = a.trusted_only;
            cfg.assistant_backend = a.backend.clone();
            cfg.assistant_allowed_contacts = a.allowed_contacts.clone();
        }
        save_config(&self.data_dir, &cfg).await?;
        Ok(())
    }

    /// Update mesh configuration.
    pub async fn configure(&mut self, config: MeshConfig) -> Result<()> {
        save_config(&self.data_dir, &config).await?;

        // An operator-set RNode serial-port override (rnode-rf-settings.json)
        // outranks `device_path` when the Reticulum session opens the radio.
        // When a *different* device path is being configured (hot-swap, or
        // "Keep As Is" on a newly detected radio), a stale override pinned to
        // the old port would silently veto the choice the user just made —
        // clear it so the explicit device selection wins. Same-device aliases
        // (/dev/mesh-radio vs its ttyUSBn target) are left alone.
        if let Some(new_path) = config.device_path.as_deref() {
            let mut rf = rnode_settings::RNodeRfSettings::load(&self.data_dir).await;
            if let Some(port) = rf.port.clone() {
                if !same_serial_device(&port, new_path).await {
                    info!(
                        old_port = %port,
                        new_path = %new_path,
                        "Clearing stale RNode serial-port override — configured device path changed"
                    );
                    rf.port = None;
                    if let Err(e) = rf.save(&self.data_dir).await {
                        warn!("Failed to clear stale RNode port override: {e}");
                    }
                }
            }
        }

        let was_enabled = self.config.enabled;
        let needs_session_restart = session_config_changed(&self.config, &config);
        self.config = config.clone();

        // Update the status to reflect new config
        {
            let mut status = self.state.status.write().await;
            status.enabled = config.enabled;
            status.channel_name = config
                .channel_name
                .clone()
                .unwrap_or_else(|| "archipelago".to_string());
        }

        // If enabled state changed, start/stop the listener
        if config.enabled && !was_enabled {
            self.start()?;
        } else if !config.enabled && was_enabled {
            self.stop().await;
            // Clear connected state
            let mut status = self.state.status.write().await;
            status.device_connected = false;
            status.device_path = None;
            status.firmware_version = None;
            status.self_node_id = None;
            status.peer_count = 0;
        } else if config.enabled && was_enabled && needs_session_restart {
            info!("Mesh session config changed — restarting listener to apply");
            self.stop().await;
            self.start()?;
        }

        Ok(())
    }

    /// The service's current (last-applied) config.
    pub fn config(&self) -> &MeshConfig {
        &self.config
    }

    /// Restart the listener (if running) so it picks up out-of-band state a
    /// spawn captured by value — currently the server name pushed by
    /// `server.set-name`.
    pub async fn restart_listener_if_running(&mut self) -> Result<()> {
        if self.listener_handle.is_some() {
            self.stop().await;
            self.start()?;
        }
        Ok(())
    }

    /// Get a reference to shared state (for RPC handlers).
    pub fn shared_state(&self) -> Arc<MeshState> {
        Arc::clone(&self.state)
    }

    /// Record user activity (resets dead man's switch timer).
    pub async fn dead_man_check_in(&self) {
        self.dead_man_switch.check_in().await;
    }

    /// Get our DID.
    pub fn our_did(&self) -> &str {
        &self.our_did
    }
}

const MAX_MESSAGES_DEFAULT: usize = 100;

// ─── Bitcoin RPC helpers for block header announcer ────────────────────

#[derive(serde::Deserialize)]
struct BitcoinRpcResponse<T> {
    result: Option<T>,
    error: Option<serde_json::Value>,
}

struct BlockHeaderInfo {
    hash: String,
    prev_hash: String,
    timestamp: u32,
}

async fn bitcoin_rpc_getblockcount(client: &reqwest::Client) -> Result<u64> {
    let (rpc_user, rpc_pass) = crate::bitcoin_rpc::bitcoin_rpc_credentials().await;
    let body = serde_json::json!({
        "jsonrpc": "1.0", "id": "mesh", "method": "getblockcount", "params": []
    });
    let resp: BitcoinRpcResponse<u64> = tokio::time::timeout(
        Duration::from_secs(10),
        client
            .post(crate::constants::BITCOIN_RPC_URL)
            .basic_auth(&rpc_user, Some(&rpc_pass))
            .json(&body)
            .send(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Bitcoin RPC getblockcount timed out after 10s"))?
    .map_err(|e| anyhow::anyhow!("Bitcoin RPC send failed: {}", e))?
    .json()
    .await
    .map_err(|e| anyhow::anyhow!("Bitcoin RPC parse failed: {}", e))?;
    if let Some(err) = resp.error {
        anyhow::bail!("Bitcoin RPC: {}", err);
    }
    resp.result
        .ok_or_else(|| anyhow::anyhow!("Bitcoin RPC null result"))
}

async fn bitcoin_rpc_getblockheader_by_height(
    client: &reqwest::Client,
    height: u64,
) -> Result<BlockHeaderInfo> {
    let (rpc_user, rpc_pass) = crate::bitcoin_rpc::bitcoin_rpc_credentials().await;
    // First get block hash for this height
    let body = serde_json::json!({
        "jsonrpc": "1.0", "id": "mesh", "method": "getblockhash", "params": [height]
    });
    let resp: BitcoinRpcResponse<String> = tokio::time::timeout(
        Duration::from_secs(10),
        client
            .post(crate::constants::BITCOIN_RPC_URL)
            .basic_auth(&rpc_user, Some(&rpc_pass))
            .json(&body)
            .send(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Bitcoin RPC getblockhash timed out after 10s"))?
    .map_err(|e| anyhow::anyhow!("Bitcoin RPC getblockhash send failed: {}", e))?
    .json()
    .await
    .map_err(|e| anyhow::anyhow!("Bitcoin RPC getblockhash parse failed: {}", e))?;
    let hash = resp
        .result
        .ok_or_else(|| anyhow::anyhow!("No block hash"))?;

    // Then get full header
    let body = serde_json::json!({
        "jsonrpc": "1.0", "id": "mesh", "method": "getblockheader", "params": [hash, true]
    });
    let resp: BitcoinRpcResponse<serde_json::Value> = tokio::time::timeout(
        Duration::from_secs(10),
        client
            .post(crate::constants::BITCOIN_RPC_URL)
            .basic_auth(&rpc_user, Some(&rpc_pass))
            .json(&body)
            .send(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Bitcoin RPC getblockheader timed out after 10s"))?
    .map_err(|e| anyhow::anyhow!("Bitcoin RPC getblockheader send failed: {}", e))?
    .json()
    .await
    .map_err(|e| anyhow::anyhow!("Bitcoin RPC getblockheader parse failed: {}", e))?;
    let header = resp
        .result
        .ok_or_else(|| anyhow::anyhow!("No block header"))?;

    Ok(BlockHeaderInfo {
        hash: header["hash"].as_str().unwrap_or_default().to_string(),
        prev_hash: header["previousblockhash"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        timestamp: header["time"].as_u64().unwrap_or(0) as u32,
    })
}

#[cfg(test)]
mod tests {

    /// Item 5: a federated/trusted peer must be messageable as soon as it is
    /// peered — no LoRa meeting first.
    ///
    /// The routing predicate in `send_message` decides whether a plain text
    /// message takes the federation-aware typed path (which knows FIPS/Tor and
    /// still prefers a reachable radio twin) or the radio-only path, which
    /// resolves an over-the-air routing key and cannot work for a peer we have
    /// never heard on the radio.
    ///
    /// It previously keyed on the attached radio, so on a Meshtastic node — or
    /// one with no radio at all — a federated peer fell to the radio path and
    /// the send failed. Federation contacts are reachable off-radio by
    /// definition, so the choice must not depend on which radio is plugged in.
    #[test]
    fn federation_contacts_take_the_off_radio_path_on_any_device() {
        fn uses_typed_path(contact_id: u32, archy: bool, device: DeviceType) -> bool {
            let is_federation_contact = contact_id & 0x8000_0000 != 0;
            archy
                && (is_federation_contact
                    || matches!(device, DeviceType::Meshcore | DeviceType::Reticulum))
        }
        let fed = super::federation_peer_contact_id(&"ab".repeat(32));
        assert!(fed >= FEDERATION_CONTACT_ID_BASE);

        // The cases that used to fail: peered node, wrong radio or none.
        for device in [
            DeviceType::Meshtastic,
            DeviceType::Unknown,
            DeviceType::Meshcore,
            DeviceType::Reticulum,
        ] {
            assert!(
                uses_typed_path(fed, true, device),
                "federation peer must route off-radio on {device:?}"
            );
        }

        // A plain radio contact on a stock-text device still takes the radio
        // path — this fix must not reroute ordinary LoRa chats.
        assert!(!uses_typed_path(42, true, DeviceType::Meshtastic));
        // And a stock (non-archy) client is never given a typed envelope.
        assert!(!uses_typed_path(42, false, DeviceType::Meshcore));
    }
    use super::*;

    #[test]
    fn session_config_change_detection() {
        let base = MeshConfig::default();

        // Same config → no restart.
        assert!(!session_config_changed(&base, &base.clone()));

        // Every session-captured field individually triggers a restart.
        let mut c = base.clone();
        c.device_kind = Some(types::DeviceType::Reticulum);
        assert!(session_config_changed(&base, &c));

        let mut c = base.clone();
        c.device_path = Some("/dev/ttyUSB0".into());
        assert!(session_config_changed(&base, &c));

        let mut c = base.clone();
        c.advert_name = Some("RNode Shaza".into());
        assert!(session_config_changed(&base, &c));

        let mut c = base.clone();
        c.manage_radio = !base.manage_radio;
        assert!(session_config_changed(&base, &c));

        let mut c = base.clone();
        c.lora_region = Some("EU_868".into());
        assert!(session_config_changed(&base, &c));

        let mut c = base.clone();
        c.channel_name = Some("private-net".into());
        assert!(session_config_changed(&base, &c));

        // Live-read fields must NOT force a session restart.
        let mut c = base.clone();
        c.broadcast_identity = !base.broadcast_identity;
        c.announce_block_headers = !base.announce_block_headers;
        c.assistant_enabled = !base.assistant_enabled;
        assert!(!session_config_changed(&base, &c));
    }

    fn mk_peer(contact_id: u32, name: &str, arch: Option<&str>, reachable: bool) -> MeshPeer {
        MeshPeer {
            contact_id,
            advert_name: name.to_string(),
            did: None,
            pubkey_hex: Some(format!("fw{contact_id}")),
            arch_pubkey_hex: arch.map(|s| s.to_string()),
            x25519_pubkey: None,
            rssi: None,
            snr: None,
            last_heard: String::new(),
            hops: 0,
            last_advert: 0,
            reachable,
            pkc_capable: false,
            lat: None,
            lon: None,
            lightning_uri: None,
        }
    }

    #[test]
    fn test_group_peer_twins_collapses_radio_and_federation() {
        let radio = mk_peer(42, "Test-Peer-1", Some("ABCD"), false);
        let fed = mk_peer(0x8000_0001, "Test-Peer-1", Some("abcd"), true);
        let groups = group_peer_twins(&[radio, fed]);
        assert_eq!(groups.len(), 1, "twins must collapse to one conversation");
        let g = &groups[0];
        // Canonical = the radio twin (mesh-first send), but reachability is the
        // OR across twins (federation is reachable off-radio).
        assert_eq!(g.canonical.contact_id, 42);
        assert!(g.canonical.reachable);
        // Both ids retained so messages can be unioned across them.
        assert!(g.contact_ids.contains(&42) && g.contact_ids.contains(&0x8000_0001));
    }

    #[test]
    fn test_group_peer_twins_keeps_distinct_identities_and_unbound_radio() {
        // Two different identities + one radio peer that was never bound to a
        // federation twin (arch = None) → three separate conversations.
        let a = mk_peer(1, "Alice", Some("aa"), false);
        let b = mk_peer(2, "Bob", Some("bb"), true);
        let lonely = mk_peer(3, "Carol-radio", None, false);
        let groups = group_peer_twins(&[a, b, lonely]);
        assert_eq!(groups.len(), 3);
    }

    #[test]
    fn test_group_peer_twins_federation_only_uses_federation_id() {
        let fed = mk_peer(0x8000_00ff, "Arch Dev", Some("dead"), true);
        let groups = group_peer_twins(&[fed]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].canonical.contact_id, 0x8000_00ff);
    }

    #[test]
    fn test_mesh_config_default() {
        let config = MeshConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.channel_name, Some("archipelago".to_string()));
        assert!(config.broadcast_identity);
    }

    #[test]
    fn test_mesh_config_serialization() {
        let config = MeshConfig {
            enabled: true,
            device_path: Some("/dev/ttyUSB0".to_string()),
            channel_name: Some("test".to_string()),
            broadcast_identity: false,
            advert_name: Some("MyNode".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: MeshConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.device_path, Some("/dev/ttyUSB0".to_string()));
        assert_eq!(parsed.advert_name, Some("MyNode".to_string()));
    }

    #[tokio::test]
    async fn test_load_config_default_when_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_config(dir.path()).await.unwrap();
        assert!(!config.enabled);
    }

    #[tokio::test]
    async fn test_save_and_load_config_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let config = MeshConfig {
            enabled: true,
            device_path: Some("/dev/ttyUSB0".to_string()),
            channel_name: Some("archy".to_string()),
            broadcast_identity: true,
            advert_name: None,
            ..Default::default()
        };
        save_config(dir.path(), &config).await.unwrap();
        let loaded = load_config(dir.path()).await.unwrap();
        assert!(loaded.enabled);
        assert_eq!(loaded.device_path, Some("/dev/ttyUSB0".to_string()));
    }

    /// End-to-end: `MeshService::start()` spawns a real `reticulum-daemon` in
    /// plain-TCP CLIENT mode (no serial RNode, no `probe_rnode`), dials a
    /// second stand-alone daemon instance running in TCP SERVER mode (the
    /// Aurora-side role — Aurora's `RnsTcpInterface` dials the same way), and
    /// reaches `device_connected: true` via the exact `mesh.status`-backing
    /// `MeshService::status()` call the RPC layer uses. This is the Rust-side
    /// half of the archy<->Aurora TCP interop gate (see
    /// docs/RETICULUM-TRANSPORT-PROGRESS.md); the LXMF wire-level proof
    /// itself already passed via a scripted RNS/LXMF stand-in (Steps 1-2 of
    /// that gate), so this test only needs to prove Rust's spawn/connect
    /// path, not re-prove LXMF content delivery.
    ///
    /// Requires `reticulum-daemon/.venv` (`python3 -m venv .venv &&
    /// .venv/bin/pip install -r requirements.txt`) — skips (not fails) if
    /// absent, since CI doesn't provision a Python/RNS/LXMF environment for
    /// the rest of the mesh test suite either.
    #[tokio::test]
    #[ignore = "spawns real reticulum-daemon subprocesses over loopback TCP; run manually with `cargo test -p archipelago -- --ignored mesh_service_connects_over_reticulum_tcp`"]
    async fn mesh_service_connects_over_reticulum_tcp_client() {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;

        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("core/archipelago has two ancestors up to the repo root")
            .to_path_buf();
        let venv_py = repo_root.join("reticulum-daemon/.venv/bin/python");
        let daemon_script = repo_root.join("reticulum-daemon/reticulum_daemon.py");
        if !venv_py.exists() {
            eprintln!(
                "SKIP mesh_service_connects_over_reticulum_tcp_client: {} not found — \
                 run `python3 -m venv .venv && .venv/bin/pip install -r requirements.txt` \
                 in reticulum-daemon/",
                venv_py.display()
            );
            return;
        }

        let root = tempfile::tempdir().unwrap();

        // Stand-in "Aurora-side" server daemon: a second, independent
        // reticulum-daemon instance in --tcp-listen mode. Any free loopback
        // port works; this test doesn't need it fixed, so ask the OS for one.
        let bind_addr = {
            let sock = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = sock.local_addr().unwrap();
            drop(sock); // release it so the daemon can bind — small TOCTOU window, acceptable for a manual/ignored test
            addr
        };
        let server_dir = root.path().join("standin-server");
        std::fs::create_dir_all(&server_dir).unwrap();
        let server_seed = SigningKey::generate(&mut OsRng);
        std::fs::write(server_dir.join("seed.bin"), server_seed.to_bytes()).unwrap();
        let mut server = tokio::process::Command::new(&venv_py)
            .arg(&daemon_script)
            .arg("--identity-key")
            .arg(server_dir.join("seed.bin"))
            .arg("--socket")
            .arg(server_dir.join("server.sock"))
            .arg("--rns-config")
            .arg(server_dir.join("rns-config"))
            .arg("--tcp-listen")
            .arg(bind_addr.to_string())
            .arg("--display-name")
            .arg("standin-server")
            .kill_on_drop(true)
            .spawn()
            .expect("failed to spawn stand-in reticulum-daemon (server)");

        // Give the server daemon time to bind before the client dials it.
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Point ReticulumLink::daemon_command's dev-fallback at our venv —
        // these env vars are read fresh on every spawn (mesh/reticulum.rs),
        // so setting them here (test-process-global, but this test owns the
        // only mesh session in this process) is sufficient.
        std::env::set_var("ARCHY_RETICULUM_DAEMON_PY", &venv_py);
        std::env::set_var("ARCHY_RETICULUM_DAEMON_SCRIPT", &daemon_script);
        // Force the dev fallback even if a packaged binary happens to exist
        // on this machine's PATH convention — this test wants exactly the
        // freshly-built venv daemon under test.
        std::env::set_var(
            "ARCHY_RETICULUM_DAEMON_BIN",
            "/nonexistent-force-dev-fallback",
        );

        let data_dir = root.path().join("node");
        std::fs::create_dir_all(data_dir.join("identity")).unwrap();
        let node_signing_key = SigningKey::generate(&mut OsRng);
        std::fs::write(
            data_dir.join("identity").join("node_key"),
            node_signing_key.to_bytes(),
        )
        .unwrap();
        let ed_pubkey_hex = hex::encode(node_signing_key.verifying_key().to_bytes());
        let did = crate::identity::did_key_from_pubkey_hex(&ed_pubkey_hex).unwrap();

        let config = MeshConfig {
            enabled: true,
            reticulum_tcp: Some(types::ReticulumTcpConfig::Client {
                connect: vec![bind_addr.to_string()],
            }),
            ..Default::default()
        };
        save_config(&data_dir, &config).await.unwrap();

        let mut service = MeshService::new(&data_dir, &node_signing_key, &did, &ed_pubkey_hex)
            .await
            .expect("MeshService::new failed");
        service.start().expect("MeshService::start failed");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        let mut last_status = service.status().await;
        while tokio::time::Instant::now() < deadline {
            last_status = service.status().await;
            if last_status.device_connected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }

        let _ = server.start_kill();

        assert!(
            last_status.device_connected,
            "MeshService never reached device_connected: true over Reticulum TCP client \
             mode within 20s (last status: {last_status:?})"
        );
        assert_eq!(last_status.device_type, DeviceType::Reticulum);
    }
}
