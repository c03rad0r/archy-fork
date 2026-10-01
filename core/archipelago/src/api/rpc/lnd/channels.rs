use crate::api::rpc::RpcHandler;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

use super::LND_REST_BASE_URL;

/// LND rejects RPCs with "server is still in the process of starting" for a
/// short window after wallet unlock (p2p/graph subsystems still loading).
/// Transient by design — match it so callers retry and, if it persists,
/// surface a calm notice instead of a scary failure. The exact phrase below
/// is what the frontend keys its softer (non-red) styling on.
fn lnd_still_starting(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("in the process of starting") || m.contains("server is still starting")
}

/// User-facing text for the still-starting state. Deliberately calm: this is
/// a "wait a moment", not an error.
const LND_STARTING_MSG: &str = "Your Lightning node is still finishing its startup — this \
     usually takes a minute or two after the node comes online. Please try again shortly.";

#[derive(Debug, Serialize)]
struct ChannelInfo {
    chan_id: String,
    remote_pubkey: String,
    capacity: i64,
    local_balance: i64,
    remote_balance: i64,
    active: bool,
    status: String,
    channel_point: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    closing_txid: String,
}

#[derive(Debug, Serialize)]
struct ChannelListResult {
    channels: Vec<ChannelInfo>,
    total_inbound: i64,
    total_outbound: i64,
}

#[derive(Debug, Deserialize)]
struct LndListChannelsResponse {
    channels: Option<Vec<LndChannel>>,
}

#[derive(Debug, Deserialize)]
struct LndChannel {
    chan_id: Option<String>,
    remote_pubkey: Option<String>,
    capacity: Option<String>,
    local_balance: Option<String>,
    remote_balance: Option<String>,
    active: Option<bool>,
    channel_point: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct LndPendingChannelsResponse {
    pending_open_channels: Option<Vec<LndPendingOpenChannel>>,
    // Cooperative closes waiting for their closing tx to confirm
    waiting_close_channels: Option<Vec<LndWaitingCloseChannel>>,
    // Force closes serving out their timelock
    pending_force_closing_channels: Option<Vec<LndForceClosingChannel>>,
}

#[derive(Debug, Deserialize)]
struct LndPendingOpenChannel {
    channel: Option<LndPendingChannel>,
}

#[derive(Debug, Deserialize)]
struct LndWaitingCloseChannel {
    channel: Option<LndPendingChannel>,
    closing_txid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LndForceClosingChannel {
    channel: Option<LndPendingChannel>,
    closing_txid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LndPendingChannel {
    remote_node_pub: Option<String>,
    capacity: Option<String>,
    local_balance: Option<String>,
    remote_balance: Option<String>,
    channel_point: Option<String>,
}

impl LndPendingChannel {
    fn into_channel_info(self, status: &str, closing_txid: Option<String>) -> ChannelInfo {
        let parse = |s: &Option<String>| s.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0);
        ChannelInfo {
            chan_id: String::new(),
            remote_pubkey: self.remote_node_pub.clone().unwrap_or_default(),
            capacity: parse(&self.capacity),
            local_balance: parse(&self.local_balance),
            remote_balance: parse(&self.remote_balance),
            active: false,
            status: status.into(),
            channel_point: self.channel_point.unwrap_or_default(),
            closing_txid: closing_txid.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct LndClosedChannelsResponse {
    channels: Option<Vec<LndClosedChannel>>,
}

#[derive(Debug, Deserialize)]
struct LndClosedChannel {
    chan_id: Option<String>,
    remote_pubkey: Option<String>,
    capacity: Option<String>,
    settled_balance: Option<String>,
    close_type: Option<String>,
    closing_tx_hash: Option<String>,
    channel_point: Option<String>,
    close_height: Option<i64>,
}

#[derive(Debug, Serialize)]
struct ClosedChannelInfo {
    chan_id: String,
    remote_pubkey: String,
    capacity: i64,
    settled_balance: i64,
    close_type: String,
    closing_tx_hash: String,
    channel_point: String,
    close_height: i64,
}

impl RpcHandler {
    pub(in crate::api::rpc) async fn handle_lnd_listchannels(&self) -> Result<serde_json::Value> {
        let (client, macaroon_hex) = self.lnd_client().await?;

        let channels_resp: LndListChannelsResponse = client
            .get(format!("{LND_REST_BASE_URL}/v1/channels"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .context("LND REST connection failed")?
            .json()
            .await
            .context("Failed to parse LND channels response")?;

        let pending_resp: LndPendingChannelsResponse = match client
            .get(format!("{LND_REST_BASE_URL}/v1/channels/pending"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
        {
            Ok(resp) => resp.json().await.unwrap_or_default(),
            Err(_) => LndPendingChannelsResponse::default(),
        };

        let channels: Vec<ChannelInfo> = channels_resp
            .channels
            .unwrap_or_default()
            .into_iter()
            .map(|ch| {
                let capacity: i64 = ch
                    .capacity
                    .as_deref()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let local: i64 = ch
                    .local_balance
                    .as_deref()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let remote: i64 = ch
                    .remote_balance
                    .as_deref()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                ChannelInfo {
                    chan_id: ch.chan_id.unwrap_or_default(),
                    remote_pubkey: ch.remote_pubkey.unwrap_or_default(),
                    capacity,
                    local_balance: local,
                    remote_balance: remote,
                    active: ch.active.unwrap_or(false),
                    status: if ch.active.unwrap_or(false) {
                        "active".into()
                    } else {
                        "inactive".into()
                    },
                    channel_point: ch.channel_point.unwrap_or_default(),
                    closing_txid: String::new(),
                }
            })
            .collect();

        let mut pending_channels: Vec<ChannelInfo> = Vec::new();
        for pch in pending_resp.pending_open_channels.unwrap_or_default() {
            if let Some(ch) = pch.channel {
                pending_channels.push(ch.into_channel_info("pending_open", None));
            }
        }
        for wch in pending_resp.waiting_close_channels.unwrap_or_default() {
            if let Some(ch) = wch.channel {
                pending_channels.push(ch.into_channel_info("closing", wch.closing_txid));
            }
        }
        for fch in pending_resp
            .pending_force_closing_channels
            .unwrap_or_default()
        {
            if let Some(ch) = fch.channel {
                pending_channels.push(ch.into_channel_info("force_closing", fch.closing_txid));
            }
        }

        let total_local: i64 = channels.iter().map(|c| c.local_balance).sum();
        let total_remote: i64 = channels.iter().map(|c| c.remote_balance).sum();

        let mut all_channels = channels;
        all_channels.extend(pending_channels);

        let result = ChannelListResult {
            channels: all_channels,
            total_inbound: total_remote,
            total_outbound: total_local,
        };

        Ok(serde_json::to_value(result)?)
    }

    pub(in crate::api::rpc) async fn handle_lnd_openchannel(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let pubkey = params
            .get("pubkey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'pubkey' parameter"))?;
        let amount = params
            .get("amount")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'amount' parameter (sats)"))?;

        // Validate pubkey: must be 66-char hex (compressed secp256k1)
        if pubkey.len() != 66 || !pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(anyhow::anyhow!(
                "Invalid pubkey: must be 66-character hex string"
            ));
        }

        if amount < 20000 {
            return Err(anyhow::anyhow!(
                "Channel amount must be at least 20,000 sats"
            ));
        }
        if amount > 16_777_215 {
            return Err(anyhow::anyhow!(
                "Channel amount exceeds maximum (16,777,215 sats)"
            ));
        }

        let private = params
            .get("private")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Fee control: either a confirmation target or an explicit fee rate
        let target_conf = params.get("target_conf").and_then(|v| v.as_i64());
        let sat_per_vbyte = params.get("sat_per_vbyte").and_then(|v| v.as_i64());
        if target_conf.is_some() && sat_per_vbyte.is_some() {
            return Err(anyhow::anyhow!(
                "Invalid fee parameters: specify either target_conf or sat_per_vbyte, not both"
            ));
        }
        if let Some(tc) = target_conf {
            if !(1..=1008).contains(&tc) {
                return Err(anyhow::anyhow!(
                    "Invalid target_conf: must be between 1 and 1008 blocks"
                ));
            }
        }
        if let Some(rate) = sat_per_vbyte {
            if !(1..=5000).contains(&rate) {
                return Err(anyhow::anyhow!(
                    "Invalid sat_per_vbyte: must be between 1 and 5000"
                ));
            }
        }

        info!(
            peer = pubkey,
            amount = amount,
            private = private,
            "Opening Lightning channel"
        );

        let (client, macaroon_hex) = self.lnd_client().await?;

        // First connect to the peer if an address is provided.
        // perm=false makes LND connect synchronously, so the peer is online
        // (or we get a real error) before we attempt the channel open.
        // perm=true queues the connection in the background and returns
        // immediately, which makes the subsequent open race and fail with
        // "peer is not online".
        if let Some(addr) = params.get("address").and_then(|v| v.as_str()) {
            // Validate peer address format (host:port)
            if addr.len() > 256 || addr.contains('\0') || addr.contains(' ') {
                return Err(anyhow::anyhow!("Invalid peer address format"));
            }
            let connect_body = serde_json::json!({
                "addr": { "pubkey": pubkey, "host": addr },
                "perm": false,
                "timeout": "30"
            });
            // Right after wallet unlock, LND's RPC answers while its p2p
            // server is still spinning up, and every connect attempt gets
            // "server is still in the process of starting". That's a
            // transient state, not a failure — it clears in seconds — so
            // retry quietly for ~30s before surfacing a calm, non-scary
            // notice (the frontend styles LND_STARTING_MSG as info, not red).
            let mut attempt = 0u32;
            loop {
                let connect_resp = client
                    .post(format!("{LND_REST_BASE_URL}/v1/peers"))
                    .header("Grpc-Metadata-macaroon", &macaroon_hex)
                    .json(&connect_body)
                    .timeout(std::time::Duration::from_secs(35))
                    .send()
                    .await
                    .context("Failed to connect to peer")?;

                if connect_resp.status().is_success() {
                    break;
                }
                let body: serde_json::Value = connect_resp.json().await.unwrap_or_default();
                let msg = body
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error");
                // LND returns an error if we already have this peer — that is fine
                if msg.contains("already connected") {
                    break;
                }
                if lnd_still_starting(msg) && attempt < 5 {
                    attempt += 1;
                    info!(attempt, "LND still starting — retrying peer connect in 6s");
                    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
                    continue;
                }
                if lnd_still_starting(msg) {
                    return Err(anyhow::anyhow!("{LND_STARTING_MSG}"));
                }
                return Err(anyhow::anyhow!("Failed to connect to peer: {}", msg));
            }
        }

        let mut open_body = serde_json::json!({
            "node_pubkey_string": pubkey,
            "local_funding_amount": amount.to_string(),
            "private": private,
        });
        if let Some(tc) = target_conf {
            open_body["target_conf"] = serde_json::json!(tc);
        }
        if let Some(rate) = sat_per_vbyte {
            // LND REST encodes uint64 as a JSON string
            open_body["sat_per_vbyte"] = serde_json::json!(rate.to_string());
        }

        let resp = client
            .post(format!("{LND_REST_BASE_URL}/v1/channels"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .json(&open_body)
            .send()
            .await
            .context("Failed to open channel")?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse open channel response")?;

        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            if lnd_still_starting(msg) {
                return Err(anyhow::anyhow!("{LND_STARTING_MSG}"));
            }
            return Err(anyhow::anyhow!("Failed to open channel: {}", msg));
        }

        Ok(body)
    }

    pub(in crate::api::rpc) async fn handle_lnd_closedchannels(&self) -> Result<serde_json::Value> {
        let (client, macaroon_hex) = self.lnd_client().await?;

        let resp: LndClosedChannelsResponse = client
            .get(format!("{LND_REST_BASE_URL}/v1/channels/closed"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .context("LND REST connection failed")?
            .json()
            .await
            .context("Failed to parse LND closed channels response")?;

        let channels: Vec<ClosedChannelInfo> = resp
            .channels
            .unwrap_or_default()
            .into_iter()
            .map(|ch| ClosedChannelInfo {
                chan_id: ch.chan_id.unwrap_or_default(),
                remote_pubkey: ch.remote_pubkey.unwrap_or_default(),
                capacity: ch
                    .capacity
                    .as_deref()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
                settled_balance: ch
                    .settled_balance
                    .as_deref()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
                close_type: ch.close_type.unwrap_or_default(),
                closing_tx_hash: ch.closing_tx_hash.unwrap_or_default(),
                channel_point: ch.channel_point.unwrap_or_default(),
                close_height: ch.close_height.unwrap_or(0),
            })
            .collect();

        Ok(serde_json::json!({ "channels": channels }))
    }

    pub(in crate::api::rpc) async fn handle_lnd_closechannel(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let channel_point = params
            .get("channel_point")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                anyhow::anyhow!("Missing 'channel_point' parameter (txid:output_index)")
            })?;

        let parts: Vec<&str> = channel_point.split(':').collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!(
                "Invalid channel_point format. Expected 'txid:output_index'"
            ));
        }
        // Validate txid is 64-char hex and output_index is numeric
        if parts[0].len() != 64 || !parts[0].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(anyhow::anyhow!(
                "Invalid txid in channel_point: must be 64-character hex"
            ));
        }
        if parts[1].parse::<u32>().is_err() {
            return Err(anyhow::anyhow!(
                "Invalid output_index in channel_point: must be a number"
            ));
        }

        let fee_query = close_channel_fee_query(&params)?;
        let force = params
            .get("force")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        info!(
            channel_point = channel_point,
            force = force,
            "Closing Lightning channel"
        );

        let (_, macaroon_hex) = self.lnd_client().await?;

        // The close endpoint is server-streaming: LND holds the connection
        // open and emits updates until the closing tx CONFIRMS on-chain
        // (potentially hours). Reading the whole body hangs the RPC even
        // though the close already went through, and the shared lnd_client's
        // 15s total timeout would abort the stream mid-read. Use a dedicated
        // client and return as soon as the first streamed update arrives.
        let client = reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(std::time::Duration::from_secs(10))
            .danger_accept_invalid_certs(true)
            .build()
            .context("Failed to create streaming HTTP client")?;

        let url = format!("{LND_REST_BASE_URL}/v1/channels/{}/{}", parts[0], parts[1]);

        let mut resp = client
            .delete(&url)
            .query(&fee_query)
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .context("Failed to close channel")?;

        if !resp.status().is_success() {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("Failed to close channel: {}", msg));
        }

        // First streamed line is {"result":{"close_pending":…}} on success or
        // {"error":…} — the stream reports errors in-band after a 200.
        let mut buf: Vec<u8> = Vec::new();
        let first_update = tokio::time::timeout(std::time::Duration::from_secs(25), async {
            while let Some(chunk) = resp.chunk().await? {
                buf.extend_from_slice(&chunk);
                let line = match buf.iter().position(|&b| b == b'\n') {
                    Some(pos) => &buf[..pos],
                    None => &buf[..],
                };
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(line) {
                    return Ok::<_, anyhow::Error>(Some(v));
                }
            }
            Ok(None)
        })
        .await;

        match first_update {
            Ok(Ok(Some(update))) => {
                if let Some(err) = update.get("error") {
                    let msg = err
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown error");
                    return Err(anyhow::anyhow!("Failed to close channel: {}", msg));
                }
                // txid arrives base64-encoded in internal byte order; flip it
                // into the display order explorers use.
                use base64::Engine as _;
                let closing_txid = update
                    .pointer("/result/close_pending/txid")
                    .and_then(|v| v.as_str())
                    .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok())
                    .map(|mut bytes| {
                        bytes.reverse();
                        hex::encode(bytes)
                    })
                    .unwrap_or_default();
                info!(channel_point, closing_txid, "Channel close initiated");
                Ok(serde_json::json!({ "success": true, "closing_txid": closing_txid }))
            }
            Ok(Ok(None)) => Err(anyhow::anyhow!(
                "LND ended the close stream without an update — check the channel list"
            )),
            Ok(Err(e)) => Err(e).context("Failed reading close channel response"),
            // No update inside the window: the close is almost certainly still
            // negotiating with the peer — report initiated, the channel list
            // will show it under Closing.
            Err(_) => Ok(serde_json::json!({ "success": true, "closing_txid": "" })),
        }
    }
}

/// LND's CloseChannel REST endpoint takes fee selection as query parameters.
/// With neither parameter LND uses a lax target; keep legacy clients on our
/// explicit Standard target rather than silently accepting that default.
fn close_channel_fee_query(params: &serde_json::Value) -> Result<Vec<(&'static str, String)>> {
    let force = match params.get("force") {
        None | Some(serde_json::Value::Null) => false,
        Some(value) => value
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("force must be a boolean"))?,
    };
    let integer = |key: &str, max: u64| -> Result<Option<u64>> {
        match params.get(key) {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(value) => {
                let n = value
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("{key} must be a positive whole number"))?;
                anyhow::ensure!((1..=max).contains(&n), "{key} must be between 1 and {max}");
                Ok(Some(n))
            }
        }
    };
    let target = integer("target_conf", 1008)?;
    let rate = integer("sat_per_vbyte", 5000)?;
    anyhow::ensure!(
        target.is_none() || rate.is_none(),
        "Specify either target_conf or sat_per_vbyte, not both"
    );
    anyhow::ensure!(
        !force || (target.is_none() && rate.is_none()),
        "Closing fee selection requires a cooperative close"
    );
    let mut query = vec![("force", force.to_string())];
    if !force {
        if let Some(rate) = rate {
            query.push(("sat_per_vbyte", rate.to_string()));
        } else {
            query.push(("target_conf", target.unwrap_or(6).to_string()));
        }
    }
    Ok(query)
}

#[cfg(test)]
mod close_fee_tests {
    use super::*;
    #[test]
    fn close_fee_query_forwards_presets_custom_and_legacy_default() {
        for target in [1, 3, 6, 1008] {
            assert_eq!(
                close_channel_fee_query(&serde_json::json!({"target_conf":target})).unwrap(),
                vec![
                    ("force", "false".into()),
                    ("target_conf", target.to_string())
                ]
            );
        }
        for rate in [1, 25, 5000] {
            let query =
                close_channel_fee_query(&serde_json::json!({"sat_per_vbyte":rate})).unwrap();
            let request = reqwest::Client::new()
                .delete("http://localhost/v1/channels/test/0")
                .query(&query)
                .build()
                .unwrap();
            assert_eq!(request.method(), reqwest::Method::DELETE);
            assert_eq!(
                request.url().query(),
                Some(format!("force=false&sat_per_vbyte={rate}").as_str())
            );
        }
        assert_eq!(
            close_channel_fee_query(&serde_json::json!({})).unwrap(),
            vec![("force", "false".into()), ("target_conf", "6".into())]
        );
        assert_eq!(
            close_channel_fee_query(&serde_json::json!({"force":true})).unwrap(),
            vec![("force", "true".into())]
        );
    }
    #[test]
    fn malformed_or_conflicting_close_fees_fail_before_wallet_access() {
        for params in [
            serde_json::json!({"target_conf":1,"sat_per_vbyte":2}),
            serde_json::json!({"force":true,"target_conf":1}),
            serde_json::json!({"force":"false"}),
            serde_json::json!({"target_conf":0}),
            serde_json::json!({"target_conf":1009}),
            serde_json::json!({"sat_per_vbyte":5001}),
            serde_json::json!({"sat_per_vbyte":-1}),
            serde_json::json!({"sat_per_vbyte":1.5}),
            serde_json::json!({"sat_per_vbyte":"25"}),
        ] {
            assert!(close_channel_fee_query(&params).is_err(), "{params}");
        }
    }
}
