// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Chunked message protocol with Reed-Solomon FEC for LoRa transport.
//!
//! Splits payloads larger than a single LoRa frame (160 bytes) into
//! numbered chunks with forward error correction, enabling reliable
//! transfer over lossy radio links.
//!
//! Chunk wire format (8 bytes header + payload):
//! ```text
//! [0x01: type] [msg_id: u32 LE] [chunk_idx: u8] [total: u8] [is_parity: u8] [payload...]
//! ```

use anyhow::{Context, Result};
use reed_solomon_erasure::galois_8::ReedSolomon;
use std::collections::HashMap;
use std::time::Instant;

/// Header size for each chunk frame.
const CHUNK_HEADER_SIZE: usize = 8;

/// Maximum payload per chunk after header.
/// 132 bytes available after ChaCha20-Poly1305 encryption overhead (12 nonce + 16 tag),
/// minus 8 byte chunk header = 124 bytes of user data per chunk.
pub const MAX_CHUNK_PAYLOAD: usize = 124;

/// Chunk type marker in the wire format.
const CHUNK_TYPE_MARKER: u8 = 0x01;

/// FEC redundancy ratio: 25% parity shards.
const FEC_RATIO_DENOMINATOR: usize = 4;

/// Maximum age of pending reassembly entries before garbage collection.
const REASSEMBLY_TIMEOUT_SECS: u64 = 60;

/// Maximum practical chunks for LoRa (airtime budget).
pub const MAX_PRACTICAL_CHUNKS: usize = 20;

/// A single chunk ready for transmission.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub message_id: u32,
    pub chunk_index: u8,
    pub total_chunks: u8,
    pub is_parity: bool,
    pub payload: Vec<u8>,
}

impl Chunk {
    /// Serialize chunk to wire format.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(CHUNK_HEADER_SIZE + self.payload.len());
        buf.push(CHUNK_TYPE_MARKER);
        buf.extend_from_slice(&self.message_id.to_le_bytes());
        buf.push(self.chunk_index);
        buf.push(self.total_chunks);
        buf.push(if self.is_parity { 1 } else { 0 });
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Parse chunk from wire format.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < CHUNK_HEADER_SIZE {
            anyhow::bail!("Chunk too small: {} bytes", data.len());
        }
        if data[0] != CHUNK_TYPE_MARKER {
            anyhow::bail!("Not a chunked message (marker: 0x{:02x})", data[0]);
        }
        let message_id = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
        let chunk_index = data[5];
        let total_chunks = data[6];
        let is_parity = data[7] != 0;
        let payload = data[CHUNK_HEADER_SIZE..].to_vec();

        Ok(Self {
            message_id,
            chunk_index,
            total_chunks,
            is_parity,
            payload,
        })
    }

    /// Check if a raw byte slice starts with the chunk type marker.
    pub fn is_chunked_message(data: &[u8]) -> bool {
        !data.is_empty() && data[0] == CHUNK_TYPE_MARKER
    }
}

/// Encode a payload into chunks with Reed-Solomon FEC parity.
///
/// Returns a vector of chunks ready for sequential transmission.
/// Each chunk's payload is exactly `shard_size` bytes (padded if needed).
pub fn encode_chunked(data: &[u8]) -> Result<Vec<Chunk>> {
    if data.is_empty() {
        anyhow::bail!("Cannot chunk empty data");
    }

    let shard_size = MAX_CHUNK_PAYLOAD;
    // Reserve the first 4 bytes of shard 0 for a length header so the
    // receiver can trim padding after FEC reconstruction. Effective
    // payload capacity is therefore (shards * shard_size) - 4.
    const LEN_HEADER: usize = 4;
    let total_payload = data.len() + LEN_HEADER;
    let data_shard_count = total_payload.div_ceil(shard_size);

    if data_shard_count > MAX_PRACTICAL_CHUNKS {
        anyhow::bail!(
            "Payload too large for LoRa chunking: {} bytes ({} chunks, max {})",
            data.len(),
            data_shard_count,
            MAX_PRACTICAL_CHUNKS
        );
    }

    let parity_shard_count = data_shard_count.div_ceil(FEC_RATIO_DENOMINATOR);
    let total_shards = data_shard_count + parity_shard_count;

    if total_shards > 255 {
        anyhow::bail!("Too many shards: {}", total_shards);
    }

    // Build a single contiguous buffer: [len_u32_le][data...][zero_padding]
    // then split into equal-size shards.
    let buffer_size = data_shard_count * shard_size;
    let mut buffer = vec![0u8; buffer_size];
    buffer[..LEN_HEADER].copy_from_slice(&(data.len() as u32).to_le_bytes());
    buffer[LEN_HEADER..LEN_HEADER + data.len()].copy_from_slice(data);

    let mut shards: Vec<Vec<u8>> = Vec::with_capacity(total_shards);
    for i in 0..data_shard_count {
        let start = i * shard_size;
        shards.push(buffer[start..start + shard_size].to_vec());
    }
    // Empty parity shards
    for _ in 0..parity_shard_count {
        shards.push(vec![0u8; shard_size]);
    }

    // Generate parity over the data shards (which now correctly include
    // the length header in shard 0).
    let rs = ReedSolomon::new(data_shard_count, parity_shard_count)
        .context("Failed to create Reed-Solomon codec")?;
    rs.encode(&mut shards)
        .context("Reed-Solomon encoding failed")?;

    // Build chunk frames
    // KEY-05: source named. A 4-byte frame correlator, not key material, so it is
    // drawn unguarded — the degenerate predicate's false-positive bound does not
    // hold below MIN_GUARDED_LEN.
    let message_id: u32 = rand::RngCore::next_u32(&mut rand::rngs::OsRng);
    let total = total_shards as u8;
    let mut chunks = Vec::with_capacity(total_shards);

    for (i, shard) in shards.into_iter().enumerate() {
        chunks.push(Chunk {
            message_id,
            chunk_index: i as u8,
            total_chunks: total,
            is_parity: i >= data_shard_count,
            payload: shard,
        });
    }

    Ok(chunks)
}

/// In-progress reassembly of a chunked message.
struct PendingMessage {
    shards: Vec<Option<Vec<u8>>>,
    data_shard_count: usize,
    parity_shard_count: usize,
    received_count: usize,
    created_at: Instant,
}

/// Reassembles chunked messages from incoming chunks.
pub struct ChunkReassembler {
    pending: HashMap<u32, PendingMessage>,
}

impl ChunkReassembler {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
        }
    }

    /// Feed a chunk into the reassembler.
    /// Returns `Some(data)` if the message is fully reconstructed.
    pub fn feed(&mut self, chunk: &Chunk) -> Result<Option<Vec<u8>>> {
        // Garbage collect stale entries
        self.pending
            .retain(|_, pm| pm.created_at.elapsed().as_secs() < REASSEMBLY_TIMEOUT_SECS);

        let total = chunk.total_chunks as usize;
        let entry = self.pending.entry(chunk.message_id).or_insert_with(|| {
            // Infer data vs parity count from chunks we've seen
            // The first non-parity chunk tells us the split point
            let data_count = if chunk.is_parity {
                // Best guess: 80% data, 20% parity
                (total * FEC_RATIO_DENOMINATOR) / (FEC_RATIO_DENOMINATOR + 1)
            } else {
                // We know this index is data — parity starts after all data
                // Exact split point: smallest i where chunk_index >= data_count AND is_parity
                total - (total + FEC_RATIO_DENOMINATOR) / (FEC_RATIO_DENOMINATOR + 1)
            };
            let parity_count = total - data_count;

            PendingMessage {
                shards: vec![None; total],
                data_shard_count: data_count,
                parity_shard_count: parity_count,
                received_count: 0,
                created_at: Instant::now(),
            }
        });

        let idx = chunk.chunk_index as usize;
        if idx >= total {
            anyhow::bail!("Chunk index {} out of range (total {})", idx, total);
        }

        if entry.shards[idx].is_none() {
            entry.shards[idx] = Some(chunk.payload.clone());
            entry.received_count += 1;
        }

        // Need at least data_shard_count shards to reconstruct
        if entry.received_count >= entry.data_shard_count {
            self.try_reconstruct(chunk.message_id)
        } else {
            Ok(None)
        }
    }

    fn try_reconstruct(&mut self, message_id: u32) -> Result<Option<Vec<u8>>> {
        let entry = match self.pending.get_mut(&message_id) {
            Some(e) => e,
            None => return Ok(None),
        };

        let rs = ReedSolomon::new(entry.data_shard_count, entry.parity_shard_count)
            .context("Failed to create Reed-Solomon codec for reconstruction")?;

        let mut shards: Vec<Option<Vec<u8>>> = entry.shards.clone();

        match rs.reconstruct(&mut shards) {
            Ok(()) => {
                // Concatenate data shards (not parity)
                let mut result = Vec::new();
                for data in shards.iter().take(entry.data_shard_count).flatten() {
                    result.extend_from_slice(data);
                }

                // Extract original length from first 4 bytes
                if result.len() < 4 {
                    anyhow::bail!("Reconstructed data too small for length header");
                }
                let original_len =
                    u32::from_le_bytes([result[0], result[1], result[2], result[3]]) as usize;

                // The actual data starts at byte 4 of the first shard
                // But wait — the length is embedded in shard 0 bytes 0..4, and the
                // actual payload starts at byte 4 of shard 0, then continues in subsequent shards.
                // Actually, encode_chunked puts the length in the first 4 bytes of shard 0,
                // and the rest of shard 0 + all other shards contain the original data.
                // So we need to skip 4 bytes from the beginning.
                if 4 + original_len > result.len() {
                    anyhow::bail!(
                        "Original length {} exceeds reconstructed data ({})",
                        original_len,
                        result.len() - 4
                    );
                }

                let data = result[4..4 + original_len].to_vec();
                self.pending.remove(&message_id);
                Ok(Some(data))
            }
            Err(_) => {
                // Not enough shards yet
                Ok(None)
            }
        }
    }
}

impl Default for ChunkReassembler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_roundtrip_small() {
        // Small payload fits in 1 data chunk + 1 parity chunk
        let data = b"Hello, mesh network!";
        let chunks = encode_chunked(data).unwrap();

        // 1 data + 1 parity = 2 chunks
        assert_eq!(chunks.len(), 2);
        assert!(!chunks[0].is_parity);
        assert!(chunks[1].is_parity);

        let mut reassembler = ChunkReassembler::new();
        // Feed data chunk — should reconstruct immediately (1 data shard needed)
        let result = reassembler.feed(&chunks[0]).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), data);
    }

    #[test]
    fn test_chunk_roundtrip_medium() {
        // 500 bytes payload + 4-byte length header = 504 bytes.
        // ceil(504 / 124) = 5 data shards, plus ceil(5/4) = 2 parity = 7 total.
        let data: Vec<u8> = (0..500).map(|i| (i % 256) as u8).collect();
        let chunks = encode_chunked(&data).unwrap();

        let data_chunks: Vec<_> = chunks.iter().filter(|c| !c.is_parity).collect();
        assert_eq!(data_chunks.len(), 5);

        let mut reassembler = ChunkReassembler::new();
        let mut result = None;
        for chunk in &chunks {
            if let Some(data) = reassembler.feed(chunk).unwrap() {
                result = Some(data);
                break;
            }
        }
        assert!(result.is_some());
        assert_eq!(result.unwrap(), data);
    }

    #[test]
    fn test_chunk_wire_format() {
        let chunk = Chunk {
            message_id: 0x12345678,
            chunk_index: 2,
            total_chunks: 5,
            is_parity: false,
            payload: vec![0xAA, 0xBB],
        };
        let bytes = chunk.to_bytes();
        assert_eq!(bytes[0], CHUNK_TYPE_MARKER);
        assert_eq!(&bytes[1..5], &0x12345678u32.to_le_bytes());
        assert_eq!(bytes[5], 2);
        assert_eq!(bytes[6], 5);
        assert_eq!(bytes[7], 0);
        assert_eq!(&bytes[8..], &[0xAA, 0xBB]);

        let parsed = Chunk::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.message_id, 0x12345678);
        assert_eq!(parsed.chunk_index, 2);
        assert_eq!(parsed.total_chunks, 5);
        assert!(!parsed.is_parity);
        assert_eq!(parsed.payload, vec![0xAA, 0xBB]);
    }

    #[test]
    fn test_chunk_is_chunked_message() {
        assert!(Chunk::is_chunked_message(&[0x01, 0x00]));
        assert!(!Chunk::is_chunked_message(&[0x02, 0x00]));
        assert!(!Chunk::is_chunked_message(&[]));
    }

    #[test]
    fn test_chunk_with_missing_chunk() {
        // Verify FEC can recover from a missing data chunk
        let data: Vec<u8> = (0..300).map(|i| (i % 256) as u8).collect();
        let chunks = encode_chunked(&data).unwrap();

        let mut reassembler = ChunkReassembler::new();
        // Skip chunk index 1 (simulate loss)
        for chunk in &chunks {
            if chunk.chunk_index == 1 {
                continue;
            }
            if let Some(recovered) = reassembler.feed(chunk).unwrap() {
                assert_eq!(recovered, data);
                return;
            }
        }
        panic!("Failed to reconstruct with one missing chunk");
    }

    #[test]
    fn test_empty_data_rejected() {
        assert!(encode_chunked(&[]).is_err());
    }

    #[test]
    fn test_too_large_rejected() {
        let data = vec![0u8; MAX_CHUNK_PAYLOAD * (MAX_PRACTICAL_CHUNKS + 1)];
        assert!(encode_chunked(&data).is_err());
    }
}
