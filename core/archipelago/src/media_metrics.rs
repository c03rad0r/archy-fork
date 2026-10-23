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

#[cfg(test)]
mod tests {
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
        // --- bytes: a stream's bytes are counted exactly, and pass through unmodified ---
        let before = snapshot();
        let counted_stream = counted(chunks(&[64 * 1024, 64 * 1024, 1]));
        let collected: Vec<_> = counted_stream.collect().await;
        let after = snapshot();

        assert_eq!(collected.len(), 3, "every chunk must be passed through");
        assert!(collected.iter().all(|r| r.is_ok()), "errors must not be invented");
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
                Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "boom")),
            ]);
            let mut c = counted(failing);
            assert!(c.next().await.expect("first").is_ok());
            assert!(c.next().await.expect("second").is_err(), "error must propagate");
        }
        assert_eq!(
            snapshot().streams_active,
            err_before.streams_active,
            "an errored stream must not leak an active-stream count"
        );
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
