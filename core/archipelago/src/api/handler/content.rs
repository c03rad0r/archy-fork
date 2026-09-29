use super::build_response;
use crate::config::Config;
use crate::content_server;
use anyhow::Result;
use hyper::{Response, StatusCode};

use super::{is_valid_app_id, ApiHandler};

impl ApiHandler {
    pub(super) async fn handle_content_catalog(config: &Config) -> Result<Response<hyper::Body>> {
        match content_server::load_catalog(&config.data_dir).await {
            Ok(catalog) => {
                // Only expose public metadata for available items
                let items: Vec<serde_json::Value> = catalog
                    .items
                    .iter()
                    .filter(|i| !matches!(i.availability, content_server::Availability::Nobody))
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
            .map(|s| s.to_string())
            .or_else(|| {
                headers
                    .get("x-onchain-address")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string())
            });

        // Extract federation peer DID from X-Federation-DID header
        let peer_did = headers
            .get("x-federation-did")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // The authenticated local operator never pays for their own node's
        // content: validate the session cookie (same discipline as the model
        // proxy — re-derived here, never trusted to the front door) and hand
        // serve_content the owner bypass. No cookie / bad session is simply
        // the buyer path, unchanged.
        let owner_session = match crate::session::extract_session_cookie(headers) {
            Some(token) => self.session_store.validate(&token).await,
            None => false,
        };

        // Parse Range header for streaming support
        let range = headers
            .get("range")
            .and_then(|v| v.to_str().ok())
            .and_then(content_server::parse_range_header);

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
                    r#"{"error":"The seller's node can't read this file right now. No payment was taken."}"#,
                ),
            )),
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
    pub(super) async fn handle_content_invoice(&self, path: &str) -> Result<Response<hyper::Body>> {
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

        let memo = format!("Archipelago peer file {content_id}");
        match self
            .rpc_handler
            .create_invoice(price_sats as i64, &memo)
            .await
        {
            Ok((bolt11, payment_hash)) if !payment_hash.is_empty() => {
                crate::content_invoice::record_pending(&payment_hash, content_id, price_sats).await;
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
        let rest = path.strip_prefix("/content/").unwrap_or("");
        let (content_id, payment_hash) = match rest.split_once("/invoice-status/") {
            Some((id, hash)) => (id, hash),
            None => {
                return Ok(build_response(
                    StatusCode::BAD_REQUEST,
                    "text/plain",
                    hyper::Body::from("Invalid request"),
                ))
            }
        };
        if content_id.is_empty() || !is_valid_app_id(content_id) || payment_hash.is_empty() {
            return Ok(build_response(
                StatusCode::BAD_REQUEST,
                "text/plain",
                hyper::Body::from("Invalid request"),
            ));
        }

        // The hash must be one we issued for exactly this content item.
        match crate::content_invoice::lookup(payment_hash).await {
            Some((cid, _)) if cid == content_id => {}
            _ => {
                return Ok(build_response(
                    StatusCode::NOT_FOUND,
                    "application/json",
                    hyper::Body::from(r#"{"error":"Unknown invoice"}"#),
                ))
            }
        }

        // Already paid? Otherwise ask our LND and persist the result.
        let mut paid = crate::content_invoice::is_paid_for(payment_hash, content_id).await;
        if !paid {
            if let Ok(true) = self.rpc_handler.invoice_is_settled(payment_hash).await {
                crate::content_invoice::mark_paid(payment_hash).await;
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

    /// Seller side (#46): issue a fresh on-chain address for a paid catalog item
    /// so a buyer can pay on-chain. Path: GET /content/{id}/onchain. Records a
    /// pending entitlement keyed by the address; price doubles as expected amount.
    pub(super) async fn handle_content_onchain(&self, path: &str) -> Result<Response<hyper::Body>> {
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
        let catalog = content_server::load_catalog(&self.config.data_dir)
            .await
            .unwrap_or_default();
        let price_sats = match catalog.items.iter().find(|i| i.id == content_id) {
            Some(i) => match &i.access {
                content_server::AccessControl::Paid { price_sats, .. } => {
                    if !content_server::method_accepted(&i.access, "onchain") {
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
            },
            None => {
                return Ok(build_response(
                    StatusCode::NOT_FOUND,
                    "text/plain",
                    hyper::Body::from("Content not found"),
                ))
            }
        };

        match self.rpc_handler.new_onchain_address().await {
            Ok(address) if !address.is_empty() => {
                crate::content_invoice::record_pending(&address, content_id, price_sats).await;
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
        let price = match crate::content_invoice::lookup(address).await {
            Some((cid, price)) if cid == content_id => price,
            _ => {
                return Ok(build_response(
                    StatusCode::NOT_FOUND,
                    "application/json",
                    hyper::Body::from(r#"{"error":"Unknown address"}"#),
                ))
            }
        };

        let mut paid = crate::content_invoice::is_paid_for(address, content_id).await;
        if !paid {
            if let Ok(true) = self.rpc_handler.onchain_received(address, price).await {
                crate::content_invoice::mark_paid(address).await;
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
