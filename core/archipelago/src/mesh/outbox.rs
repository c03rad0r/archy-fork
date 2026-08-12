// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Store-and-forward message queue for mesh networking.
//!
//! When a destination peer is offline or unreachable, messages are queued
//! in the outbox and retried periodically. Messages expire after TTL (24h default).
//! Intermediate nodes can relay messages for peers up to 3 hops away.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Default time-to-live for queued messages (24 hours).
const DEFAULT_TTL_SECS: u64 = 86400;

/// Maximum relay hops for store-and-forward.
const MAX_RELAY_HOPS: u8 = 3;

/// Maximum queued messages to prevent unbounded memory use.
const MAX_QUEUE_SIZE: usize = 200;

const OUTBOX_FILE: &str = "mesh-outbox.json";

/// A message waiting to be delivered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMessage {
    pub id: u64,
    /// Destination peer DID.
    pub dest_did: String,
    /// Encrypted payload bytes (already ratchet-encrypted or static-encrypted).
    #[serde(with = "base64_bytes")]
    pub encrypted_payload: Vec<u8>,
    /// When this message was created (RFC 3339).
    pub created_at: String,
    /// Time-to-live in seconds.
    pub ttl_secs: u64,
    /// Number of times we've attempted delivery.
    pub retry_count: u32,
    /// How many relay hops this message has traversed.
    pub relay_hops: u8,
    /// Original sender DID (for relayed messages).
    pub from_did: String,
}

impl PendingMessage {
    /// Check if this message has expired.
    pub fn is_expired(&self) -> bool {
        let Ok(created) = chrono::DateTime::parse_from_rfc3339(&self.created_at) else {
            return true; // Can't parse = treat as expired
        };
        let age = chrono::Utc::now().signed_duration_since(created);
        // Use `>=` so a ttl_secs=0 message is expired immediately (used by
        // tests and by callers that want a fire-and-forget behavior when
        // the relay can't deliver on first try).
        age.num_seconds() as u64 >= self.ttl_secs
    }

    /// Check if this message can be relayed further.
    pub fn can_relay(&self) -> bool {
        self.relay_hops < MAX_RELAY_HOPS
    }
}

/// Persistent store-and-forward queue.
pub struct MeshOutbox {
    queue: RwLock<VecDeque<PendingMessage>>,
    data_dir: PathBuf,
    next_id: RwLock<u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct OutboxFile {
    messages: Vec<PendingMessage>,
    next_id: u64,
}

impl MeshOutbox {
    /// Load outbox from disk or create empty.
    pub async fn load(data_dir: &Path) -> Result<Self> {
        let path = data_dir.join(OUTBOX_FILE);
        let (messages, next_id) = if path.exists() {
            let content = tokio::fs::read_to_string(&path)
                .await
                .context("Failed to read mesh outbox")?;
            let file: OutboxFile = serde_json::from_str(&content).unwrap_or_default();
            (VecDeque::from(file.messages), file.next_id)
        } else {
            (VecDeque::new(), 1)
        };

        Ok(Self {
            queue: RwLock::new(messages),
            data_dir: data_dir.to_path_buf(),
            next_id: RwLock::new(next_id),
        })
    }

    /// Persist queue to disk.
    pub async fn save(&self) -> Result<()> {
        let queue = self.queue.read().await;
        let next_id = *self.next_id.read().await;
        let file = OutboxFile {
            messages: queue.iter().cloned().collect(),
            next_id,
        };
        let content = serde_json::to_string_pretty(&file).context("Failed to serialize outbox")?;
        tokio::fs::write(self.data_dir.join(OUTBOX_FILE), content)
            .await
            .context("Failed to write outbox")?;
        Ok(())
    }

    /// Enqueue a message for delivery.
    pub async fn enqueue(
        &self,
        dest_did: &str,
        from_did: &str,
        encrypted_payload: Vec<u8>,
        ttl_secs: Option<u64>,
    ) -> Result<u64> {
        let mut next_id = self.next_id.write().await;
        let id = *next_id;
        *next_id += 1;

        let msg = PendingMessage {
            id,
            dest_did: dest_did.to_string(),
            encrypted_payload,
            created_at: chrono::Utc::now().to_rfc3339(),
            ttl_secs: ttl_secs.unwrap_or(DEFAULT_TTL_SECS),
            retry_count: 0,
            relay_hops: 0,
            from_did: from_did.to_string(),
        };

        let mut queue = self.queue.write().await;
        // Evict oldest if over limit
        while queue.len() >= MAX_QUEUE_SIZE {
            queue.pop_front();
        }
        queue.push_back(msg);

        info!(id = id, dest = %dest_did, "Message queued for delivery");
        Ok(id)
    }

    /// Enqueue a relayed message (from another peer, not originated by us).
    pub async fn enqueue_relay(&self, mut msg: PendingMessage) -> Result<()> {
        if !msg.can_relay() {
            anyhow::bail!("Message exceeded max relay hops ({})", MAX_RELAY_HOPS);
        }
        msg.relay_hops += 1;

        let mut queue = self.queue.write().await;
        while queue.len() >= MAX_QUEUE_SIZE {
            queue.pop_front();
        }
        queue.push_back(msg);
        Ok(())
    }

    /// Remove expired messages from the queue.
    pub async fn expire_stale(&self) -> usize {
        let mut queue = self.queue.write().await;
        let before = queue.len();
        queue.retain(|msg| !msg.is_expired());
        let expired = before - queue.len();
        if expired > 0 {
            debug!(expired = expired, "Expired stale outbox messages");
        }
        expired
    }

    /// Get messages pending for a specific peer.
    pub async fn messages_for_peer(&self, did: &str) -> Vec<PendingMessage> {
        self.queue
            .read()
            .await
            .iter()
            .filter(|m| m.dest_did == did)
            .cloned()
            .collect()
    }

    /// Mark a message as delivered (remove from queue).
    pub async fn mark_delivered(&self, id: u64) -> bool {
        let mut queue = self.queue.write().await;
        let before = queue.len();
        queue.retain(|m| m.id != id);
        queue.len() < before
    }

    /// Increment retry count for a message.
    pub async fn increment_retry(&self, id: u64) {
        let mut queue = self.queue.write().await;
        if let Some(msg) = queue.iter_mut().find(|m| m.id == id) {
            msg.retry_count += 1;
        }
    }

    /// Get all pending messages (for RPC status).
    pub async fn list(&self, limit: Option<usize>) -> Vec<PendingMessage> {
        let queue = self.queue.read().await;
        let limit = limit.unwrap_or(50);
        queue.iter().take(limit).cloned().collect()
    }

    /// Count of pending messages.
    pub async fn count(&self) -> usize {
        self.queue.read().await.len()
    }
}

// ─── base64 serde for encrypted payloads ────────────────────────────────

mod base64_bytes {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        s.serialize_str(&encoded)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD
            .decode(&s)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_enqueue_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let outbox = MeshOutbox::load(dir.path()).await.unwrap();

        let id = outbox
            .enqueue("did:key:z6MkDest", "did:key:z6MkSelf", vec![1, 2, 3], None)
            .await
            .unwrap();

        assert_eq!(outbox.count().await, 1);
        let msgs = outbox.list(None).await;
        assert_eq!(msgs[0].id, id);
        assert_eq!(msgs[0].dest_did, "did:key:z6MkDest");
    }

    #[tokio::test]
    async fn test_mark_delivered() {
        let dir = tempfile::tempdir().unwrap();
        let outbox = MeshOutbox::load(dir.path()).await.unwrap();

        let id = outbox
            .enqueue("did:key:z6MkDest", "did:key:z6MkSelf", vec![1], None)
            .await
            .unwrap();

        assert!(outbox.mark_delivered(id).await);
        assert_eq!(outbox.count().await, 0);
    }

    #[tokio::test]
    async fn test_expire_stale() {
        let dir = tempfile::tempdir().unwrap();
        let outbox = MeshOutbox::load(dir.path()).await.unwrap();

        // Enqueue with 0 TTL (immediately expired)
        outbox
            .enqueue("did:key:z6MkDest", "did:key:z6MkSelf", vec![1], Some(0))
            .await
            .unwrap();

        let expired = outbox.expire_stale().await;
        assert_eq!(expired, 1);
        assert_eq!(outbox.count().await, 0);
    }

    #[tokio::test]
    async fn test_persistence_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let outbox = MeshOutbox::load(dir.path()).await.unwrap();

        outbox
            .enqueue(
                "did:key:z6MkDest",
                "did:key:z6MkSelf",
                vec![42, 43, 44],
                None,
            )
            .await
            .unwrap();
        outbox.save().await.unwrap();

        // Reload
        let outbox2 = MeshOutbox::load(dir.path()).await.unwrap();
        assert_eq!(outbox2.count().await, 1);
        let msgs = outbox2.list(None).await;
        assert_eq!(msgs[0].encrypted_payload, vec![42, 43, 44]);
    }

    #[tokio::test]
    async fn test_max_queue_size() {
        let dir = tempfile::tempdir().unwrap();
        let outbox = MeshOutbox::load(dir.path()).await.unwrap();

        for i in 0..210 {
            outbox
                .enqueue("did:key:z6MkDest", "did:key:z6MkSelf", vec![i as u8], None)
                .await
                .unwrap();
        }

        // Should cap at MAX_QUEUE_SIZE
        assert!(outbox.count().await <= 200);
    }

    #[test]
    fn test_relay_hops() {
        let msg = PendingMessage {
            id: 1,
            dest_did: "did:key:test".to_string(),
            encrypted_payload: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            ttl_secs: 86400,
            retry_count: 0,
            relay_hops: 2,
            from_did: "did:key:sender".to_string(),
        };
        assert!(msg.can_relay()); // 2 < 3

        let msg2 = PendingMessage {
            relay_hops: 3,
            ..msg
        };
        assert!(!msg2.can_relay()); // 3 >= 3
    }
}
