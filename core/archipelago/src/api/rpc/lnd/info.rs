use crate::api::rpc::RpcHandler;
use anyhow::{Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};

use super::{read_lnd_admin_macaroon, LndAmount, LndBalanceResponse, LND_REST_BASE_URL};

#[derive(Debug, Serialize)]
struct LndInfo {
    alias: String,
    num_active_channels: u32,
    num_peers: u32,
    synced_to_chain: bool,
    block_height: u64,
    balance_sats: i64,
    channel_balance_sats: i64,
    pending_open_balance: i64,
    /// This node's Lightning identity pubkey, or `None` when LND did not
    /// report one or reported one that is not a compressed secp256k1 key.
    /// Never fabricated: the caller can tell "not available" from "available".
    identity_pubkey: Option<String>,
    /// The connection URIs LND advertises for this node (`pubkey@host:port`).
    /// Empty when LND advertises none — an honest absence, not a placeholder.
    uris: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct LndGetInfoResponse {
    alias: Option<String>,
    num_active_channels: Option<u32>,
    num_peers: Option<u32>,
    synced_to_chain: Option<bool>,
    block_height: Option<u64>,
    #[serde(default)]
    identity_pubkey: Option<String>,
    #[serde(default)]
    uris: Vec<String>,
}

/// A compressed secp256k1 pubkey is 66 hexadecimal characters. Mirrors the
/// check `handle_lnd_openchannel` performs before dialling a peer, so a key
/// this function passes is one that handler would accept.
fn is_valid_identity_pubkey(pubkey: &str) -> bool {
    pubkey.len() == 66 && pubkey.chars().all(|c| c.is_ascii_hexdigit())
}

/// Map LND's reported identity onto the RPC response.
///
/// Split out from the HTTP flow so it is testable without a live LND. A
/// malformed pubkey yields `None` rather than propagating a key that
/// `lnd.openchannel` would later reject — surfacing the problem here, where
/// the operator is reading their own node's identity, beats surfacing it at
/// the moment they try to open a channel.
fn map_identity(get_info: &LndGetInfoResponse) -> (Option<String>, Vec<String>) {
    let identity_pubkey = match get_info.identity_pubkey.as_deref() {
        Some(pubkey) if is_valid_identity_pubkey(pubkey) => Some(pubkey.to_string()),
        Some(bad) => {
            tracing::warn!(
                len = bad.len(),
                "LND getinfo returned an identity_pubkey that is not 66 hex characters — \
                 reporting no identity rather than a key lnd.openchannel would reject"
            );
            None
        }
        None => None,
    };
    (identity_pubkey, get_info.uris.clone())
}

#[derive(Debug, Deserialize)]
struct LndChannelBalanceResponse {
    local_balance: Option<LndAmount>,
    pending_open_local_balance: Option<LndAmount>,
}

impl RpcHandler {
    pub(in crate::api::rpc) async fn handle_lnd_getinfo(&self) -> Result<serde_json::Value> {
        let macaroon_bytes = read_lnd_admin_macaroon().await?;
        let macaroon_hex = hex::encode(&macaroon_bytes);

        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(10))
            .danger_accept_invalid_certs(true)
            .build()
            .context("Failed to create HTTP client")?;

        let get_info: LndGetInfoResponse = client
            .get(format!("{LND_REST_BASE_URL}/v1/getinfo"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .context("LND REST connection failed")?
            .json()
            .await
            .context("Failed to parse LND getinfo response")?;

        let channel_balance: LndChannelBalanceResponse = match client
            .get(format!("{LND_REST_BASE_URL}/v1/balance/channels"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
        {
            Ok(resp) => resp.json().await.unwrap_or(LndChannelBalanceResponse {
                local_balance: None,
                pending_open_local_balance: None,
            }),
            Err(_) => LndChannelBalanceResponse {
                local_balance: None,
                pending_open_local_balance: None,
            },
        };

        let wallet_balance: LndBalanceResponse = match client
            .get(format!("{LND_REST_BASE_URL}/v1/balance/blockchain"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
        {
            Ok(resp) => resp.json().await.unwrap_or(LndBalanceResponse {
                total_balance: None,
            }),
            Err(_) => LndBalanceResponse {
                total_balance: None,
            },
        };

        let (identity_pubkey, uris) = map_identity(&get_info);

        let info = LndInfo {
            identity_pubkey,
            uris,
            alias: get_info.alias.unwrap_or_default(),
            num_active_channels: get_info.num_active_channels.unwrap_or(0),
            num_peers: get_info.num_peers.unwrap_or(0),
            synced_to_chain: get_info.synced_to_chain.unwrap_or(false),
            block_height: get_info.block_height.unwrap_or(0),
            balance_sats: wallet_balance
                .total_balance
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            channel_balance_sats: channel_balance
                .local_balance
                .and_then(|a| a.sat.and_then(|s| s.parse().ok()))
                .unwrap_or(0),
            pending_open_balance: channel_balance
                .pending_open_local_balance
                .and_then(|a| a.sat.and_then(|s| s.parse().ok()))
                .unwrap_or(0),
        };

        Ok(serde_json::to_value(info)?)
    }

    /// Return LND connection info: base64url-encoded TLS cert and admin macaroon
    /// for building lndconnect:// URIs in the frontend.
    pub(crate) async fn handle_lnd_connect_info(&self) -> Result<serde_json::Value> {
        let cert_path = "/var/lib/archipelago/lnd/tls.cert";

        // Read and encode TLS cert (PEM -> DER -> base64url)
        let cert_pem = tokio::fs::read_to_string(cert_path)
            .await
            .context("Failed to read LND TLS certificate")?;
        let cert_der_b64: String = cert_pem
            .lines()
            .filter(|l| !l.starts_with("-----"))
            .collect();
        let cert_der = base64::engine::general_purpose::STANDARD
            .decode(&cert_der_b64)
            .context("Failed to decode PEM base64")?;
        let cert_b64url = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&cert_der);

        // Read and encode macaroon (binary -> base64url)
        let macaroon_bytes = read_lnd_admin_macaroon().await?;
        let macaroon_b64url =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&macaroon_bytes);

        // Read Tor onion address -- check system Tor path first, then legacy
        let tor_onion = {
            let mut onion = None;
            for path in &[
                "/var/lib/archipelago/tor-hostnames/lnd",
                "/var/lib/tor/hidden_service_lnd/hostname",
                "/var/lib/archipelago/tor/hidden_service_lnd/hostname",
            ] {
                if let Ok(addr) = tokio::fs::read_to_string(path).await {
                    let addr = addr.trim().to_string();
                    if addr.ends_with(".onion") {
                        onion = Some(addr);
                        break;
                    }
                }
                // Try sudo for system Tor dirs (owned by debian-tor, 0700)
                if let Ok(output) = tokio::process::Command::new("sudo")
                    .args(["cat", path])
                    .output()
                    .await
                {
                    if output.status.success() {
                        let addr = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        if addr.ends_with(".onion") {
                            onion = Some(addr);
                            break;
                        }
                    }
                }
            }
            onion
        };

        Ok(serde_json::json!({
            "cert_base64url": cert_b64url,
            "macaroon_base64url": macaroon_b64url,
            "tor_onion": tor_onion,
            "rest_port": 18080,
            "grpc_port": 10009,
        }))
    }

    /// lnd.export-channel-backup -- Export all channel static backups (SCB).
    /// Returns base64-encoded multi-channel backup that can restore channels on a new node.
    pub(in crate::api::rpc) async fn handle_lnd_export_channel_backup(
        &self,
    ) -> Result<serde_json::Value> {
        let macaroon_bytes = read_lnd_admin_macaroon().await?;
        let macaroon_hex = hex::encode(&macaroon_bytes);

        let client = reqwest::Client::builder()
            .no_proxy()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client")?;

        let resp = client
            .get(format!("{LND_REST_BASE_URL}/v1/channels/backup"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .context("Failed to reach LND REST API")?;

        if !resp.status().is_success() {
            anyhow::bail!("LND returned {}", resp.status());
        }

        let data: serde_json::Value = resp.json().await.context("Invalid JSON from LND")?;

        // Extract the multi_chan_backup bytes
        let backup_b64 = data
            .get("multi_chan_backup")
            .and_then(|m| m.get("multi_chan_backup"))
            .and_then(|b| b.as_str())
            .unwrap_or("");

        Ok(serde_json::json!({
            "backup": backup_b64,
            "channel_count": data.get("multi_chan_backup")
                .and_then(|m| m.get("chan_points"))
                .and_then(|c| c.as_array())
                .map(|a| a.len())
                .unwrap_or(0),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real compressed secp256k1 pubkey shape: 66 hex characters.
    const GOOD_PUBKEY: &str = "03a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";

    fn parse(body: &str) -> LndGetInfoResponse {
        serde_json::from_str(body).expect("LND getinfo body must deserialize")
    }

    #[test]
    fn full_body_yields_identity_and_uris() {
        let parsed = parse(&format!(
            r#"{{"alias":"archy","identity_pubkey":"{GOOD_PUBKEY}",
                "uris":["{GOOD_PUBKEY}@1.2.3.4:9735","{GOOD_PUBKEY}@abcd.onion:9735"]}}"#
        ));
        let (pubkey, uris) = map_identity(&parsed);

        assert_eq!(pubkey.as_deref(), Some(GOOD_PUBKEY));
        assert_eq!(
            uris.len(),
            2,
            "both advertised URIs must survive the mapping"
        );
        assert!(uris[0].starts_with(GOOD_PUBKEY));
    }

    #[test]
    fn absent_fields_yield_honest_absence_not_a_fabricated_identity() {
        // The pre-existing fields must still deserialize with the new ones absent —
        // this is the body every node running an older LND build returns.
        let parsed = parse(r#"{"alias":"archy","num_peers":3,"synced_to_chain":true}"#);
        let (pubkey, uris) = map_identity(&parsed);

        assert!(pubkey.is_none(), "must not invent an identity");
        assert!(uris.is_empty(), "must not invent a URI");
        assert_eq!(parsed.alias.as_deref(), Some("archy"));
        assert_eq!(parsed.num_peers, Some(3));
    }

    #[test]
    fn malformed_pubkey_is_dropped_rather_than_propagated() {
        // Too short, non-hex, and empty must all be refused. Propagating any of
        // them would push the failure to lnd.openchannel, far from the cause.
        for bad in ["deadbeef", "", &"z".repeat(66), &GOOD_PUBKEY[..65]] {
            let parsed = parse(&format!(r#"{{"identity_pubkey":"{bad}"}}"#));
            let (pubkey, _) = map_identity(&parsed);
            assert!(
                pubkey.is_none(),
                "malformed pubkey {bad:?} must map to None, not be forwarded"
            );
        }
    }

    #[test]
    fn a_malformed_pubkey_does_not_discard_the_advertised_uris() {
        // The two facts are independent: a bad identity must not silently cost
        // the caller the URI list, which is the datum the picker actually needs.
        let parsed = parse(&format!(
            r#"{{"identity_pubkey":"nope","uris":["{GOOD_PUBKEY}@1.2.3.4:9735"]}}"#
        ));
        let (pubkey, uris) = map_identity(&parsed);

        assert!(pubkey.is_none());
        assert_eq!(uris.len(), 1);
    }

    #[test]
    fn valid_pubkey_shape_matches_the_openchannel_rule() {
        assert!(is_valid_identity_pubkey(GOOD_PUBKEY));
        assert!(is_valid_identity_pubkey(&"0".repeat(66)));
        assert!(!is_valid_identity_pubkey(&"0".repeat(65)));
        assert!(!is_valid_identity_pubkey(&"0".repeat(67)));
        assert!(!is_valid_identity_pubkey(&"g".repeat(66)));
    }
}
