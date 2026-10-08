//! Add as api/handler/purchase.rs; dispatch only exact supported POST routes.
use super::{build_response, ApiHandler};
use crate::{
    content_purchase::Journal, content_purchase_protocol as protocol, identity::NodeIdentity,
};
use anyhow::{Context, Result};
use hyper::{body::HttpBody, Body, Method, Request, Response, StatusCode};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OfferRequest {
    id: String,
    content_id: String,
}
impl ApiHandler {
    pub(super) async fn handle_purchase_request(
        &self,
        mut request: Request<Body>,
    ) -> Result<Response<Body>> {
        let path = request.uri().path().to_owned();
        anyhow::ensure!(
            request.method() == Method::POST
                && matches!(
                    path.as_str(),
                    protocol::PREPARE_OFFER_ROUTE
                        | protocol::OFFER_ROUTE
                        | protocol::ACCEPT_ROUTE
                        | protocol::SETTLE_ROUTE
                        | protocol::STATUS_ROUTE
                        | protocol::CANCEL_ROUTE
                ),
            "Unsupported purchase route"
        );
        let audience = crate::identity::did_key_from_pubkey_hex(&self.self_pubkey_hex)?;
        let bytes = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let mut bytes = Vec::new();
            while let Some(chunk) = request.body_mut().data().await {
                let chunk = chunk?;
                anyhow::ensure!(
                    bytes
                        .len()
                        .checked_add(chunk.len())
                        .is_some_and(|n| n <= 1024 * 1024),
                    "Purchase body too large"
                );
                bytes.extend_from_slice(&chunk);
            }
            Ok::<_, anyhow::Error>(bytes)
        })
        .await
        .context("Purchase body timed out")??;
        let buyer = crate::content_auth::authenticate_request(
            request.headers(),
            &audience,
            &Method::POST,
            &path,
            &bytes,
            chrono::Utc::now().timestamp(),
        )?;
        let data_dir = &self.config.data_dir;
        let result = match path.as_str() {
            protocol::PREPARE_OFFER_ROUTE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Prepare {
                    content_id: String,
                    #[serde(default)]
                    retry: bool,
                    expected: Option<crate::content_purchase_caller::ExpectedRental>,
                }
                let input: Prepare = serde_json::from_slice(&bytes)?;
                let identity = std::sync::Arc::new(
                    NodeIdentity::load_existing(&data_dir.join("identity")).await?,
                );
                anyhow::ensure!(identity.did_key()? == audience, "Node identity changed");
                let root = data_dir.clone();
                serde_json::to_value(
                    tokio::task::spawn_blocking(move || {
                        if let Some(expected) = &input.expected {
                            let (receipt, _) = crate::registered_media::registered_metadata(
                                &root,
                                &identity,
                                &input.content_id,
                            )?;
                            expected.verify_metadata(&identity.did_key()?, &receipt)?;
                        }
                        crate::registered_media::prepare_registered(
                            root,
                            identity,
                            &input.content_id,
                            input.retry,
                        )
                    })
                    .await??,
                )?
            }
            protocol::OFFER_ROUTE => {
                let body: OfferRequest = serde_json::from_slice(&bytes)?;
                anyhow::ensure!(
                    uuid::Uuid::parse_str(&body.id)?.to_string() == body.id,
                    "Invalid operation identifier"
                );
                let saved = {
                    Journal::open(data_dir)
                        .await?
                        .protocol_offer(&body.id)
                        .await?
                };
                let offer = if let Some(saved) = saved {
                    anyhow::ensure!(
                        saved.buyer_did == buyer && saved.content_id == body.content_id,
                        "Original offer binding changed"
                    );
                    saved
                } else {
                    // Registration pins and immutable snapshot are node-owned;
                    // no content hash/price/path is accepted from the request.
                    if !body.content_id.starts_with("registered_") {
                        let wallet = crate::wallet::ecash::load_wallet(data_dir).await?;
                        let offer = crate::content_cloud_offer::offer(
                            data_dir,
                            &body.id,
                            &body.content_id,
                            &buyer,
                            &audience,
                            crate::wallet::ecash::load_network(data_dir).await?,
                            wallet.mint_url.trim_end_matches('/'),
                            crate::content_cloud_offer::SnapshotPolicy {
                                max_file_bytes: 64 * 1024 * 1024 * 1024,
                                max_total_bytes: 64 * 1024 * 1024 * 1024,
                                minimum_free_bytes: 512 * 1024 * 1024,
                            },
                        )
                        .await?;
                        return Ok(build_response(
                            StatusCode::OK,
                            "application/json",
                            Body::from(serde_json::to_vec(&offer)?),
                        ));
                    }
                    let identity = NodeIdentity::load_existing(&data_dir.join("identity")).await?;
                    anyhow::ensure!(identity.did_key()? == audience, "Node identity changed");
                    let selected = body.content_id.clone();
                    let root = data_dir.clone();
                    let (receipt, terms) = tokio::task::spawn_blocking(move || {
                        crate::registered_media::registered_terms(&root, &identity, &selected)
                    })
                    .await??;
                    anyhow::ensure!(
                        receipt
                            .payment_methods
                            .iter()
                            .any(|method| method == "cashu"),
                        "Content does not accept Cashu"
                    );
                    let now = chrono::Utc::now().timestamp();
                    let deadline = now.checked_add(120).context("Offer clock overflow")?;
                    let wallet = crate::wallet::ecash::load_wallet(data_dir).await?;
                    let offer = protocol::Offer {
                        id: body.id,
                        buyer_did: buyer.clone(),
                        seller_did: audience.clone(),
                        filename: receipt.content_id.clone(),
                        mime_type: "application/octet-stream".into(),
                        content_id: receipt.content_id,
                        content_sha256: receipt.sha256,
                        content_size: receipt.size_bytes.parse()?,
                        viewing_seconds: Some(receipt.viewing_seconds),
                        terms_sha256: terms,
                        network: crate::wallet::ecash::load_network(data_dir).await?,
                        mint_url: wallet.mint_url.trim_end_matches('/').to_owned(),
                        seller_net_sats: receipt.price_sats,
                        offered_at: now,
                        expires_at: deadline,
                    };
                    protocol::save_offer(data_dir, &offer, &buyer, now).await?
                };
                protocol::ensure_seller_mint_policy(data_dir, offer.network, &offer.mint_url)
                    .await?;
                serde_json::to_value(offer)?
            }
            protocol::ACCEPT_ROUTE => serde_json::to_value(
                protocol::accept(data_dir, &serde_json::from_slice(&bytes)?, &buyer, || {
                    chrono::Utc::now().timestamp()
                })
                .await?,
            )?,
            protocol::SETTLE_ROUTE => serde_json::to_value(
                protocol::settle(data_dir, &serde_json::from_slice(&bytes)?, &buyer).await?,
            )?,
            protocol::CANCEL_ROUTE => serde_json::to_value(
                protocol::cancel(data_dir, &serde_json::from_slice(&bytes)?, &buyer).await?,
            )?,
            protocol::STATUS_ROUTE => serde_json::to_value(
                protocol::status(data_dir, &serde_json::from_slice(&bytes)?, &buyer).await?,
            )?,
            _ => unreachable!(),
        };
        Ok(build_response(
            StatusCode::OK,
            "application/json",
            Body::from(serde_json::to_vec(&result)?),
        ))
    }
}
