//! Non-signable funding intent. No recipient address or executable PSBT exists
//! until explicit Pay has leased these exact inputs and recovered seller allocation.
use crate::{
    content_lightning::{Binding, RetainedFile},
    content_onchain::{ChainNetwork, FeePolicy, Funded, Lease, Quote, Record},
};
use anyhow::{Context, Result};
use base64::Engine;
use bitcoin::{
    absolute::LockTime, consensus, psbt::Psbt, transaction::Version, Amount, ScriptBuf, Sequence,
    Transaction, TxIn, TxOut, Witness,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
const MAX_SATS: u64 = 2_100_000_000_000_000;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Offer {
    pub binding: Binding,
    pub source: RetainedFile,
    pub network: ChainNetwork,
    /// This version accepts only a native P2WPKH recipient (22 script bytes).
    pub recipient_script_type: String,
}
impl Offer {
    pub fn validate(&self) -> Result<()> {
        self.binding.validate()?;
        self.source.validate()?;
        anyhow::ensure!(
            (546..=MAX_SATS).contains(&self.binding.price_sats)
                && self.recipient_script_type == "p2wpkh",
            "Unsupported original on-chain offer"
        );
        Ok(())
    }
    pub fn hash(&self) -> Result<String> {
        self.validate()?;
        Ok(hex::encode(Sha256::digest(serde_json::to_vec(self)?)))
    }
    pub fn check_quote(&self, quote: &Quote) -> Result<()> {
        self.validate()?;
        anyhow::ensure!(
            quote.binding == self.binding
                && quote.source == self.source
                && quote.network == self.network
                && quote.script()?.is_p2wpkh(),
            "Allocated address changed the original offer"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanInput {
    pub lease: Lease,
    pub previous_tx_hex: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FundingPlan {
    pub offer_sha256: String,
    pub inputs: Vec<PlanInput>,
    pub change_address: String,
    pub change_script: String,
    pub change_sats: u64,
    pub fee_sats: u64,
    pub fee_rate_sat_vbyte: u64,
    pub max_fee_sats: u64,
}
impl FundingPlan {
    pub fn hash(&self) -> Result<String> {
        Ok(hex::encode(Sha256::digest(serde_json::to_vec(self)?)))
    }
    pub fn validate(&self, record: &Record) -> Result<()> {
        let offer = record.offer.as_ref().context("Original offer missing")?;
        offer.validate()?;
        anyhow::ensure!(
            self.offer_sha256 == offer.hash()? && offer.binding == record.binding,
            "Funding plan belongs to a different offer"
        );
        let change = self
            .change_address
            .parse::<bitcoin::Address<bitcoin::address::NetworkUnchecked>>()?
            .require_network(offer.network.bitcoin())?
            .script_pubkey();
        anyhow::ensure!(
            hex::encode(change.as_bytes()) == self.change_script
                && (change.is_p2wpkh() || change.is_p2tr()),
            "Invalid original change script"
        );
        anyhow::ensure!(
            matches!(&record.change_address,Some(crate::content_onchain::ChangeAddress::Ready{address}) if address==&self.change_address),
            "Funding plan change allocation changed"
        );
        anyhow::ensure!(
            !self.inputs.is_empty()
                && self.inputs.len() <= 32
                && self.max_fee_sats > 0
                && self.max_fee_sats <= MAX_SATS
                && (1..=5000).contains(&self.fee_rate_sat_vbyte),
            "Invalid funding plan limits"
        );
        anyhow::ensure!(
            self.change_sats == 0 || self.change_sats >= 546,
            "Dust change is unsupported"
        );
        let mut seen = std::collections::HashSet::new();
        let mut total = 0u64;
        for input in &self.inputs {
            input.lease.validate(&record.lock_id)?;
            anyhow::ensure!(
                input.lease.expires_at == 0 && seen.insert(input.lease.outpoint()?),
                "Invalid or duplicate planned input"
            );
            anyhow::ensure!(
                input.previous_tx_hex.len() <= 2 * 1024 * 1024,
                "Previous transaction too large"
            );
            let previous: Transaction =
                consensus::deserialize(&hex::decode(&input.previous_tx_hex)?)?;
            anyhow::ensure!(
                previous.compute_txid().to_string() == input.lease.txid,
                "Original input transaction changed"
            );
            let output = previous
                .output
                .get(input.lease.vout as usize)
                .context("Original input index missing")?;
            anyhow::ensure!(
                output.value.to_sat() == input.lease.value_sats
                    && hex::encode(output.script_pubkey.as_bytes()) == input.lease.script,
                "Original input metadata changed"
            );
            total = total
                .checked_add(input.lease.value_sats)
                .filter(|v| *v <= MAX_SATS)
                .context("Input sum overflow")?;
        }
        let debit = offer
            .binding
            .price_sats
            .checked_add(self.change_sats)
            .and_then(|v| v.checked_add(self.fee_sats))
            .context("Payment amount overflow")?;
        anyhow::ensure!(
            total == debit && self.fee_sats > 0 && self.fee_sats <= self.max_fee_sats,
            "Funding plan fee or outputs changed"
        );
        let leases: Vec<_> = self.inputs.iter().map(|i| i.lease.clone()).collect();
        let minimum = self
            .fee_rate_sat_vbyte
            .checked_mul(max_vsize(
                &leases,
                if self.change_sats == 0 {
                    None
                } else {
                    Some(&change)
                },
            )?)
            .context("Fee estimate overflow")?;
        anyhow::ensure!(
            self.fee_sats >= minimum,
            "Funding plan fee does not cover reviewed rate"
        );
        Ok(())
    }
    /// Only now, with the real original seller address, construct a PSBT.
    pub fn bind(&self, record: &Record, quote: &Quote) -> Result<(FeePolicy, Funded)> {
        self.validate(record)?;
        record.offer.as_ref().unwrap().check_quote(quote)?;
        let recipient = quote.script()?;
        let change = ScriptBuf::from_bytes(hex::decode(&self.change_script)?);
        anyhow::ensure!(recipient != change, "Recipient cannot equal local change");
        let mut outputs = vec![TxOut {
            value: Amount::from_sat(record.binding.price_sats),
            script_pubkey: recipient,
        }];
        if self.change_sats > 0 {
            outputs.push(TxOut {
                value: Amount::from_sat(self.change_sats),
                script_pubkey: change,
            });
        }
        let tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: self
                .inputs
                .iter()
                .map(|i| {
                    Ok(TxIn {
                        previous_output: i.lease.outpoint()?,
                        script_sig: ScriptBuf::new(),
                        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                        witness: Witness::new(),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            output: outputs,
        };
        let max_rate = self.fee_sats.div_ceil(tx.vsize() as u64);
        let mut psbt = Psbt::from_unsigned_tx(tx)?;
        for (metadata, input) in psbt.inputs.iter_mut().zip(&self.inputs) {
            metadata.witness_utxo = Some(TxOut {
                value: Amount::from_sat(input.lease.value_sats),
                script_pubkey: ScriptBuf::from_bytes(hex::decode(&input.lease.script)?),
            });
            metadata.non_witness_utxo = Some(consensus::deserialize(&hex::decode(
                &input.previous_tx_hex,
            )?)?);
        }
        Ok((
            FeePolicy {
                change_script: self.change_script.clone(),
                max_fee_sats: self.max_fee_sats,
                max_fee_rate_sat_vbyte: max_rate,
            },
            Funded {
                psbt_base64: base64::engine::general_purpose::STANDARD.encode(psbt.serialize()),
                leases: self.inputs.iter().map(|i| i.lease.clone()).collect(),
            },
        ))
    }
}
/// Weight calculation only: never constructs a placeholder-address transaction.
/// Versions, sequence and locktime are fixed by this protocol; <=32 inputs/2 outputs
/// mean single-byte CompactSize counts. Native inputs have empty scriptSig.
pub(crate) fn max_vsize(inputs: &[Lease], change: Option<&ScriptBuf>) -> Result<u64> {
    anyhow::ensure!(
        !inputs.is_empty() && inputs.len() <= 32,
        "Unsupported input count"
    );
    let mut stripped = 4 + 1 + 41 * (inputs.len() as u64) + 1 + 8 + 1 + 22 + 4;
    if let Some(script) = change {
        anyhow::ensure!(script.len() < 253, "Unsupported change script length");
        stripped += 8 + 1 + script.len() as u64;
    }
    let mut witness = 2u64;
    for input in inputs {
        let script = ScriptBuf::from_bytes(hex::decode(&input.script)?);
        witness += if script.is_p2wpkh() {
            109
        } else if script.is_p2tr() {
            67
        } else {
            anyhow::bail!("Unsupported signing input")
        };
    }
    Ok((stripped * 4 + witness).div_ceil(4))
}
