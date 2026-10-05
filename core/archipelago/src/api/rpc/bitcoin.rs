use super::RpcHandler;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Retry configuration for [`bitcoin_rpc_post_with_retry`].
///
/// Exposed as a struct (rather than hard-coded constants inside the function)
/// so tests can dial down timeouts to keep the suite fast while still
/// exercising real retry/backoff behavior.
#[derive(Debug, Clone)]
struct RetryConfig {
    max_attempts: u32,
    attempt_timeout: std::time::Duration,
    /// Length must equal `max_attempts - 1` (one backoff between each
    /// successive attempt). The last attempt is not followed by a backoff.
    backoffs: Vec<std::time::Duration>,
}

impl RetryConfig {
    /// Production retry policy: 3 attempts, 15s each, 500ms + 1500ms backoffs.
    /// Total worst-case wall time: 3 * 15 + 0.5 + 1.5 = 47s.
    fn production() -> Self {
        Self {
            max_attempts: BITCOIN_RPC_MAX_ATTEMPTS,
            attempt_timeout: BITCOIN_RPC_ATTEMPT_TIMEOUT,
            backoffs: BITCOIN_RPC_BACKOFFS.to_vec(),
        }
    }
}

/// Max retry attempts for a single bitcoin_rpc_call invocation.
/// First attempt + 2 retries = 3 total.
const BITCOIN_RPC_MAX_ATTEMPTS: u32 = 3;

/// Per-attempt deadline. Must be >= the reqwest client's own timeout (we
/// build it at 15s in handle_bitcoin_getinfo) — this is the outer safety net.
const BITCOIN_RPC_ATTEMPT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Backoff between attempts. Index 0 = after first failure, 1 = after second, etc.
/// Chosen to absorb bitcoind's typical block-validation stall (2-5s) without
/// adding noticeable latency on the happy path (first attempt succeeds in ~30ms).
const BITCOIN_RPC_BACKOFFS: [std::time::Duration; 2] = [
    std::time::Duration::from_millis(500),
    std::time::Duration::from_millis(1500),
];

/// Classify a reqwest error as transient (retryable) or fatal.
/// Transient: timeout, connect refused, request/response body IO errors.
/// Fatal: TLS errors, URL parse errors, redirect loops, builder errors.
fn is_transient_transport_error(e: &reqwest::Error) -> bool {
    e.is_timeout() || e.is_connect() || e.is_request() || e.is_body()
}

#[derive(Debug, Serialize)]
struct BitcoinInfo {
    block_height: u64,
    sync_progress: f64,
    chain: String,
    difficulty: f64,
    mempool_size: u64,
    mempool_tx_count: u64,
    verification_progress: f64,
}

#[derive(Debug, Deserialize)]
struct BitcoinRpcResponse<T> {
    result: Option<T>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct BlockchainInfo {
    chain: Option<String>,
    blocks: Option<u64>,
    difficulty: Option<f64>,
    #[serde(rename = "verificationprogress")]
    verification_progress: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct MempoolInfo {
    size: Option<u64>,
    bytes: Option<u64>,
}

impl RpcHandler {
    pub(super) async fn handle_bitcoin_getinfo(&self) -> Result<serde_json::Value> {
        // Per-attempt timeout (see bitcoin_rpc_call for retry semantics).
        // 15s is enough room for bitcoind to answer getblockchaininfo even
        // during block validation; bitcoin_rpc_call wraps each attempt in a
        // separate tokio::time::timeout too, so this is belt-and-suspenders.
        // connect_timeout is tighter so a dead bitcoind doesn't steal the
        // whole attempt budget on TCP connect alone.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .connect_timeout(std::time::Duration::from_secs(3))
            .build()
            .context("Failed to create HTTP client")?;

        let blockchain_info = self
            .bitcoin_rpc_call::<BlockchainInfo>(&client, "getblockchaininfo", &[])
            .await
            .context("Failed to query getblockchaininfo")?;

        let mempool_info = self
            .bitcoin_rpc_call::<MempoolInfo>(&client, "getmempoolinfo", &[])
            .await
            .unwrap_or(MempoolInfo {
                size: Some(0),
                bytes: Some(0),
            });

        let info = BitcoinInfo {
            block_height: blockchain_info.blocks.unwrap_or(0),
            sync_progress: blockchain_info.verification_progress.unwrap_or(0.0),
            chain: blockchain_info.chain.unwrap_or_else(|| "unknown".into()),
            difficulty: blockchain_info.difficulty.unwrap_or(0.0),
            mempool_size: mempool_info.bytes.unwrap_or(0),
            mempool_tx_count: mempool_info.size.unwrap_or(0),
            verification_progress: blockchain_info.verification_progress.unwrap_or(0.0),
        };

        Ok(serde_json::to_value(info)?)
    }

    /// Call a Bitcoin Core JSON-RPC method.
    ///
    /// Retries up to [`BITCOIN_RPC_MAX_ATTEMPTS`] times on transient
    /// transport errors (timeout / connection refused / send/recv IO).
    /// Does **not** retry when bitcoind responds with a well-formed
    /// `{"error": ...}` body — those are real RPC errors and surfacing
    /// them quickly is the right behavior.
    ///
    /// Motivation: on a syncing pruned node, bitcoind's RPC thread can block
    /// for 5-10 seconds during block validation. A single 10s timeout means
    /// ~30% of UI calls error out even though the node is perfectly healthy.
    /// With retry + backoff, the UI sees a uniform slow-but-successful
    /// response instead of intermittent failures.
    pub(in crate::api::rpc) async fn bitcoin_rpc_call<T: serde::de::DeserializeOwned>(
        &self,
        client: &reqwest::Client,
        method: &str,
        params: &[serde_json::Value],
    ) -> Result<T> {
        let (rpc_user, rpc_pass) = crate::bitcoin_rpc::bitcoin_rpc_credentials().await;
        bitcoin_rpc_post_with_retry(
            client,
            crate::constants::BITCOIN_RPC_URL,
            &rpc_user,
            &rpc_pass,
            method,
            params,
        )
        .await
    }

    // NOTE: the Bitcoin Core wallet-init handler that used to live here was deleted in
    // Phase 10 (D-07b) to close audit finding F-13. It derived the BIP-84 account
    // extended *private* key, stringified it, and imported `wpkh(xprv/0/*)` /
    // `wpkh(xprv/1/*)` into Core's `wallet.dat` — a second copy of the node's spending
    // key, outside the daemon's Argon2 + ChaCha20-Poly1305 envelope. It had no caller
    // anywhere in the repo; LND is the wallet the UI drives.
    //
    // Do NOT reintroduce a Bitcoin Core wallet path that imports private keys. If a
    // Core wallet is ever needed again it must be watch-only by construction
    // (`disable_private_keys = true`, xpub descriptors with a `[fingerprint/derivation]`
    // key origin). Full rationale, evidence and the deleted symbol's name:
    // docs/security/KEY-03-SIGNING-POSTURE.md
}

/// Free-function counterpart to `RpcHandler::bitcoin_rpc_call`.
///
/// Takes the URL + credentials as parameters so it can be exercised by unit
/// tests against a mock HTTP server without constructing a full `RpcHandler`.
///
/// Production callers go through `RpcHandler::bitcoin_rpc_call`, which loads
/// credentials from the secrets file and points at `BITCOIN_RPC_URL`.
async fn bitcoin_rpc_post_with_retry<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    rpc_user: &str,
    rpc_pass: &str,
    method: &str,
    params: &[serde_json::Value],
) -> Result<T> {
    bitcoin_rpc_post_with_retry_cfg(
        client,
        url,
        rpc_user,
        rpc_pass,
        method,
        params,
        &RetryConfig::production(),
    )
    .await
}

/// Inner implementation with configurable retry policy (for tests).
async fn bitcoin_rpc_post_with_retry_cfg<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    rpc_user: &str,
    rpc_pass: &str,
    method: &str,
    params: &[serde_json::Value],
    cfg: &RetryConfig,
) -> Result<T> {
    debug_assert_eq!(
        cfg.backoffs.len(),
        (cfg.max_attempts - 1) as usize,
        "RetryConfig: backoffs.len() must equal max_attempts - 1"
    );

    let body = serde_json::json!({
        "jsonrpc": "1.0",
        "id": "archy",
        "method": method,
        "params": params,
    });

    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..cfg.max_attempts {
        if attempt > 0 {
            let backoff = cfg
                .backoffs
                .get(attempt as usize - 1)
                .copied()
                .unwrap_or_else(|| std::time::Duration::from_secs(2));
            tracing::warn!(
                "bitcoin_rpc({}): attempt {} failed, backing off {:?}",
                method,
                attempt,
                backoff
            );
            tokio::time::sleep(backoff).await;
        }

        // Per-attempt hard deadline. Independent of reqwest's built-in timeout
        // so we always cap total time even if reqwest blocks on something
        // weird (e.g., DNS starvation).
        let fut = client
            .post(url)
            .basic_auth(rpc_user, Some(rpc_pass))
            .json(&body)
            .send();

        let send_result = match tokio::time::timeout(cfg.attempt_timeout, fut).await {
            Err(_elapsed) => {
                last_err = Some(anyhow::anyhow!(
                    "Bitcoin RPC send timed out after {:?}",
                    cfg.attempt_timeout
                ));
                continue; // transient: retry
            }
            Ok(r) => r,
        };

        let resp = match send_result {
            Ok(r) => r,
            Err(e) if is_transient_transport_error(&e) => {
                last_err = Some(anyhow::Error::from(e).context("Bitcoin RPC connection failed"));
                continue; // transient: retry
            }
            Err(e) => {
                return Err(anyhow::Error::from(e).context("Bitcoin RPC connection failed"));
            }
        };

        let rpc_resp: BitcoinRpcResponse<T> = resp
            .json()
            .await
            .context("Failed to parse Bitcoin RPC response")?;

        if let Some(err) = rpc_resp.error {
            // RPC-level error: this is a real bitcoind response, not transient.
            anyhow::bail!("Bitcoin RPC error: {}", err);
        }

        return rpc_resp
            .result
            .ok_or_else(|| anyhow::anyhow!("Bitcoin RPC returned null result"));
    }

    Err(last_err
        .unwrap_or_else(|| anyhow::anyhow!("Bitcoin RPC exhausted retries with no error captured")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::service::{make_service_fn, service_fn};
    use hyper::{Body, Request, Response, Server, StatusCode};
    use std::convert::Infallible;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    /// Spin up a mock bitcoind HTTP server that behaves according to `handler`.
    /// Returns the bound URL and a JoinHandle (dropped = server shutdown via the
    /// oneshot cancel channel).
    async fn spawn_mock<F, Fut>(
        handler: F,
    ) -> (
        String,
        tokio::task::JoinHandle<()>,
        tokio::sync::oneshot::Sender<()>,
    )
    where
        F: Fn(Request<Body>) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Response<Body>> + Send + 'static,
    {
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let make_svc = make_service_fn(move |_| {
            let handler = handler.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let handler = handler.clone();
                    async move { Ok::<_, Infallible>(handler(req).await) }
                }))
            }
        });
        let server = Server::bind(&addr).serve(make_svc);
        let url = format!("http://{}", server.local_addr());
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let graceful = server.with_graceful_shutdown(async {
                let _ = rx.await;
            });
            let _ = graceful.await;
        });
        (url, handle, tx)
    }

    /// Reply body bitcoind would send for a successful getblockcount.
    fn ok_reply() -> Body {
        Body::from(r#"{"result":42,"error":null,"id":"archy"}"#)
    }

    fn err_reply() -> Body {
        Body::from(r#"{"result":null,"error":{"code":-8,"message":"nope"},"id":"archy"}"#)
    }

    /// Succeeds on first attempt — should not retry.
    #[tokio::test]
    async fn happy_path_first_attempt() {
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        let (url, _h, _tx) = spawn_mock(move |_req| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Response::new(ok_reply())
            }
        })
        .await;

        let client = reqwest::Client::builder().build().unwrap();
        let v: u64 =
            bitcoin_rpc_post_with_retry(&client, &url, "user", "pass", "getblockcount", &[])
                .await
                .expect("should succeed");
        assert_eq!(v, 42);
        assert_eq!(count.load(Ordering::SeqCst), 1, "should not have retried");
    }

    /// HTTP 503 with non-JSON body: produces a JSON-parse error which is NOT
    /// classified as transient. Must fail after first attempt.
    /// This guards against the tempting mistake of blanket-retrying every
    /// non-2xx response — which would mask real bitcoind misconfig.
    #[tokio::test]
    async fn does_not_retry_parse_errors() {
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        let (url, _h, _tx) = spawn_mock(move |_req| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .body(Body::from("busy"))
                    .unwrap()
            }
        })
        .await;

        let client = reqwest::Client::builder().build().unwrap();
        let result: Result<u64> =
            bitcoin_rpc_post_with_retry(&client, &url, "user", "pass", "getblockcount", &[]).await;
        assert!(result.is_err(), "non-JSON response should error out");
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "parse errors are not retryable"
        );
    }

    /// Connect-refused (port closed) is the canonical transient transport
    /// error. Must exhaust BITCOIN_RPC_MAX_ATTEMPTS and the total elapsed
    /// time must include at least the sum of the backoffs.
    #[tokio::test]
    async fn retries_exhausted_on_persistent_connect_refused() {
        // Bind a port then immediately drop the listener so the port is closed.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let closed_url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);

        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_millis(500))
            .build()
            .unwrap();
        let start = std::time::Instant::now();
        let result: Result<u64> =
            bitcoin_rpc_post_with_retry(&client, &closed_url, "user", "pass", "getblockcount", &[])
                .await;
        let elapsed = start.elapsed();
        assert!(result.is_err(), "connect-refused should exhaust retries");
        let min_backoff: std::time::Duration = BITCOIN_RPC_BACKOFFS.iter().sum();
        assert!(
            elapsed >= min_backoff,
            "should have backed off between retries (elapsed={:?}, expected at least {:?})",
            elapsed,
            min_backoff
        );
    }

    /// The motivating scenario: first attempt times out (bitcoind busy),
    /// subsequent attempt succeeds. Uses a short test-only RetryConfig so
    /// the test runs in <1s instead of 15s.
    #[tokio::test]
    async fn retries_on_timeout_then_succeeds() {
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        // Mock server: first request hangs for 500ms, subsequent requests reply OK.
        let (url, _h, _tx) = spawn_mock(move |_req| {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
                Response::new(ok_reply())
            }
        })
        .await;

        let client = reqwest::Client::builder().build().unwrap();
        // Attempt timeout 100ms < server's 500ms sleep => first attempt times out.
        // Backoff 20ms between attempts.
        let cfg = RetryConfig {
            max_attempts: 3,
            attempt_timeout: std::time::Duration::from_millis(100),
            backoffs: vec![
                std::time::Duration::from_millis(20),
                std::time::Duration::from_millis(20),
            ],
        };
        let v: u64 = bitcoin_rpc_post_with_retry_cfg(
            &client,
            &url,
            "user",
            "pass",
            "getblockcount",
            &[],
            &cfg,
        )
        .await
        .expect("second attempt should succeed");
        assert_eq!(v, 42);
        assert!(
            count.load(Ordering::SeqCst) >= 2,
            "expected at least 2 attempts (got {})",
            count.load(Ordering::SeqCst)
        );
    }

    /// bitcoind returned a well-formed `{"error": ...}` body. Must NOT retry.
    #[tokio::test]
    async fn does_not_retry_on_rpc_level_error() {
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        let (url, _h, _tx) = spawn_mock(move |_req| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Response::new(err_reply())
            }
        })
        .await;

        let client = reqwest::Client::builder().build().unwrap();
        let result: Result<u64> =
            bitcoin_rpc_post_with_retry(&client, &url, "user", "pass", "getblockcount", &[]).await;
        assert!(result.is_err());
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "RPC-level errors are not transient"
        );
    }

    /// Sanity: retry budget invariants. Chosen to catch regressions where
    /// someone bumps these constants without realizing the total worst-case
    /// wall time implications.
    #[test]
    fn retry_budget_invariants() {
        assert_eq!(BITCOIN_RPC_MAX_ATTEMPTS, 3);
        assert_eq!(
            BITCOIN_RPC_BACKOFFS.len(),
            (BITCOIN_RPC_MAX_ATTEMPTS - 1) as usize
        );
        // Total wall-time ceiling:
        //   3 attempts * 15s  +  (0.5s + 1.5s) backoff  =  47s
        let total: std::time::Duration = BITCOIN_RPC_ATTEMPT_TIMEOUT * BITCOIN_RPC_MAX_ATTEMPTS
            + BITCOIN_RPC_BACKOFFS.iter().sum::<std::time::Duration>();
        assert!(total < std::time::Duration::from_secs(60));
    }
}
