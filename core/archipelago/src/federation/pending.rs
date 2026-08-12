//! Pending peer-discovery requests received over Nostr.
//!
//! When another node discovers us via Nostr presence and sends an encrypted
//! `PeerRequest` (NIP-44 DM), we store the request here instead of acting
//! on it. The user explicitly approves or rejects each request via the
//! Federation UI; only on approval do we generate a federation invite code
//! and ship it back over the same encrypted channel.
//!
//! Nothing in this module ever exposes the local onion address. The onion
//! is only added to the wire later, by the approval handler, and only
//! inside a NIP-44 ciphertext addressed to the requester's nostr pubkey.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

const PENDING_FILE: &str = "federation/pending_requests.json";
const MAX_PENDING_PER_PUBKEY: usize = 5;
const PENDING_EXPIRY_DAYS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PendingState {
    /// Inbound: a remote node sent us a peer request, awaiting local approval.
    Pending,
    /// Outbound: we sent a peer request, awaiting their approval (and the
    /// invite code they will send back via NIP-44 if they accept).
    Sent,
    /// Approved locally — the inbound request has been turned into a federation
    /// invite that has been shipped back to the requester. Kept as history.
    Approved,
    /// Rejected locally. Kept as history so the same npub can't immediately
    /// re-request without the user noticing.
    Rejected,
    /// Auto-expired after `PENDING_EXPIRY_DAYS` with no action.
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPeerRequest {
    /// UUID — stable identifier the FE refers to when approving/rejecting.
    pub id: String,
    /// Sender's Nostr secp256k1 pubkey (hex). Authoritative for routing
    /// the encrypted NIP-44 reply on approval.
    pub from_nostr_pubkey: String,
    /// Sender's Nostr pubkey in bech32 npub format (display only).
    pub from_nostr_npub: String,
    /// Sender's claimed archipelago DID. Verified at *approval* time
    /// (when their onion arrives via federation.peer-joined), not now —
    /// the requester could lie here, but the worst case is a wasted
    /// approval slot.
    pub from_did: String,
    /// Optional friendly name the requester typed.
    pub from_name: Option<String>,
    /// Optional one-line message the requester attached.
    pub message: Option<String>,
    pub received_at: String,
    pub state: PendingState,
    /// True if this row represents an outbound request we sent (`Sent`)
    /// rather than an inbound one we received (`Pending`).
    #[serde(default)]
    pub outbound: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PendingRequestsFile {
    pub requests: Vec<PendingPeerRequest>,
}

pub async fn load_pending(data_dir: &Path) -> Result<Vec<PendingPeerRequest>> {
    let path = data_dir.join(PENDING_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read pending requests file")?;
    let file: PendingRequestsFile = serde_json::from_str(&content).unwrap_or_default();
    Ok(file.requests)
}

pub async fn save_pending(data_dir: &Path, requests: &[PendingPeerRequest]) -> Result<()> {
    let path = data_dir.join(PENDING_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .context("Failed to create federation dir")?;
    }
    let file = PendingRequestsFile {
        requests: requests.to_vec(),
    };
    let content =
        serde_json::to_string_pretty(&file).context("Failed to serialize pending requests")?;
    fs::write(&path, content)
        .await
        .context("Failed to write pending requests file")?;
    Ok(())
}

/// Sweep auto-expired entries. Returns the cleaned list, mutated in place.
fn expire_stale(requests: &mut Vec<PendingPeerRequest>) {
    let cutoff = chrono::Utc::now() - chrono::Duration::days(PENDING_EXPIRY_DAYS);
    for r in requests.iter_mut() {
        if !matches!(r.state, PendingState::Pending | PendingState::Sent) {
            continue;
        }
        if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&r.received_at) {
            if ts.with_timezone(&chrono::Utc) < cutoff {
                r.state = PendingState::Expired;
            }
        }
    }
}

/// Insert a new inbound peer request. Returns the stored row (with id),
/// or `None` if the request was deduplicated or rate-limited.
///
/// Dedup rule: if the same (from_nostr_pubkey, from_did) already has a
/// `Pending` OR `Approved` entry, do not insert a second one. Including
/// `Approved` is what stops an already-approved peer from re-spawning a fresh
/// pending row every time their request re-syncs (the reported "approve, Poll
/// Now, see approved + a new pending" loop). `Rejected` is intentionally NOT
/// matched so a previously-rejected peer can still ask again later. Otherwise
/// count `Pending` entries per pubkey and reject beyond `MAX_PENDING_PER_PUBKEY`.
pub async fn insert_inbound(
    data_dir: &Path,
    from_nostr_pubkey: String,
    from_nostr_npub: String,
    from_did: String,
    from_name: Option<String>,
    message: Option<String>,
) -> Result<Option<PendingPeerRequest>> {
    let mut requests = load_pending(data_dir).await?;
    expire_stale(&mut requests);

    let already_handled = requests.iter().any(|r| {
        r.from_nostr_pubkey == from_nostr_pubkey
            && r.from_did == from_did
            && matches!(r.state, PendingState::Pending | PendingState::Approved)
            && !r.outbound
    });
    if already_handled {
        save_pending(data_dir, &requests).await?;
        return Ok(None);
    }

    let live_count = requests
        .iter()
        .filter(|r| {
            r.from_nostr_pubkey == from_nostr_pubkey
                && matches!(r.state, PendingState::Pending)
                && !r.outbound
        })
        .count();
    if live_count >= MAX_PENDING_PER_PUBKEY {
        save_pending(data_dir, &requests).await?;
        anyhow::bail!(
            "rate-limited: {} already has {} pending requests",
            from_nostr_pubkey,
            live_count
        );
    }

    let row = PendingPeerRequest {
        id: uuid::Uuid::new_v4().to_string(),
        from_nostr_pubkey,
        from_nostr_npub,
        from_did,
        from_name,
        message,
        received_at: chrono::Utc::now().to_rfc3339(),
        state: PendingState::Pending,
        outbound: false,
    };
    requests.push(row.clone());
    save_pending(data_dir, &requests).await?;
    Ok(Some(row))
}

/// Record an outbound peer request we just sent, so the user can see it
/// in the "sent" tab and so the eventual NIP-44 invite reply can be
/// matched against it.
pub async fn insert_outbound(
    data_dir: &Path,
    to_nostr_pubkey: String,
    to_nostr_npub: String,
    to_did: String,
    to_name: Option<String>,
    message: Option<String>,
) -> Result<PendingPeerRequest> {
    let mut requests = load_pending(data_dir).await?;
    expire_stale(&mut requests);
    requests.retain(|r| {
        !(r.outbound
            && r.from_nostr_pubkey == to_nostr_pubkey
            && matches!(r.state, PendingState::Sent))
    });
    let row = PendingPeerRequest {
        id: uuid::Uuid::new_v4().to_string(),
        from_nostr_pubkey: to_nostr_pubkey,
        from_nostr_npub: to_nostr_npub,
        from_did: to_did,
        from_name: to_name,
        message,
        received_at: chrono::Utc::now().to_rfc3339(),
        state: PendingState::Sent,
        outbound: true,
    };
    requests.push(row.clone());
    save_pending(data_dir, &requests).await?;
    Ok(row)
}

pub async fn find_by_id(data_dir: &Path, id: &str) -> Result<Option<PendingPeerRequest>> {
    let requests = load_pending(data_dir).await?;
    Ok(requests.into_iter().find(|r| r.id == id))
}

pub async fn set_state(data_dir: &Path, id: &str, state: PendingState) -> Result<()> {
    let mut requests = load_pending(data_dir).await?;
    if let Some(r) = requests.iter_mut().find(|r| r.id == id) {
        r.state = state;
    } else {
        anyhow::bail!("Pending request not found: {}", id);
    }
    save_pending(data_dir, &requests).await?;
    Ok(())
}

/// Remove a pending request entirely. Used when the sender cancels an
/// outbound request they initiated and we want it gone (not just marked
/// Rejected/Cancelled — those states fill up the UI audit trail).
pub async fn delete(data_dir: &Path, id: &str) -> Result<()> {
    let mut requests = load_pending(data_dir).await?;
    let before = requests.len();
    requests.retain(|r| r.id != id);
    if requests.len() == before {
        anyhow::bail!("Pending request not found: {}", id);
    }
    save_pending(data_dir, &requests).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_insert_inbound_then_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        let r1 = insert_inbound(
            dir.path(),
            "npk1".into(),
            "npub1".into(),
            "did:key:zABC".into(),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(r1.is_some());

        let r2 = insert_inbound(
            dir.path(),
            "npk1".into(),
            "npub1".into(),
            "did:key:zABC".into(),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(r2.is_none(), "duplicate Pending request should be ignored");
    }

    #[tokio::test]
    async fn test_approved_request_does_not_respawn_pending() {
        // Regression for the "approve → Poll Now → approved + a fresh pending"
        // loop: once a request is Approved, a re-synced inbound for the same
        // peer must NOT create a new Pending row.
        let dir = tempfile::tempdir().unwrap();
        let r1 = insert_inbound(
            dir.path(),
            "npk1".into(),
            "npub1".into(),
            "did:key:zABC".into(),
            None,
            None,
        )
        .await
        .unwrap()
        .expect("first insert stored");

        set_state(dir.path(), &r1.id, PendingState::Approved)
            .await
            .unwrap();

        let r2 = insert_inbound(
            dir.path(),
            "npk1".into(),
            "npub1".into(),
            "did:key:zABC".into(),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(
            r2.is_none(),
            "an already-approved peer must not re-spawn a pending request"
        );

        let pending = load_pending(dir.path()).await.unwrap();
        assert_eq!(
            pending
                .iter()
                .filter(|r| matches!(r.state, PendingState::Pending))
                .count(),
            0,
            "no Pending rows should remain after approval + re-sync"
        );
    }

    #[tokio::test]
    async fn test_rate_limit() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..MAX_PENDING_PER_PUBKEY {
            let res = insert_inbound(
                dir.path(),
                "npk-spammer".into(),
                "npub-spammer".into(),
                format!("did:key:zVar{}", i),
                None,
                None,
            )
            .await
            .unwrap();
            assert!(res.is_some());
        }
        let result = insert_inbound(
            dir.path(),
            "npk-spammer".into(),
            "npub-spammer".into(),
            "did:key:zOverflow".into(),
            None,
            None,
        )
        .await;
        assert!(result.is_err(), "should rate-limit beyond MAX");
    }

    #[tokio::test]
    async fn test_set_state_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let row = insert_inbound(
            dir.path(),
            "npk2".into(),
            "npub2".into(),
            "did:key:zXYZ".into(),
            Some("Bob".into()),
            Some("hi".into()),
        )
        .await
        .unwrap()
        .unwrap();

        set_state(dir.path(), &row.id, PendingState::Approved)
            .await
            .unwrap();
        let reloaded = find_by_id(dir.path(), &row.id).await.unwrap().unwrap();
        assert_eq!(reloaded.state, PendingState::Approved);
    }
}
