//! HTTP handlers for the content-addressed blob store.
//!
//! - `POST /api/blob` — session-authenticated. Raw body is the blob;
//!   headers set mime/filename. Returns `{cid, size, mime}`.
//! - `GET /blob/<cid>?cap=<hex>&exp=<epoch>&peer=<pubkey>` — peer-facing.
//!   Capability verified against the stored HMAC key; bytes streamed back.

use super::{build_response, ApiHandler};
use crate::blobs::BlobStore;
use anyhow::Result;
use hyper::{Body, HeaderMap, Response, StatusCode};
use std::path::Path;
use std::sync::Arc;

/// Read the archipelago .onion address if Tor has published one, so uploads
/// that need to be publicly reachable (profile pictures, banners) can return
/// a URL a peer outside the LAN can actually fetch. Returns `None` before
/// onboarding or when Tor isn't running — callers fall back to the local
/// self-test URL.
async fn read_self_onion(data_dir: &Path) -> Option<String> {
    let hostnames = data_dir.join("tor-hostnames").join("archipelago");
    let legacy = Path::new("/var/lib/archipelago/tor-hostnames/archipelago");
    for p in [hostnames.as_path(), legacy] {
        if let Ok(s) = tokio::fs::read_to_string(p).await {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

impl ApiHandler {
    pub(super) async fn handle_blob_upload(
        store: &Arc<BlobStore>,
        self_pubkey_hex: &str,
        data_dir: &Path,
        headers: &HeaderMap,
        body: hyper::body::Bytes,
    ) -> Result<Response<Body>> {
        let mime = headers
            .get("x-blob-mime")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        let filename = headers
            .get("x-blob-filename")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        // Optional caller-supplied thumbnail (small, base64) — e.g. the mesh
        // chat's image-quality picker generates a tiny client-side preview so
        // a ContentRef receiver can render something before fetching the full
        // blob. Best-effort: a malformed header is just ignored, not fatal.
        let thumb_bytes = headers
            .get("x-blob-thumb")
            .and_then(|v| v.to_str().ok())
            .and_then(|b64| {
                use base64::{engine::general_purpose::STANDARD, Engine as _};
                STANDARD.decode(b64).ok()
            });

        let bytes = body.to_vec();
        // Uploads through /api/blob come from the node owner's session and
        // are almost always intended for external consumption (profile
        // pictures, banners). Store them public so `/blob/<cid>` serves
        // without a capability check — external Nostr clients fetching a
        // kind-0 `picture` URL have no cap and can't get one.
        match store.put(&bytes, &mime, filename, thumb_bytes, true).await {
            Ok(meta) => {
                let exp =
                    (chrono::Utc::now().timestamp() as u64) + crate::blobs::DEFAULT_CAP_TTL_SECS;
                let cap = store.issue_capability(&meta.cid, self_pubkey_hex, exp);
                let self_test_url = format!(
                    "/blob/{}?cap={}&exp={}&peer={}",
                    meta.cid, cap, exp, self_pubkey_hex
                );
                let public_url = match read_self_onion(data_dir).await {
                    Some(onion) => format!("http://{}/blob/{}", onion, meta.cid),
                    // Pre-onboarding / Tor-not-up: surface the local path so
                    // the UI doesn't break; publishing to Nostr should wait
                    // until Tor is live anyway.
                    None => format!("/blob/{}", meta.cid),
                };
                let resp = serde_json::json!({
                    "cid": meta.cid,
                    "size": meta.size,
                    "mime": meta.mime,
                    "filename": meta.filename,
                    "public_url": public_url,
                    "self_test_url": self_test_url,
                });
                Ok(build_response(
                    StatusCode::OK,
                    "application/json",
                    Body::from(serde_json::to_vec(&resp).unwrap_or_default()),
                ))
            }
            Err(e) => Ok(build_response(
                StatusCode::BAD_REQUEST,
                "text/plain",
                Body::from(format!("blob upload failed: {}", e)),
            )),
        }
    }

    /// Share-to-mesh iframe intent. Mirrors `handle_blob_upload` but adds
    /// CORS headers for the requesting app origin and returns a small JSON
    /// payload the app forwards to its parent via postMessage:
    /// `{ type: "share-to-mesh", cid, size, mime, filename }`.
    pub(super) async fn handle_share_to_mesh(
        store: &Arc<BlobStore>,
        self_pubkey_hex: &str,
        headers: &HeaderMap,
        body: hyper::body::Bytes,
        origin: &str,
    ) -> Result<Response<Body>> {
        let mime = headers
            .get("x-blob-mime")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        let filename = headers
            .get("x-blob-filename")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let bytes = body.to_vec();
        let meta = match store.put(&bytes, &mime, filename, None, false).await {
            Ok(m) => m,
            Err(e) => {
                return Ok(build_response(
                    StatusCode::BAD_REQUEST,
                    "text/plain",
                    Body::from(format!("share-to-mesh failed: {}", e)),
                ));
            }
        };
        // Self-signed capability so the app can preview/download its own
        // upload before the user has picked a peer.
        let exp = (chrono::Utc::now().timestamp() as u64) + crate::blobs::DEFAULT_CAP_TTL_SECS;
        let cap = store.issue_capability(&meta.cid, self_pubkey_hex, exp);
        let self_url = format!(
            "/blob/{}?cap={}&exp={}&peer={}",
            meta.cid, cap, exp, self_pubkey_hex
        );
        let resp = serde_json::json!({
            "type": "share-to-mesh",
            "cid": meta.cid,
            "size": meta.size,
            "mime": meta.mime,
            "filename": meta.filename,
            "self_url": self_url,
        });
        let body_vec = serde_json::to_vec(&resp).unwrap_or_default();
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", origin)
            .header("Access-Control-Allow-Credentials", "true")
            .header("Vary", "Origin")
            .body(Body::from(body_vec))
            .unwrap_or_else(|_| Response::new(Body::from("internal error"))))
    }

    pub(super) async fn handle_blob_download(
        store: &Arc<BlobStore>,
        path: &str,
        query: &str,
    ) -> Result<Response<Body>> {
        let cid = path.strip_prefix("/blob/").unwrap_or("");
        if cid.is_empty() || !cid.chars().all(|c| c.is_ascii_hexdigit()) || cid.len() != 64 {
            return Ok(build_response(
                StatusCode::BAD_REQUEST,
                "text/plain",
                Body::from("invalid cid"),
            ));
        }

        // Public blobs (profile pictures, banners) bypass the capability
        // check — their CID is published on Nostr relays where any reader
        // can see it, and external readers have no way to obtain a cap.
        // Only blobs explicitly marked public at upload time qualify.
        let is_public = store.meta(cid).await.map(|m| m.public).unwrap_or(false);

        if !is_public {
            let mut cap = None;
            let mut exp: Option<u64> = None;
            let mut peer = None;
            for pair in query.split('&') {
                let mut it = pair.splitn(2, '=');
                match (it.next(), it.next()) {
                    (Some("cap"), Some(v)) => cap = Some(v.to_string()),
                    (Some("exp"), Some(v)) => exp = v.parse().ok(),
                    (Some("peer"), Some(v)) => peer = Some(v.to_string()),
                    _ => {}
                }
            }
            let (Some(cap), Some(exp), Some(peer)) = (cap, exp, peer) else {
                return Ok(build_response(
                    StatusCode::UNAUTHORIZED,
                    "text/plain",
                    Body::from("missing cap/exp/peer"),
                ));
            };

            if let Err(e) = store.verify_capability(cid, &peer, exp, &cap) {
                tracing::warn!("blob cap rejected: cid={} peer={} reason={}", cid, peer, e);
                return Ok(build_response(
                    StatusCode::FORBIDDEN,
                    "text/plain",
                    Body::from(format!("capability rejected: {}", e)),
                ));
            }
        }

        let bytes = match store.get(cid).await {
            Ok(b) => b,
            Err(_) => {
                return Ok(build_response(
                    StatusCode::NOT_FOUND,
                    "text/plain",
                    Body::from("blob not found"),
                ))
            }
        };
        let mime = store
            .meta(cid)
            .await
            .map(|m| m.mime)
            .unwrap_or_else(|_| "application/octet-stream".to_string());
        Ok(build_response(StatusCode::OK, &mime, Body::from(bytes)))
    }
}
