use super::RpcHandler;
use crate::{
    api::handler::lightning_purchase::{Operation, ROUTE},
    content_lightning::{Binding, BuyerRecord, Journal, Phase, Status},
};
use anyhow::{Context, Result};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    onion: String,
    content_id: String,
    price_sats: Option<u64>,
    operation_id: Option<String>,
    #[serde(default)]
    external_exposure: bool,
}
impl RpcHandler {
    pub(super) async fn handle_lightning_operation(
        &self,
        params: Option<serde_json::Value>,
        action: &str,
    ) -> Result<serde_json::Value> {
        let params: Params = serde_json::from_value(params.context("Missing invoice operation")?)?;
        let peer =
            crate::federation::load_unique_payment_peer(&self.config.data_dir, &params.onion)
                .await?;
        let fips = peer
            .fips_npub
            .context("Seller has no authenticated mesh connection")?;
        let buyer =
            crate::identity::NodeIdentity::load_existing(&self.config.data_dir.join("identity"))
                .await?
                .did_key()?;
        anyhow::ensure!(buyer != peer.did, "Cannot buy a file from this same node");
        let _admission = crate::content_payment_admission::lock(
            &self.config.data_dir,
            &buyer,
            &peer.did,
            &params.content_id,
        )
        .await?;
        if matches!(action, "create" | "pay" | "retry") || params.external_exposure {
            self.ensure_onchain_allows_other_rail(&buyer, &peer.did, &params.content_id)
                .await?;
            let cashu = crate::content_purchase::Journal::open(&self.config.data_dir).await?;
            anyhow::ensure!(
                cashu
                    .find_buyers(&buyer, &peer.did, &params.content_id)
                    .await?
                    .iter()
                    .all(|r| r.phase == crate::content_purchase::BuyerPhase::Cancelled),
                "Recover or cancel the original Cashu purchase before exposing a Lightning invoice"
            );
        }
        let journal = Journal::open(&self.config.data_dir).await?;
        let original = if let Some(id) = &params.operation_id {
            journal.buyer(id)?
        } else {
            journal.buyer_for(&buyer, &peer.did, &params.content_id)?
        };
        if let Some(record) = &original {
            anyhow::ensure!(
                record.binding.buyer_did == buyer
                    && record.binding.seller_did == peer.did
                    && record.binding.content_id == params.content_id
                    && record.seller_onion == params.onion,
                "Original invoice belongs to another purchase"
            );
        }
        if action == "lookup" {
            return Ok(match original {
                None => serde_json::json!({"attempt":null}),
                Some(mut record) => {
                    let mut native_result = record.native_result.clone();
                    if native_result.is_none() && record.native_dispatched {
                        if let Some(status) = &record.last {
                            if let Ok(payment) = self
                                .handle_lnd_paymentstatus(Some(
                                    serde_json::json!({"payment_hash":status.payment_hash}),
                                ))
                                .await
                            {
                                if let Some(result @ ("failed" | "succeeded")) =
                                    payment["status"].as_str()
                                {
                                    native_result = Some(result.to_owned());
                                    record.native_result = native_result.clone();
                                    journal.save_buyer(&record)?;
                                }
                            }
                        }
                    }
                    let native_failed =
                        !record.external_exposure && native_result.as_deref() == Some("failed");
                    let native_succeeded = native_result.as_deref() == Some("succeeded");
                    if !record.external_exposure {
                        if let Some(status) = record.last.as_mut() {
                            status.bolt11 = None;
                        }
                    }
                    serde_json::json!({"attempt":{"operation_id":record.binding.id,"price_sats":record.binding.price_sats,"external_exposure":record.external_exposure,"native_failed":native_failed,"native_succeeded":native_succeeded,"status":record.last}})
                }
            });
        }
        let mut record = if let Some(record) = original {
            record
        } else {
            anyhow::ensure!(
                action == "create" && params.operation_id.is_none(),
                "Original invoice operation is unavailable"
            );
            BuyerRecord {
                binding: Binding {
                    id: uuid::Uuid::new_v4().to_string(),
                    buyer_did: buyer.clone(),
                    seller_did: peer.did.clone(),
                    content_id: params.content_id.clone(),
                    price_sats: params
                        .price_sats
                        .context("Expected invoice price is required")?,
                },
                seller_onion: params.onion.clone(),
                external_exposure: false,
                native_retired: false,
                native_replacement: None,
                native_dispatched: false,
                native_result: None,
                last: None,
            }
        };
        anyhow::ensure!(
            record.binding.buyer_did == buyer
                && record.binding.seller_did == peer.did
                && record.binding.content_id == params.content_id
                && record.seller_onion == params.onion
                && params
                    .operation_id
                    .as_ref()
                    .is_none_or(|id| id == &record.binding.id),
            "Original invoice operation changed"
        );
        if action == "retry" {
            anyhow::ensure!(
                params.operation_id.is_some()
                    && !params.external_exposure
                    && params.price_sats == Some(record.binding.price_sats),
                "Explicit original native retry and original price required"
            );
            if record.native_result.is_none()
                && record.native_dispatched
                && !record.external_exposure
            {
                let hash = &record
                    .last
                    .as_ref()
                    .context("Original invoice metadata missing")?
                    .payment_hash;
                let payment = self
                    .handle_lnd_paymentstatus(Some(serde_json::json!({"payment_hash":hash})))
                    .await?;
                if matches!(payment["status"].as_str(), Some("failed" | "succeeded")) {
                    record.native_result = payment["status"].as_str().map(str::to_owned);
                    journal.save_buyer(&record)?;
                }
            }
            record = journal.retry_native(&record.binding.id)?;
        }
        anyhow::ensure!((!record.native_retired || matches!(action,"status"|"cancel"|"download")) && (!record.native_retired || !params.external_exposure),"This native invoice was retired before changing payment method; recover the replacement purchase");
        if action == "pay" {
            anyhow::ensure!(
                params.operation_id.is_some(),
                "Original invoice operation required for native payment"
            );
            return crate::content_lightning::drive_native(
                &self.config.data_dir,
                journal,
                &record.binding.id,
                &super::lnd::external_invoice::NativeNode(self),
            )
            .await;
        }
        record.external_exposure |= params.external_exposure;
        journal.save_buyer(&record)?;
        drop(journal);
        // Native-only local FAILED never cancels an externally exposed invoice.
        // The seller terminal state is authoritative regardless of UI receipt loss.
        let operation = Operation {
            binding: record.binding.clone(),
            action: if action == "retry" {
                "create".into()
            } else {
                action.into()
            },
        };
        let response = crate::fips::dial::PeerRequest::new(Some(&fips), &params.onion, ROUTE)
            .require_fips()
            .single_delivery()
            .timeout(std::time::Duration::from_secs(if action == "download" {
                900
            } else {
                45
            }))
            .send_content_json(&self.config.data_dir, &peer.did, &operation)
            .await;
        let mut response = match response {
            Ok((r, _)) => r,
            Err(_) => {
                return Ok(
                    serde_json::json!({"state":"unknown","operation_id":record.binding.id,"recovery_required":true,"error":"The original invoice request is saved on this node. Recover it; no replacement invoice was requested."}),
                )
            }
        };
        anyhow::ensure!(
            response.status().is_success(),
            "Seller could not resolve original invoice; recover operation {}",
            record.binding.id
        );
        let journal = Journal::open(&self.config.data_dir).await?;
        if action == "download" {
            let mut paid = record
                .last
                .clone()
                .context("Original invoice metadata missing; recover it first")?;
            let source = paid
                .source
                .clone()
                .context("Original invoice snapshot missing")?;
            anyhow::ensure!(
                response.content_length() == Some(source.size),
                "Original invoice file length changed"
            );
            paid.state = Phase::Settled;
            paid.can_switch_method = false;
            record.last = Some(paid);
            journal.save_buyer(&record)?;
            drop(journal);
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
                    ecash_backend: "lightning".into(),
                    purchased_at: chrono::Utc::now().to_rfc3339(),
                    download_complete: false,
                },
                Box::pin(stream),
                Some(source.size),
            )
            .await?;
            return Ok(
                serde_json::json!({"owned":true,"owned_content_id":owned.content_id,"mime_type":owned.mime_type}),
            );
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(
                bytes.len() + chunk.len() <= 16384,
                "Invoice response too large"
            );
            bytes.extend_from_slice(&chunk)
        }
        let status: Status = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
            status.binding == record.binding
                && status.source.is_some()
                && status.payment_hash.len() == 64
                && status.payment_hash.bytes().all(|b| b.is_ascii_hexdigit())
                && status.can_switch_method == (status.state == Phase::CanceledUnpaid),
            "Seller invoice binding changed"
        );
        if let Some(bolt11) = &status.bolt11 {
            let invoice: lightning_invoice::Bolt11Invoice =
                bolt11.parse().context("Seller invoice is invalid")?;
            invoice.check_signature()?;
            anyhow::ensure!(
                invoice.payment_hash().to_string() == status.payment_hash
                    && invoice.amount_milli_satoshis()
                        == record.binding.price_sats.checked_mul(1000),
                "Invoice hash or amount differs from saved purchase"
            );
        }
        if let Some(previous) = &record.last {
            anyhow::ensure!(
                previous.payment_hash == status.payment_hash
                    && previous.source == status.source
                    && previous
                        .bolt11
                        .as_ref()
                        .is_none_or(|v| status.bolt11.as_ref() == Some(v)),
                "Original invoice replaced"
            );
        }
        record.last = Some(status.clone());
        journal.save_buyer(&record)?;
        Ok(
            serde_json::json!({"operation_id":record.binding.id,"price_sats":record.binding.price_sats,"payment_hash":status.payment_hash,"bolt11":if record.external_exposure{status.bolt11}else{None},"state":match status.state{Phase::Settled=>"settled",Phase::CanceledUnpaid=>"canceled",Phase::Issued=>"open",Phase::Prepared=>"prepared",Phase::Dispatched=>"unknown",Phase::CancelRequested=>"cancel_requested"},"paid":status.state==Phase::Settled,"can_switch_method":status.can_switch_method,"cancel_supported":true,"external_exposure":record.external_exposure}),
        )
    }
}

impl RpcHandler {
    /// Caller holds content_payment_admission before entering any rail journal.
    pub(super) async fn ensure_invoice_allows_other_rail(
        &self,
        buyer: &str,
        seller: &str,
        content: &str,
    ) -> Result<()> {
        let journal = Journal::open(&self.config.data_dir).await?;
        if let Some(mut record) = journal.buyer_for(buyer, seller, content)? {
            anyhow::ensure!(record.native_replacement.is_none(),
                "An explicit native retry is being recovered; recover its replacement operation first");
            anyhow::ensure!(
                !record.external_exposure,
                "An externally payable invoice remains unresolved; cancel or recover it first"
            );
            let status = record
                .last
                .clone()
                .context("Original invoice creation is unresolved; recover it first")?;
            anyhow::ensure!(
                status.state != Phase::Settled,
                "Original Lightning purchase is paid; recover its file"
            );
            // A local terminal failure can release only a never-exposed native
            // attempt. This check runs under the same outer lock as QR exposure.
            anyhow::ensure!(
                record.native_dispatched,
                "Original invoice has not been canceled; cancel it before replacing the method"
            );
            if record.native_result.as_deref() != Some("failed") {
                let payment = self
                    .handle_lnd_paymentstatus(Some(
                        serde_json::json!({"payment_hash":status.payment_hash}),
                    ))
                    .await?;
                anyhow::ensure!(
                    payment["status"] == "failed",
                    "Original native Lightning attempt remains unresolved"
                );
            }
            record.native_result = Some("failed".into());
            record.native_retired = true;
            journal.save_buyer(&record)?;
        }
        Ok(())
    }
}
