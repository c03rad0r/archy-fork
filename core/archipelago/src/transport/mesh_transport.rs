// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Mesh transport — sends messages via LoRa radio through the MeshService.
//!
//! Bridges the transport abstraction to the existing mesh serial listener.
//! For payloads exceeding the LoRa frame limit (160 bytes), uses the chunking
//! protocol with Reed-Solomon FEC for reliable delivery.

use super::chunking::{self, ChunkReassembler};
use super::{NodeTransport, TransportKind, TransportMessage};
use crate::mesh::MeshService;
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Inter-chunk delay for LoRa airtime fairness.
const CHUNK_DELAY: Duration = Duration::from_millis(200);

/// Maximum single-frame payload (before chunking kicks in).
/// After ChaCha20-Poly1305 overhead: 160 - 12 (nonce) - 16 (tag) = 132 bytes.
const MAX_SINGLE_FRAME: usize = 132;

pub struct MeshTransport {
    mesh_service: Arc<RwLock<Option<MeshService>>>,
    reassembler: Arc<RwLock<ChunkReassembler>>,
}

impl MeshTransport {
    pub fn new(mesh_service: Arc<RwLock<Option<MeshService>>>) -> Self {
        Self {
            mesh_service,
            reassembler: Arc::new(RwLock::new(ChunkReassembler::new())),
        }
    }

    /// Get a reference to the chunk reassembler (for incoming message processing).
    pub fn reassembler(&self) -> Arc<RwLock<ChunkReassembler>> {
        Arc::clone(&self.reassembler)
    }

    async fn send_impl(&self, contact_id_str: &str, message: &TransportMessage) -> Result<()> {
        let contact_id: u32 = contact_id_str.parse().context("Invalid mesh contact ID")?;

        let service = self.mesh_service.read().await;
        let service = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        // Serialize the transport message as CBOR for compact encoding
        let mut payload = Vec::new();
        ciborium::into_writer(message, &mut payload)
            .context("Failed to CBOR-encode transport message")?;

        if payload.len() <= MAX_SINGLE_FRAME {
            // Fits in a single LoRa frame — send directly as text
            let text = {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.encode(&payload)
            };
            service
                .send_message(contact_id, &text)
                .await
                .context("Mesh single-frame send failed")?;
        } else {
            // Chunk with FEC
            let chunks = chunking::encode_chunked(&payload)?;
            tracing::info!(
                chunks = chunks.len(),
                payload_bytes = payload.len(),
                "Sending chunked message over mesh"
            );
            for chunk in &chunks {
                let chunk_bytes = chunk.to_bytes();
                let text = {
                    use base64::Engine;
                    base64::engine::general_purpose::STANDARD.encode(&chunk_bytes)
                };
                service
                    .send_message(contact_id, &text)
                    .await
                    .context("Mesh chunk send failed")?;
                tokio::time::sleep(CHUNK_DELAY).await;
            }
        }

        Ok(())
    }
}

impl NodeTransport for MeshTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Mesh
    }

    fn is_available(&self) -> bool {
        // Check synchronously — we can't await here, so use try_read
        match self.mesh_service.try_read() {
            Ok(guard) => match guard.as_ref() {
                Some(_service) => true, // Service exists
                None => false,
            },
            Err(_) => false, // Lock contention — assume unavailable
        }
    }

    fn send<'a>(
        &'a self,
        address: &'a str,
        message: &'a TransportMessage,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move { self.send_impl(address, message).await })
    }
}
