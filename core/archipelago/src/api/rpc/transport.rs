use super::RpcHandler;
use crate::transport::{MessageType, TransportMessage};
use anyhow::Result;
use tracing::info;

impl RpcHandler {
    /// transport.status — Get available transports and their status.
    pub(super) async fn handle_transport_status(&self) -> Result<serde_json::Value> {
        let router = self.transport_router.read().await;
        if let Some(r) = router.as_ref() {
            let transports: Vec<serde_json::Value> = r
                .transport_status()
                .into_iter()
                .map(|(kind, available)| {
                    serde_json::json!({
                        "kind": kind.to_string(),
                        "available": available,
                    })
                })
                .collect();
            let peer_count = r.registry.count().await;
            let mesh_only = r.is_mesh_only().await;

            Ok(serde_json::json!({
                "transports": transports,
                "mesh_only": mesh_only,
                "peer_count": peer_count,
            }))
        } else {
            Ok(serde_json::json!({
                "transports": [],
                "mesh_only": false,
                "peer_count": 0,
            }))
        }
    }

    /// transport.peers — Get unified peer list with per-peer transport capabilities.
    pub(super) async fn handle_transport_peers(&self) -> Result<serde_json::Value> {
        let router = self.transport_router.read().await;
        if let Some(r) = router.as_ref() {
            let peers = r.registry.all_peers().await;
            let peer_values: Vec<serde_json::Value> = peers
                .into_iter()
                .map(|p| {
                    let available = p.available_transports();
                    let preferred = available.first().map(|t| t.to_string());
                    serde_json::json!({
                        "did": p.did,
                        "pubkey_hex": p.pubkey_hex,
                        "name": p.name,
                        "trust_level": p.trust_level,
                        "mesh_contact_id": p.mesh_contact_id,
                        "lan_address": p.lan_address,
                        "onion_address": p.onion_address,
                        "preferred_transport": preferred,
                        "available_transports": available.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
                        "last_seen": p.last_mesh.or(p.last_lan).or(p.last_tor),
                    })
                })
                .collect();

            Ok(serde_json::json!({ "peers": peer_values }))
        } else {
            Ok(serde_json::json!({ "peers": [] }))
        }
    }

    /// transport.send — Send a message to a peer via best available transport.
    pub(super) async fn handle_transport_send(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let did = params["did"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'did' param"))?
            .to_string();
        let payload = params["payload"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'payload' param"))?
            .to_string();

        let router = self.transport_router.read().await;
        let router = router
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Transport router not initialized"))?;

        let (data, _) = self.state_manager.get_snapshot().await;
        let our_did =
            crate::identity::did_key_from_pubkey_hex(&data.server_info.pubkey).unwrap_or_default();

        let message = TransportMessage {
            from_did: our_did,
            payload: payload.as_bytes().to_vec(),
            message_type: MessageType::PeerMessage,
        };

        let transport_used = router.send_to_peer(&did, &message).await?;

        info!(did = %did, transport = %transport_used, "Sent message via transport");
        Ok(serde_json::json!({
            "sent": true,
            "transport_used": transport_used.to_string(),
            "did": did,
        }))
    }

    /// transport.preferences — Return the user's per-service transport
    /// preferences. The UI renders these as five FIPS/Auto/Tor rows.
    pub(super) async fn handle_transport_preferences(&self) -> Result<serde_json::Value> {
        let prefs = crate::settings::transport::snapshot().await;
        Ok(serde_json::to_value(prefs)?)
    }

    /// transport.set-preference — Change a single service preference.
    /// Persists to disk and hot-swaps the in-memory handle so future
    /// calls see the new value without restart.
    pub(super) async fn handle_transport_set_preference(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        use crate::settings::transport::{set, PeerService, TransportPref};
        let params = params
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let service: PeerService = serde_json::from_value(
            params
                .get("service")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing 'service' param"))?,
        )
        .map_err(|e| anyhow::anyhow!("Invalid service: {}", e))?;
        let pref: TransportPref = serde_json::from_value(
            params
                .get("pref")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing 'pref' param"))?,
        )
        .map_err(|e| anyhow::anyhow!("Invalid pref: {}", e))?;

        set(&self.config.data_dir, service, pref).await?;
        info!(service = ?service, pref = ?pref, "Transport preference updated");

        let current = crate::settings::transport::snapshot().await;
        Ok(serde_json::to_value(current)?)
    }

    /// transport.set-mode — Toggle mesh-only (off-grid) mode.
    pub(super) async fn handle_transport_set_mode(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let mesh_only = params["mesh_only"]
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("Missing 'mesh_only' bool param"))?;

        let router = self.transport_router.read().await;
        let router = router
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Transport router not initialized"))?;

        router.set_mesh_only(mesh_only).await;

        // Also persist to mesh config
        let mut mesh_config = crate::mesh::load_config(&self.config.data_dir)
            .await
            .unwrap_or_default();
        mesh_config.mesh_only_mode = Some(mesh_only);
        crate::mesh::save_config(&self.config.data_dir, &mesh_config).await?;

        info!(mesh_only = mesh_only, "Transport mode updated");
        Ok(serde_json::json!({
            "mesh_only": mesh_only,
            "configured": true,
        }))
    }
}
