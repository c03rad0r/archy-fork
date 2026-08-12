//! Last-known-good FIPS peer endpoints (A3.10).
//!
//! The LAN direct-peering tick (`anchors::lan_fips_anchors`) only helps peers
//! we can currently see on the LAN. When a federation peer's LAN path is gone
//! (renumbered network, remote site, mDNS blackout) the only route left is the
//! anchor spanning tree — the exact hairpin RC2 calls out. But if we were EVER
//! connected to that peer directly, the daemon knew a working endpoint for it
//! (`fipsctl show peers` → `transport_addr`/`transport_type`, which covers
//! LAN, Tailscale, and WAN endpoints alike). This module persists those
//! npub-keyed endpoints and re-offers them as dial candidates when the live
//! paths disappear: LAN → last-known-good → anchor tree.
//!
//! Persisted at `<data_dir>/fips-endpoints.json`. Entries are refreshed every
//! time the peer is seen connected and dropped after `RETENTION` without a
//! sighting, so a peer that genuinely moved doesn't get dialed at a stale
//! address forever ( `fipsctl connect` to a dead address is harmless but not
//! free).

use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::fs;

use super::anchors::SeedAnchor;

const FILE_NAME: &str = "fips-endpoints.json";
/// Forget endpoints not seen connected for this long (seconds) — 30 days.
const RETENTION_SECS: u64 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KnownEndpoint {
    /// "ip:port" as reported by the daemon (`transport_addr`).
    pub address: String,
    /// "udp" | "tcp" (`transport_type`).
    pub transport: String,
    /// Unix seconds of the last time this peer was seen connected here.
    pub last_ok_unix: u64,
}

/// A currently-connected peer as parsed from `fipsctl show peers`.
#[derive(Debug, Clone)]
pub struct ConnectedPeer {
    pub npub: String,
    pub address: String,
    pub transport: String,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub async fn load(data_dir: &Path) -> HashMap<String, KnownEndpoint> {
    let path = data_dir.join(FILE_NAME);
    match fs::read(&path).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

async fn save(data_dir: &Path, map: &HashMap<String, KnownEndpoint>) -> Result<()> {
    let path = data_dir.join(FILE_NAME);
    let tmp = data_dir.join(format!("{FILE_NAME}.tmp"));
    fs::write(&tmp, serde_json::to_vec_pretty(map)?).await?;
    fs::rename(&tmp, &path).await?;
    Ok(())
}

/// Merge the currently-connected peers into the store (refreshing their
/// timestamps), prune expired entries, persist, and return the updated map.
/// Persistence failures are non-fatal — the in-memory result is still
/// returned so this tick's fallback logic works.
pub async fn record_connected(
    data_dir: &Path,
    connected: &[ConnectedPeer],
) -> HashMap<String, KnownEndpoint> {
    let mut map = load(data_dir).await;
    let now = now_unix();
    let before = map.clone();
    for p in connected {
        if p.npub.is_empty() || p.address.is_empty() {
            continue;
        }
        map.insert(
            p.npub.clone(),
            KnownEndpoint {
                address: p.address.clone(),
                transport: p.transport.clone(),
                last_ok_unix: now,
            },
        );
    }
    map.retain(|_, e| now.saturating_sub(e.last_ok_unix) <= RETENTION_SECS);
    if map != before {
        if let Err(e) = save(data_dir, &map).await {
            tracing::debug!("fips endpoint store save failed (non-fatal): {e}");
        }
    }
    map
}

/// Build fallback anchors for federation peers whose live paths are gone:
/// every `wanted_npub` that is neither currently connected nor covered by a
/// live LAN direct entry, but has a last-known-good endpoint, becomes a dial
/// candidate. `fipsctl connect` is idempotent and failure-tolerant, so a
/// stale candidate costs one failed dial, bounded by apply()'s per-connect
/// timeout.
pub fn fallback_anchors(
    known: &HashMap<String, KnownEndpoint>,
    wanted_npubs: &[String],
    connected_npubs: &[String],
    lan_direct: &[SeedAnchor],
) -> Vec<SeedAnchor> {
    let mut out = Vec::new();
    for npub in wanted_npubs {
        if connected_npubs.iter().any(|c| c == npub) {
            continue;
        }
        if lan_direct.iter().any(|a| &a.npub == npub) {
            continue;
        }
        if let Some(e) = known.get(npub) {
            out.push(SeedAnchor {
                npub: npub.clone(),
                address: e.address.clone(),
                transport: e.transport.clone(),
                label: "last-known-good endpoint (direct FIPS)".to_string(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(addr: &str) -> KnownEndpoint {
        KnownEndpoint {
            address: addr.to_string(),
            transport: "udp".to_string(),
            last_ok_unix: now_unix(),
        }
    }

    #[tokio::test]
    async fn record_and_reload_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let connected = vec![ConnectedPeer {
            npub: "npub1aaa".into(),
            address: "100.64.0.21:2121".into(),
            transport: "udp".into(),
        }];
        let map = record_connected(dir.path(), &connected).await;
        assert_eq!(map["npub1aaa"].address, "100.64.0.21:2121");
        let reloaded = load(dir.path()).await;
        assert_eq!(reloaded, map);
    }

    #[tokio::test]
    async fn expired_entries_are_pruned_on_record() {
        let dir = tempfile::tempdir().unwrap();
        let mut stale = HashMap::new();
        stale.insert(
            "npub1old".to_string(),
            KnownEndpoint {
                address: "10.0.0.1:2121".into(),
                transport: "udp".into(),
                last_ok_unix: now_unix() - RETENTION_SECS - 60,
            },
        );
        save(dir.path(), &stale).await.unwrap();
        let map = record_connected(dir.path(), &[]).await;
        assert!(map.is_empty());
    }

    #[test]
    fn fallback_skips_connected_and_lan_covered_peers() {
        let mut known = HashMap::new();
        known.insert("npub1gone".to_string(), ep("100.1.2.3:2121"));
        known.insert("npub1conn".to_string(), ep("100.1.2.4:2121"));
        known.insert("npub1lan".to_string(), ep("100.1.2.5:2121"));
        let wanted: Vec<String> = ["npub1gone", "npub1conn", "npub1lan", "npub1never"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let connected = vec!["npub1conn".to_string()];
        let lan = vec![SeedAnchor {
            npub: "npub1lan".into(),
            address: "192.0.2.198:2121".into(),
            transport: "udp".into(),
            label: "LAN".into(),
        }];
        let out = fallback_anchors(&known, &wanted, &connected, &lan);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].npub, "npub1gone");
        assert_eq!(out[0].address, "100.1.2.3:2121");
        // npub1never has no stored endpoint → nothing to dial.
    }
}
