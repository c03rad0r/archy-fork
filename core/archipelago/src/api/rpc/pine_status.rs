//! Node-status JSON for the Pine voice stack (`GET /api/pine/status`).
//!
//! Two tiers in one endpoint:
//! - **Public** (no credentials): version, uptime, bitcoin height / sync /
//!   peer count, mesh peer count. Feeds the Pine launcher page's live status
//!   card — nothing here a LAN visitor couldn't already infer from the
//!   existing unauthenticated `/bitcoin-status`.
//! - **Token** (`Authorization: Bearer <pine-status-token>`): adds Lightning
//!   balances and the most recent received mesh text message. Feeds the
//!   Home Assistant REST sensors seeded by `package::pine_ha` — the seeder
//!   mints the token into `data_dir/secrets/pine-status-token` (0600) and
//!   embeds it in HA's configuration, so only HA (and the node owner) can
//!   read balances or message text.
//!
//! Replaces the interim per-node socat forwarder + bitcoind-RPC-credentials-
//! in-configuration.yaml stopgap used before this endpoint existed.

use super::RpcHandler;
use crate::mesh::types::MessageDirection;
use serde_json::{json, Value};

/// File under `data_dir/secrets/` holding the bearer token that unlocks the
/// sensitive tier. Written by the pine/HA seeder, read per-request here.
pub(crate) const PINE_STATUS_TOKEN_FILE: &str = "pine-status-token";

/// Constant-time-ish equality — avoids early-exit timing on the token compare.
fn token_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

impl RpcHandler {
    /// True when `presented` matches the on-disk pine status token. Missing or
    /// empty token file means the sensitive tier is locked (seeder not run).
    pub(crate) async fn pine_status_token_ok(&self, presented: &str) -> bool {
        let path = self
            .config
            .data_dir
            .join("secrets")
            .join(PINE_STATUS_TOKEN_FILE);
        match tokio::fs::read_to_string(&path).await {
            Ok(tok) => {
                let tok = tok.trim();
                !tok.is_empty() && token_eq(tok, presented.trim())
            }
            Err(_) => false,
        }
    }

    /// Assemble the status document. `authorized` selects the token tier.
    /// Every sub-source is best-effort: a dead bitcoind/LND/mesh never turns
    /// the endpoint into an error — the field just reports what it can.
    pub(crate) async fn pine_status_json(&self, authorized: bool) -> Value {
        let bs = crate::bitcoin_status::get_bitcoin_status().await;
        let bitcoin = {
            let info = bs.blockchain_info.as_ref();
            let height = info.and_then(|i| i.get("blocks")).and_then(Value::as_u64);
            let progress = info
                .and_then(|i| i.get("verificationprogress"))
                .and_then(Value::as_f64);
            let ibd = info
                .and_then(|i| i.get("initialblockdownload"))
                .and_then(Value::as_bool);
            let peers = bs
                .network_info
                .as_ref()
                .and_then(|n| n.get("connections"))
                .and_then(Value::as_u64);
            json!({
                "ok": bs.ok,
                "height": height,
                "sync_percent": progress.map(|p| (p * 10000.0).round() / 100.0),
                "ibd": ibd,
                "peers": peers,
            })
        };

        let (mesh, mesh_message) = {
            let guard = self.mesh_service.read().await;
            match guard.as_ref() {
                Some(svc) => {
                    let status = svc.status().await;
                    let latest = if authorized {
                        svc.messages(None)
                            .await
                            .iter()
                            .rev()
                            .find(|m| {
                                m.direction == MessageDirection::Received
                                    && m.message_type == "text"
                            })
                            .map(|m| {
                                json!({
                                    "id": m.id,
                                    "from": m.peer_name.clone()
                                        .unwrap_or_else(|| format!("contact {}", m.peer_contact_id)),
                                    "text": m.plaintext,
                                    "timestamp": m.timestamp,
                                })
                            })
                    } else {
                        None
                    };
                    (
                        json!({ "enabled": status.enabled, "peers": status.peer_count }),
                        latest,
                    )
                }
                None => (json!({ "enabled": false, "peers": 0 }), None),
            }
        };

        let lightning = if authorized {
            match self.handle_lnd_getinfo().await {
                Ok(info) => json!({
                    "balance_sats": info.get("balance_sats"),
                    "channel_balance_sats": info.get("channel_balance_sats"),
                    "active_channels": info.get("num_active_channels"),
                    "synced_to_chain": info.get("synced_to_chain"),
                }),
                Err(_) => Value::Null,
            }
        } else {
            Value::Null
        };

        json!({
            "ok": true,
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_seconds": crate::crash_recovery::uptime_seconds(),
            "bitcoin": bitcoin,
            "mesh": mesh,
            "lightning": lightning,
            // Always an object: HA's REST sensor reads attributes via
            // json_attributes_path "$.mesh_message", and a null there makes
            // HA log a "JSON result was not a dictionary" warning every scan.
            "mesh_message": mesh_message.unwrap_or_else(|| json!({})),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::token_eq;

    #[test]
    fn token_eq_matches_only_exact() {
        assert!(token_eq("abc123", "abc123"));
        assert!(!token_eq("abc123", "abc124"));
        assert!(!token_eq("abc123", "abc12"));
        assert!(!token_eq("", "x"));
        assert!(token_eq("", ""));
    }
}
