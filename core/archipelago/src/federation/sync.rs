//! Federation state sync and remote deployment.
//!
//! Requests prefer FIPS (direct ULA dial, ~LAN latency) and fall back to
//! Tor on any network failure. See `crate::fips::dial::PeerRequest` for
//! the fallback mechanics.

use anyhow::{Context, Result};
use std::path::Path;

use super::storage::update_node_state;
use super::types::{AppStatus, FederatedNode, FederationPeerHint, NodeStateSnapshot, TrustLevel};
use crate::fips::dial::PeerRequest;

/// Sync state with a single federated peer. Tries FIPS first; falls back
/// to Tor on any transport-level failure.
pub async fn sync_with_peer(
    data_dir: &Path,
    peer: &FederatedNode,
    local_did: &str,
    sign_fn: impl FnOnce(&[u8]) -> String,
) -> Result<NodeStateSnapshot> {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let signature = sign_fn(timestamp.as_bytes());

    let body = serde_json::json!({
        "method": "federation.get-state",
        "params": {}
    });

    let (resp, transport) = PeerRequest::new(peer.fips_npub.as_deref(), &peer.onion, "/rpc/v1")
        .service(crate::settings::transport::PeerService::Federation)
        .header("X-Federation-DID", local_did)
        .header("X-Federation-Sig", signature)
        .header("X-Federation-Timestamp", timestamp)
        .timeout(std::time::Duration::from_secs(30))
        // Fast-fail a cold/unreachable FIPS overlay (common between LAN and
        // remote/Tailscale peers that share no FIPS path) so the Tor fallback —
        // which answers an onion in ~3-5s — isn't stuck behind the full 30s FIPS
        // budget. Without this, a state sync to a FIPS-unreachable peer "took
        // ages" and join/sync appeared to time out even though Tor was healthy.
        .fips_timeout(std::time::Duration::from_secs(6))
        .send_json(&body)
        .await
        .context("Failed to reach federated peer")?;

    if !resp.status().is_success() {
        anyhow::bail!("Peer returned {} (via {})", resp.status(), transport);
    }

    // Record transport used so the UI badge on this peer's card reflects
    // the transport that actually carried the call, not a prediction.
    if let Err(e) = super::storage::record_peer_transport(
        data_dir,
        Some(&peer.did),
        Some(&peer.onion),
        &transport.to_string(),
    )
    .await
    {
        tracing::warn!("Failed to persist peer transport badge: {e:#}");
    }

    let result: serde_json::Value = resp.json().await.context("Invalid response from peer")?;
    let state_val = result
        .get("result")
        .ok_or_else(|| anyhow::anyhow!("No result in peer response"))?;

    let state: NodeStateSnapshot =
        serde_json::from_value(state_val.clone()).context("Failed to parse peer state")?;

    update_node_state(data_dir, &peer.did, state.clone()).await?;

    // Transitive federation: merge in peers our (Trusted) source advertised
    // so we can route directly to them over FIPS without a second invite
    // hop. Only runs when the source is Trusted — Observer-level peers
    // don't get to expand our federation on their own authority.
    if peer.trust_level == TrustLevel::Trusted {
        if let Err(e) =
            merge_transitive_peers(data_dir, &peer.did, local_did, &state.federated_peers).await
        {
            tracing::warn!(
                peer_did = %peer.did,
                error = %e,
                "Transitive federation merge failed (non-fatal)"
            );
        }
    }

    Ok(state)
}

/// Convenience wrapper: look up a federated peer by DID, derive our
/// own local_did / signing context from the node identity on disk, and
/// call sync_with_peer. Used by transitive-discovery code paths where
/// the caller only knows the peer's DID (e.g. the peer-joined RPC's
/// follow-up task).
pub async fn sync_with_peer_by_did(data_dir: &Path, peer_did: &str) -> Result<NodeStateSnapshot> {
    let nodes = super::storage::load_nodes(data_dir).await?;
    let peer = nodes
        .into_iter()
        .find(|n| n.did == peer_did)
        .ok_or_else(|| anyhow::anyhow!("Unknown federation peer: {}", peer_did))?;

    let identity_dir = data_dir.join("identity");
    let node_identity = crate::identity::NodeIdentity::load_or_create(&identity_dir).await?;
    let local_pubkey_hex = node_identity.pubkey_hex();
    let local_did = crate::identity::did_key_from_pubkey_hex(&local_pubkey_hex)?;

    sync_with_peer(data_dir, &peer, &local_did, |data| node_identity.sign(data)).await
}

/// Merge peers advertised by a Trusted federated node into our own
/// federation list. New peers are added at `Trusted` — hints only
/// arrive from peers we already trust, and `build_local_state` only
/// re-exports our Trusted list, so transitive membership carries the
/// same trust the direct-invite path gives. Existing peers get their
/// `fips_npub` refreshed if we hadn't learned it yet.
///
/// Peers we are (us) or that we already track by DID are skipped.
async fn merge_transitive_peers(
    data_dir: &std::path::Path,
    source_did: &str,
    local_did: &str,
    hints: &[FederationPeerHint],
) -> Result<()> {
    if hints.is_empty() {
        return Ok(());
    }
    let mut nodes = super::storage::load_nodes(data_dir).await?;
    // Tombstoned DIDs: peers the operator explicitly removed. Never re-add
    // them via transitive discovery, or deleted (e.g. stale test) nodes
    // reappear on the next sync with any peer that still lists them.
    let removed = super::storage::load_removed_dids(data_dir)
        .await
        .unwrap_or_default();
    let mut added = 0u32;
    let mut refreshed = 0u32;

    for hint in hints {
        // Don't import the source peer advertising itself, or our own DID
        // when the source advertises us back as one of its trusted peers.
        if hint.did == source_did || hint.did == local_did {
            continue;
        }
        // Skip anything the operator deliberately removed.
        if removed.contains(&hint.did) {
            continue;
        }
        if let Some(existing) = nodes.iter_mut().find(|n| n.did == hint.did) {
            // Already known — just refresh fips_npub if we didn't have one.
            if existing.fips_npub.is_none() && hint.fips_npub.is_some() {
                existing.fips_npub = hint.fips_npub.clone();
                refreshed += 1;
            }
            continue;
        }
        // Same physical node advertised under a DIFFERENT did? Match on the
        // onion (its stable network identity). Without this, a node that
        // appears under two dids (e.g. after a key/did change) gets added
        // twice — showing up duplicated in the trusted-node list (B1) and as
        // two separate mesh chat contacts (B2). Merge into the existing entry.
        let hint_onion = hint.onion.trim_end_matches(".onion");
        if !hint_onion.is_empty() {
            if let Some(existing) = nodes
                .iter_mut()
                .find(|n| n.onion.trim_end_matches(".onion") == hint_onion)
            {
                if existing.fips_npub.is_none() && hint.fips_npub.is_some() {
                    existing.fips_npub = hint.fips_npub.clone();
                }
                if existing.name.is_none() && hint.name.is_some() {
                    existing.name = hint.name.clone();
                }
                refreshed += 1;
                continue;
            }
        }
        // TRUST IS NOT TRANSITIVE. This peer was advertised to us by a Trusted
        // source; we have no relationship with it and the operator has never
        // seen it. Granting Trusted here made trust viral: once merged at
        // Trusted, this node is itself synced with, its advertised peers are
        // merged in turn, and one federation invite anywhere in the graph
        // eventually marked the whole graph Trusted on every node.
        //
        // Observer is what the merge actually needs — the stated purpose is
        // routing ("so we can route directly to them over FIPS without a second
        // invite hop"), and Observer is reachable/syncable while being barred
        // from expanding the federation further on its own authority (the
        // guard at the call site checks for Trusted). Promotion stays an
        // operator action.
        nodes.push(FederatedNode {
            did: hint.did.clone(),
            pubkey: hint.pubkey.clone(),
            onion: hint.onion.clone(),
            name: hint.name.clone(),
            trust_level: TrustLevel::Observer,
            trust_source: Some(super::types::TrustSource::TransitiveMerge),
            added_at: chrono::Utc::now().to_rfc3339(),
            last_seen: None,
            last_state: None,
            fips_npub: hint.fips_npub.clone(),
            last_transport: None,
            last_transport_at: None,
            last_sync_error: None,
            last_sync_error_at: None,
        });
        added += 1;
    }

    if added > 0 || refreshed > 0 {
        super::storage::save_nodes(data_dir, &nodes).await?;
        tracing::info!(
            source_did = %source_did,
            added,
            refreshed,
            "Transitive federation merge complete"
        );
    }
    Ok(())
}

/// Build the local node's state snapshot for sharing with peers.
///
/// `federated_peers` should be the caller's full list of federated
/// nodes; `build_local_state` filters them down to a `FederationPeerHint`
/// so receivers can perform transitive pairing (learn peers-of-peers
/// and route directly over FIPS from now on). Only peers we trust are
/// shared — an Untrusted/Observer node should not be re-exported
/// through us to the network.
#[allow(clippy::too_many_arguments)]
pub fn build_local_state(
    apps: Vec<AppStatus>,
    cpu: Option<f64>,
    mem_used: Option<u64>,
    mem_total: Option<u64>,
    disk_used: Option<u64>,
    disk_total: Option<u64>,
    uptime: Option<u64>,
    tor_active: bool,
    server_name: Option<String>,
    nostr_npub: Option<String>,
    own_fips_npub: Option<String>,
    federated_peers: &[FederatedNode],
    // Only Some when the node has opted in via server.set-location's
    // `share` flag — see NodeStateSnapshot::lat/lon's doc comment.
    shared_location: Option<(f64, f64)>,
) -> NodeStateSnapshot {
    let hints = federated_peers
        .iter()
        .filter(|n| n.trust_level == TrustLevel::Trusted)
        .map(|n| FederationPeerHint {
            did: n.did.clone(),
            pubkey: n.pubkey.clone(),
            onion: n.onion.clone(),
            name: n.name.clone(),
            fips_npub: n.fips_npub.clone(),
        })
        .collect();

    NodeStateSnapshot {
        timestamp: chrono::Utc::now().to_rfc3339(),
        node_name: server_name,
        apps,
        cpu_usage_percent: cpu,
        mem_used_bytes: mem_used,
        mem_total_bytes: mem_total,
        disk_used_bytes: disk_used,
        disk_total_bytes: disk_total,
        uptime_secs: uptime,
        tor_active: Some(tor_active),
        nostr_npub,
        own_fips_npub,
        federated_peers: hints,
        lat: shared_location.map(|(lat, _)| lat),
        lon: shared_location.map(|(_, lon)| lon),
    }
}

/// Deploy an app to a remote federated peer over Tor.
/// Only works if the peer is trusted and the app exists in our marketplace.
pub async fn deploy_to_peer(
    peer: &FederatedNode,
    app_id: &str,
    version: &str,
    marketplace_url: &str,
    local_did: &str,
    sign_fn: impl FnOnce(&[u8]) -> String,
) -> Result<serde_json::Value> {
    if peer.trust_level != TrustLevel::Trusted {
        anyhow::bail!(
            "Can only deploy to trusted peers (current: {})",
            peer.trust_level
        );
    }

    let timestamp = chrono::Utc::now().to_rfc3339();
    let signature = sign_fn(timestamp.as_bytes());

    let body = serde_json::json!({
        "method": "package.install",
        "params": {
            "id": app_id,
            "version": version,
            "marketplace-url": marketplace_url,
        }
    });

    let (resp, transport) = PeerRequest::new(peer.fips_npub.as_deref(), &peer.onion, "/rpc/v1")
        .service(crate::settings::transport::PeerService::Federation)
        .header("X-Federation-DID", local_did)
        .header("X-Federation-Sig", signature)
        .header("X-Federation-Timestamp", timestamp)
        .timeout(std::time::Duration::from_secs(120))
        .send_json(&body)
        .await
        .context("Failed to reach federated peer for deploy")?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "Remote node returned HTTP {} (via {})",
            resp.status(),
            transport
        );
    }

    let result: serde_json::Value = resp.json().await.context("Invalid response from peer")?;

    if let Some(err) = result.get("error") {
        if !err.is_null() {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown remote error");
            anyhow::bail!("Remote node refused deploy: {}", msg);
        }
    }

    Ok(serde_json::json!({
        "deployed": true,
        "app_id": app_id,
        "peer_did": peer.did,
        "peer_onion": peer.onion,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_local_state() {
        let state = build_local_state(
            vec![AppStatus {
                id: "lnd".to_string(),
                status: "running".to_string(),
                version: Some("0.18".to_string()),
            }],
            Some(25.5),
            Some(2_000_000_000),
            Some(8_000_000_000),
            Some(100_000_000_000),
            Some(500_000_000_000),
            Some(3600),
            true,
            Some("Test Node".to_string()),
            None,
            None,
            &[],
            None,
        );
        assert_eq!(state.apps.len(), 1);
        assert_eq!(state.cpu_usage_percent, Some(25.5));
        assert_eq!(state.tor_active, Some(true));
        assert_eq!(state.node_name, Some("Test Node".to_string()));
        assert!(state.federated_peers.is_empty());
        assert_eq!(state.lat, None);
    }

    #[test]
    fn build_local_state_filters_non_trusted_peers() {
        let peers = vec![
            FederatedNode {
                trust_source: None,
                did: "did:key:zTrusted".into(),
                pubkey: "aa".into(),
                onion: "t.onion".into(),
                name: None,
                trust_level: TrustLevel::Trusted,
                added_at: "now".into(),
                last_seen: None,
                last_state: None,
                fips_npub: Some("npub1a".into()),
                last_transport: None,
                last_transport_at: None,
                last_sync_error: None,
                last_sync_error_at: None,
            },
            FederatedNode {
                trust_source: None,
                did: "did:key:zObserver".into(),
                pubkey: "bb".into(),
                onion: "o.onion".into(),
                name: None,
                trust_level: TrustLevel::Observer,
                added_at: "now".into(),
                last_seen: None,
                last_state: None,
                fips_npub: Some("npub1b".into()),
                last_transport: None,
                last_transport_at: None,
                last_sync_error: None,
                last_sync_error_at: None,
            },
            FederatedNode {
                trust_source: None,
                did: "did:key:zUntrusted".into(),
                pubkey: "cc".into(),
                onion: "u.onion".into(),
                name: None,
                trust_level: TrustLevel::Untrusted,
                added_at: "now".into(),
                last_seen: None,
                last_state: None,
                fips_npub: None,
                last_transport: None,
                last_transport_at: None,
                last_sync_error: None,
                last_sync_error_at: None,
            },
        ];
        let state = build_local_state(
            vec![],
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            None,
            None,
            None,
            &peers,
            None,
        );
        assert_eq!(state.federated_peers.len(), 1);
        assert_eq!(state.federated_peers[0].did, "did:key:zTrusted");
        assert_eq!(
            state.federated_peers[0].fips_npub.as_deref(),
            Some("npub1a")
        );
    }

    #[tokio::test]
    async fn merge_transitive_peers_skips_source_and_local_node() {
        let dir = tempfile::tempdir().unwrap();
        super::super::storage::save_nodes(
            dir.path(),
            &[FederatedNode {
                trust_source: None,
                did: "did:key:zSource".into(),
                pubkey: "aa".into(),
                onion: "source.onion".into(),
                name: Some("Source".into()),
                trust_level: TrustLevel::Trusted,
                added_at: "now".into(),
                last_seen: None,
                last_state: None,
                fips_npub: None,
                last_transport: None,
                last_transport_at: None,
                last_sync_error: None,
                last_sync_error_at: None,
            }],
        )
        .await
        .unwrap();

        merge_transitive_peers(
            dir.path(),
            "did:key:zSource",
            "did:key:zLocal",
            &[
                FederationPeerHint {
                    did: "did:key:zSource".into(),
                    pubkey: "aa".into(),
                    onion: "source.onion".into(),
                    name: Some("Source".into()),
                    fips_npub: None,
                },
                FederationPeerHint {
                    did: "did:key:zLocal".into(),
                    pubkey: "bb".into(),
                    onion: "local.onion".into(),
                    name: Some("Local".into()),
                    fips_npub: None,
                },
                FederationPeerHint {
                    did: "did:key:zPeer".into(),
                    pubkey: "cc".into(),
                    onion: "peer.onion".into(),
                    name: Some("Kitchen".into()),
                    fips_npub: Some("npub1peer".into()),
                },
            ],
        )
        .await
        .unwrap();

        let nodes = super::super::storage::load_nodes(dir.path()).await.unwrap();
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().all(|n| n.did != "did:key:zLocal"));
        let peer = nodes
            .iter()
            .find(|n| n.did == "did:key:zPeer")
            .expect("transitive peer should be added (routing needs it)");
        assert_eq!(peer.name.as_deref(), Some("Kitchen"));
        // TRUST IS NOT TRANSITIVE. This peer was advertised by a Trusted source;
        // the operator has never seen it. It is added so we can route to it, at
        // Observer — never Trusted. This assertion previously read `Trusted` and
        // was pinning the escalation in place: one invite anywhere in the graph
        // eventually marked the entire graph Trusted on every node.
        assert_eq!(
            peer.trust_level,
            TrustLevel::Observer,
            "a transitively-discovered peer must never be auto-Trusted"
        );
        assert_eq!(
            peer.trust_source,
            Some(super::super::types::TrustSource::TransitiveMerge),
            "provenance must be recorded so the operator can audit it"
        );
        assert_eq!(peer.fips_npub.as_deref(), Some("npub1peer"));
    }
}
