use super::build_response;
use crate::config::Config;
use crate::content_server;
use anyhow::Result;
use hyper::{Response, StatusCode};

use super::{is_valid_app_id, ApiHandler};

impl ApiHandler {
    fn verified_content_peer(
        &self,
        path: &str,
        headers: &hyper::HeaderMap,
    ) -> Result<Option<String>> {
        let audience = crate::identity::did_key_from_pubkey_hex(&self.self_pubkey_hex)?;
        crate::content_auth::incoming(headers, &audience, path, chrono::Utc::now().timestamp())
    }

    async fn content_access_context(
        &self,
        path: &str,
        headers: &hyper::HeaderMap,
    ) -> Result<(Option<String>, bool, bool)> {
        let peer = self.verified_content_peer(path, headers)?;
        let known = if let Some(did) = &peer {
            crate::federation::load_nodes(&self.config.data_dir)
                .await?
                .iter()
                .any(|node| &node.did == did)
        } else {
            false
        };
        let owner = match crate::session::extract_session_cookie(headers) {
            Some(token) => self.session_store.validate(&token).await,
            None => false,
        };
        Ok((peer, known, owner))
    }

    pub(super) async fn handle_content_catalog(
        &self,
        headers: &hyper::HeaderMap,
    ) -> Result<Response<hyper::Body>> {
        let (peer, known, owner) = self.content_access_context("/content", headers).await?;
        match content_server::load_catalog(&self.config.data_dir).await {
            Ok(catalog) => {
                // Only expose public metadata for available items
                let items: Vec<serde_json::Value> = catalog
                    .items
                    .iter()
                    .filter(|item| content_server::visible_to(item, peer.as_deref(), known, owner))
                    .map(|i| {
                        serde_json::json!({
                            "id": i.id,
                            "filename": i.filename,
                            "mime_type": i.mime_type,
                            "size_bytes": i.size_bytes,
                            "description": i.description,
                            "access": i.access,
                        })
                    })
                    .collect();
                let body =
                    serde_json::to_vec(&serde_json::json!({ "items": items })).unwrap_or_default();
                Ok(build_response(
                    StatusCode::OK,
                    "application/json",
                    hyper::Body::from(body),
                ))
            }
            Err(e) => {
                let body = serde_json::json!({ "error": e.to_string() });
                let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
                Ok(build_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "application/json",
                    hyper::Body::from(body_bytes),
                ))
            }
        }
    }

    pub(super) async fn handle_content_request(
        &self,
        path: &str,
        headers: &hyper::HeaderMap,
        config: &Config,
    ) -> Result<Response<hyper::Body>> {
        let content_id = path.strip_prefix("/content/").unwrap_or("");
        if content_id.is_empty() || !is_valid_app_id(content_id) {
            return Ok(build_response(
                StatusCode::BAD_REQUEST,
                "text/plain",
                hyper::Body::from("Invalid content ID"),
            ));
        }

        // Extract payment token from X-Payment-Token header
        let payment_token = headers
            .get("x-payment-token")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Extract a paid-entitlement gate token from X-Invoice-Hash (Lightning)
        // or X-Onchain-Address (on-chain) — both authorize the download if this
        // node issued+settled them, and both resolve against the same shared
        // entitlement store keyed by the token string (#46).
        let invoice_hash = headers
            .get("x-invoice-hash")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_ascii_lowercase())
            .or_else(|| {
                headers
                    .get("x-onchain-address")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string())
            });

        let peer_did = match self.verified_content_peer(path, headers) {
            Ok(peer) => peer,
            Err(_) => {
                return Ok(build_response(
                    StatusCode::FORBIDDEN,
                    "application/json",
                    hyper::Body::from(
                        r#"{"error":"Peer authentication failed. Check both nodes are updated and their clocks are correct."}"#,
                    ),
                ))
            }
        };

        // The authenticated local operator never pays for their own node's
        // content: validate the session cookie (same discipline as the model
        // proxy — re-derived here, never trusted to the front door) and hand
        // serve_content the owner bypass. No cookie / bad session is simply
        // the buyer path, unchanged.
        let owner_session = match crate::session::extract_session_cookie(headers) {
            Some(token) => self.session_store.validate(&token).await,
            None => false,
        };

        // Payment settlement is verified on the seller even when no status
        // poll preceded this download (e.g. direct payment from another node).
        let requires_payment = if !owner_session && headers.contains_key("x-invoice-hash") {
            content_server::load_catalog(&config.data_dir)
                .await?
                .items
                .iter()
                .any(|item| {
                    item.id == content_id
                        && matches!(item.access, content_server::AccessControl::Paid { .. })
                })
        } else {
            false
        };
        if requires_payment {
            if let Some(hash) = headers.get("x-invoice-hash").and_then(|v| v.to_str().ok()) {
                if hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return Ok(build_response(
                        StatusCode::BAD_REQUEST,
                        "text/plain",
                        hyper::Body::from("Invalid payment hash"),
                    ));
                }
                if let Err(error) = self
                    .rpc_handler
                    .settle_content_invoice(hash, content_id)
                    .await
                {
                    tracing::warn!("Cannot verify peer-file invoice settlement: {error:#}");
                    return Ok(build_response(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "application/json",
                        hyper::Body::from(
                            r#"{"error":"Payment verification is temporarily unavailable. Retry the download without paying again."}"#,
                        ),
                    ));
                }
            }
        }

        // Parse Range header for streaming support
        let range = match headers.get("range") {
            None => None,
            Some(value) => match value
                .to_str()
                .ok()
                .and_then(content_server::parse_range_header)
            {
                Some(range) => Some(range),
                None => {
                    return Ok(build_response(
                        StatusCode::BAD_REQUEST,
                        "text/plain",
                        hyper::Body::from("Invalid byte range"),
                    ))
                }
            },
        };

        match content_server::serve_content(
            &config.data_dir,
            content_id,
            payment_token.as_deref(),
            invoice_hash.as_deref(),
            peer_did.as_deref(),
            range,
            owner_session,
        )
        .await
        {
            Ok(content_server::ServeResult::Stream(body)) => body.into_response(),
            Ok(content_server::ServeResult::Ok(bytes, mime_type)) => {
                let len = bytes.len();
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", mime_type)
                    .header("Content-Length", len.to_string())
                    .header("Accept-Ranges", "bytes")
                    .body(hyper::Body::from(bytes))
                    .unwrap())
            }
            Ok(content_server::ServeResult::Partial {
                bytes,
                mime_type,
                start,
                end,
                total,
            }) => Ok(Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header("Content-Type", mime_type)
                .header("Content-Length", bytes.len().to_string())
                .header(
                    "Content-Range",
                    format!("bytes {}-{}/{}", start, end, total),
                )
                .header("Accept-Ranges", "bytes")
                .body(hyper::Body::from(bytes))
                .unwrap()),
            Ok(content_server::ServeResult::PaymentRequired(price_sats)) => {
                let body = serde_json::json!({
                    "error": "Payment required",
                    "price_sats": price_sats,
                    "payment_header": "X-Payment-Token",
                });
                let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
                Ok(build_response(
                    StatusCode::PAYMENT_REQUIRED,
                    "application/json",
                    hyper::Body::from(body_bytes),
                ))
            }
            Ok(content_server::ServeResult::Forbidden) => Ok(build_response(
                StatusCode::FORBIDDEN,
                "application/json",
                hyper::Body::from(
                    r#"{"error":"This file is shared with the host's federation peers only. Federate with that node (exchange invites) so it recognizes you, then try again."}"#,
                ),
            )),
            Ok(content_server::ServeResult::Unavailable) => Ok(build_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "application/json",
                hyper::Body::from(
                    r#"{"error":"The seller's node can't read this file right now. This request did not redeem an ecash payment."}"#,
                ),
            )),
            Ok(content_server::ServeResult::RangeNotSatisfiable(total)) => Ok(Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header("Content-Range", format!("bytes */{total}"))
                .body(hyper::Body::empty())
                .unwrap()),
            Ok(content_server::ServeResult::NotFound) => Ok(build_response(
                StatusCode::NOT_FOUND,
                "text/plain",
                hyper::Body::from("Content not found"),
            )),
            // Not a 404: a paid request may already have been charged by the
            // time this fails, and "not found" hid the real error entirely.
            Err(e) => {
                tracing::error!("Serving content {content_id} failed: {e:#}");
                Ok(build_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "text/plain",
                    hyper::Body::from("Failed to serve content"),
                ))
            }
        }
    }

    /// Seller side (#46): mint a Lightning invoice for a paid catalog item so a
    /// buyer can pay from any external wallet. Path: GET /content/{id}/invoice.
    /// Records a pending entitlement keyed by the invoice's payment hash.
    pub(super) async fn handle_content_invoice(
        &self,
        path: &str,
        headers: &hyper::HeaderMap,
    ) -> Result<Response<hyper::Body>> {
        let content_id = path
            .strip_prefix("/content/")
            .and_then(|s| s.strip_suffix("/invoice"))
            .unwrap_or("");
        if content_id.is_empty() || !is_valid_app_id(content_id) {
            return Ok(build_response(
                StatusCode::BAD_REQUEST,
                "text/plain",
                hyper::Body::from("Invalid content ID"),
            ));
        }

        let (peer, known, owner) = self.content_access_context(path, headers).await?;
        let catalog = content_server::load_catalog(&self.config.data_dir)
            .await
            .unwrap_or_default();
        let item = match catalog.items.iter().find(|i| i.id == content_id) {
            Some(i) => i,
            None => {
                return Ok(build_response(
                    StatusCode::NOT_FOUND,
                    "text/plain",
                    hyper::Body::from("Content not found"),
                ))
            }
        };
        if !content_server::visible_to(item, peer.as_deref(), known, owner) {
            return Ok(build_response(
                StatusCode::NOT_FOUND,
                "text/plain",
                hyper::Body::from("Content not found"),
            ));
        }
        let price_sats = match &item.access {
            content_server::AccessControl::Paid { price_sats, .. } => *price_sats,
            _ => {
                // Not a paid item — no invoice to issue.
                return Ok(build_response(
                    StatusCode::BAD_REQUEST,
                    "application/json",
                    hyper::Body::from(r#"{"error":"Item is not paid"}"#),
                ));
            }
        };
        if !content_server::method_accepted(&item.access, "lightning") {
            return Ok(build_response(
                StatusCode::BAD_REQUEST,
                "application/json",
                hyper::Body::from(
                    r#"{"error":"The seller does not accept Lightning for this item"}"#,
                ),
            ));
        }

        if let Err(error) =
            content_server::ensure_payment_source_available(&self.config.data_dir, item).await
        {
            return Ok(build_response(
                StatusCode::CONFLICT,
                "application/json",
                hyper::Body::from(serde_json::to_vec(
                    &serde_json::json!({ "error": error.to_string(), "payment_started": false }),
                )?),
            ));
        }

        let memo = format!("Archipelago peer file {content_id}");
        match self
            .rpc_handler
            .create_invoice(price_sats as i64, &memo)
            .await
        {
            Ok((bolt11, payment_hash)) if !payment_hash.is_empty() => {
                crate::content_invoice::record_pending(
                    &self.config.data_dir,
                    &payment_hash,
                    content_id,
                    price_sats,
                )
                .await?;
                let body = serde_json::json!({
                    "bolt11": bolt11,
                    "payment_hash": payment_hash,
                    "price_sats": price_sats,
                });
                Ok(build_response(
                    StatusCode::OK,
                    "application/json",
                    hyper::Body::from(serde_json::to_vec(&body).unwrap_or_default()),
                ))
            }
            Ok(_) => Ok(build_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "application/json",
                hyper::Body::from(r#"{"error":"Invoice missing payment hash"}"#),
            )),
            Err(e) => {
                // Surface the FULL error chain ({:#}) — the generic top-level
                // message hid the real cause (e.g. the LND REST connection
                // failing), which made this 503 undiagnosable.
                tracing::warn!("content invoice creation failed: {e:#}");
                let body = serde_json::json!({
                    "error": format!("Could not create invoice: {e:#}")
                });
                Ok(build_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "application/json",
                    hyper::Body::from(serde_json::to_vec(&body).unwrap_or_default()),
                ))
            }
        }
    }

    /// Seller side (#46): report whether a previously-issued invoice has settled.
    /// Path: GET /content/{id}/invoice-status/{payment_hash}. On settlement the
    /// entitlement is marked paid so the buyer can then download the file.
    pub(super) async fn handle_content_invoice_status(
        &self,
        path: &str,
    ) -> Result<Response<hyper::Body>> {
        Ok(invoice_status_response(path, |hash, id| async move {
            self.rpc_handler.settle_content_invoice(&hash, &id).await
        })
        .await)
    }

    /// Seller side (#46): issue a fresh on-chain address for a paid catalog item
    /// so a buyer can pay on-chain. Path: GET /content/{id}/onchain. Records a
    /// pending entitlement keyed by the address; price doubles as expected amount.
    pub(super) async fn handle_content_onchain(
        &self,
        path: &str,
        headers: &hyper::HeaderMap,
    ) -> Result<Response<hyper::Body>> {
        let content_id = path
            .strip_prefix("/content/")
            .and_then(|s| s.strip_suffix("/onchain"))
            .unwrap_or("");
        if content_id.is_empty() || !is_valid_app_id(content_id) {
            return Ok(build_response(
                StatusCode::BAD_REQUEST,
                "text/plain",
                hyper::Body::from("Invalid content ID"),
            ));
        }
        let (peer, known, owner) = self.content_access_context(path, headers).await?;
        let catalog = content_server::load_catalog(&self.config.data_dir)
            .await
            .unwrap_or_default();
        let Some(item) = catalog.items.iter().find(|item| item.id == content_id) else {
            return Ok(build_response(
                StatusCode::NOT_FOUND,
                "text/plain",
                hyper::Body::from("Content not found"),
            ));
        };
        if !content_server::visible_to(item, peer.as_deref(), known, owner) {
            return Ok(build_response(
                StatusCode::NOT_FOUND,
                "text/plain",
                hyper::Body::from("Content not found"),
            ));
        }
        let price_sats = match &item.access {
            content_server::AccessControl::Paid { price_sats, .. } => {
                if !content_server::method_accepted(&item.access, "onchain") {
                    return Ok(build_response(
                        StatusCode::BAD_REQUEST,
                        "application/json",
                        hyper::Body::from(
                            r#"{"error":"The seller does not accept on-chain payment for this item"}"#,
                        ),
                    ));
                }
                *price_sats
            }
            _ => {
                return Ok(build_response(
                    StatusCode::BAD_REQUEST,
                    "application/json",
                    hyper::Body::from(r#"{"error":"Item is not paid"}"#),
                ))
            }
        };

        // Match the node wallet's existing sendcoins minimum before exposing a
        // payable address for an amount its own payment flow cannot broadcast.
        if let Err(error) = content_server::validate_onchain_payment_price(price_sats) {
            return Ok(build_response(
                StatusCode::BAD_REQUEST,
                "application/json",
                hyper::Body::from(serde_json::to_vec(
                    &serde_json::json!({ "error": error.to_string(), "payment_started": false }),
                )?),
            ));
        }

        if let Err(error) =
            content_server::ensure_payment_source_available(&self.config.data_dir, item).await
        {
            return Ok(build_response(
                StatusCode::CONFLICT,
                "application/json",
                hyper::Body::from(serde_json::to_vec(
                    &serde_json::json!({ "error": error.to_string(), "payment_started": false }),
                )?),
            ));
        }

        match self.rpc_handler.new_onchain_address().await {
            Ok(address) if !address.is_empty() => {
                crate::content_invoice::record_pending_method(
                    &self.config.data_dir,
                    &address,
                    content_id,
                    price_sats,
                    crate::content_invoice::PaymentMethod::Onchain,
                )
                .await?;
                let body = serde_json::json!({
                    "address": address,
                    "amount_sats": price_sats,
                });
                Ok(build_response(
                    StatusCode::OK,
                    "application/json",
                    hyper::Body::from(serde_json::to_vec(&body).unwrap_or_default()),
                ))
            }
            _ => {
                let body = serde_json::json!({
                    "error": "Could not generate an on-chain address (is the wallet ready?)"
                });
                Ok(build_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "application/json",
                    hyper::Body::from(serde_json::to_vec(&body).unwrap_or_default()),
                ))
            }
        }
    }

    /// Seller side (#46): report whether an on-chain payment to a previously-
    /// issued address has arrived (>= price, >= 1 conf). Path:
    /// GET /content/{id}/onchain-status/{address}. Marks the entitlement paid.
    pub(super) async fn handle_content_onchain_status(
        &self,
        path: &str,
    ) -> Result<Response<hyper::Body>> {
        let rest = path.strip_prefix("/content/").unwrap_or("");
        let (content_id, address) = match rest.split_once("/onchain-status/") {
            Some((id, addr)) => (id, addr),
            None => {
                return Ok(build_response(
                    StatusCode::BAD_REQUEST,
                    "text/plain",
                    hyper::Body::from("Invalid request"),
                ))
            }
        };
        if content_id.is_empty() || !is_valid_app_id(content_id) || address.is_empty() {
            return Ok(build_response(
                StatusCode::BAD_REQUEST,
                "text/plain",
                hyper::Body::from("Invalid request"),
            ));
        }
        // The address must be one we issued for exactly this content item.
        let price = match crate::content_invoice::lookup(&self.config.data_dir, address).await? {
            Some((cid, price)) if cid == content_id => price,
            _ => {
                return Ok(build_response(
                    StatusCode::NOT_FOUND,
                    "application/json",
                    hyper::Body::from(r#"{"error":"Unknown address"}"#),
                ))
            }
        };

        let mut paid =
            crate::content_invoice::is_paid_for(&self.config.data_dir, address, content_id).await;
        if !paid {
            if let Ok(true) = self.rpc_handler.onchain_received(address, price).await {
                crate::content_invoice::mark_paid(&self.config.data_dir, address).await?;
                paid = true;
            }
        }
        let body = serde_json::json!({ "paid": paid });
        Ok(build_response(
            StatusCode::OK,
            "application/json",
            hyper::Body::from(serde_json::to_vec(&body).unwrap_or_default()),
        ))
    }

    /// Serve a degraded preview of paid content (blurred image or first 2% of video).
    pub(super) async fn handle_content_preview(
        path: &str,
        config: &Config,
    ) -> Result<Response<hyper::Body>> {
        // Path format: /content/{id}/preview
        let content_id = path
            .strip_prefix("/content/")
            .and_then(|s| s.strip_suffix("/preview"))
            .unwrap_or("");

        if content_id.is_empty() || !is_valid_app_id(content_id) {
            return Ok(build_response(
                StatusCode::BAD_REQUEST,
                "text/plain",
                hyper::Body::from("Invalid content ID"),
            ));
        }

        match content_server::serve_content_preview(&config.data_dir, content_id).await {
            Ok(content_server::PreviewResult::Stream(body)) => body.into_response(),
            Ok(content_server::PreviewResult::FullContent(bytes, mime_type)) => {
                let len = bytes.len();
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", mime_type)
                    .header("Content-Length", len.to_string())
                    .body(hyper::Body::from(bytes))
                    .unwrap())
            }
            Ok(content_server::PreviewResult::BlurPreview(bytes, mime_type)) => {
                let len = bytes.len();
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", mime_type)
                    .header("Content-Length", len.to_string())
                    .header("X-Content-Preview", "blur")
                    .body(hyper::Body::from(bytes))
                    .unwrap())
            }
            Ok(content_server::PreviewResult::TruncatedPreview(bytes, mime_type, total_size)) => {
                let len = bytes.len();
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", mime_type)
                    .header("Content-Length", len.to_string())
                    .header("X-Content-Preview", "truncated")
                    .header("X-Content-Total-Size", total_size.to_string())
                    .body(hyper::Body::from(bytes))
                    .unwrap())
            }
            Ok(content_server::PreviewResult::PreviewUnavailable) => Ok(Response::builder()
                .status(StatusCode::UNSUPPORTED_MEDIA_TYPE)
                .header("Content-Type", "text/plain")
                .header("X-Content-Preview", "unavailable")
                .body(hyper::Body::from(
                    "Preview unavailable for this media (needs re-encoding)",
                ))
                .unwrap()),
            Ok(content_server::PreviewResult::NotFound) | Err(_) => Ok(build_response(
                StatusCode::NOT_FOUND,
                "text/plain",
                hyper::Body::from("Preview not available"),
            )),
        }
    }
}

/// Keep invalid input and an unavailable wallet inside the HTTP protocol so
/// buyers can retry delivery without treating a dropped socket as lost payment.
async fn invoice_status_response<F, Fut>(path: &str, settle: F) -> Response<hyper::Body>
where
    F: FnOnce(String, String) -> Fut,
    Fut: std::future::Future<Output = Result<bool>>,
{
    let parsed = path
        .strip_prefix("/content/")
        .and_then(|rest| rest.split_once("/invoice-status/"))
        .filter(|(id, hash)| {
            !id.is_empty()
                && is_valid_app_id(id)
                && hash.len() == 64
                && hash.bytes().all(|c| c.is_ascii_hexdigit())
        });
    let Some((id, hash)) = parsed else {
        return build_response(
            StatusCode::BAD_REQUEST,
            "application/json",
            hyper::Body::from(r#"{"error":"Invalid content ID or payment hash"}"#),
        );
    };
    match settle(hash.to_ascii_lowercase(), id.to_owned()).await {
        Ok(paid) => build_response(
            StatusCode::OK,
            "application/json",
            hyper::Body::from(serde_json::json!({"paid": paid}).to_string()),
        ),
        Err(_) => {
            tracing::warn!("Peer-file payment status verification is temporarily unavailable");
            let mut response = build_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "application/json",
                hyper::Body::from(
                    r#"{"error":"Payment verification is temporarily unavailable. Retry without paying again."}"#,
                ),
            );
            response.headers_mut().insert(
                hyper::header::RETRY_AFTER,
                hyper::header::HeaderValue::from_static("5"),
            );
            response
        }
    }
}

#[cfg(test)]
mod invoice_status_tests {
    use super::*;

    #[tokio::test]
    async fn malformed_requests_do_not_query_the_wallet() {
        for path in [
            "/bad",
            "/content//invoice-status/aa",
            "/content/file/invoice-status/aa",
            "/content/file/invoice-status/",
            "/content/file/invoice-status/not-a-hash",
        ] {
            let response = invoice_status_response(path, |_, _| async {
                panic!("Invalid request reached wallet");
                #[allow(unreachable_code)]
                Ok(false)
            })
            .await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert_eq!(response.headers()["content-type"], "application/json");
            let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
            assert!(
                serde_json::from_slice::<serde_json::Value>(&body).unwrap()["error"].is_string()
            );
        }
    }

    #[tokio::test]
    async fn settlement_results_and_failures_have_explicit_http_responses() {
        let hash = "AB".repeat(32);
        let path = format!("/content/file/invoice-status/{hash}");
        for paid in [false, true] {
            let response = invoice_status_response(&path, |hash, id| async move {
                assert_eq!(hash, "ab".repeat(32));
                assert_eq!(id, "file");
                Ok(paid)
            })
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&body).unwrap()["paid"],
                paid
            );
        }
        let response = invoice_status_response(&path, |_, _| async {
            anyhow::bail!("private wallet details must not escape")
        })
        .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()["retry-after"], "5");
        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("without paying again"));
        assert!(!text.contains("private wallet"));
    }
}
