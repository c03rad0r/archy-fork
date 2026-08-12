//! Known peer nodes for P2P discovery and connection.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownPeer {
    pub onion: String,
    pub pubkey: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub added_at: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PeersFile {
    pub peers: Vec<KnownPeer>,
}

const PEERS_FILE: &str = "peers.json";

pub async fn load_peers(data_dir: &Path) -> Result<Vec<KnownPeer>> {
    let path = data_dir.join(PEERS_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read peers file")?;
    let file: PeersFile = serde_json::from_str(&content).unwrap_or_default();
    Ok(file.peers)
}

pub async fn save_peers(data_dir: &Path, peers: &[KnownPeer]) -> Result<()> {
    let path = data_dir.join(PEERS_FILE);
    fs::create_dir_all(data_dir)
        .await
        .context("Failed to create data dir")?;
    let file = PeersFile {
        peers: peers.to_vec(),
    };
    let content = serde_json::to_string_pretty(&file).context("Failed to serialize peers")?;
    fs::write(&path, content)
        .await
        .context("Failed to write peers file")?;
    Ok(())
}

pub async fn add_peer(data_dir: &Path, peer: KnownPeer) -> Result<Vec<KnownPeer>> {
    // Self-add guard: skip if the candidate pubkey matches our own identity.
    // Auto-peering paths (e.g., node-message receive handler) can otherwise
    // echo-back our own pubkey when messages bounce through federation, which
    // ended up in users' peer lists as a phantom "self-peer" entry.
    if is_own_pubkey(data_dir, &peer.pubkey).await {
        return load_peers(data_dir).await;
    }
    let mut peers = load_peers(data_dir).await?;
    let exists = peers.iter().any(|p| p.pubkey == peer.pubkey);
    if !exists {
        peers.push(peer);
        save_peers(data_dir, &peers).await?;
    }
    Ok(peers)
}

/// Reads `data_dir/identity/node_key.pub` and compares to `candidate` (hex).
/// Returns false on any I/O or format issue — guard is best-effort; a real
/// peer with a colliding pubkey is impossible, so a false negative just
/// means the entry is added normally.
async fn is_own_pubkey(data_dir: &Path, candidate: &str) -> bool {
    let path = data_dir.join("identity/node_key.pub");
    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(_) => return false,
    };
    let own_hex = hex::encode(&bytes);
    own_hex.eq_ignore_ascii_case(candidate.trim())
}

pub async fn remove_peer(data_dir: &Path, pubkey: &str) -> Result<Vec<KnownPeer>> {
    let mut peers = load_peers(data_dir).await?;
    peers.retain(|p| p.pubkey != pubkey);
    save_peers(data_dir, &peers).await?;
    Ok(peers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_peer(pubkey: &str, onion: &str) -> KnownPeer {
        KnownPeer {
            onion: onion.to_string(),
            pubkey: pubkey.to_string(),
            name: None,
            added_at: None,
        }
    }

    #[test]
    fn test_peers_file_default_is_empty() {
        let pf = PeersFile::default();
        assert!(pf.peers.is_empty());
    }

    #[test]
    fn test_known_peer_serialization_roundtrip() {
        let peer = KnownPeer {
            onion: "abc123.onion".to_string(),
            pubkey: "02aabbcc".to_string(),
            name: Some("My Node".to_string()),
            added_at: Some("2025-01-01T00:00:00Z".to_string()),
        };
        let json = serde_json::to_string(&peer).unwrap();
        let parsed: KnownPeer = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.onion, "abc123.onion");
        assert_eq!(parsed.pubkey, "02aabbcc");
        assert_eq!(parsed.name, Some("My Node".to_string()));
    }

    #[test]
    fn test_known_peer_optional_fields_default() {
        // name and added_at should default to None when missing
        let json = r#"{"onion": "test.onion", "pubkey": "deadbeef"}"#;
        let peer: KnownPeer = serde_json::from_str(json).unwrap();
        assert!(peer.name.is_none());
        assert!(peer.added_at.is_none());
    }

    #[tokio::test]
    async fn test_load_peers_returns_empty_when_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let peers = load_peers(dir.path()).await.unwrap();
        assert!(peers.is_empty());
    }

    #[tokio::test]
    async fn test_save_and_load_peers_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let peers = vec![
            make_peer("pub1", "onion1.onion"),
            make_peer("pub2", "onion2.onion"),
        ];
        save_peers(dir.path(), &peers).await.unwrap();
        let loaded = load_peers(dir.path()).await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].pubkey, "pub1");
        assert_eq!(loaded[1].onion, "onion2.onion");
    }

    #[tokio::test]
    async fn test_add_peer_appends_new() {
        let dir = tempfile::tempdir().unwrap();
        let peer = make_peer("pubkey-a", "a.onion");
        let result = add_peer(dir.path(), peer).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pubkey, "pubkey-a");

        // Add a second peer
        let peer2 = make_peer("pubkey-b", "b.onion");
        let result = add_peer(dir.path(), peer2).await.unwrap();
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_add_peer_deduplicates_by_pubkey() {
        let dir = tempfile::tempdir().unwrap();
        let peer = make_peer("same-key", "first.onion");
        add_peer(dir.path(), peer).await.unwrap();

        // Adding a peer with the same pubkey should not duplicate
        let peer_dup = make_peer("same-key", "second.onion");
        let result = add_peer(dir.path(), peer_dup).await.unwrap();
        assert_eq!(result.len(), 1);
        // Original should be kept (not replaced)
        assert_eq!(result[0].onion, "first.onion");
    }

    #[tokio::test]
    async fn test_remove_peer_by_pubkey() {
        let dir = tempfile::tempdir().unwrap();
        add_peer(dir.path(), make_peer("key-1", "a.onion"))
            .await
            .unwrap();
        add_peer(dir.path(), make_peer("key-2", "b.onion"))
            .await
            .unwrap();
        add_peer(dir.path(), make_peer("key-3", "c.onion"))
            .await
            .unwrap();

        let result = remove_peer(dir.path(), "key-2").await.unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|p| p.pubkey != "key-2"));
    }

    #[tokio::test]
    async fn test_remove_nonexistent_peer_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        add_peer(dir.path(), make_peer("key-1", "a.onion"))
            .await
            .unwrap();

        // Removing a pubkey that doesn't exist should succeed but not change the list
        let result = remove_peer(dir.path(), "nonexistent").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pubkey, "key-1");
    }
}
