//! Federation persistent storage: node list and invite management on disk.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

use super::types::{FederatedNode, FederationInvite, NodeStateSnapshot, TrustLevel, TrustSource};

pub(crate) const FEDERATION_DIR: &str = "federation";
pub(crate) const NODES_FILE: &str = "nodes.json";
pub(crate) const INVITES_FILE: &str = "invites.json";
/// Tombstones: DIDs the operator explicitly removed. Kept so transitive
/// federation discovery can't silently re-add a peer they deleted.
pub(crate) const REMOVED_FILE: &str = "removed-nodes.json";

/// Top-level file structures.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct NodesFile {
    pub(crate) nodes: Vec<FederatedNode>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct RemovedFile {
    pub(crate) removed: Vec<RemovedNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemovedNode {
    pub(crate) did: String,
    pub(crate) removed_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct InvitesFile {
    pub(crate) outgoing: Vec<FederationInvite>,
    pub(crate) incoming: Vec<FederationInvite>,
}

/// Ensure federation directory exists.
pub(crate) async fn ensure_dir(data_dir: &Path) -> Result<std::path::PathBuf> {
    let dir = data_dir.join(FEDERATION_DIR);
    fs::create_dir_all(&dir)
        .await
        .context("Failed to create federation directory")?;
    Ok(dir)
}

/// Serializes every load-mutate-save cycle against `federation/nodes.json`
/// (and its paired `removed-nodes.json` tombstone file). The concrete
/// failure this prevents: a `federation.remove-node` RPC racing the 90s
/// auto-sync loop's `update_node_state` call. Without this lock, the sync
/// task's `load_nodes` snapshot — taken *before* the removal lands — could
/// finish its own save *after* the removal's save, silently re-writing the
/// peer the operator just removed back into the node list, with no error
/// logged anywhere (the "removed nodes reappear" symptom).
///
/// Acquire with `.lock().await`, never `try_lock`. Unlike `update.rs`'s
/// `UPDATE_OP_LOCK` (which deliberately rejects a concurrent caller with an
/// "already running" error — the right UX for update downloads),
/// reject-on-contention is wrong here: a rejected federation write would
/// reproduce the very lost-write symptom this lock exists to close, instead
/// of fixing it. Federation writes are infrequent, so callers queueing
/// briefly behind `.lock().await` is the correct trade-off.
static FEDERATION_STORE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// ──────────────────────────── Node Management ────────────────────────────

pub async fn load_nodes(data_dir: &Path) -> Result<Vec<FederatedNode>> {
    let _guard = FEDERATION_STORE_LOCK.lock().await;
    load_nodes_inner(data_dir).await
}

/// Lock-free body of `load_nodes`. Callers that already hold
/// `FEDERATION_STORE_LOCK` (i.e. other functions in this module composing a
/// multi-step critical section) must call this instead of `load_nodes` to
/// avoid self-deadlock — `tokio::sync::Mutex` is not re-entrant.
async fn load_nodes_inner(data_dir: &Path) -> Result<Vec<FederatedNode>> {
    let dir = data_dir.join(FEDERATION_DIR);
    let path = dir.join(NODES_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read federation nodes")?;
    let file: NodesFile = serde_json::from_str(&content).unwrap_or_default();
    Ok(dedup_nodes_by_onion(file.nodes))
}

/// Collapse entries that share an onion. An onion is a node's stable, unique
/// network identity, so two entries with the same onion are the SAME physical
/// node lingering under two dids (e.g. after a did/key change). Returning both
/// duplicates the node in the trusted-node list (B1) and the chat list (B2).
/// Keep the first occurrence and merge any missing fips_npub/name/last_state
/// from the duplicates into it, then drop them. Non-destructive to disk; the
/// deduped list persists the next time nodes are saved (add/sync).
fn dedup_nodes_by_onion(nodes: Vec<FederatedNode>) -> Vec<FederatedNode> {
    use std::collections::HashMap;
    let mut by_onion: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<FederatedNode> = Vec::with_capacity(nodes.len());
    for node in nodes {
        let key = node.onion.trim_end_matches(".onion").to_string();
        if key.is_empty() {
            out.push(node);
            continue;
        }
        if let Some(&idx) = by_onion.get(&key) {
            let kept = &mut out[idx];
            if kept.fips_npub.is_none() {
                kept.fips_npub = node.fips_npub;
            }
            if kept.name.is_none() {
                kept.name = node.name;
            }
            if kept.last_state.is_none() {
                kept.last_state = node.last_state;
            }
            continue;
        }
        by_onion.insert(key, out.len());
        out.push(node);
    }
    out
}

/// Look up a federated peer's FIPS npub given their onion address.
/// Returns `None` when the onion isn't in our federation list or the
/// peer hasn't advertised a FIPS key. Matching is suffix-tolerant so
/// callers can pass `abc` or `abc.onion` interchangeably.
pub async fn fips_npub_for_onion(data_dir: &Path, onion: &str) -> Option<String> {
    let target = onion.trim_end_matches(".onion");
    let nodes = load_nodes(data_dir).await.ok()?;
    nodes
        .iter()
        .find(|n| n.onion.trim_end_matches(".onion") == target)
        .and_then(|n| n.fips_npub.clone())
}

/// Record the transport used on the most recent successful peer reach.
/// Used for the "FIPS"/"Tor" badge on each node card in the UI — we write
/// what we actually used, not what was predicted.
///
/// Matches by DID first (precise) and falls back to onion (when the
/// caller didn't carry the DID through). No-op if the peer isn't in
/// our federation list.
pub async fn record_peer_transport(
    data_dir: &Path,
    did: Option<&str>,
    onion: Option<&str>,
    transport: &str,
) -> Result<()> {
    let _guard = FEDERATION_STORE_LOCK.lock().await;
    let mut nodes = load_nodes_inner(data_dir).await?;
    let now = chrono::Utc::now().to_rfc3339();
    let onion_target = onion.map(|o| o.trim_end_matches(".onion"));

    let mut modified = false;
    for node in nodes.iter_mut() {
        let did_match = did.is_some_and(|d| d == node.did);
        let onion_match = onion_target.is_some_and(|t| node.onion.trim_end_matches(".onion") == t);
        if did_match || onion_match {
            node.last_transport = Some(transport.to_string());
            node.last_transport_at = Some(now.clone());
            node.last_seen = Some(now.clone());
            modified = true;
            break;
        }
    }

    if modified {
        save_nodes_inner(data_dir, &nodes).await?;
    }
    Ok(())
}

/// Upper bound on a persisted `last_sync_error` message, in characters.
///
/// T-01-18: the periodic loop records an outcome for every peer on every
/// pass, so an unbounded error string (a peer echoing a huge body, a deep
/// `anyhow` chain) would be rewritten into `nodes.json` every 90 seconds.
/// Counted in `char`s, not bytes, so truncation can never split a UTF-8
/// sequence and produce a file that fails to deserialize.
pub(crate) const MAX_SYNC_ERROR_CHARS: usize = 256;

/// Record the outcome of the most recent federation sync attempt with a peer.
///
/// `Err(msg)` stores the message (truncated to `MAX_SYNC_ERROR_CHARS`) plus
/// the current time; `Ok(())` clears both fields so a recovered peer stops
/// showing a stale error badge. Only the named DID is touched.
///
/// Why this exists: both periodic sync loops previously logged failures at
/// `debug!` and nothing else, so a peer that had not synced in days looked
/// identical in the UI to one that synced a minute ago (FED-02).
///
/// A DID that isn't in the node list is a silent `Ok` and writes nothing —
/// a peer the operator removed while a sync was in flight must not be
/// resurrected by that sync's error write. This function never creates a
/// node entry.
pub async fn record_sync_result(
    data_dir: &Path,
    did: &str,
    outcome: Result<(), String>,
) -> Result<()> {
    let _guard = FEDERATION_STORE_LOCK.lock().await;
    let mut nodes = load_nodes_inner(data_dir).await?;

    let Some(node) = nodes.iter_mut().find(|n| n.did == did) else {
        // Unknown/removed peer: nothing to record against. Not an error.
        return Ok(());
    };

    let changed = match outcome {
        Err(msg) => {
            let truncated: String = msg.chars().take(MAX_SYNC_ERROR_CHARS).collect();
            node.last_sync_error = Some(truncated);
            node.last_sync_error_at = Some(chrono::Utc::now().to_rfc3339());
            true
        }
        Ok(()) => {
            // Peer recovered — drop the badge rather than leaving a stale one.
            let had_error = node.last_sync_error.is_some() || node.last_sync_error_at.is_some();
            node.last_sync_error = None;
            node.last_sync_error_at = None;
            had_error
        }
    };

    // A healthy peer stays healthy on most passes, and the surviving loop
    // calls this for every peer every 90s. Skipping the write when nothing
    // actually changed keeps the steady state read-only, so this failure
    // surfacing doesn't add a rewrite of nodes.json (and lock contention
    // with `federation.remove-node`) every single pass.
    if changed {
        save_nodes_inner(data_dir, &nodes).await?;
    }
    Ok(())
}

pub async fn save_nodes(data_dir: &Path, nodes: &[FederatedNode]) -> Result<()> {
    let _guard = FEDERATION_STORE_LOCK.lock().await;
    save_nodes_inner(data_dir, nodes).await
}

/// Lock-free body of `save_nodes`. See `load_nodes_inner` for why callers
/// that already hold `FEDERATION_STORE_LOCK` must use this instead.
///
/// Writes atomically: serialize to a sibling `.tmp` file in the same
/// directory, then `rename` it onto the real path. Same-directory rename is
/// required — it's atomic on the same filesystem (a crash mid-write leaves
/// either the old complete file or the new complete file, never a partial
/// one); a cross-filesystem rename would not have this guarantee.
async fn save_nodes_inner(data_dir: &Path, nodes: &[FederatedNode]) -> Result<()> {
    let dir = ensure_dir(data_dir).await?;
    let file = NodesFile {
        nodes: nodes.to_vec(),
    };
    let content = serde_json::to_string_pretty(&file).context("Failed to serialize nodes")?;
    let final_path = dir.join(NODES_FILE);
    let tmp_path = dir.join(format!("{NODES_FILE}.tmp"));
    fs::write(&tmp_path, content)
        .await
        .context("Failed to write federation nodes")?;
    fs::rename(&tmp_path, &final_path)
        .await
        .context("Failed to write federation nodes")?;
    Ok(())
}

pub async fn add_node(data_dir: &Path, node: FederatedNode) -> Result<Vec<FederatedNode>> {
    let _guard = FEDERATION_STORE_LOCK.lock().await;
    let mut nodes = load_nodes_inner(data_dir).await?;
    let exists = nodes.iter().any(|n| n.did == node.did);
    if exists {
        anyhow::bail!("Node with DID {} is already federated", node.did);
    }
    // Explicitly (re-)adding a node clears any prior tombstone so the
    // operator can intentionally bring back a previously removed peer.
    // Propagate failure BEFORE mutating the node list: with the tombstone
    // still in place, sync reconciliation would silently re-remove the
    // node the operator just added.
    untombstone_did_inner(data_dir, &node.did)
        .await
        .context("clear removal tombstone")?;
    nodes.push(node);
    save_nodes_inner(data_dir, &nodes).await?;
    Ok(nodes)
}

pub async fn remove_node(data_dir: &Path, did: &str) -> Result<Vec<FederatedNode>> {
    let _guard = FEDERATION_STORE_LOCK.lock().await;
    let mut nodes = load_nodes_inner(data_dir).await?;
    let before = nodes.len();
    nodes.retain(|n| n.did != did);
    if nodes.len() == before {
        anyhow::bail!("No federated node with DID {}", did);
    }
    // Tombstone the DID so transitive federation discovery (a still-federated
    // peer advertising this DID as one of *its* trusted peers) can't silently
    // re-add it. Tombstone FIRST and propagate failure: a remove whose
    // tombstone never landed isn't a remove — the peer would quietly
    // reappear after the next sync. Tombstoning is idempotent, so if the
    // node-list save below fails the operator's retry works cleanly.
    tombstone_did_inner(data_dir, did)
        .await
        .context("persist removal tombstone")?;
    save_nodes_inner(data_dir, &nodes).await?;
    Ok(nodes)
}

/// Load the set of tombstoned (operator-removed) DIDs.
pub async fn load_removed_dids(data_dir: &Path) -> Result<std::collections::HashSet<String>> {
    let path = data_dir.join(FEDERATION_DIR).join(REMOVED_FILE);
    if !path.exists() {
        return Ok(std::collections::HashSet::new());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read removed nodes")?;
    let file: RemovedFile = serde_json::from_str(&content).unwrap_or_default();
    Ok(file.removed.into_iter().map(|r| r.did).collect())
}

/// Record a DID as removed. Idempotent.
///
/// `remove_node` now calls `tombstone_did_inner` directly (to keep the
/// tombstone write and the node-list save in one critical section), so this
/// locked outer wrapper currently has no in-crate caller. Kept `pub` and
/// `#[allow(dead_code)]` rather than removed: it's part of this module's
/// documented public surface (see PLAN.md's "public signatures unchanged"
/// contract) for any future direct caller that needs a standalone,
/// correctly-locked tombstone write.
#[allow(dead_code)]
pub async fn tombstone_did(data_dir: &Path, did: &str) -> Result<()> {
    let _guard = FEDERATION_STORE_LOCK.lock().await;
    tombstone_did_inner(data_dir, did).await
}

/// Lock-free body of `tombstone_did`. See `load_nodes_inner` for why
/// callers that already hold `FEDERATION_STORE_LOCK` must use this instead.
async fn tombstone_did_inner(data_dir: &Path, did: &str) -> Result<()> {
    let dir = ensure_dir(data_dir).await?;
    let path = dir.join(REMOVED_FILE);
    let mut file: RemovedFile = if path.exists() {
        serde_json::from_str(&fs::read_to_string(&path).await.unwrap_or_default())
            .unwrap_or_default()
    } else {
        RemovedFile::default()
    };
    if !file.removed.iter().any(|r| r.did == did) {
        file.removed.push(RemovedNode {
            did: did.to_string(),
            removed_at: chrono::Utc::now().to_rfc3339(),
        });
        let content = serde_json::to_string_pretty(&file).context("serialize removed nodes")?;
        fs::write(&path, content)
            .await
            .context("Failed to write removed nodes")?;
    }
    Ok(())
}

/// Clear a DID's tombstone (operator explicitly re-added it).
///
/// `add_node` now calls `untombstone_did_inner` directly for the same
/// single-critical-section reason as `tombstone_did` above; see that
/// doc comment.
#[allow(dead_code)]
pub async fn untombstone_did(data_dir: &Path, did: &str) -> Result<()> {
    let _guard = FEDERATION_STORE_LOCK.lock().await;
    untombstone_did_inner(data_dir, did).await
}

/// Lock-free body of `untombstone_did`. See `load_nodes_inner` for why
/// callers that already hold `FEDERATION_STORE_LOCK` must use this instead.
async fn untombstone_did_inner(data_dir: &Path, did: &str) -> Result<()> {
    let path = data_dir.join(FEDERATION_DIR).join(REMOVED_FILE);
    if !path.exists() {
        return Ok(());
    }
    let mut file: RemovedFile =
        serde_json::from_str(&fs::read_to_string(&path).await.unwrap_or_default())
            .unwrap_or_default();
    let before = file.removed.len();
    file.removed.retain(|r| r.did != did);
    if file.removed.len() != before {
        let content = serde_json::to_string_pretty(&file).context("serialize removed nodes")?;
        fs::write(&path, content)
            .await
            .context("Failed to write removed nodes")?;
    }
    Ok(())
}

/// Change a federated node's trust level, optionally recording HOW the change
/// came about.
///
/// `source` is `Some(TrustSource::Manual)` on the operator RPC path so an
/// audit of `trust_source` can tell a deliberate grant apart from the levels
/// the automatic paths assign. Pass `None` for automatic adjustments that are
/// not operator decisions (e.g. the discovery-handshake demotion safety net) —
/// those must leave the recorded provenance alone rather than claim one.
pub async fn set_trust_level(
    data_dir: &Path,
    did: &str,
    trust: TrustLevel,
    source: Option<TrustSource>,
) -> Result<Vec<FederatedNode>> {
    let _guard = FEDERATION_STORE_LOCK.lock().await;
    let mut nodes = load_nodes_inner(data_dir).await?;
    let node = nodes
        .iter_mut()
        .find(|n| n.did == did)
        .ok_or_else(|| anyhow::anyhow!("No federated node with DID {}", did))?;
    node.trust_level = trust;
    if let Some(source) = source {
        node.trust_source = Some(source);
    }
    save_nodes_inner(data_dir, &nodes).await?;
    Ok(nodes)
}

/// Update a federated node's metadata (onion, pubkey, name, last_seen).
pub async fn update_node(data_dir: &Path, updated: &FederatedNode) -> Result<()> {
    let _guard = FEDERATION_STORE_LOCK.lock().await;
    let mut nodes = load_nodes_inner(data_dir).await?;
    if let Some(node) = nodes.iter_mut().find(|n| n.did == updated.did) {
        if !updated.onion.is_empty() {
            node.onion = updated.onion.clone();
        }
        if !updated.pubkey.is_empty() {
            node.pubkey = updated.pubkey.clone();
        }
        if updated.name.is_some() {
            node.name = updated.name.clone();
        }
        if updated.last_seen.is_some() {
            node.last_seen = updated.last_seen.clone();
        }
        save_nodes_inner(data_dir, &nodes).await?;
    }
    Ok(())
}

pub async fn update_node_state(data_dir: &Path, did: &str, state: NodeStateSnapshot) -> Result<()> {
    let _guard = FEDERATION_STORE_LOCK.lock().await;
    let mut nodes = load_nodes_inner(data_dir).await?;
    if let Some(node) = nodes.iter_mut().find(|n| n.did == did) {
        node.last_seen = Some(state.timestamp.clone());
        // Update node name from sync if provided (peer announced their name)
        if let Some(ref name) = state.node_name {
            if !name.is_empty() {
                node.name = Some(name.clone());
            }
        }
        // Learn the peer's FIPS npub from their state snapshot so
        // federations established before v1.4 (pre-fips_npub) start
        // routing over FIPS on the very next sync. Refresh if the peer
        // rotated their FIPS key, too.
        if let Some(ref npub) = state.own_fips_npub {
            if !npub.is_empty() && node.fips_npub.as_deref().map(str::trim) != Some(npub.trim()) {
                node.fips_npub = Some(npub.clone());
            }
        }
        node.last_state = Some(state);
        save_nodes_inner(data_dir, &nodes).await?;
    }
    Ok(())
}

// ──────────────────────────── Invite Storage ────────────────────────────

pub(crate) async fn load_invites(data_dir: &Path) -> Result<InvitesFile> {
    let dir = data_dir.join(FEDERATION_DIR);
    let path = dir.join(INVITES_FILE);
    if !path.exists() {
        return Ok(InvitesFile::default());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read invites")?;
    let file: InvitesFile = serde_json::from_str(&content).unwrap_or_default();
    Ok(file)
}

pub(crate) async fn save_invites(data_dir: &Path, invites: &InvitesFile) -> Result<()> {
    let dir = ensure_dir(data_dir).await?;
    let content = serde_json::to_string_pretty(invites).context("Failed to serialize invites")?;
    fs::write(dir.join(INVITES_FILE), content)
        .await
        .context("Failed to write invites")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::federation::types::AppStatus;

    fn make_node(did: &str, onion: &str) -> FederatedNode {
        FederatedNode {
            trust_source: None,
            did: did.to_string(),
            pubkey: "aabbccdd".to_string(),
            onion: onion.to_string(),
            name: None,
            trust_level: TrustLevel::Trusted,
            added_at: "2026-01-01T00:00:00Z".to_string(),
            last_seen: None,
            last_state: None,
            fips_npub: None,
            last_transport: None,
            last_transport_at: None,
            last_sync_error: None,
            last_sync_error_at: None,
        }
    }

    #[test]
    fn test_dedup_nodes_by_onion_collapses_same_onion() {
        // Two entries share an onion (same physical node under two dids) — must
        // collapse to one, keeping the first did and merging fips_npub/name (B1/B2).
        let mut dup = make_node("did:key:zDUP", "shared.onion");
        dup.fips_npub = Some("npub1merged".to_string());
        dup.name = Some("Sapien".to_string());
        let nodes = vec![
            make_node("did:key:zKEEP", "shared.onion"),
            dup,
            make_node("did:key:zOTHER", "other.onion"),
        ];
        let out = dedup_nodes_by_onion(nodes);
        assert_eq!(out.len(), 2, "two distinct onions remain");
        let kept = out.iter().find(|n| n.onion == "shared.onion").unwrap();
        assert_eq!(kept.did, "did:key:zKEEP", "keeps first did for the onion");
        assert_eq!(
            kept.fips_npub.as_deref(),
            Some("npub1merged"),
            "merges fips_npub from the dropped duplicate"
        );
        assert_eq!(
            kept.name.as_deref(),
            Some("Sapien"),
            "merges name from the dup"
        );
    }

    #[test]
    fn test_dedup_onion_suffix_insensitive() {
        // The ".onion" suffix must not affect the match.
        let nodes = vec![
            make_node("did:key:z1", "abc"),
            make_node("did:key:z2", "abc.onion"),
        ];
        assert_eq!(dedup_nodes_by_onion(nodes).len(), 1);
    }

    #[tokio::test]
    async fn test_load_nodes_empty_when_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let nodes = load_nodes(dir.path()).await.unwrap();
        assert!(nodes.is_empty());
    }

    #[tokio::test]
    async fn test_save_and_load_nodes_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let nodes = vec![
            make_node("did:key:z1", "a.onion"),
            make_node("did:key:z2", "b.onion"),
        ];
        save_nodes(dir.path(), &nodes).await.unwrap();
        let loaded = load_nodes(dir.path()).await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].did, "did:key:z1");
    }

    #[tokio::test]
    async fn test_add_node_deduplicates_by_did() {
        let dir = tempfile::tempdir().unwrap();
        add_node(dir.path(), make_node("did:key:z1", "a.onion"))
            .await
            .unwrap();
        let result = add_node(dir.path(), make_node("did:key:z1", "b.onion")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_remove_node_by_did() {
        let dir = tempfile::tempdir().unwrap();
        add_node(dir.path(), make_node("did:key:z1", "a.onion"))
            .await
            .unwrap();
        add_node(dir.path(), make_node("did:key:z2", "b.onion"))
            .await
            .unwrap();
        let result = remove_node(dir.path(), "did:key:z1").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].did, "did:key:z2");
    }

    #[tokio::test]
    async fn test_remove_nonexistent_node_errors() {
        let dir = tempfile::tempdir().unwrap();
        let result = remove_node(dir.path(), "did:key:nonexistent").await;
        assert!(result.is_err());
    }

    /// FED-01 failure-surfacing edge: a removal whose tombstone write fails
    /// must return `Err` to the caller instead of reporting success, and
    /// must NOT half-apply — the node list must be left untouched. Forces
    /// the failure by pre-creating the removed-nodes path as a directory: a
    /// directory can't be replaced by `fs::write`, so `tombstone_did_inner`
    /// errors before `remove_node`'s node-list save ever runs (tombstone is
    /// written first, per `remove_node`'s documented ordering).
    #[tokio::test]
    async fn test_remove_errors_when_tombstone_write_fails() {
        let dir = tempfile::tempdir().unwrap();
        add_node(dir.path(), make_node("did:key:z1", "a.onion"))
            .await
            .unwrap();

        let federation_dir = dir.path().join(FEDERATION_DIR);
        std::fs::create_dir_all(&federation_dir).unwrap();
        std::fs::create_dir_all(federation_dir.join(REMOVED_FILE)).unwrap();

        let result = remove_node(dir.path(), "did:key:z1").await;
        assert!(
            result.is_err(),
            "a failed tombstone write must surface as an error, not a silent no-op"
        );

        let nodes = load_nodes(dir.path()).await.unwrap();
        assert!(
            nodes.iter().any(|n| n.did == "did:key:z1"),
            "a removal whose tombstone never landed must not half-apply — \
             the node list must remain untouched"
        );
    }

    #[tokio::test]
    async fn test_remove_tombstones_and_readd_clears_it() {
        let dir = tempfile::tempdir().unwrap();
        add_node(dir.path(), make_node("did:key:z1", "a.onion"))
            .await
            .unwrap();
        // No tombstones yet.
        assert!(load_removed_dids(dir.path()).await.unwrap().is_empty());

        // Removing tombstones the DID so transitive discovery won't re-add it.
        remove_node(dir.path(), "did:key:z1").await.unwrap();
        let removed = load_removed_dids(dir.path()).await.unwrap();
        assert!(
            removed.contains("did:key:z1"),
            "removed DID must be tombstoned"
        );

        // Explicitly re-adding clears the tombstone (intentional re-federate).
        add_node(dir.path(), make_node("did:key:z1", "a.onion"))
            .await
            .unwrap();
        assert!(
            !load_removed_dids(dir.path())
                .await
                .unwrap()
                .contains("did:key:z1"),
            "explicit re-add must clear the tombstone"
        );
    }

    #[tokio::test]
    async fn test_set_trust_level() {
        let dir = tempfile::tempdir().unwrap();
        add_node(dir.path(), make_node("did:key:z1", "a.onion"))
            .await
            .unwrap();
        let nodes = set_trust_level(dir.path(), "did:key:z1", TrustLevel::Observer, None)
            .await
            .unwrap();
        assert_eq!(nodes[0].trust_level, TrustLevel::Observer);
    }

    /// The operator RPC path stamps `Manual`, so an audit of `trust_source`
    /// can separate a deliberate grant from the levels the automatic paths
    /// (`UninvitedJoin`, `TransitiveMerge`) assign on their own authority.
    #[tokio::test]
    async fn test_set_trust_level_records_manual_source() {
        let dir = tempfile::tempdir().unwrap();
        add_node(dir.path(), make_node("did:key:z1", "a.onion"))
            .await
            .unwrap();
        let nodes = set_trust_level(
            dir.path(),
            "did:key:z1",
            TrustLevel::Trusted,
            Some(TrustSource::Manual),
        )
        .await
        .unwrap();
        assert_eq!(nodes[0].trust_level, TrustLevel::Trusted);
        assert_eq!(nodes[0].trust_source, Some(TrustSource::Manual));
    }

    /// An automatic adjustment must not claim a provenance it doesn't have:
    /// passing `None` leaves whatever was recorded before intact, so the
    /// discovery-handshake demotion can't launder an `UninvitedJoin` peer
    /// into looking operator-approved.
    #[tokio::test]
    async fn test_set_trust_level_none_source_preserves_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let mut node = make_node("did:key:z1", "a.onion");
        node.trust_source = Some(TrustSource::UninvitedJoin);
        add_node(dir.path(), node).await.unwrap();

        let nodes = set_trust_level(dir.path(), "did:key:z1", TrustLevel::Observer, None)
            .await
            .unwrap();
        assert_eq!(nodes[0].trust_level, TrustLevel::Observer);
        assert_eq!(
            nodes[0].trust_source,
            Some(TrustSource::UninvitedJoin),
            "an automatic level change must not rewrite how the peer got here"
        );
    }

    /// The .198 v1.7.103 update-bricking race (see `update.rs`'s
    /// `UPDATE_OP_LOCK`) had the same shape as this test: two concurrent
    /// mutators sharing one on-disk file with no coordination. Here,
    /// `add_node` and `set_trust_level` both do their own load-mutate-save
    /// cycle against `nodes.json`; without `FEDERATION_STORE_LOCK` held for
    /// the whole cycle, whichever writer's `save_nodes` lands second wins
    /// and clobbers the other writer's update entirely.
    ///
    /// Uses real `tokio::spawn` tasks (not just `tokio::join!` polled within
    /// one task) so the two writers are genuinely scheduled across the
    /// multi-thread runtime's worker pool, and loops so OS scheduling
    /// jitter gets many chances to interleave the two load-mutate-save
    /// cycles.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_writes_do_not_lose_updates() {
        for i in 0..40 {
            let dir = tempfile::tempdir().unwrap();
            add_node(dir.path(), make_node("did:key:zA", "a.onion"))
                .await
                .unwrap();

            let dir_a = dir.path().to_path_buf();
            let dir_b = dir.path().to_path_buf();
            let add_task =
                tokio::spawn(
                    async move { add_node(&dir_a, make_node("did:key:zB", "b.onion")).await },
                );
            let trust_task = tokio::spawn(async move {
                set_trust_level(&dir_b, "did:key:zA", TrustLevel::Observer, None).await
            });
            add_task.await.unwrap().unwrap();
            trust_task.await.unwrap().unwrap();

            let nodes = load_nodes(dir.path()).await.unwrap();
            assert_eq!(
                nodes.len(),
                2,
                "iteration {i}: both concurrent writes must persist — neither may be lost"
            );
            let node_a = nodes
                .iter()
                .find(|n| n.did == "did:key:zA")
                .unwrap_or_else(|| panic!("iteration {i}: did:key:zA must still be present"));
            assert_eq!(
                node_a.trust_level,
                TrustLevel::Observer,
                "iteration {i}: the concurrent set_trust_level write must not be lost"
            );
        }
    }

    /// Reproduces the FED-01 "removed nodes reappear" symptom: a
    /// `federation.remove-node` RPC racing the 90s auto-sync loop's
    /// `update_node_state` call. The sync task's `load_nodes` snapshot,
    /// taken before the removal lands, must never be allowed to re-save
    /// the peer the operator just removed.
    ///
    /// `remove_node`'s critical path does one more disk hop than
    /// `update_node_state` (the tombstone write), which structurally
    /// biases a single 1-vs-1 race toward the *safe* ordering (remove's
    /// save landing last). To reliably exercise the unsafe ordering this
    /// test races `remove_node` against a BURST of concurrent
    /// `update_node_state` calls, each a genuine `tokio::spawn`ed task, so
    /// OS scheduling jitter has many independent chances per iteration to
    /// land at least one sync save after the removal's save. Looped so a
    /// single unlucky iteration isn't required to catch it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_remove_survives_concurrent_state_sync() {
        const SYNC_RACERS: usize = 12;
        for i in 0..30 {
            let dir = tempfile::tempdir().unwrap();
            add_node(dir.path(), make_node("did:key:zA", "a.onion"))
                .await
                .unwrap();
            add_node(dir.path(), make_node("did:key:zB", "b.onion"))
                .await
                .unwrap();

            let mut sync_tasks = Vec::with_capacity(SYNC_RACERS);
            for _ in 0..SYNC_RACERS {
                let dir_sync = dir.path().to_path_buf();
                let state = NodeStateSnapshot {
                    timestamp: "2026-03-10T12:00:00Z".to_string(),
                    node_name: None,
                    apps: Vec::new(),
                    cpu_usage_percent: None,
                    mem_used_bytes: None,
                    mem_total_bytes: None,
                    disk_used_bytes: None,
                    disk_total_bytes: None,
                    uptime_secs: None,
                    tor_active: None,
                    nostr_npub: None,
                    own_fips_npub: None,
                    federated_peers: Vec::new(),
                    lat: None,
                    lon: None,
                };
                sync_tasks.push(tokio::spawn(async move {
                    update_node_state(&dir_sync, "did:key:zA", state).await
                }));
            }
            let dir_remove = dir.path().to_path_buf();
            let remove_task =
                tokio::spawn(async move { remove_node(&dir_remove, "did:key:zA").await });

            for task in sync_tasks {
                task.await.unwrap().unwrap();
            }
            remove_task.await.unwrap().unwrap();

            let nodes = load_nodes(dir.path()).await.unwrap();
            assert!(
                !nodes.iter().any(|n| n.did == "did:key:zA"),
                "iteration {i}: removed node reappeared after a concurrent sync"
            );
            let removed = load_removed_dids(dir.path()).await.unwrap();
            assert!(
                removed.contains("did:key:zA"),
                "iteration {i}: removed DID must remain tombstoned"
            );
        }
    }

    /// FED-01 empty edge: removing the last federated node must succeed
    /// cleanly, not error out on an "empty list" special case.
    #[tokio::test]
    async fn test_remove_last_node_leaves_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        add_node(dir.path(), make_node("did:key:zOnly", "only.onion"))
            .await
            .unwrap();

        let result = remove_node(dir.path(), "did:key:zOnly").await.unwrap();
        assert!(result.is_empty(), "returned Vec must be empty");

        let nodes = load_nodes(dir.path()).await.unwrap();
        assert!(
            nodes.is_empty(),
            "load_nodes must return an empty Vec, not an error"
        );

        let removed = load_removed_dids(dir.path()).await.unwrap();
        assert!(
            removed.contains("did:key:zOnly"),
            "the sole removed node must still be tombstoned"
        );
    }

    /// FED-02: a failed sync must leave a durable, per-peer record instead of
    /// only a `debug!` line, so the operator can tell a peer that hasn't
    /// synced in days from one that synced a minute ago.
    #[tokio::test]
    async fn test_record_sync_result_persists_error() {
        let dir = tempfile::tempdir().unwrap();
        add_node(dir.path(), make_node("did:key:z1", "a.onion"))
            .await
            .unwrap();
        add_node(dir.path(), make_node("did:key:z2", "b.onion"))
            .await
            .unwrap();

        record_sync_result(
            dir.path(),
            "did:key:z1",
            Err("peer unreachable".to_string()),
        )
        .await
        .unwrap();

        let nodes = load_nodes(dir.path()).await.unwrap();
        let n1 = nodes.iter().find(|n| n.did == "did:key:z1").unwrap();
        assert_eq!(n1.last_sync_error.as_deref(), Some("peer unreachable"));
        assert!(
            n1.last_sync_error_at.is_some(),
            "an error must carry the time it happened"
        );

        let n2 = nodes.iter().find(|n| n.did == "did:key:z2").unwrap();
        assert!(
            n2.last_sync_error.is_none(),
            "only the failing peer may be marked"
        );
    }

    /// FED-02 adjacency edge: the badge must not outlive the failure. A
    /// successful sync clears the previously recorded error for that peer.
    #[tokio::test]
    async fn test_record_sync_result_success_clears_error() {
        let dir = tempfile::tempdir().unwrap();
        add_node(dir.path(), make_node("did:key:z1", "a.onion"))
            .await
            .unwrap();

        record_sync_result(dir.path(), "did:key:z1", Err("timed out".to_string()))
            .await
            .unwrap();
        assert!(load_nodes(dir.path()).await.unwrap()[0]
            .last_sync_error
            .is_some());

        record_sync_result(dir.path(), "did:key:z1", Ok(()))
            .await
            .unwrap();

        let nodes = load_nodes(dir.path()).await.unwrap();
        assert!(
            nodes[0].last_sync_error.is_none(),
            "a recovered peer must not keep its stale error badge"
        );
        assert!(
            nodes[0].last_sync_error_at.is_none(),
            "the error timestamp must clear with the error"
        );
    }

    /// FED-02: a peer removed mid-pass must not be resurrected by the
    /// in-flight sync attempt's error write. Missing DID is a silent Ok.
    #[tokio::test]
    async fn test_record_sync_result_missing_did_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        add_node(dir.path(), make_node("did:key:z1", "a.onion"))
            .await
            .unwrap();

        record_sync_result(dir.path(), "did:key:zGONE", Err("unreachable".to_string()))
            .await
            .expect("recording against an unknown DID must be a silent Ok, not an error");

        let nodes = load_nodes(dir.path()).await.unwrap();
        assert_eq!(nodes.len(), 1, "a removed peer must not be resurrected");
        assert_eq!(nodes[0].did, "did:key:z1");
        assert!(nodes[0].last_sync_error.is_none());
    }

    /// FED-02 empty edge: recording against an empty node store writes
    /// nothing and errors on nobody (the zero-federated-node sync pass).
    #[tokio::test]
    async fn test_record_sync_result_on_empty_store_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        record_sync_result(dir.path(), "did:key:zAny", Err("boom".to_string()))
            .await
            .unwrap();
        assert!(load_nodes(dir.path()).await.unwrap().is_empty());
        assert!(
            !dir.path().join(FEDERATION_DIR).join(NODES_FILE).exists(),
            "a no-op must not create the node file"
        );
    }

    /// T-01-18: an unbounded error string must not bloat nodes.json on every
    /// failed pass. The recorded message is truncated before persistence.
    #[tokio::test]
    async fn test_record_sync_result_truncates_long_error() {
        let dir = tempfile::tempdir().unwrap();
        add_node(dir.path(), make_node("did:key:z1", "a.onion"))
            .await
            .unwrap();

        let huge = "x".repeat(5000);
        record_sync_result(dir.path(), "did:key:z1", Err(huge))
            .await
            .unwrap();

        let nodes = load_nodes(dir.path()).await.unwrap();
        let msg = nodes[0].last_sync_error.as_deref().unwrap();
        assert!(
            msg.chars().count() <= MAX_SYNC_ERROR_CHARS,
            "recorded error must be truncated to {MAX_SYNC_ERROR_CHARS} chars, got {}",
            msg.chars().count()
        );
    }

    #[tokio::test]
    async fn test_update_node_state() {
        let dir = tempfile::tempdir().unwrap();
        add_node(dir.path(), make_node("did:key:z1", "a.onion"))
            .await
            .unwrap();

        let state = NodeStateSnapshot {
            timestamp: "2026-03-10T12:00:00Z".to_string(),
            node_name: None,
            apps: vec![AppStatus {
                id: "bitcoin".to_string(),
                status: "running".to_string(),
                version: Some("27.0".to_string()),
            }],
            cpu_usage_percent: Some(45.2),
            mem_used_bytes: Some(4_000_000_000),
            mem_total_bytes: Some(8_000_000_000),
            disk_used_bytes: None,
            disk_total_bytes: None,
            uptime_secs: Some(86400),
            tor_active: Some(true),
            nostr_npub: None,
            own_fips_npub: None,
            federated_peers: Vec::new(),
            lat: None,
            lon: None,
        };

        update_node_state(dir.path(), "did:key:z1", state)
            .await
            .unwrap();

        let nodes = load_nodes(dir.path()).await.unwrap();
        assert!(nodes[0].last_seen.is_some());
        let ls = nodes[0].last_state.as_ref().unwrap();
        assert_eq!(ls.apps.len(), 1);
        assert_eq!(ls.cpu_usage_percent, Some(45.2));
    }
}
