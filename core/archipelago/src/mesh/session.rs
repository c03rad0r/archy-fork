//! Per-peer session manager for Double Ratchet state persistence.
//!
//! Each peer gets a separate ratchet session stored on disk at
//! `{data_dir}/ratchet/{did_hash}.json`. Sessions are loaded lazily
//! on first message and saved after each encrypt/decrypt operation.

use super::ratchet::RatchetState;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;
use tracing::{debug, warn};

const RATCHET_DIR: &str = "ratchet";

/// Thread-safe manager for per-peer ratchet sessions.
pub struct SessionManager {
    sessions: RwLock<HashMap<String, RatchetState>>,
    data_dir: PathBuf,
}

impl SessionManager {
    /// Create a new session manager. Does not load sessions from disk yet.
    pub fn new(data_dir: &Path) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            data_dir: data_dir.to_path_buf(),
        }
    }

    /// Hash a DID to a filesystem-safe filename (16 hex chars).
    fn did_hash(did: &str) -> String {
        let hash = Sha256::digest(did.as_bytes());
        hex::encode(&hash[..8])
    }

    /// Path to a session file for a given DID.
    fn session_path(&self, did: &str) -> PathBuf {
        self.data_dir
            .join(RATCHET_DIR)
            .join(format!("{}.json", Self::did_hash(did)))
    }

    /// Load a session from disk if it exists.
    async fn load_session(&self, did: &str) -> Result<Option<RatchetState>> {
        let path = self.session_path(did);
        if !path.exists() {
            return Ok(None);
        }
        let content = tokio::fs::read_to_string(&path)
            .await
            .context("Failed to read ratchet session")?;
        let state: RatchetState =
            serde_json::from_str(&content).context("Failed to deserialize ratchet session")?;
        debug!(did = %did, "Loaded ratchet session from disk");
        Ok(Some(state))
    }

    /// Save a session to disk.
    async fn save_session_to_disk(&self, did: &str, state: &RatchetState) -> Result<()> {
        let dir = self.data_dir.join(RATCHET_DIR);
        tokio::fs::create_dir_all(&dir)
            .await
            .context("Failed to create ratchet directory")?;
        let path = self.session_path(did);
        let tmp_path = path.with_extension("tmp");
        let content =
            serde_json::to_string_pretty(state).context("Failed to serialize ratchet session")?;
        // Atomic write: write to temp file, then rename
        tokio::fs::write(&tmp_path, content)
            .await
            .context("Failed to write temporary ratchet state")?;
        tokio::fs::rename(&tmp_path, &path)
            .await
            .context("Failed to atomically rename ratchet state file")?;
        debug!(did = %did, "Saved ratchet session to disk (atomic)");
        Ok(())
    }

    /// Check if a ratchet session exists for a peer (in memory or on disk).
    pub async fn has_session(&self, did: &str) -> bool {
        let sessions = self.sessions.read().await;
        if sessions.contains_key(did) {
            return true;
        }
        self.session_path(did).exists()
    }

    /// Encrypt a message for a peer using their ratchet session.
    /// Loads the session from disk if not in memory.
    pub async fn encrypt_for_peer(
        &self,
        did: &str,
        plaintext: &[u8],
    ) -> Result<super::ratchet::RatchetMessage> {
        let mut sessions = self.sessions.write().await;

        // Lazy load from disk if not in memory
        if !sessions.contains_key(did) {
            if let Some(state) = self.load_session(did).await? {
                sessions.insert(did.to_string(), state);
            } else {
                anyhow::bail!("No ratchet session for peer {}", did);
            }
        }

        let state = sessions
            .get_mut(did)
            .ok_or_else(|| anyhow::anyhow!("Session disappeared"))?;

        let message = state.encrypt(plaintext)?;

        // Save updated state after encryption (chain key advanced)
        drop(sessions);
        let sessions = self.sessions.read().await;
        if let Some(state) = sessions.get(did) {
            if let Err(e) = self.save_session_to_disk(did, state).await {
                warn!(did = %did, error = %e, "Failed to save session after encrypt");
            }
        }

        Ok(message)
    }

    /// Decrypt a message from a peer using their ratchet session.
    pub async fn decrypt_from_peer(
        &self,
        did: &str,
        message: &super::ratchet::RatchetMessage,
    ) -> Result<Vec<u8>> {
        let mut sessions = self.sessions.write().await;

        // Lazy load from disk if not in memory
        if !sessions.contains_key(did) {
            if let Some(state) = self.load_session(did).await? {
                sessions.insert(did.to_string(), state);
            } else {
                anyhow::bail!("No ratchet session for peer {}", did);
            }
        }

        let state = sessions
            .get_mut(did)
            .ok_or_else(|| anyhow::anyhow!("Session disappeared"))?;

        let plaintext = state.decrypt(message)?;

        // Save updated state after decryption
        drop(sessions);
        let sessions = self.sessions.read().await;
        if let Some(state) = sessions.get(did) {
            if let Err(e) = self.save_session_to_disk(did, state).await {
                warn!(did = %did, error = %e, "Failed to save session after decrypt");
            }
        }

        Ok(plaintext)
    }

    /// Store a ratchet session for a peer (in memory and on disk).
    #[allow(dead_code)]
    pub async fn store_session(&self, did: &str, state: RatchetState) -> Result<()> {
        self.save_session_to_disk(did, &state).await?;
        let mut sessions = self.sessions.write().await;
        sessions.insert(did.to_string(), state);
        Ok(())
    }

    /// Remove a ratchet session for a peer (from memory and disk).
    #[allow(dead_code)]
    pub async fn remove_session(&self, did: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(did);
        let path = self.session_path(did);
        if path.exists() {
            tokio::fs::remove_file(&path)
                .await
                .context("Failed to remove ratchet session file")?;
        }
        Ok(())
    }

    /// Get session info for a peer (for RPC status endpoint).
    pub async fn session_info(&self, did: &str) -> Option<SessionInfo> {
        let sessions = self.sessions.read().await;
        if let Some(state) = sessions.get(did) {
            return Some(SessionInfo {
                has_session: true,
                forward_secrecy: true,
                message_count: state.total_sent(),
                ratchet_generation: state.generation(),
            });
        }
        // Check disk
        if self.session_path(did).exists() {
            Some(SessionInfo {
                has_session: true,
                forward_secrecy: true,
                message_count: 0, // Would need to load to get exact count
                ratchet_generation: 0,
            })
        } else {
            None
        }
    }
}

/// Summary info about a ratchet session (returned via RPC).
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionInfo {
    pub has_session: bool,
    pub forward_secrecy: bool,
    pub message_count: u32,
    pub ratchet_generation: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::crypto;
    use crate::mesh::ratchet::RatchetState;

    #[tokio::test]
    async fn test_session_store_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SessionManager::new(dir.path());

        let root_key = [42u8; 32];
        let (_spk_secret, spk_public) = crypto::generate_x25519_ephemeral();
        let state = RatchetState::init_as_sender(root_key, &spk_public).unwrap();

        let did = "did:key:z6MkTestSession";
        mgr.store_session(did, state).await.unwrap();

        assert!(mgr.has_session(did).await);

        // Drop and reload
        let mgr2 = SessionManager::new(dir.path());
        assert!(mgr2.has_session(did).await);
    }

    #[tokio::test]
    async fn test_encrypt_decrypt_through_manager() {
        let dir = tempfile::tempdir().unwrap();
        let alice_mgr = SessionManager::new(dir.path());

        let dir2 = tempfile::tempdir().unwrap();
        let bob_mgr = SessionManager::new(dir2.path());

        let root_key = [55u8; 32];
        let (spk_secret, spk_public) = crypto::generate_x25519_ephemeral();

        let alice_state = RatchetState::init_as_sender(root_key, &spk_public).unwrap();
        let bob_state = RatchetState::init_as_receiver(root_key, spk_secret, spk_public);

        let alice_did = "did:key:z6MkAlice";
        let bob_did = "did:key:z6MkBob";

        alice_mgr.store_session(bob_did, alice_state).await.unwrap();
        bob_mgr.store_session(alice_did, bob_state).await.unwrap();

        // Alice encrypts
        let msg = alice_mgr
            .encrypt_for_peer(bob_did, b"Hello via manager")
            .await
            .unwrap();

        // Bob decrypts
        let plain = bob_mgr.decrypt_from_peer(alice_did, &msg).await.unwrap();
        assert_eq!(plain, b"Hello via manager");
    }

    #[tokio::test]
    async fn test_remove_session() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SessionManager::new(dir.path());

        let root_key = [33u8; 32];
        let (_, spk_public) = crypto::generate_x25519_ephemeral();
        let state = RatchetState::init_as_sender(root_key, &spk_public).unwrap();

        let did = "did:key:z6MkRemoveMe";
        mgr.store_session(did, state).await.unwrap();
        assert!(mgr.has_session(did).await);

        mgr.remove_session(did).await.unwrap();
        assert!(!mgr.has_session(did).await);
    }
}
