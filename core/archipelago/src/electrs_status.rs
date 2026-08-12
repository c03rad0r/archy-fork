//! ElectrumX sync status: fetches indexed height from Electrum RPC and network height from Bitcoin Core.
//! Status is cached and refreshed on a background timer to avoid race conditions from concurrent queries.

use anyhow::{Context, Result};
use serde::Serialize;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tracing::warn;

const ELECTRUMX_HOST: &str = "127.0.0.1";
const ELECTRUMX_PORT: u16 = 50001;
const ELECTRUMX_DATA_DIR: &str = "/var/lib/archipelago/electrumx";
// Approximate final index size in bytes for mainnet (~130GB for ElectrumX full index as of 2026)
const ESTIMATED_FULL_INDEX_BYTES: f64 = 130_000_000_000.0;

/// Refresh interval for status cache
const CACHE_REFRESH_SECS: u64 = 30;
const CACHE_ERROR_BACKOFF_SECS: u64 = 60;

/// Build Bitcoin RPC Basic auth header using shared credentials.
async fn bitcoin_rpc_auth() -> String {
    let (user, pass) = crate::bitcoin_rpc::bitcoin_rpc_credentials().await;
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", user, pass));
    format!("Basic {}", encoded)
}

#[derive(Debug, Clone, Serialize)]
pub struct ElectrsSyncStatus {
    pub indexed_height: u64,
    pub bitcoin_height: u64,
    pub network_height: u64,
    pub progress_pct: f64,
    pub status: String,
    pub stale: bool,
    pub error: Option<String>,
    /// Index data size in human-readable format (e.g. "11.2 GB")
    pub index_size: Option<String>,
    /// Tor onion address for ElectrumX (if available)
    pub tor_onion: Option<String>,
}

impl Default for ElectrsSyncStatus {
    fn default() -> Self {
        Self {
            indexed_height: 0,
            bitcoin_height: 0,
            network_height: 0,
            progress_pct: 0.0,
            status: "starting".to_string(),
            stale: false,
            error: None,
            index_size: None,
            tor_onion: None,
        }
    }
}

/// Cached status, initialized once and refreshed by background task.
static STATUS_CACHE: OnceLock<RwLock<ElectrsSyncStatus>> = OnceLock::new();

fn cache() -> &'static RwLock<ElectrsSyncStatus> {
    STATUS_CACHE.get_or_init(|| RwLock::new(ElectrsSyncStatus::default()))
}

/// Spawn background task that refreshes ElectrumX status every CACHE_REFRESH_SECS.
pub fn spawn_status_cache() {
    tokio::spawn(async {
        loop {
            let mut fresh = fetch_electrs_sync_status().await;
            let sleep_secs = if fresh.status == "waiting" && fresh.bitcoin_height == 0 {
                CACHE_ERROR_BACKOFF_SECS
            } else {
                CACHE_REFRESH_SECS
            };
            let mut cached = cache().write().await;
            if fresh.indexed_height == 0
                && cached.indexed_height > 0
                && matches!(fresh.status.as_str(), "indexing" | "waiting")
            {
                fresh.indexed_height = cached.indexed_height;
                if fresh.network_height == 0 {
                    fresh.network_height = cached.network_height;
                }
                if fresh.bitcoin_height == 0 {
                    fresh.bitcoin_height = cached.bitcoin_height;
                }
                if fresh.progress_pct <= 0.0 {
                    fresh.progress_pct = cached.progress_pct;
                }
                fresh.stale = true;
                fresh.error = Some(fresh.error.unwrap_or_else(|| {
                    "ElectrumX is reconnecting; showing last known indexed height.".to_string()
                }));
            }
            *cached = fresh;
            drop(cached);
            tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
        }
    });
}

/// Return cached status (non-blocking read).
pub async fn get_electrs_sync_status() -> ElectrsSyncStatus {
    cache().read().await.clone()
}

/// Get the total size of a directory in bytes.
async fn dir_size_bytes(path: &str) -> u64 {
    let mut total: u64 = 0;
    let mut entries = match tokio::fs::read_dir(path).await {
        Ok(entries) => entries,
        Err(_) => return 0,
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            total += Box::pin(dir_size_bytes(&entry_path.to_string_lossy())).await;
        } else if let Ok(meta) = entry.metadata().await {
            total += meta.len();
        }
    }
    total
}

/// Format bytes as human-readable string.
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else {
        format!("{} KB", bytes / 1_000)
    }
}

/// Returns true if the error is a transient connection/IO issue (not a data error).
fn is_transient_error(err_msg: &str) -> bool {
    let lower = err_msg.to_lowercase();
    lower.contains("connect")
        || lower.contains("reset")
        || lower.contains("refused")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("failed to read")
        || lower.contains("failed to write")
        || lower.contains("failed to flush")
        || lower.contains("empty response")
        || lower.contains("broken pipe")
        || lower.contains("eof")
        || lower.contains("connection")
        || lower.contains("503 service unavailable")
        || lower.contains("work queue depth exceeded")
}

/// Fetch ElectrumX indexed height via Electrum protocol (TCP JSON-RPC).
async fn electrumx_indexed_height() -> Result<u64> {
    let timeout_duration = Duration::from_secs(5);

    let stream = tokio::time::timeout(
        timeout_duration,
        TcpStream::connect((ELECTRUMX_HOST, ELECTRUMX_PORT)),
    )
    .await
    .context("ElectrumX connection timed out")?
    .context("Failed to connect to ElectrumX")?;

    let (reader_half, mut writer_half) = tokio::io::split(stream);

    // blockchain.headers.subscribe returns {"height": N, "hex": "..."}
    let req = r#"{"id":1,"method":"blockchain.headers.subscribe","params":[]}
"#;
    tokio::time::timeout(timeout_duration, writer_half.write_all(req.as_bytes()))
        .await
        .context("ElectrumX write timed out")?
        .context("Failed to write to ElectrumX")?;

    tokio::time::timeout(timeout_duration, writer_half.flush())
        .await
        .context("ElectrumX flush timed out")?
        .context("Failed to flush ElectrumX stream")?;

    let mut reader = BufReader::new(reader_half);
    let mut line = String::new();
    tokio::time::timeout(timeout_duration, reader.read_line(&mut line))
        .await
        .context("ElectrumX read timed out")?
        .context("Failed to read from ElectrumX")?;

    let line = line.trim();
    if line.is_empty() {
        anyhow::bail!("Empty response from ElectrumX");
    }

    let json: serde_json::Value = serde_json::from_str(line)?;
    // blockchain.numblocks.subscribe returns result as number; headers.subscribe returns {block_height: N, hex: ...}
    let height = json
        .get("result")
        .and_then(|r| r.as_u64())
        .or_else(|| {
            json.get("result")
                .and_then(|r| r.get("block_height"))
                .and_then(|h| h.as_u64())
        })
        .or_else(|| {
            // ElectrumX returns {"result": {"height": N, "hex": "..."}}
            json.get("result")
                .and_then(|r| r.get("height"))
                .and_then(|h| h.as_u64())
        })
        .context("Missing height in ElectrumX response")?;
    Ok(height)
}

fn parse_electrumx_height_from_logs(logs: &str) -> Option<u64> {
    let mut height = None;

    for line in logs.lines() {
        if let Some(idx) = line.find("BlockProcessor:our height:") {
            let rest = &line[idx + "BlockProcessor:our height:".len()..];
            if let Some(parsed) = parse_first_u64_token(rest) {
                height = Some(parsed);
            }
            continue;
        }

        if let Some(idx) = line.find("DB:height:") {
            let rest = &line[idx + "DB:height:".len()..];
            if let Some(parsed) = parse_first_u64_token(rest) {
                height = Some(parsed);
            }
        }
    }

    height
}

fn parse_first_u64_token(input: &str) -> Option<u64> {
    let token: String = input
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .filter(|c| *c != ',')
        .collect();

    if token.is_empty() {
        None
    } else {
        token.parse().ok()
    }
}

async fn electrumx_log_indexed_height() -> Result<u64> {
    let output = tokio::process::Command::new("podman")
        .args(["logs", "--tail", "500", "electrumx"])
        .output()
        .await
        .context("Failed to read ElectrumX logs")?;

    if !output.status.success() {
        anyhow::bail!(
            "podman logs electrumx failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let logs = String::from_utf8_lossy(&output.stdout);
    parse_electrumx_height_from_logs(&logs).context("No ElectrumX indexed height in logs")
}

/// Fetch Bitcoin local block height and best-known network header height via JSON-RPC.
async fn bitcoin_chain_heights() -> Result<(u64, u64)> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "1.0",
        "id": "electrs-status",
        "method": "getblockchaininfo",
        "params": []
    });
    let resp = client
        .post(crate::constants::BITCOIN_RPC_URL)
        .header("Content-Type", "application/json")
        .header("Authorization", bitcoin_rpc_auth().await)
        .body(body.to_string())
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .context("Bitcoin RPC request failed")?;

    if !resp.status().is_success() {
        anyhow::bail!("Bitcoin RPC returned {}", resp.status());
    }

    let json: serde_json::Value = resp.json().await?;
    let result = json
        .get("result")
        .context("Missing result in Bitcoin RPC")?;
    let blocks = result
        .get("blocks")
        .and_then(|h| h.as_u64())
        .context("Missing blocks in Bitcoin RPC")?;
    let headers = result
        .get("headers")
        .and_then(|h| h.as_u64())
        .unwrap_or(blocks);
    Ok((blocks, headers.max(blocks)))
}

/// Fetch fresh ElectrumX sync status (called by background cache task).
async fn fetch_electrs_sync_status() -> ElectrsSyncStatus {
    // Get index data size
    let data_bytes = dir_size_bytes(ELECTRUMX_DATA_DIR).await;
    let index_size = if data_bytes > 0 {
        Some(format_bytes(data_bytes))
    } else {
        None
    };

    // Read Tor onion address — check system Tor path first, then legacy
    let tor_onion = {
        let mut onion = None;
        for path in &[
            "/var/lib/archipelago/tor-hostnames/electrumx",
            "/var/lib/tor/hidden_service_electrumx/hostname",
            "/var/lib/archipelago/tor/hidden_service_electrumx/hostname",
        ] {
            if let Ok(addr) = tokio::fs::read_to_string(path).await {
                let addr = addr.trim().to_string();
                if addr.ends_with(".onion") {
                    onion = Some(addr);
                    break;
                }
            }
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

    let (bitcoin_blocks, network_height) = match bitcoin_chain_heights().await {
        Ok(heights) => heights,
        Err(e) => {
            let err_msg = e.to_string();
            if is_transient_error(&err_msg) {
                tracing::debug!("ElectrumX status: Bitcoin RPC transient: {}", err_msg);
            } else {
                warn!("ElectrumX status: Bitcoin RPC failed: {}", err_msg);
            }
            return ElectrsSyncStatus {
                indexed_height: 0,
                bitcoin_height: 0,
                network_height: 0,
                progress_pct: 0.0,
                status: "waiting".to_string(),
                stale: false,
                error: Some("Waiting for Bitcoin node...".to_string()),
                index_size,
                tor_onion,
            };
        }
    };

    let indexed_height = match electrumx_indexed_height().await {
        Ok(h) => h,
        Err(e) => match electrumx_log_indexed_height().await {
            Ok(h) if h > 0 => h,
            _ => {
                let err_msg = e.to_string();
                if is_transient_error(&err_msg) {
                    // ElectrumX is starting up or busy — estimate from data size
                    let progress_pct = if data_bytes > 0 {
                        ((data_bytes as f64 / ESTIMATED_FULL_INDEX_BYTES) * 100.0).min(99.0)
                    } else {
                        0.0
                    };
                    let size_str = index_size.clone().unwrap_or_else(|| "0 MB".to_string());
                    return ElectrsSyncStatus {
                    indexed_height: 0,
                    bitcoin_height: bitcoin_blocks,
                    network_height,
                    progress_pct,
                    status: "indexing".to_string(),
                    stale: false,
                    error: Some(format!(
                        "Building index ({} / ~130 GB estimated). Electrum RPC will be available when complete.",
                        size_str
                    )),
                    index_size,
                    tor_onion,
                };
                }
                // Genuine unexpected error
                warn!("ElectrumX status: unexpected error: {}", err_msg);
                return ElectrsSyncStatus {
                    indexed_height: 0,
                    bitcoin_height: bitcoin_blocks,
                    network_height,
                    progress_pct: 0.0,
                    status: "error".to_string(),
                    stale: false,
                    error: Some(format!("ElectrumX: {}", err_msg)),
                    index_size,
                    tor_onion,
                };
            }
        },
    };

    let observed_header_height = network_height.max(indexed_height);
    let bitcoin_catching_up = bitcoin_blocks > 0 && bitcoin_blocks < observed_header_height;
    let electrum_waiting_on_bitcoin =
        bitcoin_catching_up && indexed_height >= bitcoin_blocks.saturating_sub(1);
    let sync_target_height = if bitcoin_blocks > 0 {
        bitcoin_blocks
    } else {
        observed_header_height
    };

    let progress_pct = if electrum_waiting_on_bitcoin && observed_header_height > 0 {
        ((bitcoin_blocks as f64 / observed_header_height as f64) * 100.0).min(99.9)
    } else if sync_target_height > 0 {
        ((indexed_height as f64 / sync_target_height as f64) * 100.0).min(100.0)
    } else {
        0.0
    };

    let status = if sync_target_height == 0 {
        "waiting"
    } else if electrum_waiting_on_bitcoin {
        "waiting"
    } else if indexed_height >= sync_target_height.saturating_sub(1) {
        "synced"
    } else {
        "syncing"
    };

    let error = if electrum_waiting_on_bitcoin {
        Some(format!(
            "ElectrumX is indexed to {:}; waiting for the local Bitcoin node to catch up from {:} to known header {:}.",
            indexed_height, bitcoin_blocks, observed_header_height
        ))
    } else if status == "syncing" && bitcoin_blocks < observed_header_height {
        Some(format!(
            "Indexing local Bitcoin node height {:} of {:}. Bitcoin node is still catching up to known header {:}.",
            indexed_height, bitcoin_blocks, observed_header_height
        ))
    } else {
        None
    };

    ElectrsSyncStatus {
        indexed_height,
        bitcoin_height: bitcoin_blocks,
        network_height: observed_header_height,
        progress_pct,
        status: status.to_string(),
        stale: false,
        error,
        index_size,
        tor_onion,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_electrumx_height_from_logs;

    #[test]
    fn parses_latest_electrumx_progress_height_from_logs() {
        let logs = r#"
INFO:DB:height: 228,238
INFO:BlockProcessor:our height: 228,248 daemon: 731,568 UTXOs 1MB hist 1MB
INFO:BlockProcessor:our height: 232,117 daemon: 732,108 UTXOs 281MB hist 83MB
"#;
        assert_eq!(parse_electrumx_height_from_logs(logs), Some(232_117));
    }
}
