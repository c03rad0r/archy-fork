//! Quote the actual outgoing proof shape; never assume every exact-send proof
//! belongs to the current active keyset. The caller supplies mint-verified fees.
use super::cashu::{matches_stored_keyset_id, CashuToken, KeysetInfo, Proof};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeysetPlan {
    pub keyset_id: String,
    pub denominations: Vec<u64>,
    pub input_fee_ppk: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FeePlan {
    pub mint_url: String,
    pub keysets: Vec<KeysetPlan>,
    pub gross_sats: u64,
    pub fee_sats: u64,
    pub net_sats: u64,
}
impl FeePlan {
    pub fn from_shape(mint_url: &str, mut keysets: Vec<KeysetPlan>) -> Result<Self> {
        let parsed = reqwest::Url::parse(mint_url)?;
        anyhow::ensure!(
            matches!(parsed.scheme(), "http" | "https")
                && parsed.host_str().is_some()
                && parsed.username().is_empty()
                && parsed.password().is_none()
                && parsed.query().is_none()
                && parsed.fragment().is_none()
                && parsed.to_string().trim_end_matches('/') == mint_url,
            "Invalid canonical payment mint"
        );
        anyhow::ensure!(
            !keysets.is_empty() && keysets.len() <= 64,
            "Invalid payment keyset plan"
        );
        keysets.sort_by(|a, b| a.keyset_id.cmp(&b.keyset_id));
        let mut identifiers = HashSet::new();
        let mut count = 0usize;
        let mut gross = 0u64;
        let mut fee_ppk = 0u64;
        for group in &mut keysets {
            anyhow::ensure!(
                matches!(group.keyset_id.len(), 16 | 66)
                    && hex::decode(&group.keyset_id).is_ok()
                    && group.keyset_id == group.keyset_id.to_ascii_lowercase()
                    && identifiers.insert(group.keyset_id.clone()),
                "Invalid or duplicate payment keyset"
            );
            // Full v2 identities are required in a plan; compact wire IDs only
            // match an already known unambiguous group at validation time.
            anyhow::ensure!(
                group.keyset_id.len() != 16 || !group.keyset_id.starts_with("01"),
                "Payment plan requires full v2 keyset identity"
            );
            anyhow::ensure!(!group.denominations.is_empty(), "Empty payment proof group");
            group.denominations.sort_unstable();
            count = count
                .checked_add(group.denominations.len())
                .context("Proof count overflow")?;
            anyhow::ensure!(count <= 1024, "Too many outgoing payment proofs");
            for amount in &group.denominations {
                anyhow::ensure!(amount.is_power_of_two(), "Invalid payment denomination");
                gross = gross
                    .checked_add(*amount)
                    .context("Payment amount overflow")?;
                fee_ppk = fee_ppk
                    .checked_add(group.input_fee_ppk)
                    .context("Payment fee overflow")?;
            }
        }
        let fee = fee_ppk.div_ceil(1000);
        let net = gross
            .checked_sub(fee)
            .context("Payment fees exceed outgoing value")?;
        anyhow::ensure!(net > 0, "Payment has no net value");
        Ok(Self {
            mint_url: mint_url.into(),
            keysets,
            gross_sats: gross,
            fee_sats: fee,
            net_sats: net,
        })
    }
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            &Self::from_shape(&self.mint_url, self.keysets.clone())? == self,
            "Payment fee plan is inconsistent or noncanonical"
        );
        Ok(())
    }
    pub fn commitment(&self) -> Result<String> {
        self.validate()?;
        let groups: Vec<_> = self
            .keysets
            .iter()
            .map(|group| {
                serde_json::json!([group.keyset_id, group.input_fee_ppk, group.denominations])
            })
            .collect();
        Ok(hex::encode(Sha256::digest(serde_json::to_vec(
            &serde_json::json!([
                "content-payment-fee-plan-v1",
                self.mint_url,
                self.gross_sats,
                self.fee_sats,
                self.net_sats,
                groups
            ]),
        )?)))
    }
    /// Check authoritative mint metadata at acceptance, and again immediately
    /// before a fresh swap POST. Inactive old proofs may still be exact-spent;
    /// a prepared output keyset must remain active for a new issuance.
    pub fn verify_mint_keysets(
        &self,
        keysets: &[KeysetInfo],
        require_active_output: bool,
    ) -> Result<()> {
        self.validate()?;
        for group in &self.keysets {
            let mut matching = keysets
                .iter()
                .filter(|keyset| keyset.id.eq_ignore_ascii_case(&group.keyset_id));
            let keyset = matching
                .next()
                .context("Quoted payment keyset is missing")?;
            anyhow::ensure!(
                matching.next().is_none()
                    && keyset.unit == "sat"
                    && keyset.input_fee_ppk == group.input_fee_ppk
                    && (!require_active_output || keyset.active),
                "Quoted payment keyset or fees changed"
            );
        }
        Ok(())
    }
    pub fn validate_proofs(&self, proofs: &[Proof]) -> Result<()> {
        self.validate()?;
        let mut actual: BTreeMap<String, Vec<u64>> = BTreeMap::new();
        let mut secrets = HashSet::new();
        for proof in proofs {
            anyhow::ensure!(
                secrets.insert(proof.secret.as_str()),
                "Duplicate outgoing proof"
            );
            let mut groups = self
                .keysets
                .iter()
                .filter(|group| matches_stored_keyset_id(&proof.id, &group.keyset_id));
            let group = groups.next().context("Outgoing proof keyset changed")?;
            anyhow::ensure!(groups.next().is_none(), "Ambiguous compact outgoing keyset");
            actual
                .entry(group.keyset_id.clone())
                .or_default()
                .push(proof.amount);
        }
        for amounts in actual.values_mut() {
            amounts.sort_unstable();
        }
        let expected: BTreeMap<_, _> = self
            .keysets
            .iter()
            .map(|group| (group.keyset_id.clone(), group.denominations.clone()))
            .collect();
        anyhow::ensure!(
            actual == expected,
            "Outgoing proof count or denominations changed"
        );
        Ok(())
    }
    pub fn validate_token(&self, encoded: &str) -> Result<()> {
        let token = CashuToken::deserialize(encoded)?;
        anyhow::ensure!(
            token.token.len() == 1 && token.token[0].mint.trim_end_matches('/') == self.mint_url,
            "Outgoing payment mint changed"
        );
        self.validate_proofs(&token.token[0].proofs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn group(id: &str, denominations: Vec<u64>, fee: u64) -> KeysetPlan {
        KeysetPlan {
            keyset_id: id.into(),
            denominations,
            input_fee_ppk: fee,
        }
    }
    fn proof(id: &str, amount: u64, secret: &str) -> Proof {
        Proof {
            id: id.into(),
            amount,
            secret: secret.into(),
            c: "unused-in-shape-validation".into(),
        }
    }
    #[test]
    fn mixed_keysets_quote_actual_proof_counts_and_round_fee_once() {
        let first = "0011223344556677";
        let second = "008899aabbccddee";
        let plan = FeePlan::from_shape(
            "https://mint.invalid",
            vec![group(first, vec![4, 4], 400), group(second, vec![2], 100)],
        )
        .unwrap();
        assert_eq!((plan.gross_sats, plan.fee_sats, plan.net_sats), (10, 1, 9));
        let proofs = vec![
            proof(first, 4, "a"),
            proof(first, 4, "b"),
            proof(second, 2, "c"),
        ];
        plan.validate_proofs(&proofs).unwrap();
        assert!(plan
            .validate_proofs(&[proof(first, 8, "a"), proof(second, 2, "c")])
            .is_err());
        assert!(plan
            .validate_proofs(&[
                proof(first, 4, "a"),
                proof(first, 4, "a"),
                proof(second, 2, "c")
            ])
            .is_err());
        let mut changed = plan.clone();
        changed.keysets[0].input_fee_ppk += 1000;
        assert!(changed.validate().is_err());
    }
    #[test]
    fn compact_collision_and_changed_fee_quote_reject_before_token_transfer() {
        let first = format!("01{}", "11".repeat(32));
        let mut second = first.clone();
        second.replace_range(64..66, "22");
        let plan = FeePlan::from_shape(
            "https://mint.invalid",
            vec![group(&first, vec![4], 0), group(&second, vec![4], 0)],
        )
        .unwrap();
        plan.validate_proofs(&[proof(&first, 4, "a"), proof(&second, 4, "b")])
            .unwrap();
        assert!(plan
            .validate_proofs(&[proof(&first[..16], 4, "a"), proof(&second, 4, "b")])
            .is_err());
        let mut keysets: Vec<_> = [&first, &second]
            .into_iter()
            .map(|id| KeysetInfo {
                id: id.clone(),
                unit: "sat".into(),
                active: true,
                input_fee_ppk: 0,
            })
            .collect();
        plan.verify_mint_keysets(&keysets, true).unwrap();
        keysets[0].active = false;
        assert!(plan.verify_mint_keysets(&keysets, true).is_err());
        plan.verify_mint_keysets(&keysets, false).unwrap();
        keysets[0].input_fee_ppk = 1;
        assert!(plan.verify_mint_keysets(&keysets, false).is_err());
    }
    #[test]
    fn fee_commitment_matches_shared_ordered_array_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/purchase-fee-plan-v1.json"
        ))
        .unwrap();
        let plan: FeePlan = serde_json::from_value(fixture["plan"].clone()).unwrap();
        assert_eq!(
            plan.commitment().unwrap(),
            fixture["sha256"].as_str().unwrap()
        );
        assert_eq!(
            hex::encode(Sha256::digest(
                fixture["preimage"].as_str().unwrap().as_bytes()
            )),
            fixture["sha256"].as_str().unwrap()
        );
        let mut reversed = plan.keysets.clone();
        reversed.reverse();
        assert_eq!(
            FeePlan::from_shape(&plan.mint_url, reversed)
                .unwrap()
                .commitment()
                .unwrap(),
            plan.commitment().unwrap()
        );
    }
}
