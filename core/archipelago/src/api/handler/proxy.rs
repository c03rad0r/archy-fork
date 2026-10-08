use super::build_response;
use crate::api::rpc::lnd::LND_REST_BASE_URL;
use crate::api::rpc::RpcHandler;
use crate::bitcoin_status;
use crate::electrs_status;
use anyhow::Result;
use hyper::{Response, StatusCode};
use std::sync::Arc;

use super::{is_valid_app_id, ApiHandler};

impl ApiHandler {
    pub(super) async fn handle_container_logs_http(
        rpc: Arc<RpcHandler>,
        path: &str,
        cors_origin: &str,
    ) -> Result<Response<hyper::Body>> {
        let query = path
            .strip_prefix("/api/container/logs")
            .and_then(|s| s.strip_prefix('?'))
            .unwrap_or("");
        let params: std::collections::HashMap<String, String> = query
            .split('&')
            .filter_map(|p| {
                let mut it = p.splitn(2, '=');
                let k = it.next()?.to_string();
                let v = it.next()?.to_string();
                Some((k, v))
            })
            .collect();

        let app_id = params.get("app_id").map(|s| s.as_str()).unwrap_or("lnd");

        // Validate app_id format
        if !is_valid_app_id(app_id) {
            let body = serde_json::json!({ "error": "Invalid app_id" });
            let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
            return Ok(build_response(
                StatusCode::BAD_REQUEST,
                "application/json",
                hyper::Body::from(body_bytes),
            ));
        }

        let lines = params
            .get("lines")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(200);

        match rpc.get_container_logs_value(app_id, lines).await {
            Ok(value) => {
                let body = serde_json::json!({ "result": value });
                let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .header("Access-Control-Allow-Origin", cors_origin)
                    .header("Access-Control-Allow-Credentials", "true")
                    .header("Vary", "Origin")
                    .body(hyper::Body::from(body_bytes))
                    .unwrap())
            }
            Err(e) => {
                let body = serde_json::json!({ "error": e.to_string() });
                let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
                Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "application/json")
                    .header("Access-Control-Allow-Origin", cors_origin)
                    .header("Access-Control-Allow-Credentials", "true")
                    .header("Vary", "Origin")
                    .body(hyper::Body::from(body_bytes))
                    .unwrap())
            }
        }
    }

    pub(super) async fn handle_electrs_status() -> Result<Response<hyper::Body>> {
        let status = electrs_status::get_electrs_sync_status().await;
        let body = serde_json::to_vec(&status).unwrap_or_default();
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .header("Cache-Control", "no-store")
            .body(hyper::Body::from(body))
            .unwrap_or_else(|_| Response::new(hyper::Body::from("{}"))))
    }

    pub(super) async fn handle_bitcoin_status() -> Result<Response<hyper::Body>> {
        let status = bitcoin_status::get_bitcoin_status().await;
        let body = serde_json::to_vec(&status).unwrap_or_default();
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .header("Cache-Control", "no-store")
            .body(hyper::Body::from(body))
            .unwrap_or_else(|_| Response::new(hyper::Body::from("{}"))))
    }

    pub(super) async fn handle_lnd_connect_info(
        rpc: std::sync::Arc<super::super::rpc::RpcHandler>,
        cors_origin: &str,
    ) -> Result<Response<hyper::Body>> {
        // The LND wallet UI is served on its own APP_PORTS origin and fetches
        // this cross-origin, so it needs the CORS headers echoed back.
        let cors = |builder: hyper::http::response::Builder| {
            builder
                .header("Access-Control-Allow-Origin", cors_origin)
                .header("Access-Control-Allow-Credentials", "true")
                .header("Vary", "Origin")
        };
        match rpc.handle_lnd_connect_info().await {
            Ok(val) => {
                let body = serde_json::to_vec(&val).unwrap_or_default();
                Ok(cors(
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", "application/json"),
                )
                .body(hyper::Body::from(body))
                .unwrap_or_else(|_| Response::new(hyper::Body::from("{}"))))
            }
            Err(e) => Ok(cors(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "application/json"),
            )
            .body(hyper::Body::from(
                serde_json::json!({"error": e.to_string()}).to_string(),
            ))
            .unwrap()),
        }
    }

    pub(super) async fn handle_lnd_proxy(
        rpc: Arc<RpcHandler>,
        path: &str,
        cors_origin: &str,
    ) -> Result<Response<hyper::Body>> {
        let suffix = path.strip_prefix("/proxy/lnd").unwrap_or("/");
        if suffix == "/archy-status" {
            return Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .header("Cache-Control", "no-store")
                .header("Access-Control-Allow-Origin", cors_origin)
                .header("Access-Control-Allow-Credentials", "true")
                .header("Vary", "Origin")
                .body(hyper::Body::from(
                    rpc.handle_lnd_readiness().await.to_string(),
                ))?);
        }

        let url = format!("{LND_REST_BASE_URL}{suffix}");
        // LND REST serves a self-signed cert and requires the admin macaroon.
        // A bare reqwest::get() uses the default client, which rejects the
        // self-signed cert (TLS verify error -> 502 "failing to fetch") and
        // sends no macaroon. Use the shared authenticated client instead — the
        // same one lnd.getinfo and the wallet RPCs use.
        let request = match rpc.lnd_client().await {
            Ok((client, macaroon_hex)) => client
                .get(&url)
                .header("Grpc-Metadata-macaroon", &macaroon_hex)
                .send()
                .await
                .map_err(anyhow::Error::from),
            Err(e) => Err(e),
        };
        match request {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let headers = resp.headers().clone();
                let body = resp.bytes().await.unwrap_or_default();
                let mut builder = Response::builder().status(status);
                if let Some(ct) = headers.get("content-type") {
                    if let Ok(s) = ct.to_str() {
                        builder = builder.header("Content-Type", s);
                    }
                }
                builder
                    .header("Access-Control-Allow-Origin", cors_origin)
                    .header("Access-Control-Allow-Credentials", "true")
                    .header("Vary", "Origin")
                    .body(hyper::Body::from(body))
                    .map_err(|e| anyhow::anyhow!("response build: {}", e))
            }
            Err(e) => {
                let body = serde_json::json!({ "error": e.to_string() });
                let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
                Ok(Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .header("Content-Type", "application/json")
                    .header("Access-Control-Allow-Origin", cors_origin)
                    .header("Access-Control-Allow-Credentials", "true")
                    .header("Vary", "Origin")
                    .body(hyper::Body::from(body_bytes))
                    .unwrap())
            }
        }
    }

    /// Range-streaming proxy for a peer's content file (B3). The browser's
    /// `<video>`/`<audio>` element makes Range requests; we forward the Range
    /// header to the peer's `/content/<id>` (which already returns 206 Partial
    /// Content) and pass the bytes + Content-Range/Content-Type straight back.
    /// This replaces the old path of downloading the whole file as base64 into
    /// a non-seekable Blob URL, which broke playback/seeking for video and
    /// large audio. Same-origin + session-authenticated (checked by caller).
    /// Path: `/api/peer-content/<onion>/<content_id>`.
    pub(super) async fn handle_peer_content_stream(
        &self,
        path: &str,
        headers: &hyper::HeaderMap,
    ) -> Result<Response<hyper::Body>> {
        let bad = |msg: &str| {
            Ok(build_response(
                StatusCode::BAD_REQUEST,
                "application/json",
                hyper::Body::from(serde_json::json!({ "error": msg }).to_string()),
            ))
        };
        let rest = path.strip_prefix("/api/peer-content/").unwrap_or("");
        let (onion, content_id) = match rest.split_once('/') {
            Some((o, c)) if !o.is_empty() && !c.is_empty() => (o, c),
            _ => return bad("expected /api/peer-content/<onion>/<content_id>"),
        };
        // Validate to prevent SSRF / path traversal.
        let onion_norm = onion.trim_end_matches(".onion");
        let onion_ok = onion_norm.len() == 56
            && onion_norm
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
        let id_ok = !content_id.contains("..")
            && content_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
        if !onion_ok || !id_ok {
            return bad("invalid onion or content id");
        }

        // Ownership is checked before opening a bounded file stream. Corrupt
        // records or missing purchased bytes never trigger another purchase.
        match crate::content_owned::open_owned(&self.config.data_dir, onion, content_id).await {
            Ok(Some((mime, file))) => return crate::media_stream::file_response(file, &mime, headers).await,
            Ok(None) => {},
            Err(_) => return Ok(build_response(StatusCode::CONFLICT, "application/json",
                hyper::Body::from(serde_json::json!({"error": "Purchased file unavailable locally. Recover the existing purchase without paying again."}).to_string()))),
        }

        let fips_npub = crate::federation::fips_npub_for_onion(&self.config.data_dir, onion).await;
        let peer_path = format!("/content/{}", content_id);
        // Generous overall timeout: this endpoint serves both seek/Range
        // playback (small, finishes fast) and full-file downloads of large
        // media (#38). 60s was too tight for a multi-hundred-MB transfer over
        // slow links and aborted the download mid-stream.
        let mut req = crate::fips::dial::PeerRequest::new(fips_npub.as_deref(), onion, &peer_path)
            .service(crate::settings::transport::PeerService::PeerFiles)
            .require_fips()
            .record_transport(&self.config.data_dir)
            .timeout(std::time::Duration::from_secs(900));
        if let Some(r) = headers.get("range").and_then(|v| v.to_str().ok()) {
            req = req.header("Range", r.to_string());
        }
        let req = req.authenticate_content(&self.config.data_dir).await?;
        match req.send_get().await {
            Ok((resp, transport)) => {
                if resp.status().is_redirection() {
                    return Ok(build_response(
                        StatusCode::BAD_GATEWAY,
                        "application/json",
                        hyper::Body::from("{\"error\":\"Peer media redirects are not allowed\"}"),
                    ));
                }
                let status = resp.status().as_u16();
                let rh = resp.headers().clone();
                let mut builder = Response::builder()
                    .status(status)
                    .header("Accept-Ranges", "bytes")
                    .header("X-Archipelago-Transport", transport.to_string());
                for h in ["content-type", "content-range", "content-length"] {
                    if let Some(v) = rh.get(h).and_then(|v| v.to_str().ok()) {
                        builder = builder.header(h, v);
                    }
                }
                // Stream the peer's body straight through instead of buffering
                // the whole file into memory (#38). For a 178MB download the old
                // `resp.bytes().await` allocated the entire file on the node
                // before sending a byte; `wrap_stream` forwards chunks as they
                // arrive, with constant memory.
                Ok(builder
                    .body(hyper::Body::wrap_stream(resp.bytes_stream()))
                    .unwrap_or_else(|_| Response::new(hyper::Body::empty())))
            }
            Err(e) => Ok(build_response(
                StatusCode::BAD_GATEWAY,
                "application/json",
                hyper::Body::from(serde_json::json!({ "error": e.to_string() }).to_string()),
            )),
        }
    }
}
