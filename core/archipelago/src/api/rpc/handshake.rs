//! Nostr peer-discovery RPCs.
//!
//! `handshake.discover` — browse other nodes' presence events on configured
//! relays. Returns DID + nostr pubkey only; no onion is ever exposed.
//!
//! `handshake.connect` — send a `PeerRequest` to a discovered node's nostr
//! pubkey. Records the outbound request locally so the user can see what
//! they've sent. Does NOT include our onion address on the wire.
//!
//! `handshake.poll` — fetch new NIP-44 DMs addressed to our nostr pubkey
//! and dispatch them: inbound `PeerRequest` is queued in
//! `federation::pending` for manual approval; inbound `PeerInvite` is
//! applied via the existing federation invite-acceptance flow (which
//! adds the new peer as `Observer` — see federation.rs); inbound
//! `PeerReject` is recorded against the matching outbound row.

use super::RpcHandler;
use crate::federation::pending::{self, PendingPeerRequest, PendingState};
use crate::nostr_handshake::{self, HandshakeMessage};
use anyhow::{Context, Result};
use nostr_sdk::FromBech32;
use serde::{Deserialize, Serialize};

use crate::nostr_handshake::DISCOVERY_STATE_FILE as NOSTR_STATE_FILE;

/// Runtime override for `Config::nostr_discovery_enabled`. The OS-level
/// config file is read once at boot and is OFF by default; this state file
/// lets the user flip discoverability on/off at runtime via the Federation
/// UI without restarting the service. Both the boot-time presence publish
/// and the `handshake.poll` handler check this file before doing anything.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct NostrDiscoveryState {
    #[serde(default)]
    enabled: bool,
    /// Operator-chosen display name carried in the presence event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

async fn load_discovery_state(data_dir: &std::path::Path) -> NostrDiscoveryState {
    let path = data_dir.join(NOSTR_STATE_FILE);
    match tokio::fs::read_to_string(&path).await {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => NostrDiscoveryState::default(),
    }
}

async fn save_discovery_state(
    data_dir: &std::path::Path,
    state: &NostrDiscoveryState,
) -> Result<()> {
    let path = data_dir.join(NOSTR_STATE_FILE);
    let content = serde_json::to_string_pretty(state).context("serialize discovery state")?;
    tokio::fs::write(&path, content)
        .await
        .context("write discovery state")?;
    Ok(())
}

impl RpcHandler {
    /// Read the current runtime discoverability flag. Also returns the npub
    /// this node publishes as (the discoverability UI shows it — that npub,
    /// not the onion, is what's actually visible on the relays). Load-only:
    /// null until discovery keys exist.
    pub(super) async fn handle_nostr_discovery_status(&self) -> Result<serde_json::Value> {
        let state = load_discovery_state(&self.config.data_dir).await;
        let npub = nostr_handshake::own_npub(&self.config.data_dir.join("identity"))
            .await
            .unwrap_or(None);
        Ok(serde_json::json!({ "enabled": state.enabled, "npub": npub, "name": state.name }))
    }

    /// Set the runtime discoverability flag. If turning ON, publish presence
    /// once immediately so the user gets visible feedback that the relays
    /// have been notified. If turning OFF, do NOT actively scrub the relays
    /// here — `nostr_handshake::publish_presence` is replaceable, so the
    /// next reboot's startup pass plus the existing legacy revocation in
    /// `nostr_discovery::revoke_legacy_advertisements` are sufficient. A
    /// future Layer 3 task adds an explicit "tombstone" publish if needed.
    pub(super) async fn handle_nostr_set_discovery(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let enabled = params
            .get("enabled")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("Missing enabled"))?;

        // Optional display name. Absent param = keep the stored name (so a
        // plain off/on toggle doesn't forget it); present-but-empty clears it.
        let prior = load_discovery_state(&self.config.data_dir).await;
        let name = match params.get("name") {
            Some(v) => v.as_str().and_then(nostr_handshake::clean_display_name),
            None => prior.name,
        };

        save_discovery_state(
            &self.config.data_dir,
            &NostrDiscoveryState {
                enabled,
                name: name.clone(),
            },
        )
        .await?;

        if enabled && !self.config.nostr_relays.is_empty() {
            let (data, _) = self.state_manager.get_snapshot().await;
            let identity_dir = self.config.data_dir.join("identity");
            let did = crate::identity::did_key_from_pubkey_hex(&data.server_info.pubkey)
                .unwrap_or_default();
            let version = data.server_info.version.clone();
            let relays = self.handshake_relays().await;
            let tor_proxy = self.config.nostr_tor_proxy.clone();
            let publish_name = name.clone();
            tokio::spawn(async move {
                if let Err(e) = nostr_handshake::publish_presence(
                    &identity_dir,
                    &did,
                    &version,
                    publish_name.as_deref(),
                    &relays,
                    tor_proxy.as_deref(),
                )
                .await
                {
                    tracing::warn!("Initial presence publish failed: {}", e);
                }
            });
        } else if !enabled {
            // Switching off: overwrite our presence with an empty tombstone so
            // the node disappears from other nodes' discovery lists now, not
            // at the next TTL expiry.
            let identity_dir = self.config.data_dir.join("identity");
            let relays = self.handshake_relays().await;
            let tor_proxy = self.config.nostr_tor_proxy.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    nostr_handshake::publish_tombstone(&identity_dir, &relays, tor_proxy.as_deref())
                        .await
                {
                    tracing::warn!("Presence tombstone publish failed: {}", e);
                }
            });
        }

        Ok(serde_json::json!({ "enabled": enabled }))
    }

    /// The relay set every handshake operation uses: the user-managed relay
    /// list (Settings → Relays, `nostr_relays.json`) merged with the config
    /// defaults. Before 2026-07-22 handshake send/poll used ONLY the two
    /// hardcoded config relays (one of which is defunct) and ignored user
    /// relay edits entirely — so a sender publishing where the receiver
    /// never read was a routine, silent way for peer requests to vanish.
    pub(super) async fn handshake_relays(&self) -> Vec<String> {
        crate::nostr_relays::merged_relay_list(&self.config.data_dir, &self.config.nostr_relays)
            .await
    }

    /// Discover discoverable nodes via Nostr presence events.
    /// Returns (nostr_pubkey, npub, DID, version) only — never an onion.
    pub(super) async fn handle_handshake_discover(&self) -> Result<serde_json::Value> {
        // Discoverability gate: respect the runtime toggle. We allow `discover`
        // to query relays as long as the user is actively browsing — they're
        // an anonymous observer of presence events, not publishing anything.
        let identity_dir = self.config.data_dir.join("identity");
        let relays = self.handshake_relays().await;
        let nodes = nostr_handshake::discover_nodes(
            &identity_dir,
            &relays,
            self.config.nostr_tor_proxy.as_deref(),
        )
        .await?;
        Ok(serde_json::json!({ "nodes": nodes }))
    }

    /// Send a `PeerRequest` to a discovered node. Onion is never sent.
    /// Params: `{ recipient_nostr_pubkey, message?, name? }`.
    pub(super) async fn handle_handshake_connect(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let recipient_raw = params
            .get("recipient_nostr_pubkey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing recipient_nostr_pubkey"))?;
        let recipient_hex = if recipient_raw.starts_with("npub1") {
            nostr_sdk::PublicKey::from_bech32(recipient_raw)
                .map_err(|e| anyhow::anyhow!("Invalid npub: {}", e))?
                .to_hex()
        } else {
            recipient_raw.to_string()
        };
        let recipient_npub = nostr_sdk::PublicKey::from_hex(&recipient_hex)
            .ok()
            .and_then(|pk| nostr_sdk::ToBech32::to_bech32(&pk).ok())
            .unwrap_or_default();
        let message = params.get("message").and_then(|v| v.as_str());
        let optional_name = params.get("name").and_then(|v| v.as_str());

        let (data, _) = self.state_manager.get_snapshot().await;
        let our_did =
            crate::identity::did_key_from_pubkey_hex(&data.server_info.pubkey).unwrap_or_default();
        let our_version = &data.server_info.version;
        let our_name = optional_name.or(data.server_info.name.as_deref());

        let identity_dir = self.config.data_dir.join("identity");
        nostr_handshake::send_peer_request(
            &identity_dir,
            &recipient_hex,
            &our_did,
            our_version,
            our_name,
            message,
            &self.handshake_relays().await,
            self.config.nostr_tor_proxy.as_deref(),
        )
        .await?;

        // Record the outbound request so the user can see "Sent" status
        // and so the eventual NIP-44 PeerInvite reply can be matched.
        let row = pending::insert_outbound(
            &self.config.data_dir,
            recipient_hex.clone(),
            recipient_npub,
            String::new(), // remote DID unknown until they reply
            None,
            message.map(String::from),
        )
        .await?;

        Ok(serde_json::json!({
            "ok": true,
            "sent_to": recipient_hex,
            "id": row.id,
        }))
    }

    /// Poll relays for inbound NIP-44 handshake messages, then dispatch:
    /// - `PeerRequest`  → queue in `federation::pending` for approval
    /// - `PeerInvite`   → apply via federation invite flow (adds as Observer)
    /// - `PeerReject`   → mark matching outbound row as `Rejected`
    ///
    /// Never auto-adds peers, never auto-responds, never sends our onion.
    /// Background relay poll (2026-07-22): before this, `handshake.poll` ran
    /// ONLY when a user opened Federation and pressed the Poll button — a
    /// peer request sat on the relay until the target's operator happened to
    /// click, i.e. for most nodes forever ("requests never arrive"). Runs the
    /// same poll+dispatch as the RPC (the disabled gate inside still applies)
    /// and nudges the websocket revision when anything new lands so open UIs
    /// refresh immediately.
    pub async fn background_handshake_poll(self: &std::sync::Arc<Self>) {
        match self.handle_handshake_poll().await {
            Ok(res) => {
                let new = res
                    .get("new_requests")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                let applied = res
                    .get("applied_invites")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                if new > 0 || applied > 0 {
                    tracing::info!(
                        new_requests = new,
                        applied_invites = applied,
                        "handshake poll: inbound peer activity"
                    );
                    let (data, _) = self.state_manager.get_snapshot().await;
                    self.state_manager.update_data(data).await;
                }
            }
            Err(e) => tracing::debug!("background handshake poll failed: {e:#}"),
        }
        if load_discovery_state(&self.config.data_dir).await.enabled {
            if let Err(error) = self.retry_peer_approval_replies().await {
                tracing::warn!("Peer approval retry could not complete: {error:#}");
            }
        }
    }

    pub(super) async fn handle_handshake_poll(&self) -> Result<serde_json::Value> {
        // Runtime gate: if the user hasn't enabled discoverability, don't
        // touch the relays. The poll endpoint is a hard no-op until they
        // explicitly opt in via the Federation UI toggle.
        let state = load_discovery_state(&self.config.data_dir).await;
        if !state.enabled {
            return Ok(serde_json::json!({
                "polled": 0,
                "new_requests": Vec::<PendingPeerRequest>::new(),
                "applied_invites": Vec::<String>::new(),
                "rejected_outbound": Vec::<String>::new(),
                "skipped": Vec::<String>::new(),
                "discovery_disabled": true,
            }));
        }
        let identity_dir = self.config.data_dir.join("identity");
        let relays = self.handshake_relays().await;
        let handshakes = nostr_handshake::poll_handshakes(
            &identity_dir,
            &relays,
            self.config.nostr_tor_proxy.as_deref(),
            None,
        )
        .await?;

        let mut new_requests: Vec<PendingPeerRequest> = Vec::new();
        let mut applied_invites: Vec<String> = Vec::new();
        let mut rejected_outbound: Vec<String> = Vec::new();
        let mut cancelled_inbound: Vec<String> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();

        for hs in &handshakes {
            match &hs.message {
                HandshakeMessage::PeerRequest {
                    from_did,
                    version: _,
                    name,
                    message,
                } => {
                    match pending::insert_inbound(
                        &self.config.data_dir,
                        hs.from_nostr_pubkey.clone(),
                        hs.from_nostr_npub.clone(),
                        from_did.clone(),
                        name.clone(),
                        message.clone(),
                    )
                    .await
                    {
                        Ok(Some(row)) => new_requests.push(row),
                        Ok(None) => skipped.push(hs.from_nostr_pubkey.clone()),
                        Err(e) => {
                            tracing::warn!(
                                from = %hs.from_nostr_pubkey,
                                error = %e,
                                "Dropped peer request (rate limit or storage error)"
                            );
                            skipped.push(hs.from_nostr_pubkey.clone());
                        }
                    }
                }
                HandshakeMessage::PeerInvite { invite_code } => {
                    let _decision = pending::outbound_decision_guard().await;
                    // Match against an outbound Sent request from this nostr
                    // pubkey. If we never sent them anything, ignore — we
                    // don't accept unsolicited invites over Nostr.
                    let pendings = pending::load_pending(&self.config.data_dir).await?;
                    let matching = pendings.iter().find(|r| {
                        r.outbound
                            && r.from_nostr_pubkey == hs.from_nostr_pubkey
                            && matches!(r.state, PendingState::Sent)
                    });
                    let Some(row) = matching else {
                        tracing::warn!(
                            from = %hs.from_nostr_pubkey,
                            "Ignoring unsolicited PeerInvite — no matching Sent request"
                        );
                        continue;
                    };
                    let scoped_invite = match crate::federation::restrict_discovery_invite(
                        invite_code,
                        &row.from_did,
                    ) {
                        Ok(code) => code,
                        Err(_) => {
                            tracing::warn!("Rejected peer invite with mismatched identity");
                            continue;
                        }
                    };
                    let row_id = row.id.clone();
                    let (data, _) = self.state_manager.get_snapshot().await;
                    let local_did =
                        crate::identity::did_key_from_pubkey_hex(&data.server_info.pubkey)
                            .unwrap_or_default();
                    let local_onion = data.server_info.tor_address.clone().unwrap_or_default();
                    let local_pubkey = data.server_info.pubkey.clone();

                    let identity_dir2 = self.config.data_dir.join("identity");
                    let node_identity =
                        crate::identity::NodeIdentity::load_or_create(&identity_dir2).await?;
                    let local_fips_npub = crate::identity::fips_npub(&identity_dir2)
                        .await
                        .unwrap_or(None);
                    let local_name = data.server_info.name.clone();
                    match crate::federation::accept_invite(
                        &self.config.data_dir,
                        &scoped_invite,
                        &local_did,
                        &local_onion,
                        &local_pubkey,
                        local_fips_npub.as_deref(),
                        local_name.as_deref(),
                        |bytes| node_identity.sign(bytes),
                    )
                    .await
                    {
                        Ok(node) => {
                            // Approved-by-them: their box already has us as Observer
                            // (their approval handler added us under that trust level
                            // before sending the invite). Discovery invites are now
                            // minted with trust=observer, so accept_invite already
                            // lands on Observer; keep this explicit demotion as a
                            // safety net for legacy Trusted-only invite codes — the
                            // discovery flow should never auto-trust.
                            // `None` source: this is an automatic safety-net
                            // demotion, not an operator decision, so it must
                            // not overwrite how the peer actually got here.
                            let _ = crate::federation::set_trust_level(
                                &self.config.data_dir,
                                &node.did,
                                crate::federation::TrustLevel::Observer,
                                None,
                            )
                            .await;

                            // Mirror into the mesh peer table immediately so the
                            // chat UI can address the new peer without waiting
                            // for the next mesh restart.
                            let svc = self.mesh_service.read().await;
                            if let Some(svc) = svc.as_ref() {
                                crate::mesh::upsert_federation_peer(
                                    &svc.shared_state(),
                                    &node.pubkey,
                                    &node.did,
                                    node.name.as_deref(),
                                )
                                .await;
                            }

                            pending::complete_outbound(
                                &self.config.data_dir,
                                &row_id,
                                &hs.from_nostr_pubkey,
                                &node.did,
                            )
                            .await?;
                            applied_invites.push(node.did);
                        }
                        Err(e) => {
                            tracing::warn!(
                                from = %hs.from_nostr_pubkey,
                                error = %e,
                                "Failed to apply PeerInvite"
                            );
                        }
                    }
                }
                HandshakeMessage::PeerReject { reason } => {
                    let _decision = pending::outbound_decision_guard().await;
                    let pendings = pending::load_pending(&self.config.data_dir).await?;
                    if let Some(row) = pendings.iter().find(|r| {
                        r.outbound
                            && r.from_nostr_pubkey == hs.from_nostr_pubkey
                            && matches!(r.state, PendingState::Sent)
                    }) {
                        let row_id = row.id.clone();
                        pending::set_state(&self.config.data_dir, &row_id, PendingState::Rejected)
                            .await?;
                        rejected_outbound.push(row_id);
                        tracing::info!(
                            from = %hs.from_nostr_pubkey,
                            reason = ?reason,
                            "Outbound peer request rejected"
                        );
                    }
                }
                HandshakeMessage::PeerCancel { reason } => {
                    // Peer withdrew their PeerRequest — drop our matching
                    // inbound pending row so it disappears from the UI.
                    let pendings = pending::load_pending(&self.config.data_dir).await?;
                    if let Some(row) = pendings.iter().find(|r| {
                        !r.outbound
                            && r.from_nostr_pubkey == hs.from_nostr_pubkey
                            && matches!(r.state, PendingState::Pending)
                    }) {
                        let row_id = row.id.clone();
                        pending::delete(&self.config.data_dir, &row_id).await?;
                        cancelled_inbound.push(row_id);
                        tracing::info!(
                            from = %hs.from_nostr_pubkey,
                            reason = ?reason,
                            "Inbound peer request cancelled by sender"
                        );
                    }
                }
            }
        }

        Ok(serde_json::json!({
            "polled": handshakes.len(),
            "new_requests": new_requests,
            "applied_invites": applied_invites,
            "rejected_outbound": rejected_outbound,
            "cancelled_inbound": cancelled_inbound,
            "skipped": skipped,
        }))
    }
}
