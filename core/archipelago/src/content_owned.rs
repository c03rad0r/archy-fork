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

// Serialize purchases per seller so duplicate UI requests cannot both pass the
// ownership check and spend. Weak entries are pruned between acquisitions.
pub async fn lock_seller_purchases(onion: &str) -> tokio::sync::OwnedMutexGuard<()> {
    use std::sync::{Arc, OnceLock, Weak};
    static LOCKS: OnceLock<std::sync::Mutex<std::collections::HashMap<String, Weak<Mutex<()>>>>> =
        OnceLock::new();
    let lock = {
        let mut locks = LOCKS
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        locks.retain(|_, value| value.strong_count() > 0);
        if let Some(lock) = locks.get(onion).and_then(Weak::upgrade) {
            lock
        } else {
            let lock = Arc::new(Mutex::new(()));
            locks.insert(onion.into(), Arc::downgrade(&lock));
            lock
        }
    };
    lock.lock_owned().await
}

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
    /// False means payment succeeded but delivery still needs recovery.
    #[serde(default = "completed")]
    pub download_complete: bool,
}

fn completed() -> bool {
    true
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

struct PendingFile(PathBuf);
impl Drop for PendingFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub fn validate_identity(onion: &str, content_id: &str) -> Result<()> {
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
    Ok(())
}

async fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("Purchase path has no parent")?;
    fs::create_dir_all(parent).await?;
    let temp = PendingFile(parent.join(format!(".purchase-{}.tmp", uuid::Uuid::new_v4())));
    let result = async {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp.0)
            .await?;
        file.write_all(bytes).await?;
        file.sync_all().await?;
        drop(file);
        fs::rename(&temp.0, path).await?;
        fs::File::open(parent).await?.sync_all().await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&temp.0).await;
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
    validate_identity(onion, content_id)?;
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
        download_complete: true,
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

fn require_cache_space(file: &fs::File, additional: u64) -> Result<()> {
    use std::os::fd::AsRawFd;
    let mut value = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::fstatvfs(file.as_raw_fd(), value.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error()).context("Checking purchase storage");
    }
    let value = unsafe { value.assume_init() };
    let available = (value.f_bavail as u64).saturating_mul(value.f_frsize as u64);
    anyhow::ensure!(
        additional
            .checked_add(256 * 1024 * 1024)
            .is_some_and(|n| n <= available),
        "Insufficient purchase storage; payment remains recorded for recovery"
    );
    Ok(())
}

/// Save a peer response without holding its body in memory. A durable incomplete
/// ownership record precedes body consumption, so interrupted delivery cannot be
/// mistaken for permission to send another payment. Invoice retries may complete it.
pub async fn record_purchase_stream<S, E>(
    data_dir: &Path,
    mut entry: OwnedItem,
    mut stream: S,
    expected: Option<u64>,
) -> Result<OwnedItem>
where
    S: futures_util::Stream<Item = std::result::Result<bytes::Bytes, E>> + Unpin,
    E: std::error::Error + Send + Sync + 'static,
{
    use futures_util::StreamExt;
    validate_identity(&entry.onion, &entry.content_id)?;
    let path = bytes_path(data_dir, &entry.onion, &entry.content_id);
    let parent = path.parent().context("Purchase has no parent")?;
    fs::create_dir_all(parent).await?;
    let temporary = PendingFile(parent.join(format!(".purchase-{}.tmp", uuid::Uuid::new_v4())));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary.0)
        .await?;
    entry.download_complete = false;
    entry.size_bytes = expected.unwrap_or(0);
    {
        let _lock = PURCHASE_WRITES.lock().await;
        let mut index = load_index_checked(data_dir).await?;
        if let Some(old) = index
            .items
            .iter_mut()
            .find(|i| i.onion == entry.onion && i.content_id == entry.content_id)
        {
            anyhow::ensure!(
                !old.download_complete,
                "Purchase is already cached; use the owned copy"
            );
            *old = entry.clone();
        } else {
            index.items.push(entry.clone());
        }
        save_index(data_dir, &index).await?;
    }
    require_cache_space(&file, expected.unwrap_or(0))?;
    let mut received = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.context("Purchased file transfer interrupted; no new payment should be sent")?;
        received = received
            .checked_add(chunk.len() as u64)
            .context("Content size overflow")?;
        anyhow::ensure!(
            expected.is_none_or(|n| received <= n),
            "Seller exceeded the declared content length"
        );
        require_cache_space(&file, chunk.len() as u64)?;
        for part in chunk.chunks(64 * 1024) {
            file.write_all(part).await?;
        }
    }
    anyhow::ensure!(
        expected.is_none_or(|n| received == n),
        "Purchased file transfer ended early"
    );
    file.sync_all().await?;
    drop(file);
    let _lock = PURCHASE_WRITES.lock().await;
    let mut index = load_index_checked(data_dir).await?;
    fs::rename(&temporary.0, &path).await?;
    fs::File::open(parent).await?.sync_all().await?;
    entry.size_bytes = received;
    entry.download_complete = true;
    if let Some(old) = index
        .items
        .iter_mut()
        .find(|i| i.onion == entry.onion && i.content_id == entry.content_id)
    {
        *old = entry.clone();
    } else {
        anyhow::bail!("Purchase ownership record disappeared; no new payment should be sent");
    }
    save_index(data_dir, &index).await?;
    Ok(entry)
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
    validate_identity(onion, content_id)?;
    anyhow::ensure!(
        item.download_complete,
        "Payment is recorded, but delivery is incomplete. Retry delivery without paying again."
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
    use tokio::io::AsyncReadExt;
    let (mime, mut file) = open_owned(data_dir, onion, content_id).await.ok()??;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).await.ok()?;
    Some((mime, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry() -> OwnedItem {
        OwnedItem {
            onion: "seller.onion".into(),
            content_id: "film".into(),
            filename: "film.mp4".into(),
            mime_type: "video/mp4".into(),
            size_bytes: 0,
            paid_sats: 1,
            ecash_backend: "lightning".into(),
            purchased_at: "now".into(),
            download_complete: false,
        }
    }
    #[tokio::test]
    async fn stream_records_incomplete_delivery_then_recovers_without_losing_ownership() {
        let dir = tempfile::tempdir().unwrap();
        let short = futures_util::stream::iter([Ok::<_, std::io::Error>(
            bytes::Bytes::from_static(b"part"),
        )]);
        assert!(record_purchase_stream(dir.path(), entry(), short, Some(10))
            .await
            .is_err());
        let pending = list_owned_checked(dir.path()).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert!(!pending[0].download_complete);
        assert!(open_owned(dir.path(), "seller.onion", "film")
            .await
            .is_err());
        assert_eq!(
            std::fs::read_dir(owned_root(dir.path()).join("seller.onion"))
                .unwrap()
                .count(),
            0
        );
        let chunks = futures_util::stream::iter(
            (0..64).map(|_| Ok::<_, std::io::Error>(bytes::Bytes::from(vec![42; 65536]))),
        );
        let item = record_purchase_stream(dir.path(), entry(), chunks, Some(4 * 1024 * 1024))
            .await
            .unwrap();
        assert!(item.download_complete);
        let (_, bytes) = read_owned(dir.path(), "seller.onion", "film")
            .await
            .unwrap();
        assert_eq!(bytes.len(), 4 * 1024 * 1024);
        assert!(bytes.iter().all(|b| *b == 42));
        assert_eq!(list_owned_checked(dir.path()).await.unwrap().len(), 1);
    }
    #[tokio::test]
    async fn cancelled_stream_cleans_partial_bytes_but_keeps_payment_record() {
        let dir = tempfile::tempdir().unwrap();
        let started = std::sync::Arc::new(tokio::sync::Notify::new());
        let notify = started.clone();
        let root = dir.path().to_path_buf();
        let task = tokio::spawn(async move {
            let stream = Box::pin(futures_util::stream::once(async move {
                notify.notify_one();
                std::future::pending::<std::result::Result<bytes::Bytes, std::io::Error>>().await
            }));
            record_purchase_stream(&root, entry(), stream, Some(10)).await
        });
        tokio::time::timeout(std::time::Duration::from_secs(10), started.notified())
            .await
            .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(!list_owned_checked(dir.path()).await.unwrap()[0].download_complete);
        assert_eq!(
            std::fs::read_dir(owned_root(dir.path()).join("seller.onion"))
                .unwrap()
                .count(),
            0
        );
    }
    #[tokio::test]
    async fn unsafe_purchase_paths_are_rejected_before_writing() {
        let dir = tempfile::tempdir().unwrap();
        for id in ["..", "../private", "/absolute", "a/b"] {
            assert!(record_purchase(
                dir.path(),
                "seller.onion",
                id,
                "x",
                "x",
                b"x",
                1,
                "cashu",
                "now"
            )
            .await
            .is_err());
        }
        assert!(!owned_root(dir.path()).exists());
    }
    #[tokio::test]
    async fn seller_purchase_lock_blocks_duplicates_but_not_another_seller() {
        let first = lock_seller_purchases("one.onion").await;
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(10),
            lock_seller_purchases("one.onion")
        )
        .await
        .is_err());
        let other = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            lock_seller_purchases("two.onion"),
        )
        .await
        .unwrap();
        drop(first);
        let _next = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            lock_seller_purchases("one.onion"),
        )
        .await
        .unwrap();
        drop(other);
    }

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
