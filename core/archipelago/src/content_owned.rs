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
use tokio::fs;

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

async fn load_index(data_dir: &Path) -> OwnedIndex {
    match fs::read_to_string(index_path(data_dir)).await {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => OwnedIndex::default(),
    }
}

async fn save_index(data_dir: &Path, index: &OwnedIndex) -> Result<()> {
    let root = owned_root(data_dir);
    fs::create_dir_all(&root)
        .await
        .with_context(|| format!("creating {}", root.display()))?;
    let content = serde_json::to_string_pretty(index).context("serializing owned index")?;
    fs::write(index_path(data_dir), content)
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
    let path = bytes_path(data_dir, onion, content_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(&path, bytes)
        .await
        .with_context(|| format!("writing purchased bytes to {}", path.display()))?;

    let mut index = load_index(data_dir).await;
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
