//! Owner RPC for permanent Cloud purchases. Registered app rentals use the
//! separate native installed-app context + opaque playback-handle dispatcher.
use super::RpcHandler;
use crate::{
    content_purchase::Journal,
    content_purchase_caller::{self as caller, PurchaseConsent, PurchaseTransport, ReadyPurchase},
    content_purchase_transport::FipsPurchaseTransport,
};
use anyhow::{Context, Result};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PurchaseParams {
    onion: String,
    content_id: String,
    filename: Option<String>,
    max_wallet_debit: u64,
    consent: Option<PurchaseConsent>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelParams {
    onion: String,
    operation_id: String,
}
impl RpcHandler {
    pub(super) async fn handle_content_purchase(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params: PurchaseParams =
            serde_json::from_value(params.context("Missing purchase parameters")?)?;
        anyhow::ensure!(
            !params.content_id.starts_with("registered_"),
            "Registered rentals require the native app purchase context"
        );
        let transport =
            FipsPurchaseTransport::load(self.config.data_dir.clone(), params.onion.clone()).await?;
        let identity =
            crate::identity::NodeIdentity::load_existing(&self.config.data_dir.join("identity"))
                .await?;
        let buyer = identity.did_key()?;
        let _rail = crate::content_payment_admission::lock(
            &self.config.data_dir,
            &buyer,
            transport.seller_did(),
            &params.content_id,
        )
        .await?;
        self.ensure_onchain_allows_other_rail(&buyer, transport.seller_did(), &params.content_id)
            .await?;
        self.ensure_invoice_allows_other_rail(&buyer, transport.seller_did(), &params.content_id)
            .await?;

        let result = caller::purchase(
            &self.config.data_dir,
            &buyer,
            &params.content_id,
            params.filename.as_deref(),
            params.max_wallet_debit,
            params.consent.as_ref(),
            &transport,
        )
        .await?;
        match result {
            ReadyPurchase::Preparing { .. } => {
                anyhow::bail!("Ordinary purchase cannot prepare a timed rental")
            }
            ReadyPurchase::AwaitingConfirmation {
                operation_id,
                envelope_sha256,
                gross_token_sats,
                seller_net_sats,
                wallet_debit_sats,
                expires_at,
                network,
                mint_url,
            } => Ok(
                serde_json::json!({"state":"confirmation_required","operation_id":operation_id,
                    "envelope_sha256":envelope_sha256,"gross_token_sats":gross_token_sats,
                    "seller_net_sats":seller_net_sats,"wallet_debit_sats":wallet_debit_sats,"expires_at":expires_at,"network":network,"mint_url":mint_url}),
            ),
            ReadyPurchase::Cancelled { operation_id } => {
                Ok(serde_json::json!({"state":"cancelled_unspent","operation_id":operation_id}))
            }
            ReadyPurchase::Cached { content_id, .. } => Ok(
                serde_json::json!({"state":"delivered","owned":true,"owned_content_id":content_id}),
            ),
            ReadyPurchase::Entitlement { contract, receipt } => {
                let item = crate::content_purchase_download::cache(
                    &self.config.data_dir,
                    &params.onion,
                    &contract,
                    &receipt,
                )
                .await?;
                Ok(
                    serde_json::json!({"state":"delivered","owned":true,"operation_id":contract.id,
                    "owned_content_id":item.content_id,"mime_type":item.mime_type,"size_bytes":item.size_bytes}),
                )
            }
        }
    }
    pub(super) async fn handle_content_cancel_purchase(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params: CancelParams =
            serde_json::from_value(params.context("Missing cancellation parameters")?)?;
        let transport =
            FipsPurchaseTransport::load(self.config.data_dir.clone(), params.onion).await?;
        let identity =
            crate::identity::NodeIdentity::load_existing(&self.config.data_dir.join("identity"))
                .await?;
        let (envelope, plan) = {
            let journal = Journal::open(&self.config.data_dir).await?;
            let record = journal
                .buyer(&params.operation_id)
                .await?
                .context("Original purchase not found")?;
            anyhow::ensure!(
                record.contract.buyer_did == identity.did_key()?
                    && record.contract.seller_did == transport.seller_did(),
                "Cancellation purchase binding changed"
            );
            (
                journal
                    .protocol_envelope("buyer", &params.operation_id)
                    .await?
                    .context("Original payment shape missing")?,
                journal
                    .buyer_plan(&params.operation_id)
                    .await?
                    .context("Original wallet plan missing")?,
            )
        };
        caller::cancel_purchase(&self.config.data_dir, &envelope, &plan, &transport).await?;
        Ok(serde_json::json!({"state":"cancelled_unspent","operation_id":params.operation_id}))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RentalParams {
    #[serde(default)]
    retry_preparation: bool,
    seller_did: String,
    content_id: String,
    expected_sha256: String,
    expected_price_sats: u64,
    expected_viewing_seconds: u64,
    max_wallet_debit: u64,
    consent: Option<PurchaseConsent>,
}
impl RpcHandler {
    pub(super) async fn handle_content_rental_purchase(
        &self,
        input: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params: RentalParams =
            serde_json::from_value(input.context("Missing rental purchase terms")?)?;
        anyhow::ensure!(
            params.content_id.starts_with("registered_")
                && params.expected_price_sats > 0
                && (1..=31_536_000).contains(&params.expected_viewing_seconds)
                && params.expected_sha256.len() == 64
                && params
                    .expected_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Invalid published rental terms"
        );
        let identity =
            crate::identity::NodeIdentity::load_existing(&self.config.data_dir.join("identity"))
                .await?;
        let buyer = identity.did_key()?;
        let onion = crate::content_purchase_transport::seller_onion_for_did(
            &self.config.data_dir,
            &params.seller_did,
        )
        .await?;
        let transport = FipsPurchaseTransport::load(self.config.data_dir.clone(), onion.clone())
            .await?
            .retry_preparation(params.retry_preparation);
        // Hold the same outer admission lock as every other payment rail,
        // including quote recovery and delayed consent callbacks. Check journals
        // only after locking so an in-flight alternate rail cannot be missed.
        let _rail = crate::content_payment_admission::lock(
            &self.config.data_dir,
            &buyer,
            transport.seller_did(),
            &params.content_id,
        )
        .await?;
        self.ensure_onchain_allows_other_rail(&buyer, transport.seller_did(), &params.content_id)
            .await?;
        self.ensure_invoice_allows_other_rail(&buyer, transport.seller_did(), &params.content_id)
            .await?;
        let (state, _) = self.state_manager.get_snapshot().await;
        let data = self.config.data_dir.clone();
        tokio::task::spawn_blocking(move || {
            crate::container::registration_pin::installed_context(&data, &identity, &state)
        })
        .await??;
        let expected = caller::ExpectedRental {
            seller_did: params.seller_did,
            content_id: params.content_id.clone(),
            sha256: params.expected_sha256,
            price_sats: params.expected_price_sats,
            viewing_seconds: params.expected_viewing_seconds,
        };
        match caller::purchase_bound(
            &self.config.data_dir,
            &buyer,
            &params.content_id,
            None,
            params.max_wallet_debit,
            params.consent.as_ref(),
            &transport,
            Some(&expected),
        )
        .await?
        {
            ReadyPurchase::Preparing {
                completed_bytes,
                total_bytes,
            } => Ok(serde_json::json!({
                "state":"preparing", "completed_bytes":completed_bytes,"total_bytes":total_bytes})),
            ReadyPurchase::AwaitingConfirmation {
                operation_id,
                envelope_sha256,
                gross_token_sats,
                seller_net_sats,
                wallet_debit_sats,
                expires_at,
                network,
                mint_url,
            } => Ok(serde_json::json!({
                "state":"confirmation_required","operation_id":operation_id,"envelope_sha256":envelope_sha256,
                "gross_token_sats":gross_token_sats,"seller_net_sats":seller_net_sats,"wallet_debit_sats":wallet_debit_sats,
                "expires_at":expires_at,"seller_onion":onion,"network":network,"mint_url":mint_url})),
            ReadyPurchase::Entitlement { contract, .. } => Ok(
                serde_json::json!({"state":"entitled","operation_id":contract.id,"seller_onion":onion}),
            ),
            ReadyPurchase::Cancelled { operation_id } => Ok(
                serde_json::json!({"state":"cancelled_unspent","operation_id":operation_id,"seller_onion":onion}),
            ),
            ReadyPurchase::Cached { .. } => {
                anyhow::bail!("A timed rental cannot use a permanent owned copy")
            }
        }
    }
}
