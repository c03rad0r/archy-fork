use super::RpcHandler;
use crate::nostr_relays;
use anyhow::Result;

impl RpcHandler {
    /// List all configured relays with their connection status.
    pub(super) async fn handle_nostr_list_relays(&self) -> Result<serde_json::Value> {
        let relays = nostr_relays::list_relays(&self.config.data_dir).await?;
        let items: Vec<serde_json::Value> = relays
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "url": r.url,
                    "connected": r.connected,
                    "enabled": r.enabled,
                    "added_at": r.added_at,
                })
            })
            .collect();
        Ok(serde_json::json!({ "relays": items }))
    }

    /// Add a new relay.
    pub(super) async fn handle_nostr_add_relay(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing url"))?;

        let relay = nostr_relays::add_relay(&self.config.data_dir, url).await?;
        Ok(serde_json::json!({
            "url": relay.url,
            "enabled": relay.enabled,
        }))
    }

    /// Remove a relay.
    pub(super) async fn handle_nostr_remove_relay(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing url"))?;

        nostr_relays::remove_relay(&self.config.data_dir, url).await?;
        Ok(serde_json::json!({ "ok": true }))
    }

    /// Toggle a relay on/off.
    pub(super) async fn handle_nostr_toggle_relay(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing url"))?;
        let enabled = params
            .get("enabled")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("Missing enabled"))?;

        nostr_relays::toggle_relay(&self.config.data_dir, url, enabled).await?;
        Ok(serde_json::json!({ "ok": true }))
    }

    /// Get relay stats.
    pub(super) async fn handle_nostr_get_stats(&self) -> Result<serde_json::Value> {
        let stats = nostr_relays::get_stats(&self.config.data_dir).await?;
        Ok(serde_json::json!({
            "total_relays": stats.total_relays,
            "connected_count": stats.connected_count,
            "enabled_count": stats.enabled_count,
        }))
    }
}
