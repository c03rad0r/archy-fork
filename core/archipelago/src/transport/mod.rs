// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Transport abstraction layer for Archipelago node-to-node communication.
//!
//! Unifies mesh radio (LoRa), LAN (mDNS), FIPS (Free Internetworking Peering
//! System overlay), and Tor under a common trait. Routes messages to peers via
//! the best available transport with automatic fallback:
//! Mesh (1) > LAN (2) > FIPS (3) > Tor (4).
//!
//! FIPS sits between LAN and Tor: faster than Tor for WAN peering, but still
//! defers to direct LAN connectivity when peers are on the same network.

pub mod chunking;
pub mod delta;
pub mod fips;
pub mod lan;
pub mod mesh_transport;
pub mod tor;

use crate::federation::TrustLevel;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{info, warn};

// ─── Transport Kind ─────────────────────────────────────────────────────

/// Transport backend type, ordered by priority (lower = preferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportKind {
    Mesh = 1,
    Lan = 2,
    Fips = 3,
    Tor = 4,
}

impl std::fmt::Display for TransportKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mesh => write!(f, "mesh"),
            Self::Lan => write!(f, "lan"),
            Self::Fips => write!(f, "fips"),
            Self::Tor => write!(f, "tor"),
        }
    }
}

// ─── Message Types ──────────────────────────────────────────────────────

/// Type of transport-level message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    StateSync,
    PeerMessage,
    FederationRpc,
}

/// A message sent between nodes via any transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportMessage {
    pub from_did: String,
    pub payload: Vec<u8>,
    pub message_type: MessageType,
}

// ─── NodeTransport Trait ────────────────────────────────────────────────

/// Trait implemented by each transport backend (Tor, Mesh, LAN).
pub trait NodeTransport: Send + Sync {
    /// Which transport this is.
    fn kind(&self) -> TransportKind;

    /// Whether this transport is currently operational.
    fn is_available(&self) -> bool;

    /// Send raw bytes to a peer at their transport-specific address.
    /// For Tor: address is an onion hostname.
    /// For Mesh: address is a contact_id as string.
    /// For LAN: address is "ip:port".
    /// For FIPS: address is the peer's FIPS npub (bech32); implementation maps to fd00::/8.
    fn send<'a>(
        &'a self,
        address: &'a str,
        message: &'a TransportMessage,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>;
}

// ─── Peer Registry ──────────────────────────────────────────────────────

/// How we discovered this peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerSource {
    Federation,
    MeshDiscovery,
    LanDiscovery,
    NostrHandshake,
    Manual,
}

/// Unified peer record with per-transport capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeerRecord {
    pub did: String,
    pub pubkey_hex: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub trust_level: Option<TrustLevel>,
    #[serde(default)]
    pub source: Option<PeerSource>,

    // Transport-specific addresses
    #[serde(default)]
    pub mesh_contact_id: Option<u32>,
    #[serde(default)]
    pub lan_address: Option<String>,
    #[serde(default)]
    pub fips_npub: Option<String>,
    #[serde(default)]
    pub onion_address: Option<String>,

    // Freshness timestamps (RFC 3339)
    #[serde(default)]
    pub last_mesh: Option<String>,
    #[serde(default)]
    pub last_lan: Option<String>,
    #[serde(default)]
    pub last_fips: Option<String>,
    #[serde(default)]
    pub last_tor: Option<String>,
}

impl PeerRecord {
    /// Get the transport-specific address for a given transport kind.
    pub fn address_for(&self, kind: TransportKind) -> Option<String> {
        match kind {
            TransportKind::Mesh => self.mesh_contact_id.map(|id| id.to_string()),
            TransportKind::Lan => self.lan_address.clone(),
            TransportKind::Fips => self.fips_npub.clone(),
            TransportKind::Tor => self.onion_address.clone(),
        }
    }

    /// Check if the last-seen timestamp for a transport is fresh enough.
    /// Mesh/LAN: 5 minutes. FIPS: 30 minutes. Tor: 1 hour.
    pub fn is_fresh(&self, kind: TransportKind) -> bool {
        let timestamp = match kind {
            TransportKind::Mesh => self.last_mesh.as_deref(),
            TransportKind::Lan => self.last_lan.as_deref(),
            TransportKind::Fips => self.last_fips.as_deref(),
            TransportKind::Tor => self.last_tor.as_deref(),
        };
        let Some(ts) = timestamp else {
            // No timestamp means we haven't confirmed it, but the address exists.
            // Allow it — the send will fail if unreachable.
            return true;
        };
        let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(ts) else {
            return false;
        };
        let age = chrono::Utc::now().signed_duration_since(parsed);
        let max_age = match kind {
            TransportKind::Mesh | TransportKind::Lan => chrono::Duration::minutes(5),
            TransportKind::Fips => chrono::Duration::minutes(30),
            TransportKind::Tor => chrono::Duration::hours(1),
        };
        age < max_age
    }

    /// List available transport kinds for this peer, in priority order.
    pub fn available_transports(&self) -> Vec<TransportKind> {
        let mut result = Vec::new();
        if self.mesh_contact_id.is_some() {
            result.push(TransportKind::Mesh);
        }
        if self.lan_address.is_some() {
            result.push(TransportKind::Lan);
        }
        if self.fips_npub.is_some() {
            result.push(TransportKind::Fips);
        }
        if self.onion_address.is_some() {
            result.push(TransportKind::Tor);
        }
        result
    }
}

const PEERS_FILE: &str = "transport-peers.json";

/// Thread-safe registry of all known peers with their transport capabilities.
pub struct PeerRegistry {
    peers: RwLock<HashMap<String, PeerRecord>>,
    data_dir: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PeersFile {
    peers: Vec<PeerRecord>,
}

impl PeerRegistry {
    /// Load peer registry from disk (or create empty).
    pub async fn load(data_dir: &Path) -> Result<Self> {
        let path = data_dir.join(PEERS_FILE);
        let peers = if path.exists() {
            let content = fs::read_to_string(&path)
                .await
                .context("Failed to read transport peers")?;
            let file: PeersFile = serde_json::from_str(&content).unwrap_or_default();
            file.peers.into_iter().map(|p| (p.did.clone(), p)).collect()
        } else {
            HashMap::new()
        };

        Ok(Self {
            peers: RwLock::new(peers),
            data_dir: data_dir.to_path_buf(),
        })
    }

    /// Persist current state to disk.
    pub async fn save(&self) -> Result<()> {
        let peers = self.peers.read().await;
        let file = PeersFile {
            peers: peers.values().cloned().collect(),
        };
        let content =
            serde_json::to_string_pretty(&file).context("Failed to serialize transport peers")?;
        fs::write(self.data_dir.join(PEERS_FILE), content)
            .await
            .context("Failed to write transport peers")?;
        Ok(())
    }

    /// Register or update a peer.
    pub async fn register_peer(
        &self,
        did: &str,
        pubkey_hex: &str,
        source: PeerSource,
    ) -> PeerRecord {
        let mut peers = self.peers.write().await;
        let record = peers.entry(did.to_string()).or_insert_with(|| PeerRecord {
            did: did.to_string(),
            pubkey_hex: pubkey_hex.to_string(),
            name: None,
            trust_level: None,
            source: Some(source.clone()),
            mesh_contact_id: None,
            lan_address: None,
            fips_npub: None,
            onion_address: None,
            last_mesh: None,
            last_lan: None,
            last_fips: None,
            last_tor: None,
        });
        // Update pubkey if it changed
        if record.pubkey_hex != pubkey_hex {
            record.pubkey_hex = pubkey_hex.to_string();
        }
        record.clone()
    }

    /// Set the mesh contact ID for a peer.
    pub async fn set_mesh_id(&self, did: &str, contact_id: u32) {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(did) {
            peer.mesh_contact_id = Some(contact_id);
            peer.last_mesh = Some(chrono::Utc::now().to_rfc3339());
        }
    }

    /// Set the LAN address for a peer.
    pub async fn set_lan_address(&self, did: &str, addr: SocketAddr) {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(did) {
            peer.lan_address = Some(addr.to_string());
            peer.last_lan = Some(chrono::Utc::now().to_rfc3339());
        }
    }

    /// Set the onion address for a peer.
    pub async fn set_onion(&self, did: &str, onion: &str) {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(did) {
            peer.onion_address = Some(onion.to_string());
            peer.last_tor = Some(chrono::Utc::now().to_rfc3339());
        }
    }

    /// Set the FIPS npub for a peer (bech32 pubkey used by the FIPS mesh).
    pub async fn set_fips_npub(&self, did: &str, npub: &str) {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(did) {
            peer.fips_npub = Some(npub.to_string());
            peer.last_fips = Some(chrono::Utc::now().to_rfc3339());
        }
    }

    /// Set the display name for a peer.
    pub async fn set_name(&self, did: &str, name: &str) {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(did) {
            peer.name = Some(name.to_string());
        }
    }

    /// Get a peer by DID.
    pub async fn get_peer(&self, did: &str) -> Option<PeerRecord> {
        self.peers.read().await.get(did).cloned()
    }

    /// Get all peers.
    pub async fn all_peers(&self) -> Vec<PeerRecord> {
        self.peers.read().await.values().cloned().collect()
    }

    /// Count of registered peers.
    pub async fn count(&self) -> usize {
        self.peers.read().await.len()
    }
}

// ─── Transport Router ───────────────────────────────────────────────────

/// Routes messages to the best available transport per peer.
pub struct TransportRouter {
    transports: Vec<Box<dyn NodeTransport>>,
    pub registry: Arc<PeerRegistry>,
    mesh_only: RwLock<bool>,
}

impl TransportRouter {
    pub fn new(
        transports: Vec<Box<dyn NodeTransport>>,
        registry: Arc<PeerRegistry>,
        mesh_only: bool,
    ) -> Self {
        Self {
            transports,
            registry,
            mesh_only: RwLock::new(mesh_only),
        }
    }

    /// Send a message to a peer by DID, using the best available transport.
    pub async fn send_to_peer(
        &self,
        did: &str,
        message: &TransportMessage,
    ) -> Result<TransportKind> {
        let peer = self
            .registry
            .get_peer(did)
            .await
            .ok_or_else(|| anyhow::anyhow!("Unknown peer: {}", did))?;

        let candidates = self.route(&peer).await;
        if candidates.is_empty() {
            anyhow::bail!("No available transport for peer {}", did);
        }

        let mut last_err = None;
        for kind in &candidates {
            let transport = match self.transports.iter().find(|t| t.kind() == *kind) {
                Some(t) => t,
                None => continue,
            };

            let address = match peer.address_for(*kind) {
                Some(a) => a,
                None => continue,
            };

            match transport.send(&address, message).await {
                Ok(()) => {
                    info!(transport = %kind, peer = %did, "Message sent");
                    return Ok(*kind);
                }
                Err(e) => {
                    warn!(transport = %kind, peer = %did, error = %e, "Transport failed, trying next");
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("All transports failed for peer {}", did)))
    }

    /// Determine transport priority for a peer.
    async fn route(&self, peer: &PeerRecord) -> Vec<TransportKind> {
        let mesh_only = *self.mesh_only.read().await;
        let mut available = Vec::new();

        if mesh_only {
            // Off-grid mode: only mesh
            if peer.mesh_contact_id.is_some() {
                available.push(TransportKind::Mesh);
            }
        } else {
            // Normal mode: priority order, check freshness
            if peer.mesh_contact_id.is_some() && peer.is_fresh(TransportKind::Mesh) {
                if let Some(t) = self
                    .transports
                    .iter()
                    .find(|t| t.kind() == TransportKind::Mesh)
                {
                    if t.is_available() {
                        available.push(TransportKind::Mesh);
                    }
                }
            }
            if peer.lan_address.is_some() && peer.is_fresh(TransportKind::Lan) {
                if let Some(t) = self
                    .transports
                    .iter()
                    .find(|t| t.kind() == TransportKind::Lan)
                {
                    if t.is_available() {
                        available.push(TransportKind::Lan);
                    }
                }
            }
            if peer.fips_npub.is_some() && peer.is_fresh(TransportKind::Fips) {
                if let Some(t) = self
                    .transports
                    .iter()
                    .find(|t| t.kind() == TransportKind::Fips)
                {
                    if t.is_available() {
                        available.push(TransportKind::Fips);
                    }
                }
            }
            if peer.onion_address.is_some() {
                if let Some(t) = self
                    .transports
                    .iter()
                    .find(|t| t.kind() == TransportKind::Tor)
                {
                    if t.is_available() {
                        available.push(TransportKind::Tor);
                    }
                }
            }
        }

        available
    }

    /// Set mesh-only (off-grid) mode.
    pub async fn set_mesh_only(&self, enabled: bool) {
        *self.mesh_only.write().await = enabled;
    }

    /// Get current mesh-only mode status.
    pub async fn is_mesh_only(&self) -> bool {
        *self.mesh_only.read().await
    }

    /// Get status of all transports.
    pub fn transport_status(&self) -> Vec<(TransportKind, bool)> {
        self.transports
            .iter()
            .map(|t| (t.kind(), t.is_available()))
            .collect()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_kind_ordering() {
        assert!(TransportKind::Mesh < TransportKind::Lan);
        assert!(TransportKind::Lan < TransportKind::Fips);
        assert!(TransportKind::Fips < TransportKind::Tor);
    }

    #[test]
    fn test_fips_preferred_over_tor_in_available_transports() {
        let peer = PeerRecord {
            did: "did:key:z6MkTest".to_string(),
            pubkey_hex: "aabb".to_string(),
            name: None,
            trust_level: None,
            source: None,
            mesh_contact_id: None,
            lan_address: None,
            fips_npub: Some("npub1exampleexampleexampleexampleexampleexample".to_string()),
            onion_address: Some("abc.onion".to_string()),
            last_mesh: None,
            last_lan: None,
            last_fips: None,
            last_tor: None,
        };
        let ts = peer.available_transports();
        let fips_idx = ts.iter().position(|k| *k == TransportKind::Fips).unwrap();
        let tor_idx = ts.iter().position(|k| *k == TransportKind::Tor).unwrap();
        assert!(fips_idx < tor_idx, "FIPS must be listed before Tor");
    }

    #[test]
    fn test_peer_record_address_for() {
        let peer = PeerRecord {
            did: "did:key:z6MkTest".to_string(),
            pubkey_hex: "aabb".to_string(),
            name: Some("test-node".to_string()),
            trust_level: None,
            source: None,
            mesh_contact_id: Some(42),
            lan_address: Some("192.0.2.100:5678".to_string()),
            fips_npub: None,
            onion_address: Some("abc123.onion".to_string()),
            last_mesh: None,
            last_lan: None,
            last_fips: None,
            last_tor: None,
        };
        assert_eq!(
            peer.address_for(TransportKind::Mesh),
            Some("42".to_string())
        );
        assert_eq!(
            peer.address_for(TransportKind::Lan),
            Some("192.0.2.100:5678".to_string())
        );
        assert_eq!(
            peer.address_for(TransportKind::Tor),
            Some("abc123.onion".to_string())
        );
    }

    #[test]
    fn test_peer_record_available_transports() {
        let peer = PeerRecord {
            did: "did:key:z6MkTest".to_string(),
            pubkey_hex: "aabb".to_string(),
            name: None,
            trust_level: None,
            source: None,
            mesh_contact_id: Some(1),
            lan_address: None,
            fips_npub: None,
            onion_address: Some("test.onion".to_string()),
            last_mesh: None,
            last_lan: None,
            last_fips: None,
            last_tor: None,
        };
        let transports = peer.available_transports();
        assert_eq!(transports, vec![TransportKind::Mesh, TransportKind::Tor]);
    }

    #[test]
    fn test_freshness_no_timestamp() {
        let peer = PeerRecord {
            did: "did:key:z6MkTest".to_string(),
            pubkey_hex: "aabb".to_string(),
            name: None,
            trust_level: None,
            source: None,
            mesh_contact_id: Some(1),
            lan_address: None,
            fips_npub: None,
            onion_address: None,
            last_mesh: None,
            last_lan: None,
            last_fips: None,
            last_tor: None,
        };
        // No timestamp = considered fresh (allows first attempt)
        assert!(peer.is_fresh(TransportKind::Mesh));
    }

    #[test]
    fn test_freshness_recent_timestamp() {
        let peer = PeerRecord {
            did: "did:key:z6MkTest".to_string(),
            pubkey_hex: "aabb".to_string(),
            name: None,
            trust_level: None,
            source: None,
            mesh_contact_id: Some(1),
            lan_address: None,
            fips_npub: None,
            onion_address: None,
            last_mesh: Some(chrono::Utc::now().to_rfc3339()),
            last_lan: None,
            last_fips: None,
            last_tor: None,
        };
        assert!(peer.is_fresh(TransportKind::Mesh));
    }

    #[test]
    fn test_freshness_stale_timestamp() {
        let stale = chrono::Utc::now() - chrono::Duration::minutes(10);
        let peer = PeerRecord {
            did: "did:key:z6MkTest".to_string(),
            pubkey_hex: "aabb".to_string(),
            name: None,
            trust_level: None,
            source: None,
            mesh_contact_id: Some(1),
            lan_address: None,
            fips_npub: None,
            onion_address: None,
            last_mesh: Some(stale.to_rfc3339()),
            last_lan: None,
            last_fips: None,
            last_tor: None,
        };
        // 10 minutes old > 5 minute mesh freshness threshold
        assert!(!peer.is_fresh(TransportKind::Mesh));
    }

    #[tokio::test]
    async fn test_peer_registry_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let registry = PeerRegistry::load(dir.path()).await.unwrap();

        registry
            .register_peer("did:key:z6MkTest", "aabbccdd", PeerSource::MeshDiscovery)
            .await;
        registry.set_mesh_id("did:key:z6MkTest", 42).await;
        registry
            .set_onion("did:key:z6MkTest", "test123.onion")
            .await;
        registry.save().await.unwrap();

        // Reload from disk
        let registry2 = PeerRegistry::load(dir.path()).await.unwrap();
        let peer = registry2.get_peer("did:key:z6MkTest").await.unwrap();
        assert_eq!(peer.mesh_contact_id, Some(42));
        assert_eq!(peer.onion_address, Some("test123.onion".to_string()));
        assert_eq!(peer.pubkey_hex, "aabbccdd");
    }
}
