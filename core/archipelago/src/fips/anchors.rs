//! Seed-anchor management for FIPS bootstrap.
//!
//! A freshly-installed node can't reach the global mesh via npub
//! routing until it's connected to at least one peer that's already in
//! the DHT. Upstream `fips` solves this by dialing a public anchor
//! (e.g. `fips.v0l.io`) on first start. That's a single point of
//! failure and doesn't help nodes behind restrictive firewalls or
//! intermittent networks — archipelago operators reported fresh
//! installs failing to reach any public anchor.
//!
//! This module adds a local, operator-editable seed-anchor list. Each
//! entry is a `{npub, address, transport}` triple that archipelago
//! pushes into the running daemon via `fipsctl connect` on startup and
//! periodically thereafter. If one anchor falls over, the next one
//! seeds the DHT instead. A well-configured cluster (e.g. a VPS
//! running fips in anchor mode + a couple of home nodes) stops
//! depending on the global anchor entirely.
//!
//! The list is persisted at `<data_dir>/seed-anchors.json`. The
//! archipelago service user owns that directory, so no sudo is needed
//! to read or write it.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// On-disk filename under `data_dir/`.
const SEED_ANCHORS_FILE: &str = "seed-anchors.json";

/// Public anchor (`fips.v0l.io`) carried as a default seed for every
/// node — it bootstraps DHT routing so a fresh node isn't isolated.
/// Operators can remove it from the UI once their own cluster has
/// independent anchors (removal persists, see `load`/`remove`).
///
/// IMPORTANT transport details, learned the hard way (see git history /
/// the 2026-06-15 debugging on .116):
///   - The anchor answers ONLY on **TCP port 8443**. UDP 8668 is dead
///     (host pings on both IP families but never completes a UDP FIPS
///     handshake). `fips/config.rs` always knew this; the old default
///     here (`fips.v0l.io:8668`/udp) silently never connected fleet-wide.
///   - We use the **IPv4 literal** rather than the `fips.v0l.io` hostname
///     on purpose: the hostname resolves IPv6-first, but the daemon binds
///     its transports IPv4-only (`0.0.0.0:8443`), so a v6 target makes the
///     daemon fail to send the handshake with `EAFNOSUPPORT (os error 97)`.
///     An IPv4 literal sidesteps the resolver entirely.
pub const DEFAULT_PUBLIC_ANCHOR_NPUB: &str =
    "npub1zv58cn7v83mxvttl70w5fwjwuclfmntv9cnmv5wmz2nzz88u5urqvdx96n";
pub const DEFAULT_PUBLIC_ANCHOR_ADDR: &str = "185.18.221.160:8443";
pub const DEFAULT_PUBLIC_ANCHOR_TRANSPORT: &str = "tcp";

/// The upstream public anchor as a ready-to-apply `SeedAnchor`.
pub fn default_public_anchor() -> SeedAnchor {
    SeedAnchor {
        npub: DEFAULT_PUBLIC_ANCHOR_NPUB.to_string(),
        address: DEFAULT_PUBLIC_ANCHOR_ADDR.to_string(),
        transport: DEFAULT_PUBLIC_ANCHOR_TRANSPORT.to_string(),
        label: "Public anchor (fips.v0l.io)".to_string(),
    }
}

// Archipelago-operated anchor on vps2 (the OTA/registry host, 146.59.87.168).
// Every node already reaches this host for updates, so it is reachable from
// networks that the upstream anchor is not — which is most of them (the
// upstream anchor answers on one IPv4 that many home/office networks can't
// reach, and its DNS resolves IPv6-first while the daemon is IPv4-only).
// TCP because that traverses NAT/firewalls best; 8444 because 8443 on that
// host is already taken by a container.
pub const ARCHY_ANCHOR_NPUB: &str =
    "npub1dptaktwxv0mm245g2lqjykwm5ll0jpc6m3r4242ydfa9z7qe6urs3jvrak";
pub const ARCHY_ANCHOR_ADDR: &str = "146.59.87.168:8444";
pub const ARCHY_ANCHOR_TRANSPORT: &str = "tcp";

/// The Archipelago-operated anchor as a ready-to-apply `SeedAnchor`.
pub fn archy_anchor() -> SeedAnchor {
    SeedAnchor {
        npub: ARCHY_ANCHOR_NPUB.to_string(),
        address: ARCHY_ANCHOR_ADDR.to_string(),
        transport: ARCHY_ANCHOR_TRANSPORT.to_string(),
        label: "Archipelago anchor (vps2)".to_string(),
    }
}

/// Public FIPS-network anchors from join.fips.network (adverts published as
/// Nostr kind-37195 `fips-overlay-v1` events, refreshed hourly). Of the eight
/// live test anchors (2026-07-22), these two are the dual-transport ones —
/// they answer on TCP as well as UDP, and TCP is what traverses restrictive
/// networks. The remaining six are UDP-only; add them per-node via
/// `fips.add-seed-anchor` if more rendezvous diversity is wanted.
pub fn fips_network_anchors() -> Vec<SeedAnchor> {
    vec![
        SeedAnchor {
            npub: "npub10yffd020a4ag8zcy75f9pruq3rnghvvhd5hphl9s62zgp35s560qrksp9u".to_string(),
            address: "23.182.128.74:443".to_string(),
            transport: "tcp".to_string(),
            label: "FIPS network anchor (join.fips.network)".to_string(),
        },
        SeedAnchor {
            npub: "npub1qmc3cvfz0yu2hx96nq3gp55zdan2qclealn7xshgr448d3nh6lks7zel98".to_string(),
            address: "217.77.8.91:443".to_string(),
            transport: "tcp".to_string(),
            label: "FIPS network anchor (join.fips.network)".to_string(),
        },
    ]
}

/// The default anchor set carried implicitly by `load()` on nodes that have
/// never edited their anchor list, so every node dials them without operator
/// action. Multiple anchors so one unreachable rendezvous host can't strand a
/// node: `fipsctl connect` is attempted for each, and whichever the node's
/// network can reach wins. The Archipelago-operated anchor is listed first
/// because it is reachable from the widest set of networks.
pub fn default_public_anchors() -> Vec<SeedAnchor> {
    let mut anchors = vec![archy_anchor(), default_public_anchor()];
    anchors.extend(fips_network_anchors());
    anchors
}

/// One seed-anchor entry. `address` must be directly dialable (IP or
/// resolvable hostname + UDP port); `transport` is one of "udp", "tcp",
/// "tor", "ethernet" (the values upstream `fipsctl connect` accepts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SeedAnchor {
    /// Bech32 `npub1...` of the anchor's FIPS identity.
    pub npub: String,
    /// Directly-dialable transport address, e.g. `192.0.2.12:8668`.
    pub address: String,
    /// Transport to use — almost always `"udp"`.
    #[serde(default = "default_transport")]
    pub transport: String,
    /// Human-readable note shown in the UI (e.g. "Home anchor", "VPS").
    #[serde(default)]
    pub label: String,
}

fn default_transport() -> String {
    "udp".to_string()
}

fn anchors_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SEED_ANCHORS_FILE)
}

/// Load the seed-anchor list. A node that has never edited its anchor
/// list (no file yet) gets the default public anchor so it can bootstrap
/// the mesh out of the box. Once the operator edits anchors — including
/// removing the default — a file exists and is authoritative, so removal
/// persists and we never silently re-add it.
pub async fn load(data_dir: &Path) -> Result<Vec<SeedAnchor>> {
    let path = anchors_path(data_dir);
    if !path.exists() {
        return Ok(default_public_anchors());
    }
    let bytes = tokio::fs::read(&path)
        .await
        .with_context(|| format!("read {}", path.display()))?;
    let anchors: Vec<SeedAnchor> =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    Ok(repair_legacy_anchor_set(anchors))
}

/// Add the newer redundant public TCP anchors to legacy files that only carry
/// the Archipelago-operated vps2 anchor. A deliberately custom/private anchor
/// list remains authoritative; this only repairs the exact stale shape shipped
/// before the join.fips.network anchors became defaults.
fn repair_legacy_anchor_set(mut anchors: Vec<SeedAnchor>) -> Vec<SeedAnchor> {
    let has_archy = anchors.iter().any(|a| a.npub == ARCHY_ANCHOR_NPUB);
    let has_fips_network = fips_network_anchors()
        .iter()
        .any(|default| anchors.iter().any(|a| a.npub == default.npub));
    if has_archy && !has_fips_network {
        for anchor in fips_network_anchors() {
            anchors.push(anchor);
        }
    }
    anchors
}

/// Persist the list. Overwrites atomically via write-then-rename so a
/// crashed archipelago never leaves a half-written config.
pub async fn save(data_dir: &Path, anchors: &[SeedAnchor]) -> Result<()> {
    tokio::fs::create_dir_all(data_dir)
        .await
        .with_context(|| format!("mkdir -p {}", data_dir.display()))?;
    let path = anchors_path(data_dir);
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(anchors).context("serialize seed anchors")?;
    tokio::fs::write(&tmp, json)
        .await
        .with_context(|| format!("write {}", tmp.display()))?;
    tokio::fs::rename(&tmp, &path)
        .await
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

/// Add (or update) one anchor, keyed by npub. Returns the resulting list.
pub async fn add(data_dir: &Path, anchor: SeedAnchor) -> Result<Vec<SeedAnchor>> {
    let mut list = load(data_dir).await?;
    if let Some(existing) = list.iter_mut().find(|a| a.npub == anchor.npub) {
        *existing = anchor;
    } else {
        list.push(anchor);
    }
    save(data_dir, &list).await?;
    Ok(list)
}

/// Remove an anchor by npub. Returns the resulting list.
pub async fn remove(data_dir: &Path, npub: &str) -> Result<Vec<SeedAnchor>> {
    let mut list = load(data_dir).await?;
    list.retain(|a| a.npub != npub);
    save(data_dir, &list).await?;
    Ok(list)
}

/// Apply the seed anchors to the running FIPS daemon. For each entry,
/// asks `fipsctl connect` to dial the peer. Errors are logged but don't
/// fail the whole operation — a single unreachable anchor shouldn't
/// block the others.
///
/// `fipsctl connect` is idempotent-ish: calling it for an already-
/// connected peer is a no-op at the protocol layer, so re-applying on
/// a timer is safe. Returns a list of per-anchor results for logging.
///
/// Invoked through `sudo -n`: the upstream daemon's control socket
/// (`/run/fips/control.sock`) is owned `root:fips` 0660, and the
/// archipelago service user is not in the `fips` group, so a bare
/// `fipsctl connect` fails with EACCES. This matches the privileged
/// `sudo -n fipsctl show peers` call in `service::peer_connectivity_summary`.
/// Without it, seed anchors persist to disk but never actually dial,
/// leaving `anchor_connected=false` and every peer dial falling back to
/// a slow Tor timeout.
pub async fn apply(anchors: &[SeedAnchor]) -> Vec<ApplyResult> {
    // Concurrent, each connect hard-capped: the old serial loop waited
    // unbounded on every `sudo fipsctl connect`, so one hung subprocess
    // stalled the whole apply — and the periodic anchor tick behind it,
    // which is exactly when a wedged daemon most needs the re-apply.
    let futs = anchors.iter().cloned().map(|anchor| async move {
        let out = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            Command::new("sudo")
                .args([
                    "-n",
                    "fipsctl",
                    "connect",
                    &anchor.npub,
                    &anchor.address,
                    &anchor.transport,
                ])
                .output(),
        )
        .await;
        let result = match out {
            Ok(Ok(o)) if o.status.success() => ApplyResult {
                npub: anchor.npub.clone(),
                ok: true,
                message: String::from_utf8_lossy(&o.stdout).trim().to_string(),
            },
            Ok(Ok(o)) => ApplyResult {
                npub: anchor.npub.clone(),
                ok: false,
                message: format!(
                    "sudo fipsctl connect exited {}: {}",
                    o.status,
                    String::from_utf8_lossy(&o.stderr).trim()
                ),
            },
            Ok(Err(e)) => ApplyResult {
                npub: anchor.npub.clone(),
                ok: false,
                message: format!("sudo fipsctl launch failed: {}", e),
            },
            Err(_) => ApplyResult {
                npub: anchor.npub.clone(),
                ok: false,
                message: "sudo fipsctl connect timed out after 15s".to_string(),
            },
        };
        if result.ok {
            tracing::debug!(npub = %result.npub, "Seed anchor applied");
        } else {
            tracing::warn!(
                npub = %result.npub,
                message = %result.message,
                "Seed anchor apply failed (non-fatal)"
            );
        }
        result
    });
    futures_util::future::join_all(futs).await
}

/// Outcome of a single `fipsctl connect` call.
#[derive(Debug, Clone)]
pub struct ApplyResult {
    pub npub: String,
    pub ok: bool,
    pub message: String,
}

/// FIPS UDP transport port (matches `transports.udp.bind_addr` in the generated
/// `fips.yaml`). Direct peer links dial this, NOT the HTTP/LAN messaging port.
const FIPS_UDP_PORT: u16 = crate::fips::PUBLISHED_UDP_PORT;

/// Build transient seed-anchor entries that dial LAN-discovered federation peers
/// directly over their FIPS UDP transport. For each peer the registry knows both
/// a LAN socket address AND a FIPS npub for, point a `udp` anchor at
/// `<lan-ip>:<FIPS_UDP_PORT>`. This lets co-located federation nodes form a DIRECT FIPS link
/// instead of depending on the global anchor's spanning tree to route between
/// them (the cause of every dial falling back to Tor when the anchor link flaps).
///
/// This is FIPS's own UDP transport over the LAN — not Tailscale, not the LAN
/// HTTP messaging port. NOT persisted to `seed-anchors.json`: recomputed each
/// apply tick from live LAN discovery, so a peer's changing IP self-corrects and
/// stale entries never accumulate. `fipsctl connect` is idempotent, so
/// re-applying just keeps the link warm.
pub fn lan_fips_anchors(peers: &[crate::transport::PeerRecord]) -> Vec<SeedAnchor> {
    let mut out = Vec::new();
    for p in peers {
        let (Some(lan), Some(npub)) = (p.lan_address.as_deref(), p.fips_npub.as_deref()) else {
            continue;
        };
        // lan_address is the peer's HTTP/LAN socket ("ip:port"); reuse only its IP
        // and target the FIPS UDP port. SocketAddr::new(...).to_string() formats
        // IPv6 with brackets correctly.
        let Ok(sa) = lan.parse::<std::net::SocketAddr>() else {
            continue;
        };
        out.push(SeedAnchor {
            npub: npub.to_string(),
            address: std::net::SocketAddr::new(sa.ip(), FIPS_UDP_PORT).to_string(),
            transport: "udp".to_string(),
            label: "LAN federation peer (direct FIPS)".to_string(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(npub: &str) -> SeedAnchor {
        SeedAnchor {
            npub: npub.to_string(),
            address: "example.test:8668".to_string(),
            transport: "udp".to_string(),
            label: "test".to_string(),
        }
    }

    #[tokio::test]
    async fn load_missing_seeds_default_public_anchors() {
        // A node that has never edited its anchor list should still get the
        // full default anchor set so it can bootstrap the mesh out of the box.
        let dir = tempfile::tempdir().unwrap();
        let got = load(dir.path()).await.unwrap();
        assert_eq!(got, default_public_anchors());
        // The Archipelago-operated anchor must come first (widest reachability)
        // and the upstream anchor must remain present as a fallback.
        assert_eq!(got[0], archy_anchor());
        assert!(got.contains(&default_public_anchor()));
        // Every default must be a TCP form (traverses NAT/firewalls), never the
        // dead udp:8668 the upstream anchor never answers on.
        assert!(got.iter().all(|a| a.transport == "tcp"));
    }

    #[tokio::test]
    async fn load_repairs_legacy_archy_only_anchor_file() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &[archy_anchor()]).await.unwrap();

        let got = load(dir.path()).await.unwrap();
        assert!(got.iter().any(|a| a.npub == ARCHY_ANCHOR_NPUB));
        for anchor in fips_network_anchors() {
            assert!(got.iter().any(|a| a.npub == anchor.npub));
        }
    }

    #[tokio::test]
    async fn load_keeps_private_anchor_file_authoritative() {
        let dir = tempfile::tempdir().unwrap();
        let private = mk("npub1private");
        save(dir.path(), std::slice::from_ref(&private))
            .await
            .unwrap();

        let got = load(dir.path()).await.unwrap();
        assert_eq!(got, vec![private]);
    }

    #[tokio::test]
    async fn removing_one_default_persists_and_keeps_the_other() {
        // Editing the anchor list (here removing one default) makes the file
        // authoritative: the removed anchor must not be silently re-seeded on
        // next load, and the remaining default must stay.
        let dir = tempfile::tempdir().unwrap();
        let list = remove(dir.path(), ARCHY_ANCHOR_NPUB).await.unwrap();
        assert!(!list.iter().any(|a| a.npub == ARCHY_ANCHOR_NPUB));
        assert!(list.contains(&default_public_anchor()));
        let got = load(dir.path()).await.unwrap();
        assert_eq!(got, list, "edited list is authoritative; no re-seed");
    }

    #[tokio::test]
    async fn removing_all_defaults_persists_as_empty() {
        // Removing every default leaves an empty authoritative list that must
        // not be re-seeded on next load.
        let dir = tempfile::tempdir().unwrap();
        let mut list = Vec::new();
        for anchor in default_public_anchors() {
            list = remove(dir.path(), &anchor.npub).await.unwrap();
        }
        assert!(list.is_empty());
        let got = load(dir.path()).await.unwrap();
        assert!(got.is_empty(), "defaults must stay removed once edited");
    }

    #[tokio::test]
    async fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let a = mk("npub1aaa");
        let b = mk("npub1bbb");
        save(dir.path(), &[a.clone(), b.clone()]).await.unwrap();
        let got = load(dir.path()).await.unwrap();
        assert_eq!(got, vec![a, b]);
    }

    #[tokio::test]
    async fn add_replaces_existing_by_npub() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = mk("npub1aaa");
        save(dir.path(), &[a.clone()]).await.unwrap();
        a.address = "newhost:8668".to_string();
        let list = add(dir.path(), a.clone()).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].address, "newhost:8668");
    }

    #[tokio::test]
    async fn remove_by_npub() {
        let dir = tempfile::tempdir().unwrap();
        save(
            dir.path(),
            &[mk("npub1aaa"), mk("npub1bbb"), mk("npub1ccc")],
        )
        .await
        .unwrap();
        let list = remove(dir.path(), "npub1bbb").await.unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|a| a.npub != "npub1bbb"));
    }

    #[test]
    fn seed_anchor_uses_udp_by_default() {
        let json = r#"{"npub":"npub1x","address":"h:8668"}"#;
        let a: SeedAnchor = serde_json::from_str(json).unwrap();
        assert_eq!(a.transport, "udp");
        assert_eq!(a.label, "");
    }

    #[test]
    fn lan_fips_anchor_port_matches_daemon_bind() {
        // Drift guard: direct LAN anchors must dial the UDP port the
        // generated fips.yaml actually binds. These were out of sync for
        // months (anchors dialed 8668, the daemon bound 2121), making the
        // whole direct-peering feature dial a dead port.
        let yaml = crate::fips::config::render_config_yaml();
        assert!(
            yaml.contains(&format!("0.0.0.0:{FIPS_UDP_PORT}")),
            "lan_fips_anchors dials :{FIPS_UDP_PORT} but the daemon config binds elsewhere"
        );
    }

    #[test]
    fn lan_fips_anchors_builds_direct_entry() {
        let peer = crate::transport::PeerRecord {
            did: "did:key:zpeer".to_string(),
            lan_address: Some("192.0.2.198:5678".to_string()),
            fips_npub: Some("npub1peer".to_string()),
            ..Default::default()
        };
        let out = lan_fips_anchors(&[peer]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].address, format!("192.0.2.198:{FIPS_UDP_PORT}"));
        assert_eq!(out[0].transport, "udp");

        // Peers missing either the LAN address or the npub produce nothing.
        let no_npub = crate::transport::PeerRecord {
            did: "did:key:zother".to_string(),
            lan_address: Some("192.0.2.199:5678".to_string()),
            ..Default::default()
        };
        assert!(lan_fips_anchors(&[no_npub]).is_empty());
    }
}
