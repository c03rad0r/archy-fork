//! HTTP client for Cashu mint API (NUT-01 through NUT-06).
//!
//! Communicates with a Cashu-compatible mint for:
//! - Keyset discovery (GET /v1/keys, /v1/keysets)
//! - Mint quotes and minting (POST /v1/mint/quote/bolt11, /v1/mint/bolt11)
//! - Melt quotes and melting (POST /v1/melt/quote/bolt11, /v1/melt/bolt11)
//! - Token swaps (POST /v1/swap)
//! - Proof state checks (POST /v1/checkstate)

use super::bdhke;
use super::cashu::{
    amount_to_denominations, is_truncated_v2_keyset_id, BlindSignature, BlindedMessageRequest,
    CashuToken, KeysetInfo, MintKeyset, Proof,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Default timeout for mint API calls.
const MINT_TIMEOUT_SECS: u64 = 10;
/// Timeout for heavy operations (minting with Lightning payment).
const MINT_HEAVY_TIMEOUT_SECS: u64 = 30;

/// Mint quote response (NUT-04).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintQuote {
    pub quote: String,
    pub request: String, // BOLT11 Lightning invoice
    pub state: String,   // "UNPAID", "PAID", "ISSUED"
    #[serde(default)]
    pub expiry: u64,
}

/// Melt quote response (NUT-05).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeltQuote {
    pub quote: String,
    pub amount: u64,
    pub fee_reserve: u64,
    pub state: String, // "UNPAID", "PENDING", "PAID"
    #[serde(default)]
    pub expiry: u64,
}

/// Token state from checkstate (NUT-07).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofState {
    #[serde(rename = "Y")]
    pub y: String,
    pub state: String, // "UNSPENT", "SPENT", "PENDING"
}

/// Result of a swap operation.
pub struct SwapResult {
    pub new_proofs: Vec<Proof>,
}

/// Result of a mint operation.
pub struct MintResult {
    pub proofs: Vec<Proof>,
}

/// Translate a Cashu NUT "transaction validation" error code into plain
/// language a wallet user can act on. Mints respond to a rejected request
/// with `{"code": N, "detail": "..."}`; `detail` is implementation-defined
/// free text, but `code` is the stable identifier from the spec
/// (https://github.com/cashubtc/nuts/blob/main/error_codes.md). Covers the
/// 10001-11017 "proof/transaction validation" range plus the 12001-12003
/// keyset codes shared by NUT-02/03/04/05 — the codes a swap/melt/mint call
/// can actually hit. Returns `None` for anything else (e.g. Lightning/quote
/// codes in the 20000s) so the caller falls back to the mint's own `detail`.
fn describe_mint_error_code(code: i64) -> Option<&'static str> {
    Some(match code {
        10001 => "The mint rejected these coins as invalid.",
        11001 => "This ecash has already been redeemed — it can't be claimed twice.",
        11002 => "This ecash is already being redeemed elsewhere — try again in a moment.",
        11003 => "The mint already issued new coins for this exact request — there's nothing left to redeem.",
        11004 => "This request is still being processed by the mint — try again in a moment.",
        11005 => "The token's amounts don't add up (inputs don't match outputs) — it may be corrupt.",
        11006 => "That amount is outside the range this mint allows.",
        11007 => "This token contains duplicate coins — it may be corrupt or already used.",
        11008 => "The mint rejected this as a duplicate request.",
        11009 | 11010 => "This token mixes incompatible currency units — the mint rejected it.",
        11011 => "That Lightning invoice has no amount, which isn't supported here.",
        11012 => "The amount requested doesn't match the Lightning invoice.",
        11013 => "The mint doesn't support this currency unit.",
        11014 | 11015 => "This token has too many coins for the mint to process in one request.",
        11016 => "Duplicate quote IDs were sent in this request.",
        11017 => "Too many items were sent in a single request.",
        12001 => "The mint no longer recognizes the keyset that signed this token.",
        12002 => "The mint's signing key for this token is inactive.",
        12003 => "The mint's signing key for this token has expired.",
        _ => return None,
    })
}

/// Parse a mint's error body (`{"code": N, "detail": "..."}`) and pick the
/// best user-facing message: the plain-language translation when we know the
/// code, otherwise the mint's own `detail` text, otherwise the raw body.
fn describe_mint_error_body(status: reqwest::StatusCode, body: &str) -> String {
    let parsed: Option<serde_json::Value> = serde_json::from_str(body).ok();
    let code = parsed
        .as_ref()
        .and_then(|v| v.get("code"))
        .and_then(|c| c.as_i64());
    let detail = parsed
        .as_ref()
        .and_then(|v| v.get("detail"))
        .and_then(|d| d.as_str());

    if let Some(friendly) = code.and_then(describe_mint_error_code) {
        return friendly.to_string();
    }
    match detail {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => format!("mint returned {} with no further detail", status),
    }
}

/// Build the error for a failed mint HTTP call: `op` + status + raw body as
/// the technical cause (visible via `{:#}` in logs), with the plain-language
/// translation layered on top via `.context()` so `{}` — what reaches the
/// wallet user — shows something actionable instead of raw mint JSON.
fn mint_error(op: &str, status: reqwest::StatusCode, body: &str) -> anyhow::Error {
    let friendly = describe_mint_error_body(status, body);
    anyhow::anyhow!("{} failed ({}): {}", op, status, body).context(friendly)
}

/// HTTP client for a single Cashu mint.
pub struct MintClient {
    url: String,
    client: reqwest::Client,
}

impl MintClient {
    /// Create a new mint client for the given mint URL.
    pub fn new(mint_url: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(MINT_TIMEOUT_SECS))
            .build()
            .context("Failed to build HTTP client for mint")?;

        Ok(Self {
            url: mint_url.trim_end_matches('/').to_string(),
            client,
        })
    }

    /// Create a mint client with a custom reqwest client (e.g., for Tor proxy).
    pub fn with_client(mint_url: &str, client: reqwest::Client) -> Self {
        Self {
            url: mint_url.trim_end_matches('/').to_string(),
            client,
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    // ── Keyset discovery (NUT-01, NUT-02) ──

    /// Fetch the active keyset from the mint.
    pub async fn get_keys(&self) -> Result<Vec<MintKeyset>> {
        let url = format!("{}/v1/keys", self.url);
        let res = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch mint keys")?;

        if !res.status().is_success() {
            anyhow::bail!("Mint keys request failed: {}", res.status());
        }

        let body: serde_json::Value = res.json().await.context("Failed to parse mint keys")?;
        let keysets: Vec<MintKeyset> = serde_json::from_value(
            body.get("keysets")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .context("Failed to parse keysets")?;

        Ok(keysets)
    }

    /// List the mint's keysets (NUT-02 `GET /v1/keysets`) — ids and status
    /// only, no public keys. Unlike `/v1/keys` this includes *inactive*
    /// keysets, which a received token may well reference: coins from a
    /// retired keyset stay spendable.
    pub async fn get_keysets(&self) -> Result<Vec<KeysetInfo>> {
        let url = format!("{}/v1/keysets", self.url);
        let res = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch mint keysets")?;
        if !res.status().is_success() {
            anyhow::bail!("Mint keysets request failed: {}", res.status());
        }
        let body: serde_json::Value = res.json().await.context("Failed to parse mint keysets")?;
        let keysets: Vec<KeysetInfo> = serde_json::from_value(
            body.get("keysets")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .context("Failed to parse keyset list")?;
        Ok(keysets)
    }

    /// Get the active keyset for the "sat" unit.
    pub async fn get_active_sat_keyset(&self) -> Result<MintKeyset> {
        let keysets = self.get_keys().await?;
        keysets
            .into_iter()
            .find(|k| {
                // Find active sat keyset — check keys map is non-empty
                !k.keys.is_empty()
            })
            .ok_or_else(|| anyhow::anyhow!("No active keyset found at mint {}", self.url))
    }

    // ── Mint quotes (NUT-04) ──

    /// Request a mint quote — returns a Lightning invoice to pay.
    pub async fn mint_quote(&self, amount: u64) -> Result<MintQuote> {
        let url = format!("{}/v1/mint/quote/bolt11", self.url);
        let res = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "amount": amount, "unit": "sat" }))
            .send()
            .await
            .context("Failed to request mint quote")?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(mint_error("Mint quote", status, &body));
        }

        res.json().await.context("Failed to parse mint quote")
    }

    /// Check the status of a mint quote.
    pub async fn mint_quote_status(&self, quote_id: &str) -> Result<MintQuote> {
        let url = format!("{}/v1/mint/quote/bolt11/{}", self.url, quote_id);
        let res = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to check mint quote status")?;

        if !res.status().is_success() {
            anyhow::bail!("Mint quote status check failed: {}", res.status());
        }

        res.json()
            .await
            .context("Failed to parse mint quote status")
    }

    /// Mint tokens after Lightning invoice has been paid.
    /// Performs BDHKE blinding, sends blinded messages to mint, unblinds signatures.
    pub async fn mint_tokens(&self, quote_id: &str, amount: u64) -> Result<MintResult> {
        let keyset = self.get_active_sat_keyset().await?;
        let denominations = amount_to_denominations(amount);

        let mut blinded_messages = Vec::new();
        let mut blinding_data = Vec::new(); // (secret, blinding_factor, amount)

        for &denom in &denominations {
            let secret = bdhke::generate_secret();
            let r = bdhke::random_blinding_factor();
            let blinded = bdhke::blind_message(&secret, &r)?;

            blinded_messages.push(BlindedMessageRequest {
                amount: denom,
                id: keyset.id.clone(),
                b_prime: hex::encode(blinded.b_prime.serialize()),
            });
            blinding_data.push((secret, r, denom));
        }

        let url = format!("{}/v1/mint/bolt11", self.url);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(MINT_HEAVY_TIMEOUT_SECS))
            .build()
            .context("Failed to build client for mint operation")?;

        let res = client
            .post(&url)
            .json(&serde_json::json!({
                "quote": quote_id,
                "outputs": blinded_messages,
            }))
            .send()
            .await
            .context("Failed to mint tokens")?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(mint_error("Minting tokens", status, &body));
        }

        let body: serde_json::Value = res.json().await.context("Failed to parse mint response")?;
        let signatures: Vec<BlindSignature> = serde_json::from_value(
            body.get("signatures")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .context("Failed to parse blind signatures")?;

        if signatures.len() != blinding_data.len() {
            anyhow::bail!(
                "Mint returned {} signatures, expected {}",
                signatures.len(),
                blinding_data.len()
            );
        }

        // Unblind signatures to get real proofs
        let mut proofs = Vec::new();
        for (sig, (secret, r, amount)) in signatures.iter().zip(blinding_data.iter()) {
            let c_prime = sig.c_prime_as_pubkey()?;
            let mint_key = keyset.key_for_amount(*amount)?;
            let c = bdhke::unblind_signature(&c_prime, r, &mint_key)?;

            proofs.push(Proof {
                amount: *amount,
                id: keyset.id.clone(),
                secret: String::from_utf8_lossy(secret).to_string(),
                c: hex::encode(c.serialize()),
            });
        }

        debug!("Minted {} proofs totaling {} sats", proofs.len(), amount);
        Ok(MintResult { proofs })
    }

    // ── Melt (NUT-05) ──

    /// Request a melt quote — how much it costs to pay a Lightning invoice.
    pub async fn melt_quote(&self, bolt11: &str) -> Result<MeltQuote> {
        let url = format!("{}/v1/melt/quote/bolt11", self.url);
        let res = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "request": bolt11, "unit": "sat" }))
            .send()
            .await
            .context("Failed to request melt quote")?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(mint_error("Melt quote", status, &body));
        }

        res.json().await.context("Failed to parse melt quote")
    }

    /// Melt tokens — pay a Lightning invoice using ecash proofs.
    pub async fn melt_tokens(&self, quote_id: &str, proofs: &[Proof]) -> Result<MeltQuote> {
        let url = format!("{}/v1/melt/bolt11", self.url);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(MINT_HEAVY_TIMEOUT_SECS))
            .build()
            .context("Failed to build client for melt operation")?;

        let res = client
            .post(&url)
            .json(&serde_json::json!({
                "quote": quote_id,
                "inputs": proofs,
            }))
            .send()
            .await
            .context("Failed to melt tokens")?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(mint_error("Melt", status, &body));
        }

        res.json().await.context("Failed to parse melt response")
    }

    // ── Swap (NUT-03) ──

    /// Swap proofs for new proofs of different denominations.
    /// This is how we "receive" a token — swap it for fresh proofs that only we know.
    pub async fn swap(&self, inputs: &[Proof], target_amounts: &[u64]) -> Result<SwapResult> {
        let keyset = self.get_active_sat_keyset().await?;

        let mut blinded_messages = Vec::new();
        let mut blinding_data = Vec::new();

        for &amount in target_amounts {
            let secret = bdhke::generate_secret();
            let r = bdhke::random_blinding_factor();
            let blinded = bdhke::blind_message(&secret, &r)?;

            blinded_messages.push(BlindedMessageRequest {
                amount,
                id: keyset.id.clone(),
                b_prime: hex::encode(blinded.b_prime.serialize()),
            });
            blinding_data.push((secret, r, amount));
        }

        let url = format!("{}/v1/swap", self.url);
        let res = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "inputs": inputs,
                "outputs": blinded_messages,
            }))
            .send()
            .await
            .context("Failed to swap tokens")?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(mint_error("Swap", status, &body));
        }

        let body: serde_json::Value = res.json().await.context("Failed to parse swap response")?;
        let signatures: Vec<BlindSignature> = serde_json::from_value(
            body.get("signatures")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .context("Failed to parse swap signatures")?;

        if signatures.len() != blinding_data.len() {
            anyhow::bail!(
                "Swap returned {} signatures, expected {}",
                signatures.len(),
                blinding_data.len()
            );
        }

        let mut new_proofs = Vec::new();
        for (sig, (secret, r, amount)) in signatures.iter().zip(blinding_data.iter()) {
            let c_prime = sig.c_prime_as_pubkey()?;
            let mint_key = keyset.key_for_amount(*amount)?;
            let c = bdhke::unblind_signature(&c_prime, r, &mint_key)?;

            new_proofs.push(Proof {
                amount: *amount,
                id: keyset.id.clone(),
                secret: String::from_utf8_lossy(secret).to_string(),
                c: hex::encode(c.serialize()),
            });
        }

        debug!(
            "Swapped {} inputs for {} new proofs",
            inputs.len(),
            new_proofs.len()
        );
        Ok(SwapResult { new_proofs })
    }

    // ── Check state (NUT-07) ──

    /// Check whether proofs are spent, unspent, or pending.
    pub async fn check_state(&self, proofs: &[Proof]) -> Result<Vec<ProofState>> {
        // Compute Y = hash_to_curve(secret) for each proof
        let ys: Vec<String> = proofs
            .iter()
            .map(|p| {
                let y = bdhke::hash_to_curve(p.secret.as_bytes())?;
                Ok(hex::encode(y.serialize()))
            })
            .collect::<Result<Vec<_>>>()?;

        let url = format!("{}/v1/checkstate", self.url);
        let res = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "Ys": ys }))
            .send()
            .await
            .context("Failed to check proof state")?;

        if !res.status().is_success() {
            anyhow::bail!("Check state failed: {}", res.status());
        }

        let body: serde_json::Value = res
            .json()
            .await
            .context("Failed to parse checkstate response")?;
        let states: Vec<ProofState> =
            serde_json::from_value(body.get("states").cloned().unwrap_or(serde_json::json!([])))
                .context("Failed to parse proof states")?;

        Ok(states)
    }

    /// Receive a CashuToken by swapping its proofs for fresh ones.
    /// This prevents double-spend and ensures only we can spend the new proofs.
    /// Repair proofs whose keyset id is a truncated NUT-02 **v2** id.
    ///
    /// A v2 keyset id is 33 bytes (version byte `0x01` + 32-byte hash), but
    /// wallets written against the original 8-byte format truncate it when
    /// they build a token. The mint then reads the `0x01` version, expects 33
    /// bytes, and rejects the swap — reported as
    /// `inputs[0].id: NUT02: ID length invalid` behind a bare 422 (seen with
    /// a Minibits-issued token, 2026-08-17).
    ///
    /// The id only names which keyset signed the proof, so restoring the full
    /// id the mint advertises is exactly what the sender meant. It is also
    /// safe to attempt: an id that names the wrong keyset fails signature
    /// verification at the mint and no coins move. Anything already valid, or
    /// with no unambiguous match, is passed through untouched so the mint's
    /// own error is what the operator sees.
    async fn resolve_truncated_keyset_ids(&self, proofs: &[Proof]) -> Vec<Proof> {
        let needs_repair = proofs.iter().any(|p| is_truncated_v2_keyset_id(&p.id));
        if !needs_repair {
            return proofs.to_vec();
        }

        // The mint's own keyset list, in the reference implementation's shape
        // so its NUT-02 resolver can consume it directly.
        let known = match self.get_cdk_keysets().await {
            Ok(k) => k,
            Err(e) => {
                debug!("Could not list keysets to repair truncated keyset ids: {e:#}");
                return proofs.to_vec();
            }
        };

        proofs
            .iter()
            .cloned()
            .map(|mut p| {
                if let Some(full) = super::cashu::resolve_keyset_id(&p.id, &known) {
                    debug!("Expanded short keyset id {} to {} for swap", p.id, full);
                    p.id = full;
                }
                p
            })
            .collect()
    }

    /// The mint's keysets as upstream `KeySetInfo`, for NUT-02 id resolution.
    async fn get_cdk_keysets(&self) -> Result<Vec<cashu::nuts::nut02::KeySetInfo>> {
        let url = format!("{}/v1/keysets", self.url);
        let res = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch mint keysets")?;
        if !res.status().is_success() {
            anyhow::bail!("Mint keysets request failed: {}", res.status());
        }
        let body: serde_json::Value = res.json().await.context("Failed to parse mint keysets")?;
        // Deserialize per-entry and keep what parses: a mint may advertise a
        // keyset in a unit or format this build doesn't model, and one such
        // entry must not block resolving the id we actually need.
        let list = body
            .get("keysets")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(list
            .into_iter()
            .filter_map(|v| serde_json::from_value::<cashu::nuts::nut02::KeySetInfo>(v).ok())
            .collect())
    }

    pub async fn receive_token(&self, token: &CashuToken) -> Result<Vec<Proof>> {
        let mut all_new_proofs = Vec::new();

        for entry in &token.token {
            if entry.mint != self.url {
                debug!(
                    "Skipping proofs from different mint {} (ours: {})",
                    entry.mint, self.url
                );
                continue;
            }

            let total: u64 = entry.proofs.iter().map(|p| p.amount).sum();
            let target_amounts = amount_to_denominations(total);

            let proofs = self.resolve_truncated_keyset_ids(&entry.proofs).await;
            let result = self.swap(&proofs, &target_amounts).await?;
            all_new_proofs.extend(result.new_proofs);
        }

        if all_new_proofs.is_empty() {
            anyhow::bail!("No proofs could be swapped — mint mismatch or empty token");
        }

        Ok(all_new_proofs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mint_client_url_normalization() {
        let client = MintClient::new("http://mint.example.com/").unwrap();
        assert_eq!(client.url(), "http://mint.example.com");
    }

    #[test]
    fn test_mint_client_url_no_trailing_slash() {
        let client = MintClient::new("http://mint.example.com").unwrap();
        assert_eq!(client.url(), "http://mint.example.com");
    }
}
