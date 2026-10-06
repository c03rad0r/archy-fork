//! Buyer-side store of paid content the node has purchased.
//!
//! A paid peer download used to be ephemeral: the bytes were handed to the
//! browser as a one-shot `<a download>` and then thrown away. On the mobile
//! companion that download silently fails, so the item appeared to never
//! "unlock" even though the ecash was spent. This module persists every
//! successful purchase — bytes + metadata — keyed by (seller onion, content_id),
//! so the gallery can render owned items unblurred and play/view them in-app
//! from the local cache, with no re-payment and no reliance on a browser
//! download. The buyer can still save the file later from the cached copy.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncWriteExt, sync::Mutex};

static PURCHASE_WRITES: Mutex<()> = Mutex::const_new(());

const OWNED_DIR: &str = "purchased-content";
const OWNED_INDEX: &str = "owned.json";

/// One purchased item. `onion` + `content_id` are the identity; everything else
/// is display/metadata captured at purchase time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnedItem {
    pub onion: String,
    pub content_id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub paid_sats: u64,
    pub ecash_backend: String,
    /// RFC3339 timestamp; best-effort, empty if the clock was unavailable.
    pub purchased_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct OwnedIndex {
    items: Vec<OwnedItem>,
}

fn owned_root(data_dir: &Path) -> PathBuf {
    data_dir.join(OWNED_DIR)
}

fn index_path(data_dir: &Path) -> PathBuf {
    owned_root(data_dir).join(OWNED_INDEX)
}

/// Sanitize an onion into a safe directory component (it's already [a-z2-7].onion
/// for valid v3, but be defensive against path traversal regardless).
fn sanitize(component: &str) -> String {
    component
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn bytes_path(data_dir: &Path, onion: &str, content_id: &str) -> PathBuf {
    owned_root(data_dir)
        .join(sanitize(onion))
        .join(sanitize(content_id))
}

async fn load_index_checked(data_dir: &Path) -> Result<OwnedIndex> {
    match fs::read_to_string(index_path(data_dir)).await {
        Ok(s) => serde_json::from_str(&s)
            .context("Invalid purchase index; existing records were preserved"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(OwnedIndex::default()),
        Err(error) => Err(error).context("Reading purchase index"),
    }
}

async fn load_index(data_dir: &Path) -> OwnedIndex {
    load_index_checked(data_dir).await.unwrap_or_default()
}

async fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("Purchase path has no parent")?;
    fs::create_dir_all(parent).await?;
    let temp = parent.join(format!(".purchase-{}.tmp", uuid::Uuid::new_v4()));
    let result = async {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .await?;
        file.write_all(bytes).await?;
        file.sync_all().await?;
        drop(file);
        fs::rename(&temp, path).await?;
        fs::File::open(parent).await?.sync_all().await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&temp).await;
    }
    result
}

async fn save_index(data_dir: &Path, index: &OwnedIndex) -> Result<()> {
    let root = owned_root(data_dir);
    fs::create_dir_all(&root)
        .await
        .with_context(|| format!("creating {}", root.display()))?;
    let content = serde_json::to_string_pretty(index).context("serializing owned index")?;
    atomic_write(&index_path(data_dir), content.as_bytes())
        .await
        .context("writing owned index")
}

/// Persist a successful purchase: write the bytes to disk and upsert the index
/// entry. Idempotent on (onion, content_id) — re-buying overwrites with the
/// latest copy/metadata rather than duplicating.
pub async fn record_purchase(
    data_dir: &Path,
    onion: &str,
    content_id: &str,
    filename: &str,
    mime_type: &str,
    bytes: &[u8],
    paid_sats: u64,
    ecash_backend: &str,
    purchased_at: &str,
) -> Result<()> {
    // Read-modify-write must be one serialized transaction. Never replace a
    // damaged index with an empty one, and never expose partially written bytes.
    let _lock = PURCHASE_WRITES.lock().await;
    let mut index = load_index_checked(data_dir).await?;
    let path = bytes_path(data_dir, onion, content_id);
    atomic_write(&path, bytes)
        .await
        .with_context(|| format!("writing purchased bytes to {}", path.display()))?;

    let entry = OwnedItem {
        onion: onion.to_string(),
        content_id: content_id.to_string(),
        filename: filename.to_string(),
        mime_type: mime_type.to_string(),
        size_bytes: bytes.len() as u64,
        paid_sats,
        ecash_backend: ecash_backend.to_string(),
        purchased_at: purchased_at.to_string(),
    };
    if let Some(existing) = index
        .items
        .iter_mut()
        .find(|i| i.onion == onion && i.content_id == content_id)
    {
        *existing = entry;
    } else {
        index.items.push(entry);
    }
    save_index(data_dir, &index).await
}

/// Payment decisions must not interpret an unreadable index as no purchases.
pub async fn list_owned_checked(data_dir: &Path) -> Result<Vec<OwnedItem>> {
    Ok(load_index_checked(data_dir).await?.items)
}

/// Every item this node owns.
pub async fn list_owned(data_dir: &Path) -> Vec<OwnedItem> {
    load_index(data_dir).await.items
}

/// True if the node has already purchased this (onion, content_id).
#[allow(dead_code)] // used by the upcoming seller-side signed-entitlement path (#8)
pub async fn is_owned(data_dir: &Path, onion: &str, content_id: &str) -> bool {
    bytes_path(data_dir, onion, content_id).exists()
        && load_index(data_dir)
            .await
            .items
            .iter()
            .any(|i| i.onion == onion && i.content_id == content_id)
}

/// Read a purchased item's bytes + mime type from the local cache, if present.
pub async fn open_owned(
    data_dir: &Path,
    onion: &str,
    content_id: &str,
) -> Result<Option<(String, fs::File)>> {
    let index = load_index_checked(data_dir).await?;
    let Some(item) = index
        .items
        .iter()
        .find(|item| item.onion == onion && item.content_id == content_id)
    else {
        return Ok(None);
    };
    // Reject path components even when called outside the HTTP route.
    anyhow::ensure!(
        !onion.is_empty()
            && !content_id.is_empty()
            && onion != "."
            && content_id != "."
            && !onion.contains("..")
            && !content_id.contains("..")
            && sanitize(onion) == onion
            && sanitize(content_id) == content_id,
        "Invalid purchase path"
    );
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(bytes_path(data_dir, onion, content_id))
        .await
        .context("Purchased bytes unavailable")?;
    let metadata = file.metadata().await?;
    anyhow::ensure!(
        metadata.is_file() && metadata.len() == item.size_bytes,
        "Purchased bytes incomplete"
    );
    Ok(Some((item.mime_type.clone(), file)))
}

pub async fn read_owned(
    data_dir: &Path,
    onion: &str,
    content_id: &str,
) -> Option<(String, Vec<u8>)> {
    let bytes = fs::read(bytes_path(data_dir, onion, content_id))
        .await
        .ok()?;
    let mime = load_index(data_dir)
        .await
        .items
        .into_iter()
        .find(|i| i.onion == onion && i.content_id == content_id)
        .map(|i| i.mime_type)
        .unwrap_or_else(|| "application/octet-stream".to_string());
    Some((mime, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn owned_stream_preserves_ownership_on_missing_corrupt_or_symlinked_bytes() {
        let dir = tempfile::tempdir().unwrap();
        record_purchase(
            dir.path(),
            "seller.onion",
            "video",
            "video",
            "video/mp4",
            b"video",
            1,
            "cashu",
            "now",
        )
        .await
        .unwrap();
        assert!(open_owned(dir.path(), "seller.onion", "video")
            .await
            .unwrap()
            .is_some());
        let path = bytes_path(dir.path(), "seller.onion", "video");
        fs::write(&path, b"bad").await.unwrap();
        assert!(open_owned(dir.path(), "seller.onion", "video")
            .await
            .is_err());
        fs::remove_file(&path).await.unwrap();
        assert!(open_owned(dir.path(), "seller.onion", "video")
            .await
            .is_err());
        let private = dir.path().join("private");
        fs::write(&private, b"other").await.unwrap();
        std::os::unix::fs::symlink(&private, &path).unwrap();
        assert!(open_owned(dir.path(), "seller.onion", "video")
            .await
            .is_err());
        assert_eq!(list_owned_checked(dir.path()).await.unwrap().len(), 1);
        assert!(open_owned(dir.path(), "seller.onion", "not-bought")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn concurrent_purchases_preserve_every_item_and_exact_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let mut jobs = tokio::task::JoinSet::new();
        for n in 0..24 {
            let root = dir.path().to_path_buf();
            jobs.spawn(async move {
                let id = format!("file-{n}");
                record_purchase(
                    &root,
                    "seller.onion",
                    &id,
                    &id,
                    "text/plain",
                    id.as_bytes(),
                    5,
                    "lightning",
                    "now",
                )
                .await
                .unwrap();
            });
        }
        while let Some(result) = jobs.join_next().await {
            result.unwrap();
        }
        assert_eq!(list_owned(dir.path()).await.len(), 24);
        for n in 0..24 {
            let id = format!("file-{n}");
            assert!(is_owned(dir.path(), "seller.onion", &id).await);
            let (mime, bytes) = read_owned(dir.path(), "seller.onion", &id).await.unwrap();
            assert_eq!(mime, "text/plain");
            assert_eq!(bytes, id.as_bytes());
        }
        record_purchase(
            dir.path(),
            "seller.onion",
            "file-0",
            "file-0",
            "text/plain",
            b"updated",
            5,
            "lightning",
            "later",
        )
        .await
        .unwrap();
        assert_eq!(list_owned(dir.path()).await.len(), 24);
        assert_eq!(
            read_owned(dir.path(), "seller.onion", "file-0")
                .await
                .unwrap()
                .1,
            b"updated"
        );
    }
    #[tokio::test]
    async fn damaged_index_is_preserved_instead_of_erasing_prior_ownership() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(owned_root(dir.path())).await.unwrap();
        fs::write(index_path(dir.path()), b"damaged but preserve me")
            .await
            .unwrap();
        assert!(record_purchase(
            dir.path(),
            "seller.onion",
            "new",
            "new",
            "text/plain",
            b"bytes",
            5,
            "lightning",
            "now"
        )
        .await
        .is_err());
        assert_eq!(
            fs::read(index_path(dir.path())).await.unwrap(),
            b"damaged but preserve me"
        );
        assert!(!bytes_path(dir.path(), "seller.onion", "new").exists());
    }
}
