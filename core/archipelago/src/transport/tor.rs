// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Tor transport — sends messages via HTTP POST through SOCKS5 proxy.
//!
//! Wraps the existing `node_message.rs` Tor messaging logic behind
//! the `NodeTransport` trait.

use super::{NodeTransport, TransportKind, TransportMessage};
use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const TOR_TIMEOUT: Duration = Duration::from_secs(60);

pub struct TorTransport {
    our_pubkey_hex: String,
    available: AtomicBool,
}

impl TorTransport {
    pub fn new(our_pubkey_hex: &str) -> Self {
        Self {
            our_pubkey_hex: our_pubkey_hex.to_string(),
            available: AtomicBool::new(true), // Assume available, checked lazily
        }
    }

    /// Update availability (call periodically from health check).
    pub fn set_available(&self, avail: bool) {
        self.available.store(avail, Ordering::Relaxed);
    }

    async fn send_impl(&self, onion_address: &str, message: &TransportMessage) -> Result<()> {
        let host = if onion_address.ends_with(".onion") {
            onion_address.to_string()
        } else {
            format!("{}.onion", onion_address)
        };

        let url = format!("http://{}/archipelago/node-message", host);
        let encoded_payload = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(&message.payload)
        };
        let body = serde_json::json!({
            "from_pubkey": self.our_pubkey_hex,
            "message": encoded_payload,
            "message_type": message.message_type,
            "from_did": message.from_did,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let proxy =
            reqwest::Proxy::all(crate::constants::TOR_SOCKS_PROXY).context("Invalid Tor proxy")?;
        let client = reqwest::Client::builder()
            .proxy(proxy)
            .timeout(TOR_TIMEOUT)
            .build()
            .context("Failed to build Tor HTTP client")?;

        let resp = client.post(&url).json(&body).send().await.map_err(|e| {
            let msg = e.to_string();
            if msg.contains("connection refused") || msg.contains("Connection refused") {
                self.available.store(false, Ordering::Relaxed);
                anyhow::anyhow!("Tor not reachable at 127.0.0.1:9050")
            } else if msg.contains("timeout") || msg.contains("timed out") {
                anyhow::anyhow!("Tor connection timed out — peer may be offline")
            } else {
                anyhow::anyhow!("Tor send failed: {}", msg)
            }
        })?;

        if !resp.status().is_success() {
            anyhow::bail!("Peer returned {} over Tor", resp.status().as_u16());
        }
        Ok(())
    }
}

impl NodeTransport for TorTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Tor
    }

    fn is_available(&self) -> bool {
        self.available.load(Ordering::Relaxed)
    }

    fn send<'a>(
        &'a self,
        address: &'a str,
        message: &'a TransportMessage,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move { self.send_impl(address, message).await })
    }
}
