//! Ark protocol RPCs — bridge to the `barkd` sidecar.
//!
//! Companion to the Cashu RPCs in [`super::wallet`] and the Fedimint RPCs in
//! [`super::fedimint`]. Holding VTXOs, joining rounds and unilateral exits are
//! delegated to the barkd container via [`crate::wallet::ark_client::ArkClient`];
//! here we expose the node's JSON-RPC surface. barkd keeps its own movement
//! history, so unlike Fedimint there is no local transaction log.

use super::RpcHandler;
use crate::wallet::ark_client::{self, ArkClient};
use anyhow::Result;

impl RpcHandler {
    /// `wallet.ark-status` — sidecar reachability, wallet fingerprint, network
    /// and Ark server parameters. Soft-fails into `available: false` so the
    /// settings UI can render an install/enable hint instead of an error.
    pub(super) async fn handle_wallet_ark_status(&self) -> Result<serde_json::Value> {
        let config = ark_client::load_config(&self.config.data_dir).await;
        let client = match ArkClient::from_node(&self.config.data_dir).await {
            Ok(c) => c,
            Err(_) => {
                return Ok(serde_json::json!({
                    "available": false,
                    "wallet_ready": false,
                    "config": config,
                }))
            }
        };
        // Make sure the wallet exists before reporting (idempotent, cheap once
        // created).
        let _ = ark_client::ensure_wallet(&self.config.data_dir).await;

        let wallet = client.wallet_info().await.ok();
        let info = client.ark_info().await.ok();
        Ok(serde_json::json!({
            "available": true,
            "wallet_ready": wallet.is_some(),
            "wallet": wallet,
            "ark_info": info,
            "config": config,
        }))
    }

    /// `wallet.ark-balance` — off-chain (spendable + pending) and on-chain
    /// sats. Soft-fails to zeros so unified balances still render.
    pub(super) async fn handle_wallet_ark_balance(&self) -> Result<serde_json::Value> {
        let client = match ArkClient::from_node(&self.config.data_dir).await {
            Ok(c) => c,
            Err(_) => {
                return Ok(serde_json::json!({
                    "balance_sats": 0,
                    "spendable_sats": 0,
                    "pending_sats": 0,
                    "onchain_sats": 0,
                }))
            }
        };
        let bal = client
            .balance()
            .await
            .unwrap_or_else(|_| serde_json::json!({}));
        let sat = |key: &str| bal.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
        let spendable = sat("spendable_sat");
        let pending = sat("pending_in_round_sat")
            + sat("pending_board_sat")
            + sat("pending_lightning_send_sat")
            + sat("claimable_lightning_receive_sat")
            + bal
                .get("pending_exit_sat")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
        let onchain = client
            .onchain_balance()
            .await
            .ok()
            .and_then(|b| {
                b.get("total_sat")
                    .or_else(|| b.get("confirmed_sat"))
                    .and_then(|v| v.as_u64())
            })
            .unwrap_or(0);
        Ok(serde_json::json!({
            "balance_sats": spendable,
            "spendable_sats": spendable,
            "pending_sats": pending,
            "onchain_sats": onchain,
        }))
    }

    /// `wallet.ark-address` — fresh Ark (`tark1…`) receive address; pass
    /// `{"onchain": true}` for an on-chain boarding address instead.
    pub(super) async fn handle_wallet_ark_address(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let _ = ark_client::ensure_wallet(&self.config.data_dir).await;
        let client = ArkClient::from_node(&self.config.data_dir).await?;
        let onchain = params
            .as_ref()
            .and_then(|p| p.get("onchain"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let address = if onchain {
            client.onchain_address().await?
        } else {
            client.ark_address().await?
        };
        Ok(serde_json::json!({ "address": address, "onchain": onchain }))
    }

    /// `wallet.ark-send` — pay an Ark address, BOLT11 invoice, LNURL or
    /// lightning address from Ark funds.
    pub(super) async fn handle_wallet_ark_send(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let destination = params
            .get("destination")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing destination"))?;
        // Optional for BOLT11 invoices that carry their own amount.
        let amount_sats = params.get("amount_sats").and_then(|v| v.as_u64());
        if amount_sats == Some(0) {
            return Err(anyhow::anyhow!("Amount must be greater than zero"));
        }
        let comment = params.get("comment").and_then(|v| v.as_str());

        let client = ArkClient::from_node(&self.config.data_dir).await?;
        let movement = client.send(destination, amount_sats, comment).await?;
        Ok(serde_json::json!({
            "sent": true,
            "movement": movement,
        }))
    }

    /// `wallet.ark-invoice` — BOLT11 invoice that lands as Ark funds when paid.
    pub(super) async fn handle_wallet_ark_invoice(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let amount_sats = params
            .get("amount_sats")
            .and_then(|v| v.as_u64())
            .filter(|&v| v > 0)
            .ok_or_else(|| anyhow::anyhow!("Missing amount_sats"))?;

        let _ = ark_client::ensure_wallet(&self.config.data_dir).await;
        let client = ArkClient::from_node(&self.config.data_dir).await?;
        let res = client.lightning_invoice(amount_sats).await?;
        Ok(res)
    }

    /// `wallet.ark-board` — lift on-chain funds into Ark VTXOs. Omitting
    /// `amount_sats` boards everything.
    pub(super) async fn handle_wallet_ark_board(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let amount_sats = params
            .as_ref()
            .and_then(|p| p.get("amount_sats"))
            .and_then(|v| v.as_u64());
        if amount_sats == Some(0) {
            return Err(anyhow::anyhow!("Amount must be greater than zero"));
        }
        let client = ArkClient::from_node(&self.config.data_dir).await?;
        let res = client.board(amount_sats).await?;
        Ok(res)
    }

    /// `wallet.ark-offboard` — collaboratively move all VTXOs back on-chain,
    /// optionally to a provided address (defaults to the wallet's own).
    pub(super) async fn handle_wallet_ark_offboard(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let address = params
            .as_ref()
            .and_then(|p| p.get("address"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let client = ArkClient::from_node(&self.config.data_dir).await?;
        let res = client.offboard_all(address).await?;
        Ok(res)
    }

    /// `wallet.ark-history` — barkd movements mapped to the unified
    /// transaction shape (kind = "ark"), newest first.
    pub(super) async fn handle_wallet_ark_history(&self) -> Result<serde_json::Value> {
        let mut transactions = ark_client::load_ark_txs(&self.config.data_dir).await;
        transactions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(serde_json::json!({ "transactions": transactions }))
    }

    /// `wallet.ark-configure` — set the Ark server / esplora / network used
    /// when the barkd wallet is (re)created. Does NOT migrate an existing
    /// wallet: barkd binds a wallet to its Ark server at creation.
    pub(super) async fn handle_wallet_ark_configure(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let mut config = ark_client::load_config(&self.config.data_dir).await;
        for (key, field) in [
            ("network", &mut config.network as &mut String),
            ("ark_server", &mut config.ark_server),
            ("esplora", &mut config.esplora),
        ] {
            if let Some(v) = params.get(key).and_then(|v| v.as_str()) {
                let v = v.trim();
                if !v.is_empty() {
                    *field = v.to_string();
                }
            }
        }
        if !matches!(config.network.as_str(), "signet" | "mainnet" | "regtest") {
            return Err(anyhow::anyhow!(
                "network must be one of: signet, mainnet, regtest"
            ));
        }
        ark_client::save_config(&self.config.data_dir, &config).await?;
        Ok(serde_json::json!({ "config": config }))
    }
}
