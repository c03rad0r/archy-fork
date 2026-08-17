use super::RpcHandler;
use crate::wallet::{ark_client, ecash, fedimint_client, profits};
use anyhow::Result;

/// A Cashu token (NUT-00 `cashuA`/`cashuB`, or our legacy `cashuSend_` form)
/// always starts with `cashu`. Fedimint ecash notes never do, so a non-`cashu`
/// string is routed to the Fedimint reissue path.
fn is_cashu_token(token: &str) -> bool {
    token.trim_start().starts_with("cashu")
}

impl RpcHandler {
    pub(super) async fn handle_wallet_ecash_balance(&self) -> Result<serde_json::Value> {
        let wallet = ecash::load_wallet(&self.config.data_dir).await?;
        let cashu_sats = wallet.balance();
        // Spendable Fedimint balance too, so callers (e.g. the pay-for-file
        // pre-check) see funds available across BOTH backends (#3). Best-effort:
        // if fmcd isn't installed/joined this is just 0, never an error.
        let fedimint_sats =
            match fedimint_client::FedimintClient::from_node(&self.config.data_dir).await {
                Ok(client) => client.total_balance_sats().await.unwrap_or(0),
                Err(_) => 0,
            };
        // Spendable Ark (barkd) balance, same best-effort contract.
        let ark_sats = ark_client::spendable_sats_or_zero(&self.config.data_dir).await;
        Ok(serde_json::json!({
            // `balance_sats` stays Cashu-only for back-compat; `total_sats` is the
            // spendable amount across Cashu + Fedimint + Ark.
            "balance_sats": cashu_sats,
            "cashu_sats": cashu_sats,
            "fedimint_sats": fedimint_sats,
            "ark_sats": ark_sats,
            "total_sats": cashu_sats + fedimint_sats + ark_sats,
            "proof_count": wallet.proofs.iter().filter(|p| !p.spent && !p.reserved).count(),
            "mint_url": wallet.mint_url,
        }))
    }

    /// `wallet.ecash-network` — which ecash network this node is on, and the
    /// balance sitting in the *other* one so the UI can say what switching
    /// would reveal rather than appearing to lose money.
    pub(super) async fn handle_wallet_ecash_network(&self) -> Result<serde_json::Value> {
        let current = ecash::load_network(&self.config.data_dir).await;
        let wallet = ecash::load_wallet(&self.config.data_dir).await?;
        Ok(serde_json::json!({
            "network": current,
            "is_test": current.is_test(),
            "mint_url": wallet.mint_url,
            "balance_sats": wallet.balance(),
        }))
    }

    /// `wallet.ecash-set-network` — switch between real and test ecash.
    ///
    /// Each network keeps its own wallet file, so this never moves, merges or
    /// deletes coins: switching away parks the current balance and switching
    /// back finds it exactly as it was.
    pub(super) async fn handle_wallet_ecash_set_network(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let requested = params
            .get("network")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing network ('mainnet' or 'testnet')"))?;
        let network = match requested {
            "mainnet" => ecash::EcashNetwork::Mainnet,
            "testnet" => ecash::EcashNetwork::Testnet,
            other => anyhow::bail!("Unknown ecash network '{other}' — use 'mainnet' or 'testnet'"),
        };

        ecash::save_network(&self.config.data_dir, network).await?;
        let wallet = ecash::load_wallet(&self.config.data_dir).await?;
        tracing::info!(?network, "ecash network switched by operator");
        Ok(serde_json::json!({
            "network": network,
            "is_test": network.is_test(),
            "mint_url": wallet.mint_url,
            "balance_sats": wallet.balance(),
        }))
    }

    pub(super) async fn handle_wallet_ecash_mint(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let amount_sats = params
            .get("amount_sats")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing amount_sats"))?;

        if amount_sats == 0 || amount_sats > 1_000_000 {
            return Err(anyhow::anyhow!(
                "Amount must be between 1 and 1,000,000 sats"
            ));
        }

        // Step 1: Get a mint quote (returns Lightning invoice)
        let quote = ecash::mint_quote(&self.config.data_dir, amount_sats).await?;

        Ok(serde_json::json!({
            "quote_id": quote.quote,
            "bolt11": quote.request,
            "state": quote.state,
            "amount_sats": amount_sats,
            "message": "Pay the Lightning invoice, then call wallet.ecash-mint-claim with the quote_id",
        }))
    }

    /// Claim minted tokens after paying the Lightning invoice.
    pub(super) async fn handle_wallet_ecash_mint_claim(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let quote_id = params
            .get("quote_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing quote_id"))?;
        let amount_sats = params
            .get("amount_sats")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing amount_sats"))?;

        let minted = ecash::mint_tokens(&self.config.data_dir, quote_id, amount_sats).await?;
        Ok(serde_json::json!({
            "minted_sats": minted,
        }))
    }

    pub(super) async fn handle_wallet_ecash_melt(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let bolt11 = params
            .get("bolt11")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing bolt11 (Lightning invoice)"))?;

        // Step 1: Get melt quote
        let quote = ecash::melt_quote(&self.config.data_dir, bolt11).await?;

        Ok(serde_json::json!({
            "quote_id": quote.quote,
            "amount_sats": quote.amount,
            "fee_reserve_sats": quote.fee_reserve,
            "total_needed_sats": quote.amount + quote.fee_reserve,
            "message": "Call wallet.ecash-melt-confirm with quote_id and bolt11 to execute",
        }))
    }

    /// Confirm and execute a melt (pay Lightning invoice with ecash).
    pub(super) async fn handle_wallet_ecash_melt_confirm(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let quote_id = params
            .get("quote_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing quote_id"))?;
        let bolt11 = params
            .get("bolt11")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing bolt11"))?;

        let melted = ecash::melt_tokens(&self.config.data_dir, quote_id, bolt11).await?;
        Ok(serde_json::json!({
            "melted_sats": melted,
        }))
    }

    pub(super) async fn handle_wallet_ecash_send(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let amount_sats = params
            .get("amount_sats")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing amount_sats"))?;

        let token_str = ecash::send_token(&self.config.data_dir, amount_sats).await?;
        Ok(serde_json::json!({
            "token": token_str,
            "amount_sats": amount_sats,
        }))
    }

    pub(super) async fn handle_wallet_ecash_receive(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let token = params
            .get("token")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing token"))?;

        // Dual-ecash: one "Receive ecash" box accepts either a Cashu token
        // (redeemed at the mint) or Fedimint notes (reissued via the fmcd
        // sidecar). Detect by prefix and route accordingly.
        if is_cashu_token(token) {
            let amount = ecash::receive_token(&self.config.data_dir, token).await?;
            return Ok(serde_json::json!({
                "received_sats": amount,
                "kind": "cashu",
            }));
        }

        let (amount, federation_id) =
            fedimint_client::reissue_into_any(&self.config.data_dir, token).await?;
        Ok(serde_json::json!({
            "received_sats": amount,
            "kind": "fedimint",
            "federation_id": federation_id,
        }))
    }

    pub(super) async fn handle_wallet_ecash_history(&self) -> Result<serde_json::Value> {
        // Unified history: Cashu transactions (tagged kind="cashu") + the local
        // Fedimint transaction log (kind="fedimint"), newest first. Previously
        // only Cashu was returned, so a Fedimint receive showed up nowhere.
        let wallet = ecash::load_wallet(&self.config.data_dir).await?;
        let mut transactions = wallet.transactions;
        transactions.extend(fedimint_client::load_fedimint_txs(&self.config.data_dir).await);
        // Ark movements from barkd (kind="ark"), best-effort like Fedimint.
        transactions.extend(ark_client::load_ark_txs(&self.config.data_dir).await);
        // Sort by RFC-3339 timestamp descending (string compare is valid for
        // same-offset RFC-3339), newest first.
        transactions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(serde_json::json!({
            "transactions": transactions,
        }))
    }

    pub(super) async fn handle_wallet_networking_profits(&self) -> Result<serde_json::Value> {
        let summary = profits::get_networking_profits(&self.config.data_dir).await?;
        Ok(serde_json::json!({
            "total_sats": summary.total_sats,
            "content_sales_sats": summary.content_sales_sats,
            "routing_fees_sats": summary.routing_fees_sats,
            "streaming_revenue_sats": summary.streaming_revenue_sats,
            "recent": summary.recent,
        }))
    }
}
