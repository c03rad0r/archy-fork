//! Buyer orchestration. Transport implementation MUST use authenticated exact-body
//! single-delivery FIPS requests to the verified seller; retries replay this UUID.
use crate::{
    content_purchase::{BuyerPhase, Contract, Journal, Receipt},
    content_purchase_protocol::{Accepted, Cancelled, Envelope, Offer, SellerStatus, Settlement},
    wallet::purchase_plan,
};
use anyhow::{Context, Result};
use std::{future::Future, path::Path};

pub(crate) trait PurchaseTransport: Send + Sync {
    /// This identity is the independently verified peer binding, not response JSON.
    fn seller_did(&self) -> &str;
    fn seller_onion(&self) -> &str;
    fn prepare_offer(
        &self,
        _content_id: &str,
    ) -> impl Future<Output = Result<Option<(u64, u64)>>> + Send {
        async { Ok(None) }
    }
    fn offer(&self, id: &str, content_id: &str) -> impl Future<Output = Result<Offer>> + Send;
    fn accept(&self, envelope: &Envelope) -> impl Future<Output = Result<Accepted>> + Send;
    fn status(&self, envelope: &Envelope) -> impl Future<Output = Result<SellerStatus>> + Send;
    fn cancel(&self, envelope: &Envelope) -> impl Future<Output = Result<Cancelled>> + Send;
    fn settle(&self, request: &Settlement) -> impl Future<Output = Result<Receipt>> + Send;
}
#[derive(Clone)]
pub(crate) enum ReadyPurchase {
    Preparing {
        completed_bytes: u64,
        total_bytes: u64,
    },
    AwaitingConfirmation {
        operation_id: String,
        envelope_sha256: String,
        gross_token_sats: u64,
        seller_net_sats: u64,
        wallet_debit_sats: u64,
        expires_at: i64,
        network: crate::wallet::ecash::EcashNetwork,
        mint_url: String,
    },
    Cancelled {
        operation_id: String,
    },
    Cached {
        seller_onion: String,
        content_id: String,
    },
    Entitlement {
        contract: Contract,
        receipt: Receipt,
    },
}

#[derive(Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PurchaseConsent {
    pub operation_id: String,
    pub envelope_sha256: String,
    pub wallet_debit_sats: u64,
}

/// Called after owner consent to content and maximum total wallet debit. Existing
/// pending purchase lookup runs before UUID allocation, even after browser loss.
/// Caller must separately reject unresolved legacy attempts; never invent their IDs.
pub(crate) async fn purchase(
    data_dir: &Path,
    verified_buyer: &str,
    content_id: &str,
    filename: Option<&str>,
    max_wallet_debit: u64,
    consent: Option<&PurchaseConsent>,
    transport: &impl PurchaseTransport,
) -> Result<ReadyPurchase> {
    purchase_bound(
        data_dir,
        verified_buyer,
        content_id,
        filename,
        max_wallet_debit,
        consent,
        transport,
        None,
    )
    .await
}

#[derive(Clone)]
pub(crate) struct ExpectedRental {
    pub seller_did: String,
    pub content_id: String,
    pub sha256: String,
    pub price_sats: u64,
    pub viewing_seconds: u64,
}
impl ExpectedRental {
    pub fn verify(&self, offer: &Offer) -> Result<()> {
        anyhow::ensure!(
            self.content_id.starts_with("registered_")
                && offer.content_id == self.content_id
                && offer.seller_did == self.seller_did
                && offer.content_sha256 == self.sha256
                && offer.seller_net_sats == self.price_sats
                && offer.viewing_seconds == Some(self.viewing_seconds),
            "Published rental hash, price, duration or seller changed; no payment started"
        );
        Ok(())
    }
}
pub(crate) async fn purchase_bound(
    data_dir: &Path,
    verified_buyer: &str,
    content_id: &str,
    filename: Option<&str>,
    max_wallet_debit: u64,
    consent: Option<&PurchaseConsent>,
    transport: &impl PurchaseTransport,
    expected: Option<&ExpectedRental>,
) -> Result<ReadyPurchase> {
    let _purchase_lock =
        crate::content_owned::lock_seller_purchases(transport.seller_onion()).await;
    let owned = crate::content_owned::list_owned_checked(data_dir)
        .await
        .context("Could not verify prior purchases; no new payment started")?;
    let mut unresolved_owned = false;
    if let Some(cached) = owned.iter().find(|item| {
        item.onion == transport.seller_onion()
            && (item.content_id == content_id
                || filename.is_some_and(|name| {
                    !name.is_empty()
                        && item.filename.trim_start_matches('/') == name.trim_start_matches('/')
                }))
    }) {
        // Metadata without bytes remains a delivery recovery, not permission to pay.
        if let Ok(Some((_, mut file))) =
            crate::content_owned::open_owned(data_dir, &cached.onion, &cached.content_id).await
        {
            let records = {
                Journal::open(data_dir)
                    .await?
                    .find_buyers(verified_buyer, transport.seller_did(), &cached.content_id)
                    .await?
            };
            if let Some(record) = records
                .iter()
                .find(|record| record.phase == BuyerPhase::ReceiptSaved)
            {
                // Recover the narrow crash window after cache publication but
                // before recording delivery. Never pay to repair this boundary.
                use sha2::{Digest, Sha256};
                use tokio::io::AsyncReadExt;
                let mut hash = Sha256::new();
                let mut count = 0u64;
                let mut buffer = vec![0; 65536];
                loop {
                    let read = file.read(&mut buffer).await?;
                    if read == 0 {
                        break;
                    }
                    count = count
                        .checked_add(read as u64)
                        .context("Cached size overflow")?;
                    anyhow::ensure!(
                        count <= record.contract.content_size,
                        "Cached purchase changed; no new payment allowed"
                    );
                    hash.update(&buffer[..read]);
                }
                let sha = hex::encode(hash.finalize());
                Journal::open(data_dir)
                    .await?
                    .record_delivery(&record.contract, &sha, count)
                    .await?;
            }
            return Ok(ReadyPurchase::Cached {
                seller_onion: cached.onion.clone(),
                content_id: cached.content_id.clone(),
            });
        }
        unresolved_owned = true;
    }
    let previous = {
        let journal = Journal::open(data_dir).await?;
        let matching = journal
            .find_buyers(verified_buyer, transport.seller_did(), content_id)
            .await?;
        let unresolved: Vec<_> = matching
            .into_iter()
            .filter(|record| record.phase != BuyerPhase::Cancelled)
            .collect();
        anyhow::ensure!(
            unresolved.len() <= 1,
            "Multiple original purchases require recovery; no new payment started"
        );
        unresolved.into_iter().next()
    };
    let envelope = if let Some(previous) = previous {
        let journal = Journal::open(data_dir).await?;
        let envelope = journal
            .protocol_envelope("buyer", &previous.contract.id)
            .await?
            .context("Original payment shape is missing; no new spend allowed")?;
        anyhow::ensure!(
            envelope.contract()? == previous.contract,
            "Original purchase binding changed"
        );
        if let Some(expected) = expected {
            expected.verify(&envelope.offer)?;
        }
        if let Some(receipt) = previous.receipt() {
            return Ok(ReadyPurchase::Entitlement {
                contract: previous.contract.clone(),
                receipt: receipt.clone(),
            });
        }
        envelope
    } else {
        anyhow::ensure!(
            !unresolved_owned,
            "Prior purchase delivery needs recovery; no new payment operation was created"
        );
        if content_id.starts_with("registered_") {
            if let Some((completed_bytes, total_bytes)) =
                transport.prepare_offer(content_id).await?
            {
                return Ok(ReadyPurchase::Preparing {
                    completed_bytes,
                    total_bytes,
                });
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        let offer = transport.offer(&id, content_id).await?;
        offer.validate()?;
        anyhow::ensure!(
            offer.id == id
                && offer.content_id == content_id
                && offer.buyer_did == verified_buyer
                && offer.seller_did == transport.seller_did(),
            "Authenticated seller offer binding changed"
        );
        if let Some(expected) = expected {
            expected.verify(&offer)?;
        }
        let plan = purchase_plan::prepare(
            data_dir,
            &offer.mint_url,
            offer.network,
            offer.seller_net_sats,
            max_wallet_debit,
        )
        .await?;
        let envelope = Envelope {
            offer,
            fee_plan: plan.fee_plan.clone(),
        };
        purchase_plan::persist_intent(data_dir, &envelope, &plan).await?;
        envelope
    };
    let contract = envelope.contract()?;
    let (phase, plan) = {
        let journal = Journal::open(data_dir).await?;
        let buyer = journal
            .buyer(&contract.id)
            .await?
            .context("Buyer intent disappeared")?;
        let plan = journal
            .buyer_plan(&contract.id)
            .await?
            .context("Original wallet plan is missing")?;
        anyhow::ensure!(
            plan.fee_plan == envelope.fee_plan,
            "Original wallet fee shape changed"
        );
        (buyer.phase, plan)
    };
    if phase == BuyerPhase::CancellationPending {
        cancel_purchase(data_dir, &envelope, &plan, transport).await?;
        return Ok(ReadyPurchase::Cancelled {
            operation_id: contract.id,
        });
    }
    if phase == BuyerPhase::Intent {
        let expected = envelope.commitment()?;
        if let Some(consent) = consent {
            anyhow::ensure!(
                consent.operation_id == contract.id
                    && consent.envelope_sha256 == expected
                    && consent.wallet_debit_sats == plan.wallet_debit_sats,
                "Payment confirmation does not match the saved quote"
            );
        } else {
            return Ok(ReadyPurchase::AwaitingConfirmation {
                operation_id: contract.id,
                envelope_sha256: expected,
                gross_token_sats: contract.gross_token_sats,
                seller_net_sats: contract.minimum_net_sats,
                wallet_debit_sats: plan.wallet_debit_sats,
                expires_at: contract.expires_at,
                network: envelope.offer.network,
                mint_url: envelope.offer.mint_url.clone(),
            });
        }
    }
    // Replaying a prepared send journal never selects fresh wallet proofs.
    purchase_plan::persist(data_dir, &contract, &plan).await?;
    if phase == BuyerPhase::Intent {
        let accepted = transport.accept(&envelope).await?;
        anyhow::ensure!(
            accepted.envelope_sha256 == envelope.commitment()?,
            "Seller accepted another payment shape"
        );
        let journal = Journal::open(data_dir).await?;
        journal
            .record_acceptance(&contract, &accepted.acceptance, transport.seller_did())
            .await?;
    }
    // Confirm the required authenticated FIPS route before any fresh mint
    // dispatch. An outage keeps this operation; it never selects Tor/new UUID.
    // Disconnect after this check is still possible and remains recoverable.
    let known_receipt = if phase != BuyerPhase::TokenPrepared {
        match transport.status(&envelope).await? {
            SellerStatus::Accepted => None,
            SellerStatus::Settled { receipt } => Some(receipt),
            SellerStatus::Cancelled => anyhow::bail!(
                "Seller cancelled this operation; reconcile the original unspent intent"
            ),
        }
    } else {
        None
    };
    // Planned executor performs fresh-post keyset/fee validation at the wallet
    // mutation boundary; original committed/restore results recover before expiry.
    let token = crate::content_purchase_executor::prepare_buyer_token_planned(
        data_dir,
        &contract,
        &envelope.fee_plan,
    )
    .await?;
    envelope.fee_plan.validate_token(&token)?;
    let receipt = if let Some(receipt) = known_receipt {
        receipt
    } else {
        transport.settle(&Settlement { envelope, token }).await?
    };
    {
        let journal = Journal::open(data_dir).await?;
        journal.record_receipt(&contract, &receipt).await?;
    }
    Ok(ReadyPurchase::Entitlement { contract, receipt })
}

/// Explicit caller cancellation/expired-unfunded recovery, never an automatic
/// interpretation of a failed HTTP call or unknown mint state.
pub(crate) async fn cancel_purchase(
    data_dir: &Path,
    envelope: &Envelope,
    plan: &purchase_plan::PreparedPayment,
    transport: &impl PurchaseTransport,
) -> Result<()> {
    let contract = envelope.contract()?;
    anyhow::ensure!(
        contract.seller_did == transport.seller_did(),
        "Cancellation seller changed"
    );
    purchase_plan::cancel_unspent(data_dir, &contract, plan).await?;
    {
        Journal::open(data_dir)
            .await?
            .begin_cancellation(&contract)
            .await?;
    }
    let reply = transport.cancel(envelope).await?;
    anyhow::ensure!(
        reply.envelope_sha256 == envelope.commitment()?,
        "Seller cancelled another operation"
    );
    Journal::open(data_dir)
        .await?
        .finish_cancellation(&contract, transport.seller_did())
        .await
}
