//! Node-signed byte indexes, created only by a scan matching the producer's
//! original signed full SHA. Metadata alone never authorizes streamed bytes.
use crate::identity::NodeIdentity;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};
pub(crate) const CHUNK_BYTES: usize = 64 * 1024;
const MAX_BYTES: u64 = 16 * 1024 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Binding {
    pub content_id: String,
    pub receipt_sha256: String,
    pub full_sha256: String,
    pub size: u64,
}
impl Binding {
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.content_id
                .strip_prefix("registered_")
                .and_then(|id| uuid::Uuid::parse_str(id).ok())
                .map(|id| format!("registered_{id}") == self.content_id)
                .unwrap_or(false)
                && self.size > 0
                && self.size <= MAX_BYTES,
            "Invalid byte index binding"
        );
        for value in [&self.receipt_sha256, &self.full_sha256] {
            anyhow::ensure!(
                value.len() == 64
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "Invalid byte index digest"
            );
        }
        Ok(())
    }
    pub fn index_bytes(&self) -> Result<u64> {
        self.validate()?;
        Ok(self.size.div_ceil(CHUNK_BYTES as u64) * 32)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    version: u32,
    binding: Binding,
    chunks_sha256: String,
    signature: String,
}
fn preimage(binding: &Binding, chunks_sha256: &str) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&(
        "archipelago-rental-chunk-index-v1",
        CHUNK_BYTES,
        &binding.content_id,
        &binding.receipt_sha256,
        &binding.full_sha256,
        binding.size,
        chunks_sha256,
    ))?)
}
pub(crate) struct Index {
    pub binding: Binding,
    chunks: Vec<[u8; 32]>,
    pub commitment: String,
}
#[cfg(test)]
fn scan_counts() -> &'static std::sync::Mutex<std::collections::HashMap<String, usize>> {
    static COUNTS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, usize>>> =
        std::sync::OnceLock::new();
    COUNTS.get_or_init(Default::default)
}
#[cfg(test)]
pub(crate) fn scans(content_id: &str) -> usize {
    *scan_counts().lock().unwrap().get(content_id).unwrap_or(&0)
}
impl Index {
    /// Exactly one whole-file scan. Caller runs it as a bounded background job;
    /// progress/cancellation cannot create or alter a purchase lease.
    pub fn scan(
        file: &mut File,
        binding: Binding,
        mut progress: impl FnMut(u64) -> Result<()>,
    ) -> Result<Self> {
        let bytes = binding.index_bytes()?;
        #[cfg(test)]
        {
            *scan_counts()
                .lock()
                .unwrap()
                .entry(binding.content_id.clone())
                .or_default() += 1;
        }
        anyhow::ensure!(
            file.metadata()?.is_file() && file.metadata()?.len() == binding.size,
            "Snapshot size changed"
        );
        file.seek(SeekFrom::Start(0))?;
        let mut chunks = Vec::with_capacity(bytes as usize / 32);
        let mut full = Sha256::new();
        let mut buffer = [0u8; CHUNK_BYTES];
        let mut read = 0;
        while read < binding.size {
            progress(read)?;
            let count = (binding.size - read).min(CHUNK_BYTES as u64) as usize;
            file.read_exact(&mut buffer[..count])?;
            full.update(&buffer[..count]);
            chunks.push(Sha256::digest(&buffer[..count]).into());
            read += count as u64;
        }
        anyhow::ensure!(
            file.metadata()?.len() == binding.size
                && hex::encode(full.finalize()) == binding.full_sha256,
            "Registered snapshot content changed"
        );
        progress(read)?;
        let mut digest = Sha256::new();
        for chunk in &chunks {
            digest.update(chunk);
        }
        let commitment = hex::encode(digest.finalize());
        file.seek(SeekFrom::Start(0))?;
        Ok(Self {
            binding,
            chunks,
            commitment,
        })
    }
    /// Persist under the registration's existing verification flock. Sync rename
    /// is the commit point; no mutation is queued after a canceled future drops.
    pub fn save(&self, path: &Path, identity: &NodeIdentity) -> Result<()> {
        let header = Header {
            version: 1,
            binding: self.binding.clone(),
            chunks_sha256: self.commitment.clone(),
            signature: identity.sign(&preimage(&self.binding, &self.commitment)?),
        };
        let encoded = serde_json::to_vec(&header)?;
        anyhow::ensure!(encoded.len() <= 4096, "Byte index header too large");
        let parent = path.parent().context("Byte index directory missing")?;
        let temporary = parent.join(format!(".chunk-index-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&temporary)?;
            file.write_all(&(encoded.len() as u32).to_be_bytes())?;
            file.write_all(&encoded)?;
            for chunk in &self.chunks {
                file.write_all(chunk)?;
            }
            file.sync_all()?;
            std::fs::rename(&temporary, path)?;
            File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
    /// A signed index survives restart without treating an editable metadata
    /// cache as proof. Every delivered chunk is independently checked below.
    pub fn load(path: &Path, binding: &Binding, identity: &NodeIdentity) -> Result<Option<Self>> {
        let expected = binding.index_bytes()?;
        let mut file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let metadata = file.metadata()?;
        anyhow::ensure!(
            metadata.is_file() && metadata.len() <= expected + 4100,
            "Invalid byte index file"
        );
        let mut len = [0u8; 4];
        file.read_exact(&mut len)?;
        let len = u32::from_be_bytes(len) as usize;
        anyhow::ensure!(
            len <= 4096 && metadata.len() == 4 + len as u64 + expected,
            "Invalid byte index length"
        );
        let mut header = vec![0; len];
        file.read_exact(&mut header)?;
        let header: Header = serde_json::from_slice(&header)?;
        anyhow::ensure!(
            header.version == 1
                && &header.binding == binding
                && NodeIdentity::verify(
                    &identity.pubkey_hex(),
                    &preimage(binding, &header.chunks_sha256)?,
                    &header.signature
                )?,
            "Byte index signature or immutable binding changed"
        );
        let mut chunks = Vec::with_capacity(expected as usize / 32);
        let mut digest = Sha256::new();
        for _ in 0..expected / 32 {
            let mut chunk = [0; 32];
            file.read_exact(&mut chunk)?;
            digest.update(chunk);
            chunks.push(chunk);
        }
        anyhow::ensure!(
            hex::encode(digest.finalize()) == header.chunks_sha256,
            "Byte index was altered"
        );
        Ok(Some(Self {
            binding: binding.clone(),
            chunks,
            commitment: header.chunks_sha256,
        }))
    }
    /// Return only a slice of a completely verified aligned chunk. This bounds
    /// seek/range work to64KiB and detects same-inode/same-stamp byte changes.
    pub fn read_slice(&self, file: &mut File, start: u64, requested: usize) -> Result<Vec<u8>> {
        anyhow::ensure!(
            start < self.binding.size
                && requested > 0
                && file.metadata()?.len() == self.binding.size,
            "Snapshot range or size changed"
        );
        let chunk = start / CHUNK_BYTES as u64;
        let aligned = chunk * CHUNK_BYTES as u64;
        let size = (self.binding.size - aligned).min(CHUNK_BYTES as u64) as usize;
        let mut bytes = vec![0; size];
        file.seek(SeekFrom::Start(aligned))?;
        file.read_exact(&mut bytes)?;
        let actual: [u8; 32] = Sha256::digest(&bytes).into();
        anyhow::ensure!(
            self.chunks.get(chunk as usize) == Some(&actual),
            "Registered snapshot chunk changed; recover original entitlement"
        );
        let offset = (start - aligned) as usize;
        Ok(bytes[offset..offset + requested.min(size - offset)].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding(bytes: &[u8]) -> Binding {
        Binding {
            content_id: format!("registered_{}", uuid::Uuid::new_v4()),
            receipt_sha256: "ab".repeat(32),
            full_sha256: hex::encode(Sha256::digest(bytes)),
            size: bytes.len() as u64,
        }
    }
    #[tokio::test]
    async fn signed_index_reopens_without_whole_scan_and_seek_slices_reject_mutation() {
        let root = tempfile::tempdir().unwrap();
        let identity = NodeIdentity::load_or_create(&root.path().join("identity"))
            .await
            .unwrap();
        let bytes: Vec<u8> = (0..CHUNK_BYTES * 5 + 19).map(|n| (n % 251) as u8).collect();
        let media = root.path().join("media");
        std::fs::write(&media, &bytes).unwrap();
        let mut file = File::open(&media).unwrap();
        let mut progress = Vec::new();
        let original = binding(&bytes);
        let index = Index::scan(&mut file, original.clone(), |n| {
            progress.push(n);
            Ok(())
        })
        .unwrap();
        assert_eq!(progress.last(), Some(&(bytes.len() as u64)));
        index.save(&root.path().join("index"), &identity).unwrap();
        drop(index);
        let loaded = Index::load(&root.path().join("index"), &original, &identity)
            .unwrap()
            .unwrap();
        for start in [0, CHUNK_BYTES + 7, CHUNK_BYTES * 4, bytes.len() - 1] {
            let got = loaded.read_slice(&mut file, start as u64, 37).unwrap();
            assert_eq!(got, bytes[start..start + got.len()]);
        }
        let mut corrupt = bytes.clone();
        corrupt[CHUNK_BYTES + 9] ^= 1;
        std::fs::write(&media, corrupt).unwrap();
        assert!(loaded
            .read_slice(&mut file, (CHUNK_BYTES + 7) as u64, 1)
            .is_err());
        assert_eq!(loaded.read_slice(&mut file, 0, 5).unwrap(), &bytes[..5]);
        let mut saved = std::fs::read(root.path().join("index")).unwrap();
        *saved.last_mut().unwrap() ^= 1;
        std::fs::write(root.path().join("index"), saved).unwrap();
        assert!(Index::load(&root.path().join("index"), &original, &identity).is_err());
    }
    #[test]
    fn canceled_or_wrong_hash_scan_never_produces_index() {
        let root = tempfile::tempdir().unwrap();
        let bytes = vec![7; CHUNK_BYTES * 2];
        let media = root.path().join("media");
        std::fs::write(&media, &bytes).unwrap();
        let mut file = File::open(media).unwrap();
        assert!(Index::scan(&mut file, binding(&bytes), |n| {
            anyhow::ensure!(n == 0, "canceled");
            Ok(())
        })
        .is_err());
        let mut wrong = binding(&bytes);
        wrong.full_sha256 = "cd".repeat(32);
        assert!(Index::scan(&mut file, wrong, |_| Ok(())).is_err());
    }
}
