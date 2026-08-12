//! Federation type definitions.

use serde::{Deserialize, Serialize};

/// Trust level for a federated node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    Trusted,
    Observer,
    Untrusted,
}

impl std::fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrustLevel::Trusted => write!(f, "trusted"),
            TrustLevel::Observer => write!(f, "observer"),
            TrustLevel::Untrusted => write!(f, "untrusted"),
        }
    }
}

impl TrustLevel {
    /// Parse the lowercase wire form ("trusted" | "observer" | "untrusted").
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "trusted" => Some(TrustLevel::Trusted),
            "observer" => Some(TrustLevel::Observer),
            "untrusted" => Some(TrustLevel::Untrusted),
            _ => None,
        }
    }

    /// The lower (less privileged) of two levels: Untrusted < Observer < Trusted.
    pub fn min(self, other: Self) -> Self {
        fn rank(l: TrustLevel) -> u8 {
            match l {
                TrustLevel::Untrusted => 0,
                TrustLevel::Observer => 1,
                TrustLevel::Trusted => 2,
            }
        }
        if rank(self) <= rank(other) {
            self
        } else {
            other
        }
    }
}

/// A federated node in our cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedNode {
    pub did: String,
    pub pubkey: String,
    pub onion: String,
    #[serde(default)]
    pub name: Option<String>,
    pub trust_level: TrustLevel,
    pub added_at: String,
    #[serde(default)]
    pub last_seen: Option<String>,
    #[serde(default)]
    pub last_state: Option<NodeStateSnapshot>,
    /// FIPS mesh npub (bech32) for this peer, when they advertise one.
    /// Lets the transport router prefer FIPS over Tor for peer traffic.
    #[serde(default)]
    pub fips_npub: Option<String>,
    /// Transport kind used on the most recent successful reach
    /// ("fips" | "tor" | "mesh" | "lan"). Written after each successful
    /// PeerRequest so the UI can show a ground-truth badge ("this peer
    /// is currently being reached over FIPS") instead of a prediction
    /// based on available addresses.
    #[serde(default)]
    pub last_transport: Option<String>,
    /// RFC 3339 timestamp of the last_transport value.
    #[serde(default)]
    pub last_transport_at: Option<String>,
    /// Error from the most recent federation sync attempt with this peer,
    /// `None` when that attempt succeeded. Written back after every attempt
    /// (same shape as `last_transport`, for the failure side) so the
    /// operator can tell a peer that hasn't synced in days from one that
    /// synced a minute ago — previously a failed sync existed only as a
    /// `debug!` log line, making the two indistinguishable in the UI.
    ///
    /// Truncated to `storage::MAX_SYNC_ERROR_CHARS` before it is persisted
    /// so a pathological error can't bloat `nodes.json` on every pass, and
    /// carries only the error's own display string — never credential
    /// material (see FED-02's privacy prohibition).
    #[serde(default)]
    pub last_sync_error: Option<String>,
    /// RFC 3339 timestamp of the last_sync_error value. Cleared together
    /// with `last_sync_error` when the peer recovers.
    #[serde(default)]
    pub last_sync_error_at: Option<String>,
    /// HOW this peer's trust level came to be what it is.
    ///
    /// `None` means "recorded before this field existed" — which is exactly
    /// the population an operator needs to audit, because it is the set that
    /// may have been granted Trusted by the two fail-open paths this field was
    /// added to close (an uninvited `federation.peer-joined`, and transitive
    /// merge). It deliberately does NOT default to a made-up provenance: an
    /// unknown origin must read as unknown, not as `Invite`.
    #[serde(default)]
    pub trust_source: Option<TrustSource>,
}

/// Why a federated node holds the trust level it does.
///
/// Trust must be traceable to an operator decision. Anything that is not is a
/// candidate for review, which is what makes this worth persisting rather than
/// logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrustSource {
    /// Matched an invite THIS node minted — the only path that may grant
    /// `Trusted`. The level is the one the operator chose when minting.
    Invite,
    /// Joined via `federation.peer-joined` without presenting an invite token
    /// we recognise. Capped at `Observer`: the caller is unauthenticated and
    /// its signature only proves it holds the key it just supplied, never that
    /// the operator ever invited it.
    UninvitedJoin,
    /// Learned from a Trusted peer's advertised peer list (transitive merge).
    /// Capped at `Observer`: trust is not transitive, and a peer must not be
    /// able to expand our trusted set on its own authority.
    TransitiveMerge,
    /// Set explicitly by the operator through the federation UI/RPC.
    Manual,
}

/// State snapshot received from a federated peer during sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStateSnapshot {
    pub timestamp: String,
    #[serde(default)]
    pub node_name: Option<String>,
    #[serde(default)]
    pub apps: Vec<AppStatus>,
    #[serde(default)]
    pub cpu_usage_percent: Option<f64>,
    #[serde(default)]
    pub mem_used_bytes: Option<u64>,
    #[serde(default)]
    pub mem_total_bytes: Option<u64>,
    #[serde(default)]
    pub disk_used_bytes: Option<u64>,
    #[serde(default)]
    pub disk_total_bytes: Option<u64>,
    #[serde(default)]
    pub uptime_secs: Option<u64>,
    #[serde(default)]
    pub tor_active: Option<bool>,
    /// bech32-encoded Nostr identity pubkey (npub1…) for cross-transport
    /// peer identification in the mesh UI. Optional: older nodes that
    /// haven't synced after this field was added will report None.
    #[serde(default)]
    pub nostr_npub: Option<String>,
    /// The sender's own FIPS npub (bech32). Lets pre-FIPS federation
    /// pairs — who federated before v1.4 added fips_npub to the invite
    /// code — discover each other's FIPS identity on the next state
    /// sync and route over FIPS from then on. Optional for back-compat
    /// with older peers.
    #[serde(default)]
    pub own_fips_npub: Option<String>,
    /// Minimal summary of peers this node trusts, used for transitive
    /// federation: when Alice syncs with Bob, she learns Bob's trusted
    /// peers and adds them as Observers on her side so `fips_npub` is
    /// known and future state-syncs can route directly. Bounded to one
    /// hop (Alice doesn't auto-promote Observer-via-Bob to Trusted nor
    /// re-export them in her own state snapshots).
    #[serde(default)]
    pub federated_peers: Vec<FederationPeerHint>,
    /// This node's own location, for the Mesh Map — only present when the
    /// sender has opted in via `server.set-location`'s `share` flag. Absent
    /// (not just null) for nodes that haven't opted in, so older receivers
    /// and the map's "no location shared" state both fall out naturally.
    #[serde(default)]
    pub lat: Option<f64>,
    #[serde(default)]
    pub lon: Option<f64>,
}

/// Minimal peer summary shared via `NodeStateSnapshot.federated_peers`.
/// Excludes sensitive/per-receiver fields like trust_level and added_at.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationPeerHint {
    pub did: String,
    pub pubkey: String,
    pub onion: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub fips_npub: Option<String>,
}

/// Status of a single app/container on a remote node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStatus {
    pub id: String,
    pub status: String, // "running", "stopped", "installed"
    #[serde(default)]
    pub version: Option<String>,
}

/// A pending invite (outgoing or incoming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationInvite {
    pub code: String,
    pub did: String,
    pub onion: String,
    pub pubkey: String,
    pub created_at: String,
    #[serde(default)]
    pub accepted: bool,
    /// Inviter's FIPS mesh npub if advertised in the code.
    #[serde(default)]
    pub fips_npub: Option<String>,
    /// Trust level this invite grants both sides ("Invite a Peer" mints
    /// Observer invites, "Link Your Nodes" mints Trusted ones). Defaults
    /// to Trusted for invites minted before this field existed.
    #[serde(default = "default_invite_trust")]
    pub trust_level: TrustLevel,
}

fn default_invite_trust() -> TrustLevel {
    TrustLevel::Trusted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trust_level_serialization() {
        let json = serde_json::to_string(&TrustLevel::Trusted).unwrap();
        assert_eq!(json, "\"trusted\"");

        let parsed: TrustLevel = serde_json::from_str("\"observer\"").unwrap();
        assert_eq!(parsed, TrustLevel::Observer);
    }

    #[test]
    fn test_federated_node_serialization_roundtrip() {
        let node = FederatedNode {
            trust_source: None,
            did: "did:key:zABC".to_string(),
            pubkey: "aabbccdd".to_string(),
            onion: "test.onion".to_string(),
            name: None,
            trust_level: TrustLevel::Trusted,
            added_at: "2026-01-01T00:00:00Z".to_string(),
            last_seen: None,
            last_state: None,
            fips_npub: None,
            last_transport: None,
            last_transport_at: None,
            last_sync_error: None,
            last_sync_error_at: None,
        };
        let json = serde_json::to_string(&node).unwrap();
        let parsed: FederatedNode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.did, "did:key:zABC");
        assert_eq!(parsed.trust_level, TrustLevel::Trusted);
        assert!(parsed.last_state.is_none());
        assert!(parsed.fips_npub.is_none());
    }

    #[test]
    fn test_federated_node_deserializes_without_fips_field() {
        // Backward compat: nodes on older versions omit fips_npub entirely.
        let json = r#"{
            "did": "did:key:zOld",
            "pubkey": "0011",
            "onion": "old.onion",
            "trust_level": "trusted",
            "added_at": "2026-01-01T00:00:00Z"
        }"#;
        let parsed: FederatedNode = serde_json::from_str(json).unwrap();
        assert!(parsed.fips_npub.is_none());
    }

    #[test]
    fn test_node_state_snapshot_defaults() {
        let json = r#"{"timestamp": "2026-01-01T00:00:00Z"}"#;
        let state: NodeStateSnapshot = serde_json::from_str(json).unwrap();
        assert!(state.apps.is_empty());
        assert!(state.cpu_usage_percent.is_none());
    }
}
