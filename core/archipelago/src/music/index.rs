//! The persisted music library index (`13-MUSIC-MODEL.md`, D-13): scan the
//! media roots, extract tags per audio file (`tags::extract_tags`), group
//! albums/artists at read time, persist as a single JSON file at
//! `data_dir/music/index.json` (the `content_server.rs::load_catalog`
//! precedent), and refresh incrementally via `(path, mtime, size)`
//! comparison so unchanged files are never re-extracted.
//!
//! Two invariants live here:
//!
//! - **Atomic persistence** (T-13-42): `save_atomic` writes to a sibling
//!   temp file and `rename`s over the target, so a concurrent read sees
//!   either the complete previous index or the complete new one — never a
//!   partial file — and a crash mid-write leaves the previous index intact.
//! - **Forward-version refusal** (T-13-43): `load` refuses an index whose
//!   `schema_version` exceeds `MUSIC_SCHEMA_VERSION` with a distinct error
//!   rather than misreading it; the caller treats the index as absent and
//!   rebuilds via an explicit reindex (`13-MUSIC-MODEL.md`'s
//!   newer-version-on-older-binary contract — the on-disk file is never
//!   overwritten until a reindex is explicitly triggered).

use super::tags::extract_tags;
use super::{Album, AlbumId, Artist, ArtistId, LibrarySnapshot, MusicSource, Track, TrackId};
use crate::music::MUSIC_SCHEMA_VERSION;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering as CmpOrdering;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, warn};

/// On-disk location of the index, relative to `data_dir` — matches
/// `content_server.rs`'s `CATALOG_FILE` convention (JSON file under a
/// `data_dir` subdirectory).
pub const INDEX_FILENAME: &str = "music/index.json";

/// Extensions the walker treats as audio candidates. Everything else is
/// ignored without being opened; candidates that fail `extract_tags`
/// (hostile/corrupt files, T-13-44) are counted in `ScanStats::skipped` and
/// the walk continues — never a panic, never an aborted scan.
const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "m4a", "mp4", "aac", "ogg", "oga", "opus", "wav", "aiff", "aif", "ape", "wv",
];

/// Errors from loading/saving the index. `NewerSchema` is deliberately its
/// own variant so callers can distinguish "an index from the future"
/// (treat as absent, rebuild on explicit reindex) from real I/O failures.
#[derive(Debug, Error)]
pub enum IndexError {
    #[error(
        "on-disk music index schema {found} is newer than this binary's supported \
         {supported}; treating it as absent (an explicit reindex rebuilds it)"
    )]
    NewerSchema { found: u32, supported: u32 },
    #[error("music index io: {0}")]
    Io(#[from] std::io::Error),
    #[error("music index serialization: {0}")]
    Serde(#[from] serde_json::Error),
}

/// The persisted index: schema version + scan timestamp + one entry per
/// track. Albums and artists are NOT stored (derived-albums,
/// `13-MUSIC-MODEL.md`) — they are computed by `snapshot()` at read time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MusicIndex {
    pub schema_version: u32,
    /// RFC3339 timestamp of the scan that produced this index.
    pub scanned_at: String,
    pub entries: Vec<IndexEntry>,
}

/// One track row plus the stat data (`mtime`, `size`) that
/// `refresh_incremental` compares to decide whether the file changed since
/// the last scan without re-reading its contents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub track: Track,
    /// File mtime in milliseconds since the Unix epoch (0 if unavailable).
    pub mtime_ms: u64,
    pub size_bytes: u64,
}

/// Feedback from a scan — a library scan that gives no feedback is
/// indistinguishable from a hang on a large collection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanStats {
    /// Audio candidates seen during the walk.
    pub scanned: u64,
    /// Files whose tags were (re-)extracted this scan.
    pub extracted: u64,
    /// Candidates skipped: extraction errors, escaping symlinks, stat
    /// failures.
    pub skipped: u64,
    /// Rows removed because their file disappeared since the last scan.
    pub removed: u64,
    pub elapsed_ms: u64,
}

impl MusicIndex {
    /// An empty index at the current schema version. `scanned_at` is
    /// populated with "now" so even a never-scanned library serves a
    /// timestamp, never a null.
    pub fn empty() -> Self {
        Self {
            schema_version: MUSIC_SCHEMA_VERSION,
            scanned_at: chrono::Utc::now().to_rfc3339(),
            entries: Vec::new(),
        }
    }

    /// Derive the read-time view: tracks sorted by the library comparator,
    /// albums/artists grouped from tags (derived-albums). Deterministic and
    /// stable across repeated calls — see `track_cmp` for the tiebreak
    /// chain.
    pub fn snapshot(&self) -> LibrarySnapshot {
        let mut tracks: Vec<Track> = self.entries.iter().map(|e| e.track.clone()).collect();
        tracks.sort_by(track_cmp);
        LibrarySnapshot {
            schema_version: self.schema_version,
            scanned_at: self.scanned_at.clone(),
            albums: group_albums(&tracks),
            artists: group_artists(&tracks),
            tracks,
        }
    }
}

// ---------------------------------------------------------------------
// Ordering — one comparator used everywhere (persisted file, snapshot,
// album/artist track lists), so equal keys never reorder between calls.
// Albums: album artist, then album title, then year, then identity.
// Tracks: disc, then track number, then title, then identity.
// ---------------------------------------------------------------------

fn source_sort_key(source: &MusicSource) -> (u8, &str) {
    match source {
        MusicSource::OwnLibrary => (0, ""),
        MusicSource::Peer { onion } => (1, onion.as_str()),
    }
}

fn track_id_cmp(a: &TrackId, b: &TrackId) -> CmpOrdering {
    source_sort_key(&a.source)
        .cmp(&source_sort_key(&b.source))
        .then_with(|| a.path.cmp(&b.path))
}

/// The track comparator: disc, track number, title, then the decided
/// identity `(source, path)` as the final tiebreak so equal sort keys never
/// reorder between calls.
pub(crate) fn track_cmp(a: &Track, b: &Track) -> CmpOrdering {
    a.disc_number
        .cmp(&b.disc_number)
        .then_with(|| a.track_number.cmp(&b.track_number))
        .then_with(|| a.title.cmp(&b.title))
        .then_with(|| track_id_cmp(&a.id, &b.id))
}

/// Group tracks into albums on `(album_artist, album)` per derived-albums.
/// Tracks with no album tag belong to no album (they still appear in the
/// track list). Albums are ordered by album artist, album title, earliest
/// track year, then the grouping key itself as the final tiebreak.
fn group_albums(tracks: &[Track]) -> Vec<Album> {
    let mut groups: HashMap<AlbumId, Vec<&Track>> = HashMap::new();
    for track in tracks {
        if let Some(album) = &track.album {
            let id = AlbumId {
                album_artist: track.album_artist.clone(),
                album: album.clone(),
            };
            groups.entry(id).or_default().push(track);
        }
    }

    let mut albums: Vec<(Option<u32>, Album)> = groups
        .into_iter()
        .map(|(id, mut members)| {
            members.sort_by(|a, b| track_cmp(a, b));
            let year = members.iter().filter_map(|t| t.year).min();
            let album = Album {
                id,
                track_ids: members.iter().map(|t| t.id.clone()).collect(),
            };
            (year, album)
        })
        .collect();

    albums.sort_by(|(year_a, a), (year_b, b)| {
        a.id.album_artist
            .cmp(&b.id.album_artist)
            .then_with(|| a.id.album.cmp(&b.id.album))
            .then_with(|| year_a.cmp(year_b))
    });
    albums.into_iter().map(|(_, album)| album).collect()
}

/// Group tracks into artists on the `artist` tag. Tracks with no artist tag
/// belong to no artist. Artists are ordered by name; each artist's tracks
/// use the shared track comparator.
fn group_artists(tracks: &[Track]) -> Vec<Artist> {
    let mut groups: BTreeMap<String, Vec<&Track>> = BTreeMap::new();
    for track in tracks {
        if let Some(artist) = &track.artist {
            groups.entry(artist.clone()).or_default().push(track);
        }
    }
    groups
        .into_iter()
        .map(|(name, mut members)| {
            members.sort_by(|a, b| track_cmp(a, b));
            Artist {
                id: ArtistId(name),
                track_ids: members.iter().map(|t| t.id.clone()).collect(),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------

/// Load the index from `data_dir/music/index.json`.
///
/// - Missing file → `Ok(MusicIndex::empty())` (the `load_catalog`
///   precedent: a fresh node just has an empty library).
/// - Corrupt/unparseable JSON → `Ok(MusicIndex::empty())` with a warning
///   (same precedent) — an explicit reindex rebuilds it.
/// - `schema_version > MUSIC_SCHEMA_VERSION` → `Err(IndexError::NewerSchema)`,
///   a distinct error so the caller treats the index as absent WITHOUT
///   overwriting the newer file (`13-MUSIC-MODEL.md`'s downgrade contract);
///   a forward-incompatible index misread as current is worse than no index.
pub fn load(data_dir: &Path) -> Result<MusicIndex, IndexError> {
    let path = data_dir.join(INDEX_FILENAME);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(MusicIndex::empty()),
        Err(e) => return Err(e.into()),
    };
    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(e) => {
            warn!(
                "music index at {} is corrupt ({e}); treating as empty",
                path.display()
            );
            return Ok(MusicIndex::empty());
        }
    };
    let found = value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    if found > MUSIC_SCHEMA_VERSION {
        return Err(IndexError::NewerSchema {
            found,
            supported: MUSIC_SCHEMA_VERSION,
        });
    }
    match serde_json::from_value::<MusicIndex>(value) {
        Ok(index) => Ok(index),
        Err(e) => {
            warn!(
                "music index at {} did not match schema {found} ({e}); treating as empty",
                path.display()
            );
            Ok(MusicIndex::empty())
        }
    }
}

/// Unique suffix for sibling temp files so concurrent writers (only
/// possible in tests — production writes are serialized by the reindex
/// guard) never collide on the same temp name.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Write the index atomically: serialize to a sibling temp file in the
/// same directory, fsync it, then `rename` over the target. A reader never
/// sees a partial file and a crash mid-write leaves the previous index
/// intact (T-13-42). Do not replace this with an in-place write — this
/// single choice is what makes the concurrent-read guarantee true.
pub fn save_atomic(data_dir: &Path, index: &MusicIndex) -> Result<(), IndexError> {
    let path = data_dir.join(INDEX_FILENAME);
    let dir = path
        .parent()
        .expect("INDEX_FILENAME always has a parent directory");
    std::fs::create_dir_all(dir)?;

    let tmp = dir.join(format!(
        ".index.json.tmp.{}.{}",
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let json = serde_json::to_string_pretty(index)?;
    let result = (|| -> Result<(), IndexError> {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    })();
    if result.is_err() {
        // Best-effort cleanup of the orphaned temp file.
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

// ---------------------------------------------------------------------
// Reindex guard — two concurrent scans must not both walk the tree.
// ---------------------------------------------------------------------

/// Shared scan state: an atomic in-progress flag plus the last completed
/// scan's stats. The RPC layer uses the process-wide `shared_state()`;
/// tests construct their own so parallel tests never interfere.
#[derive(Default)]
pub struct ReindexState {
    running: AtomicBool,
    last_stats: Mutex<Option<ScanStats>>,
}

impl ReindexState {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn last_stats(&self) -> Option<ScanStats> {
        self.last_stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Try to become the running scan. `None` means one is already running
    /// — the caller reports "already running" with `last_stats()` rather
    /// than queueing a duplicate walk (T-13-41).
    pub(crate) fn try_begin(&self) -> Option<ReindexGuard<'_>> {
        self.running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()?;
        Some(ReindexGuard { state: self })
    }

    fn record_stats(&self, stats: ScanStats) {
        *self
            .last_stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(stats);
    }
}

/// RAII holder of the in-progress flag; releasing on drop means an
/// erroring or panicking scan can never wedge the library in a permanently
/// "already running" state.
pub(crate) struct ReindexGuard<'a> {
    state: &'a ReindexState,
}

impl Drop for ReindexGuard<'_> {
    fn drop(&mut self) {
        self.state.running.store(false, Ordering::SeqCst);
    }
}

/// The process-wide scan state used by the `music.*` RPC surface (one node
/// process, one library).
pub fn shared_state() -> &'static ReindexState {
    static SHARED: OnceLock<ReindexState> = OnceLock::new();
    SHARED.get_or_init(ReindexState::default)
}

/// Outcome of a scan request.
#[derive(Debug)]
pub enum ReindexOutcome {
    Completed(ScanStats),
    /// A scan was already running; carries the previous completed scan's
    /// stats (if any) so the caller has something to report.
    AlreadyRunning(Option<ScanStats>),
}

// ---------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------

enum ScanMode {
    Full,
    Incremental,
}

/// Full rebuild: walk the media roots, extract tags for every audio file,
/// group nothing (albums/artists are derived at read time), and persist
/// atomically. This is also the recovery path for a refused newer-schema
/// index. Returns `AlreadyRunning` instead of starting a duplicate scan.
pub async fn reindex(
    state: &ReindexState,
    data_dir: &Path,
    media_roots: &[PathBuf],
) -> Result<ReindexOutcome, IndexError> {
    run_scan(state, data_dir, media_roots, ScanMode::Full).await
}

/// Incremental refresh: compare each file's `(path, mtime, size)` against
/// the stored `IndexEntry` and only re-extract changed files; rows for
/// files that disappeared are removed (albums that lost their last track
/// vanish automatically, since albums are derived at read time). A stored
/// index with a newer schema is treated as absent and this refresh becomes
/// a full rebuild — an explicit scan is the sanctioned overwrite path.
pub async fn refresh_incremental(
    state: &ReindexState,
    data_dir: &Path,
    media_roots: &[PathBuf],
) -> Result<ReindexOutcome, IndexError> {
    run_scan(state, data_dir, media_roots, ScanMode::Incremental).await
}

async fn run_scan(
    state: &ReindexState,
    data_dir: &Path,
    media_roots: &[PathBuf],
    mode: ScanMode,
) -> Result<ReindexOutcome, IndexError> {
    let Some(guard) = state.try_begin() else {
        return Ok(ReindexOutcome::AlreadyRunning(state.last_stats()));
    };

    let previous = match mode {
        ScanMode::Full => MusicIndex::empty(),
        ScanMode::Incremental => match load(data_dir) {
            Ok(index) => index,
            Err(IndexError::NewerSchema { found, supported }) => {
                warn!(
                    "music index schema {found} > supported {supported}; \
                     explicit scan requested — rebuilding from scratch"
                );
                MusicIndex::empty()
            }
            Err(e) => return Err(e),
        },
    };

    // The walk + per-file tag extraction is blocking filesystem work; move
    // it off the async runtime. Only owned data crosses the boundary.
    let data_dir_owned = data_dir.to_path_buf();
    let roots_owned = media_roots.to_vec();
    let started = Instant::now();
    let (_index, stats) =
        tokio::task::spawn_blocking(move || scan(&data_dir_owned, &roots_owned, previous, started))
            .await
            .map_err(|e| {
                IndexError::Io(std::io::Error::other(format!(
                    "music scan task failed: {e}"
                )))
            })??;

    state.record_stats(stats.clone());
    drop(guard);
    Ok(ReindexOutcome::Completed(stats))
}

/// The blocking scan body: walk, diff against the previous index, extract
/// where needed, persist atomically. `previous` is empty for a full scan.
fn scan(
    data_dir: &Path,
    media_roots: &[PathBuf],
    previous: MusicIndex,
    started: Instant,
) -> Result<(MusicIndex, ScanStats), IndexError> {
    let mut stats = ScanStats::default();

    // Canonicalize the roots once; every path the walker yields is inside
    // one of these, and `extract_tags` re-checks confinement per file
    // (T-13-39 — enforced here as well as in `tags.rs`).
    let canonical_roots: Vec<PathBuf> = media_roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect();

    // BTreeMap dedupes (a symlink to a sibling file resolves to the same
    // canonical path) and makes the walk order deterministic.
    let mut files: BTreeMap<PathBuf, MusicSource> = BTreeMap::new();
    for root in &canonical_roots {
        walk_dir(root, root, &mut files, &mut stats);
    }

    let mut prev_entries: HashMap<PathBuf, IndexEntry> = previous
        .entries
        .into_iter()
        .map(|entry| (entry.track.id.path.clone(), entry))
        .collect();

    let mut entries: Vec<IndexEntry> = Vec::with_capacity(files.len());
    for (path, source) in files {
        stats.scanned += 1;
        let meta = match std::fs::metadata(&path) {
            Ok(meta) => meta,
            Err(e) => {
                debug!("music scan: cannot stat {}: {e}", path.display());
                stats.skipped += 1;
                continue;
            }
        };
        let mtime_ms = mtime_millis(&meta);
        let size_bytes = meta.len();

        if let Some(prev) = prev_entries.remove(&path) {
            if prev.mtime_ms == mtime_ms && prev.size_bytes == size_bytes {
                // Unchanged since last scan: keep the row (including its
                // lazily-backfilled content_hash) without re-extracting.
                entries.push(prev);
                continue;
            }
            // Changed: fall through and re-extract. The row identity
            // `(source, path)` is preserved; the content_hash column is
            // reset to None because the bytes changed.
        }

        match extract_tags(&path, &canonical_roots) {
            Ok(raw) => {
                stats.extracted += 1;
                let id = TrackId {
                    source,
                    path: path.clone(),
                };
                entries.push(IndexEntry {
                    track: track_from_raw(id, raw),
                    mtime_ms,
                    size_bytes,
                });
            }
            Err(e) => {
                // Per-file failures never abort the walk (T-13-44).
                debug!("music scan: skipping {}: {e}", path.display());
                stats.skipped += 1;
            }
        }
    }

    // Whatever is left in prev_entries has no file behind it any more.
    stats.removed = prev_entries.len() as u64;

    entries.sort_by(|a, b| track_cmp(&a.track, &b.track));
    let index = MusicIndex {
        schema_version: MUSIC_SCHEMA_VERSION,
        scanned_at: chrono::Utc::now().to_rfc3339(),
        entries,
    };
    save_atomic(data_dir, &index)?;
    stats.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok((index, stats))
}

fn track_from_raw(id: TrackId, raw: super::tags::RawTags) -> Track {
    let fallback_title = || {
        id.path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Unknown".to_string())
    };
    Track {
        title: raw.title.unwrap_or_else(fallback_title),
        artist: raw.artist,
        album: raw.album,
        album_artist: raw.album_artist,
        track_number: raw.track,
        disc_number: raw.disc,
        year: raw.year,
        duration_secs: raw.duration_secs,
        has_tags: raw.has_tags,
        content_hash: None,
        id,
    }
}

fn mtime_millis(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn has_audio_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_ascii_lowercase();
            AUDIO_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

/// Which `MusicSource` a file under `root` belongs to. The
/// `purchased-content` root is the peer byte cache laid out as
/// `<onion>/<content_id>`; everything else is the node's own library.
fn source_for(root: &Path, file: &Path) -> MusicSource {
    if root.file_name().and_then(|n| n.to_str()) == Some("purchased-content") {
        let onion = file
            .strip_prefix(root)
            .ok()
            .and_then(|rel| rel.components().next())
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_default();
        MusicSource::Peer { onion }
    } else {
        MusicSource::OwnLibrary
    }
}

/// Recursive walk confined to `canonical_root`. Symlinks are resolved and
/// followed ONLY when their canonical target stays inside the root —
/// a symlink escaping the media roots is skipped, not followed (T-13-39).
/// Dotfiles/dot-directories are ignored.
fn walk_dir(
    dir: &Path,
    canonical_root: &Path,
    out: &mut BTreeMap<PathBuf, MusicSource>,
    stats: &mut ScanStats,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            debug!("music scan: cannot read dir {}: {e}", dir.display());
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(link_meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        let resolved = if link_meta.file_type().is_symlink() {
            match path.canonicalize() {
                Ok(target) if target.starts_with(canonical_root) => target,
                _ => {
                    // Escapes the media roots (or dangling): skip, never
                    // follow. This is the symlink-escape mitigation.
                    stats.skipped += 1;
                    continue;
                }
            }
        } else {
            path
        };
        let Ok(meta) = std::fs::metadata(&resolved) else {
            continue;
        };
        if meta.is_dir() {
            walk_dir(&resolved, canonical_root, out, stats);
        } else if meta.is_file() && has_audio_extension(&resolved) {
            let source = source_for(canonical_root, &resolved);
            out.insert(resolved, source);
        }
    }
}

// ---------------------------------------------------------------------
// Tests — one per 13-07 Task 1 <behavior> bullet, over programmatic FLAC
// fixtures built into tempdirs (the 13-04 pattern: byte-level builders,
// zero committed binary fixtures).
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // -- FLAC fixture builder (compact version of the byte-level builder
    // -- established in music/tags.rs's tests; FLAC alone suffices here —
    // -- format coverage is tags.rs's job, this module tests the index).

    fn flac_streaminfo(sample_rate: u32, total_samples: u64) -> Vec<u8> {
        let mut info: u32 = (sample_rate << 12) | (1 << 9) | (15 << 4);
        info |= ((total_samples >> 32) as u32) & 0xF;
        let mut content = Vec::with_capacity(34);
        content.extend_from_slice(&[0, 0, 0, 0]); // min/max block size (unused)
        content.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // min/max frame size (unused)
        content.extend_from_slice(&info.to_be_bytes());
        content.extend_from_slice(&((total_samples & 0xFFFF_FFFF) as u32).to_be_bytes());
        content.extend_from_slice(&[0u8; 16]); // MD5 (unused)
        content
    }

    fn flac_block(block_type: u8, is_last: bool, content: &[u8]) -> Vec<u8> {
        let mut block = Vec::with_capacity(4 + content.len());
        block.push(((is_last as u8) << 7) | (block_type & 0x7F));
        let len = content.len() as u32;
        block.push(((len >> 16) & 0xFF) as u8);
        block.push(((len >> 8) & 0xFF) as u8);
        block.push((len & 0xFF) as u8);
        block.extend_from_slice(content);
        block
    }

    fn vorbis_comment_block(comments: &[(&str, &str)]) -> Vec<u8> {
        let vendor = b"test-vendor";
        let mut content = Vec::new();
        content.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        content.extend_from_slice(vendor);
        content.extend_from_slice(&(comments.len() as u32).to_le_bytes());
        for (key, value) in comments {
            let field = format!("{key}={value}");
            content.extend_from_slice(&(field.len() as u32).to_le_bytes());
            content.extend_from_slice(field.as_bytes());
        }
        content
    }

    fn build_flac(comments: &[(&str, &str)]) -> Vec<u8> {
        let streaminfo = flac_streaminfo(44100, 44100 * 3);
        let mut file = Vec::new();
        file.extend_from_slice(b"fLaC");
        file.extend_from_slice(&flac_block(0, false, &streaminfo));
        file.extend_from_slice(&flac_block(4, true, &vorbis_comment_block(comments)));
        file
    }

    /// Write a tagged FLAC track fixture into `dir`.
    #[allow(clippy::too_many_arguments)]
    fn write_track(
        dir: &Path,
        name: &str,
        title: &str,
        artist: &str,
        album: &str,
        album_artist: &str,
        track_no: u32,
    ) -> PathBuf {
        let bytes = build_flac(&[
            ("TITLE", title),
            ("ARTIST", artist),
            ("ALBUM", album),
            ("ALBUMARTIST", album_artist),
            ("TRACKNUMBER", &track_no.to_string()),
            ("DISCNUMBER", "1"),
            ("DATE", "2024"),
        ]);
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("write track fixture");
        path
    }

    /// A three-track, two-album fixture library: album "Alpha" by "X"
    /// (2 tracks) and album "Beta" by "Y" (1 track).
    fn seed_library(root: &Path) {
        write_track(root, "a1.flac", "Dawn", "X", "Alpha", "X", 1);
        write_track(root, "a2.flac", "Noon", "X", "Alpha", "X", 2);
        write_track(root, "b1.flac", "Dusk", "Y", "Beta", "Y", 1);
    }

    fn state() -> ReindexState {
        ReindexState::default()
    }

    fn completed(outcome: ReindexOutcome) -> ScanStats {
        match outcome {
            ReindexOutcome::Completed(stats) => stats,
            ReindexOutcome::AlreadyRunning(_) => panic!("expected a completed scan"),
        }
    }

    #[tokio::test]
    async fn first_reindex_builds_grouped_library() {
        let data_dir = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        seed_library(media.path());
        let roots = vec![media.path().to_path_buf()];

        let stats = completed(
            reindex(&state(), data_dir.path(), &roots)
                .await
                .expect("first reindex succeeds"),
        );
        assert_eq!(stats.extracted, 3);
        assert_eq!(stats.scanned, 3);

        let snapshot = load(data_dir.path()).expect("index loads").snapshot();
        assert_eq!(snapshot.tracks.len(), 3);

        // Albums grouped on (album_artist, album) per 13-MUSIC-MODEL.md,
        // ordered by album artist then title: Alpha/X before Beta/Y.
        assert_eq!(snapshot.albums.len(), 2);
        assert_eq!(snapshot.albums[0].id.album, "Alpha");
        assert_eq!(snapshot.albums[0].id.album_artist.as_deref(), Some("X"));
        assert_eq!(snapshot.albums[0].track_ids.len(), 2);
        assert_eq!(snapshot.albums[1].id.album, "Beta");
        assert_eq!(snapshot.albums[1].track_ids.len(), 1);

        // Artists derived from the artist tag.
        assert_eq!(snapshot.artists.len(), 2);
        assert_eq!(snapshot.artists[0].id.0, "X");
        assert_eq!(snapshot.artists[1].id.0, "Y");

        // Alpha's tracks ordered by track number: Dawn (1) then Noon (2).
        let alpha_first = &snapshot.albums[0].track_ids[0];
        let dawn = snapshot
            .tracks
            .iter()
            .find(|t| &t.id == alpha_first)
            .unwrap();
        assert_eq!(dawn.title, "Dawn");
        assert!(dawn
            .id
            .path
            .starts_with(media.path().canonicalize().unwrap()));
        assert_eq!(dawn.id.source, MusicSource::OwnLibrary);
    }

    #[tokio::test]
    async fn refresh_adds_new_file_without_reextracting_unchanged() {
        let data_dir = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        seed_library(media.path());
        let roots = vec![media.path().to_path_buf()];
        let st = state();

        completed(reindex(&st, data_dir.path(), &roots).await.unwrap());

        write_track(media.path(), "b2.flac", "Night", "Y", "Beta", "Y", 2);
        let stats = completed(
            refresh_incremental(&st, data_dir.path(), &roots)
                .await
                .expect("refresh succeeds"),
        );

        // Four files seen, but only the new one had its tags extracted.
        assert_eq!(stats.scanned, 4);
        assert_eq!(stats.extracted, 1);
        assert_eq!(stats.removed, 0);

        let snapshot = load(data_dir.path()).unwrap().snapshot();
        assert_eq!(snapshot.tracks.len(), 4);
        assert!(snapshot.tracks.iter().any(|t| t.title == "Night"));
    }

    #[tokio::test]
    async fn refresh_removes_deleted_file_and_its_emptied_album() {
        let data_dir = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        seed_library(media.path());
        let roots = vec![media.path().to_path_buf()];
        let st = state();

        completed(reindex(&st, data_dir.path(), &roots).await.unwrap());

        // Beta's only track disappears — the row goes, and with it the
        // whole (derived) album.
        std::fs::remove_file(media.path().join("b1.flac")).unwrap();
        let stats = completed(
            refresh_incremental(&st, data_dir.path(), &roots)
                .await
                .unwrap(),
        );
        assert_eq!(stats.removed, 1);

        let snapshot = load(data_dir.path()).unwrap().snapshot();
        assert_eq!(snapshot.tracks.len(), 2);
        assert_eq!(snapshot.albums.len(), 1);
        assert_eq!(snapshot.albums[0].id.album, "Alpha");
    }

    #[tokio::test]
    async fn refresh_reextracts_changed_file_in_place_keeping_identity() {
        let data_dir = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        seed_library(media.path());
        let roots = vec![media.path().to_path_buf()];
        let st = state();

        completed(reindex(&st, data_dir.path(), &roots).await.unwrap());
        let before = load(data_dir.path()).unwrap().snapshot();
        let old_id = before
            .tracks
            .iter()
            .find(|t| t.title == "Dusk")
            .unwrap()
            .id
            .clone();

        // Retag b1.flac in place and push its mtime clearly forward so the
        // (mtime, size) comparison sees a change even on coarse clocks.
        let path = media.path().join("b1.flac");
        write_track(
            media.path(),
            "b1.flac",
            "Dusk (Remaster)",
            "Y",
            "Beta",
            "Y",
            1,
        );
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(10))
            .unwrap();

        let stats = completed(
            refresh_incremental(&st, data_dir.path(), &roots)
                .await
                .unwrap(),
        );
        assert_eq!(stats.extracted, 1, "only the changed file re-extracts");
        assert_eq!(stats.removed, 0);

        let after = load(data_dir.path()).unwrap().snapshot();
        assert_eq!(after.tracks.len(), 3, "updated in place, not duplicated");
        let updated = after
            .tracks
            .iter()
            .find(|t| t.title == "Dusk (Remaster)")
            .expect("retagged track present");
        assert_eq!(updated.id, old_id, "identity (source, path) is preserved");
    }

    #[tokio::test]
    async fn newer_schema_index_is_refused_then_rebuilt() {
        let data_dir = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        seed_library(media.path());
        let roots = vec![media.path().to_path_buf()];

        // An index written by a "future" binary.
        let music_dir = data_dir.path().join("music");
        std::fs::create_dir_all(&music_dir).unwrap();
        std::fs::write(
            music_dir.join("index.json"),
            serde_json::json!({
                "schema_version": MUSIC_SCHEMA_VERSION + 1,
                "scanned_at": "2099-01-01T00:00:00Z",
                "entries": [],
                "field_from_the_future": true,
            })
            .to_string(),
        )
        .unwrap();

        // Refused with the distinct variant — not defaulted, not misread.
        let err = load(data_dir.path()).expect_err("newer schema must be refused");
        assert!(
            matches!(
                err,
                IndexError::NewerSchema { found, supported }
                    if found == MUSIC_SCHEMA_VERSION + 1 && supported == MUSIC_SCHEMA_VERSION
            ),
            "expected NewerSchema, got {err:?}"
        );

        // An explicit reindex is the recovery path: full rebuild.
        completed(reindex(&state(), data_dir.path(), &roots).await.unwrap());
        let rebuilt = load(data_dir.path()).expect("rebuilt index loads");
        assert_eq!(rebuilt.schema_version, MUSIC_SCHEMA_VERSION);
        assert_eq!(rebuilt.entries.len(), 3);
    }

    #[test]
    fn concurrent_read_never_sees_partial_index() {
        let data_dir = tempfile::tempdir().unwrap();

        let synthetic = |n: usize| -> MusicIndex {
            let entries = (0..n)
                .map(|i| IndexEntry {
                    track: Track {
                        id: TrackId {
                            source: MusicSource::OwnLibrary,
                            path: PathBuf::from(format!("/synthetic/track-{i:04}.flac")),
                        },
                        title: format!(
                            "A deliberately long synthetic title {i} {}",
                            "x".repeat(120)
                        ),
                        artist: Some(format!("Artist {i}")),
                        album: Some(format!("Album {}", i % 7)),
                        album_artist: Some(format!("Album Artist {}", i % 7)),
                        track_number: Some(i as u32),
                        disc_number: Some(1),
                        year: Some(2024),
                        duration_secs: 180,
                        has_tags: true,
                        content_hash: None,
                    },
                    mtime_ms: 1_700_000_000_000 + i as u64,
                    size_bytes: 4096,
                })
                .collect();
            MusicIndex {
                schema_version: MUSIC_SCHEMA_VERSION,
                scanned_at: chrono::Utc::now().to_rfc3339(),
                entries,
            }
        };
        // Distinct, unambiguous sizes: a torn/partial read of the large
        // index can't parse as either (corrupt JSON would default to 0
        // entries, which is also not in the allowed set).
        let small = synthetic(1);
        let large = synthetic(300);
        save_atomic(data_dir.path(), &small).unwrap();

        let dir = data_dir.path().to_path_buf();
        let reader = std::thread::spawn(move || {
            for _ in 0..200 {
                let index = load(&dir).expect("a concurrent read never errors");
                let len = index.entries.len();
                assert!(
                    len == 1 || len == 300,
                    "read a partial/mixed index: {len} entries"
                );
            }
        });

        for i in 0..100 {
            let index = if i % 2 == 0 { &large } else { &small };
            save_atomic(data_dir.path(), index).unwrap();
        }
        reader.join().expect("reader thread panicked");
    }

    #[tokio::test]
    async fn empty_directory_reindex_yields_empty_index_with_timestamp() {
        let data_dir = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        let roots = vec![media.path().to_path_buf()];

        let stats = completed(
            reindex(&state(), data_dir.path(), &roots)
                .await
                .expect("empty library is not an error"),
        );
        assert_eq!(stats.scanned, 0);
        assert_eq!(stats.extracted, 0);

        let snapshot = load(data_dir.path()).unwrap().snapshot();
        assert!(snapshot.tracks.is_empty());
        assert!(snapshot.albums.is_empty());
        assert!(snapshot.artists.is_empty());
        assert!(!snapshot.scanned_at.is_empty(), "scanned_at is populated");
        assert_eq!(snapshot.schema_version, MUSIC_SCHEMA_VERSION);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_escaping_media_roots_is_skipped_not_followed() {
        let data_dir = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let roots = vec![media.path().to_path_buf()];

        write_track(media.path(), "inside.flac", "Inside", "X", "Alpha", "X", 1);
        let evil = write_track(outside.path(), "evil.flac", "Evil", "Z", "Zeta", "Z", 1);
        std::os::unix::fs::symlink(&evil, media.path().join("link.flac")).unwrap();
        std::os::unix::fs::symlink(outside.path(), media.path().join("linkdir")).unwrap();

        let stats = completed(reindex(&state(), data_dir.path(), &roots).await.unwrap());
        assert!(stats.skipped >= 2, "both escaping symlinks are skipped");

        let snapshot = load(data_dir.path()).unwrap().snapshot();
        assert_eq!(snapshot.tracks.len(), 1);
        assert_eq!(snapshot.tracks[0].title, "Inside");
        let outside_root = outside.path().canonicalize().unwrap();
        assert!(
            snapshot
                .tracks
                .iter()
                .all(|t| !t.id.path.starts_with(&outside_root)),
            "nothing outside the media roots was indexed"
        );
    }

    #[tokio::test]
    async fn second_concurrent_scan_reports_already_running() {
        let data_dir = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        let roots = vec![media.path().to_path_buf()];
        let st = state();

        // Simulate a scan in flight by holding the guard.
        let guard = st.try_begin().expect("first acquisition succeeds");
        let outcome = reindex(&st, data_dir.path(), &roots).await.unwrap();
        assert!(
            matches!(outcome, ReindexOutcome::AlreadyRunning(_)),
            "a second scan must not start while one is running"
        );
        drop(guard);

        // Once released, scanning works again.
        completed(reindex(&st, data_dir.path(), &roots).await.unwrap());
        assert!(!st.is_running());
    }
}
