//! Concrete buyer transport. Never redirects, changes route, or automatically
//! resends after an ambiguous delivery; the durable caller owns all replay.
use crate::{
    content_purchase::Receipt,
    content_purchase_caller::PurchaseTransport,
    content_purchase_protocol::{
        self as protocol, Accepted, Cancelled, Envelope, Offer, SellerStatus, Settlement,
    },
};
use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::{path::PathBuf, time::Duration};
pub(crate) struct FipsPurchaseTransport {
    data_dir: PathBuf,
    onion: String,
    seller_did: String,
    fips_npub: String,
    retry_preparation: bool,
}
impl FipsPurchaseTransport {
    pub async fn load(data_dir: PathBuf, onion: String) -> Result<Self> {
        let peer = crate::federation::load_unique_payment_peer(&data_dir, &onion).await?;
        let fips_npub = peer
            .fips_npub
            .context("Purchase seller has no authenticated mesh binding")?;
        anyhow::ensure!(
            !fips_npub.is_empty(),
            "Purchase seller has no authenticated mesh binding"
        );
        Ok(Self {
            data_dir,
            onion,
            seller_did: peer.did,
            fips_npub,
            retry_preparation: false,
        })
    }
    pub fn retry_preparation(mut self, retry: bool) -> Self {
        self.retry_preparation = retry;
        self
    }

    /// Ask the authenticated seller for the original rental window without
    /// starting or extending it. The capability never crosses into the app.
    pub async fn expired_rental_window(
        &self,
        contract: &crate::content_purchase::Contract,
        capability: &str,
        viewing_seconds: u64,
    ) -> Result<(u64, u64)> {
        let route = format!(
            "/content/{}/rental/{}/prepare",
            contract.content_id, contract.id
        );
        let body = serde_json::json!({"capability":capability,"ready_id":null,"retry":false});
        let (mut response, _) =
            crate::fips::dial::PeerRequest::new(Some(&self.fips_npub), &self.onion, &route)
                .require_fips()
                .single_delivery()
                .timeout(Duration::from_secs(20))
                .send_rental_control_json(&self.data_dir, &self.seller_did, &body)
                .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "Original rental status is unavailable"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(
                bytes
                    .len()
                    .checked_add(chunk.len())
                    .is_some_and(|n| n <= 16 * 1024),
                "Rental status response is too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
            value["state"] == "expired",
            "Original rental viewing period is still available"
        );
        let started = value["started_at"]
            .as_u64()
            .context("Missing rental start")?;
        let expires = value["expires_at"]
            .as_u64()
            .context("Missing rental expiry")?;
        anyhow::ensure!(
            value["viewing_seconds"].as_u64() == Some(viewing_seconds)
                && started.checked_add(viewing_seconds) == Some(expires),
            "Seller changed the original rental window"
        );
        Ok((started, expires))
    }
    async fn post<B: Serialize + Sync, R: DeserializeOwned>(
        &self,
        route: &str,
        body: &B,
    ) -> Result<R> {
        let (mut response, _) =
            crate::fips::dial::PeerRequest::new(Some(&self.fips_npub), &self.onion, route)
                .require_fips()
                .single_delivery()
                .timeout(Duration::from_secs(45))
                .send_content_json(&self.data_dir, &self.seller_did, body)
                .await?;
        let status = response.status();
        anyhow::ensure!(
            status.is_success(),
            "Purchase endpoint returned {}; recover the original operation",
            status.as_u16()
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(
                bytes
                    .len()
                    .checked_add(chunk.len())
                    .is_some_and(|n| n <= 65536),
                "Purchase response is too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes)
            .context("Invalid purchase response; original operation remains recoverable")
    }
}
impl PurchaseTransport for FipsPurchaseTransport {
    fn seller_onion(&self) -> &str {
        &self.onion
    }
    fn seller_did(&self) -> &str {
        &self.seller_did
    }
    async fn prepare_offer(
        &self,
        content_id: &str,
        expected: Option<&crate::content_purchase_caller::ExpectedRental>,
    ) -> Result<Option<(u64, u64)>> {
        let state: crate::rental_readiness::Status = self
            .post(
                protocol::PREPARE_OFFER_ROUTE,
                &serde_json::json!({"content_id":content_id,"retry":self.retry_preparation,"expected":expected}),
            )
            .await?;
        match state {
            crate::rental_readiness::Status::Ready { .. } => Ok(None),
            crate::rental_readiness::Status::Preparing {
                completed_bytes,
                total_bytes,
            } => {
                anyhow::ensure!(
                    completed_bytes <= total_bytes && total_bytes > 0,
                    "Invalid media preparation progress"
                );
                Ok(Some((completed_bytes, total_bytes)))
            }
            crate::rental_readiness::Status::Unavailable => anyhow::bail!(
                "Media verification failed; no payment started. Retry media preparation."
            ),
        }
    }
    async fn offer(&self, id: &str, content_id: &str) -> Result<Offer> {
        #[derive(Serialize)]
        struct Request<'a> {
            id: &'a str,
            content_id: &'a str,
        }
        self.post(protocol::OFFER_ROUTE, &Request { id, content_id })
            .await
    }
    async fn accept(&self, envelope: &Envelope) -> Result<Accepted> {
        self.post(protocol::ACCEPT_ROUTE, envelope).await
    }
    async fn status(&self, envelope: &Envelope) -> Result<SellerStatus> {
        self.post(protocol::STATUS_ROUTE, envelope).await
    }
    async fn cancel(&self, envelope: &Envelope) -> Result<Cancelled> {
        self.post(protocol::CANCEL_ROUTE, envelope).await
    }
    async fn settle(&self, request: &Settlement) -> Result<Receipt> {
        self.post(protocol::SETTLE_ROUTE, request).await
    }
}

pub(crate) async fn seller_onion_for_did(data_dir: &std::path::Path, did: &str) -> Result<String> {
    Ok(
        crate::federation::load_unique_payment_peer_by_did(data_dir, did)
            .await?
            .onion,
    )
}
