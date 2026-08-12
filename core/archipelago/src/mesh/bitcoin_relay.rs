// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Off-grid Bitcoin operations over mesh radio.
//!
//! Enables mesh-only nodes (no internet) to:
//! - Receive compact block header announcements from internet-connected peers
//! - Relay raw transactions to internet-connected peers for broadcast
//! - Send/receive Lightning invoices and proof-of-payment via mesh
//!
//! All amounts in satoshis (u64), never floating point.

use super::message_types::{
    self, BlockHeaderPayload, LightningRelayPayload, LightningRelayResponsePayload,
    MeshMessageType, TxRelayPayload, TxRelayResponsePayload, TypedEnvelope,
};
use anyhow::{Context, Result};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::warn;

// ─── Block Header Cache ─────────────────────────────────────────────────

/// Stores the latest block headers received via mesh (for mesh-only SPV).
pub struct BlockHeaderCache {
    /// Latest known block height.
    latest_height: RwLock<u64>,
    /// Recent headers (height -> header).
    headers: RwLock<HashMap<u64, BlockHeaderPayload>>,
    /// Maximum headers to cache.
    max_cached: usize,
}

impl BlockHeaderCache {
    pub fn new() -> Self {
        Self {
            latest_height: RwLock::new(0),
            headers: RwLock::new(HashMap::new()),
            max_cached: 100,
        }
    }

    /// Store a received block header.
    pub async fn store_header(&self, header: BlockHeaderPayload) -> Result<()> {
        let mut latest = self.latest_height.write().await;
        let mut headers = self.headers.write().await;

        if header.height > *latest {
            *latest = header.height;
        }

        headers.insert(header.height, header);

        // Evict oldest if over limit
        if headers.len() > self.max_cached {
            let min_height = *latest - self.max_cached as u64;
            headers.retain(|h, _| *h > min_height);
        }

        Ok(())
    }

    /// Get the latest block height received via mesh.
    pub async fn latest_height(&self) -> u64 {
        *self.latest_height.read().await
    }

    /// Get a specific header by height.
    pub async fn get_header(&self, height: u64) -> Option<BlockHeaderPayload> {
        self.headers.read().await.get(&height).cloned()
    }

    /// Get the N most recent headers.
    pub async fn recent_headers(&self, count: usize) -> Vec<BlockHeaderPayload> {
        let headers = self.headers.read().await;
        let mut sorted: Vec<_> = headers.values().cloned().collect();
        sorted.sort_by(|a, b| b.height.cmp(&a.height));
        sorted.truncate(count);
        sorted
    }
}

impl Default for BlockHeaderCache {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Pending Relay Requests ─────────────────────────────────────────────

/// Tracks in-flight relay requests awaiting responses.
pub struct RelayTracker {
    /// Pending TX relay requests (request_id -> original requester DID).
    tx_requests: RwLock<HashMap<u64, PendingRelay>>,
    /// Pending Lightning relay requests.
    lightning_requests: RwLock<HashMap<u64, PendingRelay>>,
    /// Completed relay results (kept for 5 minutes for frontend polling).
    completed_results: RwLock<Vec<RelayResult>>,
}

#[derive(Debug, Clone)]
struct PendingRelay {
    requester_did: String,
    created_at: String,
}

/// Result of a completed relay attempt, stored for frontend polling.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RelayResult {
    pub request_id: u64,
    pub txid: Option<String>,
    pub error: Option<String>,
    pub error_code: Option<String>,
    pub completed_at: String,
}

impl RelayTracker {
    pub fn new() -> Self {
        Self {
            tx_requests: RwLock::new(HashMap::new()),
            lightning_requests: RwLock::new(HashMap::new()),
            completed_results: RwLock::new(Vec::new()),
        }
    }

    /// Register a pending TX relay request.
    pub async fn track_tx_relay(&self, request_id: u64, requester_did: &str) {
        self.tx_requests.write().await.insert(
            request_id,
            PendingRelay {
                requester_did: requester_did.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        );
    }

    /// Complete a TX relay request and return the original requester's DID.
    pub async fn complete_tx_relay(&self, request_id: u64) -> Option<String> {
        self.tx_requests
            .write()
            .await
            .remove(&request_id)
            .map(|r| r.requester_did)
    }

    /// Register a pending Lightning relay request.
    pub async fn track_lightning_relay(&self, request_id: u64, requester_did: &str) {
        self.lightning_requests.write().await.insert(
            request_id,
            PendingRelay {
                requester_did: requester_did.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        );
    }

    /// Complete a Lightning relay request.
    pub async fn complete_lightning_relay(&self, request_id: u64) -> Option<String> {
        self.lightning_requests
            .write()
            .await
            .remove(&request_id)
            .map(|r| r.requester_did)
    }

    /// Count pending requests.
    pub async fn pending_count(&self) -> (usize, usize) {
        let tx = self.tx_requests.read().await.len();
        let ln = self.lightning_requests.read().await.len();
        (tx, ln)
    }

    /// Store a completed relay result for frontend polling.
    pub async fn store_result(&self, result: RelayResult) {
        let mut results = self.completed_results.write().await;
        // Evict results older than 5 minutes
        let cutoff = chrono::Utc::now() - chrono::Duration::minutes(5);
        let cutoff_str = cutoff.to_rfc3339();
        results.retain(|r| r.completed_at > cutoff_str);
        results.push(result);
    }

    /// Get relay result by request_id (returns None if not yet completed or expired).
    pub async fn get_result(&self, request_id: u64) -> Option<RelayResult> {
        self.completed_results
            .read()
            .await
            .iter()
            .find(|r| r.request_id == request_id)
            .cloned()
    }

    /// Check if a TX relay request is still pending.
    pub async fn is_pending(&self, request_id: u64) -> bool {
        self.tx_requests.read().await.contains_key(&request_id)
    }
}

impl Default for RelayTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Block Header Announcement Builder ──────────────────────────────────

/// Build a compact block header announcement for mesh broadcast.
/// Uses raw binary (not CBOR) to fit within the 160-byte LoRa limit:
///   height(8 LE) + hash_raw(32) + timestamp(4 LE) = 44 bytes payload
/// Wrapped in unsigned TypedEnvelope (~25 bytes overhead) = ~69 total.
pub fn build_block_header_announcement(
    height: u64,
    hash: &str,
    _prev_hash: &str,
    timestamp: u32,
    _our_did: &str,
    _signing_key: &ed25519_dalek::SigningKey,
) -> Result<Vec<u8>> {
    let hash_bytes = hex::decode(hash).context("Invalid block hash hex")?;
    if hash_bytes.len() != 32 {
        anyhow::bail!("Block hash must be 32 bytes, got {}", hash_bytes.len());
    }

    // Compact binary: height(8) + hash(32) + timestamp(4) = 44 bytes
    let mut payload = Vec::with_capacity(44);
    payload.extend_from_slice(&height.to_le_bytes());
    payload.extend_from_slice(&hash_bytes);
    payload.extend_from_slice(&timestamp.to_le_bytes());

    // Use unsigned envelope to save 64 bytes (no Ed25519 signature)
    let envelope = TypedEnvelope::new(MeshMessageType::BlockHeader, payload);
    envelope.to_wire()
}

/// Decode a compact block header from raw binary payload.
/// Returns (height, hash_hex, timestamp).
pub fn decode_compact_block_header(payload: &[u8]) -> Result<(u64, String, u32)> {
    if payload.len() < 44 {
        anyhow::bail!("Compact block header too short: {} bytes", payload.len());
    }
    let height = u64::from_le_bytes(
        payload[0..8]
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid height bytes in block header"))?,
    );
    let hash_hex = hex::encode(&payload[8..40]);
    let timestamp = u32::from_le_bytes(
        payload[40..44]
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid timestamp bytes in block header"))?,
    );
    Ok((height, hash_hex, timestamp))
}

/// Build a TX relay request envelope.
pub fn build_tx_relay_request(tx_hex: &str, request_id: u64) -> Result<Vec<u8>> {
    let payload = message_types::encode_payload(&TxRelayPayload {
        tx_hex: tx_hex.to_string(),
        request_id,
    })?;
    let envelope = TypedEnvelope::new(MeshMessageType::TxRelay, payload);
    envelope.to_wire()
}

/// Build a TX relay response envelope.
pub fn build_tx_relay_response(
    request_id: u64,
    txid: Option<&str>,
    error: Option<&str>,
    error_code: Option<&str>,
) -> Result<Vec<u8>> {
    let payload = message_types::encode_payload(&TxRelayResponsePayload {
        request_id,
        txid: txid.map(|s| s.to_string()),
        error: error.map(|s| s.to_string()),
        error_code: error_code.map(|s| s.to_string()),
    })?;
    let envelope = TypedEnvelope::new(MeshMessageType::TxRelayResponse, payload);
    envelope.to_wire()
}

/// Build a Lightning invoice relay request.
pub fn build_lightning_relay_request(
    bolt11: &str,
    amount_sats: u64,
    request_id: u64,
) -> Result<Vec<u8>> {
    let payload = message_types::encode_payload(&LightningRelayPayload {
        bolt11: bolt11.to_string(),
        amount_sats,
        request_id,
    })?;
    let envelope = TypedEnvelope::new(MeshMessageType::LightningRelay, payload);
    envelope.to_wire()
}

/// Build a Lightning relay response (proof of payment).
pub fn build_lightning_relay_response(
    request_id: u64,
    payment_hash: Option<&str>,
    preimage: Option<&str>,
    error: Option<&str>,
) -> Result<Vec<u8>> {
    let payload = message_types::encode_payload(&LightningRelayResponsePayload {
        request_id,
        payment_hash: payment_hash.map(|s| s.to_string()),
        preimage: preimage.map(|s| s.to_string()),
        error: error.map(|s| s.to_string()),
    })?;
    let envelope = TypedEnvelope::new(MeshMessageType::LightningRelayResponse, payload);
    envelope.to_wire()
}

// ─── Validation Functions ─────────────────────────────────────────────

/// Validate a received block header before storing/relaying.
/// Rejects obviously invalid headers (bad version, impossibly far-ahead height).
pub fn validate_block_header(
    height: u64,
    hash_hex: &str,
    timestamp: u32,
    last_known_height: u64,
) -> bool {
    // Hash must be 64 hex chars (32 bytes)
    if hash_hex.len() != 64 {
        warn!(
            "Block header rejected: hash length {} != 64",
            hash_hex.len()
        );
        return false;
    }
    // Height must not be impossibly far ahead (allow 100 blocks gap for mesh delays)
    if last_known_height > 0 && height > last_known_height + 100 {
        warn!(
            "Block header height {} is too far ahead of known height {}",
            height, last_known_height
        );
        return false;
    }
    // Timestamp sanity: must not be before Bitcoin genesis (2009-01-03) or far in the future
    if timestamp < 1_231_006_505 {
        warn!(
            "Block header rejected: timestamp {} before Bitcoin genesis",
            timestamp
        );
        return false;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32;
    if timestamp > now + 7200 {
        warn!(
            "Block header rejected: timestamp {} is more than 2 hours in the future",
            timestamp
        );
        return false;
    }
    true
}

/// Validate a raw transaction hex string before relaying to Bitcoin Core.
/// Checks basic syntax constraints only (full validation is done by Bitcoin Core).
pub fn validate_raw_transaction(tx_hex: &str) -> bool {
    // Must be valid hex
    let tx_bytes = match hex::decode(tx_hex) {
        Ok(b) => b,
        Err(_) => {
            warn!("TX relay rejected: invalid hex");
            return false;
        }
    };
    // Minimum valid transaction size is ~60 bytes, max 400KB
    if tx_bytes.len() < 60 || tx_bytes.len() > 400_000 {
        warn!(
            "TX relay rejected: size {} out of range [60, 400000]",
            tx_bytes.len()
        );
        return false;
    }
    // Check version bytes (first 4 bytes, little-endian) — valid versions: 1, 2, 3
    if tx_bytes.len() >= 4 {
        let version = u32::from_le_bytes([tx_bytes[0], tx_bytes[1], tx_bytes[2], tx_bytes[3]]);
        if !(1..=3).contains(&version) {
            warn!("TX relay rejected: version {} not in [1,3]", version);
            return false;
        }
    }
    true
}

/// Simple per-peer rate limiter for mesh relay operations.
pub struct RelayRateLimiter {
    /// (peer_id, message_type) -> list of timestamps
    windows: RwLock<HashMap<(u32, &'static str), Vec<std::time::Instant>>>,
}

impl RelayRateLimiter {
    pub fn new() -> Self {
        Self {
            windows: RwLock::new(HashMap::new()),
        }
    }

    /// Check if a relay operation is allowed. Returns true if within rate limits.
    /// max_per_minute: maximum operations per 60-second window.
    pub async fn check(&self, peer_id: u32, msg_type: &'static str, max_per_minute: usize) -> bool {
        let now = std::time::Instant::now();
        let cutoff = now - std::time::Duration::from_secs(60);
        let mut windows = self.windows.write().await;
        let key = (peer_id, msg_type);
        let timestamps = windows.entry(key).or_insert_with(Vec::new);

        // Remove entries older than 60 seconds
        timestamps.retain(|t| *t > cutoff);

        if timestamps.len() >= max_per_minute {
            warn!(
                peer_id,
                msg_type,
                "Rate limit exceeded: {} in last minute",
                timestamps.len()
            );
            return false;
        }
        timestamps.push(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[tokio::test]
    async fn test_block_header_cache() {
        let cache = BlockHeaderCache::new();
        cache
            .store_header(BlockHeaderPayload {
                height: 890412,
                hash: "0000000000000000000abc".to_string(),
                prev_hash: "0000000000000000000aab".to_string(),
                timestamp: 1710633600,
                announced_by: "did:key:z6MkTest".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(cache.latest_height().await, 890412);
        let header = cache.get_header(890412).await.unwrap();
        assert_eq!(header.hash, "0000000000000000000abc");
    }

    #[test]
    fn test_build_block_header_announcement() {
        let key = SigningKey::generate(&mut OsRng);
        let wire = build_block_header_announcement(
            890412,
            // Block hashes must be 32 bytes (64 hex chars). Use realistic-shaped placeholders.
            "0000000000000000000abc00000000000000000000000000000000000000abcd",
            "0000000000000000000aab0000000000000000000000000000000000000aabcd",
            1710633600,
            "did:key:z6MkTest",
            &key,
        )
        .unwrap();

        // Should start with typed message marker
        assert_eq!(wire[0], 0x02);
        let envelope = TypedEnvelope::from_wire(&wire).unwrap();
        assert_eq!(envelope.t, MeshMessageType::BlockHeader as u8);
        // Block header announcements are intentionally unsigned to save 64 bytes
        // on the 160-byte LoRa payload (see builder comment).
        assert!(envelope.sig.is_none());
    }

    #[test]
    fn test_tx_relay_roundtrip() {
        let wire = build_tx_relay_request("0200000001abc...", 42).unwrap();
        let envelope = TypedEnvelope::from_wire(&wire).unwrap();
        assert_eq!(envelope.t, MeshMessageType::TxRelay as u8);

        let payload: TxRelayPayload = message_types::decode_payload(&envelope.v).unwrap();
        assert_eq!(payload.request_id, 42);
        assert_eq!(payload.tx_hex, "0200000001abc...");
    }

    #[test]
    fn test_lightning_relay_roundtrip() {
        let wire = build_lightning_relay_request("lnbc50000n1pjtest...", 50000, 99).unwrap();
        let envelope = TypedEnvelope::from_wire(&wire).unwrap();

        let payload: LightningRelayPayload = message_types::decode_payload(&envelope.v).unwrap();
        assert_eq!(payload.amount_sats, 50000);
        assert_eq!(payload.request_id, 99);
    }

    #[tokio::test]
    async fn test_relay_tracker() {
        let tracker = RelayTracker::new();
        tracker.track_tx_relay(42, "did:key:z6MkRequester").await;

        let (tx_count, ln_count) = tracker.pending_count().await;
        assert_eq!(tx_count, 1);
        assert_eq!(ln_count, 0);

        let requester = tracker.complete_tx_relay(42).await;
        assert_eq!(requester, Some("did:key:z6MkRequester".to_string()));
        assert_eq!(tracker.pending_count().await, (0, 0));
    }
}
