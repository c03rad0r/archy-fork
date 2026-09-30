mod channels;
mod info;
mod macaroons;
mod payments;
mod seed_backup;
mod wallet;

use crate::api::rpc::RpcHandler;
use anyhow::{anyhow, Context, Result};

/// Canonical on-host path for LND's admin macaroon.
pub(crate) const LND_ADMIN_MACAROON_PATH: &str =
    "/var/lib/archipelago/lnd/data/chain/bitcoin/mainnet/admin.macaroon";
pub(in crate::api) const LND_REST_BASE_URL: &str = "https://127.0.0.1:18080";

// Shared LND response types used by multiple submodules
#[derive(Debug, serde::Deserialize)]
pub(super) struct LndBalanceResponse {
    pub total_balance: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct LndAmount {
    pub sat: Option<String>,
}

/// Read LND's admin macaroon from disk.
///
/// The macaroon lives inside LND's container data dir and is owned by a
/// rootless-podman subordinate UID (typically 100000), mode 640. The
/// archipelago server runs as UID 1000 and therefore cannot read it
/// directly. We first try a plain read (works if an operator has relaxed
/// permissions), then fall back to `sudo cat` — mirroring the pattern
/// already used for Tor hidden-service hostnames.
pub(crate) async fn read_lnd_admin_macaroon() -> Result<Vec<u8>> {
    match tokio::fs::read(LND_ADMIN_MACAROON_PATH).await {
        Ok(bytes) => Ok(bytes),
        Err(direct_err) => {
            let output = tokio::process::Command::new("sudo")
                .args(["-n", "cat", LND_ADMIN_MACAROON_PATH])
                .output()
                .await
                .with_context(|| {
                    format!(
                        "Failed to read LND admin macaroon (direct: {direct_err}); sudo fallback also failed"
                    )
                })?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow!(
                    "Failed to read LND admin macaroon — is LND installed? (direct: {direct_err}; sudo: {})",
                    stderr.trim()
                ));
            }
            Ok(output.stdout)
        }
    }
}

/// Real-time wallet push (user req 2026-07-22): the UI must reflect an
/// incoming on-chain transaction the moment the node sees it, not on the
/// next poll. Streams LND's `/v1/transactions/subscribe` — it fires on
/// 0-conf mempool arrival AND again on each confirmation — and nudges the
/// shared data-model revision on every event; /ws/db pushes that to every
/// connected client and the frontend refetches wallet balance/transactions.
/// Reconnects forever with capped backoff: LND restarting, wallet locked, or
/// LND not installed yet all just mean "try again shortly".
pub(crate) fn spawn_lnd_tx_watcher(state_manager: std::sync::Arc<crate::state::StateManager>) {
    tokio::spawn(async move {
        let mut delay = std::time::Duration::from_secs(5);
        loop {
            match stream_lnd_transactions(&state_manager).await {
                // Stream ended cleanly (LND shutdown) — resume fast.
                Ok(()) => delay = std::time::Duration::from_secs(5),
                Err(e) => {
                    tracing::debug!("lnd tx watcher: {e:#} — retrying in {delay:?}");
                }
            }
            tokio::time::sleep(delay).await;
            delay = (delay * 2).min(std::time::Duration::from_secs(120));
        }
    });
}

async fn stream_lnd_transactions(sm: &crate::state::StateManager) -> Result<()> {
    let macaroon_hex = hex::encode(read_lnd_admin_macaroon().await?);
    // Dedicated client: the shared lnd_client() carries a 15s total timeout,
    // which would kill this deliberately long-lived stream.
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(std::time::Duration::from_secs(10))
        .danger_accept_invalid_certs(true)
        .build()
        .context("Failed to create streaming HTTP client")?;
    let mut resp = client
        .get(format!("{LND_REST_BASE_URL}/v1/transactions/subscribe"))
        .header("Grpc-Metadata-macaroon", &macaroon_hex)
        .send()
        .await
        .context("subscribe request failed")?;
    anyhow::ensure!(
        resp.status().is_success(),
        "transactions/subscribe returned {}",
        resp.status()
    );
    tracing::info!("lnd tx watcher: streaming wallet transaction events");
    while let Some(chunk) = resp.chunk().await? {
        if chunk.is_empty() {
            continue;
        }
        // Any streamed event = wallet activity. Revision bump is the push
        // contract (same pattern as the mesh-peer bridge in server.rs) — the
        // clients refetch, so we don't need to parse the event body.
        let (data, _) = sm.get_snapshot().await;
        sm.update_data(data).await;
        tracing::debug!(
            bytes = chunk.len(),
            "lnd tx watcher: wallet tx event — nudged ws clients"
        );
    }
    Ok(())
}

/// LND wedge watchdog (2026-07-22, "100% uptime"): a test node's LND sat
/// for 14 HOURS with its RPC answering but the server never finishing
/// startup — synced_to_chain=false, zero peers, every channel inactive —
/// and nothing noticed until a human tried to open a channel. The wedge
/// signature is precise: RPC healthy while (!synced_to_chain, or zero peers
/// with channels that need a peer) persists. A restart reliably clears it
/// (the backend-churn wedge is a known lnd+rpcpolling failure mode), so
/// after 15 consecutive bad minutes we bounce the container ourselves, with
/// a 30-minute cooldown so a genuinely broken LND can't restart-loop.
/// RPC-unreachable and locked-wallet states are deliberately NOT handled
/// here — container-down is crash-recovery's job, and unlocking needs the
/// operator.
fn bitcoin_ready_for_lnd_watchdog(status: &crate::bitcoin_status::BitcoinNodeStatus) -> bool {
    status.ok
        && !status.stale
        && status.age_ms < 30_000
        && status
            .blockchain_info
            .as_ref()
            .and_then(|v| v.get("initialblockdownload"))
            .and_then(|v| v.as_bool())
            == Some(false)
}

pub(crate) fn spawn_lnd_health_watchdog() {
    tokio::spawn(async move {
        let mut bad_minutes: u32 = 0;
        let mut last_restart: Option<tokio::time::Instant> = None;
        let mut last_height: Option<u64> = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            // Initial Bitcoin sync, warmup, and outages are dependencies to
            // wait for, never evidence that LND is wedged. Do not accumulate
            // restart pressure during a days-long initial block download.
            let bitcoin = crate::bitcoin_status::get_bitcoin_status().await;
            if !bitcoin_ready_for_lnd_watchdog(&bitcoin)
                || crate::app_ops::lifecycle_op_in_flight("lnd")
            {
                bad_minutes = 0;
                last_height = None;
                continue;
            }
            let Ok(bytes) = read_lnd_admin_macaroon().await else {
                bad_minutes = 0; // no LND on this node (or not set up yet)
                continue;
            };
            let macaroon_hex = hex::encode(bytes);
            let Ok(client) = reqwest::Client::builder()
                .no_proxy()
                .timeout(std::time::Duration::from_secs(10))
                .danger_accept_invalid_certs(true)
                .build()
            else {
                continue;
            };
            let Ok(resp) = client
                .get(format!("{LND_REST_BASE_URL}/v1/getinfo"))
                .header("Grpc-Metadata-macaroon", &macaroon_hex)
                .send()
                .await
            else {
                bad_minutes = 0; // down/locked — not the wedge signature
                continue;
            };
            if !resp.status().is_success() {
                bad_minutes = 0;
                continue;
            }
            let Ok(info) = resp.json::<serde_json::Value>().await else {
                bad_minutes = 0;
                continue;
            };
            let synced = info
                .get("synced_to_chain")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let peers = info.get("num_peers").and_then(|v| v.as_u64()).unwrap_or(0);
            let channels = info
                .get("num_active_channels")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                + info
                    .get("num_inactive_channels")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
                + info
                    .get("num_pending_channels")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
            let height = info.get("block_height").and_then(|v| v.as_u64());
            let progressing = height
                .zip(last_height)
                .is_some_and(|(now, before)| now > before);
            last_height = height;
            let wedged = !progressing && (!synced || (channels > 0 && peers == 0));
            if !wedged {
                bad_minutes = 0;
                continue;
            }
            bad_minutes += 1;
            if bad_minutes < 15 {
                continue;
            }
            if last_restart
                .map(|t| t.elapsed() < std::time::Duration::from_secs(1800))
                .unwrap_or(false)
            {
                continue;
            }
            tracing::warn!(
                synced_to_chain = synced,
                num_peers = peers,
                channels,
                "LND wedged for {bad_minutes} minutes (RPC up, server never ready) — restarting the lnd container"
            );
            let out = tokio::process::Command::new("podman")
                .args(["restart", "lnd"])
                .output()
                .await;
            match out {
                Ok(o) if o.status.success() => {
                    tracing::info!("LND watchdog restart complete");
                }
                Ok(o) => tracing::warn!(
                    "LND watchdog restart failed: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                ),
                Err(e) => tracing::warn!("LND watchdog restart failed: {e}"),
            }
            last_restart = Some(tokio::time::Instant::now());
            bad_minutes = 0;
        }
    });
}

impl RpcHandler {
    /// Helper: create an authenticated LND REST client.
    /// Returns an HTTP client configured for LND's self-signed TLS and the
    /// hex-encoded admin macaroon for request headers.
    pub(crate) async fn lnd_client(&self) -> Result<(reqwest::Client, String)> {
        let macaroon_bytes = read_lnd_admin_macaroon().await?;
        let macaroon_hex = hex::encode(&macaroon_bytes);
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(15))
            .danger_accept_invalid_certs(true)
            .build()
            .context("Failed to create HTTP client")?;
        Ok((client, macaroon_hex))
    }
}

#[cfg(test)]
mod watchdog_dependency_tests {
    use super::bitcoin_ready_for_lnd_watchdog;
    use crate::bitcoin_status::BitcoinNodeStatus;
    use serde_json::json;
    #[test]
    fn initial_sync_warmup_outage_stale_and_unknown_never_trigger_lnd_restart() {
        let mut status = BitcoinNodeStatus::default();
        assert!(!bitcoin_ready_for_lnd_watchdog(&status));
        status.ok = true;
        status.blockchain_info = Some(json!({"initialblockdownload":true}));
        assert!(!bitcoin_ready_for_lnd_watchdog(&status));
        status.blockchain_info = Some(json!({"initialblockdownload":false}));
        assert!(bitcoin_ready_for_lnd_watchdog(&status));
        status.stale = true;
        assert!(!bitcoin_ready_for_lnd_watchdog(&status));
        status.stale = false;
        status.ok = false;
        assert!(!bitcoin_ready_for_lnd_watchdog(&status));
        status.ok = true;
        status.age_ms = 30_000;
        assert!(!bitcoin_ready_for_lnd_watchdog(&status));
        status.age_ms = 0;
        status.blockchain_info = Some(json!({}));
        assert!(!bitcoin_ready_for_lnd_watchdog(&status));
    }
}
