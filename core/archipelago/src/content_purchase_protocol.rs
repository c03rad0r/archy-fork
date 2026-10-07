//! Typed bodies for authenticated, single-delivery purchase POST endpoints.
//! The handler verifies v2 method/path/audience/exact-body proof before calling.
use crate::{
    content_purchase::{Acceptance, Contract, Journal, Receipt, SellerPhase},
    wallet::{ecash::EcashNetwork, mint_client::MintClient, purchase_fee_plan::FeePlan},
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub(crate) const OFFER_ROUTE: &str = "/content/purchase/v1/offer";
pub(crate) const ACCEPT_ROUTE: &str = "/content/purchase/v1/accept";
pub(crate) const SETTLE_ROUTE: &str = "/content/purchase/v1/settle";
pub(crate) const STATUS_ROUTE: &str = "/content/purchase/v1/status";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Offer {
    pub id: String,
    pub buyer_did: String,
    pub seller_did: String,
    pub content_id: String,
    pub filename: String,
    pub mime_type: String,
    pub content_sha256: String,
    pub content_size: u64,
    #[serde(default)]
    pub viewing_seconds: Option<u64>,
    pub terms_sha256: String,
    pub network: EcashNetwork,
    pub mint_url: String,
    pub seller_net_sats: u64,
    pub offered_at: i64,
    pub expires_at: i64,
}
impl Offer {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            if self.content_id.starts_with("registered_") {
                self.viewing_seconds
                    .is_some_and(|seconds| (1..=31_536_000).contains(&seconds))
            } else {
                self.viewing_seconds.is_none()
            },
            "Offer rental duration binding is invalid"
        );
        anyhow::ensure!(
            !self.filename.is_empty()
                && self.filename.len() <= 4096
                && !self.filename.chars().any(char::is_control),
            "Invalid offer filename"
        );
        anyhow::ensure!(
            self.mime_type.len() <= 128
                && self.mime_type.parse::<hyper::header::HeaderValue>().is_ok(),
            "Invalid offer MIME type"
        );
        let contract = Contract {
            version: 1,
            id: self.id.clone(),
            buyer_did: self.buyer_did.clone(),
            seller_did: self.seller_did.clone(),
            content_id: self.content_id.clone(),
            content_sha256: self.content_sha256.clone(),
            content_size: self.content_size,
            terms_sha256: self.terms_sha256.clone(),
            network: self.network,
            mint_url: self.mint_url.clone(),
            gross_token_sats: self.seller_net_sats,
            minimum_net_sats: self.seller_net_sats,
            offered_at: self.offered_at,
            expires_at: self.expires_at,
        };
        contract.validate()
    }
    pub fn contract(&self, plan: &FeePlan) -> Result<Contract> {
        self.validate()?;
        plan.validate()?;
        anyhow::ensure!(
            plan.mint_url == self.mint_url && plan.net_sats >= self.seller_net_sats,
            "Payment plan does not cover this offer"
        );
        let contract = Contract {
            version: 1,
            id: self.id.clone(),
            buyer_did: self.buyer_did.clone(),
            seller_did: self.seller_did.clone(),
            content_id: self.content_id.clone(),
            content_sha256: self.content_sha256.clone(),
            content_size: self.content_size,
            terms_sha256: self.terms_sha256.clone(),
            network: self.network,
            mint_url: self.mint_url.clone(),
            gross_token_sats: plan.gross_sats,
            minimum_net_sats: self.seller_net_sats,
            offered_at: self.offered_at,
            expires_at: self.expires_at,
        };
        contract.validate()?;
        Ok(contract)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Envelope {
    pub offer: Offer,
    pub fee_plan: FeePlan,
}
impl Envelope {
    pub fn contract(&self) -> Result<Contract> {
        self.offer.contract(&self.fee_plan)
    }
    pub fn commitment(&self) -> Result<String> {
        use sha2::{Digest, Sha256};
        let contract = self.contract()?;
        // Explicit ordered components: never hash an incidental map ordering.
        Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
            "archipelago-purchase-envelope-v1",
            contract.context_hash()?,
            self.fee_plan.commitment()?,
            &self.offer.filename,
            &self.offer.mime_type,
            self.offer.viewing_seconds,
        ))?)))
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Accepted {
    pub envelope_sha256: String,
    pub acceptance: Acceptance,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settlement {
    pub envelope: Envelope,
    pub token: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum SellerStatus {
    Accepted,
    Cancelled,
    Settled { receipt: Receipt },
}

/// Match the policy used by fresh seller redemption before inviting a spend.
/// Settled receipts remain recoverable independently of later wallet settings.
pub(crate) async fn ensure_seller_mint_policy(
    data_dir: &Path,
    network: crate::wallet::ecash::EcashNetwork,
    mint_url: &str,
) -> Result<()> {
    use crate::{content_purchase::canonical_mint, wallet::ecash};
    anyhow::ensure!(
        ecash::load_network(data_dir).await? == network,
        "Seller wallet is on another payment network; no new payment should be sent"
    );
    let accepted = ecash::load_accepted_mints(data_dir).await?;
    let mint = canonical_mint(mint_url)?;
    anyhow::ensure!(
        accepted
            .mints
            .iter()
            .any(|candidate| canonical_mint(candidate).ok().as_deref() == Some(mint.as_str())),
        "Seller does not accept the quoted mint; no new payment should be sent"
    );
    Ok(())
}

/// Caller obtained these fields from a currently shared, immutable verified
/// snapshot (registered_terms/ordinary catalog snapshot), never from client JSON.
/// Save before returning the offer; replay always returns original terms.
pub(crate) async fn save_offer(
    data_dir: &Path,
    offered: &Offer,
    verified_buyer: &str,
    now: i64,
) -> Result<Offer> {
    anyhow::ensure!(offered.buyer_did == verified_buyer, "Offer buyer mismatch");
    ensure_seller_mint_policy(data_dir, offered.network, &offered.mint_url).await?;
    let journal = Journal::open(data_dir).await?;
    if let Some(saved) = journal.protocol_offer(&offered.id).await? {
        anyhow::ensure!(
            saved.buyer_did == verified_buyer && saved.content_id == offered.content_id,
            "Offer operation changed buyer/content"
        );
        return Ok(saved);
    }
    anyhow::ensure!(
        now >= offered.offered_at && now < offered.expires_at,
        "Offer is expired"
    );
    journal.save_protocol_offer(offered).await?;
    Ok(offered.clone())
}
/// POST ACCEPT_ROUTE. Persist both fee commitment and liability before replying.
pub(crate) async fn accept(
    data_dir: &Path,
    envelope: &Envelope,
    verified_buyer: &str,
    now: impl Fn() -> i64,
) -> Result<Accepted> {
    let contract = envelope.contract()?;
    anyhow::ensure!(
        contract.buyer_did == verified_buyer,
        "Acceptance buyer mismatch"
    );
    {
        let journal = Journal::open(data_dir).await?;
        let offer = journal
            .protocol_offer(&contract.id)
            .await?
            .context("Seller offer is missing")?;
        anyhow::ensure!(offer == envelope.offer, "Seller offer changed");
        if let Some(previous) = journal.protocol_envelope("seller", &contract.id).await? {
            anyhow::ensure!(previous == *envelope, "Accepted payment shape changed");
            if let Some(record) = journal.seller(&contract.id).await? {
                return Ok(Accepted {
                    envelope_sha256: envelope.commitment()?,
                    acceptance: record.acceptance()?,
                });
            }
        }
    }
    ensure_seller_mint_policy(data_dir, contract.network, &contract.mint_url).await?;
    // No database lock across remote mint query. Recheck durable binding/time
    // when committing below, so concurrent acceptance cannot alter the plan.
    let keysets = MintClient::new(&contract.mint_url)?.get_keysets().await?;
    envelope.fee_plan.verify_mint_keysets(&keysets, false)?;
    // Same wallet -> purchase lock order as policy updates: an accepted
    // liability cannot race removal of its mint or a network switch.
    let _wallet = crate::wallet::mutation::guard(data_dir).await?;
    ensure_seller_mint_policy(data_dir, contract.network, &contract.mint_url).await?;
    let journal = Journal::open(data_dir).await?;
    anyhow::ensure!(
        journal.protocol_offer(&contract.id).await?.as_ref() == Some(&envelope.offer)
            && !journal.retired_offer_matches(&envelope.offer).await?,
        "Original offer retired before acceptance; recover cancellation without spending"
    );
    journal.save_protocol_envelope("seller", envelope).await?;
    let record = journal.prepare_seller(&contract, now()).await?;
    Ok(Accepted {
        envelope_sha256: envelope.commitment()?,
        acceptance: record.acceptance()?,
    })
}
/// POST SETTLE_ROUTE. Accepting new liability is deliberately impossible here.
pub(crate) async fn settle(
    data_dir: &Path,
    request: &Settlement,
    verified_buyer: &str,
) -> Result<Receipt> {
    let contract = request.envelope.contract()?;
    anyhow::ensure!(
        contract.buyer_did == verified_buyer,
        "Settlement buyer mismatch"
    );
    {
        let journal = Journal::open(data_dir).await?;
        let saved = journal
            .protocol_envelope("seller", &contract.id)
            .await?
            .context("Seller acceptance is missing")?;
        anyhow::ensure!(
            saved == request.envelope,
            "Settlement changed accepted payment shape"
        );
    }
    request.envelope.fee_plan.validate_token(&request.token)?;
    crate::content_purchase_executor::settle_seller_token(
        data_dir,
        &contract,
        &request.token,
        verified_buyer,
    )
    .await
}
/// POST STATUS_ROUTE. Read-only, bound to buyer and exact accepted envelope.
pub(crate) async fn status(
    data_dir: &Path,
    envelope: &Envelope,
    verified_buyer: &str,
) -> Result<SellerStatus> {
    let contract = envelope.contract()?;
    anyhow::ensure!(
        contract.buyer_did == verified_buyer,
        "Status buyer mismatch"
    );
    let journal = Journal::open(data_dir).await?;
    anyhow::ensure!(
        journal
            .protocol_envelope("seller", &contract.id)
            .await?
            .as_ref()
            == Some(envelope),
        "Accepted operation not found"
    );
    let record = journal
        .seller(&contract.id)
        .await?
        .context("Accepted operation not found")?;
    match record.phase {
        SellerPhase::ReceiptSaved(receipt) => Ok(SellerStatus::Settled { receipt }),
        SellerPhase::Cancelled => Ok(SellerStatus::Cancelled),
        _ => {
            ensure_seller_mint_policy(data_dir, contract.network, &contract.mint_url).await?;
            Ok(SellerStatus::Accepted)
        }
    }
}

pub(crate) const CANCEL_ROUTE: &str = "/content/purchase/v1/cancel";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Cancelled {
    pub envelope_sha256: String,
}
/// Authenticated buyer sealed its wallet before requesting cancellation. The
/// seller only seals an unfunded intent and rejects every subsequent late accept.
pub(crate) async fn cancel(
    data_dir: &Path,
    envelope: &Envelope,
    verified_buyer: &str,
) -> Result<Cancelled> {
    let contract = envelope.contract()?;
    anyhow::ensure!(
        contract.buyer_did == verified_buyer,
        "Cancellation buyer changed"
    );
    let journal = Journal::open(data_dir).await?;
    anyhow::ensure!(
        journal.protocol_offer(&contract.id).await?.as_ref() == Some(&envelope.offer)
            || journal.retired_offer_matches(&envelope.offer).await?,
        "Original seller offer changed"
    );
    journal.save_protocol_envelope("seller", envelope).await?;
    journal.cancel_seller(&contract).await?;
    Ok(Cancelled {
        envelope_sha256: envelope.commitment()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::{
        cashu::{CashuToken, Proof},
        purchase_fee_plan::KeysetPlan,
    };
    fn envelope() -> Envelope {
        let buyer = crate::identity::did_key_from_pubkey_hex(&hex::encode([1u8; 32])).unwrap();
        let seller = crate::identity::did_key_from_pubkey_hex(&hex::encode([2u8; 32])).unwrap();
        let fee_plan = FeePlan::from_shape(
            "https://unused-mint.invalid",
            vec![KeysetPlan {
                keyset_id: "0011223344556677".into(),
                denominations: vec![8],
                input_fee_ppk: 0,
            }],
        )
        .unwrap();
        Envelope {
            offer: Offer {
                id: uuid::Uuid::new_v4().to_string(),
                buyer_did: buyer,
                seller_did: seller,
                content_id: "registered_film".into(),
                filename: "film.mp4".into(),
                mime_type: "video/mp4".into(),
                content_sha256: "ab".repeat(32),
                content_size: 10,
                viewing_seconds: Some(60),
                terms_sha256: "cd".repeat(32),
                network: EcashNetwork::Mainnet,
                mint_url: "https://unused-mint.invalid".into(),
                seller_net_sats: 8,
                offered_at: 1000,
                expires_at: 1100,
            },
            fee_plan,
        }
    }
    #[tokio::test]
    async fn unaccepted_mint_cannot_publish_offer_and_cancel_releases_policy_pin() {
        use crate::wallet::ecash::{
            save_accepted_mints, save_network, AcceptedMints, EcashNetwork,
        };
        let root = tempfile::tempdir().unwrap();
        let value = envelope();
        let buyer = &value.offer.buyer_did;
        assert!(save_offer(root.path(), &value.offer, buyer, 1001)
            .await
            .is_err());
        assert!(Journal::open(root.path())
            .await
            .unwrap()
            .protocol_offer(&value.offer.id)
            .await
            .unwrap()
            .is_none());
        save_accepted_mints(
            root.path(),
            &AcceptedMints {
                mints: vec![value.offer.mint_url.clone()],
            },
        )
        .await
        .unwrap();
        save_offer(root.path(), &value.offer, buyer, 1001)
            .await
            .unwrap();
        {
            let journal = Journal::open(root.path()).await.unwrap();
            journal
                .save_protocol_envelope("seller", &value)
                .await
                .unwrap();
            journal
                .prepare_seller(&value.contract().unwrap(), 1001)
                .await
                .unwrap();
        }
        assert!(
            save_accepted_mints(root.path(), &AcceptedMints { mints: vec![] })
                .await
                .is_err()
        );
        assert!(save_network(root.path(), EcashNetwork::Testnet)
            .await
            .is_err());
        cancel(root.path(), &value, buyer).await.unwrap();
        save_accepted_mints(root.path(), &AcceptedMints { mints: vec![] })
            .await
            .unwrap();
        save_network(root.path(), EcashNetwork::Testnet)
            .await
            .unwrap();
        assert!(matches!(
            status(root.path(), &value, buyer).await.unwrap(),
            SellerStatus::Cancelled
        ));
    }
    #[tokio::test]
    async fn seller_cancellation_replays_after_lost_reply_and_seals_late_acceptance() {
        let root = tempfile::tempdir().unwrap();
        let value = envelope();
        crate::wallet::ecash::save_accepted_mints(
            root.path(),
            &crate::wallet::ecash::AcceptedMints {
                mints: vec![value.offer.mint_url.clone()],
            },
        )
        .await
        .unwrap();
        let contract = value.contract().unwrap();
        save_offer(root.path(), &value.offer, &contract.buyer_did, 1001)
            .await
            .unwrap();
        {
            let journal = Journal::open(root.path()).await.unwrap();
            journal
                .save_protocol_envelope("seller", &value)
                .await
                .unwrap();
            journal.prepare_seller(&contract, 1001).await.unwrap();
            journal.prepare_buyer(&contract, 1001).await.unwrap();
            journal.begin_cancellation(&contract).await.unwrap();
        }
        let lost = cancel(root.path(), &value, &contract.buyer_did)
            .await
            .unwrap();
        assert!(accept(root.path(), &value, &contract.buyer_did, || 1002)
            .await
            .is_err());
        let replay = cancel(root.path(), &value, &contract.buyer_did)
            .await
            .unwrap();
        assert_eq!(lost.envelope_sha256, replay.envelope_sha256);
        let journal = Journal::open(root.path()).await.unwrap();
        journal
            .finish_cancellation(&contract, &contract.seller_did)
            .await
            .unwrap();
        assert_eq!(
            journal.buyer(&contract.id).await.unwrap().unwrap().phase,
            crate::content_purchase::BuyerPhase::Cancelled
        );
        let delayed = Acceptance {
            contract_hash: contract.context_hash().unwrap(),
            accepted_at: 1001,
        };
        assert!(journal
            .record_acceptance(&contract, &delayed, &contract.seller_did)
            .await
            .is_err());
    }
    #[tokio::test]
    async fn incoming_token_or_saved_settlement_prevents_seller_cancellation() {
        let root = tempfile::tempdir().unwrap();
        let value = envelope();
        crate::wallet::ecash::save_accepted_mints(
            root.path(),
            &crate::wallet::ecash::AcceptedMints {
                mints: vec![value.offer.mint_url.clone()],
            },
        )
        .await
        .unwrap();
        let contract = value.contract().unwrap();
        save_offer(root.path(), &value.offer, &contract.buyer_did, 1001)
            .await
            .unwrap();
        let token = CashuToken::new(
            &contract.mint_url,
            vec![Proof {
                amount: 8,
                id: "0011223344556677".into(),
                secret: "test-original-payment".into(),
                c: "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798".into(),
            }],
        )
        .serialize()
        .unwrap();
        {
            let journal = Journal::open(root.path()).await.unwrap();
            journal
                .save_protocol_envelope("seller", &value)
                .await
                .unwrap();
            journal.prepare_seller(&contract, 1001).await.unwrap();
            journal
                .record_incoming_token(&contract, &token)
                .await
                .unwrap();
        }
        assert!(cancel(root.path(), &value, &contract.buyer_did)
            .await
            .is_err());
        let original = {
            let journal = Journal::open(root.path()).await.unwrap();
            journal.record_settlement(&contract, 8).await.unwrap();
            journal.issue_receipt(&contract).await.unwrap()
        };
        assert!(cancel(root.path(), &value, &contract.buyer_did)
            .await
            .is_err());
        crate::wallet::ecash::save_accepted_mints(
            root.path(),
            &crate::wallet::ecash::AcceptedMints { mints: vec![] },
        )
        .await
        .unwrap();
        crate::wallet::ecash::save_network(
            root.path(),
            crate::wallet::ecash::EcashNetwork::Testnet,
        )
        .await
        .unwrap();
        match status(root.path(), &value, &contract.buyer_did)
            .await
            .unwrap()
        {
            SellerStatus::Settled { receipt } => assert!(receipt == original),
            _ => panic!("Original receipt lost"),
        }
    }
    #[tokio::test]
    async fn expired_quote_cap_recovers_without_reusing_ids_or_retiring_accepted_liability() {
        let root = tempfile::tempdir().unwrap();
        let now = chrono::Utc::now().timestamp();
        let mut accepted = envelope();
        accepted.offer.offered_at = now;
        accepted.offer.expires_at = now + 300;
        let mut expired = Vec::new();
        {
            let journal = Journal::open(root.path()).await.unwrap();
            journal.save_protocol_offer(&accepted.offer).await.unwrap();
            journal
                .save_protocol_envelope("seller", &accepted)
                .await
                .unwrap();
            journal
                .prepare_seller(&accepted.contract().unwrap(), now)
                .await
                .unwrap();
            for _ in 0..128 {
                let mut value = accepted.clone();
                value.offer.id = uuid::Uuid::new_v4().to_string();
                journal.save_protocol_offer(&value.offer).await.unwrap();
                expired.push(value);
            }
            let mut next = accepted.clone();
            next.offer.id = uuid::Uuid::new_v4().to_string();
            assert!(journal.save_protocol_offer(&next.offer).await.is_err());
            journal.retire_unaccepted_offers(now + 301).await.unwrap();
            assert!(journal
                .protocol_offer(&accepted.offer.id)
                .await
                .unwrap()
                .is_some());
            assert!(journal.seller(&accepted.offer.id).await.unwrap().is_some());
            assert!(journal
                .protocol_offer(&expired[0].offer.id)
                .await
                .unwrap()
                .is_none());
            assert!(journal
                .retired_offer_matches(&expired[0].offer)
                .await
                .unwrap());
            journal.save_protocol_offer(&next.offer).await.unwrap();
            let mut changed = expired[0].offer.clone();
            changed.expires_at += 300;
            assert!(journal.save_protocol_offer(&changed).await.is_err());
            assert!(!journal.retired_offer_matches(&changed).await.unwrap());
        }
        let original = &expired[0];
        let ack = cancel(root.path(), original, &original.offer.buyer_did)
            .await
            .unwrap();
        assert_eq!(ack.envelope_sha256, original.commitment().unwrap());
        assert_eq!(
            cancel(root.path(), original, &original.offer.buyer_did)
                .await
                .unwrap()
                .envelope_sha256,
            ack.envelope_sha256
        );
        assert!(
            accept(root.path(), original, &original.offer.buyer_did, || now)
                .await
                .is_err()
        );
    }
}
