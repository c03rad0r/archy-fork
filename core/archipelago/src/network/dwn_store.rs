//! DWN message store — persists DWN messages as JSON files on disk.
//!
//! Implements core CRUD operations, protocol registration, and query interface
//! for the Decentralized Web Node spec.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::debug;
use uuid::Uuid;

/// A DWN message descriptor following the spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDescriptor {
    pub interface: String,
    pub method: String,
    pub protocol: Option<String>,
    pub schema: Option<String>,
    #[serde(rename = "dateCreated")]
    pub date_created: String,
    #[serde(rename = "dateModified")]
    pub date_modified: Option<String>,
    #[serde(rename = "dataFormat")]
    pub data_format: Option<String>,
}

/// A stored DWN message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DwnMessage {
    pub record_id: String,
    pub descriptor: MessageDescriptor,
    pub author: String,
    pub data: Option<serde_json::Value>,
    #[serde(rename = "dateCreated")]
    pub date_created: String,
}

/// A registered protocol definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolDefinition {
    pub protocol: String,
    pub published: bool,
    pub types: HashMap<String, serde_json::Value>,
    pub structure: HashMap<String, serde_json::Value>,
    #[serde(rename = "dateRegistered")]
    pub date_registered: String,
}

/// Query parameters for searching messages.
#[derive(Debug, Default)]
pub struct MessageQuery {
    pub protocol: Option<String>,
    pub schema: Option<String>,
    pub author: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub limit: Option<usize>,
}

/// The DWN message store backed by the filesystem.
pub struct DwnStore {
    messages_dir: PathBuf,
    protocols_dir: PathBuf,
}

impl DwnStore {
    /// Create a new DWN store at the given data directory.
    pub async fn new(data_dir: &Path) -> Result<Self> {
        let messages_dir = data_dir.join("dwn/messages");
        let protocols_dir = data_dir.join("dwn/protocols");
        fs::create_dir_all(&messages_dir)
            .await
            .context("Failed to create DWN messages dir")?;
        fs::create_dir_all(&protocols_dir)
            .await
            .context("Failed to create DWN protocols dir")?;
        Ok(Self {
            messages_dir,
            protocols_dir,
        })
    }

    /// Write a new message or update an existing one.
    pub async fn write_message(
        &self,
        author: &str,
        protocol: Option<&str>,
        schema: Option<&str>,
        data_format: Option<&str>,
        data: Option<serde_json::Value>,
    ) -> Result<DwnMessage> {
        let now = chrono::Utc::now().to_rfc3339();
        let record_id = Uuid::new_v4().to_string();

        let message = DwnMessage {
            record_id: record_id.clone(),
            descriptor: MessageDescriptor {
                interface: "Records".to_string(),
                method: "Write".to_string(),
                protocol: protocol.map(|s| s.to_string()),
                schema: schema.map(|s| s.to_string()),
                date_created: now.clone(),
                date_modified: Some(now.clone()),
                data_format: data_format.map(|s| s.to_string()),
            },
            author: author.to_string(),
            data,
            date_created: now,
        };

        let path = self.messages_dir.join(format!("{}.json", record_id));
        let content =
            serde_json::to_string_pretty(&message).context("Failed to serialize message")?;
        fs::write(&path, content)
            .await
            .context("Failed to write message file")?;

        debug!(record_id = %message.record_id, "DWN message written");
        Ok(message)
    }

    /// Validate a record ID to prevent path traversal.
    fn validate_record_id(record_id: &str) -> Result<()> {
        if record_id.is_empty()
            || record_id.len() > 128
            || !record_id
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(anyhow::anyhow!(
                "Invalid record ID (alphanumeric, hyphens, underscores only)"
            ));
        }
        Ok(())
    }

    /// Read a message by record ID.
    pub async fn read_message(&self, record_id: &str) -> Result<Option<DwnMessage>> {
        Self::validate_record_id(record_id)?;
        let path = self.messages_dir.join(format!("{}.json", record_id));
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
            .await
            .context("Failed to read message file")?;
        let message: DwnMessage =
            serde_json::from_str(&content).context("Failed to parse message")?;
        Ok(Some(message))
    }

    /// Delete a message by record ID.
    pub async fn delete_message(&self, record_id: &str) -> Result<bool> {
        Self::validate_record_id(record_id)?;
        let path = self.messages_dir.join(format!("{}.json", record_id));
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(&path)
            .await
            .context("Failed to delete message file")?;
        debug!(record_id = %record_id, "DWN message deleted");
        Ok(true)
    }

    /// Query messages by various criteria.
    pub async fn query_messages(&self, query: &MessageQuery) -> Result<Vec<DwnMessage>> {
        let mut results = Vec::new();
        let mut entries = fs::read_dir(&self.messages_dir)
            .await
            .context("Failed to read messages dir")?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            let content = match fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(_) => continue,
            };
            let message: DwnMessage = match serde_json::from_str(&content) {
                Ok(m) => m,
                Err(_) => continue,
            };

            if let Some(ref proto) = query.protocol {
                if message.descriptor.protocol.as_deref() != Some(proto) {
                    continue;
                }
            }
            if let Some(ref schema) = query.schema {
                if message.descriptor.schema.as_deref() != Some(schema) {
                    continue;
                }
            }
            if let Some(ref author) = query.author {
                if &message.author != author {
                    continue;
                }
            }
            if let Some(ref from) = query.date_from {
                if message.date_created < *from {
                    continue;
                }
            }
            if let Some(ref to) = query.date_to {
                if message.date_created > *to {
                    continue;
                }
            }

            results.push(message);
        }

        // Sort by date descending (newest first)
        results.sort_by(|a, b| b.date_created.cmp(&a.date_created));

        if let Some(limit) = query.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    /// Register a protocol definition.
    pub async fn register_protocol(&self, definition: &ProtocolDefinition) -> Result<()> {
        if definition.protocol.is_empty() {
            bail!("Protocol URI cannot be empty");
        }
        let safe_name = definition.protocol.replace(['/', ':', '.'], "_");
        let path = self.protocols_dir.join(format!("{}.json", safe_name));
        let content =
            serde_json::to_string_pretty(definition).context("Failed to serialize protocol")?;
        fs::write(&path, content)
            .await
            .context("Failed to write protocol file")?;
        debug!(protocol = %definition.protocol, "Protocol registered");
        Ok(())
    }

    /// List all registered protocols.
    pub async fn list_protocols(&self) -> Result<Vec<ProtocolDefinition>> {
        let mut protocols = Vec::new();
        let mut entries = fs::read_dir(&self.protocols_dir)
            .await
            .context("Failed to read protocols dir")?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let content = match fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(_) => continue,
            };
            if let Ok(proto) = serde_json::from_str::<ProtocolDefinition>(&content) {
                protocols.push(proto);
            }
        }

        Ok(protocols)
    }

    /// Remove a registered protocol.
    pub async fn remove_protocol(&self, protocol_uri: &str) -> Result<bool> {
        let safe_name = protocol_uri.replace(['/', ':', '.'], "_");
        let path = self.protocols_dir.join(format!("{}.json", safe_name));
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(&path)
            .await
            .context("Failed to remove protocol file")?;
        debug!(protocol = %protocol_uri, "Protocol removed");
        Ok(true)
    }

    /// Get storage statistics.
    pub async fn stats(&self) -> Result<StoreStats> {
        let mut message_count: u64 = 0;
        let mut total_bytes: u64 = 0;

        let mut entries = fs::read_dir(&self.messages_dir)
            .await
            .context("Failed to read messages dir")?;

        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                message_count += 1;
                if let Ok(meta) = entry.metadata().await {
                    total_bytes += meta.len();
                }
            }
        }

        let protocol_count = self.list_protocols().await?.len() as u64;

        Ok(StoreStats {
            message_count,
            protocol_count,
            total_bytes,
        })
    }
}

/// Storage statistics.
#[derive(Debug, Serialize, Deserialize)]
pub struct StoreStats {
    pub message_count: u64,
    pub protocol_count: u64,
    pub total_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup() -> (TempDir, DwnStore) {
        let dir = TempDir::new().unwrap();
        let store = DwnStore::new(dir.path()).await.unwrap();
        (dir, store)
    }

    #[tokio::test]
    async fn write_and_read_message() {
        let (_dir, store) = setup().await;
        let msg = store
            .write_message(
                "did:key:test",
                Some("proto://chat"),
                None,
                None,
                Some(serde_json::json!({"text": "hello"})),
            )
            .await
            .unwrap();
        assert!(!msg.record_id.is_empty());

        let read = store.read_message(&msg.record_id).await.unwrap();
        assert!(read.is_some());
        let read = read.unwrap();
        assert_eq!(read.author, "did:key:test");
        assert_eq!(read.data, Some(serde_json::json!({"text": "hello"})));
    }

    #[tokio::test]
    async fn read_nonexistent_returns_none() {
        let (_dir, store) = setup().await;
        let read = store.read_message("nonexistent-id").await.unwrap();
        assert!(read.is_none());
    }

    #[tokio::test]
    async fn delete_message() {
        let (_dir, store) = setup().await;
        let msg = store
            .write_message("did:key:test", None, None, None, None)
            .await
            .unwrap();
        assert!(store.delete_message(&msg.record_id).await.unwrap());
        assert!(!store.delete_message(&msg.record_id).await.unwrap());
        assert!(store.read_message(&msg.record_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn query_by_protocol() {
        let (_dir, store) = setup().await;
        store
            .write_message("did:key:a", Some("proto://chat"), None, None, None)
            .await
            .unwrap();
        store
            .write_message("did:key:a", Some("proto://files"), None, None, None)
            .await
            .unwrap();
        store
            .write_message("did:key:b", Some("proto://chat"), None, None, None)
            .await
            .unwrap();

        let results = store
            .query_messages(&MessageQuery {
                protocol: Some("proto://chat".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn query_by_author() {
        let (_dir, store) = setup().await;
        store
            .write_message("did:key:a", None, None, None, None)
            .await
            .unwrap();
        store
            .write_message("did:key:b", None, None, None, None)
            .await
            .unwrap();

        let results = store
            .query_messages(&MessageQuery {
                author: Some("did:key:a".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].author, "did:key:a");
    }

    #[tokio::test]
    async fn query_with_limit() {
        let (_dir, store) = setup().await;
        for i in 0..5 {
            store
                .write_message(&format!("did:key:{}", i), None, None, None, None)
                .await
                .unwrap();
        }

        let results = store
            .query_messages(&MessageQuery {
                limit: Some(3),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn register_and_list_protocols() {
        let (_dir, store) = setup().await;
        let proto = ProtocolDefinition {
            protocol: "https://example.com/chat".to_string(),
            published: true,
            types: HashMap::new(),
            structure: HashMap::new(),
            date_registered: chrono::Utc::now().to_rfc3339(),
        };
        store.register_protocol(&proto).await.unwrap();

        let list = store.list_protocols().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].protocol, "https://example.com/chat");
    }

    #[tokio::test]
    async fn remove_protocol() {
        let (_dir, store) = setup().await;
        let proto = ProtocolDefinition {
            protocol: "https://example.com/test".to_string(),
            published: false,
            types: HashMap::new(),
            structure: HashMap::new(),
            date_registered: chrono::Utc::now().to_rfc3339(),
        };
        store.register_protocol(&proto).await.unwrap();
        assert!(store
            .remove_protocol("https://example.com/test")
            .await
            .unwrap());
        assert!(!store
            .remove_protocol("https://example.com/test")
            .await
            .unwrap());
        assert!(store.list_protocols().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn store_stats() {
        let (_dir, store) = setup().await;
        store
            .write_message("did:key:a", None, None, None, None)
            .await
            .unwrap();
        store
            .write_message("did:key:b", None, None, None, None)
            .await
            .unwrap();

        let stats = store.stats().await.unwrap();
        assert_eq!(stats.message_count, 2);
        assert!(stats.total_bytes > 0);
    }
}
