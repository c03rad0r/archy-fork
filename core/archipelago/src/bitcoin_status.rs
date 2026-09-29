//! Cached Bitcoin node status for browser UIs.
//!
//! The bitcoin-ui should not poll Bitcoin RPC directly for display state.
//! During container restarts, reindexing, and IBD, direct browser RPC polling
//! turns short RPC gaps into visible UI failures. This module owns the RPC
//! polling loop, caches the last successful snapshot, and serves stale-but-known
//! state while the node is reconnecting.

use anyhow::{Context, Result};
use serde::Serialize;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, warn};

// Poll frequently and recover fast so the cached snapshot tracks bitcoind's
// responsive windows during IBD. During heavy block-connection, getblockchaininfo
// can block briefly; a slow 10s/15s/20s cadence let one missed poll age the
// snapshot past the UI's 30s "stale" threshold, so the UI dwelled on
// "reconnecting…" long after bitcoind was answering again. Tight cadence + short
// timeout keeps last-known state fresh and clears the stale banner promptly.
const CACHE_REFRESH_SECS: u64 = 5;
const CACHE_ERROR_BACKOFF_SECS: u64 = 5;

// Grace window before a failing poll marks the snapshot "stale" for the UI.
// On a busy / swap-thrashing node (e.g. .198) getblockchaininfo intermittently
// exceeds the RPC timeout, so a single missed poll is normal and must NOT flip
// the UI to "reconnecting…". Only after the cached snapshot is genuinely old —
// several polls failed in a row — do we surface the banner.
const STALE_GRACE_MS: u64 = 20_000;

#[derive(Debug, Clone, Serialize)]
pub struct BitcoinNodeStatus {
    pub ok: bool,
    pub stale: bool,
    pub updated_at_ms: u64,
    // Server-computed age of the snapshot, filled in at serve time. The browser
    // must not derive this itself (Date.now() - updated_at_ms) because that
    // compares the browser clock against this node's clock — any skew made a
    // fresh snapshot look stale and the "reconnecting…" banner never cleared.
    pub age_ms: u64,
    pub error: Option<String>,
    pub blockchain_info: Option<serde_json::Value>,
    pub network_info: Option<serde_json::Value>,
    pub index_info: Option<serde_json::Value>,
    pub zmq_notifications: Option<serde_json::Value>,
}

impl Default for BitcoinNodeStatus {
    fn default() -> Self {
        Self {
            ok: false,
            stale: false,
            updated_at_ms: 0,
            age_ms: 0,
            error: Some("Connecting to Bitcoin node...".to_string()),
            blockchain_info: None,
            network_info: None,
            index_info: None,
            zmq_notifications: None,
        }
    }
}

static STATUS_CACHE: OnceLock<RwLock<BitcoinNodeStatus>> = OnceLock::new();

fn cache() -> &'static RwLock<BitcoinNodeStatus> {
    STATUS_CACHE.get_or_init(|| RwLock::new(BitcoinNodeStatus::default()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn transient_error(err_msg: &str) -> bool {
    let lower = err_msg.to_lowercase();
    lower.contains("connect")
        || lower.contains("reset")
        || lower.contains("refused")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("broken pipe")
        || lower.contains("eof")
        || lower.contains("500 internal server error")
        || lower.contains("503 service unavailable")
        || lower.contains("work queue depth exceeded")
        || lower.contains("decode bitcoin rpc json")
        || lower.contains("error decoding response body")
        || lower.contains("expected value at line 1 column 1")
}

fn friendly_transient_error(has_cached_state: bool, err_msg: &str) -> String {
    let detail = err_msg
        .lines()
        .next()
        .unwrap_or(err_msg)
        .trim()
        .trim_end_matches('.');
    let lower = detail.to_lowercase();
    let state = if lower.contains("loading block index") {
        Some("loading its block index. This can take a while after installation or restart")
    } else if lower.contains("replaying blocks") {
        Some("checking saved blocks before startup completes")
    } else if lower.contains("verifying blocks") {
        Some("verifying blocks after restart")
    } else if lower.contains("connection reset") {
        Some("starting up and not yet accepting RPC connections")
    } else if lower.contains("connection refused") || lower.contains("tcp connect error") {
        Some("waiting for the Bitcoin RPC listener")
    } else if lower.contains("timed out") || lower.contains("timeout") {
        Some("busy and not answering RPC before the timeout")
    } else {
        None
    };

    // Recognized transient causes get a clean human sentence only — the raw
    // transport error (URLs, repeated "os error 104" chains) is operator
    // noise that was ending up verbatim on the app card. Unrecognized errors
    // keep a bounded detail so a genuinely new failure stays diagnosable.
    let (state, detail) = match state {
        Some(state) => (state, None),
        None => (
            "starting or busy syncing",
            Some(if detail.len() > 120 {
                let mut cut = 120;
                while !detail.is_char_boundary(cut) {
                    cut -= 1;
                }
                format!("{}…", &detail[..cut])
            } else {
                detail.to_string()
            }),
        ),
    };

    let base = if has_cached_state {
        format!("Bitcoin node is {state}; showing last known state and retrying.")
    } else {
        format!("Bitcoin node is {state}; retrying automatically.")
    };
    match detail {
        Some(detail) => format!("{base} Detail: {detail}"),
        None => base,
    }
}

pub fn spawn_status_cache() {
    tokio::spawn(async {
        loop {
            let fresh = fetch_bitcoin_status().await;
            let mut cached = cache().write().await;
            let mut sleep_secs = CACHE_REFRESH_SECS;
            match fresh {
                Ok(mut status) => {
                    status.ok = true;
                    status.stale = false;
                    status.error = None;
                    *cached = status;
                }
                Err(e) => {
                    let err_msg = format!("{e:#}");
                    if transient_error(&err_msg) {
                        debug!("Bitcoin status: transient RPC failure: {}", err_msg);
                    } else {
                        warn!("Bitcoin status: RPC failure: {}", err_msg);
                    }
                    sleep_secs = CACHE_ERROR_BACKOFF_SECS;

                    if cached.blockchain_info.is_some() {
                        cached.ok = false;
                        // Only flip to "stale" once the last good snapshot is older
                        // than the grace window. A brief RPC gap on a busy node keeps
                        // showing last-known state silently instead of a banner flicker.
                        let snapshot_age_ms = now_ms().saturating_sub(cached.updated_at_ms);
                        cached.stale = snapshot_age_ms > STALE_GRACE_MS;
                        cached.error = Some(friendly_transient_error(true, &err_msg));
                    } else {
                        *cached = BitcoinNodeStatus {
                            ok: false,
                            stale: false,
                            updated_at_ms: now_ms(),
                            error: Some(friendly_transient_error(false, &err_msg)),
                            ..BitcoinNodeStatus::default()
                        };
                    }
                }
            }
            drop(cached);
            tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
        }
    });
}

pub async fn get_bitcoin_status() -> BitcoinNodeStatus {
    let mut status = cache().read().await.clone();
    // Compute age here (server clock only) so the browser never has to subtract
    // across clocks. A successful snapshot serves age_ms ≈ 0 → the UI clears the
    // "reconnecting…" banner on its very next poll regardless of browser-clock skew.
    if status.updated_at_ms > 0 {
        status.age_ms = now_ms().saturating_sub(status.updated_at_ms);
    }
    status
}

async fn fetch_bitcoin_status() -> Result<BitcoinNodeStatus> {
    // 12s (not 8s): on a swap-thrashing node getblockchaininfo can answer slowly
    // but correctly; too tight a timeout turned working-but-slow polls into
    // failures and tripped the "reconnecting…" banner. Stays under STALE_GRACE_MS.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .context("build Bitcoin status HTTP client")?;

    // Fetch all four calls concurrently: getblockchaininfo gates freshness, so a
    // slow auxiliary call (network/index/zmq) must not delay the snapshot or block
    // the next refresh. Only getblockchaininfo failing marks the status stale.
    let (blockchain_info, network_info, index_info, zmq_notifications) = tokio::join!(
        bitcoin_rpc_call(&client, "getblockchaininfo", serde_json::json!([])),
        bitcoin_rpc_call(&client, "getnetworkinfo", serde_json::json!([])),
        bitcoin_rpc_call(&client, "getindexinfo", serde_json::json!([])),
        bitcoin_rpc_call(&client, "getzmqnotifications", serde_json::json!([])),
    );
    let blockchain_info = blockchain_info.context("getblockchaininfo")?;

    Ok(BitcoinNodeStatus {
        ok: true,
        stale: false,
        updated_at_ms: now_ms(),
        age_ms: 0,
        error: None,
        blockchain_info: Some(blockchain_info),
        network_info: network_info.ok(),
        index_info: index_info.ok(),
        zmq_notifications: zmq_notifications.ok(),
    })
}

async fn bitcoin_rpc_call(
    client: &reqwest::Client,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    let (rpc_user, rpc_pass) = crate::bitcoin_rpc::bitcoin_rpc_credentials().await;
    let body = serde_json::json!({
        "jsonrpc": "1.0",
        "id": "bitcoin-status",
        "method": method,
        "params": params,
    });

    let resp = client
        .post(crate::constants::BITCOIN_RPC_URL)
        .basic_auth(rpc_user, Some(rpc_pass))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Bitcoin RPC request failed")?;

    let status = resp.status();
    let json: serde_json::Value = resp.json().await.context("decode Bitcoin RPC JSON")?;
    if !status.is_success() {
        anyhow::bail!("Bitcoin RPC returned {}: {}", status, json);
    }
    if let Some(error) = json.get("error").filter(|e| !e.is_null()) {
        anyhow::bail!("Bitcoin RPC {} error: {}", method, error);
    }
    json.get("result")
        .cloned()
        .context("missing Bitcoin RPC result")
}

#[cfg(test)]
mod tests {
    use super::friendly_transient_error;

    #[test]
    fn explains_verifying_blocks_without_generic_timeout_copy() {
        let msg = friendly_transient_error(
            false,
            r#"getblockchaininfo: Bitcoin RPC returned 500 Internal Server Error: {"error":{"code":-28,"message":"Verifying blocks..."}}"#,
        );

        assert!(msg.contains("verifying blocks after restart"));
        assert!(msg.contains("retrying automatically"));
    }

    #[test]
    fn explains_missing_rpc_listener() {
        let msg = friendly_transient_error(
            true,
            "getblockchaininfo: tcp connect error: Connection refused (os error 111)",
        );

        assert!(msg.contains("waiting for the Bitcoin RPC listener"));
        assert!(msg.contains("showing last known state"));
    }

    #[test]
    fn explains_rpc_timeout() {
        let msg = friendly_transient_error(
            false,
            "getblockchaininfo: Bitcoin RPC request failed: operation timed out",
        );

        assert!(msg.contains("busy and not answering RPC before the timeout"));
    }

    #[test]
    fn connection_reset_gets_clean_message_without_raw_detail() {
        // The exact string a fresh install showed on the app card: the raw
        // reqwest chain (URL + repeated "os error 104") must not surface.
        let msg = friendly_transient_error(
            false,
            "getblockchaininfo: Bitcoin RPC request failed: error sending request for url (http://127.0.0.1:8332/): connection error: Connection reset by peer (os error 104): connection error: Connection reset by peer (os error 104): Connection reset by peer (os error 104)",
        );

        assert!(msg.contains("starting up and not yet accepting RPC connections"));
        assert!(!msg.contains("os error"));
        assert!(!msg.contains("127.0.0.1"));
        assert!(!msg.contains("Detail:"));
    }

    #[test]
    fn recognized_causes_omit_detail_entirely() {
        for raw in [
            "x: Connection refused (os error 111)",
            "x: operation timed out",
            r#"x: {"error":{"code":-28,"message":"Verifying blocks..."}}"#,
        ] {
            let msg = friendly_transient_error(false, raw);
            assert!(!msg.contains("Detail:"), "leaked detail for: {raw}");
        }
    }

    #[test]
    fn unknown_errors_keep_bounded_detail() {
        let long = format!("weird new failure {}", "x".repeat(300));
        let msg = friendly_transient_error(false, &long);
        assert!(msg.contains("Detail: weird new failure"));
        assert!(msg.len() < 260);
    }
}

#[cfg(test)]
mod startup_message_tests {
    #[test]
    fn loading_block_index_is_explained_without_rpc_error_dump() {
        for cached in [false, true] {
            let message = super::friendly_transient_error(
                cached,
                r#"getblockchaininfo: Bitcoin RPC returned 500 Internal Server Error: {"error":{"code":-28,"message":"Loading block index…"}}"#,
            );
            assert!(message.contains("loading its block index"));
            for raw in ["500", "-28", "Detail:", "getblockchaininfo", "{", "RPC"] {
                assert!(!message.contains(raw));
            }
            assert_eq!(message.contains("last known state"), cached);
        }
    }
}
