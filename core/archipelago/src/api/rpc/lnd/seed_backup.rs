//! Encrypted LND aezeed backup: status, reveal, and acknowledgment.
//!
//! The aezeed is captured once at wallet-init time (see
//! `crate::container::lnd::persist_aezeed_backup`) and stored under
//! `identity/lnd_aezeed.enc`, encrypted with the per-node wallet secret.
//! Reveal is gated like `seed.reveal`: authenticated session + password
//! re-verification + TOTP when enabled.

use crate::api::rpc::RpcHandler;
use anyhow::Result;
use zeroize::Zeroize;

impl RpcHandler {
    /// Whether an encrypted aezeed backup exists and whether the user has
    /// confirmed writing it down. Drives the first-launch backup prompt.
    pub(in crate::api::rpc) async fn handle_lnd_seed_backup_status(
        &self,
    ) -> Result<serde_json::Value> {
        let data_dir = &self.config.data_dir;
        Ok(serde_json::json!({
            "available": crate::seed::lnd_aezeed_exists(data_dir),
            "acknowledged": crate::seed::lnd_aezeed_acknowledged(data_dir),
        }))
    }

    /// Reveal the Lightning wallet's 24 aezeed words. Same gating as
    /// `seed.reveal`; the words are returned to the caller only, never logged.
    pub(in crate::api::rpc) async fn handle_lnd_seed_reveal(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();

        if !crate::seed::lnd_aezeed_exists(&self.config.data_dir) {
            anyhow::bail!(
                "No Lightning seed backup exists on this node. It is captured \
                 automatically when the Lightning wallet is first created."
            );
        }

        let mut password = self
            .verify_reveal_auth(&params, "the Lightning seed")
            .await?;
        password.zeroize();

        // The backup is encrypted with the per-node wallet secret (the boot
        // path has no user password), so re-auth above is the actual gate.
        let mut node_secret = crate::container::lnd::wallet_password_if_exists()
            .await
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Could not decrypt the saved Lightning seed — the per-node \
                     wallet secret is missing"
                )
            })?;
        let words =
            crate::seed::load_lnd_aezeed_encrypted(&self.config.data_dir, &node_secret).await;
        node_secret.zeroize();
        let words = words
            .map_err(|_| anyhow::anyhow!("Could not decrypt the saved Lightning seed backup"))?;

        let word_count = words.len();
        Ok(serde_json::json!({ "words": words, "word_count": word_count }))
    }

    /// Record that the user confirmed backing up the Lightning seed, which
    /// dismisses the first-launch prompt.
    pub(in crate::api::rpc) async fn handle_lnd_seed_backup_ack(
        &self,
    ) -> Result<serde_json::Value> {
        crate::seed::mark_lnd_aezeed_acknowledged(&self.config.data_dir).await?;
        Ok(serde_json::json!({ "acknowledged": true }))
    }
}
