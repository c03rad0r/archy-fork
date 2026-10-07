//! Owner-only original-operation on-chain flow. No generic sendcoins fallback.
use super::RpcHandler;
use crate::{
    content_lightning::Binding,
    content_onchain::{self as engine, Journal, Phase, Record},
};
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    onion: String,
    content_id: String,
    operation_id: Option<String>,
    price_sats: Option<u64>,
    max_fee_sats: Option<u64>,
    sat_per_vbyte: Option<u64>,
    template_sha256: Option<String>,
    plan_sha256: Option<String>,
}
fn public(record: &Record) -> Result<Value> {
    let fee = record
        .template
        .as_ref()
        .map(|t| engine::validate_funded(record, t))
        .transpose()?;
    let template_sha256 = record
        .template
        .as_ref()
        .map(|t| hex::encode(Sha256::digest(t.psbt_base64.as_bytes())));
    Ok(
        json!({"operation_id":record.binding.id,"price_sats":record.binding.price_sats,"phase":record.phase,
        "network":record.network(),"external_exposure":record.externally_exposed,
        "address":if record.externally_exposed {record.quote.as_ref().map(|q|q.address.as_str())}else{None},
        "fee_sats":fee.or_else(|| record.plan.as_ref().map(|p|p.fee_sats)),
        "max_fee_sats":record.policy.as_ref().map(|p|p.max_fee_sats).or_else(||record.plan.as_ref().map(|p|p.max_fee_sats)),
        "template_sha256":template_sha256,"plan_sha256":record.plan.as_ref().map(|p|p.hash()).transpose()?,
        "txid":record.signed.as_ref().map(|s|s.txid.as_str()),"paid":record.settled,
        "change_allocation_ambiguous":matches!(record.change_address,Some(engine::ChangeAddress::Dispatched)),
        "can_switch_method":record.retirement.is_some(),"retired_unallocated":record.retirement.is_some()}),
    )
}
impl RpcHandler {
    async fn request_onchain_allocation(
        &self,
        record: &Record,
        fips: &str,
    ) -> Result<crate::content_onchain_seller::Record> {
        let operation = crate::api::handler::onchain_purchase::Operation {
            binding: record.binding.clone(),
            action: "allocate".into(),
        };
        let (mut response, _) = crate::fips::dial::PeerRequest::new(
            Some(fips),
            &record.seller_onion,
            crate::api::handler::onchain_purchase::ROUTE,
        )
        .require_fips()
        .single_delivery()
        .timeout(std::time::Duration::from_secs(45))
        .send_content_json(
            &self.config.data_dir,
            &record.binding.seller_did,
            &operation,
        )
        .await
        .context("Original seller allocation reply unavailable; recover the same operation")?;
        anyhow::ensure!(
            response.status().is_success(),
            "Original seller allocation remains unresolved"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(
                bytes.len() + chunk.len() <= 16384,
                "Seller response too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        let saved: crate::content_onchain_seller::Record = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
            saved.binding == record.binding,
            "Seller changed original purchase"
        );
        if let Some(offer) = &record.offer {
            anyhow::ensure!(saved.offer()? == *offer, "Seller changed original offer");
        }
        Ok(saved)
    }
    pub(super) async fn ensure_onchain_allows_other_rail(
        &self,
        buyer: &str,
        seller: &str,
        content: &str,
    ) -> Result<()> {
        anyhow::ensure!(
            Journal::find_for(&self.config.data_dir, buyer, seller, content)?.is_none(),
            "An original on-chain purchase remains recoverable; do not pay again or switch methods"
        );
        Ok(())
    }
    pub(super) async fn handle_onchain_operation(
        &self,
        params: Option<Value>,
        action: &str,
    ) -> Result<Value> {
        let params: Params = serde_json::from_value(params.context("Missing on-chain operation")?)?;
        anyhow::ensure!(
            !params.content_id.starts_with("registered_"),
            "Registered rentals require their native purchase contract"
        );
        let peer =
            crate::federation::load_unique_payment_peer(&self.config.data_dir, &params.onion)
                .await?;
        let buyer =
            crate::identity::NodeIdentity::load_existing(&self.config.data_dir.join("identity"))
                .await?
                .did_key()?;
        anyhow::ensure!(buyer != peer.did, "Cannot buy from this same node");
        let _admission = crate::content_payment_admission::lock(
            &self.config.data_dir,
            &buyer,
            &peer.did,
            &params.content_id,
        )
        .await?;
        let original = if let Some(id) = &params.operation_id {
            let journal = Journal::open(&self.config.data_dir, id).await?;
            let original = journal.load()?;
            if let Some(record) = &original {
                anyhow::ensure!(
                    record.binding.buyer_did == buyer
                        && record.binding.seller_did == peer.did
                        && record.binding.content_id == params.content_id,
                    "Original on-chain operation belongs to another purchase"
                );
            }
            original
        } else {
            Journal::find_for(&self.config.data_dir, &buyer, &peer.did, &params.content_id)?
        };
        if action == "lookup" {
            return Ok(json!({"attempt":original.as_ref().map(public).transpose()?}));
        }
        if let Some(id) = &params.operation_id {
            anyhow::ensure!(
                original.as_ref().is_some_and(|r| &r.binding.id == id),
                "Original on-chain operation changed"
            );
        }
        let mut record = if let Some(record) = original {
            record
        } else {
            anyhow::ensure!(
                matches!(action, "create" | "expose") && params.operation_id.is_none(),
                "Recover original on-chain operation first"
            );
            self.ensure_invoice_allows_other_rail(&buyer, &peer.did, &params.content_id)
                .await?;
            let cashu = crate::content_purchase::Journal::open(&self.config.data_dir).await?;
            anyhow::ensure!(
                cashu
                    .find_buyers(&buyer, &peer.did, &params.content_id)
                    .await?
                    .iter()
                    .all(|r| r.phase == crate::content_purchase::BuyerPhase::Cancelled),
                "Recover or cancel original Cashu purchase first"
            );
            Record::new(
                Binding {
                    id: uuid::Uuid::new_v4().to_string(),
                    buyer_did: buyer,
                    seller_did: peer.did.clone(),
                    content_id: params.content_id.clone(),
                    price_sats: params.price_sats.context("Expected price required")?,
                },
                params.onion.clone(),
            )?
        };
        anyhow::ensure!(
            record.seller_onion == params.onion
                && params
                    .price_sats
                    .is_none_or(|p| p == record.binding.price_sats),
            "Original payment address or price changed"
        );
        let journal = Journal::open(&self.config.data_dir, &record.binding.id).await?;
        if journal.load()?.is_none() {
            journal.save(&record)?;
        }
        if record.retirement.is_some() {
            return public(&record);
        }
        if action == "cancel" {
            anyhow::ensure!(params.operation_id.is_some() && record.can_retire_unallocated(),"An allocated or mutated on-chain purchase cannot be canceled; recover its original payment");
        }
        if matches!(action, "create" | "expose" | "prepare" | "pay") && !record.settled {
            // Recheck while the same admission guard is held, including resumes
            // from another window and records predating this owner flow.
            self.ensure_invoice_allows_other_rail(
                &record.binding.buyer_did,
                &peer.did,
                &params.content_id,
            )
            .await?;
            let cashu = crate::content_purchase::Journal::open(&self.config.data_dir).await?;
            anyhow::ensure!(
                cashu
                    .find_buyers(&record.binding.buyer_did, &peer.did, &params.content_id)
                    .await?
                    .iter()
                    .all(|r| r.phase == crate::content_purchase::BuyerPhase::Cancelled),
                "Another saved Cashu liability must be recovered before on-chain dispatch"
            );
        }
        if action == "prepare" {
            anyhow::ensure!(
                params.operation_id.is_some(),
                "Original operation ID required"
            );
            if record.template.is_none() && record.plan.is_none() {
                let max_fee_sats = params
                    .max_fee_sats
                    .context("Explicit maximum fee required")?;
                anyhow::ensure!(
                    (1..=2_100_000_000_000_000).contains(&max_fee_sats),
                    "Invalid maximum fee"
                );
                if let Some(rate) = params.sat_per_vbyte {
                    anyhow::ensure!((1..=5000).contains(&rate), "Invalid fee rate");
                }
                let wallet = self.onchain_purchase_wallet().await?;
                let change = wallet.prepare_change(&journal).await?;
                record = wallet
                    .prepare_plan(
                        &journal,
                        super::lnd::onchain_purchase::PlanRequest {
                            change_address: change,
                            max_fee_sats,
                            sat_per_vbyte: params.sat_per_vbyte,
                        },
                    )
                    .await?;
            }
            return public(&record);
        }
        if action == "pay" {
            anyhow::ensure!(
                params.operation_id.is_some(),
                "Original operation ID required"
            );
            if record.settled {
                return public(&record);
            }
            if let Some(plan) = &record.plan {
                anyhow::ensure!(
                    params.plan_sha256.as_deref() == Some(plan.hash()?.as_str()),
                    "Confirm the original saved funding plan before payment"
                );
            } else {
                let template = record
                    .template
                    .as_ref()
                    .context("Review the original fee first")?;
                anyhow::ensure!(
                    params.template_sha256.as_deref()
                        == Some(
                            hex::encode(Sha256::digest(template.psbt_base64.as_bytes())).as_str()
                        ),
                    "Confirm the original saved transaction before payment"
                );
            }
            let wallet = self.onchain_purchase_wallet().await?;
            if record.plan.is_some() && record.quote.is_none() {
                record = engine::lease_plan(&journal, &wallet).await?;
                engine::mark_address_allocation(&journal, false)?;
                let status = self
                    .request_onchain_allocation(
                        &record,
                        peer.fips_npub
                            .as_deref()
                            .context("Seller has no authenticated mesh connection")?,
                    )
                    .await?;
                let quote = status
                    .quote()?
                    .context("Original seller allocation is unresolved; recover this operation")?;
                record = engine::accept_quote(&journal, quote)?;
            }
            if record.plan.is_some() && record.template.is_none() {
                record = engine::bind_plan(&journal)?;
            }
            if matches!(
                record.phase,
                Phase::TemplatePrepared | Phase::LeaseDispatched
            ) {
                record = engine::drive(&journal, &wallet, engine::Action::Lease, None).await?;
            }
            if matches!(record.phase, Phase::Funded | Phase::SigningDispatched) {
                record = engine::drive(&journal, &wallet, engine::Action::Sign, None).await?;
            }
            if matches!(
                record.phase,
                Phase::Signed | Phase::BroadcastDispatched | Phase::Published
            ) {
                record = engine::drive(&journal, &wallet, engine::Action::Publish, None).await?;
            }
            return public(&record);
        }
        anyhow::ensure!(
            matches!(
                action,
                "create" | "status" | "expose" | "download" | "cancel"
            ),
            "Unsupported on-chain action"
        );
        let fips = peer
            .fips_npub
            .context("Seller has no authenticated mesh connection")?;
        if action == "expose" && record.offer.is_some() && record.quote.is_none() {
            engine::mark_address_allocation(&journal, true)?;
            let status = self.request_onchain_allocation(&record, &fips).await?;
            record = engine::accept_quote(
                &journal,
                status
                    .quote()?
                    .context("Original seller allocation is unresolved; recover this operation")?,
            )?;
        }
        let remote_action = if action == "create" || action == "expose" && record.offer.is_none() {
            "offer"
        } else if action == "expose" {
            "status"
        } else {
            action
        };
        let operation = crate::api::handler::onchain_purchase::Operation {
            binding: record.binding.clone(),
            action: remote_action.into(),
        };
        let route = crate::api::handler::onchain_purchase::ROUTE;
        let remote = crate::fips::dial::PeerRequest::new(Some(&fips), &params.onion, route)
            .require_fips()
            .single_delivery()
            .timeout(std::time::Duration::from_secs(if action == "download" {
                900
            } else {
                45
            }))
            .send_content_json(&self.config.data_dir, &peer.did, &operation)
            .await;
        let (mut response, _) = match remote {
            Ok(value) => value,
            Err(_) => {
                return Ok(
                    json!({"attempt":public(&record)?,"recovery_required":true,"error":"Original on-chain request is saved. Recover this operation; do not request another address or pay again."}),
                )
            }
        };
        anyhow::ensure!(
            response.status().is_success(),
            "Seller could not recover original on-chain purchase {}; retain it",
            record.binding.id
        );
        if action == "download" {
            let source = record
                .quote
                .as_ref()
                .context("Recover original address first")?
                .source
                .clone();
            anyhow::ensure!(
                response.content_length() == Some(source.size),
                "Original file length changed"
            );
            record.settled = true;
            journal.save(&record)?;
            let stream = crate::content_purchase_download::verified_stream(
                response.bytes_stream(),
                source.sha256,
                source.size,
            );
            let owned = crate::content_owned::record_purchase_stream(
                &self.config.data_dir,
                crate::content_owned::OwnedItem {
                    onion: params.onion,
                    content_id: params.content_id,
                    filename: source.filename,
                    mime_type: source.mime_type,
                    size_bytes: source.size,
                    paid_sats: record.binding.price_sats,
                    ecash_backend: "onchain".into(),
                    purchased_at: chrono::Utc::now().to_rfc3339(),
                    download_complete: false,
                },
                Box::pin(stream),
                Some(source.size),
            )
            .await?;
            return Ok(
                json!({"owned":true,"owned_content_id":owned.content_id,"mime_type":owned.mime_type}),
            );
        }
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(
                bytes.len() + chunk.len() <= 16384,
                "On-chain response too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        let body: Value = serde_json::from_slice(&bytes)?;
        if body["state"] == "cancelled_unallocated" {
            let ack: crate::content_onchain_seller::UnallocatedAck = serde_json::from_value(body)?;
            record = engine::retire_unallocated(&journal, ack)?;
            return public(&record);
        }
        anyhow::ensure!(
            action != "cancel",
            "Seller did not acknowledge unallocated retirement; preserve original operation"
        );
        let status: crate::content_onchain_seller::Record = serde_json::from_value(body)?;
        anyhow::ensure!(
            status.binding == record.binding,
            "Seller changed original purchase"
        );
        if record.offer.is_none() && record.quote.is_none() {
            record = engine::accept_offer(&journal, status.offer()?)?;
        }
        if let Some(quote) = status.quote()? {
            record = engine::accept_quote(&journal, quote)?;
        }
        anyhow::ensure!(
            !status.paid || record.quote.is_some(),
            "Paid purchase lacks original address"
        );
        record.settled |= status.paid;
        journal.save(&record)?;
        if action == "expose" && !record.settled {
            if record.quote.is_none() {
                engine::mark_address_allocation(&journal, true)?;
                let status = self.request_onchain_allocation(&record, &fips).await?;
                record = engine::accept_quote(
                    &journal,
                    status.quote()?.context(
                        "Original seller allocation is unresolved; recover this operation",
                    )?,
                )?;
            }
            engine::expose_address(&journal)?;
            record = journal.load()?.context("Original record unavailable")?;
        }
        public(&record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding() -> Binding {
        Binding {
            id: uuid::Uuid::new_v4().to_string(),
            buyer_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([7; 32])).unwrap(),
            seller_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([8; 32])).unwrap(),
            content_id: "file".into(),
            price_sats: 546,
        }
    }
    #[tokio::test]
    async fn unresolved_onchain_operation_blocks_cashu_and_every_lightning_spend_entry() {
        let data = tempfile::tempdir().unwrap();
        let identity = crate::identity::NodeIdentity::load_or_create(&data.path().join("identity"))
            .await
            .unwrap();
        let mut binding = binding();
        binding.buyer_did = identity.did_key().unwrap();
        let onion = format!("{}.onion", "a".repeat(56));
        let peer = serde_json::from_value(json!({
            "did": binding.seller_did, "pubkey": hex::encode([8; 32]),
            "onion": onion, "trust_level": "trusted", "added_at": "now",
            "fips_npub": "fixture-no-network"
        }))
        .unwrap();
        crate::federation::save_nodes(data.path(), &[peer])
            .await
            .unwrap();
        let mut config = crate::config::Config::default();
        config.data_dir = data.path().to_path_buf();
        let handler = RpcHandler::new(
            config,
            std::sync::Arc::new(crate::state::StateManager::new()),
            std::sync::Arc::new(crate::monitoring::MetricsStore::new()),
            crate::session::SessionStore::new_for_tests(data.path().join("sessions.json")),
            None,
            None,
        )
        .await
        .unwrap();
        let journal = Journal::open(data.path(), &binding.id).await.unwrap();
        journal
            .save(&Record::new(binding.clone(), onion.clone()).unwrap())
            .unwrap();
        drop(journal);
        let original = std::fs::read(
            data.path()
                .join("content-onchain")
                .join(format!("{}.json", binding.id)),
        )
        .unwrap();

        // Exercise the real RPC entry points, not just the admission helper.
        // Each must reject before a Cashu/Lightning journal, offer, invoice or
        // wallet dispatch is created, including delayed callbacks and retries.
        for consent in [
            None,
            Some(json!({
                "operation_id": uuid::Uuid::new_v4().to_string(),
                "envelope_sha256": "ab".repeat(32), "wallet_debit_sats": 546
            })),
        ] {
            let error = handler
                .handle_content_purchase(Some(json!({
                    "onion": onion, "content_id": binding.content_id,
                    "max_wallet_debit": 546, "consent": consent
                })))
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains("original on-chain purchase"),
                "{error:#}"
            );
        }
        for (action, exposure) in [
            ("create", false),
            ("pay", false),
            ("retry", false),
            ("create", true),
            ("status", true),
        ] {
            let error = handler
                .handle_lightning_operation(
                    Some(json!({
                        "onion": onion, "content_id": binding.content_id,
                        "price_sats": 546, "external_exposure": exposure
                    })),
                    action,
                )
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains("original on-chain purchase"),
                "{action}: {error:#}"
            );
        }
        // Read-only recovery lookup remains available despite the blocked rail.
        let lookup = handler
            .handle_lightning_operation(
                Some(json!({
                    "onion": onion, "content_id": binding.content_id
                })),
                "lookup",
            )
            .await
            .unwrap();
        assert!(lookup["attempt"].is_null());
        assert_eq!(
            std::fs::read(
                data.path()
                    .join("content-onchain")
                    .join(format!("{}.json", binding.id))
            )
            .unwrap(),
            original
        );
        assert!(!data.path().join("wallet").exists());
        // No replacement operation has been persisted by any rejected call.
        assert!(crate::content_purchase::Journal::open(data.path())
            .await
            .unwrap()
            .find_buyers(&binding.buyer_did, &binding.seller_did, &binding.content_id)
            .await
            .unwrap()
            .is_empty());
        assert!(crate::content_lightning::Journal::open(data.path())
            .await
            .unwrap()
            .buyer_for(&binding.buyer_did, &binding.seller_did, &binding.content_id)
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn buyer_discovery_retains_unresolved_address_and_rejects_duplicate_operations() {
        let data = tempfile::tempdir().unwrap();
        let binding = binding();
        let saved = Record::new(binding.clone(), format!("{}.onion", "a".repeat(56))).unwrap();
        let j = Journal::open(data.path(), &binding.id).await.unwrap();
        j.save(&saved).unwrap();
        drop(j);
        let found = Journal::find_for(
            data.path(),
            &binding.buyer_did,
            &binding.seller_did,
            &binding.content_id,
        )
        .unwrap()
        .unwrap();
        assert_eq!(found.binding.id, binding.id);
        assert!(found.blocks_other_rails());
        assert!(public(&found).unwrap()["address"].is_null());
        assert!(Journal::find_for(
            data.path(),
            &binding.seller_did,
            &binding.buyer_did,
            &binding.content_id
        )
        .unwrap()
        .is_none());
        let mut second = binding.clone();
        second.id = uuid::Uuid::new_v4().to_string();
        let j = Journal::open(data.path(), &second.id).await.unwrap();
        j.save(&Record::new(second, saved.seller_onion).unwrap())
            .unwrap();
        drop(j);
        assert!(Journal::find_for(
            data.path(),
            &binding.buyer_did,
            &binding.seller_did,
            &binding.content_id
        )
        .is_err());
    }
    #[tokio::test]
    async fn corrupted_node_record_cannot_be_treated_as_permission_to_pay_again() {
        let data = tempfile::tempdir().unwrap();
        let binding = binding();
        let j = Journal::open(data.path(), &binding.id).await.unwrap();
        j.save(&Record::new(binding.clone(), format!("{}.onion", "a".repeat(56))).unwrap())
            .unwrap();
        drop(j);
        std::fs::write(
            data.path()
                .join("content-onchain")
                .join(format!("{}.json", binding.id)),
            b"{}",
        )
        .unwrap();
        assert!(Journal::find_for(
            data.path(),
            &binding.buyer_did,
            &binding.seller_did,
            &binding.content_id
        )
        .is_err());
    }
    #[tokio::test]
    async fn only_durable_matching_empty_ack_releases_cross_rail_and_stale_callback_cannot_revive()
    {
        let data = tempfile::tempdir().unwrap();
        let binding = binding();
        let _rail = crate::content_payment_admission::lock(
            data.path(),
            &binding.buyer_did,
            &binding.seller_did,
            &binding.content_id,
        )
        .await
        .unwrap();
        let journal = Journal::open(data.path(), &binding.id).await.unwrap();
        let original = Record::new(binding.clone(), format!("{}.onion", "a".repeat(56))).unwrap();
        journal.save(&original).unwrap();
        let ack = crate::content_onchain_seller::UnallocatedAck {
            binding: binding.clone(),
            state: "cancelled_unallocated".into(),
            address: serde_json::Value::Null,
            allocation_dispatched: false,
            can_switch_method: true,
        };
        for wrong in [
            crate::content_onchain_seller::UnallocatedAck {
                allocation_dispatched: true,
                ..ack.clone()
            },
            crate::content_onchain_seller::UnallocatedAck {
                address: serde_json::json!("not-empty"),
                ..ack.clone()
            },
            crate::content_onchain_seller::UnallocatedAck {
                binding: Binding {
                    id: uuid::Uuid::new_v4().to_string(),
                    ..binding.clone()
                },
                ..ack.clone()
            },
        ] {
            assert!(engine::retire_unallocated(&journal, wrong).is_err());
        }
        assert!(Journal::find_for(
            data.path(),
            &binding.buyer_did,
            &binding.seller_did,
            &binding.content_id
        )
        .unwrap()
        .is_some());
        let retired = engine::retire_unallocated(&journal, ack).unwrap();
        assert!(!retired.blocks_other_rails());
        assert!(public(&retired).unwrap()["can_switch_method"] == true);
        assert!(journal.save(&original).is_err());
        drop(journal);
        assert!(Journal::find_for(
            data.path(),
            &binding.buyer_did,
            &binding.seller_did,
            &binding.content_id
        )
        .unwrap()
        .is_none());
        let mut replacement = binding.clone();
        replacement.id = uuid::Uuid::new_v4().to_string();
        let journal = Journal::open(data.path(), &replacement.id).await.unwrap();
        journal
            .save(&Record::new(replacement.clone(), original.seller_onion).unwrap())
            .unwrap();
        drop(journal);
        assert_eq!(
            Journal::find_for(
                data.path(),
                &binding.buyer_did,
                &binding.seller_did,
                &binding.content_id
            )
            .unwrap()
            .unwrap()
            .binding
            .id,
            replacement.id
        );
    }
    #[tokio::test]
    async fn owner_address_is_redacted_until_exposure_is_durable_and_cannot_then_be_retired() {
        let data = tempfile::tempdir().unwrap();
        let binding = binding();
        let journal = Journal::open(data.path(), &binding.id).await.unwrap();
        journal
            .save(&Record::new(binding.clone(), format!("{}.onion", "a".repeat(56))).unwrap())
            .unwrap();
        let mut bytes = vec![0, 20];
        bytes.extend([1; 20]);
        let address = bitcoin::Address::from_script(
            &bitcoin::ScriptBuf::from_bytes(bytes),
            bitcoin::Network::Regtest,
        )
        .unwrap()
        .to_string();
        let saved = engine::accept_quote(
            &journal,
            engine::Quote {
                binding: binding.clone(),
                address: address.clone(),
                network: engine::ChainNetwork::Regtest,
                source: crate::content_lightning::RetainedFile {
                    sha256: "a".repeat(64),
                    size: 4,
                    filename: "original.txt".into(),
                    mime_type: "text/plain".into(),
                },
            },
        )
        .unwrap();
        assert!(public(&saved).unwrap()["address"].is_null());
        let ack = crate::content_onchain_seller::UnallocatedAck {
            binding: binding.clone(),
            state: "cancelled_unallocated".into(),
            address: serde_json::Value::Null,
            allocation_dispatched: false,
            can_switch_method: true,
        };
        assert!(engine::retire_unallocated(&journal, ack.clone()).is_err());
        assert_eq!(engine::expose_address(&journal).unwrap(), address);
        drop(journal);
        let journal = Journal::open(data.path(), &binding.id).await.unwrap();
        let exposed = journal.load().unwrap().unwrap();
        assert!(exposed.externally_exposed);
        assert_eq!(public(&exposed).unwrap()["address"], address);
        assert!(engine::retire_unallocated(&journal, ack).is_err());
        assert!(exposed.blocks_other_rails());
    }
}
