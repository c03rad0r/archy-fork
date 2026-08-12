//! Content-addressed blob store for attachments shared over mesh/federation.
//!
//! Blobs live at `${data_dir}/blobs/<cid>` where `cid` is the hex-encoded
//! SHA-256 of the content. A sibling `<cid>.meta` file holds JSON metadata
//! (mime, filename, size, created_at). Capability URLs are HMAC-signed tokens
//! scoped to a recipient pubkey and expiry, verified before serving.

use anyhow::{anyhow, Context, Result};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

type HmacSha256 = Hmac<Sha256>;

/// Default capability URL validity window.
pub const DEFAULT_CAP_TTL_SECS: u64 = 7 * 24 * 60 * 60;

/// Maximum blob size accepted by the store (64 MiB). Keep attachments
/// reasonable so /var/lib/archipelago doesn't balloon unnoticed.
pub const MAX_BLOB_SIZE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobMeta {
    pub cid: String,
    /// DHT Phase 1: BLAKE3 hash of the content (iroh-native swarm address).
    /// The on-disk path stays SHA-256-keyed (`cid`) for back-compat; this
    /// advertises the hash a peer swarm can fetch/range-verify by. Absent in
    /// legacy metadata written before Phase 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blake3: Option<String>,
    pub size: u64,
    pub mime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    pub created_at: String,
    /// Optional raw thumbnail bytes (small — up to ~60 bytes is LoRa-safe).
    /// Stored alongside meta so ContentRef senders don't re-fetch the blob.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_bytes: Option<Vec<u8>>,
    /// Public blobs (profile pictures, banners) are served at `/blob/<cid>`
    /// without a capability check so external Nostr clients can fetch them.
    /// Missing in legacy metadata = default false (cap required).
    #[serde(default)]
    pub public: bool,
}

pub struct BlobStore {
    root: PathBuf,
    /// HMAC key used to sign capability URLs. Derived from node identity;
    /// callers pass it in so we don't duplicate key management here.
    cap_key: [u8; 32],
}

impl BlobStore {
    /// Create (or open) a blob store rooted at `data_dir/blobs`.
    pub async fn open(data_dir: &Path, cap_key: [u8; 32]) -> Result<Self> {
        let root = data_dir.join("blobs");
        fs::create_dir_all(&root)
            .await
            .context("create blobs dir")?;
        Ok(Self { root, cap_key })
    }

    fn path_for(&self, cid: &str) -> PathBuf {
        self.root.join(cid)
    }

    fn meta_path_for(&self, cid: &str) -> PathBuf {
        self.root.join(format!("{}.meta", cid))
    }

    /// Write bytes to the store, returning the CID and metadata. Idempotent:
    /// identical bytes produce the same CID and short-circuit re-writes.
    pub async fn put(
        &self,
        bytes: &[u8],
        mime: &str,
        filename: Option<String>,
        thumb_bytes: Option<Vec<u8>>,
        public: bool,
    ) -> Result<BlobMeta> {
        if bytes.len() as u64 > MAX_BLOB_SIZE {
            anyhow::bail!(
                "Blob too large: {} bytes (max {})",
                bytes.len(),
                MAX_BLOB_SIZE
            );
        }
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let cid = hex::encode(hasher.finalize());
        let meta = BlobMeta {
            cid: cid.clone(),
            blake3: Some(crate::content_hash::blake3_hex(bytes)),
            size: bytes.len() as u64,
            mime: mime.to_string(),
            filename,
            created_at: chrono::Utc::now().to_rfc3339(),
            thumb_bytes,
            public,
        };

        let blob_path = self.path_for(&cid);
        if !blob_path.exists() {
            let mut f = fs::File::create(&blob_path).await.context("create blob")?;
            f.write_all(bytes).await.context("write blob")?;
            f.sync_all().await.ok();
        }
        let meta_json = serde_json::to_vec(&meta)?;
        fs::write(self.meta_path_for(&cid), meta_json)
            .await
            .context("write blob meta")?;
        Ok(meta)
    }

    /// Read raw bytes for a CID. Errors if missing.
    pub async fn get(&self, cid: &str) -> Result<Vec<u8>> {
        let path = self.path_for(cid);
        fs::read(&path)
            .await
            .with_context(|| format!("blob not found: {}", cid))
    }

    /// Load metadata for a CID.
    pub async fn meta(&self, cid: &str) -> Result<BlobMeta> {
        let raw = fs::read(self.meta_path_for(cid))
            .await
            .with_context(|| format!("blob meta not found: {}", cid))?;
        Ok(serde_json::from_slice(&raw)?)
    }

    /// Check whether a CID is held locally.
    pub async fn has(&self, cid: &str) -> bool {
        fs::try_exists(self.path_for(cid)).await.unwrap_or(false)
    }

    /// Sign a capability token: HMAC-SHA256(cid || peer_pubkey || expiry).
    /// Returned token is hex — callers append `?cap=<token>&exp=<epoch>` to
    /// the blob URL sent to the peer.
    pub fn issue_capability(&self, cid: &str, peer_pubkey_hex: &str, expiry_epoch: u64) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.cap_key).expect("hmac key");
        mac.update(cid.as_bytes());
        mac.update(b"|");
        mac.update(peer_pubkey_hex.as_bytes());
        mac.update(b"|");
        mac.update(&expiry_epoch.to_be_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    /// Verify a capability token against (cid, peer_pubkey, expiry).
    /// Returns Ok(()) on success, Err describing the failure otherwise.
    /// Expired tokens fail even with a correct signature.
    pub fn verify_capability(
        &self,
        cid: &str,
        peer_pubkey_hex: &str,
        expiry_epoch: u64,
        token_hex: &str,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp() as u64;
        if expiry_epoch < now {
            return Err(anyhow!("capability expired"));
        }
        let expected = self.issue_capability(cid, peer_pubkey_hex, expiry_epoch);
        // Constant-time compare via HMAC verify.
        let token_bytes =
            hex::decode(token_hex).map_err(|_| anyhow!("capability token not hex"))?;
        let expected_bytes = hex::decode(&expected).unwrap();
        if token_bytes.len() != expected_bytes.len() {
            return Err(anyhow!("capability length mismatch"));
        }
        // hmac::Mac::verify is the idiomatic constant-time path, but we
        // already computed `expected` so fall back to ct_eq via subtle.
        let mut diff = 0u8;
        for (a, b) in token_bytes.iter().zip(expected_bytes.iter()) {
            diff |= a ^ b;
        }
        if diff == 0 {
            Ok(())
        } else {
            Err(anyhow!("capability signature mismatch"))
        }
    }
}
