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
}
