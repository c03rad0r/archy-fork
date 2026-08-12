//! Tor-based content serving with access control.
//!
//! Serves only explicitly shared content items to authenticated peers.
//! Content items can be free or ecash-gated (gating implemented later).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, warn};

const CATALOG_FILE: &str = "content/catalog.json";
const CONTENT_DIR: &str = "content/files";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentItem {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub access: AccessControl,
    #[serde(default)]
    pub availability: Availability,
    #[serde(default)]
    pub added_at: String,
}

/// Who can see/access this content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Availability {
    /// Nobody — content is not available.
    Nobody,
    /// All connected peers can access.
    #[default]
    AllPeers,
    /// Only specific peers (by onion address).
    Specific { peers: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum AccessControl {
    #[default]
    Free,
    PeersOnly,
    Paid {
        price_sats: u64,
        /// Payment methods the sharer accepts: "lightning", "onchain",
        /// "ecash", "fedimint". Empty = everything — which is also what
        /// catalogs written before this field deserialize to.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        accepted: Vec<String>,
    },
}

/// Does the sharer accept this payment method for the item? Empty list =
/// all methods (pre-field catalogs and "no preference").
pub fn method_accepted(access: &AccessControl, method: &str) -> bool {
    match access {
        AccessControl::Paid { accepted, .. } => {
            accepted.is_empty() || accepted.iter().any(|m| m == method)
        }
        _ => true,
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ContentCatalog {
    pub items: Vec<ContentItem>,
}

/// Load the content catalog from disk.
pub async fn load_catalog(data_dir: &Path) -> Result<ContentCatalog> {
    let path = data_dir.join(CATALOG_FILE);
    if !path.exists() {
        return Ok(ContentCatalog::default());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read content catalog")?;
    let catalog: ContentCatalog = serde_json::from_str(&content).unwrap_or_default();
    Ok(catalog)
}

/// Save the content catalog to disk.
pub async fn save_catalog(data_dir: &Path, catalog: &ContentCatalog) -> Result<()> {
    let dir = data_dir.join("content");
    fs::create_dir_all(&dir)
        .await
        .context("Failed to create content dir")?;
    let path = data_dir.join(CATALOG_FILE);
    let content = serde_json::to_string_pretty(catalog).context("Failed to serialize catalog")?;
    fs::write(&path, content)
        .await
        .context("Failed to write catalog")?;
    Ok(())
}

/// Removes `id` from the on-disk catalog. Best-effort: a failure here just
/// means the entry gets pruned again next time it's requested, so errors are
/// logged rather than propagated.
async fn prune_missing_content_entry(data_dir: &Path, id: &str) {
    let Ok(mut catalog) = load_catalog(data_dir).await else {
        return;
    };
    let before = catalog.items.len();
    catalog.items.retain(|i| i.id != id);
    if catalog.items.len() != before {
        if let Err(e) = save_catalog(data_dir, &catalog).await {
            warn!(error = %e, content_id = %id, "failed to save catalog after pruning missing content entry");
        }
    }
}

/// Get the full filesystem path for a content item.
/// Checks the dedicated content/files/ directory first, then falls back to the
/// FileBrowser data directory (where users manage files via the web UI).
pub fn content_file_path(data_dir: &Path, item: &ContentItem) -> PathBuf {
    // Strip leading slash from filename for path joining
    let clean_name = item.filename.trim_start_matches('/');

    // Primary: dedicated content directory
    let primary = data_dir.join(CONTENT_DIR).join(clean_name);
    if primary.exists() {
        return primary;
    }

    // Fallback: FileBrowser data directory (users share files managed via FileBrowser)
    let fb_path = data_dir.join("filebrowser").join(clean_name);
    if fb_path.exists() {
        return fb_path;
    }

    // Return primary path even if it doesn't exist (caller checks existence)
    primary
}

/// Add a content item to the catalog.
///
/// Idempotent per FILE, not just per id: `content.add` mints a fresh UUID on
/// every call, so id-only dedup let the same file be shared twice as two
/// separately-priced entries — and a buyer paid twice for one file
/// (2026-07-22). Same filename → update the existing entry in place and
/// keep its id, so existing buyers' owned records stay valid.
pub async fn add_item(data_dir: &Path, item: ContentItem) -> Result<ContentCatalog> {
    let mut catalog = load_catalog(data_dir).await?;
    if catalog.items.iter().any(|i| i.id == item.id) {
        return Err(anyhow::anyhow!("Content item '{}' already exists", item.id));
    }
    let norm = |f: &str| f.trim_start_matches('/').to_string();
    if let Some(existing) = catalog
        .items
        .iter_mut()
        .find(|i| norm(&i.filename) == norm(&item.filename))
    {
        let keep_id = existing.id.clone();
        *existing = item;
        existing.id = keep_id;
    } else {
        catalog.items.push(item);
    }
    save_catalog(data_dir, &catalog).await?;
    Ok(catalog)
}

/// Remove a content item from the catalog.
pub async fn remove_item(data_dir: &Path, id: &str) -> Result<ContentCatalog> {
    let mut catalog = load_catalog(data_dir).await?;
    catalog.items.retain(|i| i.id != id);
    save_catalog(data_dir, &catalog).await?;
    Ok(catalog)
}

/// Update access control for a content item.
pub async fn set_access(data_dir: &Path, id: &str, access: AccessControl) -> Result<()> {
    let mut catalog = load_catalog(data_dir).await?;
    if let Some(item) = catalog.items.iter_mut().find(|i| i.id == id) {
        item.access = access;
        save_catalog(data_dir, &catalog).await?;
        Ok(())
    } else {
        Err(anyhow::anyhow!("Content item '{}' not found", id))
    }
}

/// Update availability for a content item.
pub async fn set_availability(data_dir: &Path, id: &str, availability: Availability) -> Result<()> {
    let mut catalog = load_catalog(data_dir).await?;
    if let Some(item) = catalog.items.iter_mut().find(|i| i.id == id) {
        item.availability = availability;
        save_catalog(data_dir, &catalog).await?;
        Ok(())
    } else {
        Err(anyhow::anyhow!("Content item '{}' not found", id))
    }
}

/// A byte range request (start, optional end).
pub struct ByteRange {
    pub start: u64,
    pub end: Option<u64>,
}

/// Parse an HTTP Range header value like "bytes=0-1023".
pub fn parse_range_header(header: &str) -> Option<ByteRange> {
    let s = header.strip_prefix("bytes=")?;
    let mut parts = s.splitn(2, '-');
    let start_str = parts.next()?.trim();
    let end_str = parts.next().map(|s| s.trim());
    let start = start_str.parse::<u64>().ok()?;
    let end = end_str
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<u64>().ok());
    Some(ByteRange { start, end })
}

/// Result of attempting to serve content.
pub enum ServeResult {
    /// Content served successfully (full body).
    Ok(Vec<u8>, String),
    /// Partial content served (range response).
    Partial {
        bytes: Vec<u8>,
        mime_type: String,
        start: u64,
        end: u64,
        total: u64,
    },
    /// Payment required — includes price in sats.
    PaymentRequired(u64),
    /// Access forbidden — peer not authorized.
    Forbidden,
    /// Content not found.
    NotFound,
}

/// Serve a content item by ID with access control and optional range request.
/// If the content is paid, checks for a valid payment token in the header.
/// `peer_did` is the DID from the X-Federation-DID header (if present).
pub async fn serve_content(
    data_dir: &Path,
    id: &str,
    payment_token: Option<&str>,
    invoice_hash: Option<&str>,
    peer_did: Option<&str>,
    range: Option<ByteRange>,
    owner_session: bool,
) -> Result<ServeResult> {
    let catalog = load_catalog(data_dir).await?;
    let item = match catalog.items.iter().find(|i| i.id == id) {
        Some(i) => i,
        None => return Ok(ServeResult::NotFound),
    };

    // The authenticated local operator never pays for — and is never fenced
    // out of — their own node's content. The paid/peers-only gates exist for
    // buyers and peers on OTHER nodes; charging the owner for their own file
    // made the owner's own dashboard render 402s and lock overlays on their
    // own photos. `Availability::Nobody` still means delisted: not served
    // even here.
    if owner_session && matches!(item.availability, Availability::Nobody) {
        return Ok(ServeResult::NotFound);
    }

    // Load known federation peers for access checks
    let is_known_peer = if peer_did.is_some() {
        let nodes = crate::federation::load_nodes(data_dir)
            .await
            .unwrap_or_default();
        nodes.iter().any(|n| Some(n.did.as_str()) == peer_did)
    } else {
        false
    };

    // Check availability
    if !owner_session {
        match &item.availability {
            Availability::Nobody => return Ok(ServeResult::NotFound),
            Availability::Specific { peers } => {
                if let Some(did) = peer_did {
                    if !peers.iter().any(|p| p == did) {
                        debug!("Content '{}' not available to peer {}", id, did);
                        return Ok(ServeResult::Forbidden);
                    }
                } else {
                    return Ok(ServeResult::Forbidden);
                }
            }
            Availability::AllPeers => {}
        }
    }

    // Check access control
    if !owner_session {
        match &item.access {
            AccessControl::Paid { price_sats, .. } => {
                // Two ways to satisfy payment:
                //  (a) a valid ecash token (the local-wallet fast path), or
                //  (b) a Lightning-invoice payment hash this node issued and has
                //      since confirmed settled (the "pay from any wallet" path, #46).
                // Each path only counts when the sharer accepts that method.
                let mut authorized = false;
                if let Some(token) = payment_token {
                    if (method_accepted(&item.access, "ecash")
                        || method_accepted(&item.access, "fedimint"))
                        && verify_payment_token(data_dir, token, *price_sats).await
                    {
                        authorized = true;
                    }
                }
                if !authorized {
                    if let Some(hash) = invoice_hash {
                        if method_accepted(&item.access, "lightning")
                            && crate::content_invoice::is_paid_for(hash, id).await
                        {
                            authorized = true;
                        }
                    }
                }
                if !authorized {
                    return Ok(ServeResult::PaymentRequired(*price_sats));
                }
            }
            AccessControl::PeersOnly => {
                if !is_known_peer {
                    return Ok(ServeResult::Forbidden);
                }
            }
            AccessControl::Free => {}
        }
    }

    let file_path = content_file_path(data_dir, item);
    if !file_path.exists() {
        // The catalog entry survived (it's a separate JSON file) but its
        // backing file is gone — most likely lost in an unrelated data-dir
        // reset (a shared filebrowser file, 2026-07-01: two catalog entries
        // outlived a filebrowser reinstall that wiped the files themselves).
        // Leaving the entry in place would keep advertising it as available
        // to every peer forever, each hitting the exact same dead end this
        // one just did. Prune it so it stops being offered.
        warn!(
            content_id = %id,
            filename = %item.filename,
            "content catalog entry's file is missing on disk — pruning the stale entry"
        );
        prune_missing_content_entry(data_dir, id).await;
        return Ok(ServeResult::NotFound);
    }

    let metadata = fs::metadata(&file_path)
        .await
        .context("Failed to read file metadata")?;
    let total_size = metadata.len();

    // Handle range request for streaming
    if let Some(range) = range {
        let start = range.start.min(total_size.saturating_sub(1));
        let end = range
            .end
            .map(|e| e.min(total_size - 1))
            .unwrap_or(total_size - 1);

        if start > end || start >= total_size {
            return Ok(ServeResult::NotFound);
        }

        let len = (end - start + 1) as usize;
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        let mut file = tokio::fs::File::open(&file_path)
            .await
            .context("Failed to open content file")?;
        file.seek(std::io::SeekFrom::Start(start))
            .await
            .context("Failed to seek")?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf)
            .await
            .context("Failed to read range")?;

        debug!(
            "Serving content '{}' range {}-{}/{} ({} bytes)",
            id, start, end, total_size, len
        );
        return Ok(ServeResult::Partial {
            bytes: buf,
            mime_type: item.mime_type.clone(),
            start,
            end,
            total: total_size,
        });
    }

    let bytes = fs::read(&file_path)
        .await
        .context("Failed to read content file")?;

    debug!("Serving content '{}' ({} bytes)", id, bytes.len());
    Ok(ServeResult::Ok(bytes, item.mime_type.clone()))
}

/// Result of attempting to serve a preview.
pub enum PreviewResult {
    /// Full content (free/peers-only items — redirect to normal serve).
    FullContent(Vec<u8>, String),
    /// Blurred preview for paid image (full bytes, frontend applies blur).
    BlurPreview(Vec<u8>, String),
    /// Truncated preview for paid video (first ~2% of bytes).
    TruncatedPreview(Vec<u8>, String, u64),
    /// A preview can't be produced for this media without re-encoding (e.g. a
    /// non-faststart MP4 whose moov atom is at the end, so a byte prefix won't
    /// play). The UI shows its "preview unavailable" overlay instead of a
    /// broken player. (#35)
    PreviewUnavailable,
    /// Content not found.
    NotFound,
}

/// Scan an MP4's top-level boxes and report whether `moov` appears before
/// `mdat` ("faststart"). Returns `Some(true)` if faststart (a byte prefix is
/// playable), `Some(false)` if the media data precedes the index (a prefix
/// will NOT play), or `None` if neither box is found / the file isn't parseable
/// as ISO-BMFF (caller falls back to the legacy prefix behavior).
async fn mp4_is_faststart(path: &std::path::Path) -> Option<bool> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
    let mut f = tokio::fs::File::open(path).await.ok()?;
    let file_len = f.metadata().await.ok()?.len();
    let mut pos: u64 = 0;
    // Bound the walk so a malformed file can't spin forever.
    for _ in 0..1024 {
        if pos.saturating_add(8) > file_len {
            return None;
        }
        f.seek(SeekFrom::Start(pos)).await.ok()?;
        let mut hdr = [0u8; 8];
        if f.read_exact(&mut hdr).await.is_err() {
            return None;
        }
        let mut size = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as u64;
        let btype = &hdr[4..8];
        let mut header_len = 8u64;
        if size == 1 {
            // 64-bit extended size.
            let mut ext = [0u8; 8];
            if f.read_exact(&mut ext).await.is_err() {
                return None;
            }
            size = u64::from_be_bytes(ext);
            header_len = 16;
        } else if size == 0 {
            // Box runs to EOF — it's the last one.
            size = file_len.saturating_sub(pos);
        }
        match btype {
            b"moov" => return Some(true),  // index before media → faststart
            b"mdat" => return Some(false), // media before index → not faststart
            _ => {}
        }
        if size < header_len {
            return None; // malformed
        }
        pos = pos.checked_add(size)?;
    }
    None
}

/// Serve a preview of content by ID. For paid content, returns degraded previews:
/// - Images: full file with X-Content-Preview: blur (frontend applies CSS blur)
/// - Videos: first 2% of file bytes (minimum 512KB for codec headers)
/// - Other: not available
/// For free/peers-only content, returns the full file.
pub async fn serve_content_preview(data_dir: &Path, id: &str) -> Result<PreviewResult> {
    let catalog = load_catalog(data_dir).await?;
    let item = match catalog.items.iter().find(|i| i.id == id) {
        Some(i) => i,
        None => return Ok(PreviewResult::NotFound),
    };

    // Check availability — don't preview hidden items
    if matches!(item.availability, Availability::Nobody) {
        return Ok(PreviewResult::NotFound);
    }

    let file_path = content_file_path(data_dir, item);
    if !file_path.exists() {
        return Ok(PreviewResult::NotFound);
    }

    match &item.access {
        AccessControl::Paid { .. } => {
            let mime = &item.mime_type;
            if mime.starts_with("image/") {
                // Serve full image — frontend applies CSS blur
                let bytes = fs::read(&file_path)
                    .await
                    .context("Failed to read preview file")?;
                debug!(
                    "Serving blur preview for paid image '{}' ({} bytes)",
                    id,
                    bytes.len()
                );
                Ok(PreviewResult::BlurPreview(bytes, item.mime_type.clone()))
            } else if mime.starts_with("video/") || mime.starts_with("audio/") {
                // A byte-prefix preview only plays if the container's index is at
                // the front. For MP4/MOV that means the `moov` atom must precede
                // `mdat` (faststart). Non-faststart files have moov at the end, so
                // a 10% prefix is an unplayable truncated MP4 (#35) — report it as
                // unavailable rather than streaming bytes that hang the player.
                let is_isobmff = mime == "video/mp4"
                    || mime == "video/quicktime"
                    || matches!(
                        file_path.extension().and_then(|e| e.to_str()),
                        Some("mp4") | Some("m4v") | Some("mov") | Some("m4a")
                    );
                if is_isobmff && mp4_is_faststart(&file_path).await == Some(false) {
                    debug!(
                        "Paid {} '{}' is a non-faststart MP4 (moov after mdat) — no playable prefix preview",
                        if mime.starts_with("video/") { "video" } else { "audio" },
                        id
                    );
                    return Ok(PreviewResult::PreviewUnavailable);
                }

                // Serve first 10% of video/audio, minimum 512KB for codec headers
                let metadata = fs::metadata(&file_path)
                    .await
                    .context("Failed to read file metadata")?;
                let total_size = metadata.len();
                let preview_bytes = ((total_size * 10) / 100).max(512 * 1024).min(total_size);

                use tokio::io::AsyncReadExt;
                let mut file = tokio::fs::File::open(&file_path)
                    .await
                    .context("Failed to open file")?;
                let mut buf = vec![0u8; preview_bytes as usize];
                file.read_exact(&mut buf)
                    .await
                    .context("Failed to read preview bytes")?;

                let kind = if mime.starts_with("video/") {
                    "video"
                } else {
                    "audio"
                };
                debug!(
                    "Serving truncated preview for paid {} '{}' ({}/{} bytes)",
                    kind, id, preview_bytes, total_size
                );
                Ok(PreviewResult::TruncatedPreview(
                    buf,
                    item.mime_type.clone(),
                    total_size,
                ))
            } else {
                // Non-media paid content — no preview available
                Ok(PreviewResult::NotFound)
            }
        }
        _ => {
            // Free or peers-only — serve full content as preview
            let bytes = fs::read(&file_path)
                .await
                .context("Failed to read content file")?;
            Ok(PreviewResult::FullContent(bytes, item.mime_type.clone()))
        }
    }
}

/// Verify a payment token covers the required amount.
/// Accepts both cashuA tokens (real Cashu) and legacy cashuSend_ format.
/// Swaps proofs at the mint to verify they're unspent before accepting.
async fn verify_payment_token(data_dir: &Path, token: &str, required_sats: u64) -> bool {
    match crate::wallet::ecash::verify_and_receive_payment(data_dir, token, required_sats).await {
        Ok(received) => {
            debug!(
                "Payment verified: {} sats received for {} required",
                received, required_sats
            );
            // Record the content sale for profit tracking
            if let Err(e) = crate::wallet::profits::record_content_sale(
                data_dir,
                received,
                "Content download payment",
            )
            .await
            {
                debug!("Failed to record content sale profit (non-fatal): {}", e);
            }
            true
        }
        Err(e) => {
            debug!("Payment verification failed: {}", e);
            false
        }
    }
}

#[cfg(test)]
mod faststart_tests {
    use super::*;

    fn box_hdr(size: u32, typ: &[u8; 4]) -> Vec<u8> {
        let mut v = size.to_be_bytes().to_vec();
        v.extend_from_slice(typ);
        v
    }

    #[tokio::test]
    async fn detects_faststart_moov_before_mdat() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("fast.mp4");
        let mut data = Vec::new();
        data.extend(box_hdr(16, b"ftyp"));
        data.extend([0u8; 8]);
        data.extend(box_hdr(8, b"moov"));
        data.extend(box_hdr(8, b"mdat"));
        tokio::fs::write(&p, &data).await.unwrap();
        assert_eq!(mp4_is_faststart(&p).await, Some(true));
    }

    #[tokio::test]
    async fn detects_non_faststart_mdat_before_moov() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("slow.mp4");
        let mut data = Vec::new();
        data.extend(box_hdr(16, b"ftyp"));
        data.extend([0u8; 8]);
        data.extend(box_hdr(16, b"mdat"));
        data.extend([0u8; 8]);
        data.extend(box_hdr(8, b"moov"));
        tokio::fs::write(&p, &data).await.unwrap();
        assert_eq!(mp4_is_faststart(&p).await, Some(false));
    }
}

#[cfg(test)]
mod prune_missing_content_tests {
    use super::*;

    #[tokio::test]
    async fn serve_content_prunes_catalog_entry_whose_file_is_missing() {
        // Simulates a catalog entry that outlived its backing file (a shared
        // filebrowser file lost in an unrelated data-dir reset, 2026-07-01) —
        // every peer request for it would otherwise 404 forever with no way
        // to tell it apart from a transient failure.
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path();
        let item = ContentItem {
            id: "missing-item".to_string(),
            filename: "gone.mp4".to_string(),
            mime_type: "video/mp4".to_string(),
            size_bytes: 123,
            description: String::new(),
            access: AccessControl::Free,
            availability: Availability::AllPeers,
            added_at: "2026-01-01T00:00:00Z".to_string(),
        };
        save_catalog(data_dir, &ContentCatalog { items: vec![item] })
            .await
            .unwrap();

        // File was never written to disk under content/files/ or filebrowser/.
        let result = serve_content(data_dir, "missing-item", None, None, None, None, false)
            .await
            .unwrap();
        assert!(matches!(result, ServeResult::NotFound));

        let reloaded = load_catalog(data_dir).await.unwrap();
        assert!(
            reloaded.items.is_empty(),
            "stale entry should have been pruned after the 404"
        );
    }

    #[tokio::test]
    async fn serve_content_leaves_other_entries_untouched_when_pruning() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path();
        let missing = ContentItem {
            id: "missing-item".to_string(),
            filename: "gone.mp4".to_string(),
            mime_type: "video/mp4".to_string(),
            size_bytes: 123,
            description: String::new(),
            access: AccessControl::Free,
            availability: Availability::AllPeers,
            added_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let present = ContentItem {
            id: "present-item".to_string(),
            filename: "here.mp4".to_string(),
            mime_type: "video/mp4".to_string(),
            size_bytes: 4,
            description: String::new(),
            access: AccessControl::Free,
            availability: Availability::AllPeers,
            added_at: "2026-01-01T00:00:00Z".to_string(),
        };
        save_catalog(
            data_dir,
            &ContentCatalog {
                items: vec![missing, present],
            },
        )
        .await
        .unwrap();
        let content_dir = data_dir.join("content").join("files");
        tokio::fs::create_dir_all(&content_dir).await.unwrap();
        tokio::fs::write(content_dir.join("here.mp4"), b"data")
            .await
            .unwrap();

        let _ = serve_content(data_dir, "missing-item", None, None, None, None, false)
            .await
            .unwrap();

        let reloaded = load_catalog(data_dir).await.unwrap();
        assert_eq!(reloaded.items.len(), 1);
        assert_eq!(reloaded.items[0].id, "present-item");
    }
}
