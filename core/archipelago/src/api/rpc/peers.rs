use super::RpcHandler;
use crate::peers::KnownPeer;
use crate::{federation, node_message, nostr_discovery, peers};
use anyhow::Result;

impl RpcHandler {
    pub(super) async fn handle_node_add_peer(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing onion"))?;
        let pubkey = params
            .get("pubkey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing pubkey"))?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .map(String::from);

        let peer = KnownPeer {
            onion: onion.to_string(),
            pubkey: pubkey.to_string(),
            name,
            added_at: Some(chrono::Utc::now().to_rfc3339()),
        };
        let peers = peers::add_peer(&self.config.data_dir, peer).await?;
        Ok(serde_json::json!({ "peers": peers }))
    }

    pub(super) async fn handle_node_list_peers(&self) -> Result<serde_json::Value> {
        let peers = peers::load_peers(&self.config.data_dir).await?;
        Ok(serde_json::json!({ "peers": peers }))
    }

    pub(super) async fn handle_node_remove_peer(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let pubkey = params
            .get("pubkey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing pubkey"))?;
        let peers = peers::remove_peer(&self.config.data_dir, pubkey).await?;
        Ok(serde_json::json!({ "peers": peers }))
    }

    pub(super) async fn handle_node_send_message(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing onion"))?;
        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing message"))?;

        // Limit message size to 1MB to prevent DoS
        if message.len() > 1_048_576 {
            anyhow::bail!("Message too large (max 1MB)");
        }

        // Validate onion is a known peer or federated node to prevent SSRF
        let known_peers = peers::load_peers(&self.config.data_dir).await?;
        let is_known_peer = known_peers.iter().any(|p| {
            p.onion == onion
                || p.onion == format!("{}.onion", onion)
                || format!("{}.onion", p.onion) == onion
        });
        let is_known_fed = if !is_known_peer {
            let fed_nodes = federation::load_nodes(&self.config.data_dir)
                .await
                .unwrap_or_default();
            fed_nodes.iter().any(|n| {
                n.onion == onion
                    || n.onion == format!("{}.onion", onion)
                    || format!("{}.onion", n.onion) == onion
            })
        } else {
            false
        };
        if !is_known_peer && !is_known_fed {
            return Err(anyhow::anyhow!(
                "Onion address not in known peers or federation. Add the peer first."
            ));
        }

        let (data, _) = self.state_manager.get_snapshot().await;
        let pubkey = data.server_info.pubkey.clone();

        // Skip sending to ourselves (prevents duplicate messages in group chat)
        if let Some(ref our_onion) = data.server_info.tor_address {
            let our = our_onion.trim_end_matches(".onion");
            let their = onion.trim_end_matches(".onion");
            if our == their {
                return Ok(serde_json::json!({ "ok": true, "sent_to": onion, "skipped": "self" }));
            }
        }

        // Load signing key for E2E encryption
        let identity_dir = self.config.data_dir.join("identity");
        let node_id = crate::identity::NodeIdentity::load_or_create(&identity_dir).await?;

        // Look up recipient's pubkey from federation nodes
        let fed_nodes = federation::load_nodes(&self.config.data_dir)
            .await
            .unwrap_or_default();
        let recipient = fed_nodes.iter().find(|n| {
            n.onion == onion
                || n.onion == format!("{}.onion", onion)
                || format!("{}.onion", n.onion) == onion
        });
        let recipient_pubkey = recipient.map(|n| n.pubkey.clone());
        let recipient_fips_npub = recipient.and_then(|n| n.fips_npub.clone());

        // Include our node name so the recipient can display it
        let node_name = data.server_info.name.clone();

        node_message::send_to_peer(
            onion,
            recipient_fips_npub.as_deref(),
            &pubkey,
            message,
            Some(node_id.signing_key()),
            recipient_pubkey.as_deref(),
            node_name.as_deref(),
            Some(&self.config.data_dir),
        )
        .await?;
        Ok(serde_json::json!({ "ok": true, "sent_to": onion }))
    }

    pub(super) async fn handle_node_check_peer(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing onion"))?;
        let fips_npub = crate::federation::fips_npub_for_onion(&self.config.data_dir, onion).await;
        let reachable = node_message::check_peer_reachable(onion, fips_npub.as_deref())
            .await
            .unwrap_or(false);
        Ok(serde_json::json!({ "onion": onion, "reachable": reachable }))
    }

    pub(super) async fn handle_node_messages_received(&self) -> Result<serde_json::Value> {
        let messages = node_message::get_received();
        Ok(serde_json::json!({ "messages": messages }))
    }

    /// Store a sent message for Archipelago channel history persistence.
    pub(super) async fn handle_node_store_sent(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing message"))?;
        node_message::store_sent(message);
        Ok(serde_json::json!({ "ok": true }))
    }

    pub(super) async fn handle_node_nostr_discover(&self) -> Result<serde_json::Value> {
        let identity_dir = self.config.data_dir.join("identity");
        let nodes = nostr_discovery::discover_archipelago_nodes(
            &identity_dir,
            &self.config.nostr_relays,
            self.config.nostr_tor_proxy.as_deref(),
        )
        .await?;
        Ok(serde_json::json!({ "nodes": nodes }))
    }
}
