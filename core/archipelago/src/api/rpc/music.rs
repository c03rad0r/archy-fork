//! `music.*` RPC surface (13-07 Task 2) — the library index behind
//! `SongGrid`. Mirrors `assistant_chat.rs`'s shape: every `music.*` method
//! is added inside this file's own match; `dispatcher.rs` registers exactly
//! one guarded arm for the whole `music.` prefix
//! (`grep -c 'starts_with("music.")' dispatcher.rs` == 1).
//!
//! Reached only after the session-cookie + CSRF + `role.can_access()` gate
//! in `api/rpc/mod.rs` — no bespoke auth here, and no `music.*` method is
//! ever added to `UNAUTHENTICATED_METHODS` (T-13-40, asserted by
//! `music_methods_require_session` below).

use super::RpcHandler;
use crate::music::index::{self, IndexError, MusicIndex, ReindexOutcome, ReindexState};
use crate::music::{media_roots, AlbumId, MUSIC_SCHEMA_VERSION};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// `music.list-tracks` page-size cap: a huge library cannot be pulled in
/// one response (T-13-41). An out-of-range `limit` is clamped, not
/// rejected, so a UI bug degrades to a smaller page rather than an error.
const MAX_TRACK_LIMIT: u64 = 500;
const DEFAULT_TRACK_LIMIT: u64 = 100;

impl RpcHandler {
    /// Prefix sub-dispatcher for `music.*` — the only entry point
    /// `dispatcher.rs` knows about, mirroring 13-01's `assistant.` arm.
    pub(in crate::api::rpc) async fn handle_music(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        music_dispatch(
            index::shared_state(),
            &self.config.data_dir,
            media_roots(&self.config),
            method,
            params,
        )
        .await
    }
}

/// The actual dispatch, parameterized on the scan state and paths so tests
/// can drive it against tempdirs and their own (leaked) states without
/// constructing an `RpcHandler`.
async fn music_dispatch(
    state: &'static ReindexState,
    data_dir: &Path,
    roots: Vec<PathBuf>,
    method: &str,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value> {
    match method {
        "music.list-albums" => handle_music_list_albums(data_dir).await,
        "music.list-artists" => handle_music_list_artists(data_dir).await,
        "music.list-tracks" => handle_music_list_tracks(data_dir, params).await,
        "music.status" => handle_music_status(state, data_dir).await,
        "music.reindex" => handle_music_reindex(state, data_dir.to_path_buf(), roots, params).await,
        other => anyhow::bail!("no such music method: {other}"),
    }
}

/// Read the index for serving. A newer-schema index is treated as absent
/// (empty library) per `13-MUSIC-MODEL.md`'s downgrade contract — readers
/// never overwrite or reinterpret it; only an explicit `music.reindex`
/// rebuilds.
fn load_for_read(data_dir: &Path) -> Result<MusicIndex> {
    match index::load(data_dir) {
        Ok(idx) => Ok(idx),
        Err(IndexError::NewerSchema { found, supported }) => {
            tracing::warn!(
                "music index schema {found} > supported {supported}; \
                 serving an empty library until an explicit reindex"
            );
            Ok(MusicIndex::empty())
        }
        Err(e) => Err(e.into()),
    }
}

/// music.list-albums — all albums in the deterministic library order, with
/// `scanned_at` and a total count. An empty library is empty arrays plus a
/// timestamp, never null and never an error.
async fn handle_music_list_albums(data_dir: &Path) -> Result<serde_json::Value> {
    let snapshot = load_for_read(data_dir)?.snapshot();
    let albums: Vec<serde_json::Value> = snapshot
        .albums
        .iter()
        .map(|album| {
            serde_json::json!({
                "id": album.id,
                "album": album.id.album,
                "album_artist": album.id.album_artist,
                "track_count": album.track_ids.len(),
                "track_ids": album.track_ids,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "albums": albums,
        "total": snapshot.albums.len(),
        "scanned_at": snapshot.scanned_at,
    }))
}

/// music.list-artists — all artists in deterministic order.
async fn handle_music_list_artists(data_dir: &Path) -> Result<serde_json::Value> {
    let snapshot = load_for_read(data_dir)?.snapshot();
    let artists: Vec<serde_json::Value> = snapshot
        .artists
        .iter()
        .map(|artist| {
            serde_json::json!({
                "id": artist.id,
                "name": artist.id.0,
                "track_count": artist.track_ids.len(),
                "track_ids": artist.track_ids,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "artists": artists,
        "total": snapshot.artists.len(),
        "scanned_at": snapshot.scanned_at,
    }))
}

/// music.list-tracks — paginated tracks, optionally filtered by `album_id`
/// (`{ "album": string, "album_artist": string|null }`). `limit` defaults
/// to 100 and is clamped to [1, 500]; `total` is the filtered count before
/// pagination so the UI can page honestly.
async fn handle_music_list_tracks(
    data_dir: &Path,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value> {
    let params = params.unwrap_or_default();
    let album_id: Option<AlbumId> = match params.get("album_id") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => Some(serde_json::from_value(value.clone()).map_err(|_| {
            anyhow::anyhow!("Invalid album_id: expected {{ album, album_artist }}")
        })?),
    };
    let limit = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_TRACK_LIMIT)
        .clamp(1, MAX_TRACK_LIMIT) as usize;
    let offset = params.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    let snapshot = load_for_read(data_dir)?.snapshot();
    let filtered: Vec<&crate::music::Track> = snapshot
        .tracks
        .iter()
        .filter(|track| match &album_id {
            None => true,
            Some(id) => {
                track.album.as_deref() == Some(id.album.as_str())
                    && track.album_artist == id.album_artist
            }
        })
        .collect();
    let total = filtered.len();
    let page: Vec<&crate::music::Track> = filtered.into_iter().skip(offset).take(limit).collect();

    Ok(serde_json::json!({
        "tracks": page,
        "total": total,
        "limit": limit,
        "offset": offset,
        "scanned_at": snapshot.scanned_at,
    }))
}

/// music.status — the last scan's stats, whether a scan is running, and
/// the schema version (both the binary's and the on-disk index's).
async fn handle_music_status(state: &ReindexState, data_dir: &Path) -> Result<serde_json::Value> {
    let index = load_for_read(data_dir)?;
    Ok(serde_json::json!({
        "running": state.is_running(),
        "last_stats": state.last_stats(),
        "schema_version": MUSIC_SCHEMA_VERSION,
        "index_schema_version": index.schema_version,
        "scanned_at": index.scanned_at,
        "track_count": index.entries.len(),
    }))
}

/// music.reindex — start a scan and return immediately; the walk runs in a
/// spawned task (a synchronous reindex would hold an RPC connection for
/// the length of a library walk) and holds no shared lock across it.
/// Params: `{ "incremental": bool }` (default false). The default is the
/// on-demand full rebuild; `incremental: true` runs the cheap
/// stat-comparison refresh so a file added/changed/removed since the last
/// scan is reflected without re-extracting the whole library.
/// A second call while one is running reports already-running with the
/// previous scan's stats instead of queueing a duplicate (T-13-41) — the
/// in-progress flag inside `index::run_scan` is the authoritative guard.
async fn handle_music_reindex(
    state: &'static ReindexState,
    data_dir: PathBuf,
    roots: Vec<PathBuf>,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value> {
    let incremental = params
        .as_ref()
        .and_then(|p| p.get("incremental"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if state.is_running() {
        return Ok(serde_json::json!({
            "started": false,
            "already_running": true,
            "running": true,
            "stats": state.last_stats(),
        }));
    }
    tokio::spawn(async move {
        let outcome = if incremental {
            index::refresh_incremental(state, &data_dir, &roots).await
        } else {
            index::reindex(state, &data_dir, &roots).await
        };
        match outcome {
            Ok(ReindexOutcome::Completed(stats)) => {
                tracing::info!(
                    "music {} scan complete: {} scanned, {} extracted, {} skipped, {} removed in {}ms",
                    if incremental { "incremental" } else { "full" },
                    stats.scanned, stats.extracted, stats.skipped, stats.removed, stats.elapsed_ms
                );
            }
            Ok(ReindexOutcome::AlreadyRunning(prev)) => {
                tracing::info!(
                    "music reindex: a scan was already running (previous: {prev:?}); not duplicating"
                );
            }
            Err(e) => tracing::warn!("music reindex failed: {e:#}"),
        }
    });
    Ok(serde_json::json!({
        "started": true,
        "already_running": false,
        "running": true,
    }))
}

// ---------------------------------------------------------------------
// Tests — one per 13-07 Task 2 <behavior> bullet, driving music_dispatch
// against tempdirs and leaked per-test ReindexStates (never the process-
// wide shared_state, so parallel tests can't interfere).
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::index::{reindex, ReindexState};

    fn leaked_state() -> &'static ReindexState {
        Box::leak(Box::new(ReindexState::default()))
    }

    /// Minimal tagged-FLAC fixture (same byte-level builder family as
    /// music/tags.rs and music/index.rs tests — no committed binaries).
    fn build_flac(comments: &[(&str, &str)]) -> Vec<u8> {
        let info_bits: u32 = (44100 << 12) | (1 << 9) | (15 << 4);
        let mut streaminfo = Vec::with_capacity(34);
        streaminfo.extend_from_slice(&[0, 0, 0, 0]);
        streaminfo.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        streaminfo.extend_from_slice(&info_bits.to_be_bytes());
        streaminfo.extend_from_slice(&(44100u32 * 3).to_be_bytes());
        streaminfo.extend_from_slice(&[0u8; 16]);

        let block = |block_type: u8, is_last: bool, content: &[u8]| {
            let mut b = Vec::with_capacity(4 + content.len());
            b.push(((is_last as u8) << 7) | (block_type & 0x7F));
            let len = content.len() as u32;
            b.push(((len >> 16) & 0xFF) as u8);
            b.push(((len >> 8) & 0xFF) as u8);
            b.push((len & 0xFF) as u8);
            b.extend_from_slice(content);
            b
        };

        let vendor = b"test-vendor";
        let mut vorbis = Vec::new();
        vorbis.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        vorbis.extend_from_slice(vendor);
        vorbis.extend_from_slice(&(comments.len() as u32).to_le_bytes());
        for (key, value) in comments {
            let field = format!("{key}={value}");
            vorbis.extend_from_slice(&(field.len() as u32).to_le_bytes());
            vorbis.extend_from_slice(field.as_bytes());
        }

        let mut file = Vec::new();
        file.extend_from_slice(b"fLaC");
        file.extend_from_slice(&block(0, false, &streaminfo));
        file.extend_from_slice(&block(4, true, &vorbis));
        file
    }

    fn write_track(dir: &Path, name: &str, title: &str, artist: &str, album: &str, no: u32) {
        let bytes = build_flac(&[
            ("TITLE", title),
            ("ARTIST", artist),
            ("ALBUM", album),
            ("ALBUMARTIST", artist),
            ("TRACKNUMBER", &no.to_string()),
            ("DISCNUMBER", "1"),
            ("DATE", "2024"),
        ]);
        std::fs::write(dir.join(name), bytes).expect("write track fixture");
    }

    /// data_dir + media root with 2 albums / 3 tracks, already indexed.
    async fn indexed_library() -> (tempfile::TempDir, tempfile::TempDir, Vec<PathBuf>) {
        let data_dir = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        write_track(media.path(), "a1.flac", "Dawn", "X", "Alpha", 1);
        write_track(media.path(), "a2.flac", "Noon", "X", "Alpha", 2);
        write_track(media.path(), "b1.flac", "Dusk", "Y", "Beta", 1);
        let roots = vec![media.path().to_path_buf()];
        reindex(&ReindexState::default(), data_dir.path(), &roots)
            .await
            .expect("fixture reindex succeeds");
        (data_dir, media, roots)
    }

    async fn dispatch(
        state: &'static ReindexState,
        data_dir: &Path,
        roots: &[PathBuf],
        method: &str,
        params: Option<serde_json::Value>,
    ) -> serde_json::Value {
        music_dispatch(state, data_dir, roots.to_vec(), method, params)
            .await
            .unwrap_or_else(|e| panic!("{method} failed: {e:#}"))
    }

    #[test]
    fn music_methods_require_session() {
        // Mirrors assistant_methods_require_session: the whole surface
        // rides the normal session/CSRF/RBAC gate; nothing music.* may
        // ever appear in the unauthenticated allowlist (T-13-40).
        let has_music_method = crate::api::rpc::UNAUTHENTICATED_METHODS
            .iter()
            .any(|m| m.starts_with("music."));
        assert!(
            !has_music_method,
            "music.* must never be added to UNAUTHENTICATED_METHODS"
        );
    }

    #[tokio::test]
    async fn list_albums_is_deterministic_with_scanned_at_and_total() {
        let (data_dir, _media, roots) = indexed_library().await;
        let state = leaked_state();

        let first = dispatch(state, data_dir.path(), &roots, "music.list-albums", None).await;
        let second = dispatch(state, data_dir.path(), &roots, "music.list-albums", None).await;
        assert_eq!(first, second, "ordering is stable across repeated calls");

        assert_eq!(first["total"], 2);
        assert!(first["scanned_at"].as_str().is_some_and(|s| !s.is_empty()));
        let albums = first["albums"].as_array().unwrap();
        assert_eq!(albums[0]["album"], "Alpha");
        assert_eq!(albums[0]["track_count"], 2);
        assert_eq!(albums[1]["album"], "Beta");
    }

    #[tokio::test]
    async fn list_tracks_filters_by_album_and_paginates_with_clamped_limit() {
        let (data_dir, _media, roots) = indexed_library().await;
        let state = leaked_state();

        // album_id filter.
        let alpha = dispatch(
            state,
            data_dir.path(),
            &roots,
            "music.list-tracks",
            Some(serde_json::json!({ "album_id": { "album": "Alpha", "album_artist": "X" } })),
        )
        .await;
        assert_eq!(alpha["total"], 2);
        assert_eq!(alpha["tracks"].as_array().unwrap().len(), 2);

        // Pagination: limit/offset slice the filtered set; total is the
        // pre-pagination count.
        let page = dispatch(
            state,
            data_dir.path(),
            &roots,
            "music.list-tracks",
            Some(serde_json::json!({ "limit": 1, "offset": 1 })),
        )
        .await;
        assert_eq!(page["total"], 3);
        assert_eq!(page["tracks"].as_array().unwrap().len(), 1);
        assert_eq!(page["limit"], 1);
        assert_eq!(page["offset"], 1);

        // A huge limit is clamped, not rejected.
        let clamped = dispatch(
            state,
            data_dir.path(),
            &roots,
            "music.list-tracks",
            Some(serde_json::json!({ "limit": 1_000_000 })),
        )
        .await;
        assert_eq!(clamped["limit"], 500, "limit clamps to the 500 cap");
        // And a zero limit degrades to the smallest page, still no error.
        let floor = dispatch(
            state,
            data_dir.path(),
            &roots,
            "music.list-tracks",
            Some(serde_json::json!({ "limit": 0 })),
        )
        .await;
        assert_eq!(floor["limit"], 1);
    }

    #[tokio::test]
    async fn status_reports_stats_running_flag_and_schema_version() {
        let (data_dir, _media, roots) = indexed_library().await;
        let state = leaked_state();

        // Run a scan through THIS state so last_stats is populated.
        reindex(state, data_dir.path(), &roots).await.unwrap();

        let status = dispatch(state, data_dir.path(), &roots, "music.status", None).await;
        assert_eq!(status["running"], false);
        assert_eq!(status["schema_version"], MUSIC_SCHEMA_VERSION);
        assert_eq!(status["last_stats"]["scanned"], 3);
        assert_eq!(status["track_count"], 3);
        assert!(status["scanned_at"].as_str().is_some_and(|s| !s.is_empty()));
    }

    #[tokio::test]
    async fn reindex_while_running_reports_already_running_not_a_duplicate() {
        let (data_dir, _media, roots) = indexed_library().await;
        let state = leaked_state();

        // Simulate a scan in flight by holding the in-progress guard.
        let guard = state.try_begin().expect("guard acquired");
        let response = dispatch(state, data_dir.path(), &roots, "music.reindex", None).await;
        assert_eq!(response["already_running"], true);
        assert_eq!(response["started"], false);
        assert_eq!(response["running"], true);
        drop(guard);

        // Free again: a reindex starts and returns immediately.
        let response = dispatch(state, data_dir.path(), &roots, "music.reindex", None).await;
        assert_eq!(response["started"], true);
    }

    #[tokio::test]
    async fn reindex_incremental_mode_refreshes_without_full_reextraction() {
        let (data_dir, media, roots) = indexed_library().await;
        let state = leaked_state();

        write_track(media.path(), "b2.flac", "Night", "Y", "Beta", 2);
        let response = dispatch(
            state,
            data_dir.path(),
            &roots,
            "music.reindex",
            Some(serde_json::json!({ "incremental": true })),
        )
        .await;
        assert_eq!(response["started"], true);

        // The scan runs in a spawned task; poll until it records stats.
        let mut stats = None;
        for _ in 0..200 {
            if !state.is_running() {
                if let Some(s) = state.last_stats() {
                    stats = Some(s);
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let stats = stats.expect("incremental scan completed");
        assert_eq!(stats.scanned, 4);
        assert_eq!(
            stats.extracted, 1,
            "only the new file is extracted — no full rebuild"
        );
    }

    #[tokio::test]
    async fn empty_library_returns_empty_arrays_with_scanned_at() {
        // Fresh data_dir: no index has ever been written.
        let data_dir = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        let roots = vec![media.path().to_path_buf()];
        let state = leaked_state();

        for (method, key) in [
            ("music.list-albums", "albums"),
            ("music.list-artists", "artists"),
            ("music.list-tracks", "tracks"),
        ] {
            let response = dispatch(state, data_dir.path(), &roots, method, None).await;
            let items = response[key]
                .as_array()
                .unwrap_or_else(|| panic!("{method}: {key} is an array, never null"));
            assert!(items.is_empty());
            assert_eq!(response["total"], 0);
            assert!(
                response["scanned_at"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty()),
                "{method}: scanned_at is populated even for an empty library"
            );
        }
    }
}
