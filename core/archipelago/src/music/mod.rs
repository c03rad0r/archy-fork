//! Music domain root — entity types decided in `13-MUSIC-MODEL.md` (D-13,
//! one-way). See that document for the full rationale and the rejected
//! alternatives; this module implements the decision, it does not re-derive
//! it.
//!
//! Track identity is hybrid: `(source, canonical path)` is the row key
//! (cheap, stat-only, incremental via mtime), with a lazily-backfilled
//! content-hash dedupe column (`Track::content_hash`) for the move/dedupe
//! case that identity alone can't handle. Albums and artists are derived at
//! read time by grouping tracks on their tags, not stored as first-class
//! rows. The on-disk index is a single JSON file at
//! `data_dir/music/index.json`, matching `content_server.rs::load_catalog`'s
//! precedent.

pub mod index;
pub mod tags;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The filesystem roots the music indexer is confined to — one per source
/// `13-MUSIC-MODEL.md` decided to index ("Sources indexed: both"):
///
/// - `data_dir/filebrowser/Music` — the node's own FileBrowser `Music`
///   folder (`MusicSource::OwnLibrary`). Peer purchases of audio are also
///   auto-filed here by `content.*`'s paid-download path, so they enter the
///   library through this root with their real filenames.
/// - `data_dir/purchased-content` — the local byte cache of peer-purchased
///   content (`MusicSource::Peer { onion }`), laid out as
///   `<onion>/<content_id>`.
///
/// Confinement is a parameter everywhere downstream (`tags::extract_tags`
/// and `index::reindex` both take these roots and refuse paths outside
/// them) — an indexer that can be aimed at `data_dir/secrets` is a
/// secret-exfiltration primitive (T-13-39), so the roots are computed in
/// exactly one place and passed through.
pub fn media_roots(config: &crate::config::Config) -> Vec<PathBuf> {
    vec![
        config.data_dir.join("filebrowser").join("Music"),
        config.data_dir.join("purchased-content"),
    ]
}

/// Schema version of the on-disk music index (`data_dir/music/index.json`).
/// Bump when `Track`'s shape changes in a way that needs a reindex. See
/// `13-MUSIC-MODEL.md`'s "Schema version and the reindex path" section for
/// the newer-version-on-older-binary handling contract: an older binary
/// encountering a newer-versioned index treats it as absent rather than
/// reinterpreting or overwriting it.
pub const MUSIC_SCHEMA_VERSION: u32 = 1;

/// Where a track was discovered. Both sources are indexed
/// (`13-MUSIC-MODEL.md`'s "Sources indexed: both").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MusicSource {
    /// The node's own FileBrowser `Music` folder.
    OwnLibrary,
    /// A peer's shared audio, reachable via the content/peer-proxy
    /// subsystem.
    Peer { onion: String },
}

/// Stable identity for a track (hybrid-identity, `13-MUSIC-MODEL.md`):
/// `(source, canonical path)` is the row key — cheap, stat-only, survives a
/// rescan via mtime. A file move or rename orphans this identity;
/// `Track::content_hash` is the lazily-backfilled dedupe column that exists
/// for exactly that case, and for cross-peer dedupe once it's populated.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId {
    pub source: MusicSource,
    pub path: PathBuf,
}

/// Derived album grouping key (derived-albums, `13-MUSIC-MODEL.md`). Albums
/// are not stored rows — this is the key produced by grouping `Track`s at
/// read time on `(album_artist, album)`, not a persisted identity. A retag
/// simply changes what the grouping produces on the next read.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlbumId {
    pub album_artist: Option<String>,
    pub album: String,
}

/// Derived artist grouping key. Same read-time-only status as `AlbumId`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtistId(pub String);

/// A single indexed track — a row in `data_dir/music/index.json`. This is
/// the migration surface `13-MUSIC-MODEL.md`'s one-way decision is about:
/// changing this shape after nodes have indexed libraries needs a reindex
/// path (`MUSIC_SCHEMA_VERSION`), not just a code change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<u32>,
    pub duration_secs: u64,
    /// `false` when `title` was derived from the filename stem because the
    /// file carried no readable tags — the track still appears in the
    /// library rather than being dropped (see `tags::extract_tags`).
    pub has_tags: bool,
    /// Lazily-backfilled dedupe column (hybrid-identity,
    /// `13-MUSIC-MODEL.md`). `None` until a background backfill pass
    /// computes it; absence is not an error state.
    #[serde(default)]
    pub content_hash: Option<String>,
}

/// An album, computed at read time by grouping `Track`s on
/// `(album_artist, album)` — never persisted directly (derived-albums,
/// `13-MUSIC-MODEL.md`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Album {
    pub id: AlbumId,
    pub track_ids: Vec<TrackId>,
}

/// An artist, computed at read time by grouping `Track`s on `artist`. Same
/// read-time-only status as `Album`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artist {
    pub id: ArtistId,
    pub track_ids: Vec<TrackId>,
}

/// A complete, consistent read of the library at one point in time: the
/// persisted track rows plus the albums/artists derived from them
/// (derived-albums, `13-MUSIC-MODEL.md`). Produced by
/// `index::MusicIndex::snapshot`; because the on-disk index is written
/// atomically (`index::save_atomic`), a snapshot is always taken from a
/// complete index — never a partially-written one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibrarySnapshot {
    pub schema_version: u32,
    /// RFC3339 timestamp of the scan this snapshot was read from. Populated
    /// even for an empty, never-scanned library (with the time the empty
    /// snapshot was produced) — an empty library is empty arrays plus a
    /// timestamp, never a null and never an error.
    pub scanned_at: String,
    pub tracks: Vec<Track>,
    pub albums: Vec<Album>,
    pub artists: Vec<Artist>,
}
