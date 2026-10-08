use super::*;
use crate::api::rpc::RpcHandler;
use crate::credentials;
use crate::federation::{self, pending, FederatedNode, TrustLevel};
use crate::identity;
use crate::mesh;
use crate::network::dwn_store::DwnStore;
use crate::nostr_handshake;
use anyhow::Result;
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::sync::Arc;
use tracing::{debug, info, warn};

const FEDERATION_PROTOCOL: &str = "https://archipelago.dev/protocols/federation/v1";

impl RpcHandler {
    /// Register a federation node with the running mesh service so it's
    /// immediately addressable as a chat target. The mesh service seeds
    /// federation peers at startup, but federation nodes added or rotated
    /// later in the session would otherwise stay invisible to the mesh
    /// chat UI until the next mesh restart, and `mesh.send` against the
    /// frontend's synthesised contact_id would fail with "Unknown
    /// federation peer". Best-effort: silently no-ops when mesh is off.
    async fn register_federation_peer_in_mesh(
        &self,
        pubkey_hex: &str,
        did: &str,
        name: Option<&str>,
    ) {
        let svc = self.mesh_service.read().await;
        if let Some(svc) = svc.as_ref() {
            mesh::upsert_federation_peer(&svc.shared_state(), pubkey_hex, did, name).await;
        }
    }

    /// Re-seed every federation node from disk into the mesh peer table so the
    /// chat list reflects what the latest federation sync learned — display
    /// names (landed in `nodes.json` by `update_node_state` when a peer
    /// announces its name) and transitively-discovered peers (merged by
    /// `merge_transitive_peers`) — WITHOUT waiting for a mesh restart.
    ///
    /// Without this, a peer accepted via invite (seeded with `name = None`)
    /// stays "Archipelago <pubkey8>" in chat until the next restart even after
    /// sync has learned its real name, and transitive peers never appear as
    /// chat contacts at all. `seed_federation_peers_into_mesh` is idempotent
    /// and dedups by onion, so calling it after each sync is safe.
    /// Best-effort: silently no-ops when mesh is off.
    pub(crate) async fn refresh_federation_mesh_peers(&self) {
        let svc = self.mesh_service.read().await;
        if let Some(svc) = svc.as_ref() {
            mesh::seed_federation_peers_into_mesh(&svc.shared_state(), &self.config.data_dir).await;
        }
    }
}

/// Error prefix the frontend keys on to know it should prompt for the node
/// password and retry, rather than surface the message as a dead end.
pub(in crate::api::rpc) const PASSWORD_REQUIRED_PREFIX: &str = "PASSWORD_REQUIRED";

impl RpcHandler {
    /// Re-authenticate the operator before granting `Trusted`.
    ///
    /// A Trusted peer can read node state, be deployed to, and is exempt from
    /// the `!= Untrusted` gates federation/DWN/messaging use — so granting it
    /// is a privilege escalation and must cost a fresh proof that the person
    /// at the keyboard is the operator, not merely that a session cookie
    /// exists. This is the same reasoning as `node.rotate-identity` and 2FA
    /// setup, both of which already re-verify.
    ///
    /// Only ever called on the way UP. Demotion stays ungated: making
    /// something less privileged must never be harder than leaving it alone,
    /// or the safe action becomes the inconvenient one.
    async fn verify_operator_password(&self, params: Option<&serde_json::Value>) -> Result<()> {
        let password = params
            .and_then(|p| p.get("password"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if password.is_empty() {
            anyhow::bail!("{PASSWORD_REQUIRED_PREFIX}: node password required to grant Trusted");
        }

        if !self.auth_manager.verify_password(password).await? {
            anyhow::bail!("Password verification failed");
        }

        Ok(())
    }

    /// federation.invite — Generate an invite code containing our DID + onion for a peer.
    /// Optional param `trust_level`: "trusted" (default, "Link Your Nodes") or
    /// "observer" ("Invite a Peer") — the level BOTH sides assign for this invite.
    ///
    /// Minting a **Trusted** invite requires the node password (param
    /// `password`): the invite is a bearer grant of Trusted to whoever
    /// redeems it, so it is the escalation, not the later redemption.
    /// Observer invites are unchanged.
    pub(in crate::api::rpc) async fn handle_federation_invite(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let trust_level = params
            .as_ref()
            .and_then(|p| p.get("trust_level"))
            .and_then(|v| v.as_str())
            .map(|s| {
                TrustLevel::parse(s).ok_or_else(|| {
                    anyhow::anyhow!("Invalid trust_level: {s} (expected trusted|observer)")
                })
            })
            .transpose()?
            .unwrap_or(TrustLevel::Trusted);

        // Note this covers the DEFAULT too: "Link Your Nodes" sends no
        // `trust_level` and lands on Trusted above, so the gate must key off
        // the resolved level rather than an explicit request for Trusted.
        if trust_level == TrustLevel::Trusted {
            self.verify_operator_password(params.as_ref()).await?;
        }

        let (data, _) = self.state_manager.get_snapshot().await;
        let did = identity::did_key_from_pubkey_hex(&data.server_info.pubkey)?;
        let onion = data.server_info.tor_address.clone().unwrap_or_default();
        let pubkey = data.server_info.pubkey.clone();

        if onion.is_empty() {
            anyhow::bail!("Tor address not available. Tor may not be running.");
        }

        let identity_dir = self.config.data_dir.join("identity");
        let fips_npub = identity::fips_npub(&identity_dir).await.unwrap_or(None);

        let code = federation::create_invite(
            &self.config.data_dir,
            &did,
            &onion,
            &pubkey,
            fips_npub.as_deref(),
            trust_level,
        )
        .await?;

        info!(did = %did, trust = %trust_level, fips_advertised = fips_npub.is_some(), "Generated federation invite");
        Ok(serde_json::json!({
            "code": code,
            "did": did,
            "onion": onion,
            "trust_level": trust_level.to_string(),
        }))
    }

    /// federation.join — Accept an invite code and establish federation with the remote node.
    pub(in crate::api::rpc) async fn handle_federation_join(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let code = params
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'code' parameter"))?;

        let (data, _) = self.state_manager.get_snapshot().await;
        let local_did = identity::did_key_from_pubkey_hex(&data.server_info.pubkey)?;
        let local_onion = data.server_info.tor_address.clone().unwrap_or_default();
        let local_pubkey = data.server_info.pubkey.clone();
        let local_name = data.server_info.name.clone();

        let identity_dir = self.config.data_dir.join("identity");
        let node_identity = identity::NodeIdentity::load_or_create(&identity_dir).await?;
        let local_fips_npub = identity::fips_npub(&identity_dir).await.unwrap_or(None);
        let node = federation::accept_invite(
            &self.config.data_dir,
            code,
            &local_did,
            &local_onion,
            &local_pubkey,
            local_fips_npub.as_deref(),
            local_name.as_deref(),
            |data| node_identity.sign(data),
        )
        .await?;

        info!(peer_did = %node.did, "Joined federation with peer");

        // Make the new peer immediately addressable from the mesh chat UI.
        // Without this, the row exists in the federation list but `mesh.send`
        // against it fails until the next mesh service restart re-seeds.
        self.register_federation_peer_in_mesh(&node.pubkey, &node.did, node.name.as_deref())
            .await;

        // Store federation membership as DWN message
        if let Ok(store) = DwnStore::new(&self.config.data_dir).await {
            let dwn_data = serde_json::json!({
                "node_did": node.did,
                "trust_level": node.trust_level.to_string(),
                "joined_at": chrono::Utc::now().to_rfc3339(),
                "apps": [],
            });
            if let Err(e) = store
                .write_message(
                    &local_did,
                    Some(FEDERATION_PROTOCOL),
                    Some("https://archipelago.dev/schemas/federation-membership/v1"),
                    Some("application/json"),
                    Some(dwn_data),
                )
                .await
            {
                debug!("DWN federation membership write (non-fatal): {}", e);
            }
        }

        // Issue a federation trust VC attesting the peer relationship
        {
            let data_dir = self.config.data_dir.clone();
            let peer_did = node.did.clone();
            let issuer_did_vc = local_did.clone();
            tokio::spawn(async move {
                let claims = serde_json::json!({
                    "federationPeer": true,
                    "establishedAt": chrono::Utc::now().to_rfc3339(),
                });
                match credentials::issue_credential(
                    &data_dir,
                    &issuer_did_vc,
                    &peer_did,
                    "FederationTrustCredential",
                    claims,
                    None,
                    |bytes| {
                        // Sign with node identity key
                        let identity_dir = data_dir.join("identity");
                        tokio::task::block_in_place(|| {
                            let rt = tokio::runtime::Handle::current();
                            rt.block_on(async {
                                let id =
                                    crate::identity::NodeIdentity::load_or_create(&identity_dir)
                                        .await?;
                                Ok(id.sign(bytes))
                            })
                        })
                    },
                )
                .await
                {
                    Ok(vc) => {
                        debug!(vc_id = %vc.id, peer = %peer_did, "Issued federation trust VC")
                    }
                    Err(e) => debug!(error = %e, "Federation trust VC issuance failed (non-fatal)"),
                }
            });
        }

        Ok(serde_json::json!({
            "joined": true,
            "node": {
                "did": node.did,
                "onion": node.onion,
                "pubkey": node.pubkey,
                "trust_level": node.trust_level.to_string(),
            }
        }))
    }

    /// federation.list-nodes — List all federated nodes with their status, last state, and VC verification.
    pub(in crate::api::rpc) async fn handle_federation_list_nodes(
        &self,
    ) -> Result<serde_json::Value> {
        let nodes = federation::load_nodes(&self.config.data_dir).await?;

        // Load credentials to check for federation VCs
        let cred_store = credentials::load_credentials(&self.config.data_dir)
            .await
            .ok();
        let vc_subjects: std::collections::HashSet<String> = cred_store
            .as_ref()
            .map(|s| {
                s.credentials
                    .iter()
                    .filter(|vc| {
                        vc.credential_type
                            .iter()
                            .any(|t| t == "FederationTrustCredential")
                            && !credentials::is_revoked(vc)
                    })
                    .map(|vc| vc.credential_subject.id.clone())
                    .collect()
            })
            .unwrap_or_default();

        let nodes_json: Vec<serde_json::Value> = nodes
            .iter()
            .map(|n| {
                let mut obj = serde_json::json!({
                    "did": n.did,
                    "pubkey": n.pubkey,
                    "onion": n.onion,
                    "trust_level": n.trust_level.to_string(),
                    "added_at": n.added_at,
                    "vc_verified": vc_subjects.contains(&n.did),
                });
                if let Some(name) = &n.name {
                    obj["name"] = serde_json::json!(name);
                }
                if let Some(last_seen) = &n.last_seen {
                    obj["last_seen"] = serde_json::json!(last_seen);
                }
                if let Some(state) = &n.last_state {
                    obj["last_state"] = serde_json::to_value(state).unwrap_or_default();
                }
                // FED-02: surface the most recent sync failure so the operator
                // sees a stale peer in the UI instead of it living only in the
                // node's debug log. Omitted (not null) when the last attempt
                // succeeded, so a recovered peer's badge disappears.
                if let Some(err) = &n.last_sync_error {
                    obj["last_sync_error"] = serde_json::json!(err);
                }
                if let Some(at) = &n.last_sync_error_at {
                    obj["last_sync_error_at"] = serde_json::json!(at);
                }
                // How this peer's trust level came to be. Emitted as an
                // explicit null when unknown rather than omitted: "recorded
                // before provenance was tracked" is the population the
                // operator most needs to review, so the UI must be able to
                // distinguish it from a field it simply didn't read.
                obj["trust_source"] = match &n.trust_source {
                    Some(src) => serde_json::to_value(src).unwrap_or(serde_json::Value::Null),
                    None => serde_json::Value::Null,
                };
                obj
            })
            .collect();

        Ok(serde_json::json!({ "nodes": nodes_json }))
    }

    /// federation.remove-node — Remove a node from the federation by DID.
    pub(in crate::api::rpc) async fn handle_federation_remove_node(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let did = params
            .get("did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'did' parameter"))?;
        validate_did(did)?;

        // Capture the node's pubkey before removal so we can also purge its
        // synthetic mesh contact/thread (#2) — remove_node only touches
        // nodes.json, which would otherwise leave a stale chat contact behind.
        let removed_pubkey = federation::load_nodes(&self.config.data_dir)
            .await
            .ok()
            .and_then(|nodes| nodes.into_iter().find(|n| n.did == did).map(|n| n.pubkey));

        let nodes = federation::remove_node(&self.config.data_dir, did).await?;
        info!(did = %did, "Removed node from federation");

        if let Some(pubkey) = removed_pubkey.filter(|p| !p.is_empty()) {
            let svc = self.mesh_service.read().await;
            if let Some(svc) = svc.as_ref() {
                let contact_id = mesh::federation_peer_contact_id(&pubkey);
                mesh::purge_federation_peer(
                    &svc.shared_state(),
                    contact_id,
                    &pubkey,
                    &self.config.data_dir,
                )
                .await;
            }
        }

        Ok(serde_json::json!({
            "removed": true,
            "nodes_remaining": nodes.len(),
        }))
    }

    /// federation.set-trust — Change trust level for a federated node.
    ///
    /// Promoting a node TO `Trusted` requires the node password (param
    /// `password`). Demotion and no-op re-sets do not: see
    /// `verify_operator_password` for why the gate is one-directional.
    pub(in crate::api::rpc) async fn handle_federation_set_trust(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let did = params
            .get("did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'did' parameter"))?;
        validate_did(did)?;
        let trust_str = params
            .get("trust_level")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'trust_level' parameter"))?;

        let trust = match trust_str {
            "trusted" => TrustLevel::Trusted,
            "observer" => TrustLevel::Observer,
            "untrusted" => TrustLevel::Untrusted,
            _ => anyhow::bail!(
                "Invalid trust level: {} (expected trusted/observer/untrusted)",
                trust_str
            ),
        };

        // Gate the ESCALATION only. Comparing against the node's current level
        // means a re-set of an already-Trusted peer (the dropdown re-emitting
        // its own value) doesn't pointlessly demand a password, while every
        // path that actually raises a peer to Trusted does.
        if trust == TrustLevel::Trusted {
            let already_trusted = federation::load_nodes(&self.config.data_dir)
                .await?
                .iter()
                .any(|n| n.did == did && n.trust_level == TrustLevel::Trusted);
            if !already_trusted {
                self.verify_operator_password(Some(&params)).await?;
            }
        }

        // Stamp Manual: this is the one path where a human chose the level, so
        // an audit of `trust_source` can tell it apart from the automatic
        // grants that `UninvitedJoin` / `TransitiveMerge` mark.
        federation::set_trust_level(
            &self.config.data_dir,
            did,
            trust,
            Some(federation::TrustSource::Manual),
        )
        .await?;

        info!(did = %did, trust = %trust, "Operator set federation trust level");

        Ok(serde_json::json!({
            "updated": true,
            "did": did,
            "trust_level": trust.to_string(),
        }))
    }

    /// federation.sync-state — Manually trigger state sync with all federated peers.
    pub(in crate::api::rpc) async fn handle_federation_sync_state(
        &self,
    ) -> Result<serde_json::Value> {
        let nodes = federation::load_nodes(&self.config.data_dir).await?;

        if nodes.is_empty() {
            return Ok(serde_json::json!({
                "synced": 0,
                "failed": 0,
                "results": [],
            }));
        }

        let (data, _) = self.state_manager.get_snapshot().await;
        let local_did = identity::did_key_from_pubkey_hex(&data.server_info.pubkey)?;

        let identity_dir = self.config.data_dir.join("identity");
        let node_identity = identity::NodeIdentity::load_or_create(&identity_dir).await?;

        // A dead Tor peer can take the bounded transport timeout. Serialising
        // those waits made one bad peer block every healthy peer behind it.
        // Keep concurrency bounded so a large federation cannot exhaust the
        // node's sockets or overwhelm a peer, while allowing healthy peers to
        // finish independently. Results are tagged and sorted below so the
        // response remains stable for callers and tests.
        const MAX_CONCURRENT_SYNCS: usize = 4;
        let semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_SYNCS));
        let identity = Arc::new(node_identity);
        let data_dir = self.config.data_dir.clone();
        let mut pending = FuturesUnordered::new();

        for (index, node) in nodes
            .into_iter()
            .filter(|node| node.trust_level != TrustLevel::Untrusted)
            .enumerate()
        {
            let data_dir = data_dir.clone();
            let local_did = local_did.clone();
            let identity = identity.clone();
            let semaphore = semaphore.clone();
            pending.push(async move {
                let permit = semaphore
                    .acquire_owned()
                    .await
                    .expect("sync semaphore lives for all pending syncs");
                let result = federation::sync_with_peer(&data_dir, &node, &local_did, |bytes| {
                    identity.sign(bytes)
                })
                .await;
                drop(permit);
                (index, node.did, result)
            });
        }

        let mut results = Vec::new();
        while let Some((index, did, result)) = pending.next().await {
            let row = match result {
                Ok(state) => serde_json::json!({
                    "index": index,
                    "did": did,
                    "status": "ok",
                    "apps": state.apps.len(),
                }),
                Err(e) => serde_json::json!({
                    "index": index,
                    "did": did,
                    "status": "error",
                    "error": e.to_string(),
                }),
            };
            results.push(row);
        }
        results.sort_by_key(|row| row["index"].as_u64().unwrap_or(u64::MAX));
        let synced = results.iter().filter(|row| row["status"] == "ok").count() as u32;
        let failed = results.len() as u32 - synced;
        for row in &mut results {
            if let Some(object) = row.as_object_mut() {
                object.remove("index");
            }
        }

        // Push any names/roster the sync just learned into the live mesh peer
        // table so the chat list updates without a restart (#42).
        self.refresh_federation_mesh_peers().await;

        Ok(serde_json::json!({
            "synced": synced,
            "failed": failed,
            "results": results,
        }))
    }

    /// federation.get-state — Return this node's state snapshot (called by peers during sync).
    pub(in crate::api::rpc) async fn handle_federation_get_state(
        &self,
    ) -> Result<serde_json::Value> {
        let (data, _) = self.state_manager.get_snapshot().await;

        // Build app statuses from package_data
        let apps: Vec<federation::AppStatus> = data
            .package_data
            .iter()
            .map(|(id, pkg)| federation::AppStatus {
                id: id.clone(),
                status: format!("{:?}", pkg.state).to_lowercase(),
                version: Some(pkg.manifest.version.clone()),
            })
            .collect();

        let tor_active = data.server_info.tor_address.is_some();

        let server_name = data.server_info.name.clone().filter(|n| !n.is_empty());

        // Encode our local Nostr identity as bech32 npub so federated peers
        // can display it under our name in the mesh UI without each peer
        // having to know how to convert hex → bech32 themselves.
        let nostr_npub =
            tokio::fs::read_to_string(self.config.data_dir.join("identity/nostr_pubkey"))
                .await
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .and_then(|hex| nostr_sdk::PublicKey::from_hex(&hex).ok())
                .and_then(|pk| nostr_sdk::ToBech32::to_bech32(&pk).ok());

        // Pass the current federated-peer list so the snapshot can include
        // a `federated_peers` hint for transitive federation — receivers
        // who trust us learn our Trusted peers and can route to them
        // over FIPS without a separate invite round-trip.
        let federated_peers = federation::load_nodes(&self.config.data_dir)
            .await
            .unwrap_or_default();

        // Our own FIPS npub, so pre-v1.4 federation pairs (whose
        // invite codes didn't carry it) can learn it on the next sync.
        let identity_dir = self.config.data_dir.join("identity");
        let own_fips_npub = crate::identity::fips_npub(&identity_dir)
            .await
            .ok()
            .flatten()
            .or_else(|| {
                // Legacy/dev nodes without a seed-derived key fall back
                // to the upstream daemon's public key on disk.
                None
            });
        let own_fips_npub = match own_fips_npub {
            Some(n) => Some(n),
            None => crate::fips::service::read_upstream_npub()
                .await
                .ok()
                .flatten(),
        };

        let shared_location = if data.server_info.share_location {
            data.server_info.lat.zip(data.server_info.lon)
        } else {
            None
        };

        // Reuse the minute collector instead of running expensive probes for
        // every peer. An absent/stalled collector is unknown, never zero load.
        let now = chrono::Utc::now().timestamp();
        let latest = self
            .metrics_store
            .latest()
            .await
            .filter(|sample| (0..=180).contains(&now.saturating_sub(sample.timestamp)));
        let metrics = latest.as_ref().map(|sample| &sample.system);
        let uptime = tokio::fs::read_to_string("/proc/uptime")
            .await
            .ok()
            .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
            .map(|v| v as u64);
        let state = federation::build_local_state(
            apps,
            metrics
                .map(|m| m.cpu_percent)
                .filter(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            metrics
                .filter(|m| m.mem_total_bytes > 0)
                .map(|m| m.mem_used_bytes),
            metrics
                .filter(|m| m.mem_total_bytes > 0)
                .map(|m| m.mem_total_bytes),
            metrics
                .filter(|m| m.disk_total_bytes > 0)
                .map(|m| m.disk_used_bytes),
            metrics
                .filter(|m| m.disk_total_bytes > 0)
                .map(|m| m.disk_total_bytes),
            uptime,
            tor_active,
            server_name,
            nostr_npub,
            own_fips_npub,
            &federated_peers,
            shared_location,
        );

        Ok(serde_json::to_value(&state)?)
    }

    /// federation.peer-joined — Called by a remote peer after they accept our invite.
    /// Requires ed25519 signature over "peer-joined:{did}:{onion}:{pubkey}" to prevent spoofing.
    pub(in crate::api::rpc) async fn handle_federation_peer_joined(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let did = params
            .get("did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'did'"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'onion'"))?;
        let pubkey = params
            .get("pubkey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'pubkey'"))?;
        // Optional, unsigned: peer's FIPS mesh npub. Carried for transport
        // selection only; FIPS handshake re-authenticates the session.
        let fips_npub = params
            .get("fips_npub")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        // Optional, unsigned: peer's display name. Display-only — identity
        // claims are anchored on the signed did/pubkey below.
        let incoming_name = params
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Resolve the trust level granted by this join. Authoritative source:
        // the acceptor echoes the invite's random token, which we match against
        // OUR stored outgoing invites — the level we minted the code with wins.
        // Fallback: the peer's (unsigned) "trust" claim, honored only as a
        // DOWNGRADE from Trusted so it can never escalate. Legacy peers send
        // neither → Trusted, matching pre-threading behavior.
        let claimed_trust = params
            .get("trust")
            .and_then(|v| v.as_str())
            .and_then(TrustLevel::parse)
            .unwrap_or(TrustLevel::Trusted);
        let invite_trust = match params.get("invite_token").and_then(|v| v.as_str()) {
            Some(token) => federation::load_invites(&self.config.data_dir)
                .await
                .ok()
                .and_then(|invites| {
                    invites.outgoing.iter().find_map(|inv| {
                        federation::parse_invite(&inv.code)
                            .ok()
                            .filter(|p| p.token == token)
                            .map(|_| inv.trust_level)
                    })
                }),
            None => None,
        };
        // An invite WE minted is the only thing that may grant Trusted.
        //
        // This handler is unauthenticated (see middleware.rs: federated peers
        // call it over Tor with no session) and reachable on /rpc/v1. Its
        // signature check proves only that the caller holds the private key for
        // the pubkey IT SUPPLIED — anyone can generate a keypair — so it
        // establishes identity, never authorisation. Defaulting an unmatched
        // join to Trusted therefore let any party that could reach the node
        // self-grant Trusted by simply omitting `invite_token`.
        //
        // Capped at Observer instead: still recorded, still reachable, still
        // passes the `!= Untrusted` gates that federation/DWN/messaging use, so
        // a legacy peer re-joining degrades rather than breaks — but it cannot
        // reach a level the operator never granted. `min` keeps a peer's own
        // lower claim honoured, so this can only ever reduce trust.
        let (granted_trust, trust_source) = match invite_trust {
            Some(level) => (level, federation::TrustSource::Invite),
            None => (
                TrustLevel::Observer.min(claimed_trust),
                federation::TrustSource::UninvitedJoin,
            ),
        };

        // Reject self-peering. If somehow our own did / onion / pubkey
        // comes back at us (misconfigured invite, gossip loop), adding
        // the entry causes sync loops where the node syncs with itself
        // forever. Drop it quietly — no useful recovery path.
        let (own_data, _) = self.state_manager.get_snapshot().await;
        let own_did_result = identity::did_key_from_pubkey_hex(&own_data.server_info.pubkey).ok();
        let own_onion_trim = own_data
            .server_info
            .tor_address
            .as_deref()
            .unwrap_or("")
            .trim_end_matches(".onion")
            .to_string();
        let incoming_onion_trim = onion.trim_end_matches(".onion");
        if own_did_result.as_deref() == Some(did)
            || pubkey == own_data.server_info.pubkey
            || (!own_onion_trim.is_empty() && own_onion_trim == incoming_onion_trim)
        {
            tracing::warn!(
                peer_did = %did,
                "Rejected peer-joined: inbound identity matches this node"
            );
            anyhow::bail!("Refusing to peer with self");
        }

        // Bind the DID to the advertised pubkey. Without this the signature
        // check below is self-referential (the caller signs over a pubkey it
        // also supplies), so a consistent-but-unrelated keypair would pass.
        // The DID-rotation handler already enforces the same invariant.
        match identity::did_key_from_pubkey_hex(pubkey) {
            Ok(derived) if derived == did => {}
            Ok(derived) => {
                tracing::warn!(peer_did = %did, derived_did = %derived, "Rejected peer-joined: DID does not match pubkey");
                anyhow::bail!("DID does not match pubkey");
            }
            Err(e) => {
                tracing::warn!(peer_did = %did, error = %e, "Rejected peer-joined: invalid pubkey");
                anyhow::bail!("Invalid pubkey");
            }
        }

        // Verify ed25519 signature to prevent federation spoofing (H2 security fix)
        let signature = params.get("signature").and_then(|v| v.as_str());
        match signature {
            Some(sig) => {
                let sign_data = format!("peer-joined:{}:{}:{}", did, onion, pubkey);
                match identity::NodeIdentity::verify(pubkey, sign_data.as_bytes(), sig) {
                    Ok(true) => {}
                    Ok(false) => {
                        tracing::warn!(peer_did = %did, "Rejected peer-joined: invalid signature");
                        anyhow::bail!("Invalid signature");
                    }
                    Err(e) => {
                        // Malformed hex / wrong length — distinguish from a
                        // genuine mismatch so the log tells us which it was.
                        tracing::warn!(peer_did = %did, error = %e, "Rejected peer-joined: malformed signature");
                        anyhow::bail!("Invalid signature");
                    }
                }
            }
            None => {
                tracing::warn!(peer_did = %did, "Rejected peer-joined: missing signature");
                anyhow::bail!(
                    "Missing signature — all federation peers must be cryptographically verified"
                );
            }
        }

        let nodes = federation::load_nodes(&self.config.data_dir).await?;
        if let Some(existing) = nodes.iter().find(|n| n.did == did) {
            // If already known but missing onion/pubkey/fips_npub/name, update them
            let needs_onion = existing.onion.is_empty();
            let needs_pubkey = existing.pubkey.is_empty();
            let needs_fips = existing.fips_npub.is_none() && fips_npub.is_some();
            let needs_name = existing.name.is_none() && incoming_name.is_some();
            if needs_onion || needs_pubkey || needs_fips || needs_name {
                let mut updated = existing.clone();
                if needs_onion && !onion.is_empty() {
                    updated.onion = onion.to_string();
                }
                if needs_pubkey && !pubkey.is_empty() {
                    updated.pubkey = pubkey.to_string();
                }
                if needs_fips {
                    updated.fips_npub = fips_npub.clone();
                }
                if needs_name {
                    updated.name = incoming_name.clone();
                }
                updated.last_seen = Some(chrono::Utc::now().to_rfc3339());
                federation::update_node(&self.config.data_dir, &updated).await?;
                info!(peer_did = %did, peer_onion = %onion, "Updated existing peer with fresh identity fields");
            }
            return Ok(serde_json::json!({ "accepted": true, "already_known": true }));
        }

        // Respect operator removal: a peer the operator deleted must not
        // silently re-join via a stale invite. The tombstone is only cleared
        // by an explicit local action (manually adding the node or accepting
        // an incoming invite) — not by a remote-triggered join.
        if federation::load_removed_dids(&self.config.data_dir)
            .await
            .unwrap_or_default()
            .contains(did)
        {
            info!(peer_did = %did, "Ignoring peer-joined for a removed (tombstoned) DID");
            return Ok(serde_json::json!({ "accepted": false, "removed": true }));
        }

        let node = FederatedNode {
            did: did.to_string(),
            pubkey: pubkey.to_string(),
            onion: onion.to_string(),
            name: incoming_name.clone(),
            trust_level: granted_trust,
            added_at: chrono::Utc::now().to_rfc3339(),
            last_seen: None,
            last_state: None,
            fips_npub,
            last_transport: None,
            last_transport_at: None,
            last_sync_error: None,
            last_sync_error_at: None,
            trust_source: Some(trust_source),
        };

        federation::add_node(&self.config.data_dir, node).await?;
        info!(
            peer_did = %did,
            trust = %granted_trust,
            source = ?trust_source,
            "Peer joined our federation"
        );

        // Mirror into mesh state so the inbound peer is addressable from
        // the chat UI without waiting for the next mesh restart.
        self.register_federation_peer_in_mesh(pubkey, did, incoming_name.as_deref())
            .await;

        // Bump the data-model revision so any Federation view with an
        // open WebSocket reloads its node list without waiting for the
        // user to click Sync.
        let (data, _) = self.state_manager.get_snapshot().await;
        self.state_manager.update_data(data).await;

        // Transitive discovery: spawn a task that pulls the new peer's
        // state (its own federated peers end up as Observer entries on
        // our side) so after a join every existing peer in our list is
        // aware of the newcomer via the next pair of syncs, without the
        // user clicking anything. Best-effort; errors are logged only.
        let data_dir = self.config.data_dir.clone();
        let new_peer_did = did.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if let Err(e) = crate::federation::sync_with_peer_by_did(&data_dir, &new_peer_did).await
            {
                tracing::debug!(
                    peer_did = %new_peer_did,
                    error = %e,
                    "Transitive sync on peer-joined failed (non-fatal)"
                );
            }
        });

        Ok(serde_json::json!({ "accepted": true }))
    }

    /// federation.deploy-app — Deploy an app to a remote federated node.
    pub(in crate::api::rpc) async fn handle_federation_deploy_app(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let peer_did = params
            .get("did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'did' (target node)"))?;
        let app_id = params
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'app_id'"))?;
        let version = params
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("latest");
        let marketplace_url = params
            .get("marketplace_url")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let nodes = federation::load_nodes(&self.config.data_dir).await?;
        let peer = nodes
            .iter()
            .find(|n| n.did == peer_did)
            .ok_or_else(|| anyhow::anyhow!("No federated node with DID {}", peer_did))?;

        let (data, _) = self.state_manager.get_snapshot().await;
        let local_did = identity::did_key_from_pubkey_hex(&data.server_info.pubkey)?;

        let identity_dir = self.config.data_dir.join("identity");
        let node_identity = identity::NodeIdentity::load_or_create(&identity_dir).await?;

        let result = federation::deploy_to_peer(
            peer,
            app_id,
            version,
            marketplace_url,
            &local_did,
            |bytes| node_identity.sign(bytes),
        )
        .await?;

        info!(app = %app_id, peer = %peer_did, "Deployed app to federated peer");
        Ok(result)
    }

    /// federation.peer-address-changed — A peer notifies us that their .onion changed.
    /// Requires ed25519 signature over "address-changed:{did}:{new_onion}" using the
    /// peer's known pubkey. This prevents attackers from redirecting federation traffic.
    pub(in crate::api::rpc) async fn handle_federation_peer_address_changed(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let did = params
            .get("did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing did"))?;
        let new_onion = params
            .get("new_onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing new_onion"))?;
        let signature = params
            .get("signature")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing signature — address changes must be signed"))?;

        // Load existing nodes, find the peer by DID
        let mut nodes = federation::load_nodes(&self.config.data_dir).await?;
        let found = nodes.iter_mut().find(|n| n.did == did);

        match found {
            Some(node) => {
                // Verify signature using the peer's KNOWN pubkey (H3 security fix)
                let sign_data = format!("address-changed:{}:{}", did, new_onion);
                match identity::NodeIdentity::verify(&node.pubkey, sign_data.as_bytes(), signature)
                {
                    Ok(true) => {}
                    _ => {
                        tracing::warn!(did = %did, "Rejected address change: invalid signature");
                        anyhow::bail!("Invalid signature — address change rejected");
                    }
                }

                let old = node.onion.clone();
                node.onion = new_onion.to_string();
                federation::save_nodes(&self.config.data_dir, &nodes).await?;
                info!(did = %did, old_onion = %old, new_onion = %new_onion, "Updated federated peer address (signature verified)");
                Ok(serde_json::json!({
                    "updated": true,
                    "did": did,
                    "old_onion": old,
                    "new_onion": new_onion,
                }))
            }
            None => {
                info!(did = %did, "Received address change from unknown peer — ignoring");
                Ok(serde_json::json!({
                    "updated": false,
                    "reason": "Unknown peer DID",
                }))
            }
        }
    }

    /// federation.notify-did-change — Notify all federated peers that our DID has rotated.
    /// Called after `node.rotate-did` to propagate the rotation proof to peers.
    pub(in crate::api::rpc) async fn handle_federation_notify_did_change(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let old_did = params
            .get("old_did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'old_did'"))?;
        let new_did = params
            .get("new_did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'new_did'"))?;
        let proof_signature = params
            .get("proof_signature")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'proof_signature'"))?;
        let proof_message = params
            .get("proof_message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'proof_message'"))?;

        validate_did(old_did)?;
        validate_did(new_did)?;

        // Get the new pubkey to include in the notification
        let (data, _) = self.state_manager.get_snapshot().await;
        let new_pubkey = data.server_info.pubkey.clone();

        let nodes = federation::load_nodes(&self.config.data_dir).await?;

        let mut notified = 0u32;
        let mut failed = 0u32;
        let mut results = Vec::new();

        for node in &nodes {
            // Only notify trusted and observer peers
            if node.trust_level == TrustLevel::Untrusted {
                continue;
            }

            let body = serde_json::json!({
                "method": "federation.peer-did-changed",
                "params": {
                    "old_did": old_did,
                    "new_did": new_did,
                    "new_pubkey": new_pubkey,
                    "signature": proof_signature,
                    "proof_message": proof_message,
                }
            });

            let req = crate::fips::dial::PeerRequest::new(
                node.fips_npub.as_deref(),
                &node.onion,
                "/rpc/v1",
            )
            .service(crate::settings::transport::PeerService::Peers)
            .timeout(std::time::Duration::from_secs(30))
            .fips_timeout(std::time::Duration::from_secs(6))
            .record_transport(&self.config.data_dir);

            match req.send_json(&body).await {
                Ok((resp, transport)) if resp.status().is_success() => {
                    notified += 1;
                    results.push(serde_json::json!({
                        "did": node.did,
                        "status": "ok",
                        "transport": transport.to_string(),
                    }));
                    info!(peer_did = %node.did, transport = %transport, "Notified peer of DID rotation");
                }
                Ok((resp, transport)) => {
                    failed += 1;
                    results.push(serde_json::json!({
                        "did": node.did,
                        "status": "error",
                        "error": format!("Peer returned {} (via {})", resp.status(), transport),
                    }));
                    warn!(peer_did = %node.did, status = %resp.status(), transport = %transport, "Peer rejected DID rotation notification");
                }
                Err(e) => {
                    failed += 1;
                    results.push(serde_json::json!({
                        "did": node.did,
                        "status": "error",
                        "error": e.to_string(),
                    }));
                    warn!(peer_did = %node.did, error = %e, "Failed to notify peer of DID rotation");
                }
            }
        }

        Ok(serde_json::json!({
            "notified": notified,
            "failed": failed,
            "results": results,
        }))
    }

    /// federation.peer-did-changed — A peer notifies us that their DID has rotated.
    /// Verifies the rotation proof against the peer's KNOWN pubkey before accepting.
    pub(in crate::api::rpc) async fn handle_federation_peer_did_changed(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let old_did = params
            .get("old_did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'old_did'"))?;
        let new_did = params
            .get("new_did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'new_did'"))?;
        let new_pubkey = params
            .get("new_pubkey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'new_pubkey'"))?;
        let signature = params
            .get("signature")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'signature'"))?;

        validate_did(old_did)?;
        validate_did(new_did)?;

        // Validate new_pubkey is a valid 32-byte hex-encoded Ed25519 public key
        let pubkey_bytes = hex::decode(new_pubkey)
            .map_err(|_| anyhow::anyhow!("Invalid new_pubkey: not valid hex"))?;
        if pubkey_bytes.len() != 32 {
            anyhow::bail!("Invalid new_pubkey: must be 32 bytes (64 hex chars)");
        }

        // Validate signature is valid hex of correct length (64 bytes = 128 hex chars)
        let sig_bytes = hex::decode(signature)
            .map_err(|_| anyhow::anyhow!("Invalid signature: not valid hex"))?;
        if sig_bytes.len() != 64 {
            anyhow::bail!("Invalid signature: must be 64 bytes (128 hex chars)");
        }

        // Verify the new_did matches the new_pubkey
        let expected_new_did = identity::did_key_from_pubkey_hex(new_pubkey)?;
        if expected_new_did != new_did {
            anyhow::bail!("new_did does not match new_pubkey");
        }

        // Load existing nodes, find the peer by their OLD DID
        let mut nodes = federation::load_nodes(&self.config.data_dir).await?;
        let found = nodes.iter_mut().find(|n| n.did == old_did);

        match found {
            Some(node) => {
                // Verify the rotation proof: the old key signed
                // "did-rotate:{old_did}:{new_did}:{timestamp}" and the sender
                // forwards both the signature and the full proof_message.
                let proof_message = params.get("proof_message").and_then(|v| v.as_str());

                let verified = if let Some(msg) = proof_message {
                    // Verify the proof_message starts with the expected prefix
                    let expected_prefix = format!("did-rotate:{}:{}:", old_did, new_did);
                    if !msg.starts_with(&expected_prefix) {
                        warn!(old_did = %old_did, "Rejected DID rotation: proof_message has wrong prefix");
                        anyhow::bail!("Invalid proof_message format");
                    }
                    // Verify signature against the full proof_message using the KNOWN pubkey
                    matches!(
                        identity::NodeIdentity::verify(&node.pubkey, msg.as_bytes(), signature),
                        Ok(true)
                    )
                } else {
                    // Fallback: verify without timestamp (backwards-compatible)
                    let fallback_msg = format!("did-rotate:{}:{}", old_did, new_did);
                    matches!(
                        identity::NodeIdentity::verify(
                            &node.pubkey,
                            fallback_msg.as_bytes(),
                            signature
                        ),
                        Ok(true)
                    )
                };

                if !verified {
                    warn!(old_did = %old_did, "Rejected DID rotation: invalid signature");
                    anyhow::bail!("Invalid signature — DID rotation rejected");
                }

                let old_pubkey = node.pubkey.clone();
                let rotated_name = node.name.clone();
                node.did = new_did.to_string();
                node.pubkey = new_pubkey.to_string();
                node.last_seen = Some(chrono::Utc::now().to_rfc3339());
                federation::save_nodes(&self.config.data_dir, &nodes).await?;

                // Drop the stale mesh peer entry keyed by the old pubkey's
                // synthetic contact_id, then upsert a fresh one under the
                // new pubkey so the chat UI doesn't show two rows post-rotation.
                {
                    let svc = self.mesh_service.read().await;
                    if let Some(svc) = svc.as_ref() {
                        let state = svc.shared_state();
                        let stale_id = mesh::federation_peer_contact_id(&old_pubkey);
                        state.peers.write().await.remove(&stale_id);
                        mesh::upsert_federation_peer(
                            &state,
                            new_pubkey,
                            new_did,
                            rotated_name.as_deref(),
                        )
                        .await;
                    }
                }

                info!(
                    old_did = %old_did,
                    new_did = %new_did,
                    old_pubkey = %old_pubkey,
                    "Updated federated peer DID (rotation signature verified)"
                );

                Ok(serde_json::json!({
                    "updated": true,
                    "old_did": old_did,
                    "new_did": new_did,
                }))
            }
            None => {
                info!(old_did = %old_did, "Received DID rotation from unknown peer — ignoring");
                Ok(serde_json::json!({
                    "updated": false,
                    "reason": "Unknown peer DID",
                }))
            }
        }
    }

    /// federation.list-pending-requests — return the inbox of inbound peer
    /// requests received over Nostr (and our outbound `Sent` rows). Each
    /// row carries a stable `id` the FE refers to when calling
    /// `federation.approve-request` / `federation.reject-request`.
    pub(in crate::api::rpc) async fn handle_federation_list_pending_requests(
        &self,
    ) -> Result<serde_json::Value> {
        let requests = pending::load_pending(&self.config.data_dir).await?;
        Ok(serde_json::json!({ "requests": requests }))
    }

    /// federation.approve-request — turn a pending peer request into a
    /// federation invite, ship it back via NIP-44, and add the requester
    /// to our federation list as `Observer` (NOT Trusted — the user must
    /// explicitly promote afterwards via `federation.set-trust`).
    ///
    /// This is the *only* code path that ever causes our onion to leave
    /// this box over Nostr, and the onion only travels inside a NIP-44
    /// ciphertext addressed to the requester's specific nostr pubkey.
    pub(in crate::api::rpc) async fn handle_federation_approve_request(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?;

        let req = pending::find_by_id(&self.config.data_dir, id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Pending request not found: {}", id))?;
        if !matches!(req.state, pending::PendingState::Pending) || req.outbound {
            anyhow::bail!(
                "Pending request is not awaiting approval (state={:?})",
                req.state
            );
        }

        let reply = self.prepare_peer_approval_reply(&req).await?;
        // Persist the operator decision before transport. A relay outage must
        // not require another approval or lose the already-authorized reply.
        pending::decide(&self.config.data_dir, id, pending::PendingState::Approved).await?;
        let delivered = self.deliver_peer_approval_reply(&reply).await?;
        Ok(serde_json::json!({ "approved": true, "id": id, "delivery_pending": !delivered }))
    }

    async fn prepare_peer_approval_reply(
        &self,
        req: &pending::PendingPeerRequest,
    ) -> Result<federation::handshake_delivery::ApprovalReply> {
        use federation::handshake_delivery::{self, ApprovalReply};
        if let Some(reply) = handshake_delivery::find(&self.config.data_dir, &req.id).await? {
            anyhow::ensure!(
                reply.recipient == req.from_nostr_pubkey && reply.expected_did == req.from_did,
                "Approval recipient changed"
            );
            return Ok(reply);
        }
        let (data, _) = self.state_manager.get_snapshot().await;
        let local_did = identity::did_key_from_pubkey_hex(&data.server_info.pubkey)?;
        let local_onion = data
            .server_info
            .tor_address
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Tor address not available"))?;
        let local_fips_npub = identity::fips_npub(&self.config.data_dir.join("identity"))
            .await
            .unwrap_or(None);
        let invite_code = federation::create_invite(
            &self.config.data_dir,
            &local_did,
            local_onion,
            &data.server_info.pubkey,
            local_fips_npub.as_deref(),
            TrustLevel::Observer,
        )
        .await?;
        handshake_delivery::stage(
            &self.config.data_dir,
            ApprovalReply {
                request_id: req.id.clone(),
                recipient: req.from_nostr_pubkey.clone(),
                expected_did: req.from_did.clone(),
                invite_code,
                attempts: 0,
                next_attempt: 0,
            },
        )
        .await
    }

    async fn deliver_peer_approval_reply(
        &self,
        reply: &federation::handshake_delivery::ApprovalReply,
    ) -> Result<bool> {
        let Some(claimed) = federation::handshake_delivery::claim(
            &self.config.data_dir,
            &reply.request_id,
            chrono::Utc::now().timestamp(),
        )
        .await?
        else {
            return Ok(false);
        };
        let result = nostr_handshake::send_peer_invite(
            &self.config.data_dir.join("identity"),
            &claimed.recipient,
            &claimed.invite_code,
            &self.handshake_relays().await,
            self.config.nostr_tor_proxy.as_deref(),
        )
        .await;
        if result.is_err() {
            warn!(request_id = %reply.request_id, "Peer approval delivery deferred; durable retry scheduled");
        }
        Ok(result.is_ok())
    }

    /// Recover relay loss and legacy approvals without changing trust or
    /// resurrecting a node that the operator explicitly removed.
    pub(in crate::api::rpc) async fn retry_peer_approval_replies(&self) -> Result<()> {
        let requests = pending::load_pending(&self.config.data_dir).await?;
        let nodes = federation::load_nodes(&self.config.data_dir).await?;
        let removed = federation::load_removed_dids(&self.config.data_dir).await?;
        let cutoff = chrono::Utc::now() - chrono::Duration::days(30);
        let mut sent = 0;
        for req in requests {
            if req.outbound || req.state != pending::PendingState::Approved {
                federation::handshake_delivery::remove(&self.config.data_dir, &req.id).await?;
                continue;
            }
            let expired = chrono::DateTime::parse_from_rfc3339(&req.received_at)
                .map(|time| time < cutoff)
                .unwrap_or(true);
            if expired
                || removed.contains(&req.from_did)
                || nodes.iter().any(|node| node.did == req.from_did)
            {
                federation::handshake_delivery::remove(&self.config.data_dir, &req.id).await?;
                continue;
            }
            if req.from_did.is_empty() || sent >= 4 {
                continue;
            }
            let reply = self.prepare_peer_approval_reply(&req).await?;
            if reply.next_attempt > chrono::Utc::now().timestamp() {
                continue;
            }
            self.deliver_peer_approval_reply(&reply).await?;
            sent += 1;
        }
        Ok(())
    }

    /// federation.reject-request — drop a pending request and, if requested,
    /// ship a NIP-44 `PeerReject` to the sender so their UI can update.
    pub(in crate::api::rpc) async fn handle_federation_reject_request(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?;
        let reason = params.get("reason").and_then(|v| v.as_str());
        let notify = params
            .get("notify")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let req = pending::find_by_id(&self.config.data_dir, id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Pending request not found: {}", id))?;
        if !matches!(req.state, pending::PendingState::Pending) || req.outbound {
            anyhow::bail!(
                "Pending request is not awaiting approval (state={:?})",
                req.state
            );
        }

        pending::decide(&self.config.data_dir, id, pending::PendingState::Rejected).await?;
        if notify {
            let identity_dir = self.config.data_dir.join("identity");
            let _ = nostr_handshake::send_peer_reject(
                &identity_dir,
                &req.from_nostr_pubkey,
                reason,
                &self.handshake_relays().await,
                self.config.nostr_tor_proxy.as_deref(),
            )
            .await;
        }

        info!(id = %id, from = %req.from_nostr_pubkey, "Rejected peer request");
        Ok(serde_json::json!({ "rejected": true, "id": id }))
    }

    /// federation.cancel-request — withdraw an outbound peer request we
    /// sent but haven't heard back on. The local row is deleted and,
    /// unless `notify=false`, a PeerCancel nostr DM is sent so the
    /// target drops their inbound pending row.
    pub(in crate::api::rpc) async fn handle_federation_cancel_request(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?;
        let reason = params.get("reason").and_then(|v| v.as_str());
        // Default TRUE — cancelling without notifying is a footgun (the
        // recipient's UI keeps showing an unanswerable request).
        let notify = params
            .get("notify")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let req = pending::find_by_id(&self.config.data_dir, id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Pending request not found: {}", id))?;
        if !req.outbound || !matches!(req.state, pending::PendingState::Sent) {
            anyhow::bail!(
                "Can only cancel outbound requests in Sent state (outbound={}, state={:?})",
                req.outbound,
                req.state
            );
        }

        if notify {
            let identity_dir = self.config.data_dir.join("identity");
            // Best-effort: log but don't fail the cancel if the nostr
            // relay is unreachable — the local row is still dropped.
            if let Err(e) = nostr_handshake::send_peer_cancel(
                &identity_dir,
                &req.from_nostr_pubkey,
                reason,
                &self.handshake_relays().await,
                self.config.nostr_tor_proxy.as_deref(),
            )
            .await
            {
                tracing::warn!(
                    id = %id,
                    error = %e,
                    "peer-cancel DM failed; local row dropped anyway"
                );
            }
        }

        pending::delete(&self.config.data_dir, id).await?;
        info!(id = %id, to = %req.from_nostr_pubkey, notified = notify, "Cancelled outbound peer request");
        Ok(serde_json::json!({ "cancelled": true, "id": id, "notified": notify }))
    }
}
