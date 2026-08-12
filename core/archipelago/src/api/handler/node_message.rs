use super::build_response;
use crate::api::rpc::RpcHandler;
use crate::node_message as node_msg;
use anyhow::Result;
use hyper::{Response, StatusCode};
use std::sync::Arc;

use super::{is_valid_pubkey_hex, sanitize_html, sanitize_log_string, ApiHandler};

impl ApiHandler {
    pub(super) async fn handle_node_message(
        body: hyper::body::Bytes,
    ) -> Result<Response<hyper::Body>> {
        #[derive(serde::Deserialize)]
        struct Incoming {
            from_pubkey: Option<String>,
            from_name: Option<String>,
            message: Option<String>,
            signature: Option<String>,
            #[serde(default)]
            encrypted: bool,
            #[serde(default)]
            msg_id: Option<String>,
        }
        let incoming: Incoming = serde_json::from_slice(&body).unwrap_or(Incoming {
            from_pubkey: None,
            from_name: None,
            message: None,
            signature: None,
            encrypted: false,
            msg_id: None,
        });
        if let (Some(from), Some(msg)) = (incoming.from_pubkey.as_ref(), incoming.message.as_ref())
        {
            // Validate from_pubkey is a valid hex ed25519 pubkey
            if !is_valid_pubkey_hex(from) {
                return Ok(build_response(
                    StatusCode::BAD_REQUEST,
                    "application/json",
                    hyper::Body::from(r#"{"error":"Invalid pubkey format"}"#),
                ));
            }
            // Verify ed25519 signature if provided (required for trusted messages)
            if let Some(sig_hex) = &incoming.signature {
                match crate::identity::NodeIdentity::verify(from, msg.as_bytes(), sig_hex) {
                    Ok(true) => {}
                    _ => {
                        return Ok(build_response(
                            StatusCode::FORBIDDEN,
                            "application/json",
                            hyper::Body::from(r#"{"error":"Invalid signature"}"#),
                        ));
                    }
                }
            }

            // Decrypt if the message is E2E encrypted
            let plaintext = if incoming.encrypted {
                // Load our identity to derive shared secret
                let data_dir = std::path::Path::new("/var/lib/archipelago");
                let identity_dir = data_dir.join("identity");
                match crate::identity::NodeIdentity::load_or_create(&identity_dir).await {
                    Ok(node_id) => {
                        match node_msg::decrypt_from_peer(node_id.signing_key(), from, msg) {
                            Ok(decrypted) => {
                                tracing::info!(
                                    "Decrypted E2E message from {}...",
                                    &from[..16.min(from.len())]
                                );
                                decrypted
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "E2E decryption failed from {}: {}",
                                    &from[..16.min(from.len())],
                                    e
                                );
                                return Ok(build_response(
                                    StatusCode::BAD_REQUEST,
                                    "application/json",
                                    hyper::Body::from(r#"{"error":"Decryption failed"}"#),
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Cannot decrypt: identity load failed: {}", e);
                        msg.clone()
                    }
                }
            } else {
                msg.clone()
            };

            // Detect a `connection_accepted` reply: the remote peer just
            // approved an outbound request we sent, so mirror their add on
            // our side (bidirectional peering without a manual second
            // click). JSON-shape only — any non-matching payload stays in
            // the normal received-messages store below.
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&plaintext) {
                if val.get("type").and_then(|v| v.as_str()) == Some("connection_accepted") {
                    if let (Some(their_onion), Some(their_pubkey)) = (
                        val.get("from_onion").and_then(|v| v.as_str()),
                        val.get("from_pubkey").and_then(|v| v.as_str()),
                    ) {
                        let data_dir = std::path::Path::new("/var/lib/archipelago");
                        let peer = crate::peers::KnownPeer {
                            onion: their_onion.to_string(),
                            pubkey: their_pubkey.to_string(),
                            name: val
                                .get("from_name")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                            added_at: Some(chrono::Utc::now().to_rfc3339()),
                        };
                        match crate::peers::add_peer(data_dir, peer).await {
                            Ok(_) => tracing::info!(
                                from = %sanitize_log_string(from),
                                "Auto-added peer after connection_accepted"
                            ),
                            Err(e) => tracing::warn!(
                                from = %sanitize_log_string(from),
                                error = %e,
                                "Failed to auto-add peer on connection_accepted"
                            ),
                        }
                    }
                    return Ok(build_response(
                        StatusCode::OK,
                        "application/json",
                        hyper::Body::from(r#"{"ok":true,"handled":"connection_accepted"}"#),
                    ));
                }

                if let Some(handled) =
                    crate::api::rpc::bitcoin_relay::record_incoming_relay_message(
                        std::path::Path::new("/var/lib/archipelago"),
                        from,
                        incoming.from_name.as_deref(),
                        &val,
                    )
                    .await?
                {
                    return Ok(build_response(
                        StatusCode::OK,
                        "application/json",
                        hyper::Body::from(format!(r#"{{"ok":true,"handled":"{}"}}"#, handled)),
                    ));
                }
            }

            let safe_from = sanitize_log_string(from);
            let safe_msg = sanitize_log_string(&plaintext);
            tracing::info!("Received message from {}: {}", safe_from, safe_msg);
            let clean_from = sanitize_html(from);
            let clean_msg = sanitize_html(&plaintext);
            let clean_name = incoming.from_name.as_deref().map(sanitize_html);
            node_msg::store_received(
                &clean_from,
                &clean_msg,
                clean_name.as_deref(),
                incoming.msg_id.as_deref(),
            )
            .await;
        }
        Ok(build_response(
            StatusCode::OK,
            "application/json",
            hyper::Body::from(r#"{"ok":true}"#),
        ))
    }

    /// Federation-routed mesh typed envelope. Body:
    /// `{from_pubkey, from_name?, typed_envelope_b64, signature}`
    /// Signature is ed25519 over the raw wire bytes, verified against
    /// from_pubkey before dispatch.
    pub(super) async fn handle_mesh_typed_relay(
        rpc_handler: Arc<RpcHandler>,
        body: hyper::body::Bytes,
    ) -> Result<Response<hyper::Body>> {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        #[derive(serde::Deserialize)]
        struct Incoming {
            from_pubkey: String,
            #[serde(default)]
            from_name: Option<String>,
            typed_envelope_b64: String,
            signature: String,
        }
        let incoming: Incoming = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => {
                return Ok(build_response(
                    StatusCode::BAD_REQUEST,
                    "application/json",
                    hyper::Body::from(format!(r#"{{"error":"bad json: {}"}}"#, e)),
                ));
            }
        };
        if !is_valid_pubkey_hex(&incoming.from_pubkey) {
            return Ok(build_response(
                StatusCode::BAD_REQUEST,
                "application/json",
                hyper::Body::from(r#"{"error":"invalid pubkey"}"#),
            ));
        }
        let wire = match BASE64.decode(incoming.typed_envelope_b64.as_bytes()) {
            Ok(v) => v,
            Err(_) => {
                return Ok(build_response(
                    StatusCode::BAD_REQUEST,
                    "application/json",
                    hyper::Body::from(r#"{"error":"bad base64"}"#),
                ));
            }
        };
        match crate::identity::NodeIdentity::verify(
            &incoming.from_pubkey,
            &wire,
            &incoming.signature,
        ) {
            Ok(true) => {}
            _ => {
                return Ok(build_response(
                    StatusCode::FORBIDDEN,
                    "application/json",
                    hyper::Body::from(r#"{"error":"signature rejected"}"#),
                ));
            }
        }

        // Inject into mesh state via the shared MeshService. Mirrors a radio
        // receive, so the message lands in the same chat stream as LoRa-
        // delivered messages from the same peer.
        let service = rpc_handler.mesh_service_arc();
        let svc_guard = service.read().await;
        let Some(svc) = svc_guard.as_ref() else {
            return Ok(build_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "application/json",
                hyper::Body::from(r#"{"error":"mesh not running"}"#),
            ));
        };
        if let Err(e) = svc
            .inject_typed_from_federation(
                &incoming.from_pubkey,
                incoming.from_name.as_deref(),
                wire,
            )
            .await
        {
            tracing::warn!("mesh-typed relay inject failed: {}", e);
            return Ok(build_response(
                StatusCode::BAD_REQUEST,
                "application/json",
                hyper::Body::from(format!(r#"{{"error":"{}"}}"#, e)),
            ));
        }
        Ok(build_response(
            StatusCode::OK,
            "application/json",
            hyper::Body::from(r#"{"ok":true}"#),
        ))
    }
}
