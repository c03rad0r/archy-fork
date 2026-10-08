//! Local-only deterministic payment planning. No swap POST or reservation here.
//! Serialize this object only into the private buyer journal, never onto the wire.
use super::{
    cashu::{KeysetInfo, Proof},
    ecash,
    mint_client::MintClient,
    mutation,
    purchase_fee_plan::{FeePlan, KeysetPlan},
    send_journal::{Binding, Journal, Request},
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PreparedPayment {
    pub fee_plan: FeePlan,
    /// Includes any buyer funding-swap input fee, distinct from seller redemption.
    pub wallet_debit_sats: u64,
    request: Request,
}
fn exact_shape(proofs: &[Proof], keysets: &[KeysetInfo]) -> Result<Vec<KeysetPlan>> {
    let mut groups: BTreeMap<String, KeysetPlan> = BTreeMap::new();
    for proof in proofs {
        let matches: Vec<_> = keysets
            .iter()
            .filter(|keyset| super::cashu::matches_stored_keyset_id(&proof.id, &keyset.id))
            .collect();
        anyhow::ensure!(matches.len() == 1, "Missing or ambiguous payment keyset");
        let keyset = matches[0];
        anyhow::ensure!(keyset.unit == "sat", "Payment keyset has another unit");
        groups
            .entry(keyset.id.to_ascii_lowercase())
            .or_insert_with(|| KeysetPlan {
                keyset_id: keyset.id.to_ascii_lowercase(),
                denominations: vec![],
                input_fee_ppk: keyset.input_fee_ppk,
            })
            .denominations
            .push(proof.amount);
    }
    Ok(groups.into_values().collect())
}
/// Quote at most 1024 gross amounts. Failure is explicit; never silently increase
/// the user-approved debit limit or substitute another mint/network.
pub(crate) async fn prepare(
    data_dir: &Path,
    mint: &str,
    expected_network: ecash::EcashNetwork,
    seller_net_sats: u64,
    max_wallet_debit: u64,
) -> Result<PreparedPayment> {
    anyhow::ensure!(
        seller_net_sats > 0 && max_wallet_debit >= seller_net_sats,
        "Invalid payment budget"
    );
    let _held = mutation::guard(data_dir).await?;
    anyhow::ensure!(
        ecash::load_network(data_dir).await? == expected_network,
        "Offer uses another wallet network; no intent was created"
    );
    let wallet = ecash::load_wallet(data_dir).await?;
    anyhow::ensure!(
        wallet.select_proofs(mint, seller_net_sats).is_some(),
        "No spendable balance at the seller's mint; no intent was created"
    );
    let client =
        MintClient::new(mint)?.with_recovery(super::nut13::RecoverySource::load(data_dir).await?);
    let keysets = client.get_keysets().await?;
    let active = client.get_active_sat_keyset().await?;
    let active_fee: Vec<_> = keysets
        .iter()
        .filter(|keyset| keyset.id == active.id && keyset.active && keyset.unit == "sat")
        .collect();
    anyhow::ensure!(
        active_fee.len() == 1,
        "Active payment keyset is not uniquely bound"
    );
    for additional in 0..1024u64 {
        let gross = seller_net_sats
            .checked_add(additional)
            .context("Payment amount overflow")?;
        if gross > max_wallet_debit {
            break;
        }
        let Some((indices, excess)) = wallet.select_proofs(mint, gross) else {
            break;
        };
        let proofs: Vec<_> = indices
            .iter()
            .map(|&index| wallet.proofs[index].proof.clone())
            .collect();
        let input_shape = exact_shape(&proofs, &keysets)?;
        let input_ppk = input_shape
            .iter()
            .try_fold(0u64, |total, group| {
                total.checked_add(
                    group
                        .input_fee_ppk
                        .checked_mul(group.denominations.len() as u64)?,
                )
            })
            .context("Funding fee overflow")?;
        let quoted_funding_fee = input_ppk.checked_add(999).context("Funding fee overflow")? / 1000;
        if excess > 0
            && (quoted_funding_fee > excess
                || gross
                    .checked_add(quoted_funding_fee)
                    .is_none_or(|debit| debit > max_wallet_debit))
        {
            continue;
        }
        let shape = if excess == 0 {
            input_shape
        } else {
            vec![KeysetPlan {
                keyset_id: active.id.to_ascii_lowercase(),
                denominations: super::cashu::amount_to_denominations(gross),
                input_fee_ppk: active_fee[0].input_fee_ppk,
            }]
        };
        let ppk = shape
            .iter()
            .try_fold(0u64, |total, group| {
                total.checked_add(
                    group
                        .input_fee_ppk
                        .checked_mul(group.denominations.len() as u64)?,
                )
            })
            .context("Quoted fee overflow")?;
        let fee = ppk.checked_add(999).context("Quoted fee overflow")? / 1000;
        if gross <= fee || gross - fee < seller_net_sats {
            continue;
        }
        let fee_plan = FeePlan::from_shape(mint, shape)?;
        if fee_plan.net_sats < seller_net_sats {
            continue;
        }
        let (request, funding_fee) = if excess == 0 {
            (Request::Exact { proofs }, 0)
        } else {
            let mut amounts = super::cashu::amount_to_denominations(gross);
            amounts.extend(super::cashu::amount_to_denominations(excess));
            let prepared = client
                .prepare_swap_at_least(&proofs, &amounts, gross)
                .await?;
            anyhow::ensure!(
                prepared.payment_keyset_id() == active.id,
                "Mint rotated while planning; refresh the offer"
            );
            let fee = prepared.input_fee_sats()?;
            anyhow::ensure!(
                fee == quoted_funding_fee,
                "Mint funding fee changed while planning; refresh before acceptance"
            );
            anyhow::ensure!(
                client.restore_prepared_swap(&prepared).await?.is_none(),
                "Planned outputs already exist; recover the previous operation"
            );
            (Request::Swap(prepared), fee)
        };
        let debit = gross
            .checked_add(funding_fee)
            .context("Payment debit overflow")?;
        anyhow::ensure!(
            debit <= max_wallet_debit,
            "Funding fee exceeds the approved debit limit"
        );
        return Ok(PreparedPayment {
            fee_plan,
            wallet_debit_sats: debit,
            request,
        });
    }
    anyhow::bail!("No bounded payment plan covers the seller price within the approved debit limit")
}
/// Must precede authenticated seller acceptance. This durably pins the exact
/// inputs/outputs without reserving or spending; stale inputs later reject.
pub(crate) async fn persist(
    data_dir: &Path,
    contract: &crate::content_purchase::Contract,
    plan: &PreparedPayment,
) -> Result<()> {
    contract.validate()?;
    anyhow::ensure!(
        plan.fee_plan.mint_url == contract.mint_url
            && plan.fee_plan.gross_sats == contract.gross_token_sats
            && plan.fee_plan.net_sats >= contract.minimum_net_sats,
        "Wallet plan changed purchase amounts"
    );
    let held = mutation::guard(data_dir).await?;
    anyhow::ensure!(
        ecash::load_network(data_dir).await? == contract.network,
        "Purchase wallet network changed"
    );
    let binding = Binding {
        id: contract.id.clone(),
        network: contract.network,
        mint_url: contract.mint_url.clone(),
        amount_sats: contract.gross_token_sats,
        context_hash: contract.context_hash()?,
    };
    let journal = Journal::new(&held);
    if let Some(existing) = journal.load(&contract.id).await? {
        anyhow::ensure!(
            existing.binding == binding
                && serde_json::to_value(&existing.request)? == serde_json::to_value(&plan.request)?,
            "Original wallet preparation changed"
        );
    }
    if journal.load(&contract.id).await?.is_none() {
        let buyer = crate::content_purchase::Journal::open(data_dir)
            .await?
            .buyer(&contract.id)
            .await?
            .context("Buyer intent is missing")?;
        anyhow::ensure!(buyer.phase == crate::content_purchase::BuyerPhase::Intent,
            "Accepted operation lost its wallet journal; no new preparation or cancellation is allowed");
    }
    journal.prepare(binding, plan.request.clone()).await?;
    Ok(())
}

/// Seal before contacting seller. Repeating after a lost seller reply is safe.
pub(crate) async fn cancel_unspent(
    data_dir: &Path,
    contract: &crate::content_purchase::Contract,
    plan: &PreparedPayment,
) -> Result<()> {
    persist(data_dir, contract, plan).await?;
    let held = mutation::guard(data_dir).await?;
    let binding = Binding {
        id: contract.id.clone(),
        network: contract.network,
        mint_url: contract.mint_url.clone(),
        amount_sats: contract.gross_token_sats,
        context_hash: contract.context_hash()?,
    };
    Journal::new(&held).cancel_unspent(&binding).await
}

/// Publish the buyer intent while holding the wallet network mutation guard, so
/// a concurrent network switch cannot strand a newly published wrong-network row.
pub(crate) async fn persist_intent(
    data_dir: &Path,
    envelope: &crate::content_purchase_protocol::Envelope,
    plan: &PreparedPayment,
) -> Result<()> {
    let contract = envelope.contract()?;
    anyhow::ensure!(plan.fee_plan == envelope.fee_plan, "Buyer fee plan changed");
    let _held = mutation::guard(data_dir).await?;
    anyhow::ensure!(
        ecash::load_network(data_dir).await? == contract.network,
        "Offer uses another wallet network; no intent was created"
    );
    let journal = crate::content_purchase::Journal::open(data_dir).await?;
    journal.save_buyer_plan(&contract.id, plan).await?;
    journal.save_protocol_envelope("buyer", envelope).await?;
    journal
        .prepare_buyer(&contract, chrono::Utc::now().timestamp())
        .await?;
    Ok(())
}
