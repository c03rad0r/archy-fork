use super::RpcHandler;
use crate::federation;
use crate::network::dwn_store::{DwnStore, MessageQuery, ProtocolDefinition};
use crate::network::dwn_sync;
use anyhow::Result;

impl RpcHandler {
    /// Get DWN status and sync state.
    pub(super) async fn handle_dwn_status(&self) -> Result<serde_json::Value> {
        let sync_state = dwn_sync::load_sync_state(&self.config.data_dir).await?;
        let server_status =
            dwn_sync::get_dwn_status()
                .await
                .unwrap_or(dwn_sync::DwnStatusResponse {
                    running: false,
                    version: String::new(),
                });

        let store = DwnStore::new(&self.config.data_dir).await?;
        let stats = store.stats().await?;

        Ok(serde_json::json!({
            "running": server_status.running,
            "version": server_status.version,
            "sync_status": sync_state.status,
            "last_sync": sync_state.last_sync,
            "messages_synced": sync_state.messages_synced,
            "storage_bytes": stats.total_bytes,
            "message_count": stats.message_count,
            "protocol_count": stats.protocol_count,
            "registered_protocols": sync_state.registered_protocols,
            "peer_sync_targets": sync_state.peer_sync_targets,
        }))
    }

    /// Trigger DWN sync with connected peers.
    /// Spawns sync as a background task and returns immediately.
    pub(super) async fn handle_dwn_sync(&self) -> Result<serde_json::Value> {
        // Check if already syncing
        let current_state = dwn_sync::load_sync_state(&self.config.data_dir).await?;
        if matches!(current_state.status, dwn_sync::SyncStatus::Syncing) {
            return Ok(serde_json::json!({
                "sync_status": "syncing",
                "last_sync": current_state.last_sync,
                "messages_synced": current_state.messages_synced,
            }));
        }

        let nodes = federation::load_nodes(&self.config.data_dir).await?;
        let onions: Vec<String> = nodes
            .iter()
            .filter(|n| !n.onion.is_empty() && n.trust_level != federation::TrustLevel::Untrusted)
            .map(|n| n.onion.clone())
            .collect();

        // Spawn sync in background so we don't block the RPC response
        let data_dir = self.config.data_dir.clone();
        tokio::spawn(async move {
            if let Err(e) = dwn_sync::sync_with_peers(&data_dir, &onions).await {
                tracing::warn!(error = %e, "DWN background sync failed");
            }
        });

        // Return immediately with "syncing" status
        Ok(serde_json::json!({
            "sync_status": "syncing",
            "last_sync": current_state.last_sync,
            "messages_synced": current_state.messages_synced,
        }))
    }

    /// Register a DWN protocol.
    pub(super) async fn handle_dwn_register_protocol(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let protocol = params["protocol"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'protocol' parameter"))?;
        let published = params["published"].as_bool().unwrap_or(false);

        let definition = ProtocolDefinition {
            protocol: protocol.to_string(),
            published,
            types: params
                .get("types")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            structure: params
                .get("structure")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            date_registered: chrono::Utc::now().to_rfc3339(),
        };

        let store = DwnStore::new(&self.config.data_dir).await?;
        store.register_protocol(&definition).await?;

        Ok(serde_json::json!({"registered": true, "protocol": protocol}))
    }

    /// List registered DWN protocols.
    pub(super) async fn handle_dwn_list_protocols(&self) -> Result<serde_json::Value> {
        let store = DwnStore::new(&self.config.data_dir).await?;
        let protocols = store.list_protocols().await?;
        Ok(serde_json::json!({"protocols": protocols}))
    }

    /// Remove a DWN protocol.
    pub(super) async fn handle_dwn_remove_protocol(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let protocol = params["protocol"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'protocol' parameter"))?;

        let store = DwnStore::new(&self.config.data_dir).await?;
        let removed = store.remove_protocol(protocol).await?;

        Ok(serde_json::json!({"removed": removed, "protocol": protocol}))
    }

    /// Query DWN messages.
    pub(super) async fn handle_dwn_query_messages(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let query = MessageQuery {
            protocol: params["protocol"].as_str().map(|s| s.to_string()),
            schema: params["schema"].as_str().map(|s| s.to_string()),
            author: params["author"].as_str().map(|s| s.to_string()),
            date_from: params["dateFrom"].as_str().map(|s| s.to_string()),
            date_to: params["dateTo"].as_str().map(|s| s.to_string()),
            limit: params["limit"].as_u64().map(|n| n as usize),
        };

        let store = DwnStore::new(&self.config.data_dir).await?;
        let messages = store.query_messages(&query).await?;

        Ok(serde_json::json!({"messages": messages, "count": messages.len()}))
    }

    /// Write a DWN message.
    pub(super) async fn handle_dwn_write_message(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let author = params["author"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'author' parameter"))?;
        let protocol = params["protocol"].as_str();
        let schema = params["schema"].as_str();
        let data_format = params["dataFormat"].as_str();
        let data = params.get("data").cloned();

        // Limit data size to 10MB to prevent disk exhaustion
        if let Some(ref d) = data {
            let data_str = d.to_string();
            if data_str.len() > 10_485_760 {
                anyhow::bail!("Message data too large (max 10MB)");
            }
        }

        let store = DwnStore::new(&self.config.data_dir).await?;
        let message = store
            .write_message(author, protocol, schema, data_format, data)
            .await?;

        Ok(serde_json::json!({"written": true, "record_id": message.record_id}))
    }
}
