//! In-process counters for FIPS dial outcomes.
//!
//! Every peer dial that could have used FIPS either succeeds over FIPS or
//! falls back to Tor for one of six reasons (F1–F6). Before these counters
//! existed, fallbacks were `debug!`-only and invisible in production, which
//! made "FIPS uptime" unfalsifiable — several paths were 100% Tor for months
//! (dead ports, firewalled listeners, allowlist 404s) and nothing surfaced
//! it. The counters are process-lifetime (reset on restart) and exposed via
//! `fips.status` as `dial_stats`, so a fleet-wide fallback regression shows
//! up on the dashboard instead of as vague slowness.

use std::sync::atomic::{AtomicU64, Ordering};

/// Why a FIPS-capable dial fell back to Tor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackReason {
    /// F1 — no FIPS npub known for the peer (never meshed, or pre-npub
    /// federation record). Expected for non-FIPS peers; high counts here
    /// mean npub propagation is broken, not the transport.
    NoNpub,
    /// F2 — the local FIPS daemon service isn't active.
    ServiceInactive,
    /// F3 — the local FIPS DNS resolver couldn't resolve the peer's npub
    /// (daemon up but peer not in the identity cache / mesh unreachable).
    DnsFail,
    /// F4 — TCP/HTTP dial to the peer's ULA failed or exceeded the FIPS
    /// attempt budget (firewalled :5679, cold hole-punch, peer down).
    ConnectFail,
    /// F5 — peer answered over FIPS with 404: its listener doesn't serve
    /// this path (older build / stricter allowlist).
    Http404,
    /// F6 — peer answered over FIPS with a 5xx server error.
    Http5xx,
}

impl FallbackReason {
    pub fn key(self) -> &'static str {
        match self {
            Self::NoNpub => "no_npub",
            Self::ServiceInactive => "service_inactive",
            Self::DnsFail => "dns_fail",
            Self::ConnectFail => "connect_fail",
            Self::Http404 => "http_404",
            Self::Http5xx => "http_5xx",
        }
    }
}

static FIPS_OK: AtomicU64 = AtomicU64::new(0);
static NO_NPUB: AtomicU64 = AtomicU64::new(0);
static SERVICE_INACTIVE: AtomicU64 = AtomicU64::new(0);
static DNS_FAIL: AtomicU64 = AtomicU64::new(0);
static CONNECT_FAIL: AtomicU64 = AtomicU64::new(0);
static HTTP_404: AtomicU64 = AtomicU64::new(0);
static HTTP_5XX: AtomicU64 = AtomicU64::new(0);

fn counter(reason: FallbackReason) -> &'static AtomicU64 {
    match reason {
        FallbackReason::NoNpub => &NO_NPUB,
        FallbackReason::ServiceInactive => &SERVICE_INACTIVE,
        FallbackReason::DnsFail => &DNS_FAIL,
        FallbackReason::ConnectFail => &CONNECT_FAIL,
        FallbackReason::Http404 => &HTTP_404,
        FallbackReason::Http5xx => &HTTP_5XX,
    }
}

/// A dial completed over FIPS (any HTTP status that wasn't a fallback
/// trigger — the peer was reached on the mesh).
pub fn record_fips_ok() {
    FIPS_OK.fetch_add(1, Ordering::Relaxed);
}

/// A FIPS-capable dial fell back to Tor.
pub fn record_fallback(reason: FallbackReason) {
    counter(reason).fetch_add(1, Ordering::Relaxed);
}

/// `(fips_ok, connect_fail)` totals for the connectivity watcher: a window
/// where connect_fail grows while fips_ok doesn't is a degraded data path —
/// including the "daemon says connected but packets blackhole" failure the
/// link-state check alone can't see (observed live 2026-07-27 on .198).
pub fn totals() -> (u64, u64) {
    (
        FIPS_OK.load(Ordering::Relaxed),
        CONNECT_FAIL.load(Ordering::Relaxed),
    )
}

/// Snapshot for `fips.status` (`dial_stats`). Process-lifetime counts.
pub fn snapshot() -> serde_json::Value {
    let f1 = NO_NPUB.load(Ordering::Relaxed);
    let f2 = SERVICE_INACTIVE.load(Ordering::Relaxed);
    let f3 = DNS_FAIL.load(Ordering::Relaxed);
    let f4 = CONNECT_FAIL.load(Ordering::Relaxed);
    let f5 = HTTP_404.load(Ordering::Relaxed);
    let f6 = HTTP_5XX.load(Ordering::Relaxed);
    serde_json::json!({
        "fips_ok": FIPS_OK.load(Ordering::Relaxed),
        "fallbacks": {
            "no_npub": f1,
            "service_inactive": f2,
            "dns_fail": f3,
            "connect_fail": f4,
            "http_404": f5,
            "http_5xx": f6,
            "total": f1 + f2 + f3 + f4 + f5 + f6,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_counts_recorded_events() {
        // Counters are global; assert deltas rather than absolutes so this
        // test stays correct alongside any other test that dials.
        let before = snapshot();
        record_fips_ok();
        record_fallback(FallbackReason::ConnectFail);
        record_fallback(FallbackReason::Http404);
        let after = snapshot();
        let d = |v: &serde_json::Value, path: &[&str]| -> u64 {
            let mut cur = v;
            for p in path {
                cur = &cur[p];
            }
            cur.as_u64().unwrap()
        };
        assert_eq!(d(&after, &["fips_ok"]) - d(&before, &["fips_ok"]), 1);
        assert_eq!(
            d(&after, &["fallbacks", "connect_fail"]) - d(&before, &["fallbacks", "connect_fail"]),
            1
        );
        assert_eq!(
            d(&after, &["fallbacks", "http_404"]) - d(&before, &["fallbacks", "http_404"]),
            1
        );
        assert!(d(&after, &["fallbacks", "total"]) >= 2);
    }

    #[test]
    fn reason_keys_are_stable() {
        // These strings are the fips.status API surface — renaming one is a
        // breaking change for the UI.
        assert_eq!(FallbackReason::NoNpub.key(), "no_npub");
        assert_eq!(FallbackReason::ServiceInactive.key(), "service_inactive");
        assert_eq!(FallbackReason::DnsFail.key(), "dns_fail");
        assert_eq!(FallbackReason::ConnectFail.key(), "connect_fail");
        assert_eq!(FallbackReason::Http404.key(), "http_404");
        assert_eq!(FallbackReason::Http5xx.key(), "http_5xx");
    }
}
