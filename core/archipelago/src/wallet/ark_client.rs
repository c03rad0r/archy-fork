//! Thin HTTP bridge to the `barkd` sidecar container (Ark protocol).
//!
//! Same shape as [`super::fedimint_client`]: the heavy `bark-wallet` SDK stays
//! OUT of this binary. The `barkd` daemon (in `apps/barkd`) holds the Ark
//! wallet (VTXOs, rounds, unilateral exits) and we speak its REST API
//! (`/api/v1/*`, Bearer auth). Endpoint/JSON shapes target barkd 0.3.0 and
//! must be pinned to the vendored image tag.
//!
//! ARK is on-chain-anchored: VTXOs expire (`vtxo_expiry_delta` blocks) and the
//! barkd daemon refreshes them by joining rounds on its own — the bridge never
//! has to schedule anything. Unlike Cashu/Fedimint, funds survive the sidecar
//! dying (the wallet mnemonic in barkd's datadir can unilaterally exit
//! on-chain), so back up `/var/lib/archipelago/barkd`.

use anyhow::{Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

const BARKD_TIMEOUT_SECS: u64 = 15;
/// Send/board/offboard can wait on Ark round participation (signet rounds run
/// every 5 minutes), so give mutating calls generous room.
const BARKD_HEAVY_TIMEOUT_SECS: u64 = 120;

/// Default host port the `barkd` container is mapped to (its in-container
/// REST port; 3535 is unused elsewhere on the node — see `port_allocator`).
const DEFAULT_BARKD_URL: &str = "http://127.0.0.1:3535";

/// Shared secret between the barkd container and this bridge. The barkd
/// manifest generates it via `generated_secrets: [{barkd-secret, hex32}]`; the
/// container entrypoint installs it with `barkd secret refresh --secret` and
/// the bridge derives the matching Bearer token from the same file.
const BARKD_SECRET: &str = "barkd-secret";

/// Wallet configuration used when the bridge has to create the barkd wallet
/// (first use). Persisted so operators can point at their own Ark server.
/// Defaults target Second's public signet deployment while Ark matures —
/// mainnet needs an explicit opt-in edit of `wallet/ark_config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkConfig {
    pub network: String,
    pub ark_server: String,
    pub esplora: String,
}

impl Default for ArkConfig {
    fn default() -> Self {
        Self {
            network: "signet".to_string(),
            ark_server: "https://ark.signet.2nd.dev".to_string(),
            esplora: "https://esplora.signet.2nd.dev".to_string(),
        }
    }
}

const ARK_CONFIG_FILE: &str = "wallet/ark_config.json";

pub async fn load_config(data_dir: &Path) -> ArkConfig {
    match fs::read_to_string(data_dir.join(ARK_CONFIG_FILE)).await {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => ArkConfig::default(),
    }
}

pub async fn save_config(data_dir: &Path, config: &ArkConfig) -> Result<()> {
    let dir = data_dir.join("wallet");
    fs::create_dir_all(&dir)
        .await
        .context("Failed to create wallet dir")?;
    let content = serde_json::to_string_pretty(config).context("Failed to serialize ark config")?;
    fs::write(data_dir.join(ARK_CONFIG_FILE), content)
        .await
        .context("Failed to write ark config")?;
    Ok(())
}

/// Encode barkd's Bearer token from the raw 32-byte shared secret:
/// base64url-nopad of `<version 0x00><32-byte secret>` (see barkd `AuthToken`).
fn encode_auth_token(secret: &[u8; 32]) -> String {
    let mut buf = Vec::with_capacity(33);
    buf.push(0u8);
    buf.extend_from_slice(secret);
    URL_SAFE_NO_PAD.encode(&buf)
}

fn secret_hex_to_token(hex: &str) -> Result<String> {
    let hex = hex.trim();
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!("barkd-secret must be exactly 64 hex characters");
    }
    let mut secret = [0u8; 32];
    for (i, byte) in secret.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("validated hex");
    }
    Ok(encode_auth_token(&secret))
}

/// HTTP client for a `barkd` instance.
pub struct ArkClient {
    base_url: String,
    token: String,
    client: reqwest::Client,
}

impl ArkClient {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(BARKD_HEAVY_TIMEOUT_SECS))
            .build()
            .context("Failed to build HTTP client for barkd")?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            client,
        })
    }

    /// Resolve URL + auth token from env / node secret, with sane defaults.
    /// URL: `BARKD_URL` else the default mapped port. Token: `BARKD_TOKEN`
    /// (already-encoded Bearer token) else derived from the shared
    /// `barkd-secret` the manifest generated for the container.
    pub async fn from_node(data_dir: &Path) -> Result<Self> {
        let base_url = std::env::var("BARKD_URL").unwrap_or_else(|_| DEFAULT_BARKD_URL.to_string());
        let token = match std::env::var("BARKD_TOKEN") {
            Ok(t) if !t.is_empty() => t,
            _ => {
                let path = data_dir.join("secrets").join(BARKD_SECRET);
                let hex = fs::read_to_string(&path).await.context(
                    "Ark wallet not configured (no BARKD_TOKEN and no barkd-secret \
                     secret). Install the Ark (barkd) app.",
                )?;
                secret_hex_to_token(&hex)?
            }
        };
        Self::new(&base_url, &token)
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.bearer_auth(&self.token)
    }

    async fn get(&self, path: &str) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .auth(self.client.get(&url))
            .timeout(std::time::Duration::from_secs(BARKD_TIMEOUT_SECS))
            .send()
            .await
            .with_context(|| format!("barkd GET {path} failed (is it running?)"))?;
        Self::parse(resp, path).await
    }

    async fn post(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .auth(self.client.post(&url))
            .json(&body)
            .send()
            .await
            .with_context(|| format!("barkd POST {path} failed (is it running?)"))?;
        Self::parse(resp, path).await
    }

    async fn parse(resp: reqwest::Response, path: &str) -> Result<serde_json::Value> {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            // barkd errors are `{"message": "..."}`; surface the message.
            let msg = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
                .unwrap_or(text);
            anyhow::bail!("barkd {path} returned {status}: {msg}");
        }
        if text.is_empty() {
            return Ok(serde_json::json!({}));
        }
        serde_json::from_str(&text)
            .with_context(|| format!("barkd {path} returned non-JSON: {text}"))
    }

    /// `GET /api/v1/wallet` — wallet info (fingerprint, network, config).
    /// Errors with "No wallet set" until `create_wallet` has run.
    pub async fn wallet_info(&self) -> Result<serde_json::Value> {
        self.get("/api/v1/wallet").await
    }

    /// `POST /api/v1/wallet/create` — create (or restore, with a mnemonic) the
    /// barkd wallet. Idempotent guard is on the caller (`ensure_wallet`).
    pub async fn create_wallet(&self, config: &ArkConfig) -> Result<serde_json::Value> {
        self.post(
            "/api/v1/wallet/create",
            serde_json::json!({
                "network": config.network,
                "ark_server": config.ark_server,
                "chain_source": { "esplora": { "url": config.esplora } },
            }),
        )
        .await
    }

    /// `GET /api/v1/wallet/balance` — off-chain balance breakdown, in sats.
    pub async fn balance(&self) -> Result<serde_json::Value> {
        self.get("/api/v1/wallet/balance").await
    }

    /// Spendable off-chain sats (0 on any missing field, never an error once
    /// the call itself succeeds).
    pub async fn spendable_sats(&self) -> Result<u64> {
        let bal = self.balance().await?;
        Ok(bal
            .get("spendable_sat")
            .and_then(|v| v.as_u64())
            .unwrap_or(0))
    }

    /// `GET /api/v1/onchain/balance` — the wallet's on-chain (boarding) funds.
    pub async fn onchain_balance(&self) -> Result<serde_json::Value> {
        self.get("/api/v1/onchain/balance").await
    }

    /// `POST /api/v1/wallet/addresses/next` — fresh Ark (`tark1…`) address.
    pub async fn ark_address(&self) -> Result<String> {
        let res = self
            .post("/api/v1/wallet/addresses/next", serde_json::json!({}))
            .await?;
        res.get("address")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("barkd address: no address in response"))
    }

    /// `POST /api/v1/onchain/addresses/next` — fresh on-chain boarding address.
    pub async fn onchain_address(&self) -> Result<String> {
        let res = self
            .post("/api/v1/onchain/addresses/next", serde_json::json!({}))
            .await?;
        res.get("address")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("barkd onchain address: no address in response"))
    }

    /// `POST /api/v1/wallet/send` — pay an Ark address, BOLT11 invoice, LNURL
    /// or lightning address from off-chain funds. Returns the movement barkd
    /// reports for the payment.
    pub async fn send(
        &self,
        destination: &str,
        amount_sats: Option<u64>,
        comment: Option<&str>,
    ) -> Result<serde_json::Value> {
        self.post(
            "/api/v1/wallet/send",
            serde_json::json!({
                "destination": destination,
                "amount_sat": amount_sats,
                "comment": comment,
            }),
        )
        .await
    }

    /// `POST /api/v1/lightning/receives/invoice` — BOLT11 invoice that lands
    /// as an Ark VTXO when paid.
    pub async fn lightning_invoice(&self, amount_sats: u64) -> Result<serde_json::Value> {
        self.post(
            "/api/v1/lightning/receives/invoice",
            serde_json::json!({ "amount_sat": amount_sats }),
        )
        .await
    }

    /// `POST /api/v1/boards/board-amount` (or `board-all` when `amount_sats`
    /// is None) — lift on-chain funds into Ark VTXOs.
    pub async fn board(&self, amount_sats: Option<u64>) -> Result<serde_json::Value> {
        match amount_sats {
            Some(sats) => {
                self.post(
                    "/api/v1/boards/board-amount",
                    serde_json::json!({ "amount_sat": sats }),
                )
                .await
            }
            None => {
                self.post("/api/v1/boards/board-all", serde_json::json!({}))
                    .await
            }
        }
    }

    /// `POST /api/v1/wallet/offboard/all` — move all VTXOs back on-chain via a
    /// collaborative round.
    pub async fn offboard_all(&self, address: Option<&str>) -> Result<serde_json::Value> {
        self.post(
            "/api/v1/wallet/offboard/all",
            serde_json::json!({ "address": address }),
        )
        .await
    }

    /// `GET /api/v1/wallet/movements` — barkd's own movement history. This is
    /// authoritative (includes receives we never initiated), so unlike the
    /// Fedimint bridge there is no local tx log to maintain.
    pub async fn movements(&self) -> Result<Vec<serde_json::Value>> {
        let res = self.get("/api/v1/wallet/movements").await?;
        Ok(res.as_array().cloned().unwrap_or_default())
    }

    /// `GET /api/v1/wallet/ark-info` — connected Ark server parameters.
    pub async fn ark_info(&self) -> Result<serde_json::Value> {
        self.get("/api/v1/wallet/ark-info").await
    }
}

/// Idempotently make sure barkd has a wallet, creating one with the node's
/// Ark config on first use. Best-effort no-op when the sidecar isn't
/// installed/running yet — mirrors `fedimint_client::ensure_default_federation`.
pub async fn ensure_wallet(data_dir: &Path) -> Result<()> {
    let client = match ArkClient::from_node(data_dir).await {
        Ok(c) => c,
        Err(_) => return Ok(()), // barkd not configured yet
    };
    if client.wallet_info().await.is_ok() {
        return Ok(());
    }
    let config = load_config(data_dir).await;
    match client.create_wallet(&config).await {
        Ok(_) => {
            tracing::info!(
                "created barkd Ark wallet ({} via {})",
                config.network,
                config.ark_server
            );
            // Persist the effective config so the settings UI shows what the
            // wallet was actually created with.
            let _ = save_config(data_dir, &config).await;
        }
        Err(e) => tracing::debug!("barkd wallet auto-create skipped: {e}"),
    }
    Ok(())
}

/// Total spendable Ark sats, soft-failing to 0 when the sidecar is not
/// installed or unreachable so unified balances still render.
pub async fn spendable_sats_or_zero(data_dir: &Path) -> u64 {
    match ArkClient::from_node(data_dir).await {
        Ok(client) => client.spendable_sats().await.unwrap_or(0),
        Err(_) => 0,
    }
}

/// Map barkd movements into unified [`EcashTransaction`] history entries
/// (kind = "ark"). Best-effort: empty on any error, never blocks history.
pub async fn load_ark_txs(data_dir: &Path) -> Vec<crate::wallet::ecash::EcashTransaction> {
    let client = match ArkClient::from_node(data_dir).await {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let movements = match client.movements().await {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    movements.iter().filter_map(movement_to_tx).collect()
}

/// Convert one barkd `Movement` into an [`EcashTransaction`]. `None` for
/// zero-delta movements (e.g. internal refreshes) so history stays meaningful.
fn movement_to_tx(m: &serde_json::Value) -> Option<crate::wallet::ecash::EcashTransaction> {
    use crate::wallet::ecash::{EcashTransaction, TransactionType};

    let delta = m.get("effective_balance_sat").and_then(|v| v.as_i64())?;
    if delta == 0 {
        return None;
    }
    let tx_type = if delta < 0 {
        TransactionType::Send
    } else {
        TransactionType::Receive
    };
    // `time` holds created/updated/completed; prefer the completion time.
    let timestamp = m
        .get("time")
        .and_then(|t| {
            t.get("completed_at")
                .or_else(|| t.get("updated_at"))
                .or_else(|| t.get("created_at"))
        })
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    // Describe via the recipient list (send) or receive source when present.
    let peer = m
        .get("sent_to")
        .or_else(|| m.get("received_on"))
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|d| {
            d.get("destination")
                .or_else(|| d.get("address"))
                .or_else(|| d.get("invoice"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or_default()
        .to_string();
    let subsystem = m
        .get("subsystem")
        .map(|s| match s {
            serde_json::Value::String(v) => v.clone(),
            other => other
                .as_object()
                .and_then(|o| o.keys().next().cloned())
                .unwrap_or_default(),
        })
        .unwrap_or_default();
    let description = if delta < 0 {
        format!("Sent via Ark{}", suffix(&subsystem))
    } else {
        format!("Received via Ark{}", suffix(&subsystem))
    };
    Some(EcashTransaction {
        id: format!("ark-{}", m.get("id").and_then(|v| v.as_u64()).unwrap_or(0)),
        tx_type,
        amount_sats: delta.unsigned_abs(),
        timestamp,
        description,
        mint_url: String::new(),
        peer,
        kind: "ark".to_string(),
    })
}

fn suffix(subsystem: &str) -> String {
    if subsystem.is_empty() {
        String::new()
    } else {
        format!(" ({subsystem})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_encoding_matches_barkd_format() {
        // barkd token = base64url-nopad(0x00 || secret); 33 bytes -> 44 chars.
        let token = secret_hex_to_token(&"ab".repeat(32)).unwrap();
        assert_eq!(token.len(), 44);
        let bytes = URL_SAFE_NO_PAD.decode(&token).unwrap();
        assert_eq!(bytes.len(), 33);
        assert_eq!(bytes[0], 0);
        assert_eq!(&bytes[1..], &[0xabu8; 32]);
    }

    #[test]
    fn token_rejects_bad_secret() {
        assert!(secret_hex_to_token("deadbeef").is_err(), "too short");
        assert!(secret_hex_to_token(&"zz".repeat(32)).is_err(), "not hex");
    }

    #[test]
    fn movement_maps_to_history_entry() {
        let m = serde_json::json!({
            "id": 7,
            "effective_balance_sat": -1500,
            "time": { "completed_at": "2026-07-14T12:00:00Z" },
            "sent_to": [{ "destination": "tark1abc" }],
            "subsystem": "arkoor",
        });
        let tx = movement_to_tx(&m).expect("mapped");
        assert_eq!(tx.amount_sats, 1500);
        assert_eq!(tx.kind, "ark");
        assert_eq!(tx.peer, "tark1abc");
        assert!(matches!(
            tx.tx_type,
            crate::wallet::ecash::TransactionType::Send
        ));

        // Zero-delta refresh movements are dropped.
        let refresh = serde_json::json!({ "id": 8, "effective_balance_sat": 0 });
        assert!(movement_to_tx(&refresh).is_none());
    }
}
