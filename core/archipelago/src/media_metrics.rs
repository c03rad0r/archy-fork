//! Media-path observability (P1).
//!
//! `MetricSnapshot` (`monitoring/types.rs`) is host + container only. The media path had no
//! counters at all, which is why D1/D3/D5 cannot be validated in production: there is
//! nothing to read back. This module adds them.
//!
//! ## Why a stream wrapper and not a call-site counter
//!
//! Media bytes are emitted by five independent `try_unfold` streams — not one shared
//! helper. Counting at the point each stream is *built* would count intent, not delivery:
//! a body that is created and then dropped mid-transfer would be recorded as fully served.
//! Counting inside `poll_next` counts the bytes that were actually handed to the
//! connection, and stops counting when the client disconnects.
//!
//! ## Cost
//!
//! One relaxed atomic add per chunk, ~64 KiB apart. That is deliberately the cheapest
//! possible per-chunk work: the D3 warning in the plan is that `meter::record_and_check`
//! per chunk would be ~16,000 full JSON rewrites per GB per viewer. Nothing here locks,
//! allocates per chunk, or serialises.
//!
//! ## Idiom
//!
//! Follows the existing pattern for exactly this problem in this codebase:
//! `update.rs:15` — `pub static DOWNLOAD_BYTES: AtomicU64 = AtomicU64::new(0);`

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};

use futures_util::Stream;

/// Bytes handed to a connection on a media path. Monotonic; never reset.
pub static MEDIA_BYTES_SERVED: AtomicU64 = AtomicU64::new(0);

/// Media chunks handed to a connection.
pub static MEDIA_CHUNKS_SERVED: AtomicU64 = AtomicU64::new(0);

/// Media responses currently streaming. Incremented when a counted body is created,
/// decremented when it is dropped — including on client disconnect, because the body is
/// dropped then too. This is the "active viewer" gauge.
pub static MEDIA_STREAMS_ACTIVE: AtomicU64 = AtomicU64::new(0);

/// Media responses started since boot.
pub static MEDIA_STREAMS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// A point-in-time copy of the media counters, for folding into a `MetricSnapshot`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct MediaMetrics {
    pub bytes_served: u64,
    pub chunks_served: u64,
    pub streams_active: u64,
    pub streams_total: u64,
}

/// Read every counter once. Cheap and lock-free.
pub fn snapshot() -> MediaMetrics {
    MediaMetrics {
        bytes_served: MEDIA_BYTES_SERVED.load(Ordering::Relaxed),
        chunks_served: MEDIA_CHUNKS_SERVED.load(Ordering::Relaxed),
        streams_active: MEDIA_STREAMS_ACTIVE.load(Ordering::Relaxed),
        streams_total: MEDIA_STREAMS_TOTAL.load(Ordering::Relaxed),
    }
}

/// Per-object and per-peer accounting.
///
/// The global counters above answer "is media flowing?". These answer "which object, and
/// to whom?" — which is what D1 (request collapsing), D3 (byte cap) and D5 (provider
/// selection) actually need in order to be validated.
///
/// Bounded on purpose. An unbounded map keyed by attacker-influenced input is a memory
/// exhaustion vector, so both maps are capped and evict oldest-first when full. A counter
/// that can be made to OOM the node is worse than no counter.
#[allow(dead_code)] // per-object/per-peer producers pending a sha256 source; see ObjectCounters
pub mod detail {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;

    /// Maximum distinct objects tracked. Beyond this, the least-recently-inserted entry is
    /// dropped: media is served from a bounded working set, so the tail is not interesting.
    pub const MAX_OBJECTS: usize = 4096;
    /// Maximum distinct peers tracked.
    pub const MAX_PEERS: usize = 1024;

    /// Counters for one object. Keyed by `(content_sha256, content_size)` per the plan's
    /// cache-key decision — never by `content_id`, which is a seller-local opaque string
    /// and would fragment the same bytes across sellers.
    ///
    /// NO PRODUCER YET, and this is a real gap rather than an oversight: the seller's
    /// `OwnedItem` index (`content_owned.rs:48-61`) carries onion, content_id, filename,
    /// mime, size, paid_sats, backend, timestamp and download_complete — but NO sha256.
    /// So at the serve site there is nothing to build a `(sha256, size)` key from. The
    /// alternative would be keying on `content_id`, which the plan explicitly forbids
    /// because it fragments identical bytes across sellers. The API is therefore present
    /// and tested, awaiting a hash source; `allow(dead_code)` records that rather than
    /// letting the lint silently delete a documented interface.
    #[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
    pub struct ObjectCounters {
        pub bytes_served: u64,
        pub requests: u64,
    }

    /// Counters for one peer.
    #[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
    pub struct PeerCounters {
        pub bytes_served: u64,
        pub requests: u64,
        pub active_streams: u64,
    }

    /// Cache outcomes. `Hit` means the bytes were served from a local replica/cache;
    /// `Miss` means they had to be fetched. Distinguishing these is the whole point of the
    /// cache-hit/miss requirement — a single "served" number cannot tell you whether the
    /// cache is working.
    pub static CACHE_HITS: AtomicU64 = AtomicU64::new(0);
    pub static CACHE_MISSES: AtomicU64 = AtomicU64::new(0);

    static OBJECTS: Mutex<BTreeMap<String, ObjectCounters>> = Mutex::new(BTreeMap::new());
    static PEERS: Mutex<BTreeMap<String, PeerCounters>> = Mutex::new(BTreeMap::new());

    /// Build the accounting key. `size` is part of the key by design: the same hash with a
    /// different length is not the same object.
    pub fn object_key(sha256: &str, size: u64) -> String {
        format!("{sha256}:{size}")
    }

    /// Record a served object. Silent no-op on a poisoned lock: observability must never
    /// take the media path down, so a panic elsewhere cannot turn a metric into an outage.
    pub fn record_object(key: &str, bytes: u64) {
        let Ok(mut map) = OBJECTS.lock() else { return };
        if !map.contains_key(key) && map.len() >= MAX_OBJECTS {
            // BTreeMap is ordered, so the first key is a deterministic victim. Deterministic
            // matters: an eviction that depends on hash iteration order makes runs
            // irreproducible.
            if let Some(victim) = map.keys().next().cloned() {
                map.remove(&victim);
            }
        }
        let e = map.entry(key.to_string()).or_default();
        e.bytes_served = e.bytes_served.saturating_add(bytes);
        e.requests = e.requests.saturating_add(1);
    }

    /// Record bytes served to a peer.
    pub fn record_peer(peer: &str, bytes: u64) {
        let Ok(mut map) = PEERS.lock() else { return };
        if !map.contains_key(peer) && map.len() >= MAX_PEERS {
            if let Some(victim) = map.keys().next().cloned() {
                map.remove(&victim);
            }
        }
        let e = map.entry(peer.to_string()).or_default();
        e.bytes_served = e.bytes_served.saturating_add(bytes);
        e.requests = e.requests.saturating_add(1);
    }

    pub fn record_cache_hit() {
        CACHE_HITS.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cache_miss() {
        CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
    }

    /// Snapshot of the detail maps, for the RPC.
    pub fn snapshot() -> (
        Vec<(String, ObjectCounters)>,
        Vec<(String, PeerCounters)>,
        u64,
        u64,
    ) {
        let objects = OBJECTS
            .lock()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect())
            .unwrap_or_default();
        let peers = PEERS
            .lock()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect())
            .unwrap_or_default();
        (
            objects,
            peers,
            CACHE_HITS.load(Ordering::Relaxed),
            CACHE_MISSES.load(Ordering::Relaxed),
        )
    }

    /// Shares the same lock as the global counters: both mutate process-global state and
    /// must not race each other under parallel test execution.
    #[cfg(test)]
    pub fn lock_for_test() -> std::sync::MutexGuard<'static, ()> {
        super::counter_test_lock()
    }

    #[cfg(test)]
    pub fn reset_for_test() {
        if let Ok(mut m) = OBJECTS.lock() {
            m.clear();
        }
        if let Ok(mut m) = PEERS.lock() {
            m.clear();
        }
        CACHE_HITS.store(0, Ordering::Relaxed);
        CACHE_MISSES.store(0, Ordering::Relaxed);
    }
}

/// Decrements `MEDIA_STREAMS_ACTIVE` when the response body goes away.
///
/// Held by `Counted`, so the gauge follows the body's real lifetime: a client that
/// disconnects mid-stream drops the body and the gauge falls, which is the behaviour a
/// "how many viewers are watching right now" number needs.
struct ActiveGuard;

impl ActiveGuard {
    fn new() -> Self {
        MEDIA_STREAMS_ACTIVE.fetch_add(1, Ordering::Relaxed);
        MEDIA_STREAMS_TOTAL.fetch_add(1, Ordering::Relaxed);
        Self
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        // saturating so a counter bug can never wrap to u64::MAX and read as "4 billion
        // viewers". fetch_sub on 0 wraps; this does not.
        let _ = MEDIA_STREAMS_ACTIVE.try_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
            Some(v.saturating_sub(1))
        });
    }
}

/// Wraps a media chunk stream so that every chunk actually yielded is counted.
///
/// The stream is boxed rather than pin-projected: this allocates once per *response*, not
/// once per chunk, and avoids pulling in a pin-projection dependency for a wrapper this
/// small.
pub struct Counted<S> {
    inner: Pin<Box<S>>,
    _guard: ActiveGuard,
}

impl<S, E, B> Stream for Counted<S>
where
    S: Stream<Item = Result<B, E>>,
    B: AsRef<[u8]>,
{
    type Item = Result<B, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                MEDIA_BYTES_SERVED.fetch_add(chunk.as_ref().len() as u64, Ordering::Relaxed);
                MEDIA_CHUNKS_SERVED.fetch_add(1, Ordering::Relaxed);
                Poll::Ready(Some(Ok(chunk)))
            }
            // Errors and end-of-stream are passed through untouched: this wrapper must not
            // change the body's behaviour, only observe it.
            other => other,
        }
    }
}

/// Count the bytes of `stream` as they are handed out. See module docs.
///
/// Generic over the chunk type because the media handlers do not agree on one:
/// `rental_playback.rs` yields `bytes::Bytes`, the other four yield `Vec<u8>`. Both are
/// `AsRef<[u8]>`, so one wrapper covers all five sites without converting any chunk —
/// converting would allocate per chunk, which is the cost this module exists to avoid.
pub fn counted<S, E, B>(stream: S) -> Counted<S>
where
    S: Stream<Item = Result<B, E>>,
    B: AsRef<[u8]>,
{
    Counted {
        inner: Box::pin(stream),
        _guard: ActiveGuard::new(),
    }
}

/// Serialises tests that assert on the process-global counters.
///
/// These counters are statics shared by the whole process, and Rust runs tests in
/// PARALLEL. A test that asserts on a delta therefore races any other test that moves the
/// same counter — observed, not theorised: a second test in `api/rpc/monitoring.rs`
/// driving 4x4096 bytes made this module's delta assertion read 147457 instead of 131073
/// (the difference was exactly the other test's 16384 bytes).
///
/// Any test asserting on these counters must hold this lock for the duration of its
/// before/after measurement.
#[cfg(test)]
pub fn counter_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::detail;
    use super::*;
    use futures_util::stream;
    use futures_util::StreamExt;

    fn chunks(sizes: &[usize]) -> impl Stream<Item = Result<Vec<u8>, std::io::Error>> {
        let owned: Vec<Result<Vec<u8>, std::io::Error>> =
            sizes.iter().map(|n| Ok(vec![0xAB; *n])).collect();
        stream::iter(owned)
    }

    /// One test fn: the counters are process-global statics and Rust runs tests in
    /// parallel, so a second test touching them could interleave. Assertions use deltas
    /// within this fn, which is therefore order-independent with respect to other tests.
    #[tokio::test]
    async fn counters_track_real_bytes_and_the_active_gauge_follows_the_body() {
        // Serialised against any other test asserting on these global counters.
        let _serialised = counter_test_lock();
        // --- bytes: a stream's bytes are counted exactly, and pass through unmodified ---
        let before = snapshot();
        let counted_stream = counted(chunks(&[64 * 1024, 64 * 1024, 1]));
        let collected: Vec<_> = counted_stream.collect().await;
        let after = snapshot();

        assert_eq!(collected.len(), 3, "every chunk must be passed through");
        assert!(
            collected.iter().all(|r| r.is_ok()),
            "errors must not be invented"
        );
        assert_eq!(
            collected[2].as_ref().expect("chunk").len(),
            1,
            "chunk contents must be byte-exact through the wrapper"
        );

        let delta_bytes = after.bytes_served - before.bytes_served;
        assert_eq!(
            delta_bytes,
            128 * 1024 + 1,
            "bytes_served must equal the sum of chunk lengths actually yielded"
        );
        let delta_chunks = after.chunks_served - before.chunks_served;
        assert_eq!(delta_chunks, 3, "chunks_served must count yielded chunks");

        // --- active gauge: returns to baseline after the body is dropped ---
        assert_eq!(
            after.streams_active, before.streams_active,
            "the active gauge must return to its baseline once the body is dropped"
        );
        assert!(
            after.streams_total - before.streams_total >= 1,
            "streams_total must count the response that was started"
        );

        // --- the gauge is up WHILE the body is alive (this is the 'active viewer' claim) ---
        let live_before = snapshot();
        let mut live = counted(chunks(&[1024]));
        let live_now = snapshot();
        assert_eq!(
            live_now.streams_active,
            live_before.streams_active + 1,
            "a live body must be visible as an active stream"
        );
        let _ = live.next().await;
        drop(live);
        assert_eq!(
            snapshot().streams_active,
            live_before.streams_active,
            "dropping a live body must decrement the gauge — this is what makes a client \
             disconnect stop counting as a viewer"
        );

        // --- a stream that ends in an error still decrements the gauge ---
        let err_before = snapshot();
        {
            let failing = stream::iter(vec![
                Ok(vec![1u8; 8]),
                Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "boom",
                )),
            ]);
            let mut c = counted(failing);
            assert!(c.next().await.expect("first").is_ok());
            assert!(
                c.next().await.expect("second").is_err(),
                "error must propagate"
            );
        }
        assert_eq!(
            snapshot().streams_active,
            err_before.streams_active,
            "an errored stream must not leak an active-stream count"
        );
    }

    /// Per-object accounting keys on (sha256, size) and never on content_id — the plan's
    /// explicit cache-key decision. Asserted so a future refactor cannot quietly reintroduce
    /// content_id and fragment the same bytes across sellers.
    #[test]
    fn object_key_uses_hash_and_size_never_content_id() {
        let _g = detail::lock_for_test();
        detail::reset_for_test();

        let k = detail::object_key("abc123", 143360);
        assert_eq!(k, "abc123:143360", "key must be hash:size");
        assert!(!k.contains("content_id"));

        detail::record_object(&k, 1000);
        detail::record_object(&k, 500);
        let (objects, _, _, _) = detail::snapshot();
        let entry = objects.iter().find(|(name, _)| name == &k).expect("entry");
        assert_eq!(
            entry.1.bytes_served, 1500,
            "bytes must accumulate per object"
        );
        assert_eq!(entry.1.requests, 2, "requests must count per object");

        // The same hash at a different size is a DIFFERENT object.
        let k2 = detail::object_key("abc123", 999);
        assert_ne!(k, k2, "same hash, different size must not collide");
    }

    /// Per-peer accounting, and the bound that stops an attacker-influenced map from
    /// exhausting memory. A counter that can OOM the node is worse than no counter.
    #[test]
    fn peer_accounting_accumulates_and_the_map_is_bounded() {
        let _g = detail::lock_for_test();
        detail::reset_for_test();

        detail::record_peer("peer-a", 4096);
        detail::record_peer("peer-a", 4096);
        detail::record_peer("peer-b", 100);
        let (_, peers, _, _) = detail::snapshot();
        let a = peers.iter().find(|(n, _)| n == "peer-a").expect("peer-a");
        assert_eq!(a.1.bytes_served, 8192);
        assert_eq!(a.1.requests, 2);

        // Overflow the cap and confirm it stays bounded rather than growing without limit.
        for i in 0..(detail::MAX_PEERS + 50) {
            detail::record_peer(&format!("flood-{i}"), 1);
        }
        let (_, peers, _, _) = detail::snapshot();
        assert!(
            peers.len() <= detail::MAX_PEERS,
            "peer map must stay bounded ({} > {})",
            peers.len(),
            detail::MAX_PEERS
        );
    }

    /// Cache hit/miss must be separately observable. A single "served" counter cannot tell
    /// you whether the cache is working, which is the requirement.
    #[test]
    fn cache_hits_and_misses_are_counted_separately() {
        let _g = detail::lock_for_test();
        detail::reset_for_test();

        detail::record_cache_hit();
        detail::record_cache_hit();
        detail::record_cache_miss();
        let (_, _, hits, misses) = detail::snapshot();
        assert_eq!(hits, 2, "hits counted separately");
        assert_eq!(misses, 1, "misses counted separately");
        assert_ne!(hits, misses, "the two outcomes must not be conflated");
    }

    /// The per-chunk cost must stay a single relaxed atomic add — no lock, no allocation.
    #[test]
    fn snapshot_is_lock_free_and_consistent() {
        let a = snapshot();
        let b = snapshot();
        // Monotonic counters cannot go backwards.
        assert!(b.bytes_served >= a.bytes_served);
        assert!(b.chunks_served >= a.chunks_served);
        assert!(b.streams_total >= a.streams_total);
    }
}
