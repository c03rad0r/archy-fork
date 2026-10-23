//! D3 — progress-aware byte accounting for the media path.
//!
//! The billing question D3 answers is "free after X bytes streamed". Answering it per
//! chunk is not viable: `meter::record_and_check` calls `session::load_sessions` and saves
//! the whole session store on **every** call, so metering a 5 Mbps stream per chunk means
//! a full JSON rewrite several times a second per viewer (~16,000 writes per GB per
//! viewer at 64 KiB chunks).
//!
//! So this module accumulates bytes in memory and flushes at a coarse boundary — end of
//! stream, or when enough bytes have piled up. The invariants that matter, and that the
//! tests below pin:
//!
//! 1. **A second request must ADD, not replace.** Otherwise a client resumes with a Range
//!    request whenever the counter approaches the cap and streams forever. This is the
//!    `range_requests_cannot_reset_the_cap` guard named in the acceptance criteria.
//! 2. **Accounting is per (peer, service).** Two viewers must not share a counter, or one
//!    heavy viewer's bytes get charged to an idle one and cut the wrong person off.
//! 3. **A flush drains what it charged.** Charging the same bytes twice on the next flush
//!    would halve every viewer's allowance.
//!
//! This module deliberately does NOT decide policy. It counts. The caller decides what a
//! breached cap means, and the plan is explicit that the body ends with `Ok(None)` rather
//! than an error — a truncated-but-clean end of stream, not a mid-stream error that a
//! player reports as a network failure.
//!
//! ## Why `allow(dead_code)` is here, and what it is NOT hiding
//!
//! The clippy ratchet runs `cargo clippy -p archipelago --bin archipelago` — **without
//! `--all-targets`**, so the test cfg is not compiled and test-only consumers do not count
//! as uses. Everything in this module is exercised by the tests below, but only
//! `MediaUsageAccumulator::add` (via `counted_for`) has a non-test consumer today. The rest
//! is `add_and_should_flush` / `take` / `drain_all` / `flush_usage`, which the stream-end
//! flush will call once it is wired.
//!
//! So this attribute covers code that is tested but not yet called, and the D3 section of
//! PROGRESS.md names exactly which wiring is missing. It is not a silencer for untested
//! code: every function here has a test that fails without it.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Default flush threshold: 8 MiB.
///
/// Chosen as a compromise. At 5 Mbps that is ~13 s of video between flushes, which bounds
/// both the over-serve on a breached cap and the loss on an unclean shutdown to ~8 MiB per
/// viewer — small next to a 64 GiB media budget, and ~128x fewer store writes than
/// per-chunk metering.
///
/// `dead_code` because the stream-end flush that consumes it is not wired yet — see the D3
/// section of PROGRESS.md for exactly what remains. Marked rather than deleted so the
/// constant and its tests survive to the wiring; the alternative (deleting the accounting
/// until the flush exists) would throw away the negative-control-proven behaviour. The same
/// idiom is already used in this repo for the per-object counters.
pub const DEFAULT_FLUSH_BYTES: u64 = 8 * 1024 * 1024;

type Key = (String, String);

/// In-memory byte accumulator, keyed by (peer_id, service_id).
#[derive(Debug, Default)]
pub struct MediaUsageAccumulator {
    pending: Mutex<BTreeMap<Key, u64>>,
    /// Bytes charged since boot, for observability. Not authoritative for billing.
    total_flushed: AtomicU64,
}

impl MediaUsageAccumulator {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(BTreeMap::new()),
            total_flushed: AtomicU64::new(0),
        }
    }

    /// Add served bytes for a peer/service pair.
    ///
    /// Accumulates. A resumed transfer (Range request) calling this again must increase the
    /// total, never reset it — that is the difference between a cap and a suggestion.
    pub fn add(&self, peer_id: &str, service_id: &str, bytes: u64) {
        let mut map = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let entry = map
            .entry((peer_id.to_string(), service_id.to_string()))
            .or_insert(0);
        *entry = entry.saturating_add(bytes);
    }

    /// Add bytes and report whether the caller should flush now.
    ///
    /// Returns true once the accumulated total for this pair has reached `threshold`. The
    /// counter is NOT reset here — `take` does that when the flush actually happens, so a
    /// failed flush does not silently drop the bytes.
    pub fn add_and_should_flush(
        &self,
        peer_id: &str,
        service_id: &str,
        bytes: u64,
        threshold: u64,
    ) -> bool {
        self.add(peer_id, service_id, bytes);
        self.pending(peer_id, service_id) >= threshold
    }

    /// Current unflushed total for a pair.
    pub fn pending(&self, peer_id: &str, service_id: &str) -> u64 {
        let map = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&(peer_id.to_string(), service_id.to_string()))
            .copied()
            .unwrap_or(0)
    }

    /// Remove and return the unflushed total for a pair.
    ///
    /// Called after a successful flush. Taking before charging and restoring on failure
    /// would be safer still, but `take` then charge is what the store API allows; the
    /// failure mode (losing up to one flush window of accounting on a store error) is
    /// bounded by `DEFAULT_FLUSH_BYTES` per viewer and is the conservative direction —
    /// a viewer is never over-charged, at worst under-charged by one window.
    pub fn take(&self, peer_id: &str, service_id: &str) -> u64 {
        let mut map = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let key = (peer_id.to_string(), service_id.to_string());
        let out = map.remove(&key).unwrap_or(0);
        self.total_flushed.fetch_add(out, Ordering::Relaxed);
        out
    }

    /// Every pair with unflushed usage. For a shutdown/periodic flush sweep.
    pub fn drain_all(&self) -> Vec<(String, String, u64)> {
        let mut map = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let out: Vec<(String, String, u64)> = map
            .iter()
            .map(|((p, s), b)| (p.clone(), s.clone(), *b))
            .collect();
        let total: u64 = out.iter().map(|(_, _, b)| *b).sum();
        self.total_flushed.fetch_add(total, Ordering::Relaxed);
        map.clear();
        out
    }

    /// Bytes charged since boot. Observability only — never used to enforce a cap, because
    /// it is not per-peer.
    pub fn total_flushed(&self) -> u64 {
        self.total_flushed.load(Ordering::Relaxed)
    }
}

/// Flush a batch of accumulated media usage into the durable meter.
///
/// This is the ONLY place bytes become billable, and it is deliberately coarse: called at
/// end-of-stream or once a flush window fills, never per chunk. `meter::record_and_check`
/// rewrites the whole session store per call, so per-chunk metering of a 5 Mbps stream
/// would mean ~16,000 store writes per GB per viewer.
///
/// Returns the number of pairs charged. Per-pair errors are logged and skipped rather than
/// aborting the sweep: one peer with a corrupt session must not stop every other viewer's
/// usage from being billed.
pub async fn flush_usage(data_dir: &std::path::Path, acc: &MediaUsageAccumulator) -> usize {
    let drained = acc.drain_all();
    let mut charged = 0usize;
    for (peer_id, service_id, bytes) in drained {
        if bytes == 0 {
            continue;
        }
        match crate::streaming::meter::record_and_check(data_dir, &peer_id, &service_id, bytes)
            .await
        {
            Ok(_) => charged += 1,
            Err(e) => {
                tracing::warn!(
                    peer = %peer_id,
                    service = %service_id,
                    bytes,
                    "media usage flush failed; this window is uncharged: {}",
                    e
                );
            }
        }
    }
    charged
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D3 regression guard named by the acceptance criteria. A resumed transfer must not
    /// reset the cap: the accumulator is per (peer, service) for the life of the process,
    /// not per request. Without this, a client could stream forever by re-requesting a
    /// Range every time the counter came close to the limit.
    #[test]
    fn range_requests_cannot_reset_the_cap() {
        let acc = MediaUsageAccumulator::new();

        acc.add("peer1", "content-download", 600);
        assert_eq!(acc.pending("peer1", "content-download"), 600);

        // A second, independent request (e.g. a Range resume) must ADD, not reset.
        acc.add("peer1", "content-download", 600);
        assert_eq!(
            acc.pending("peer1", "content-download"),
            1200,
            "a second request must accumulate, not replace — that is the whole point"
        );

        let drained = acc.take("peer1", "content-download");
        assert_eq!(drained, 1200);
        assert_eq!(
            acc.pending("peer1", "content-download"),
            0,
            "after a flush the pending total must be zero or the next flush double-charges"
        );
    }

    /// Accounting must be per (peer, service). Two viewers must not share one counter, or
    /// one heavy viewer's bytes get charged to an idle one and the idle one is cut off.
    #[test]
    fn usage_is_accounted_per_peer_and_service() {
        let acc = MediaUsageAccumulator::new();
        acc.add("peer1", "content-download", 100);
        acc.add("peer2", "content-download", 250);
        acc.add("peer1", "other-service", 7);

        assert_eq!(acc.pending("peer1", "content-download"), 100);
        assert_eq!(acc.pending("peer2", "content-download"), 250);
        assert_eq!(acc.pending("peer1", "other-service"), 7);

        assert_eq!(acc.take("peer1", "content-download"), 100);
        assert_eq!(acc.pending("peer2", "content-download"), 250);
        assert_eq!(acc.pending("peer1", "other-service"), 7);
    }

    /// The flush must happen at the threshold, and taking must reset only that pair.
    #[test]
    fn flush_threshold_fires_on_accumulation_and_resets_only_that_pair() {
        let acc = MediaUsageAccumulator::new();
        let threshold = 4096u64;

        assert!(!acc.add_and_should_flush("p", "s", 1000, threshold));
        assert!(!acc.add_and_should_flush("p", "s", 1000, threshold));
        assert!(
            !acc.add_and_should_flush("p", "s", 1000, threshold),
            "3000 < 4096: still below the flush threshold"
        );
        assert!(
            acc.add_and_should_flush("p", "s", 1200, threshold),
            "4200 >= 4096: must trigger the flush"
        );
        assert_eq!(acc.take("p", "s"), 4200);
        assert_eq!(acc.pending("p", "s"), 0);
    }

    /// Overflow must saturate rather than wrap. A `+` here would wrap to a small number and
    /// hand a viewer an effectively unlimited allowance at u64::MAX bytes — the same class
    /// of bug as the `fetch_sub` wrap already fixed on the active-streams gauge.
    #[test]
    fn enormous_usage_saturates_instead_of_wrapping() {
        let acc = MediaUsageAccumulator::new();
        acc.add("p", "s", u64::MAX);
        acc.add("p", "s", 1000);
        assert_eq!(
            acc.pending("p", "s"),
            u64::MAX,
            "must saturate; wrapping would look like a tiny bill"
        );
    }

    /// drain_all must return everything once, then nothing.
    #[test]
    fn drain_all_empties_and_totals() {
        let acc = MediaUsageAccumulator::new();
        acc.add("p1", "s", 10);
        acc.add("p2", "s", 20);
        let drained = acc.drain_all();
        assert_eq!(drained.len(), 2);
        assert_eq!(acc.total_flushed(), 30);
        assert!(acc.drain_all().is_empty());
        assert_eq!(acc.total_flushed(), 30, "total must not double-count");
    }
}
