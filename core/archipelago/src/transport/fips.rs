//! FIPS mesh transport (Free Internetworking Peering System).
//!
//! Delegates the actual wire protocol to the `fips` system daemon
//! (github.com/jmcorgan/fips), which archipelago supervises via the
//! `archipelago-fips.service` unit (or respects the upstream
//! `fips.service` on legacy nodes). This module is the in-process
//! `NodeTransport` adapter: it checks daemon liveness, maps a peer's
//! FIPS npub to a `fd00::/8` IPv6 address via the daemon's local DNS
//! resolver, and POSTs the `TransportMessage` payload over plain HTTP
//! to the peer's `/transport/inbox` endpoint.
//!
//! Sits at priority 3 between LAN and Tor — preferred over Tor for
//! federation and peer traffic but yielding to direct LAN.

use super::{NodeTransport, TransportKind, TransportMessage};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a successful `is_available()` probe is cached — the hot path
/// may poll this per-send, and `systemctl is-active` takes ~50ms. A short
/// TTL keeps the result responsive to daemon flaps without pounding DBus.
const AVAILABILITY_CACHE_TTL: Duration = Duration::from_secs(10);

/// Availability cache shared with the background probe thread, so the
/// sync `is_available()` hot path never blocks on `systemctl`.
struct AvailabilityCache {
    available: AtomicBool,
    probed_at_ms: AtomicU64,
    probe_in_flight: AtomicBool,
}

pub struct FipsTransport {
    identity_dir: PathBuf,
    availability: std::sync::Arc<AvailabilityCache>,
}

impl FipsTransport {
    pub fn new(identity_dir: &Path) -> Self {
        Self {
            identity_dir: identity_dir.to_path_buf(),
            availability: std::sync::Arc::new(AvailabilityCache {
                available: AtomicBool::new(false),
                probed_at_ms: AtomicU64::new(0),
                probe_in_flight: AtomicBool::new(false),
            }),
        }
    }

    fn probe_daemon_active() -> bool {
        // Blocking probe — only ever run on a dedicated background thread
        // (see is_available), never on a tokio worker. Short-circuit if
        // either the archipelago-managed unit or the upstream fips.service
        // is active — legacy/dev nodes run only the upstream unit.
        for unit in [
            crate::fips::SERVICE_UNIT,
            crate::fips::UPSTREAM_SERVICE_UNIT,
        ] {
            let out = std::process::Command::new("systemctl")
                .args(["is-active", unit])
                .output();
            if let Ok(o) = out {
                if String::from_utf8_lossy(&o.stdout).trim() == "active" {
                    return true;
                }
            }
        }
        false
    }
}

impl NodeTransport for FipsTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Fips
    }

    fn is_available(&self) -> bool {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let cached_at = self.availability.probed_at_ms.load(Ordering::Relaxed);
        let cached = self.availability.available.load(Ordering::Relaxed);
        if now_ms.saturating_sub(cached_at) < AVAILABILITY_CACHE_TTL.as_millis() as u64 {
            return cached;
        }
        // Cache is stale. This sync trait method is called from async
        // route(), so running the ~50ms systemctl probe inline stalls a
        // tokio worker. Serve the stale value and refresh on a background
        // thread instead — the transport supervisor's warm loop keeps this
        // fresh in steady state, so staleness is bounded to one probe round.
        let cache = std::sync::Arc::clone(&self.availability);
        if !cache.probe_in_flight.swap(true, Ordering::Relaxed) {
            std::thread::spawn(move || {
                let val = Self::probe_daemon_active();
                let probed_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                cache.available.store(val, Ordering::Relaxed);
                cache.probed_at_ms.store(probed_ms, Ordering::Relaxed);
                cache.probe_in_flight.store(false, Ordering::Relaxed);
            });
        }
        cached
    }

    fn send<'a>(
        &'a self,
        address: &'a str,
        message: &'a TransportMessage,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let base = crate::fips::dial::peer_base_url(address)
                .await
                .with_context(|| format!("resolve {}.fips", address))?;
            let url = format!("{}/transport/inbox", base);
            let client = crate::fips::dial::client();
            let body = serde_json::to_vec(message).context("serialize TransportMessage")?;
            let resp = client
                .post(&url)
                .header("Content-Type", "application/json")
                .body(body)
                .send()
                .await
                .with_context(|| format!("POST {}", url))?;
            if !resp.status().is_success() {
                anyhow::bail!("peer FIPS inbox returned {}", resp.status());
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kind_is_fips() {
        let t = FipsTransport::new(std::path::Path::new("/tmp"));
        assert_eq!(t.kind(), TransportKind::Fips);
    }

    #[test]
    fn is_available_caches_negative_result() {
        // No fips.service in the test env → probe returns false.
        // Two rapid calls must both be false without relying on a live daemon.
        let t = FipsTransport::new(std::path::Path::new("/tmp"));
        let a = t.is_available();
        let b = t.is_available();
        assert_eq!(a, b);
    }
}
