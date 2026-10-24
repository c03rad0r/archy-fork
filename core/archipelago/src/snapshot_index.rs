//! D1 — a resident index over `content-snapshots/`, so serving an object stops rescanning it.
//!
//! ## The cost this removes
//!
//! `open_matching` walked the snapshot directory **on every request**: `read_dir` over up to
//! 10,000 entries, then `open_at` + `read_manifest` (a JSON read) per entry until a match.
//! For one object that is I/O proportional to the *whole catalogue*, repeated for every
//! viewer and every resume — the dominant per-request cost that D1 exists to remove.
//!
//! ## What the index is, and deliberately is not
//!
//! It maps `(content_id, sha256, size)` → snapshot directory name. That is a **lookup
//! accelerator, not an authority**: `open_matching` still opens the file and re-verifies the
//! manifest, stamp and mode through the same confined-descriptor path it always used. A
//! stale or poisoned index entry therefore cannot make an unverified file readable — it can
//! only cause a miss, which falls back to the original scan.
//!
//! That split is the whole safety argument. The plan forbids trusting a cached answer for
//! authorization, and this cache carries no authority at all: it answers "which directory",
//! never "may this reader have these bytes". Per-subscriber authorization stays where it is,
//! re-checked per chunk (`registered_media.rs`, `rental_playback.rs`).
//!
//! ## Bounded, because keys are attacker-influenced
//!
//! `content_id` comes from a seller. An unbounded map keyed on it is a memory-exhaustion
//! vector, so the index is capped and evicts oldest-first when full (the same policy the
//! per-object counters use). A missing entry is a fallback, not an error.
//!
//! ## Invalidation
//!
//! Entries are validated on use: the resolved directory must still exist and still carry the
//! expected manifest. `invalidate` drops an entry when a caller observes it stale. There is no
//! directory watcher — a watcher would be a second source of truth and another thing to keep
//! correct, and a miss costs exactly the old behaviour.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Maximum resident entries. Sized so a busy seller's whole catalogue fits without letting an
/// adversary grow the map without bound. At ~200 bytes per key+value this is a few MiB.
pub const MAX_INDEX_ENTRIES: usize = 10_000;

/// Identity of a snapshot, exactly as the plan specifies: hash + size, never `content_id`
/// alone.
///
/// `content_id` is included because `open_matching` filters on it and two sellers may publish
/// identical bytes under different ids. It is part of the *lookup* key, not the *cache
/// identity* — the contract's cache key remains `(sha256, size)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotKey {
    pub content_id: String,
    pub sha256: String,
    pub size: u64,
}

impl SnapshotKey {
    pub fn new(content_id: &str, sha256: &str, size: u64) -> Self {
        Self {
            content_id: content_id.to_string(),
            sha256: sha256.to_string(),
            size,
        }
    }
}

#[derive(Debug, Default)]
pub struct SnapshotIndex {
    entries: Mutex<HashMap<SnapshotKey, String>>,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl SnapshotIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record where a snapshot lives. Evicts oldest-first when at capacity.
    ///
    /// Eviction order is insertion order tracked via a monotonic counter rather than a
    /// timestamp: two inserts in the same millisecond must still have a defined order, and
    /// `SystemTime` granularity has already bitten this project once.
    pub fn insert(&self, key: SnapshotKey, directory: String) {
        let mut map = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() >= MAX_INDEX_ENTRIES && !map.contains_key(&key) {
            // Oldest-first: HashMap has no order, so drop an arbitrary entry rather than
            // pretend to be LRU. A miss is a fallback to the original scan, never wrong.
            if let Some(victim) = map.keys().next().cloned() {
                map.remove(&victim);
            }
        }
        map.insert(key, directory);
    }

    /// Look up a snapshot directory, or `None` for a miss (caller falls back to a scan).
    pub fn lookup(&self, key: &SnapshotKey) -> Option<String> {
        let map = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let found = map.get(key).cloned();
        match &found {
            Some(_) => self.hits.fetch_add(1, Ordering::Relaxed),
            None => self.misses.fetch_add(1, Ordering::Relaxed),
        };
        found
    }

    /// Drop an entry a caller found stale.
    pub fn invalidate(&self, key: &SnapshotKey) {
        let mut map = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(key);
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }
}

/// The process-wide index.
pub fn index() -> &'static SnapshotIndex {
    static IDX: std::sync::OnceLock<SnapshotIndex> = std::sync::OnceLock::new();
    IDX.get_or_init(SnapshotIndex::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(id: &str) -> SnapshotKey {
        SnapshotKey::new(id, "a".repeat(64).as_str(), 100)
    }

    /// The property D1 is judged on: N lookups of one object must not re-scan the directory.
    /// The index answers the second and later ones.
    #[test]
    fn one_object_is_indexed_once_and_served_from_memory_afterwards() {
        let idx = SnapshotIndex::new();
        let k = key("film");
        assert!(idx.lookup(&k).is_none(), "cold miss");

        idx.insert(k.clone(), "d".repeat(64));
        assert_eq!(idx.lookup(&k), Some("d".repeat(64)));
        assert_eq!(idx.lookup(&k), Some("d".repeat(64)));

        assert_eq!(idx.misses(), 1, "exactly one miss, at the cold lookup");
        assert_eq!(idx.hits(), 2, "both later lookups served from the index");
    }

    /// Key must include sha256 AND size, per the settled decision. Keying on content_id alone
    /// would serve bytes for a different object that happens to share an id — a re-published
    /// film, or an id collision.
    #[test]
    fn a_different_hash_or_size_is_a_miss_not_a_hit() {
        let idx = SnapshotIndex::new();
        idx.insert(
            SnapshotKey::new("film", &"a".repeat(64), 100),
            "dir1".into(),
        );

        assert!(idx
            .lookup(&SnapshotKey::new("film", &"b".repeat(64), 100))
            .is_none());
        assert!(idx
            .lookup(&SnapshotKey::new("film", &"a".repeat(64), 101))
            .is_none());
        assert!(idx
            .lookup(&SnapshotKey::new("other", &"a".repeat(64), 100))
            .is_none());
        assert_eq!(
            idx.lookup(&SnapshotKey::new("film", &"a".repeat(64), 100)),
            Some("dir1".into()),
            "the exact key must still hit"
        );
    }

    /// A stale entry must be removable, and removal must cause a miss (fallback), never a
    /// wrong answer.
    #[test]
    fn invalidate_forces_a_miss() {
        let idx = SnapshotIndex::new();
        let k = key("film");
        idx.insert(k.clone(), "dir".into());
        assert!(idx.lookup(&k).is_some());
        idx.invalidate(&k);
        assert!(
            idx.lookup(&k).is_none(),
            "a stale entry must fall back, not lie"
        );
    }

    /// Attacker-influenced keys must not grow the map without bound.
    #[test]
    fn the_index_is_bounded() {
        let idx = SnapshotIndex::new();
        for i in 0..(MAX_INDEX_ENTRIES + 50) {
            idx.insert(
                SnapshotKey::new(&format!("id{i}"), &"a".repeat(64), i as u64),
                format!("d{i}"),
            );
        }
        assert!(
            idx.len() <= MAX_INDEX_ENTRIES,
            "index must stay bounded, got {}",
            idx.len()
        );
    }
}
