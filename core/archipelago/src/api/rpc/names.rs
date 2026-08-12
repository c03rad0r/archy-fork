use super::RpcHandler;
use crate::names;
use anyhow::Result;

impl RpcHandler {
    /// List all registered names.
    pub(super) async fn handle_identity_list_names(
        &self,
        _params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let store = names::load_names(&self.config.data_dir).await?;
        let items: Vec<serde_json::Value> = store
            .names
            .into_iter()
            .map(|n| {
                serde_json::json!({
                    "id": n.id,
                    "name": n.name,
                    "domain": n.domain,
                    "nip05": n.nip05,
                    "identity_id": n.identity_id,
                    "did": n.did,
                    "nostr_pubkey": n.nostr_pubkey,
                    "status": n.status,
                    "registered_at": n.registered_at,
                    "expires_at": n.expires_at,
                })
            })
            .collect();
        Ok(serde_json::json!({ "names": items }))
    }

    /// Register a new name linked to an identity.
    pub(super) async fn handle_identity_register_name(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing name"))?;
        let domain = params
            .get("domain")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing domain"))?;
        let identity_id = params
            .get("identity_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing identity_id"))?;
        let did = params
            .get("did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing did"))?;
        let nostr_pubkey = params.get("nostr_pubkey").and_then(|v| v.as_str());

        let record = names::register_name(
            &self.config.data_dir,
            name,
            domain,
            identity_id,
            did,
            nostr_pubkey,
        )
        .await?;

        Ok(serde_json::json!({
            "id": record.id,
            "nip05": record.nip05,
            "status": record.status,
        }))
    }

    /// Remove a registered name.
    pub(super) async fn handle_identity_remove_name(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?;

        names::remove_name(&self.config.data_dir, id).await?;
        Ok(serde_json::json!({ "ok": true }))
    }

    /// Resolve a NIP-05 identifier to verify it.
    pub(super) async fn handle_identity_resolve_name(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let identifier = params
            .get("identifier")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing identifier (user@domain)"))?;

        let result = names::resolve_nip05(identifier).await?;
        Ok(serde_json::json!({
            "name": result.name,
            "domain": result.domain,
            "nostr_pubkey": result.nostr_pubkey,
            "relays": result.relays,
            "verified": result.verified,
        }))
    }

    /// Link a name to a different DID/identity.
    pub(super) async fn handle_identity_link_name(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let name_id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?;
        let did = params
            .get("did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing did"))?;
        let identity_id = params
            .get("identity_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing identity_id"))?;

        let updated =
            names::link_name_to_did(&self.config.data_dir, name_id, did, identity_id).await?;
        Ok(serde_json::json!({
            "id": updated.id,
            "nip05": updated.nip05,
            "did": updated.did,
        }))
    }
}
