//! Federation invite creation, parsing, and acceptance.

use anyhow::{Context, Result};
use std::path::Path;

use super::storage::{add_node, load_invites, load_nodes, save_invites, save_nodes};
use super::types::{FederatedNode, FederationInvite, TrustLevel};

/// Parsed contents of a federation invite code.
#[derive(Debug, Clone)]
pub struct ParsedInvite {
    pub did: String,
    pub onion: String,
    pub pubkey: String,
    /// Per-invite randomness; retained by parsers but not consumed
    /// end-to-end — the outer signature binds the relationship.
    #[allow(dead_code)]
    pub token: String,
    /// Inviter's FIPS npub if advertised in the code.
    pub fips_npub: Option<String>,
    /// Trust level the invite grants both sides. Absent in legacy codes,
    /// which default to Trusted.
    pub trust_level: TrustLevel,
}

/// Generate an invite code. Format: `fed1:<base64(json{did, onion, pubkey, token, fips_npub?, trust})>`.
/// `fips_npub` is only included when the local node has a materialised FIPS key.
/// `trust_level` is the level BOTH sides assign to each other for this invite
/// ("Invite a Peer" = Observer, "Link Your Nodes" = Trusted).
pub async fn create_invite(
    data_dir: &Path,
    did: &str,
    onion: &str,
    pubkey: &str,
    fips_npub: Option<&str>,
    trust_level: TrustLevel,
) -> Result<String> {
    use base64::Engine;

    // KEY-05: a federation invite token is unguessable-by-design — it is the
    // whole authorisation for a peer join — so the source is named and the
    // 16-byte draw is guarded. The `rand::Rng` import that brought `fill` into
    // scope is gone with the call that needed it.
    let mut token_bytes = [0u8; 16];
    crate::entropy::draw_key_bytes(&mut rand::rngs::OsRng, &mut token_bytes).map_err(|e| {
        anyhow::anyhow!("Refusing to mint an invite token from degenerate entropy: {e}")
    })?;
    let token = hex::encode(token_bytes);

    let mut payload = serde_json::json!({
        "did": did,
        "onion": onion,
        "pubkey": pubkey,
        "token": token,
        "trust": trust_level.to_string(),
    });
    if let Some(npub) = fips_npub {
        payload["fips_npub"] = serde_json::Value::String(npub.to_string());
    }
    let json = serde_json::to_string(&payload).context("Failed to serialize invite")?;
    let code = format!(
        "fed1:{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes())
    );

    let invite = FederationInvite {
        code: code.clone(),
        did: did.to_string(),
        onion: onion.to_string(),
        pubkey: pubkey.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        accepted: false,
        fips_npub: fips_npub.map(|s| s.to_string()),
        trust_level,
    };

    let mut invites = load_invites(data_dir).await?;
    invites.outgoing.push(invite);
    save_invites(data_dir, &invites).await?;

    Ok(code)
}

/// Parse an invite code into its components.
pub fn parse_invite(code: &str) -> Result<ParsedInvite> {
    use base64::Engine;

    let encoded = code
        .strip_prefix("fed1:")
        .ok_or_else(|| anyhow::anyhow!("Invalid invite format: must start with fed1:"))?;

    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .context("Invalid base64 in invite code")?;

    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).context("Invalid JSON in invite")?;

    let did = payload["did"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing did in invite"))?
        .to_string();
    let onion = payload["onion"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing onion in invite"))?
        .to_string();
    let pubkey = payload["pubkey"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing pubkey in invite"))?
        .to_string();
    let token = payload["token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing token in invite"))?
        .to_string();
    let fips_npub = payload
        .get("fips_npub")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    // Legacy codes (pre trust-threading) carry no "trust" field → Trusted,
    // which matches what both sides did before the field existed.
    let trust_level = payload
        .get("trust")
        .and_then(|v| v.as_str())
        .and_then(TrustLevel::parse)
        .unwrap_or(TrustLevel::Trusted);

    Ok(ParsedInvite {
        did,
        onion,
        pubkey,
        token,
        fips_npub,
        trust_level,
    })
}

/// Bind a Nostr-discovery reply to the node the operator requested, and cap
/// its grant before any local node entry or callback is written. Legacy invites
/// default to Trusted, which must never transiently authorize discovery peers.
pub(crate) fn restrict_discovery_invite(code: &str, expected_did: &str) -> Result<String> {
    use base64::Engine;
    let parsed = parse_invite(code)?;
    anyhow::ensure!(
        !expected_did.is_empty() && parsed.did == expected_did,
        "Peer invite does not match the requested node"
    );
    anyhow::ensure!(
        crate::identity::did_key_from_pubkey_hex(&parsed.pubkey)? == parsed.did,
        "Peer invite DID does not match its identity key"
    );
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(
        code.strip_prefix("fed1:")
            .context("Invalid invite prefix")?,
    )?;
    let mut payload: serde_json::Value = serde_json::from_slice(&bytes)?;
    payload["trust"] = serde_json::json!("observer");
    Ok(format!(
        "fed1:{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload)?)
    ))
}

/// Accept an invite: parse code, verify the remote node, add to federation.
pub async fn accept_invite(
    data_dir: &Path,
    code: &str,
    local_did: &str,
    local_onion: &str,
    local_pubkey: &str,
    local_fips_npub: Option<&str>,
    local_name: Option<&str>,
    sign_fn: impl FnOnce(&[u8]) -> String,
) -> Result<FederatedNode> {
    let ParsedInvite {
        did,
        onion,
        pubkey,
        token,
        fips_npub,
        trust_level,
    } = parse_invite(code)?;

    // Refuse self-peering. If the invite's did / onion / pubkey matches
    // our own, adding it pollutes the federation list with a node that
    // sees itself as its own peer and causes sync loops. The user
    // almost certainly pasted the wrong invite.
    if did == local_did || pubkey == local_pubkey || {
        let a = onion.trim_end_matches(".onion");
        let b = local_onion.trim_end_matches(".onion");
        !a.is_empty() && a == b
    } {
        anyhow::bail!(
            "Refusing to federate with self — invite points at this node's own did / onion / pubkey"
        );
    }

    // Make accept idempotent: drop any existing entry that conflicts with
    // this invite — same DID (same node, refreshing the link), same onion
    // (node rotated identity but kept its hidden service), or same pubkey
    // (DID and onion reformatted but the underlying key is the same).
    // Whatever is there gets replaced so re-accepting an invite is always
    // safe and the user never has to manually remove an entry first.
    let mut nodes = load_nodes(data_dir).await?;
    let onion_norm = onion.trim_end_matches(".onion");
    let before = nodes.len();
    nodes.retain(|n| {
        n.did != did && n.onion.trim_end_matches(".onion") != onion_norm && n.pubkey != pubkey
    });
    if nodes.len() != before {
        save_nodes(data_dir, &nodes).await?;
        tracing::info!(
            removed = before - nodes.len(),
            new_did = %did,
            onion = %onion,
            "Replaced stale federation entry on re-accept"
        );
    }

    let node = FederatedNode {
        trust_source: Some(super::types::TrustSource::Invite),
        did: did.clone(),
        pubkey,
        onion,
        name: None,
        // The invite code itself says what this relationship is —
        // Observer for "Invite a Peer", Trusted for "Link Your Nodes".
        trust_level,
        added_at: chrono::Utc::now().to_rfc3339(),
        last_seen: None,
        last_state: None,
        fips_npub: fips_npub.clone(),
        last_transport: None,
        last_transport_at: None,
        last_sync_error: None,
        last_sync_error_at: None,
    };

    add_node(data_dir, node.clone()).await?;

    // Record as incoming accepted invite
    let mut invites = load_invites(data_dir).await?;
    invites.incoming.push(FederationInvite {
        code: code.to_string(),
        did: did.clone(),
        onion: node.onion.clone(),
        pubkey: node.pubkey.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        accepted: true,
        fips_npub,
        trust_level,
    });
    save_invites(data_dir, &invites).await?;

    // Notify remote node (best-effort, FIPS-first → Tor fallback)
    let _ = notify_join(
        &node.onion,
        node.fips_npub.as_deref(),
        local_did,
        local_onion,
        local_pubkey,
        local_fips_npub,
        local_name,
        Some(&token),
        trust_level,
        sign_fn,
    )
    .await;

    Ok(node)
}

/// Best-effort notification to the remote node that we joined their federation.
/// Prefers FIPS (if the remote advertised an npub in their invite) and
/// falls back to Tor. Signs the message with our ed25519 key so the
/// remote peer can verify authenticity regardless of transport.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn notify_join(
    remote_onion: &str,
    remote_fips_npub: Option<&str>,
    local_did: &str,
    local_onion: &str,
    local_pubkey: &str,
    local_fips_npub: Option<&str>,
    local_name: Option<&str>,
    invite_token: Option<&str>,
    trust_level: TrustLevel,
    sign_fn: impl FnOnce(&[u8]) -> String,
) -> Result<()> {
    // Sign the canonical message: "peer-joined:{did}:{onion}:{pubkey}"
    // Signature domain intentionally unchanged — fips_npub + name are
    // carried as unsigned informational fields. Name is display-only
    // (any identity claim is anchored on the signed did/pubkey); the
    // FIPS daemon's own Noise handshake authenticates the transport
    // session regardless of the advertised npub.
    //
    // invite_token + trust are also unsigned: the inviter treats the token
    // as a lookup key into its own stored invites (authoritative for the
    // granted level) and only ever accepts the trust claim as a DOWNGRADE,
    // so neither field can escalate privileges.
    let sign_data = format!("peer-joined:{}:{}:{}", local_did, local_onion, local_pubkey);
    let signature = sign_fn(sign_data.as_bytes());

    let mut params = serde_json::json!({
        "did": local_did,
        "onion": local_onion,
        "pubkey": local_pubkey,
        "signature": signature,
        "trust": trust_level.to_string(),
    });
    if let Some(npub) = local_fips_npub {
        params["fips_npub"] = serde_json::Value::String(npub.to_string());
    }
    if let Some(name) = local_name {
        params["name"] = serde_json::Value::String(name.to_string());
    }
    if let Some(token) = invite_token {
        params["invite_token"] = serde_json::Value::String(token.to_string());
    }

    let body = serde_json::json!({
        "method": "federation.peer-joined",
        "params": params,
    });

    // Deliver the notification in the BACKGROUND with retries, and return
    // immediately. Two reasons:
    //   1. The join RPC must not block on this. Awaiting a cold FIPS overlay
    //      (no shared FIPS path between LAN and remote/Tailscale peers) stalled
    //      the whole join until FIPS timed out, surfacing as "Request timeout"
    //      in the UI even though the local membership was already saved.
    //   2. If this single best-effort POST failed, the inviter never learned
    //      about us → asymmetric federation (they couldn't see us). Retrying in
    //      the background until it lands makes federation converge to symmetric.
    // `fips_timeout` fast-fails a dead FIPS path so the Tor fallback (which
    // answers an onion in ~3-5s) is reached quickly on each attempt.
    let remote_onion = remote_onion.to_string();
    let remote_fips_npub = remote_fips_npub.map(|s| s.to_string());
    tokio::spawn(async move {
        // ~5 attempts with linear backoff: 0s, 10s, 20s, 30s, 40s — covers a
        // peer that is briefly unreachable (restarting, publishing its onion)
        // without hammering it.
        for attempt in 1..=5u32 {
            let res = crate::fips::dial::PeerRequest::new(
                remote_fips_npub.as_deref(),
                &remote_onion,
                "/rpc/v1",
            )
            .service(crate::settings::transport::PeerService::Federation)
            .timeout(std::time::Duration::from_secs(30))
            .fips_timeout(std::time::Duration::from_secs(6))
            .send_json(&body)
            .await;
            match res {
                Ok((resp, transport)) if resp.status().is_success() => {
                    // A JSON-RPC-level rejection still arrives as HTTP 200
                    // (the RPC layer returns errors in-band), so checking the
                    // status alone logged "delivered" for calls the peer had
                    // just rejected. Read the body: an in-band error is
                    // terminal — the signed payload is deterministic, so
                    // retrying identical bytes can never succeed.
                    let body = resp.text().await.unwrap_or_default();
                    let rpc_err = serde_json::from_str::<serde_json::Value>(&body)
                        .ok()
                        .and_then(|v| v.get("error").cloned())
                        .filter(|e| !e.is_null());
                    if let Some(err) = rpc_err {
                        tracing::warn!(
                            attempt,
                            transport = %transport,
                            error = %err,
                            "peer-joined notification rejected by peer — giving up (retrying identical payload cannot succeed)"
                        );
                        return;
                    }
                    tracing::info!(
                        attempt,
                        transport = %transport,
                        "peer-joined notification delivered to inviter"
                    );
                    return;
                }
                Ok((resp, _)) => tracing::warn!(
                    attempt,
                    status = %resp.status(),
                    "peer-joined notification rejected; will retry"
                ),
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "peer-joined notification failed; will retry")
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(10 * attempt as u64)).await;
        }
        tracing::warn!(
            onion = %remote_onion,
            "peer-joined notification gave up after retries — peer may not see us until next sync"
        );
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::federation::storage::load_nodes;

    #[tokio::test]
    async fn test_create_and_parse_invite() {
        let dir = tempfile::tempdir().unwrap();
        let code = create_invite(
            dir.path(),
            "did:key:z1",
            "test.onion",
            "aabbcc",
            None,
            TrustLevel::Trusted,
        )
        .await
        .unwrap();
        assert!(code.starts_with("fed1:"));

        let parsed = parse_invite(&code).unwrap();
        assert_eq!(parsed.did, "did:key:z1");
        assert_eq!(parsed.onion, "test.onion");
        assert_eq!(parsed.pubkey, "aabbcc");
        assert_eq!(parsed.token.len(), 32); // 16 bytes = 32 hex chars
        assert!(parsed.fips_npub.is_none());
    }

    #[tokio::test]
    async fn test_invite_roundtrips_fips_npub() {
        let dir = tempfile::tempdir().unwrap();
        let fips = "npub1fipstest0000000000000000000000000000000000";
        let code = create_invite(
            dir.path(),
            "did:key:z1",
            "test.onion",
            "aabbcc",
            Some(fips),
            TrustLevel::Trusted,
        )
        .await
        .unwrap();
        let parsed = parse_invite(&code).unwrap();
        assert_eq!(parsed.fips_npub.as_deref(), Some(fips));
    }

    #[tokio::test]
    async fn test_parse_invite_tolerates_missing_fips() {
        // Older invites minted before fips_npub existed must still parse.
        use base64::Engine;
        let legacy = serde_json::json!({
            "did": "did:key:zOld",
            "onion": "old.onion",
            "pubkey": "00",
            "token": "aa",
        });
        let code = format!(
            "fed1:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_string(&legacy).unwrap())
        );
        let parsed = parse_invite(&code).unwrap();
        assert_eq!(parsed.did, "did:key:zOld");
        assert!(parsed.fips_npub.is_none());
    }

    #[tokio::test]
    async fn test_observer_invite_threads_trust_level() {
        // "Invite a Peer" mints an observer invite; the acceptor must land
        // on Observer, not Trusted (the original bug).
        let dir = tempfile::tempdir().unwrap();
        let code = create_invite(
            dir.path(),
            "did:key:zRemote",
            "remote.onion",
            "remotepub",
            None,
            TrustLevel::Observer,
        )
        .await
        .unwrap();

        let parsed = parse_invite(&code).unwrap();
        assert_eq!(parsed.trust_level, TrustLevel::Observer);

        let dir2 = tempfile::tempdir().unwrap();
        let node = accept_invite(
            dir2.path(),
            &code,
            "did:key:zLocal",
            "local.onion",
            "localpub",
            None,
            None,
            |_| "test-sig".to_string(),
        )
        .await
        .unwrap();
        assert_eq!(node.trust_level, TrustLevel::Observer);

        // The inviter's stored outgoing invite carries the level too — this
        // is what peer-joined uses as the authoritative grant on the far side.
        let invites = load_invites(dir.path()).await.unwrap();
        assert_eq!(invites.outgoing.len(), 1);
        assert_eq!(invites.outgoing[0].trust_level, TrustLevel::Observer);
    }

    #[test]
    fn test_parse_legacy_invite_defaults_to_trusted() {
        // Codes minted before the trust field existed must stay Trusted.
        use base64::Engine;
        let legacy = serde_json::json!({
            "did": "did:key:zOld",
            "onion": "old.onion",
            "pubkey": "00",
            "token": "aa",
        });
        let code = format!(
            "fed1:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_string(&legacy).unwrap())
        );
        let parsed = parse_invite(&code).unwrap();
        assert_eq!(parsed.trust_level, TrustLevel::Trusted);
    }

    #[test]
    fn test_parse_invalid_invite() {
        assert!(parse_invite("invalid").is_err());
        assert!(parse_invite("fed1:not-valid-base64!!!").is_err());
    }

    #[tokio::test]
    async fn test_accept_invite_creates_node() {
        let dir = tempfile::tempdir().unwrap();
        let code = create_invite(
            dir.path(),
            "did:key:zRemote",
            "remote.onion",
            "remotepub",
            None,
            TrustLevel::Trusted,
        )
        .await
        .unwrap();

        // Accept from a different "local" perspective
        let dir2 = tempfile::tempdir().unwrap();
        let node = accept_invite(
            dir2.path(),
            &code,
            "did:key:zLocal",
            "local.onion",
            "localpub",
            None,
            None,
            |_| "test-sig".to_string(),
        )
        .await
        .unwrap();

        assert_eq!(node.did, "did:key:zRemote");
        assert_eq!(node.trust_level, TrustLevel::Trusted);

        let nodes = load_nodes(dir2.path()).await.unwrap();
        assert_eq!(nodes.len(), 1);
    }

    #[tokio::test]
    async fn test_accept_invite_persists_fips_npub() {
        let dir = tempfile::tempdir().unwrap();
        let fips = "npub1remotefipsaddrxxxxxxxxxxxxxxxxxxxxxxxxxx";
        let code = create_invite(
            dir.path(),
            "did:key:zRemote",
            "remote.onion",
            "remotepub",
            Some(fips),
            TrustLevel::Trusted,
        )
        .await
        .unwrap();

        let dir2 = tempfile::tempdir().unwrap();
        let node = accept_invite(
            dir2.path(),
            &code,
            "did:key:zLocal",
            "local.onion",
            "localpub",
            None,
            None,
            |_| "test-sig".to_string(),
        )
        .await
        .unwrap();

        assert_eq!(node.fips_npub.as_deref(), Some(fips));
    }

    #[tokio::test]
    async fn test_accept_invite_is_idempotent() {
        // Re-accepting the same invite is a no-op refresh — it must not
        // duplicate the entry and must not error. This is the contract the
        // UI relies on: clicking "Join" twice or refreshing after an
        // identity rotation always converges to one entry.
        let dir = tempfile::tempdir().unwrap();
        let code = create_invite(
            dir.path(),
            "did:key:zRemote",
            "remote.onion",
            "remotepub",
            None,
            TrustLevel::Trusted,
        )
        .await
        .unwrap();

        let dir2 = tempfile::tempdir().unwrap();
        accept_invite(
            dir2.path(),
            &code,
            "did:key:zLocal",
            "local.onion",
            "localpub",
            None,
            None,
            |_| "test-sig".to_string(),
        )
        .await
        .unwrap();

        accept_invite(
            dir2.path(),
            &code,
            "did:key:zLocal",
            "local.onion",
            "localpub",
            None,
            None,
            |_| "test-sig".to_string(),
        )
        .await
        .unwrap();

        let nodes = load_nodes(dir2.path()).await.unwrap();
        assert_eq!(nodes.len(), 1, "re-accept should not duplicate");
    }
}

#[cfg(test)]
mod discovery_invite_scope_tests {
    use super::*;
    use base64::Engine;
    #[test]
    fn discovery_reply_binds_identity_and_caps_legacy_trust_before_acceptance() {
        let key = "33".repeat(32);
        let did = crate::identity::did_key_from_pubkey_hex(&key).unwrap();
        let payload =
            serde_json::json!({"did":did,"pubkey":key,"onion":"test.onion","token":"test-token"});
        let code = format!(
            "fed1:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(&payload).unwrap())
        );
        let restricted = restrict_discovery_invite(&code, &did).unwrap();
        let parsed = parse_invite(&restricted).unwrap();
        assert_eq!(parsed.trust_level, TrustLevel::Observer);
        assert_eq!(parsed.token, "test-token");
        assert!(restrict_discovery_invite(&code, "did:key:someone-else").is_err());
        assert!(restrict_discovery_invite(&code, "").is_err());
        let mut forged = payload;
        forged["pubkey"] = serde_json::json!("44".repeat(32));
        let forged = format!(
            "fed1:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(&forged).unwrap())
        );
        assert!(restrict_discovery_invite(&forged, &did).is_err());
    }
}
