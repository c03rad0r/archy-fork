//! RPC handlers for node network visibility and overlay controls.

use super::RpcHandler;
use crate::container::docker_packages;
use crate::{identity, peers};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;

const VISIBILITY_FILE: &str = "network_visibility";
const REQUESTS_DIR: &str = "connection_requests";

/// A pending connection request from another node.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConnectionRequest {
    id: String,
    from_did: String,
    from_onion: String,
    from_pubkey: String,
    message: Option<String>,
    created_at: String,
}

/// Node visibility levels for peer discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeVisibility {
    Hidden,
    Discoverable,
    Public,
}

impl NodeVisibility {
    fn as_str(&self) -> &'static str {
        match self {
            NodeVisibility::Hidden => "hidden",
            NodeVisibility::Discoverable => "discoverable",
            NodeVisibility::Public => "public",
        }
    }

    fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "discoverable" => NodeVisibility::Discoverable,
            "public" => NodeVisibility::Public,
            _ => NodeVisibility::Hidden,
        }
    }
}

impl RpcHandler {
    /// Get the current node visibility setting.
    pub(super) async fn handle_network_get_visibility(&self) -> Result<serde_json::Value> {
        let vis = self.load_visibility().await;
        let tor_address = docker_packages::read_tor_address("archipelago").await;
        Ok(serde_json::json!({
            "visibility": vis.as_str(),
            "tor_address": tor_address,
        }))
    }

    /// Set node visibility. When discoverable/public, publishes to Nostr relays.
    /// When hidden, stops advertising.
    pub(super) async fn handle_network_set_visibility(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let vis_str = params
            .get("visibility")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: visibility"))?;

        let vis = NodeVisibility::from_str(vis_str);

        // Persist the setting
        let vis_path = self.config.data_dir.join(VISIBILITY_FILE);
        fs::write(&vis_path, vis.as_str().as_bytes())
            .await
            .context("Failed to write visibility setting")?;

        // Visibility is stored but we never publish to public relays.
        // Nodes connect via federation ID, not Nostr discovery.
        tracing::info!("Node visibility set to {}", vis.as_str());
        Ok(serde_json::json!({
            "visibility": vis.as_str(),
            "published": false,
            "reason": "Public relay publishing is disabled for security — nodes connect via federation ID",
        }))
    }

    /// Send a connection request to a peer (stores locally as pending).
    pub(super) async fn handle_network_request_connection(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let to_did = params
            .get("did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: did"))?;
        let to_onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: onion"))?;
        let to_pubkey = params
            .get("pubkey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: pubkey"))?;
        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Send a message to the peer over Tor with connection request
        let (data, _) = self.state_manager.get_snapshot().await;
        let my_pubkey = &data.server_info.pubkey;
        let my_did = identity::did_key_from_pubkey_hex(my_pubkey)?;
        let my_onion = docker_packages::read_tor_address("archipelago")
            .await
            .unwrap_or_default();

        let req_msg = serde_json::json!({
            "type": "connection_request",
            "from_did": my_did,
            "from_onion": my_onion,
            "from_pubkey": my_pubkey,
            "message": message,
        });

        let to_fips_npub =
            crate::federation::fips_npub_for_onion(&self.config.data_dir, to_onion).await;
        crate::node_message::send_to_peer(
            to_onion,
            to_fips_npub.as_deref(),
            my_pubkey,
            &req_msg.to_string(),
            None,
            None,
            None,
            Some(&self.config.data_dir),
        )
        .await?;

        // Also add them as a pending peer locally
        let req = ConnectionRequest {
            id: uuid::Uuid::new_v4().to_string(),
            from_did: to_did.to_string(),
            from_onion: to_onion.to_string(),
            from_pubkey: to_pubkey.to_string(),
            message,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.save_request(&req).await?;

        Ok(serde_json::json!({ "ok": true, "request_id": req.id }))
    }

    /// List pending connection requests.
    pub(super) async fn handle_network_list_requests(&self) -> Result<serde_json::Value> {
        let requests = self.load_requests().await?;
        Ok(serde_json::json!({ "requests": requests }))
    }

    /// Accept a connection request — add peer to trusted list AND send
    /// a `connection_accepted` notification back to the requester so
    /// their side auto-adds us without a second manual round-trip.
    pub(super) async fn handle_network_accept_request(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        // The web UI historically sent `request_id`; accept both spellings.
        let request_id = params
            .get("id")
            .or_else(|| params.get("request_id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: id"))?;

        let requests = self.load_requests().await?;
        let req = requests
            .iter()
            .find(|r| r.id == request_id)
            .ok_or_else(|| anyhow::anyhow!("Request not found: {}", request_id))?
            .clone();

        // Add to known peers
        let peer = peers::KnownPeer {
            onion: req.from_onion.clone(),
            pubkey: req.from_pubkey.clone(),
            name: None,
            added_at: Some(chrono::Utc::now().to_rfc3339()),
        };
        peers::add_peer(&self.config.data_dir, peer).await?;

        // Remove the request
        self.delete_request(request_id).await?;

        // Notify the requester we've accepted so their UI auto-adds us and
        // clears its outbound pending row. Best-effort — if the peer is
        // offline we don't fail the accept; the next connection_request
        // retry on their side will resolve eventually.
        let (data, _) = self.state_manager.get_snapshot().await;
        let my_pubkey = data.server_info.pubkey.clone();
        let my_did = crate::identity::did_key_from_pubkey_hex(&my_pubkey).ok();
        let my_onion = crate::container::docker_packages::read_tor_address("archipelago")
            .await
            .unwrap_or_default();
        let accept_msg = serde_json::json!({
            "type": "connection_accepted",
            "request_id": request_id,
            "from_did": my_did,
            "from_onion": my_onion,
            "from_pubkey": my_pubkey,
        });
        let to_fips_npub =
            crate::federation::fips_npub_for_onion(&self.config.data_dir, &req.from_onion).await;
        let identity_dir = self.config.data_dir.join("identity");
        let signing_key = crate::identity::NodeIdentity::load_or_create(&identity_dir)
            .await
            .ok();
        if let Err(e) = crate::node_message::send_to_peer(
            &req.from_onion,
            to_fips_npub.as_deref(),
            &my_pubkey,
            &accept_msg.to_string(),
            signing_key.as_ref().map(|i| i.signing_key()),
            Some(&req.from_pubkey),
            data.server_info.name.as_deref(),
            Some(&self.config.data_dir),
        )
        .await
        {
            tracing::warn!(
                to = %req.from_did,
                error = %e,
                "connection_accepted notify failed (requester will still be able to see us on their next retry)"
            );
        }

        tracing::info!("Accepted connection from {}", req.from_did);
        Ok(serde_json::json!({ "ok": true }))
    }

    /// Reject a connection request.
    pub(super) async fn handle_network_reject_request(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        // The web UI historically sent `request_id`; accept both spellings.
        let request_id = params
            .get("id")
            .or_else(|| params.get("request_id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: id"))?;

        self.delete_request(request_id).await?;
        Ok(serde_json::json!({ "ok": true }))
    }

    // --- internal helpers ---

    /// Load current visibility setting from disk (defaults to hidden).
    async fn load_visibility(&self) -> NodeVisibility {
        let vis_path = self.config.data_dir.join(VISIBILITY_FILE);
        match fs::read_to_string(&vis_path).await {
            Ok(s) => NodeVisibility::from_str(&s),
            Err(_) => NodeVisibility::Hidden,
        }
    }

    async fn requests_dir(&self) -> Result<std::path::PathBuf> {
        let dir = self.config.data_dir.join(REQUESTS_DIR);
        fs::create_dir_all(&dir)
            .await
            .context("Failed to create requests dir")?;
        Ok(dir)
    }

    async fn save_request(&self, req: &ConnectionRequest) -> Result<()> {
        let dir = self.requests_dir().await?;
        let path = dir.join(format!("{}.json", req.id));
        let json = serde_json::to_string_pretty(req).context("Failed to serialize request")?;
        fs::write(&path, json)
            .await
            .context("Failed to write request")?;
        Ok(())
    }

    async fn load_requests(&self) -> Result<Vec<ConnectionRequest>> {
        let dir = self.requests_dir().await?;
        let mut requests = Vec::new();
        let mut entries = fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(data) = fs::read(&path).await {
                if let Ok(req) = serde_json::from_slice::<ConnectionRequest>(&data) {
                    requests.push(req);
                }
            }
        }
        requests.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(requests)
    }

    async fn delete_request(&self, id: &str) -> Result<()> {
        // Validate ID to prevent path traversal
        if id.is_empty()
            || id.len() > 128
            || id.contains('/')
            || id.contains('\\')
            || id.contains("..")
            || id.contains('\0')
        {
            anyhow::bail!("Invalid request ID");
        }
        let dir = self.requests_dir().await?;
        let path = dir.join(format!("{}.json", id));
        if path.exists() {
            fs::remove_file(&path)
                .await
                .context("Failed to delete request")?;
        }
        Ok(())
    }
}
