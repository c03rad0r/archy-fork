//! Peer content serving with access control.
//!
//! Serves only explicitly shared content items to authenticated peers.
//! Content items can be public, peer-restricted, or gated by verified payment.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncWriteExt, sync::Mutex};
use tracing::{debug, warn};

const CATALOG_FILE: &str = "content/catalog.json";
const CONTENT_DIR: &str = "content/files";
static CATALOG_WRITES: Mutex<()> = Mutex::const_new(());

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
    /// Only specific peers (by verified node DID).
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
    let content = match fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ContentCatalog::default())
        }
        Err(error) => return Err(error).context("Failed to read content catalog"),
    };
    serde_json::from_slice(&content)
        .context("Invalid content catalog; existing shares were preserved")
}

/// Save the content catalog to disk.
pub async fn save_catalog(data_dir: &Path, catalog: &ContentCatalog) -> Result<()> {
    let _lock = CATALOG_WRITES.lock().await;
    save_catalog_unlocked(data_dir, catalog).await
}

async fn save_catalog_unlocked(data_dir: &Path, catalog: &ContentCatalog) -> Result<()> {
    let dir = data_dir.join("content");
    fs::create_dir_all(&dir)
        .await
        .context("Failed to create content dir")?;
    let path = data_dir.join(CATALOG_FILE);
    let content = serde_json::to_string_pretty(catalog).context("Failed to serialize catalog")?;
    let (file, temporary) = tempfile::NamedTempFile::new_in(&dir)?.into_parts();
    let mut file = fs::File::from_std(file);
    file.write_all(content.as_bytes()).await?;
    file.sync_all().await?;
    drop(file);
    // Complete the rename synchronously while holding the catalog lock: an
    // aborted async caller must not leave a late rename racing the next writer.
    std::fs::rename(&temporary, &path).context("Failed to commit content catalog")?;
    fs::File::open(&dir).await?.sync_all().await?;
    Ok(())
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

pub(crate) fn validate_onchain_payment_price(price_sats: u64) -> Result<()> {
    anyhow::ensure!(
        price_sats >= 546,
        "On-chain payment requires at least 546 sats. Choose Lightning or ecash for this file."
    );
    Ok(())
}

/// Read-only preflight before issuing a payable invoice/address. This catches
/// missing/replaced files but does not substitute for an immutable purchase
/// snapshot: later delivery must still preserve the original accepted contract.
pub(crate) async fn ensure_payment_source_available(
    data_dir: &Path,
    item: &ContentItem,
) -> Result<()> {
    let path = content_file_path(data_dir, item);
    let canonical = fs::canonicalize(&path)
        .await
        .context("The shared file is currently unavailable; no payment request was created")?;
    let mut inside_root = false;
    for root in [data_dir.join(CONTENT_DIR), data_dir.join("filebrowser")] {
        if let Ok(root) = fs::canonicalize(root).await {
            inside_root |= canonical.starts_with(root);
        }
    }
    anyhow::ensure!(inside_root, "The shared file is outside the content roots");
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let file = options
        .open(&canonical)
        .await
        .context("The shared file cannot be opened; no payment request was created")?;
    let metadata = file.metadata().await?;
    anyhow::ensure!(metadata.is_file() && metadata.len() > 0 && metadata.len() == item.size_bytes,
        "The shared file has changed or is unavailable; refresh its catalog before accepting payment");
    Ok(())
}

/// Add a content item to the catalog.
///
/// Idempotent per FILE, not just per id: `content.add` mints a fresh UUID on
/// every call, so id-only dedup let the same file be shared twice as two
/// separately-priced entries — and a buyer paid twice for one file
/// (2026-07-22). Same filename → update the existing entry in place and
/// keep its id, so existing buyers' owned records stay valid.
pub async fn add_item(data_dir: &Path, item: ContentItem) -> Result<ContentCatalog> {
    let _lock = CATALOG_WRITES.lock().await;
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
    save_catalog_unlocked(data_dir, &catalog).await?;
    Ok(catalog)
}

/// Remove a content item from the catalog.
pub async fn remove_item(data_dir: &Path, id: &str) -> Result<ContentCatalog> {
    let _lock = CATALOG_WRITES.lock().await;
    let mut catalog = load_catalog(data_dir).await?;
    catalog.items.retain(|i| i.id != id);
    save_catalog_unlocked(data_dir, &catalog).await?;
    Ok(catalog)
}

/// Update access control for a content item.
pub async fn set_access(data_dir: &Path, id: &str, access: AccessControl) -> Result<()> {
    let _lock = CATALOG_WRITES.lock().await;
    let mut catalog = load_catalog(data_dir).await?;
    if let Some(item) = catalog.items.iter_mut().find(|i| i.id == id) {
        item.access = access;
        save_catalog_unlocked(data_dir, &catalog).await?;
        Ok(())
    } else {
        Err(anyhow::anyhow!("Content item '{}' not found", id))
    }
}

/// Update availability for a content item.
pub async fn set_availability(data_dir: &Path, id: &str, availability: Availability) -> Result<()> {
    let _lock = CATALOG_WRITES.lock().await;
    let mut catalog = load_catalog(data_dir).await?;
    if let Some(item) = catalog.items.iter_mut().find(|i| i.id == id) {
        item.availability = availability;
        save_catalog_unlocked(data_dir, &catalog).await?;
        Ok(())
    } else {
        Err(anyhow::anyhow!("Content item '{}' not found", id))
    }
}

/// Change price and visibility in one durable catalog transaction.
pub async fn configure_item(
    data_dir: &Path,
    id: &str,
    access: AccessControl,
    availability: Availability,
) -> Result<()> {
    let _lock = CATALOG_WRITES.lock().await;
    let mut catalog = load_catalog(data_dir).await?;
    let item = catalog
        .items
        .iter_mut()
        .find(|item| item.id == id)
        .context("Content item not found")?;
    item.access = access;
    item.availability = availability;
    save_catalog_unlocked(data_dir, &catalog).await
}

/// A byte range request (start, optional end).
pub enum ByteRange {
    From { start: u64, end: Option<u64> },
    Suffix(u64),
}

/// Parse an HTTP Range header value like "bytes=0-1023".
pub fn parse_range_header(header: &str) -> Option<ByteRange> {
    let (start, end) = header.strip_prefix("bytes=")?.split_once('-')?;
    let number = |s: &str| {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u64>().ok())
            .flatten()
    };
    if start.is_empty() {
        let count = number(end)?;
        return (count > 0).then_some(ByteRange::Suffix(count));
    }
    Some(ByteRange::From {
        start: number(start)?,
        end: if end.is_empty() {
            None
        } else {
            Some(number(end)?)
        },
    })
}

/// Result of attempting to serve content.
pub enum ServeResult {
    /// Bounded file-backed response; payment is checked before returning it.
    Stream(crate::prepared_media::PreparedMedia),
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
    /// The catalog entry and file exist but this node can't read the file.
    /// Returned before any payment is taken.
    Unavailable,
    /// Requested byte range cannot be served; no payment was taken.
    RangeNotSatisfiable(u64),
}

/// Shared metadata/payment/bytes visibility gate. `peer_did` must already have
/// a verified request signature; a header claim alone must never reach here.
pub fn visible_to(
    item: &ContentItem,
    peer_did: Option<&str>,
    known_peer: bool,
    owner: bool,
) -> bool {
    if matches!(item.availability, Availability::Nobody) {
        return false;
    }
    if owner {
        return true;
    }
    if let Availability::Specific { peers } = &item.availability {
        if !peer_did.is_some_and(|did| peers.iter().any(|allowed| allowed == did)) {
            return false;
        }
    }
    !matches!(item.access, AccessControl::PeersOnly) || known_peer
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
    serve_content_with(
        data_dir,
        id,
        payment_token,
        invoice_hash,
        peer_did,
        range,
        owner_session,
        |path, range, mime| {
            prepare_content_mode(
                data_dir,
                path,
                range,
                mime,
                payment_token.is_some() && !owner_session,
            )
        },
        |token, amount| async move { verify_payment_token(data_dir, &token, amount).await },
    )
    .await
}

// Inject only the read and payment boundaries, so tests can prove ordering
// without mint access, file-permission assumptions or privileged commands.
async fn serve_content_with<R, RF, V, VF>(
    data_dir: &Path,
    id: &str,
    payment_token: Option<&str>,
    invoice_hash: Option<&str>,
    peer_did: Option<&str>,
    range: Option<ByteRange>,
    owner_session: bool,
    read: R,
    verify: V,
) -> Result<ServeResult>
where
    R: FnOnce(PathBuf, Option<ByteRange>, String) -> RF,
    RF: std::future::Future<Output = Result<ServeResult>>,
    V: FnOnce(String, u64) -> VF,
    VF: std::future::Future<Output = bool>,
{
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

    if !visible_to(item, peer_did, is_known_peer, owner_session) {
        return Ok(if matches!(item.availability, Availability::Nobody) {
            ServeResult::NotFound
        } else {
            ServeResult::Forbidden
        });
    }

    let file_path = content_file_path(data_dir, item);
    if !file_path.exists() {
        // A disconnected mount, moved file or permission failure is not an
        // instruction to unshare content or erase its purchase metadata.
        warn!(content_id = %id, "Shared content is temporarily unavailable; catalog retained");
        return Ok(ServeResult::Unavailable);
    }

    // Refuse unauthorized viewers before opening or reading any bytes.
    if !owner_session && matches!(item.access, AccessControl::PeersOnly) && !is_known_peer {
        return Ok(ServeResult::Forbidden);
    }
    if !owner_session {
        if let AccessControl::Paid { price_sats, .. } = &item.access {
            if payment_token.is_none() && invoice_hash.is_none() {
                return Ok(ServeResult::PaymentRequired(*price_sats));
            }
        }
    }

    // Validate response metadata before touching a bearer payment too.
    if hyper::header::HeaderValue::from_str(&item.mime_type).is_err() {
        return Ok(ServeResult::Unavailable);
    }
    // Finish all file I/O before consuming bearer payment. Merely opening then
    // reopening after charging still lost payments on read errors or deletion.
    let prepared = match read(file_path, range, item.mime_type.clone()).await {
        Ok(
            result @ (ServeResult::Ok(..) | ServeResult::Partial { .. } | ServeResult::Stream(..)),
        ) => result,
        Ok(other) => return Ok(other),
        Err(error) => {
            warn!(content_id = %id, "Cannot prepare shared content: {error:#}");
            return Ok(ServeResult::Unavailable);
        }
    };

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
                    let method = if token.trim().starts_with("cashu") {
                        "ecash"
                    } else {
                        "fedimint"
                    };
                    if method_accepted(&item.access, method)
                        && verify(token.to_owned(), *price_sats).await
                    {
                        authorized = true;
                    }
                }
                if !authorized {
                    if let Some(hash) = invoice_hash {
                        if let Some(method) =
                            crate::content_invoice::paid_method_for(data_dir, hash, id).await
                        {
                            authorized = method_accepted(&item.access, method.as_str());
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

    Ok(prepared)
}

// Preserve a fully readable snapshot before redeeming a bearer token. Free,
// owner and durable invoice downloads can stream their open file directly.
#[cfg(test)]
async fn prepare_content(
    data_dir: &Path,
    path: PathBuf,
    range: Option<ByteRange>,
    mime: String,
) -> Result<ServeResult> {
    prepare_content_mode(data_dir, path, range, mime, true).await
}

async fn prepare_content_mode(
    data_dir: &Path,
    path: PathBuf,
    range: Option<ByteRange>,
    mime: String,
    snapshot: bool,
) -> Result<ServeResult> {
    use tokio::io::AsyncSeekExt;
    let mut file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(&path)
        .await
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return prepare_filebrowser_via_userns(data_dir, &path, range, mime).await;
        }
        Err(error) => return Err(error).context("Opening shared content"),
    };
    let metadata = file.metadata().await?;
    anyhow::ensure!(metadata.is_file(), "Shared content is not a regular file");
    let total = metadata.len();
    let selected = match range {
        Some(range) => match checked_range(&range, total) {
            Some((start, end)) => Some((start, end, total)),
            None => return Ok(ServeResult::RangeNotSatisfiable(total)),
        },
        None => None,
    };
    let (start, length) = selected
        .map(|(start, end, _)| (start, end - start + 1))
        .unwrap_or((0, total));
    file.seek(std::io::SeekFrom::Start(start)).await?;
    if snapshot || length <= 1024 * 1024 {
        prepare_reader(data_dir, file, length, mime, selected).await
    } else {
        Ok(ServeResult::Stream(
            crate::prepared_media::PreparedMedia::direct(file, start, length, mime, selected)
                .await?,
        ))
    }
}

async fn prepare_reader<R: tokio::io::AsyncRead + Unpin>(
    data_dir: &Path,
    mut source: R,
    length: u64,
    mime: String,
    selected: Option<(u64, u64, u64)>,
) -> Result<ServeResult> {
    use tokio::io::AsyncReadExt;
    if length > 1024 * 1024 {
        return Ok(ServeResult::Stream(
            crate::prepared_media::PreparedMedia::snapshot(
                data_dir, source, length, mime, selected,
            )
            .await?,
        ));
    }
    let mut bytes = vec![0; length as usize];
    source
        .read_exact(&mut bytes)
        .await
        .context("Preparing shared content")?;
    Ok(match selected {
        Some((start, end, total)) => ServeResult::Partial {
            bytes,
            mime_type: mime,
            start,
            end,
            total,
        },
        None => ServeResult::Ok(bytes, mime),
    })
}

fn checked_range(range: &ByteRange, total: u64) -> Option<(u64, u64)> {
    let last = total.checked_sub(1)?;
    match range {
        ByteRange::Suffix(count) => (*count > 0).then_some((total.saturating_sub(*count), last)),
        ByteRange::From { start, end } => {
            let end = end.unwrap_or(last).min(last);
            (*start <= end && *start < total).then_some((*start, end))
        }
    }
}

#[cfg(test)]
fn slice_prepared_content(
    bytes: Vec<u8>,
    range: Option<ByteRange>,
    mime: String,
) -> Result<ServeResult> {
    let total = bytes.len() as u64;
    match range {
        None => Ok(ServeResult::Ok(bytes, mime)),
        Some(range) => match checked_range(&range, total) {
            Some((start, end)) => Ok(ServeResult::Partial {
                bytes: bytes[start as usize..=end as usize].to_vec(),
                mime_type: mime,
                start,
                end,
                total,
            }),
            None => Ok(ServeResult::RangeNotSatisfiable(total)),
        },
    }
}

/// Read only an explicitly shared, regular file within FileBrowser storage.
/// Do not change its mode or grant world-readable access to paid/private data.
async fn filebrowser_read_path(data_dir: &Path, path: &Path) -> Result<PathBuf> {
    let root = fs::canonicalize(data_dir.join("filebrowser")).await?;
    let target = fs::canonicalize(path).await?;
    anyhow::ensure!(
        target.starts_with(&root) && target != root,
        "Shared file is outside Files storage"
    );
    anyhow::ensure!(
        fs::metadata(&target).await?.is_file(),
        "Shared content is not a regular file"
    );
    Ok(target)
}

async fn prepare_filebrowser_via_userns(
    data_dir: &Path,
    path: &Path,
    range: Option<ByteRange>,
    mime: String,
) -> Result<ServeResult> {
    let path = filebrowser_read_path(data_dir, path).await?;
    #[cfg(test)]
    {
        let _ = (path, range, mime);
        anyhow::bail!("Files namespace read disabled in unit tests")
    }
    #[cfg(not(test))]
    {
        let total = fs::metadata(&path).await?.len();
        let selected = match range {
            Some(range) => match checked_range(&range, total) {
                Some((start, end)) => Some((start, end, total)),
                None => return Ok(ServeResult::RangeNotSatisfiable(total)),
            },
            None => None,
        };
        let (start, length) = selected
            .map(|(start, end, _)| (start, end - start + 1))
            .unwrap_or((0, total));
        tokio::time::timeout(std::time::Duration::from_secs(900), async {
            // No shell, no full stdout buffering, and only the selected bytes.
            let mut input = std::ffi::OsString::from("if=");
            input.push(&path);
            let mut child = tokio::process::Command::new("podman")
                .args([
                    "unshare",
                    "dd",
                    "iflag=skip_bytes,count_bytes,nonblock,nofollow",
                    "status=none",
                ])
                .arg(input)
                .arg(format!("skip={start}"))
                .arg(format!("count={length}"))
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .context("Starting Files namespace read")?;
            let stdout = child
                .stdout
                .take()
                .context("Missing Files namespace output")?;
            let result = prepare_reader(data_dir, stdout, length, mime, selected).await?;
            anyhow::ensure!(
                child.wait().await?.success(),
                "Files namespace read failed; no payment was redeemed"
            );
            Ok(result)
        })
        .await
        .context("Files namespace read timed out")?
    }
}

/// Result of attempting to serve a preview.
pub enum PreviewResult {
    Stream(crate::prepared_media::PreparedMedia),
    /// Full publicly shared free content.
    FullContent(Vec<u8>, String),
    /// Small, server-blurred JPEG for a paid image; never the original bytes.
    BlurPreview(Vec<u8>, String),
    /// Bounded preview for paid video/audio (at most 10% of bytes).
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
/// Decode only bounded raster inputs, discard original metadata, then reduce
/// and blur pixels before encoding a new image. Browser styling is not a gate.
async fn blurred_image_preview(path: PathBuf) -> Result<Vec<u8>> {
    static WORKERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
    let permit = WORKERS.try_acquire().context("Preview workers busy")?;
    tokio::task::spawn_blocking(move || {
        use std::io::{Cursor, Read};
        use std::os::unix::fs::OpenOptionsExt;
        let _permit = permit;
        const MAX_INPUT: u64 = 16 * 1024 * 1024;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)?;
        let meta = file.metadata()?;
        anyhow::ensure!(
            meta.is_file() && meta.len() <= MAX_INPUT,
            "Invalid preview source"
        );
        let mut encoded = Vec::new();
        file.take(MAX_INPUT + 1).read_to_end(&mut encoded)?;
        anyhow::ensure!(
            encoded.len() as u64 <= MAX_INPUT,
            "Preview source grew too large"
        );
        let mut reader = image::ImageReader::new(Cursor::new(encoded)).with_guessed_format()?;
        anyhow::ensure!(
            matches!(
                reader.format(),
                Some(image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::WebP)
            ),
            "Unsupported preview image"
        );
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(32 * 1024 * 1024);
        reader.limits(limits);
        let reduced = reader.decode()?.thumbnail(160, 160).blur(8.0).to_rgb8();
        let mut output = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(reduced).write_to(&mut output, image::ImageFormat::Jpeg)?;
        Ok(output.into_inner())
    })
    .await
    .context("Preview worker failed")?
}

pub async fn serve_content_preview(data_dir: &Path, id: &str) -> Result<PreviewResult> {
    let catalog = load_catalog(data_dir).await?;
    let item = match catalog.items.iter().find(|i| i.id == id) {
        Some(i) => i,
        None => return Ok(PreviewResult::NotFound),
    };

    // This endpoint is anonymous. It must not become an alternate download
    // route around peer-only or specific-recipient access checks.
    if !matches!(item.availability, Availability::AllPeers)
        || matches!(item.access, AccessControl::PeersOnly)
    {
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
                match blurred_image_preview(file_path).await {
                    Ok(bytes) => Ok(PreviewResult::BlurPreview(bytes, "image/jpeg".into())),
                    Err(_) => Ok(PreviewResult::PreviewUnavailable),
                }
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

                // Never return the whole paid file just to reach a minimum
                // header size. Bound allocation even for very large videos.
                let metadata = fs::metadata(&file_path)
                    .await
                    .context("Failed to read file metadata")?;
                let total_size = metadata.len();
                let preview_bytes = (total_size / 10).min(8 * 1024 * 1024);
                if preview_bytes == 0 {
                    return Ok(PreviewResult::PreviewUnavailable);
                }

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
            // Only publicly available free content reaches this branch.
            match prepare_content_mode(data_dir, file_path, None, item.mime_type.clone(), false)
                .await?
            {
                ServeResult::Ok(bytes, mime) => Ok(PreviewResult::FullContent(bytes, mime)),
                ServeResult::Stream(body) => Ok(PreviewResult::Stream(body)),
                _ => Ok(PreviewResult::PreviewUnavailable),
            }
        }
    }
}

/// Verify a payment token covers the required amount.
/// Accepts real Cashu tokens and Fedimint notes.
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
mod unavailable_content_tests {
    use super::*;

    #[tokio::test]
    async fn unavailable_file_retains_identity_and_recovers_when_storage_returns() {
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
        assert!(matches!(result, ServeResult::Unavailable));
        let reloaded = load_catalog(data_dir).await.unwrap();
        assert_eq!(reloaded.items.len(), 1);
        assert_eq!(reloaded.items[0].id, "missing-item");
        fs::create_dir_all(data_dir.join("filebrowser"))
            .await
            .unwrap();
        fs::write(data_dir.join("filebrowser/gone.mp4"), b"recovered")
            .await
            .unwrap();
        let result = serve_content(data_dir, "missing-item", None, None, None, None, false)
            .await
            .unwrap();
        assert!(matches!(result, ServeResult::Ok(bytes, _) if bytes == b"recovered"));
    }

    #[tokio::test]
    async fn unavailable_file_does_not_rewrite_any_catalog_entries() {
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
        assert_eq!(reloaded.items.len(), 2);
        assert_eq!(reloaded.items[0].id, "missing-item");
        assert_eq!(reloaded.items[1].id, "present-item");
    }
}

#[cfg(test)]
mod paid_read_order_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn fixture(bytes: &[u8]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("content/files"))
            .await
            .unwrap();
        fs::write(dir.path().join("content/files/test.bin"), bytes)
            .await
            .unwrap();
        save_catalog(
            dir.path(),
            &ContentCatalog {
                items: vec![ContentItem {
                    id: "paid".into(),
                    filename: "test.bin".into(),
                    mime_type: "application/octet-stream".into(),
                    size_bytes: bytes.len() as u64,
                    description: String::new(),
                    access: AccessControl::Paid {
                        price_sats: 10,
                        accepted: vec!["ecash".into()],
                    },
                    availability: Availability::AllPeers,
                    added_at: "2026-09-30".into(),
                }],
            },
        )
        .await
        .unwrap();
        dir
    }

    #[tokio::test]
    async fn corrupt_catalog_is_not_overwritten_by_a_mutation() {
        let dir = fixture(b"file").await;
        let item = load_catalog(dir.path()).await.unwrap().items.remove(0);
        fs::write(dir.path().join(CATALOG_FILE), b"corrupt")
            .await
            .unwrap();
        assert!(add_item(dir.path(), item).await.is_err());
        assert!(remove_item(dir.path(), "paid").await.is_err());
        assert_eq!(
            fs::read(dir.path().join(CATALOG_FILE)).await.unwrap(),
            b"corrupt"
        );
    }

    #[tokio::test]
    async fn concurrent_catalog_updates_preserve_all_items_and_sharing_fields() {
        let dir = fixture(b"file").await;
        let template = load_catalog(dir.path()).await.unwrap().items.remove(0);
        let mut tasks = Vec::new();
        for i in 0..16 {
            let root = dir.path().to_owned();
            let mut item = template.clone();
            item.id = format!("item-{i}");
            item.filename = format!("file-{i}");
            tasks.push(tokio::spawn(async move {
                add_item(&root, item).await.unwrap();
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(load_catalog(dir.path()).await.unwrap().items.len(), 17);
        let (price, visibility) = tokio::join!(
            set_access(
                dir.path(),
                "paid",
                AccessControl::Paid {
                    price_sats: 7,
                    accepted: vec!["ecash".into()]
                }
            ),
            set_availability(dir.path(), "paid", Availability::Nobody),
        );
        price.unwrap();
        visibility.unwrap();
        let item = load_catalog(dir.path()).await.unwrap().items.remove(0);
        assert!(matches!(
            item.access,
            AccessControl::Paid { price_sats: 7, .. }
        ));
        assert!(matches!(item.availability, Availability::Nobody));
        configure_item(
            dir.path(),
            "paid",
            AccessControl::Paid {
                price_sats: 9,
                accepted: vec!["ecash".into()],
            },
            Availability::AllPeers,
        )
        .await
        .unwrap();
        let item = load_catalog(dir.path()).await.unwrap().items.remove(0);
        assert!(matches!(
            item.access,
            AccessControl::Paid { price_sats: 9, .. }
        ));
        assert!(matches!(item.availability, Availability::AllPeers));
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(dir.path().join(CATALOG_FILE))
                .await
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[tokio::test]
    async fn all_read_failures_precede_redemption_even_as_root() {
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::UnexpectedEof,
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::Other,
        ] {
            let dir = fixture(b"abc").await;
            let charged = AtomicUsize::new(0);
            let result = serve_content_with(
                dir.path(),
                "paid",
                Some("cashuBtest"),
                None,
                None,
                None,
                false,
                |_, _, _| async move { Err(std::io::Error::from(kind).into()) },
                |_, _| async {
                    charged.fetch_add(1, Ordering::SeqCst);
                    true
                },
            )
            .await
            .unwrap();
            assert!(matches!(result, ServeResult::Unavailable));
            assert_eq!(charged.load(Ordering::SeqCst), 0);
            assert_eq!(load_catalog(dir.path()).await.unwrap().items.len(), 1);
        }
    }

    #[test]
    fn peer_ranges_reject_malformed_headers_and_support_suffixes() {
        for invalid in [
            "bytes=0-invalid",
            "bytes=0",
            "bytes=+0-2",
            "bytes=0-1,3-4",
            "bytes=-0",
            "bytes=0--1",
            "bytes=18446744073709551616-",
            "bytes=-",
            "nope",
        ] {
            assert!(parse_range_header(invalid).is_none(), "{invalid}");
        }
        for (value, expected) in [
            ("bytes=-4", Some((6, 9))),
            ("bytes=-100", Some((0, 9))),
            ("bytes=2-", Some((2, 9))),
            ("bytes=2-100", Some((2, 9))),
            ("bytes=8-3", None),
            ("bytes=10-", None),
        ] {
            assert_eq!(
                checked_range(&parse_range_header(value).unwrap(), 10),
                expected,
                "{value}"
            );
        }
        assert_eq!(
            checked_range(&parse_range_header("bytes=0-").unwrap(), 0),
            None
        );
    }

    #[tokio::test]
    async fn large_paid_stream_still_requires_redemption_and_survives_source_deletion() {
        use hyper::body::HttpBody;
        let bytes = vec![91; 2 * 1024 * 1024];
        let dir = fixture(&bytes).await;
        let charged = std::sync::atomic::AtomicUsize::new(0);
        let result = serve_content_with(
            dir.path(),
            "paid",
            Some("cashuBtest"),
            None,
            None,
            None,
            false,
            |path, range, mime| prepare_content(dir.path(), path, range, mime),
            |_, amount| {
                assert_eq!(amount, 10);
                async {
                    charged.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    fs::remove_file(dir.path().join("content/files/test.bin"))
                        .await
                        .unwrap();
                    true
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(charged.load(std::sync::atomic::Ordering::SeqCst), 1);
        let ServeResult::Stream(body) = result else {
            panic!("expected bounded stream")
        };
        let mut response = body.into_response().unwrap();
        let mut received = Vec::new();
        while let Some(chunk) = response.body_mut().data().await {
            let chunk = chunk.unwrap();
            assert!(chunk.len() <= 65536);
            received.extend_from_slice(&chunk);
        }
        assert_eq!(received, bytes);
    }

    #[tokio::test]
    async fn rejected_payment_never_returns_the_prepared_large_stream() {
        let bytes = vec![91; 2 * 1024 * 1024];
        let dir = fixture(&bytes).await;
        let checked = std::sync::atomic::AtomicUsize::new(0);
        let result = serve_content_with(
            dir.path(),
            "paid",
            Some("cashuBinvalid"),
            None,
            None,
            None,
            false,
            |path, range, mime| prepare_content(dir.path(), path, range, mime),
            |_, _| async {
                checked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                false
            },
        )
        .await
        .unwrap();
        assert!(matches!(result, ServeResult::PaymentRequired(10)));
        assert_eq!(checked.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            std::fs::read_dir(dir.path().join("content-staging"))
                .unwrap()
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn free_large_preview_uses_bounded_stream_without_private_snapshot() {
        use hyper::body::HttpBody;
        let dir = fixture(&[0; 1]).await;
        let mut catalog = load_catalog(dir.path()).await.unwrap();
        catalog.items[0].access = AccessControl::Free;
        save_catalog(dir.path(), &catalog).await.unwrap();
        let file = fs::OpenOptions::new()
            .write(true)
            .open(dir.path().join("content/files/test.bin"))
            .await
            .unwrap();
        file.set_len(4 * 1024 * 1024 * 1024).await.unwrap();
        let PreviewResult::Stream(body) = serve_content_preview(dir.path(), "paid").await.unwrap()
        else {
            panic!("expected bounded preview")
        };
        let mut response = body.into_response().unwrap();
        assert_eq!(response.headers()["content-length"], "4294967296");
        assert_eq!(
            response.body_mut().data().await.unwrap().unwrap().len(),
            65536
        );
        drop(response);
        assert!(!dir.path().join("content-staging").exists());
    }

    #[tokio::test]
    async fn deletion_during_payment_cannot_lose_prepared_bytes() {
        let dir = fixture(b"original").await;
        let result = serve_content_with(
            dir.path(),
            "paid",
            Some("cashuBtest"),
            None,
            None,
            None,
            false,
            |path, range, mime| prepare_content(dir.path(), path, range, mime),
            |_, amount| {
                assert_eq!(amount, 10);
                async {
                    fs::remove_file(dir.path().join("content/files/test.bin"))
                        .await
                        .unwrap();
                    true
                }
            },
        )
        .await
        .unwrap();
        assert!(matches!(result, ServeResult::Ok(bytes, _) if bytes == b"original"));
    }

    #[tokio::test]
    async fn empty_out_of_bounds_and_reversed_ranges_never_charge() {
        for (bytes, start, end) in [
            (b"".as_slice(), 0, None),
            (b"abc".as_slice(), 3, None),
            (b"abc".as_slice(), 2, Some(1)),
        ] {
            let dir = fixture(bytes).await;
            let result = serve_content_with(
                dir.path(),
                "paid",
                Some("cashuBtest"),
                None,
                None,
                Some(ByteRange::From { start, end }),
                false,
                |path, range, mime| prepare_content(dir.path(), path, range, mime),
                |_, _| async { panic!("invalid range reached payment") },
            )
            .await
            .unwrap();
            assert!(
                matches!(result, ServeResult::RangeNotSatisfiable(n) if n == bytes.len() as u64)
            );
        }
    }

    #[tokio::test]
    async fn prepared_range_survives_file_change_while_payment_is_verified() {
        let dir = fixture(b"abcdef").await;
        let result = serve_content_with(
            dir.path(),
            "paid",
            Some("cashuBtest"),
            None,
            None,
            Some(ByteRange::From {
                start: 2,
                end: Some(999),
            }),
            false,
            |path, range, mime| prepare_content(dir.path(), path, range, mime),
            |_, _| async {
                fs::write(dir.path().join("content/files/test.bin"), b"x")
                    .await
                    .unwrap();
                true
            },
        )
        .await
        .unwrap();
        assert!(
            matches!(result, ServeResult::Partial { bytes, start: 2, end: 5, total: 6, .. } if bytes == b"cdef")
        );
    }

    #[tokio::test]
    async fn durable_payment_uses_its_recorded_method_for_seller_acceptance() {
        use crate::content_invoice::{mark_paid, record_pending_method, PaymentMethod};
        for (paid_with, accepted, expected) in [
            (PaymentMethod::Onchain, "onchain", true),
            (PaymentMethod::Onchain, "lightning", false),
            (PaymentMethod::Lightning, "lightning", true),
            (PaymentMethod::Lightning, "onchain", false),
        ] {
            let dir = fixture(b"paid bytes").await;
            let mut catalog = load_catalog(dir.path()).await.unwrap();
            catalog.items[0].access = AccessControl::Paid {
                price_sats: 10,
                accepted: vec![accepted.into()],
            };
            save_catalog(dir.path(), &catalog).await.unwrap();
            record_pending_method(dir.path(), "receipt", "paid", 10, paid_with)
                .await
                .unwrap();
            mark_paid(dir.path(), "receipt").await.unwrap();
            let result = serve_content_with(
                dir.path(),
                "paid",
                None,
                Some("receipt"),
                None,
                None,
                false,
                |path, range, mime| prepare_content(dir.path(), path, range, mime),
                |_, _| async { panic!("must not redeem another payment") },
            )
            .await
            .unwrap();
            if expected {
                assert!(matches!(result, ServeResult::Ok(bytes, _) if bytes == b"paid bytes"));
            } else {
                assert!(matches!(result, ServeResult::PaymentRequired(10)));
            }
        }
    }

    #[tokio::test]
    async fn payment_denial_never_returns_prepared_content() {
        let dir = fixture(b"secret").await;
        let charged = AtomicUsize::new(0);
        let result = serve_content_with(
            dir.path(),
            "paid",
            Some("cashuBtest"),
            None,
            None,
            None,
            false,
            |path, range, mime| prepare_content(dir.path(), path, range, mime),
            |_, _| async {
                charged.fetch_add(1, Ordering::SeqCst);
                false
            },
        )
        .await
        .unwrap();
        assert!(matches!(result, ServeResult::PaymentRequired(10)));
        assert_eq!(charged.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn missing_payment_and_peer_restrictions_precede_file_reads() {
        let dir = fixture(b"secret").await;
        let result = serve_content_with(
            dir.path(),
            "paid",
            None,
            None,
            None,
            None,
            false,
            |_, _, _| async { panic!("unauthorized file read") },
            |_, _| async { panic!("unexpected payment") },
        )
        .await
        .unwrap();
        assert!(matches!(result, ServeResult::PaymentRequired(10)));
        let mut catalog = load_catalog(dir.path()).await.unwrap();
        catalog.items[0].access = AccessControl::PeersOnly;
        save_catalog(dir.path(), &catalog).await.unwrap();
        let result = serve_content_with(
            dir.path(),
            "paid",
            None,
            None,
            None,
            None,
            false,
            |_, _, _| async { panic!("unauthorized file read") },
            |_, _| async { panic!("unexpected payment") },
        )
        .await
        .unwrap();
        assert!(matches!(result, ServeResult::Forbidden));
    }

    #[tokio::test]
    async fn owner_reads_paid_content_without_redemption() {
        let dir = fixture(b"own file").await;
        let result = serve_content_with(
            dir.path(),
            "paid",
            None,
            None,
            None,
            None,
            true,
            |path, range, mime| prepare_content(dir.path(), path, range, mime),
            |_, _| async { panic!("owner charged") },
        )
        .await
        .unwrap();
        assert!(matches!(result, ServeResult::Ok(bytes, _) if bytes == b"own file"));
    }

    #[tokio::test]
    async fn directory_in_place_of_file_does_not_charge() {
        let dir = fixture(b"abc").await;
        let path = dir.path().join("content/files/test.bin");
        fs::remove_file(&path).await.unwrap();
        fs::create_dir(&path).await.unwrap();
        let result = serve_content_with(
            dir.path(),
            "paid",
            Some("cashuBtest"),
            None,
            None,
            None,
            false,
            |path, range, mime| prepare_content(dir.path(), path, range, mime),
            |_, _| async { panic!("directory charged") },
        )
        .await
        .unwrap();
        assert!(matches!(result, ServeResult::Unavailable));
    }

    #[tokio::test]
    async fn files_namespace_read_is_scoped_to_regular_files_and_keeps_mode() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = fixture(b"outside").await;
        let root = dir.path().join("filebrowser");
        fs::create_dir(&root).await.unwrap();
        let inside = root.join("song");
        fs::write(&inside, b"song").await.unwrap();
        fs::set_permissions(&inside, std::fs::Permissions::from_mode(0o640))
            .await
            .unwrap();
        assert_eq!(
            filebrowser_read_path(dir.path(), &inside).await.unwrap(),
            inside
        );
        assert_eq!(
            fs::metadata(&inside).await.unwrap().permissions().mode() & 0o777,
            0o640
        );
        let outside = dir.path().join("content/files/test.bin");
        symlink(&outside, root.join("escape")).unwrap();
        for path in [outside, root.join("escape"), root.clone()] {
            assert!(filebrowser_read_path(dir.path(), &path).await.is_err());
        }
    }

    #[test]
    fn user_namespace_bytes_use_the_same_range_rules() {
        assert!(matches!(
            slice_prepared_content(
                vec![],
                Some(ByteRange::From {
                    start: 0,
                    end: None
                }),
                "x".into()
            )
            .unwrap(),
            ServeResult::RangeNotSatisfiable(0)
        ));
        assert!(
            matches!(slice_prepared_content(b"abc".to_vec(), Some(ByteRange::From { start: 1, end: None }), "x".into()).unwrap(), ServeResult::Partial { bytes, start: 1, end: 2, total: 3, .. } if bytes == b"bc")
        );
    }
}

#[cfg(test)]
mod preview_boundary_tests {
    use super::*;

    async fn fixture(
        bytes: &[u8],
        mime: &str,
        access: AccessControl,
        availability: Availability,
    ) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(CONTENT_DIR))
            .await
            .unwrap();
        fs::write(dir.path().join(CONTENT_DIR).join("preview.bin"), bytes)
            .await
            .unwrap();
        save_catalog(
            dir.path(),
            &ContentCatalog {
                items: vec![ContentItem {
                    id: "preview-test".into(),
                    filename: "preview.bin".into(),
                    mime_type: mime.into(),
                    size_bytes: bytes.len() as u64,
                    description: String::new(),
                    added_at: String::new(),
                    access,
                    availability,
                }],
            },
        )
        .await
        .unwrap();
        dir
    }

    fn paid() -> AccessControl {
        AccessControl::Paid {
            price_sats: 10,
            accepted: vec!["ecash".into()],
        }
    }

    #[tokio::test]
    async fn anonymous_preview_never_bypasses_restricted_sharing() {
        for (access, availability) in [
            (AccessControl::Free, Availability::Nobody),
            (paid(), Availability::Nobody),
            (
                AccessControl::Free,
                Availability::Specific {
                    peers: vec!["did:key:allowed".into()],
                },
            ),
            (
                paid(),
                Availability::Specific {
                    peers: vec!["did:key:allowed".into()],
                },
            ),
            (AccessControl::PeersOnly, Availability::AllPeers),
        ] {
            let dir = fixture(b"PRIVATE", "image/png", access, availability).await;
            assert!(matches!(
                serve_content_preview(dir.path(), "preview-test")
                    .await
                    .unwrap(),
                PreviewResult::NotFound
            ));
        }
    }

    #[tokio::test]
    async fn paid_image_preview_is_degraded_and_drops_original_metadata() {
        use std::io::Cursor;
        let source = image::RgbImage::from_fn(640, 320, |x, y| {
            let c = if (x / 8 + y / 8) % 2 == 0 { 0 } else { 255 };
            image::Rgb([c, c, c])
        });
        let original = image::DynamicImage::ImageRgb8(source);
        let mut encoded = Cursor::new(Vec::new());
        original
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let mut bytes = encoded.into_inner();
        const PRIVATE: &[u8] = b"PRIVATE-ORIGINAL-METADATA";
        bytes.extend_from_slice(PRIVATE);
        let dir = fixture(&bytes, "image/png", paid(), Availability::AllPeers).await;
        let PreviewResult::BlurPreview(preview, mime) =
            serve_content_preview(dir.path(), "preview-test")
                .await
                .unwrap()
        else {
            panic!("No degraded preview")
        };
        assert_eq!(mime, "image/jpeg");
        assert_ne!(preview, bytes);
        assert!(!preview.windows(PRIVATE.len()).any(|w| w == PRIVATE));
        let decoded = image::load_from_memory(&preview).unwrap().to_rgb8();
        assert!(decoded.width() <= 160 && decoded.height() <= 160);
        assert!(
            decoded.pixels().all(|p| p[0] > 30 && p[0] < 225),
            "High-contrast original detail must be blurred"
        );
        assert_eq!(
            fs::read(dir.path().join(CONTENT_DIR).join("preview.bin"))
                .await
                .unwrap(),
            bytes
        );

        // Malformed/unsupported and oversized inputs never fall back to the paid original.
        fs::write(
            dir.path().join(CONTENT_DIR).join("preview.bin"),
            b"<svg>private source</svg>",
        )
        .await
        .unwrap();
        assert!(matches!(
            serve_content_preview(dir.path(), "preview-test")
                .await
                .unwrap(),
            PreviewResult::PreviewUnavailable
        ));
        let file = fs::OpenOptions::new()
            .write(true)
            .open(dir.path().join(CONTENT_DIR).join("preview.bin"))
            .await
            .unwrap();
        file.set_len(17 * 1024 * 1024).await.unwrap();
        assert!(matches!(
            serve_content_preview(dir.path(), "preview-test")
                .await
                .unwrap(),
            PreviewResult::PreviewUnavailable
        ));
    }

    #[tokio::test]
    async fn paid_audio_preview_does_not_release_small_files_or_allocate_a_whole_film() {
        let dir = fixture(
            &vec![42; 1000],
            "audio/mpeg",
            paid(),
            Availability::AllPeers,
        )
        .await;
        let PreviewResult::TruncatedPreview(bytes, _, total) =
            serve_content_preview(dir.path(), "preview-test")
                .await
                .unwrap()
        else {
            panic!("No audio preview")
        };
        assert_eq!(total, 1000);
        assert_eq!(bytes, vec![42; 100]);
        let file = fs::OpenOptions::new()
            .write(true)
            .open(dir.path().join(CONTENT_DIR).join("preview.bin"))
            .await
            .unwrap();
        file.set_len(100 * 1024 * 1024).await.unwrap();
        let PreviewResult::TruncatedPreview(bytes, _, total) =
            serve_content_preview(dir.path(), "preview-test")
                .await
                .unwrap()
        else {
            panic!("No bounded preview")
        };
        assert_eq!(total, 100 * 1024 * 1024);
        assert_eq!(bytes.len(), 8 * 1024 * 1024);
        file.set_len(0).await.unwrap();
        assert!(matches!(
            serve_content_preview(dir.path(), "preview-test")
                .await
                .unwrap(),
            PreviewResult::PreviewUnavailable
        ));
    }

    #[tokio::test]
    async fn public_free_preview_remains_available() {
        let dir = fixture(
            b"public",
            "text/plain",
            AccessControl::Free,
            Availability::AllPeers,
        )
        .await;
        assert!(
            matches!(serve_content_preview(dir.path(), "preview-test").await.unwrap(), PreviewResult::FullContent(bytes, _) if bytes == b"public")
        );
    }
}

#[cfg(test)]
mod payment_source_tests {
    use super::*;
    #[test]
    fn onchain_quote_preserves_wallet_minimum_boundary() {
        assert!(validate_onchain_payment_price(545).is_err());
        assert!(validate_onchain_payment_price(546).is_ok());
    }

    #[tokio::test]
    async fn payment_preflight_rejects_missing_changed_directory_and_escaped_sources() {
        let root = tempfile::tempdir().unwrap();
        let item = ContentItem {
            id: "paid".into(),
            filename: "Photos/clip.mp4".into(),
            mime_type: "video/mp4".into(),
            size_bytes: 4,
            description: String::new(),
            access: AccessControl::Paid {
                price_sats: 2,
                accepted: vec![],
            },
            availability: Availability::AllPeers,
            added_at: String::new(),
        };
        assert!(ensure_payment_source_available(root.path(), &item)
            .await
            .is_err());
        fs::create_dir_all(root.path().join("filebrowser/Photos"))
            .await
            .unwrap();
        let path = root.path().join("filebrowser/Photos/clip.mp4");
        fs::write(&path, b"film").await.unwrap();
        ensure_payment_source_available(root.path(), &item)
            .await
            .unwrap();
        fs::write(&path, b"changed").await.unwrap();
        assert!(ensure_payment_source_available(root.path(), &item)
            .await
            .is_err());
        fs::remove_file(&path).await.unwrap();
        fs::create_dir(&path).await.unwrap();
        assert!(ensure_payment_source_available(root.path(), &item)
            .await
            .is_err());
        fs::remove_dir(&path).await.unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("film");
        fs::write(&outside_file, b"film").await.unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside_file, &path).unwrap();
            assert!(ensure_payment_source_available(root.path(), &item)
                .await
                .is_err());
        }
    }
}

// Make existing content_file_path pub(crate). Add this method in content_server.
pub(crate) async fn publish_snapshot_offer(
    data_dir: &Path,
    original: &ContentItem,
    offer: &crate::content_purchase_protocol::Offer,
) -> Result<crate::content_purchase_protocol::Offer> {
    let _held = CATALOG_WRITES.lock().await;
    let current = load_catalog(data_dir).await?;
    let item = current
        .items
        .iter()
        .find(|item| item.id == original.id)
        .context("Content was unshared before this offer")?;
    anyhow::ensure!(
        serde_json::to_value(item)? == serde_json::to_value(original)?,
        "Shared content terms changed before offer publication"
    );
    crate::content_purchase_protocol::save_offer(
        data_dir,
        offer,
        &offer.buyer_did,
        chrono::Utc::now().timestamp(),
    )
    .await
}

/// Commit a new invoice only while its exact selected share remains current.
/// Caller holds the invoice journal; catalog writers do not acquire that journal.
pub(crate) async fn publish_snapshot_invoice(
    data_dir: &Path,
    original: &ContentItem,
    journal: &crate::content_lightning::Journal,
    binding: crate::content_lightning::Binding,
    retained: crate::content_lightning::RetainedFile,
) -> Result<crate::content_lightning::SellerRecord> {
    let _held = CATALOG_WRITES.lock().await;
    let catalog = load_catalog(data_dir).await?;
    let current = catalog
        .items
        .iter()
        .find(|item| item.id == original.id)
        .context("Content was unshared before invoice preparation")?;
    anyhow::ensure!(
        serde_json::to_value(current)? == serde_json::to_value(original)?,
        "Shared content terms changed before invoice preparation"
    );
    anyhow::ensure!(
        binding.content_id == original.id
            && retained.filename == original.filename
            && retained.mime_type == original.mime_type
            && retained.size == original.size_bytes
            && matches!(&original.access, AccessControl::Paid { price_sats, .. } if *price_sats == binding.price_sats)
            && method_accepted(&original.access, "lightning"),
        "Invoice snapshot terms changed"
    );
    let visible = match &original.availability {
        Availability::Nobody => false,
        Availability::AllPeers => true,
        Availability::Specific { peers } => peers.contains(&binding.buyer_did),
    };
    anyhow::ensure!(visible, "Item is not shared with this invoice buyer");
    journal.prepare_seller_source(binding, Some(retained))
}

pub(crate) async fn publish_snapshot_onchain(
    data_dir: &Path,
    original: &ContentItem,
    journal: &crate::content_onchain_seller::Journal,
    binding: crate::content_lightning::Binding,
    retained: crate::content_lightning::RetainedFile,
    network: crate::content_onchain::ChainNetwork,
) -> Result<crate::content_onchain_seller::Record> {
    let _held = CATALOG_WRITES.lock().await;
    let catalog = load_catalog(data_dir).await?;
    let current = catalog
        .items
        .iter()
        .find(|item| item.id == original.id)
        .context("Content was unshared before invoice preparation")?;
    anyhow::ensure!(
        serde_json::to_value(current)? == serde_json::to_value(original)?,
        "Shared content terms changed before invoice preparation"
    );
    anyhow::ensure!(
        binding.content_id == original.id
            && retained.filename == original.filename
            && retained.mime_type == original.mime_type
            && retained.size == original.size_bytes
            && matches!(&original.access, AccessControl::Paid { price_sats, .. } if *price_sats == binding.price_sats)
            && method_accepted(&original.access, "onchain"),
        "Invoice snapshot terms changed"
    );
    let visible = match &original.availability {
        Availability::Nobody => false,
        Availability::AllPeers => true,
        Availability::Specific { peers } => peers.contains(&binding.buyer_did),
    };
    anyhow::ensure!(visible, "Item is not shared with this invoice buyer");
    journal.prepare(binding, retained, network)
}

/// Serialize the first allocation with catalog changes. Previously allocated
/// operations recover their original terms even if sharing changes afterwards.
pub(crate) async fn allocate_onchain_offer<W: crate::content_onchain_seller::Wallet>(
    data_dir: &Path,
    journal: &crate::content_onchain_seller::Journal,
    binding: &crate::content_lightning::Binding,
    wallet: &W,
) -> Result<crate::content_onchain_seller::Record> {
    let saved = journal.load(binding)?.context("Original offer missing")?;
    if saved.allocation != crate::content_onchain_seller::Allocation::Prepared {
        return crate::content_onchain_seller::drive(journal, binding, true, wallet).await;
    }
    let source_data = data_dir.to_path_buf();
    let source_id = binding.content_id.clone();
    let source = saved.source.clone();
    tokio::task::spawn_blocking(move || {
        crate::content_snapshot::open_matching(
            &source_data,
            &source_id,
            &source.sha256,
            source.size,
        )
    })
    .await??;
    let _held = CATALOG_WRITES.lock().await;
    let catalog = load_catalog(data_dir).await?;
    let item = catalog
        .items
        .iter()
        .find(|item| item.id == binding.content_id)
        .context("Original offer is no longer shared; cancel before allocation")?;
    let visible = match &item.availability {
        Availability::Nobody => false,
        Availability::AllPeers => true,
        Availability::Specific { peers } => peers.contains(&binding.buyer_did),
    };
    anyhow::ensure!(
        visible
            && item.filename == saved.source.filename
            && item.mime_type == saved.source.mime_type
            && item.size_bytes == saved.source.size
            && matches!(&item.access,AccessControl::Paid {price_sats,..} if *price_sats==binding.price_sats)
            && method_accepted(&item.access, "onchain"),
        "Original offer terms changed; no seller address allocated"
    );
    crate::content_onchain_seller::drive(journal, binding, true, wallet).await
}
