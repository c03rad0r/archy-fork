//! Fedimint ecash RPCs — bridge to the `fedimint-clientd` sidecar.
//!
//! Companion to the Cashu wallet RPCs in [`super::wallet`]. Joining/holding
//! Fedimint ecash is delegated to the clientd container via
//! [`crate::wallet::fedimint_client::FedimintClient`]; here we expose the
//! node's JSON-RPC surface and keep a local registry of joined federations so
//! the list survives clientd being temporarily unreachable.
//!
//! See `docs/dual-ecash-design.md`.

use super::RpcHandler;
use crate::wallet::fedimint_client::{self, FedimintClient, JoinedFederation};
use anyhow::Result;

impl RpcHandler {
    /// `wallet.fedimint-list` — joined federations with live balances.
    pub(super) async fn handle_wallet_fedimint_list(&self) -> Result<serde_json::Value> {
        // Best-effort: make sure the default federation is joined/tracked.
        let _ = fedimint_client::ensure_default_federation(&self.config.data_dir).await;

        let reg = fedimint_client::load_registry(&self.config.data_dir).await?;

        // Live balances are best-effort: if clientd is down we still return the
        // tracked federations (with 0 balance) rather than failing the call.
        let info = match FedimintClient::from_node(&self.config.data_dir).await {
            Ok(client) => client.info().await.ok(),
            Err(_) => None,
        };

        let federations: Vec<serde_json::Value> = reg
            .federations
            .iter()
            .map(|f| {
                let balance_sats = info
                    .as_ref()
                    .and_then(|i| i.get(&f.federation_id))
                    .and_then(|e| {
                        e.get("totalAmountMsat")
                            .or_else(|| e.get("totalMsat"))
                            .and_then(|v| v.as_u64())
                    })
                    .map(|msat| msat / 1000)
                    .unwrap_or(0);
                serde_json::json!({
                    "federation_id": f.federation_id,
                    "name": f.name,
                    "balance_sats": balance_sats,
                })
            })
            .collect();

        Ok(serde_json::json!({ "federations": federations }))
    }

    /// `wallet.fedimint-join` — join a federation by invite code.
    pub(super) async fn handle_wallet_fedimint_join(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let invite_code = params
            .get("invite_code")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing invite_code"))?;

        let client = FedimintClient::from_node(&self.config.data_dir).await?;
        let federation_id = client.join(invite_code).await?;

        // Try to label it from the federation meta (best-effort).
        let name = client.info().await.ok().and_then(|i| {
            i.get(&federation_id)
                .and_then(|e| e.get("meta"))
                .and_then(|m| {
                    m.get("federation_name")
                        .or_else(|| m.get("federation_expiry_timestamp"))
                })
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

        let mut reg = fedimint_client::load_registry(&self.config.data_dir).await?;
        if !reg
            .federations
            .iter()
            .any(|f| f.federation_id == federation_id)
        {
            reg.federations.push(JoinedFederation {
                federation_id: federation_id.clone(),
                name,
            });
            fedimint_client::save_registry(&self.config.data_dir, &reg).await?;
        }

        Ok(serde_json::json!({ "federation_id": federation_id }))
    }

    /// `wallet.fedimint-leave` — stop tracking a federation locally.
    pub(super) async fn handle_wallet_fedimint_leave(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let federation_id = params
            .get("federation_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing federation_id"))?;

        let mut reg = fedimint_client::load_registry(&self.config.data_dir).await?;
        let before = reg.federations.len();
        reg.federations.retain(|f| f.federation_id != federation_id);
        let removed = reg.federations.len() != before;
        if removed {
            fedimint_client::save_registry(&self.config.data_dir, &reg).await?;
        }

        Ok(serde_json::json!({ "removed": removed }))
    }

    /// `wallet.fedimint-send` — spend ecash notes from any joined federation
    /// with sufficient balance. Returns the notes token for the recipient
    /// (rendered as text + QR by the send modal — the wallet's Fedi rail,
    /// split from Cashu 2026-07-22). The heavy lifting already existed in
    /// `fedimint_client::spend_from_any`; it was simply never exposed.
    pub(super) async fn handle_wallet_fedimint_send(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let amount_sats = params
            .as_ref()
            .and_then(|p| p.get("amount_sats"))
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing amount_sats"))?;
        anyhow::ensure!(amount_sats > 0, "must be at least 1 sat");
        let (token, federation_id) =
            crate::wallet::fedimint_client::spend_from_any(&self.config.data_dir, amount_sats)
                .await?;
        Ok(serde_json::json!({
            "token": token,
            "federation_id": federation_id,
            "amount_sats": amount_sats,
        }))
    }

    /// `wallet.fedimint-balance` — total sats across all joined federations.
    pub(super) async fn handle_wallet_fedimint_balance(&self) -> Result<serde_json::Value> {
        // Soft-fail to zero when clientd isn't installed/running, so the unified
        // wallet balance still renders from the Cashu side.
        let balance_sats = match FedimintClient::from_node(&self.config.data_dir).await {
            Ok(client) => client.total_balance_sats().await.unwrap_or(0),
            Err(_) => 0,
        };
        Ok(serde_json::json!({ "balance_sats": balance_sats }))
    }
}
