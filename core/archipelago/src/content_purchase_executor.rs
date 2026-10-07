//! Purchase/wallet sequencing only. The caller handles authenticated transport,
//! node-side pending-operation lookup and retained immutable content snapshots.
//! No legacy purchase RPC should call this until those boundaries are connected.
use crate::{
    content_purchase::{BuyerPhase, Contract, Journal, Receipt, SellerPhase},
    wallet::ecash,
};
use anyhow::{Context, Result};
use std::path::Path;

/// Buyer Intent and authenticated AcceptanceSaved must already be durable.
/// This function never allocates a purchase UUID or accepts a client mint time.
pub(crate) async fn prepare_buyer_token(data_dir: &Path, contract: &Contract) -> Result<String> {
    contract.validate()?;
    {
        let journal = Journal::open(data_dir).await?;
        let record = journal
            .buyer(&contract.id)
            .await?
            .context("Buyer intent is not durable")?;
        anyhow::ensure!(&record.contract == contract, "Buyer purchase terms changed");
        if let Some(token) = record.token() {
            return Ok(token.to_owned());
        }
        anyhow::ensure!(
            record.phase == BuyerPhase::AcceptanceSaved,
            "Authenticated seller acceptance is not durable"
        );
    }
    // Deadline is enforced inside the wallet after recovering original results,
    // immediately before a fresh exact send or mint POST. Do not pre-reject an
    // expired contract here: a previous wallet commit may need to be recovered.
    let token = ecash::send_token_recoverable_before(
        data_dir,
        &contract.id,
        contract.network,
        &contract.mint_url,
        contract.gross_token_sats,
        &contract.context_hash()?,
        contract.expires_at,
    )
    .await?;
    let journal = Journal::open(data_dir).await?;
    let record = journal.record_token(contract, &token).await?;
    Ok(record
        .token()
        .context("Prepared buyer token is missing")?
        .to_owned())
}

/// Caller verified the buyer's signed exact-body request and bound it to this
/// contract. Seller acceptance must predate offer expiry; an existing accepted
/// liability honors delayed first settlement, not merely an already minted claim.
pub(crate) async fn settle_seller_token(
    data_dir: &Path,
    contract: &Contract,
    encoded: &str,
    verified_buyer_did: &str,
) -> Result<Receipt> {
    anyhow::ensure!(
        verified_buyer_did == contract.buyer_did,
        "Payment is from another buyer"
    );
    contract.validate()?;
    {
        let journal = Journal::open(data_dir).await?;
        // Missing acceptance rejects, regardless of current time; this endpoint
        // cannot manufacture seller acceptance from an arriving payment token.
        let record = journal.record_incoming_token(contract, encoded).await?;
        match record.phase {
            SellerPhase::ReceiptSaved(receipt) => return Ok(receipt),
            SellerPhase::Settled { .. } => return journal.issue_receipt(contract).await,
            SellerPhase::Intent => (),
        }
    }
    let received = ecash::receive_token_recoverable(
        data_dir,
        &contract.id,
        contract.network,
        &contract.mint_url,
        encoded,
        contract.minimum_net_sats,
        &contract.context_hash()?,
    )
    .await?;
    let journal = Journal::open(data_dir).await?;
    journal.record_settlement(contract, received).await?;
    journal.issue_receipt(contract).await
}
