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
            // Which mint issued it, for the success screen. Ecash leaves no
            // public trace once redeemed, so the mint URL is the only thing a
            // person can quote later if the payment is ever disputed.
            let mint_url = crate::wallet::cashu::CashuToken::deserialize(token)
                .ok()
                .and_then(|t| t.mint_urls().first().map(|m| m.to_string()));
            let amount = ecash::receive_token(&self.config.data_dir, token).await?;
            return Ok(serde_json::json!({
                "received_sats": amount,
                "kind": "cashu",
                "mint_url": mint_url,
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

    /// `wallet.ecash-seed-status` — whether this wallet has a NUT-13 phrase
    /// yet, and therefore whether its coins can be restored at all.
    ///
    /// Deliberately says nothing secret. `active: false` is the honest answer
    /// for a node that predates NUT-13: its existing proofs live in exactly one
    /// file and nothing can bring them back, which the UI needs to be able to
    /// say plainly rather than implying a backup exists.
    pub(super) async fn handle_wallet_ecash_seed_status(&self) -> Result<serde_json::Value> {
        let data_dir = &self.config.data_dir;
        let active = crate::wallet::nut13::seed_exists(data_dir);
        let source = match crate::wallet::nut13::load_seed(data_dir).await {
            Ok(Some(seed)) => Some(seed.source()),
            _ => None,
        };
        // A phrase can always be established. Whether the node has an
        // encrypted master seed decides only *which kind*: derived from it
        // (the node's 24 words already cover the ecash), or independent (the
        // phrase is the only copy). The UI needs both facts to set the right
        // expectation before the operator commits to writing something down.
        Ok(serde_json::json!({
            "active": active,
            "source": source,
            "can_activate": true,
            "derivable_from_node_seed": crate::seed::seed_exists(data_dir),
        }))
    }

    /// `wallet.ecash-seed-reveal` — show the ecash wallet's 24 words, and
    /// establish them from the node's master seed if this is the first time.
    ///
    /// Gated exactly like `seed.reveal` and `lnd.seed-reveal`: authenticated
    /// session, password re-verification, TOTP when enabled. The words are
    /// returned to the caller only and never logged.
    ///
    /// Reveal doubles as activation because the master seed is encrypted at
    /// rest: this password prompt is the only moment the node can legitimately
    /// open it, so it is also the only moment the ecash phrase can be derived
    /// from it. A node that has never been here mints valid but unrecoverable
    /// proofs; one visit fixes that for every proof minted afterwards.
    pub(super) async fn handle_wallet_ecash_seed_reveal(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        use zeroize::Zeroize;

        let params = params.unwrap_or_default();
        let data_dir = &self.config.data_dir;

        let mut password = self.verify_reveal_auth(&params, "the ecash seed").await?;

        // Already established: just open it. No master seed needed, so this
        // still works on a node whose backup passphrase has been forgotten.
        if let Some(seed) = crate::wallet::nut13::load_seed(data_dir).await? {
            password.zeroize();
            let words = seed.words();
            return Ok(serde_json::json!({
                "words": words,
                "word_count": words.len(),
                "source": seed.source(),
                "newly_activated": false,
            }));
        }

        // No encrypted master seed to derive from — common on nodes onboarded
        // before that step existed. The choice here is not "derived or
        // independent", it is "independent or no backup at all", so we make
        // one and label it honestly. Every surface that shows an
        // `independent` phrase says the node's own recovery phrase does not
        // cover it.
        if !crate::seed::seed_exists(data_dir) {
            password.zeroize();
            let seed = crate::wallet::nut13::establish_independent(data_dir).await?;
            let words = seed.words();
            return Ok(serde_json::json!({
                "words": words,
                "word_count": words.len(),
                "source": seed.source(),
                "newly_activated": true,
            }));
        }

        // The backup passphrase may differ from the login password — same
        // fallback `seed.reveal` uses.
        let passphrase = params
            .get("passphrase")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| password.clone());
        let master = crate::seed::load_seed_encrypted(data_dir, &passphrase).await;
        password.zeroize();
        let mnemonic = master.map_err(|_| {
            anyhow::anyhow!(
                "Could not decrypt the saved seed. If you set a separate backup \
                 passphrase during setup, enter that passphrase."
            )
        })?;
        let master = crate::seed::MasterSeed::from_mnemonic(&mnemonic);
        let seed = crate::wallet::nut13::establish_from_master(data_dir, &master).await?;

        let words = seed.words();
        Ok(serde_json::json!({
            "words": words,
            "word_count": words.len(),
            "source": seed.source(),
            "newly_activated": true,
        }))
    }

    /// `wallet.ecash-restore` — rebuild the wallet's coins from its NUT-13
    /// phrase by asking a mint which re-derived secrets it has signed.
    ///
    /// Defaults to the wallet's own mint; `mint_url` targets another one, for
    /// a wallet whose coins were spread across mints.
    pub(super) async fn handle_wallet_ecash_restore(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let mint_url = match params.get("mint_url").and_then(|v| v.as_str()) {
            Some(url) if !url.trim().is_empty() => url.trim().to_string(),
            _ => {
                crate::wallet::ecash::load_wallet(&self.config.data_dir)
                    .await?
                    .mint_url
            }
        };

        let outcome =
            crate::wallet::ecash::restore_from_seed(&self.config.data_dir, &mint_url).await?;
        Ok(serde_json::json!({
            "mint_url": mint_url,
            "recovered_sats": outcome.recovered_sats,
            "recovered_proofs": outcome.recovered_proofs,
            "already_spent": outcome.already_spent,
            "keysets_scanned": outcome.keysets_scanned,
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
