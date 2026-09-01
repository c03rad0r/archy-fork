//! RPC handlers for the FIPS mesh transport subsystem.
//!
//! Surface is deliberately thin: a read-only `fips.status`, a user-gated
//! `fips.check-update`, a stubbed `fips.apply-update`, and a
//! `fips.install` that (re-)materialises the daemon config + key and
//! activates the service. All writes go through `sudo` helpers in
//! `crate::fips`.

use super::RpcHandler;
use crate::fips;
use anyhow::Result;

impl RpcHandler {
    pub(super) async fn handle_fips_status(&self) -> Result<serde_json::Value> {
        let status = fips::FipsStatus::query(&self.config.data_dir).await;
        let mut v = serde_json::to_value(status)?;
        // Dial outcome counters (process-lifetime): how often peer dials
        // used FIPS vs fell back to Tor, broken down by reason. This is
        // the observability that makes "FIPS uptime" measurable.
        if let Some(obj) = v.as_object_mut() {
            obj.insert("dial_stats".to_string(), fips::telemetry::snapshot());
        }
        Ok(v)
    }

    /// Everything the companion app needs to join this node's mesh, embedded
    /// in the pairing QR by the web UI: the daemon's npub (identity to dial),
    /// the fips0 ULA (where the UI is reachable once the phone is meshed),
    /// and the transport ports on this host. The QR builder supplies the
    /// host/IP itself — it knows which origin the browser reached the node on.
    pub(super) async fn handle_fips_pair_info(&self) -> Result<serde_json::Value> {
        let identity_dir = fips::identity_dir_from(&self.config.data_dir);
        let npub = crate::identity::fips_npub(&identity_dir)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("FIPS identity not provisioned yet — complete onboarding first")
            })?;
        let ula = fips::iface::fips0_ula().map(|ip| ip.to_string());
        // The node's seed anchors ride along so the phone can rendezvous
        // through the same public mesh points when the node's LAN endpoint
        // isn't directly dialable (phone away from home, node behind NAT).
        let mut anchor_list = fips::anchors::load(&self.config.data_dir)
            .await
            .unwrap_or_default();
        // Pairing must always carry a public rendezvous point: without one the
        // phone is IP-bound to the LAN host it scanned and goes dark the
        // moment it leaves that network. This is a pairing hint only — the
        // node's own anchor file is not modified, so an operator's removal of
        // the default anchors still sticks for the node itself.
        if !anchor_list
            .iter()
            .any(|a| a.npub == fips::anchors::ARCHY_ANCHOR_NPUB)
        {
            anchor_list.push(fips::anchors::archy_anchor());
        }
        let anchors = anchor_list
            .into_iter()
            .map(|a| {
                serde_json::json!({
                    "npub": a.npub,
                    "addr": a.address,
                    "transport": a.transport,
                })
            })
            .collect::<Vec<_>>();
        Ok(serde_json::json!({
            "npub": npub,
            "ula": ula,
            "udp_port": fips::PUBLISHED_UDP_PORT,
            "tcp_port": fips::DEFAULT_TCP_PORT,
            "anchors": anchors,
        }))
    }

    pub(super) async fn handle_fips_check_update(&self) -> Result<serde_json::Value> {
        let check = fips::update::check().await?;
        Ok(serde_json::to_value(check)?)
    }

    pub(super) async fn handle_fips_apply_update(&self) -> Result<serde_json::Value> {
        fips::update::apply().await?;
        Ok(serde_json::json!({ "applied": true }))
    }

    /// Install config + key into /etc/fips and activate the service.
    /// Intended to be called:
    ///   - once by the seed-onboarding flow, right after the FIPS key
    ///     is written to /data/identity/fips_key, and
    ///   - on user demand from the dashboard if something drifted.
    pub(super) async fn handle_fips_install(&self) -> Result<serde_json::Value> {
        let identity_dir = fips::identity_dir_from(&self.config.data_dir);
        fips::config::install(&identity_dir).await?;
        let unit = fips::service::activation_unit().await;
        fips::service::activate(unit).await?;
        let status = fips::FipsStatus::query(&self.config.data_dir).await;
        Ok(serde_json::to_value(status)?)
    }

    /// Restart whichever fips unit is supervising the daemon on this host.
    /// Nodes installed from the archipelago ISO use `archipelago-fips.service`;
    /// nodes that had the upstream debian package set up first may only have
    /// `fips.service`. We resolve the active one via `service::active_unit()`
    /// so the UI button is never a no-op.
    pub(super) async fn handle_fips_restart(&self) -> Result<serde_json::Value> {
        let unit = fips::service::active_unit().await;
        fips::service::restart(unit).await?;
        Ok(serde_json::json!({ "restarted": true, "unit": unit }))
    }

    /// Full reconnect: stop the daemon, bring it back, wait for the DHT
    /// bootstrap window, poll the identity-cache + peer list, and
    /// classify what recovered (or didn't) so the UI can explain it to
    /// the user instead of showing a generic failure.
    ///
    /// Runtime: ~20s. Needs an RPC timeout ≥ 45s on the client.
    pub(super) async fn handle_fips_reconnect(&self) -> Result<serde_json::Value> {
        let identity_dir = fips::identity_dir_from(&self.config.data_dir);
        let before = fips::FipsStatus::query(&self.config.data_dir).await;

        // Heal the pre-fix bech32-text fips_key.pub → 32-raw-bytes
        // mismatch. The daemon silently authenticates with a garbage
        // pubkey when the .pub file is 63-char text, which looks like
        // "anchor unreachable" to the user even though the real fault
        // was an identity malformed on the node itself. Re-install the
        // config + keys so /etc/fips gets the healed .pub.
        let key_src = identity_dir.join("fips_key");
        let pub_src = identity_dir.join("fips_key.pub");
        if key_src.exists() {
            let _ = fips::config::normalize_pub_file(&key_src, &pub_src).await;
            // Re-install refreshes /etc/fips/fips.pub from the healed
            // source. No-op if nothing changed.
            let _ = fips::config::install(&identity_dir).await;
        }

        // Operate on whichever fips unit is actually up — nodes that
        // have the upstream `fips.service` rather than the
        // archipelago-managed `archipelago-fips.service` used to see
        // Reconnect silently fail because we stopped a unit that
        // didn't exist. Clean stop+start rather than `restart` so a
        // daemon that fails to come back up surfaces as
        // service_active=false instead of quietly sticking with the
        // old process.
        let unit = fips::service::active_unit().await;
        let _ = fips::service::stop(unit).await;
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        fips::service::activate(unit).await?;

        // Re-push seed anchors after restart so freshly-bound daemons
        // don't have to wait 5 min for the periodic apply loop.
        if let Ok(list) = fips::anchors::load(&self.config.data_dir).await {
            if !list.is_empty() {
                let _ = fips::anchors::apply(&list).await;
            }
        }

        // Anchor bootstrap window: poll the status every ~3s for up to
        // 20s. Bail as soon as the anchor is connected.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let after = loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let s = fips::FipsStatus::query(&self.config.data_dir).await;
            if s.anchor_connected || std::time::Instant::now() >= deadline {
                break s;
            }
        };

        let recovered = after.anchor_connected && !before.anchor_connected;
        let likely_cause = if after.anchor_connected {
            "connected"
        } else if !after.service_active {
            "daemon_down"
        } else if !after.key_present {
            "no_seed_key"
        } else if after.authenticated_peer_count == 0 {
            // Daemon is up with a key but hasn't authenticated any peers —
            // almost always the outbound connection to the anchor being
            // dropped by the local firewall/router, or the anchor itself
            // being down. The public anchor is reached over TCP/8443 (not
            // UDP/8668 — that endpoint is dead).
            "no_outbound_or_anchor_down"
        } else {
            "peers_but_no_anchor"
        };
        let hint = match likely_cause {
            "connected" => "An anchor is reachable.",
            "daemon_down" => "The FIPS daemon didn't come back up — check the FIPS service on this host.",
            "no_seed_key" => "No seed-derived FIPS key on disk. Re-run the onboarding unlock step.",
            "no_outbound_or_anchor_down" =>
                "Daemon is running but no peers handshook. Your router or ISP may be blocking the outbound connection to the mesh anchor (TCP port 8443), or every configured anchor is down. The public anchor is added automatically — if it still won't connect, add another reachable peer in Seed Anchors.",
            "peers_but_no_anchor" =>
                "Mesh has peers but none of them are anchors we recognise. Add your cluster's anchor in Seed Anchors.",
            _ => "",
        };

        Ok(serde_json::json!({
            "recovered": recovered,
            "likely_cause": likely_cause,
            "hint": hint,
            "before": before,
            "after": after,
        }))
    }

    /// List the seed-anchor entries configured on this node.
    pub(super) async fn handle_fips_list_seed_anchors(&self) -> Result<serde_json::Value> {
        let list = fips::anchors::load(&self.config.data_dir).await?;
        Ok(serde_json::json!({ "seed_anchors": list }))
    }

    /// Add (or update) a seed anchor and immediately push it into the
    /// running daemon. Params: `{ npub, address, transport?, label? }`.
    pub(super) async fn handle_fips_add_seed_anchor(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let anchor: fips::anchors::SeedAnchor = serde_json::from_value(params.clone())
            .map_err(|e| anyhow::anyhow!("bad seed anchor payload: {}", e))?;
        if !anchor.npub.starts_with("npub1") {
            anyhow::bail!("npub must be bech32 (npub1...)");
        }
        if !anchor.address.contains(':') {
            anyhow::bail!("address must be host:port (e.g. 192.0.2.12:8668)");
        }
        let list = fips::anchors::add(&self.config.data_dir, anchor.clone()).await?;
        // Push just the newly-added anchor into the running daemon so
        // the user sees effect without waiting for the periodic apply.
        let results = fips::anchors::apply(&[anchor]).await;
        Ok(serde_json::json!({
            "seed_anchors": list,
            "apply": results.iter().map(|r| {
                serde_json::json!({ "npub": r.npub, "ok": r.ok, "message": r.message })
            }).collect::<Vec<_>>(),
        }))
    }

    /// Remove a seed anchor by npub. Params: `{ npub }`. Does NOT tear
    /// down an already-authenticated peer connection — it only stops
    /// us from re-dialing the anchor on the next apply cycle.
    pub(super) async fn handle_fips_remove_seed_anchor(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let npub = params
            .get("npub")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing npub"))?;
        let list = fips::anchors::remove(&self.config.data_dir, npub).await?;
        Ok(serde_json::json!({ "seed_anchors": list }))
    }

    /// Re-apply all seed anchors to the running daemon. Useful after a
    /// FIPS restart or when the user wants to force a reconnection
    /// attempt without waiting for the periodic apply loop.
    pub(super) async fn handle_fips_apply_seed_anchors(&self) -> Result<serde_json::Value> {
        let list = fips::anchors::load(&self.config.data_dir).await?;
        let results = fips::anchors::apply(&list).await;
        Ok(serde_json::json!({
            "applied": results.len(),
            "results": results.iter().map(|r| {
                serde_json::json!({ "npub": r.npub, "ok": r.ok, "message": r.message })
            }).collect::<Vec<_>>(),
        }))
    }

    /// The SSH-over-mesh toggle state plus sshd preflights (the card explains
    /// the rule instead of gating on it — see ssh_mesh.rs).
    pub(super) async fn handle_fips_ssh_over_mesh_get(&self) -> Result<serde_json::Value> {
        let state = fips::ssh_mesh::load(&self.config.data_dir).await;
        let preflights = fips::ssh_mesh::preflights().await;
        Ok(serde_json::json!({
            "enabled": state.enabled,
            "sources": state.sources,
            "scope": if state.sources.is_empty() { "any" } else { "list" },
            "preflights": preflights,
        }))
    }

    /// Set the toggle. Params: `{ enabled: bool, sources?: string[] }` —
    /// an empty/absent source list opens port 22 to every mesh peer (the UI
    /// confirms that explicitly before calling with it).
    pub(super) async fn handle_fips_ssh_over_mesh_set(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let enabled = params
            .get("enabled")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("missing boolean 'enabled'"))?;
        let sources: Vec<String> = params
            .get("sources")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let (state, outcome) =
            fips::ssh_mesh::set(&self.config.data_dir, enabled, &sources).await?;
        let preflights = fips::ssh_mesh::preflights().await;
        Ok(serde_json::json!({
            "enabled": state.enabled,
            "sources": state.sources,
            "scope": if state.sources.is_empty() { "any" } else { "list" },
            "applied": outcome.applied,
            "removed": outcome.removed,
            "reloaded": outcome.reloaded,
            "preflights": preflights,
        }))
    }
}
