//! `security.app-gate-status` — what the app gate is actually enforcing.
//!
//! The gate rolls out per app (an app must be pinned to loopback before the
//! gate can claim its port — see `appgate::listener`), so for a while every
//! node is partially protected. "Partially" is only safe if it is *visible*:
//! this is the RPC that lets the UI say which app ports are still reachable
//! without a credential, instead of the operator having to port-scan their
//! own node to find out.

use anyhow::Result;

use super::RpcHandler;

impl RpcHandler {
    pub(in crate::api::rpc) async fn handle_app_gate_status(&self) -> Result<serde_json::Value> {
        let status = crate::appgate::listener::shared_status();
        let status = status.read().await.clone();
        let port_map = self.app_gate.port_map().await;

        // Exemptions are reported alongside, and with their manifest
        // rationale, because "which ports are open and why" is the actual
        // question — a list of unprotected ports without the deliberate ones
        // next to it invites someone to "fix" LND's gRPC port and break every
        // remote wallet.
        let exempt: Vec<serde_json::Value> = port_map
            .exempt_ports()
            .iter()
            .map(|e| {
                serde_json::json!({
                    "port": e.port,
                    "app_id": e.app_id,
                    "protocol": e.protocol,
                    "rationale": e.rationale,
                })
            })
            .collect();

        let gated: Vec<serde_json::Value> = port_map
            .gated_ports()
            .map(|g| {
                serde_json::json!({
                    "port": g.port,
                    "app_id": g.app_id,
                    "app_name": g.app_name,
                    // Is the login challenge active on this port right now
                    // (manifest default + operator override, resolved)?
                    "gate_enabled": g.auth_enabled,
                    // Whether an operator override is recorded, and what the
                    // manifest would do without it — the UI needs all three
                    // to render a meaningful toggle.
                    "override": crate::container::app_gate_config::gate_override(&g.app_id),
                })
            })
            .collect();

        Ok(serde_json::json!({
            // The headline. False means this node still has app ports that
            // answer without authentication.
            "fully_enforced": status.is_fully_enforced(),
            "claimed": status.claimed,
            "unprotected": status.unprotected,
            "gated": gated,
            "exempt": exempt,
        }))
    }

    /// `security.set-app-gate` — the operator's per-app gate toggle.
    ///
    /// Params: `{ id: "<app_id>", enabled: true | false | null }`.
    /// `enabled: null` clears the override so the manifest default applies
    /// again. Takes effect on the next request (the gate resolves per-request
    /// policy from the live port map) — no rebind, no restart.
    pub(in crate::api::rpc) async fn handle_set_app_gate(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let app_id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?
            .to_string();
        let enabled = match params.get("enabled") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::Bool(b)) => Some(*b),
            Some(other) => anyhow::bail!("enabled must be true, false or null, got {other}"),
        };

        // Only apps the gate actually fronts have a challenge to toggle.
        // Writing an override for anything else would sit silently in the
        // config doing nothing — reject instead so a typo'd id is loud.
        let port_map = self.app_gate.port_map().await;
        if !port_map.gated_ports().any(|g| g.app_id == app_id) {
            anyhow::bail!(
                "'{app_id}' has no gate-fronted ports — nothing to toggle \
                 (auth: none/local ports are manifest-declared, not runtime-toggled)"
            );
        }

        crate::container::app_gate_config::write_gate_override(&app_id, enabled)
            .map_err(|e| anyhow::anyhow!("Failed to persist gate override: {e}"))?;
        // Rebuild the port map now so the change is live on the next request
        // instead of after the next 60s sweep.
        self.app_gate.refresh().await;

        let effective: Vec<serde_json::Value> = self
            .app_gate
            .port_map()
            .await
            .gated_ports()
            .filter(|g| g.app_id == app_id)
            .map(|g| serde_json::json!({ "port": g.port, "gate_enabled": g.auth_enabled }))
            .collect();
        tracing::info!(
            app = %app_id,
            override_ = ?enabled,
            "app gate override updated by operator"
        );
        Ok(serde_json::json!({ "id": app_id, "override": enabled, "ports": effective }))
    }
}
