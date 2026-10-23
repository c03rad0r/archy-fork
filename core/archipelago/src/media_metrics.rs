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
    /// D3: when set, bytes are also accumulated per (peer, service) for the byte cap.
    /// `None` for callers with no billable peer (local UI, unauthenticated reads) — those
    /// still count globally, they just have nobody to charge.
    usage: Option<(String, String)>,
    /// D3 refusal: when set, each poll checks what this peer has already had against the
    /// remaining allowance, and ENDS THE STREAM when it is exceeded. `None` = no cap (the
    /// pre-D3 behaviour, kept for callers with no entitlement to enforce).
    cap: Option<CapState>,
}

/// The remaining allowance a stream is checked against, refreshed as it changes.
///
/// `remaining` is an `Arc<AtomicU64>` rather than a copied `u64` because the allowance
/// shrinks as bytes are served and as the durable session is charged by the flush task. A
/// copy taken at construction would let a viewer keep streaming against a stale allowance.
pub struct CapState {
    pub remaining: std::sync::Arc<AtomicU64>,
    /// Set once the cap is breached, so the reason is logged once and not per chunk.
    pub breached: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl<S, E, B> Stream for Counted<S>
where
    S: Stream<Item = Result<B, E>>,
    B: AsRef<[u8]>,
{
    type Item = Result<B, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        // D3 refusal: if this peer has already had more than its allowance, end the stream
        // CLEANLY. `Poll::Ready(None)` is end-of-body, which a player treats as a completed
        // transfer; returning an error here would surface as a network failure to the
        // viewer, which the plan explicitly forbids ("end the body with Ok(None), not an
        // error"). Checked before polling so no byte past the allowance is ever handed out.
        if let (Some((peer, service)), Some(cap)) = (&this.usage, &this.cap) {
            let remaining = cap.remaining.load(Ordering::Relaxed);
            if media_usage()
                .check_within_allowance(peer, service, remaining)
                .is_err()
            {
                if !cap.breached.swap(true, Ordering::Relaxed) {
                    tracing::info!(
                        peer = %peer,
                        service = %service,
                        remaining,
                        "media byte cap reached; ending the stream cleanly"
                    );
                }
                return Poll::Ready(None);
            }
        }
        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                let n = chunk.as_ref().len() as u64;
                MEDIA_BYTES_SERVED.fetch_add(n, Ordering::Relaxed);
                MEDIA_CHUNKS_SERVED.fetch_add(1, Ordering::Relaxed);
                // D3: accumulate for the byte cap. Deliberately NOT a durable write here —
                // `meter::record_and_check` rewrites the whole session store, so calling it
                // per chunk would mean ~16,000 store writes per GB per viewer. The flush
                // happens at end-of-stream or on a coarse window.
                if let Some((peer, service)) = &this.usage {
                    media_usage().add(peer, service, n);
                }
                Poll::Ready(Some(Ok(chunk)))
            }
            // Errors and end-of-stream are passed through untouched: this wrapper must not
            // change the body's behaviour, only observe it.
            other => other,
        }
    }
}

/// The process-wide media usage accumulator (D3).
///
/// A global rather than a field threaded through every handler: the five media sites are
/// reached from different modules with different state, and the accumulator is keyed by
/// (peer, service) anyway — so one instance is both simpler and more correct than five that
/// could drift.
pub fn media_usage() -> &'static crate::media_meter::MediaUsageAccumulator {
    static ACC: std::sync::OnceLock<crate::media_meter::MediaUsageAccumulator> =
        std::sync::OnceLock::new();
    ACC.get_or_init(crate::media_meter::MediaUsageAccumulator::new)
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
        usage: None,
        cap: None,
    }
}

/// As `counted`, but also accumulates bytes against (peer_id, service_id) for D3's byte cap.
///
/// `allow(dead_code)` for the same reason as the rest of D3's accounting: `counted_for` has
/// no non-test caller yet — the three media sites (`rental_playback.rs:406`,
/// `registered_media.rs:337`, `cloud_purchase.rs:140`) still call `counted()`, because
/// switching them needs a per-request peer identity and they currently carry only a session
/// cookie. The clippy ratchet runs without `--all-targets`, so the test consumer does not
/// count. See the D3 section of PROGRESS.md.
#[allow(dead_code)]
pub fn counted_for<S, E, B>(stream: S, peer_id: &str, service_id: &str) -> Counted<S>
where
    S: Stream<Item = Result<B, E>>,
    B: AsRef<[u8]>,
{
    Counted {
        inner: Box::pin(stream),
        _guard: ActiveGuard::new(),
        usage: Some((peer_id.to_string(), service_id.to_string())),
        cap: None,
    }
}

/// As `counted_for`, but ALSO ends the stream when the peer's allowance is spent.
///
/// This is D3's refusal half. `remaining` is an `Arc<AtomicU64>` so the allowance is re-read
/// on every poll: the durable session shrinks as the flush task charges it, and a copy
/// captured at construction would let a viewer keep streaming against a stale allowance.
///
/// The number comes from the caller (which knows the session) rather than being read here,
/// because reading it means a store hit on the request path — and the plan is explicit that
/// this must not become per-chunk metering.
pub fn counted_within_allowance<S, E, B>(
    stream: S,
    peer_id: &str,
    service_id: &str,
    remaining: std::sync::Arc<AtomicU64>,
) -> Counted<S>
where
    S: Stream<Item = Result<B, E>>,
    B: AsRef<[u8]>,
{
    Counted {
        inner: Box::pin(stream),
        _guard: ActiveGuard::new(),
        usage: Some((peer_id.to_string(), service_id.to_string())),
        cap: Some(CapState {
            remaining,
            breached: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }),
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

    /// D3's refusal half, and the observation that makes it real: a stream whose allowance
    /// runs out must END — not error, not hang, not keep serving. Before this existed, the
    /// counter moved and nothing ever stopped.
    #[tokio::test]
    async fn stream_ends_cleanly_when_the_allowance_is_spent() {
        use futures_util::stream;
        use futures_util::StreamExt;

        // 10 chunks of 100 bytes = 1000 bytes available; allow only 250.
        let chunks: Vec<Result<Vec<u8>, std::io::Error>> =
            (0..10).map(|_| Ok(vec![0u8; 100])).collect();
        let remaining = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(250));
        let mut body = counted_within_allowance(
            stream::iter(chunks),
            "cap-peer",
            "content-download",
            remaining.clone(),
        );

        let mut served = 0usize;
        let mut items = 0usize;
        while let Some(item) = body.next().await {
            match item {
                Ok(chunk) => served += chunk.len(),
                Err(_) => panic!("the cap must end the body, never error it"),
            }
            items += 1;
        }

        // The loop above only exits when `next()` returns None — i.e. the body ENDED. That
        // is the whole point: not an error, not a hang, a normal end of stream.
        assert!(
            served < 1000,
            "the stream must have been cut short, served={served} of 1000"
        );
        assert!(
            served <= 250 + 100,
            "at most one chunk past the allowance may be handed out, served={served}"
        );
        assert!(
            items <= 4,
            "the body must stop promptly, not run to the end: {items} items"
        );

        // CONTROL: with a generous allowance the whole body is delivered.
        let chunks2: Vec<Result<Vec<u8>, std::io::Error>> =
            (0..10).map(|_| Ok(vec![0u8; 100])).collect();
        let mut control = counted_within_allowance(
            stream::iter(chunks2),
            "cap-peer-control",
            "content-download",
            std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1_000_000)),
        );
        let mut control_served = 0usize;
        while let Some(Ok(chunk)) = control.next().await {
            control_served += chunk.len();
        }
        assert_eq!(
            control_served, 1000,
            "a generous allowance must deliver the whole body"
        );

        media_usage().take("cap-peer", "content-download");
        media_usage().take("cap-peer-control", "content-download");
    }

    /// D3: the accumulator must be driven by the BODY, not by a caller who remembers to add.
    /// A body that yields 3 chunks must leave exactly those bytes pending against the right
    /// (peer, service) pair — this is what makes the cap enforceable on the media path
    /// rather than only in a unit test.
    #[tokio::test]
    async fn counted_for_accumulates_body_bytes_against_the_peer() {
        use futures_util::stream;
        use futures_util::StreamExt;

        let chunks: Vec<Result<Vec<u8>, std::io::Error>> =
            vec![Ok(vec![0u8; 100]), Ok(vec![0u8; 250]), Ok(vec![0u8; 7])];
        let before = media_usage().pending("d3-peer", "content-download");
        // `Counted` is a Stream, not an HttpBody — drive it with StreamExt::next, not
        // HttpBody::data. (`counted` yields the chunk type unchanged, so on the real media
        // paths this same stream is what `Body::wrap_stream` consumes.)
        let mut body = counted_for(stream::iter(chunks), "d3-peer", "content-download");

        let mut total = 0usize;
        while let Some(chunk) = body.next().await {
            total += chunk.unwrap().len();
        }
        assert_eq!(total, 357);

        let after = media_usage().pending("d3-peer", "content-download");
        assert_eq!(
            after - before,
            357,
            "the body must have accumulated exactly the bytes it yielded"
        );

        // CONTROL: a plain `counted` body (no billable peer) must NOT move the accumulator.
        let control_before = media_usage().pending("nobody", "content-download");
        let control_chunks: Vec<Result<Vec<u8>, std::io::Error>> = vec![Ok(vec![0u8; 999])];
        let mut control = counted(stream::iter(control_chunks));
        while let Some(chunk) = control.next().await {
            chunk.unwrap();
        }
        assert_eq!(
            media_usage().pending("nobody", "content-download"),
            control_before,
            "counted() has no billable peer and must not charge anyone"
        );

        // Clean up so this test cannot race others asserting on the same global.
        media_usage().take("d3-peer", "content-download");
    }

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
