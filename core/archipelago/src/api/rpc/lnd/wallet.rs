use crate::api::rpc::RpcHandler;
use anyhow::{Context, Result};
use base64::Engine;
use tracing::info;
use zeroize::Zeroize;

use super::LND_REST_BASE_URL;

impl RpcHandler {
    // Add inside api/rpc/lnd/wallet.rs RpcHandler impl; seller owns this lookup.
    // Wire invoice-status response to this Value instead of reducing it to paid bool.
    pub(crate) async fn content_invoice_lifecycle(
        &self,
        hash: &str,
        content_id: &str,
    ) -> Result<serde_json::Value> {
        anyhow::ensure!(
            hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit()),
            "Invalid payment hash"
        );
        let hash = hash.to_ascii_lowercase();
        let existing = crate::content_invoice::lookup(&self.config.data_dir, &hash).await?;
        anyhow::ensure!(
            existing.as_ref().is_none_or(|(id, _)| id == content_id),
            "Invoice belongs to another content item"
        );
        if crate::content_invoice::is_paid_for(&self.config.data_dir, &hash, content_id).await {
            return Ok(
                serde_json::json!({"paid":true,"state":"settled","can_switch_method":false}),
            );
        }
        let (client, macaroon_hex) = self.lnd_client().await?;
        let response = client
            .get(format!("{LND_REST_BASE_URL}/v1/invoice/{hash}"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(
                serde_json::json!({"paid":false,"state":"unknown","can_switch_method":false}),
            );
        }
        let body: serde_json::Value = response.error_for_status()?.json().await?;
        let price = content_invoice_amount(&body, content_id)
            .context("Invoice content binding is unavailable")?;
        anyhow::ensure!(
            existing
                .as_ref()
                .is_none_or(|(_, expected)| *expected == price),
            "Invoice price binding changed"
        );
        crate::content_invoice::record_pending(&self.config.data_dir, &hash, content_id, price)
            .await?;
        let result = content_invoice_lifecycle_body(&body, price);
        if result["paid"] == true {
            crate::content_invoice::mark_paid(&self.config.data_dir, &hash).await?;
        }
        Ok(result)
    }

    /// Generate a new on-chain Bitcoin address.
    pub(in crate::api::rpc) async fn handle_lnd_newaddress(&self) -> Result<serde_json::Value> {
        let (client, macaroon_hex) = self.lnd_client().await.map_err(|e| {
            tracing::warn!(error = %format!("{e:#}"), "LND newaddress: client/macaroon unavailable");
            receive_error(
                RECEIVE_WALLET_UNINITIALIZED,
                "The Lightning wallet isn't set up on this node yet. Finish wallet setup, then try again.",
            )
        })?;

        let resp = match client
            .get(format!("{LND_REST_BASE_URL}/v1/newaddress"))
            // LND's REST gateway parses `type` as the AddressType enum by its
            // proto name (or integer), NOT the lncli aliases. "p2wkh" is not a
            // valid enum value and returns 400 "parsing field type"; the native
            // SegWit (bech32) variant is WITNESS_PUBKEY_HASH (= 0).
            .query(&[("type", "WITNESS_PUBKEY_HASH")])
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                // The .116 case: LND container is up but its REST endpoint isn't
                // reachable on the expected port (e.g. published-port drift), so
                // the connection is refused. This is NOT a locked wallet — emit a
                // distinct code so the UI stops mislabelling it.
                tracing::warn!(error = %format!("{e:#}"), "LND newaddress: REST connection failed");
                return Err(receive_error(
                    RECEIVE_REST_UNREACHABLE,
                    "The Lightning wallet service isn't reachable yet. It may be starting up or recovering — please try again in a moment.",
                ));
            }
        };

        let status = resp.status();
        let raw_body = resp
            .text()
            .await
            .context("Bitcoin address response could not be read")?;
        let body: serde_json::Value = serde_json::from_str(&raw_body).unwrap_or_else(|_| {
            serde_json::json!({
                "raw": raw_body,
            })
        });

        if !status.is_success() {
            let message = lnd_error_message(&body);
            let code = classify_lnd_address_error(&message);
            tracing::warn!(%status, lnd_message = %message, code, "LND newaddress returned an error");
            return Err(receive_error(code, default_receive_detail(code)));
        }

        if let Some(error) = body
            .get("error")
            .or_else(|| body.get("message"))
            .and_then(|v| v.as_str())
        {
            let code = classify_lnd_address_error(error);
            tracing::warn!(lnd_message = %error, code, "LND newaddress returned an error body");
            return Err(receive_error(code, default_receive_detail(code)));
        }

        let address = body
            .get("address")
            .and_then(|v| v.as_str())
            .filter(|addr| !addr.trim().is_empty())
            .ok_or_else(|| {
                receive_error(
                    RECEIVE_WALLET_UNINITIALIZED,
                    "The wallet didn't return an address yet. It may still be unlocking or waiting for Bitcoin to sync — please try again shortly.",
                )
            })?
            .to_string();

        Ok(serde_json::json!({ "address": address }))
    }

    /// Send on-chain Bitcoin to an address.
    pub(in crate::api::rpc) async fn handle_lnd_sendcoins(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let addr = params
            .get("addr")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'addr' parameter"))?;
        // send_all sweeps the entire confirmed on-chain balance (LND computes
        // the amount after fees); amount is required otherwise.
        let send_all = params
            .get("send_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let amount = if send_all {
            None
        } else {
            let amount = params
                .get("amount")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'amount' parameter (sats)"))?;
            if amount < 546 {
                return Err(anyhow::anyhow!(
                    "Amount must be at least 546 sats (dust limit)"
                ));
            }
            if amount > 21_000_000 * 100_000_000 {
                return Err(anyhow::anyhow!("Amount exceeds maximum Bitcoin supply"));
            }
            Some(amount)
        };

        // Validate Bitcoin address format (basic: length and allowed chars)
        if addr.len() < 14 || addr.len() > 90 || !addr.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(anyhow::anyhow!("Invalid Bitcoin address format"));
        }

        // Omitted fees target the next block; explicit slower/custom choices win.
        let (target_conf, sat_per_vbyte) = super::fee_policy::fee_options(&params)?;

        info!(
            addr = addr,
            amount = amount,
            send_all = send_all,
            target_conf = target_conf,
            sat_per_vbyte = sat_per_vbyte,
            "Sending on-chain Bitcoin"
        );

        let (client, macaroon_hex) = self.lnd_client().await?;

        let mut send_body = match amount {
            Some(amount) => serde_json::json!({
                "addr": addr,
                "amount": amount.to_string(),
            }),
            None => serde_json::json!({
                "addr": addr,
                "send_all": true,
            }),
        };
        if let Some(tc) = target_conf {
            send_body["target_conf"] = serde_json::json!(tc);
        }
        if let Some(rate) = sat_per_vbyte {
            // LND REST encodes uint64 as a JSON string
            send_body["sat_per_vbyte"] = serde_json::json!(rate.to_string());
        }

        let resp = client
            .post(format!("{LND_REST_BASE_URL}/v1/transactions"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .json(&send_body)
            .send()
            .await
            .context("Failed to send on-chain transaction")?;

        let status = resp.status();
        let body: serde_json::Value = resp.json().await.context("Failed to parse send response")?;

        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            // LND's sweep refusal reads like a debug dump ("insufficient
            // input to create sweep tx: input_sum=0 BTC, output_sum=…").
            // input_sum=0 with a tiny output means the wallet's coins are
            // unconfirmed or below Bitcoin's dust minimum — say that
            // (a test node sweep of 92 sats, 2026-08-06).
            if msg.contains("insufficient input to create sweep tx") {
                return Err(anyhow::anyhow!(
                    "Failed to send: your on-chain balance is too small or still \
                     unconfirmed to sweep. Bitcoin cannot build a transaction from \
                     coins below the dust minimum (~546 sats) or from funds that \
                     have not confirmed yet. (LND: {msg})"
                ));
            }
            return Err(anyhow::anyhow!("Failed to send: {}", msg));
        }

        let txid = body
            .get("txid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Ok(serde_json::json!({ "txid": txid }))
    }

    /// Estimate the on-chain fee for sending `amount` sats to `addr` at a
    /// confirmation target. Returns `{ fee_sat, sat_per_vbyte }` so the send
    /// UI can show the cost of each preset before the user confirms.
    pub(in crate::api::rpc) async fn handle_lnd_estimatefee(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let addr = params
            .get("addr")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'addr' parameter"))?;
        if addr.len() < 14 || addr.len() > 90 || !addr.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(anyhow::anyhow!("Invalid Bitcoin address format"));
        }
        let amount = params
            .get("amount")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'amount' parameter (sats)"))?;
        if !(546..=21_000_000 * 100_000_000).contains(&amount) {
            return Err(anyhow::anyhow!("Invalid amount"));
        }
        let (target_conf, custom_rate) = super::fee_policy::fee_options(&params)?;
        anyhow::ensure!(
            custom_rate.is_none(),
            "Fee estimation requires a confirmation target"
        );
        let target_conf = target_conf.unwrap_or(super::fee_policy::DEFAULT_TARGET);

        let (client, macaroon_hex) = self.lnd_client().await?;

        let resp = client
            .get(format!("{LND_REST_BASE_URL}/v1/transactions/fee"))
            .query(&[
                (format!("AddrToAmount[{addr}]"), amount.to_string()),
                ("target_conf".to_string(), target_conf.to_string()),
            ])
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .context("Failed to estimate fee")?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse fee estimate response")?;

        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("Failed to estimate fee: {}", msg));
        }

        // LND REST encodes int64 as JSON strings
        let fee_sat = body
            .get("fee_sat")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let sat_per_vbyte = body
            .get("sat_per_vbyte")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        Ok(serde_json::json!({
            "fee_sat": fee_sat,
            "sat_per_vbyte": sat_per_vbyte,
        }))
    }

    /// Create a Lightning invoice.
    /// Create a Lightning invoice and return `(bolt11, payment_hash_hex)`.
    ///
    /// Shared helper used by both the `lnd.createinvoice` RPC and the seller-side
    /// peer-file invoice flow (#46). LND returns `r_hash` as base64; we re-encode
    /// it as hex so it can be used as a stable lookup key and passed in URLs.
    /// Whether LND reports it's synced to its Bitcoin chain backend. Used to
    /// fail invoice minting FAST with a clear reason while the node's Bitcoin
    /// backend is still in initial block download — otherwise the `/v1/invoices`
    /// POST hangs for the full client timeout (×3 retries ≈ 45s) and surfaces as
    /// an opaque failure. `getinfo` answers in ~2s even mid-IBD. Returns
    /// `Some(false)` only when LND is reachable AND explicitly not synced;
    /// `None` when we couldn't tell (let the mint attempt proceed and report its
    /// own error rather than guess "syncing").
    pub(crate) async fn lnd_chain_synced(&self) -> Option<bool> {
        let (client, macaroon_hex) = self.lnd_client().await.ok()?;
        let resp = client
            .get(format!("{LND_REST_BASE_URL}/v1/getinfo"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .ok()?;
        let body: serde_json::Value = resp.json().await.ok()?;
        body.get("synced_to_chain").and_then(|v| v.as_bool())
    }

    /// Error returned when the node can't mint a Lightning invoice because its
    /// Bitcoin backend is still syncing. Kept as one string so every invoice
    /// entry point surfaces the same clear, user-facing reason.
    fn syncing_invoice_err() -> anyhow::Error {
        anyhow::anyhow!(
            "Your Bitcoin node is still syncing — Lightning invoices are unavailable until it finishes. Try again once the node is fully synced."
        )
    }

    pub(crate) async fn create_invoice(
        &self,
        amount_sats: i64,
        memo: &str,
    ) -> Result<(String, String)> {
        if amount_sats < 0 {
            return Err(anyhow::anyhow!("Amount must be non-negative"));
        }
        if memo.len() > 639 {
            return Err(anyhow::anyhow!("Memo too long (max 639 bytes)"));
        }

        let (client, macaroon_hex) = self.lnd_client().await?;
        // See `build_invoice_request_body` for why `private: true` is
        // unconditional — this is the seller-side/peer-file invoice path,
        // the twin of the bug fixed in `handle_lnd_createinvoice` below.
        let invoice_body = build_invoice_request_body(amount_sats, memo);
        // LND's REST endpoint can briefly drop/reset connections under load
        // (swap pressure, just-restarted, TLS handshake races), which used to
        // hard-fail the buy-file invoice with an opaque 503. Retry on a
        // CONNECTION error with short backoff so a transient blip doesn't
        // surface as a payment failure. A *timeout* is NOT retried: it means LND
        // accepted the connection but isn't answering the mint (e.g. a degraded
        // node), and retrying just multiplies the wait (3×15s ≈ 45s) — fail
        // after the first hang and let the caller surface the real reason.
        let mut last_err: Option<anyhow::Error> = None;
        let mut resp = None;
        for attempt in 0..3u32 {
            match client
                .post(format!("{LND_REST_BASE_URL}/v1/invoices"))
                .header("Grpc-Metadata-macaroon", &macaroon_hex)
                .json(&invoice_body)
                .send()
                .await
            {
                Ok(r) => {
                    resp = Some(r);
                    break;
                }
                Err(e) => {
                    let timed_out = e.is_timeout();
                    last_err = Some(anyhow::anyhow!(
                        "LND REST send failed (attempt {}): {e}",
                        attempt + 1
                    ));
                    if timed_out {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                }
            }
        }
        let resp = match resp {
            Some(r) => r,
            None => {
                // If LND is reachable but explicitly not synced to chain, say so —
                // it's the most common reason a just-restored/syncing node can't
                // mint. Otherwise surface the underlying transport error.
                if self.lnd_chain_synced().await == Some(false) {
                    return Err(Self::syncing_invoice_err());
                }
                return Err(last_err.unwrap_or_else(|| {
                    anyhow::anyhow!("Failed to reach LND REST to create invoice")
                }));
            }
        };

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse invoice response")?;
        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("Failed to create invoice: {}", msg));
        }

        let payment_request = body
            .get("payment_request")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // r_hash is base64 in LND's REST response — convert to hex.
        use base64::Engine;
        let payment_hash_hex = body
            .get("r_hash")
            .and_then(|v| v.as_str())
            .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok())
            .map(hex::encode)
            .unwrap_or_default();

        Ok((payment_request, payment_hash_hex))
    }

    /// Look up an invoice by hex payment hash; true if it has settled.
    pub(crate) async fn invoice_is_settled(&self, payment_hash_hex: &str) -> Result<bool> {
        if payment_hash_hex.is_empty() || hex::decode(payment_hash_hex).is_err() {
            return Err(anyhow::anyhow!("Invalid payment hash"));
        }
        let (client, macaroon_hex) = self.lnd_client().await?;
        // LND REST: GET /v1/invoice/{r_hash_str} where r_hash_str is hex.
        let resp = client
            .get(format!("{LND_REST_BASE_URL}/v1/invoice/{payment_hash_hex}"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .context("Failed to look up invoice")?;
        if !resp.status().is_success() {
            return Ok(false);
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse invoice lookup response")?;
        let settled = body
            .get("settled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            || body.get("state").and_then(|v| v.as_str()) == Some("SETTLED");
        Ok(settled)
    }

    /// Verify against LND at download time, rather than relying on a browser
    /// having polled first. The memo/amount also recover pre-upgrade in-memory
    /// entitlements after restart; unrelated invoices never unlock a file.
    pub(crate) async fn settle_content_invoice(
        &self,
        hash: &str,
        content_id: &str,
    ) -> Result<bool> {
        anyhow::ensure!(
            hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit()),
            "Invalid payment hash"
        );
        let hash = hash.to_ascii_lowercase();
        let existing = crate::content_invoice::lookup(&self.config.data_dir, &hash).await?;
        if let Some((id, _)) = &existing {
            if id != content_id {
                return Ok(false);
            }
        }
        if crate::content_invoice::is_paid_for(&self.config.data_dir, &hash, content_id).await {
            return Ok(true);
        }
        let (client, macaroon_hex) = self.lnd_client().await?;
        let response = client
            .get(format!("{LND_REST_BASE_URL}/v1/invoice/{hash}"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        let body: serde_json::Value = response.error_for_status()?.json().await?;
        let Some(price) = content_invoice_amount(&body, content_id) else {
            return Ok(false);
        };
        if existing
            .as_ref()
            .is_some_and(|(_, expected)| *expected != price)
        {
            return Ok(false);
        }
        crate::content_invoice::record_pending(&self.config.data_dir, &hash, content_id, price)
            .await?;
        let settled = content_invoice_fully_settled(&body, price);
        if settled {
            crate::content_invoice::mark_paid(&self.config.data_dir, &hash).await?;
        }
        Ok(settled)
    }

    /// Generate a fresh on-chain receive address (seller side, #46).
    pub(crate) async fn new_onchain_address(&self) -> Result<String> {
        let (client, macaroon_hex) = self.lnd_client().await?;
        let resp = client
            .get(format!("{LND_REST_BASE_URL}/v1/newaddress"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .context("Failed to get new address")?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("LND newaddress failed: {}", resp.status()));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse newaddress response")?;
        body.get("address")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("LND newaddress returned no address"))
    }

    /// True if an on-chain payment of >= `min_sats` to `address` has been seen
    /// with at least one confirmation (seller side, #46). Conservative on
    /// purpose: requires a confirmation + exact-address + sufficient-amount so a
    /// file sale is never released on an unconfirmed (reorg-able) tx.
    pub(crate) async fn onchain_received(&self, address: &str, min_sats: u64) -> Result<bool> {
        if address.is_empty() {
            return Err(anyhow::anyhow!("Empty address"));
        }
        let (client, macaroon_hex) = self.lnd_client().await?;
        let resp = client
            .get(format!("{LND_REST_BASE_URL}/v1/transactions"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .context("Failed to list transactions")?;
        anyhow::ensure!(
            resp.status().is_success(),
            "Wallet transaction verification is unavailable"
        );
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse transactions response")?;
        Ok(confirmed_address_sats(&body, address)? >= min_sats)
    }

    pub(in crate::api::rpc) async fn handle_lnd_createinvoice(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let amount_sats = params
            .get("amount_sats")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'amount_sats' parameter"))?;
        let memo = params.get("memo").and_then(|v| v.as_str()).unwrap_or("");

        if amount_sats < 0 {
            return Err(anyhow::anyhow!("Amount must be non-negative"));
        }
        if amount_sats > 21_000_000 * 100_000_000 {
            return Err(anyhow::anyhow!("Amount exceeds maximum Bitcoin supply"));
        }

        // Limit memo length to prevent abuse
        if memo.len() > 639 {
            return Err(anyhow::anyhow!("Memo too long (max 639 bytes)"));
        }

        info!(amount_sats = amount_sats, "Creating Lightning invoice");

        let (client, macaroon_hex) = self.lnd_client().await?;

        // See `build_invoice_request_body` for why `private: true` is
        // unconditional. This is the wallet UI's Receive flow / the
        // `lnd.createinvoice` RPC — the bug diagnosed on a test node.
        let invoice_body = build_invoice_request_body(amount_sats, memo);

        let resp = match client
            .post(format!("{LND_REST_BASE_URL}/v1/invoices"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .json(&invoice_body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                // A hung/failed mint while LND is explicitly not synced to chain
                // gets a clear, user-facing reason instead of an opaque error.
                if self.lnd_chain_synced().await == Some(false) {
                    return Err(Self::syncing_invoice_err());
                }
                return Err(anyhow::anyhow!(e).context("Failed to create invoice"));
            }
        };

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse invoice response")?;

        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("Failed to create invoice: {}", msg));
        }

        let payment_request = body
            .get("payment_request")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // LND returns r_hash base64-encoded; the lookup endpoint the Receive
        // flow polls (`lnd.invoicestatus`) wants it hex — hand the UI the
        // ready-to-use form.
        let r_hash_hex = {
            use base64::Engine as _;
            body.get("r_hash")
                .and_then(|v| v.as_str())
                .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok())
                .map(hex::encode)
                .unwrap_or_default()
        };

        Ok(serde_json::json!({
            "payment_request": payment_request,
            "amount_sats": amount_sats,
            "r_hash_hex": r_hash_hex,
        }))
    }

    /// lnd.invoicestatus — is this invoice settled yet? Polled by the wallet's
    /// Receive flow so a Lightning payment gets the same "money has arrived"
    /// success screen as on-chain (minus the broadcast step: settlement is
    /// final). Params: `{ "r_hash_hex": string }`.
    pub(in crate::api::rpc) async fn handle_lnd_invoicestatus(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let r_hash_hex = params
            .get("r_hash_hex")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'r_hash_hex' parameter"))?;
        if r_hash_hex.len() != 64 || !r_hash_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(anyhow::anyhow!("r_hash_hex must be 64 hex characters"));
        }

        let (client, macaroon_hex) = self.lnd_client().await?;
        let resp = client
            .get(format!("{LND_REST_BASE_URL}/v1/invoice/{r_hash_hex}"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .context("Failed to query invoice")?;
        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse invoice lookup response")?;
        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("Invoice lookup failed: {}", msg));
        }

        let settled = body
            .get("state")
            .and_then(|v| v.as_str())
            .map(|s| s == "SETTLED")
            .unwrap_or_else(|| {
                body.get("settled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            });
        let amt_paid_sat = body
            .get("amt_paid_sat")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .or_else(|| body.get("amt_paid_sat").and_then(|v| v.as_i64()))
            .unwrap_or(0);

        Ok(serde_json::json!({
            "settled": settled,
            "amt_paid_sat": amt_paid_sat,
        }))
    }

    /// Create an unsigned PSBT for hardware wallet signing.
    /// Uses LND's WalletKit.FundPsbt to select UTXOs and create a PSBT template.
    pub(in crate::api::rpc) async fn handle_lnd_create_psbt(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let outputs = params
            .get("outputs")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                anyhow::anyhow!("Missing 'outputs' array (each: address + amount_sats)")
            })?;

        if outputs.is_empty() {
            return Err(anyhow::anyhow!("outputs must not be empty"));
        }

        // Build the outputs map for LND: { "address": "amount_sats_as_string" }
        let mut lnd_outputs: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        let mut total_amount: i64 = 0;
        for output in outputs {
            let addr = output
                .get("address")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Each output must have an 'address'"))?;
            // Validate Bitcoin address format
            if addr.len() < 14
                || addr.len() > 90
                || !addr.chars().all(|c| c.is_ascii_alphanumeric())
            {
                return Err(anyhow::anyhow!("Invalid Bitcoin address format in output"));
            }
            let amount = output
                .get("amount_sats")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow::anyhow!("Each output must have 'amount_sats'"))?;
            if amount < 546 {
                return Err(anyhow::anyhow!(
                    "Amount must be at least 546 sats (dust limit)"
                ));
            }
            lnd_outputs.insert(addr.to_string(), serde_json::json!(amount));
            total_amount += amount;
        }

        let (_, explicit_rate) = super::fee_policy::fee_options(&serde_json::json!({
            "sat_per_vbyte": params.get("fee_rate_sat_per_vbyte")
        }))?;
        let (client, macaroon_hex) = self.lnd_client().await?;
        let sat_per_vbyte = if let Some(rate) = explicit_rate {
            rate as u64
        } else {
            let response = client
                .get(format!("{LND_REST_BASE_URL}/v2/wallet/estimatefee/1"))
                .header("Grpc-Metadata-macaroon", &macaroon_hex)
                .send()
                .await
                .context("Cannot estimate the next-block fee")?
                .error_for_status()
                .context("Next-block fee estimate rejected")?;
            let estimate: serde_json::Value = response.json().await?;
            super::fee_policy::estimated_sat_per_vbyte(&estimate)?
        };

        info!(
            total_amount = total_amount,
            fee_rate = sat_per_vbyte,
            "Creating PSBT for hardware wallet signing"
        );

        let fund_body = serde_json::json!({
            "raw": {
                "outputs": lnd_outputs,
            },
            "sat_per_vbyte": sat_per_vbyte,
            "spend_unconfirmed": false,
        });

        let resp = client
            .post(format!("{LND_REST_BASE_URL}/v2/wallet/psbt/fund"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .json(&fund_body)
            .send()
            .await
            .context("Failed to create PSBT via LND")?;

        let status = resp.status();
        let body: serde_json::Value = resp.json().await.context("Failed to parse PSBT response")?;

        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("Failed to create PSBT: {}", msg));
        }

        let funded_psbt = body
            .get("funded_psbt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let change_output_index = body
            .get("change_output_index")
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);

        // Report whether this PSBT carries the BIP-32 key-origin data an external
        // signer needs to locate its own key. Best-effort by design: a decode
        // failure degrades to `null`, never to an error. A user's send must not
        // fail because an inspection helper could not parse something.
        let key_origin = match psbt_key_origin_report(&funded_psbt) {
            Ok(report) => {
                if !report.all_inputs_have_key_origin {
                    // This is the exact condition under which a hardware signer
                    // refuses the PSBT, so name it here rather than letting the
                    // user discover it as an opaque failure at the device.
                    tracing::warn!(
                        input_count = report.input_count,
                        inputs_with_key_origin = report.inputs_with_key_origin,
                        "PSBT is missing BIP-32 key origin on one or more inputs; an external signer will not be able to locate its key"
                    );
                }
                serde_json::json!({
                    "input_count": report.input_count,
                    "inputs_with_key_origin": report.inputs_with_key_origin,
                    "all_inputs_have_key_origin": report.all_inputs_have_key_origin,
                })
            }
            Err(e) => {
                tracing::warn!(
                    error = %format!("{e:#}"),
                    "Could not inspect PSBT for key origin; reporting null"
                );
                serde_json::Value::Null
            }
        };

        Ok(serde_json::json!({
            "psbt_base64": funded_psbt,
            "change_output_index": change_output_index,
            "total_amount_sats": total_amount,
            "fee_rate_sat_per_vbyte": sat_per_vbyte,
            "key_origin": key_origin,
        }))
    }

    /// Finalize a signed PSBT and broadcast the transaction.
    /// Takes a PSBT that has been signed by a hardware wallet.
    pub(in crate::api::rpc) async fn handle_lnd_finalize_psbt(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let signed_psbt = params
            .get("signed_psbt_base64")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'signed_psbt_base64'"))?;

        info!("Finalizing signed PSBT from hardware wallet");

        let (client, macaroon_hex) = self.lnd_client().await?;

        let finalize_body = serde_json::json!({
            "funded_psbt": signed_psbt,
        });

        let resp = client
            .post(format!("{LND_REST_BASE_URL}/v2/wallet/psbt/finalize"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .json(&finalize_body)
            .send()
            .await
            .context("Failed to finalize PSBT via LND")?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse finalize response")?;

        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("Failed to finalize PSBT: {}", msg));
        }

        let raw_final_tx = body
            .get("raw_final_tx")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Broadcast the finalized transaction
        let publish_body = serde_json::json!({
            "tx_hex": raw_final_tx,
        });

        let pub_resp = client
            .post(format!("{LND_REST_BASE_URL}/v2/wallet/tx"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .json(&publish_body)
            .send()
            .await
            .context("Failed to broadcast transaction")?;

        let pub_status = pub_resp.status();
        let pub_body: serde_json::Value = pub_resp
            .json()
            .await
            .context("Failed to parse broadcast response")?;

        if !pub_status.is_success() {
            let msg = pub_body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("Transaction broadcast failed: {}", msg));
        }

        Ok(serde_json::json!({
            "raw_final_tx": raw_final_tx,
            "broadcast": true,
        }))
    }

    /// Create a signed raw transaction WITHOUT broadcasting.
    /// Used for mesh relay: create the TX locally, then relay the hex to an
    /// internet-connected peer who broadcasts it.
    pub(in crate::api::rpc) async fn handle_lnd_create_raw_tx(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let addr = params
            .get("addr")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'addr'"))?;
        let amount_sats = params
            .get("amount_sats")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'amount_sats'"))?;

        if amount_sats < 546 {
            anyhow::bail!("Amount must be at least 546 sats (dust limit)");
        }
        if amount_sats > 2_100_000_000_000_000 {
            anyhow::bail!("Amount exceeds 21M BTC");
        }

        let (client, macaroon_hex) = self.lnd_client().await?;

        // Step 1: Fund a PSBT with the desired output
        let fee_rate = params.get("fee_rate").and_then(|v| v.as_u64()).unwrap_or(5);
        let fund_body = serde_json::json!({
            "raw": {
                "outputs": { addr: amount_sats }
            },
            "sat_per_vbyte": fee_rate,
            "spend_unconfirmed": false,
        });

        let resp = client
            .post(format!("{LND_REST_BASE_URL}/v2/wallet/psbt/fund"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .json(&fund_body)
            .send()
            .await
            .context("Failed to fund PSBT via LND")?;

        let status = resp.status();
        let body: serde_json::Value = resp.json().await.context("Failed to parse fund response")?;

        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("Failed to create TX: {}", msg));
        }

        let funded_psbt = body
            .get("funded_psbt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No funded_psbt in response"))?;

        // Step 2: Finalize (LND auto-signs with hot wallet keys)
        let finalize_body = serde_json::json!({
            "funded_psbt": funded_psbt,
        });

        let resp = client
            .post(format!("{LND_REST_BASE_URL}/v2/wallet/psbt/finalize"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .json(&finalize_body)
            .send()
            .await
            .context("Failed to finalize PSBT")?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse finalize response")?;

        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("Failed to sign TX: {}", msg));
        }

        // raw_final_tx from LND is base64-encoded -- decode to hex for Bitcoin RPC
        let raw_final_tx_b64 = body
            .get("raw_final_tx")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No raw_final_tx in response"))?;

        let tx_bytes = base64::engine::general_purpose::STANDARD
            .decode(raw_final_tx_b64)
            .context("Failed to decode raw_final_tx base64")?;
        let raw_tx_hex = hex::encode(&tx_bytes);

        info!(
            addr,
            amount_sats,
            tx_len = raw_tx_hex.len(),
            "Created raw TX for mesh relay (NOT broadcast)"
        );

        Ok(serde_json::json!({
            "raw_tx_hex": raw_tx_hex,
            "amount_sats": amount_sats,
            "addr": addr,
            "broadcast": false,
        }))
    }

    /// Initialize LND wallet with entropy derived from the node's BIP-39 master seed.
    /// The 16-byte entropy deterministically produces an aezeed mnemonic inside LND.
    /// Requires: password re-verification via params.password, encrypted seed on disk.
    pub(in crate::api::rpc) async fn handle_lnd_init_wallet_from_seed(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let password = params
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'password' for seed access"))?;
        let wallet_password = params
            .get("wallet_password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'wallet_password' for LND"))?;

        // Verify user password before granting seed access.
        self.auth_manager
            .verify_password(password)
            .await
            .context("Password verification failed")?;

        // Load encrypted seed from disk.
        let mnemonic = crate::seed::load_seed_encrypted(&self.config.data_dir, password)
            .await
            .context("Failed to load encrypted seed. Was a seed phrase saved during onboarding?")?;
        let seed = crate::seed::MasterSeed::from_mnemonic(&mnemonic);

        // Derive 16 bytes of LND entropy.
        let mut entropy = crate::seed::derive_lnd_entropy(&seed)?;
        let entropy_b64 = base64::engine::general_purpose::STANDARD.encode(entropy);
        entropy.zeroize();

        // Use the per-node secret as the LND wallet password (NOT the
        // caller-supplied one) so the unattended boot path can auto-unlock this
        // wallet. The wallet stays recoverable from the Archipelago seed via the
        // derived entropy above. This unifies both init paths on one password
        // source — the divergence here is what left wallets locked fleet-wide.
        let _ = wallet_password; // accepted for API compat; superseded by the per-node secret
        let node_wallet_pw = crate::container::lnd::ensure_wallet_password().await?;
        let wallet_password_b64 =
            base64::engine::general_purpose::STANDARD.encode(node_wallet_pw.as_bytes());

        // LND must be running but NOT yet initialized (no existing wallet).
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(30))
            .danger_accept_invalid_certs(true)
            .build()
            .context("Failed to create HTTP client")?;

        // InitWallet does NOT accept raw entropy — `seed_entropy` is a GenSeed
        // field. Posting it to /v1/initwallet meant the wallet was never
        // actually derived from the Archipelago seed. GenSeed(entropy) returns
        // the deterministic aezeed words, which InitWallet then consumes — and
        // which we capture for the encrypted seed backup (lnd.seed-reveal).
        let genseed_resp = client
            .get(format!("{LND_REST_BASE_URL}/v1/genseed"))
            .query(&[("seed_entropy", entropy_b64.as_str())])
            .send()
            .await
            .context("LND genseed request failed — is LND running and uninitialized?")?;
        let genseed_status = genseed_resp.status();
        let genseed_body: serde_json::Value = genseed_resp
            .json()
            .await
            .context("Failed to parse genseed response")?;
        if !genseed_status.is_success() {
            let msg = genseed_body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("LND seed generation failed: {}", msg));
        }
        let cipher_seed_mnemonic: Vec<String> = genseed_body
            .get("cipher_seed_mnemonic")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|w| w.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if cipher_seed_mnemonic.is_empty() {
            anyhow::bail!("LND genseed returned no seed words");
        }

        let init_body = serde_json::json!({
            "wallet_password": wallet_password_b64,
            "cipher_seed_mnemonic": cipher_seed_mnemonic,
        });

        let resp = client
            .post(format!("{LND_REST_BASE_URL}/v1/initwallet"))
            .json(&init_body)
            .send()
            .await
            .context("LND initwallet request failed — is LND running and uninitialized?")?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse initwallet response")?;

        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("LND wallet init failed: {}", msg));
        }

        crate::container::lnd::persist_aezeed_backup(
            &self.config.data_dir,
            &cipher_seed_mnemonic,
            &node_wallet_pw,
        )
        .await;

        info!("LND wallet initialized from master seed entropy");

        Ok(serde_json::json!({
            "initialized": true,
        }))
    }
}

fn lnd_error_message(body: &serde_json::Value) -> String {
    body.get("message")
        .or_else(|| body.get("error"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("unknown LND error")
        .to_string()
}

// Stable, machine-readable reason codes for receive-address failures. They are
// embedded in the error message as a `[CODE]` token so the frontend
// (neode-ui/src/utils/bitcoinReceive.ts) can show an accurate explanation
// instead of guessing wallet state by substring-matching — which is what made
// .228 report "wallet is locked" when LND's REST was merely unreachable.
//
// Every receive error string starts with "Bitcoin address" so it survives the
// RPC error sanitizer (api/rpc/middleware.rs) unchanged rather than being
// flattened to the generic "Operation failed" message (the .116 symptom).
pub(crate) const RECEIVE_REST_UNREACHABLE: &str = "LND_REST_UNREACHABLE";
pub(crate) const RECEIVE_WALLET_LOCKED: &str = "LND_WALLET_LOCKED";
pub(crate) const RECEIVE_WALLET_UNINITIALIZED: &str = "LND_WALLET_UNINITIALIZED";
pub(crate) const RECEIVE_SYNCING: &str = "LND_SYNCING";
pub(crate) const RECEIVE_LND_ERROR: &str = "LND_ERROR";

/// Build a receive-address error carrying a reason code the UI can map.
fn receive_error(code: &str, detail: &str) -> anyhow::Error {
    anyhow::anyhow!("Bitcoin address unavailable [{code}]: {detail}")
}

/// A sensible default human message per code (used for non-UI callers and logs;
/// the frontend renders its own copy from the code).
fn default_receive_detail(code: &str) -> &'static str {
    match code {
        RECEIVE_REST_UNREACHABLE => {
            "The Lightning wallet service isn't reachable yet. It may be starting up or recovering — please try again in a moment."
        }
        RECEIVE_WALLET_LOCKED => {
            "The Lightning wallet is locked. Unlock it (or finish wallet setup), then try again."
        }
        RECEIVE_WALLET_UNINITIALIZED => {
            "The Lightning wallet isn't set up yet. Finish wallet setup, then try again."
        }
        RECEIVE_SYNCING => {
            "The wallet is still syncing with the Bitcoin network. Please try again once it has caught up."
        }
        _ => "Couldn't generate a Bitcoin address right now. Please try again shortly.",
    }
}

/// Classify a non-2xx LND error body/message into a reason code. The wording of
/// LND's REST errors is stable enough to bucket: a locked wallet, an
/// uninitialized wallet, a syncing chain, or some other failure.
fn classify_lnd_address_error(message: &str) -> &'static str {
    let m = message.to_lowercase();
    if m.contains("locked") || m.contains("unlock") {
        RECEIVE_WALLET_LOCKED
    } else if m.contains("synchroniz")
        || m.contains("syncing")
        || m.contains("not yet ready")
        || m.contains("in the process of starting")
    {
        RECEIVE_SYNCING
    } else if m.contains("wallet not found")
        || m.contains("not exist")
        || m.contains("uninitialized")
        || m.contains("not initialized")
        || m.contains("create a wallet")
        || m.contains("no wallet")
    {
        RECEIVE_WALLET_UNINITIALIZED
    } else {
        RECEIVE_LND_ERROR
    }
}

/// Build the LND `/v1/invoices` POST body used to mint a Lightning invoice.
///
/// `private: true` is unconditional and must stay that way: it tells LND to
/// embed a route hint for every private/unannounced channel this node
/// holds, which is the only way a payment can find its way in over such a
/// channel. It is harmless when the node also has public channels — LND
/// still routes directly over a public channel when it can, and the hint is
/// simply an unused alternate path.
///
/// Shared by both invoice-creation call sites (`create_invoice`, the
/// seller-side/peer-file flow, and `handle_lnd_createinvoice`, the wallet
/// UI's Receive flow / `lnd.createinvoice` RPC) so a single test pins the
/// field for both and neither can silently drift back to `false`.
fn build_invoice_request_body(amount_sats: i64, memo: &str) -> serde_json::Value {
    serde_json::json!({
        "value": amount_sats.to_string(),
        "memo": memo,
        "private": true,
    })
}

/// What an external signer needs in order to find its own key in a PSBT.
///
/// A hardware signer locates the key it must sign with by reading each input's
/// BIP-32 key-origin data (`[fingerprint/derivation]`). An input carrying none is
/// an input the device cannot sign — it refuses rather than guesses. This is the
/// protection D-09 was really about; with Bitcoin Core's descriptors deleted under
/// D-07b, the PSBT itself is where key origin now has to be checked.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PsbtKeyOriginReport {
    input_count: usize,
    inputs_with_key_origin: usize,
    all_inputs_have_key_origin: bool,
}

/// Inspect a base64 PSBT and report how many of its inputs carry BIP-32 key origin.
///
/// An input counts as carrying key origin when either its `bip32_derivation` map
/// (ECDSA / segwit v0) or its `tap_key_origins` map (taproot) is non-empty.
///
/// A PSBT with **zero** inputs reports `all_inputs_have_key_origin: false` rather
/// than vacuous truth — an inputless PSBT cannot be signed at all, and answering
/// "yes, everything a signer needs is present" would be actively misleading.
///
/// This is an *inspection*, never a precondition: callers must degrade to a null
/// report on error, not fail the user's transaction (see `handle_lnd_create_psbt`).
fn psbt_key_origin_report(psbt_base64: &str) -> Result<PsbtKeyOriginReport> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(psbt_base64.trim())
        .context("PSBT is not valid base64")?;

    let psbt = bitcoin::psbt::Psbt::deserialize(&raw).context("PSBT failed to deserialize")?;

    let input_count = psbt.inputs.len();
    let inputs_with_key_origin = psbt
        .inputs
        .iter()
        .filter(|input| !input.bip32_derivation.is_empty() || !input.tap_key_origins.is_empty())
        .count();

    Ok(PsbtKeyOriginReport {
        input_count,
        inputs_with_key_origin,
        all_inputs_have_key_origin: input_count > 0 && inputs_with_key_origin == input_count,
    })
}

/// LND's transaction `amount` is the wallet-wide net amount, not the value
/// paid to a purchase address. Attribute only confirmed output values, once
/// per outpoint. Missing/malformed evidence is unknown, never proof of payment.
fn confirmed_address_sats(body: &serde_json::Value, address: &str) -> Result<u64> {
    use std::collections::{HashMap, HashSet};
    const MAX_SATS: u64 = 21_000_000 * 100_000_000;
    fn integer(value: &serde_json::Value) -> Result<u64> {
        match value {
            serde_json::Value::String(s)
                if !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()) =>
            {
                s.parse().context("Invalid on-chain output integer")
            }
            value => value
                .as_u64()
                .context("Missing or invalid on-chain output integer"),
        }
    }
    anyhow::ensure!(!address.is_empty(), "Missing purchase address");
    let transactions = body
        .get("transactions")
        .and_then(|v| v.as_array())
        .context("Wallet omitted transaction evidence")?;
    let mut seen = HashMap::new();
    let mut total = 0u64;
    for tx in transactions {
        let confirmations = match &tx["num_confirmations"] {
            serde_json::Value::String(s) => {
                s.parse::<i64>().context("Invalid confirmation count")?
            }
            value => value.as_i64().context("Missing confirmation count")?,
        };
        if confirmations < 1 {
            continue;
        }
        let hash = tx["tx_hash"]
            .as_str()
            .context("Missing transaction identifier")?;
        anyhow::ensure!(
            hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit()),
            "Invalid transaction identifier"
        );
        if let Some(previous) = seen.insert(hash.to_ascii_lowercase(), tx) {
            anyhow::ensure!(previous == tx, "Conflicting duplicate transaction evidence");
            continue;
        }
        let outputs = tx["output_details"]
            .as_array()
            .context("Wallet omitted output values")?;
        let mut indices = HashSet::new();
        for output in outputs {
            let index =
                u32::try_from(integer(&output["output_index"])?).context("Invalid output index")?;
            anyhow::ensure!(indices.insert(index), "Duplicate transaction output");
            let amount = integer(&output["amount"])?;
            anyhow::ensure!(amount <= MAX_SATS, "Invalid output amount");
            // A non-address script (e.g. OP_RETURN) has no receiving address.
            if output["address"].as_str() != Some(address) {
                continue;
            }
            total = total
                .checked_add(amount)
                .filter(|sum| *sum <= MAX_SATS)
                .context("Invalid total received amount")?;
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payment_tx(hash: &str, confirmations: i64, outputs: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"tx_hash":hash.repeat(64),"num_confirmations":confirmations,
            "amount":"999999","dest_addresses":["purchase"],"output_details":outputs})
    }

    #[test]
    fn onchain_purchase_counts_only_its_outputs_not_wallet_total() {
        let tx = payment_tx(
            "a",
            1,
            serde_json::json!([
                {"output_index":"0","amount":"1","address":"purchase"},
                {"output_index":"1","amount":"999998","address":"other"}
            ]),
        );
        assert_eq!(
            confirmed_address_sats(&serde_json::json!({"transactions":[tx]}), "purchase").unwrap(),
            1
        );
    }

    #[test]
    fn onchain_purchase_sums_confirmed_partial_outputs_once() {
        let mut first = payment_tx(
            "a",
            1,
            serde_json::json!([
                {"output_index":0,"amount":"300","address":"purchase"},
                {"output_index":"1","amount":46,"address":"purchase"}
            ]),
        );
        first["amount"] = serde_json::json!("-9999"); // net wallet debit is irrelevant
        let second = payment_tx(
            "b",
            2,
            serde_json::json!([
                {"output_index":"2","amount":"200","address":"purchase"}
            ]),
        );
        let unconfirmed = payment_tx(
            "c",
            0,
            serde_json::json!([
                {"output_index":0,"amount":"10000","address":"purchase"}
            ]),
        );
        let conflicted = payment_tx(
            "d",
            -1,
            serde_json::json!([
                {"output_index":0,"amount":"10000","address":"purchase"}
            ]),
        );
        assert_eq!(confirmed_address_sats(&serde_json::json!({"transactions":[first.clone(),first,second,unconfirmed,conflicted]}), "purchase").unwrap(), 546);
    }

    #[test]
    fn onchain_purchase_rejects_missing_values_and_ambiguous_outpoints() {
        let good = payment_tx(
            "a",
            1,
            serde_json::json!([
                {"output_index":"0","amount":"546","address":"purchase"}
            ]),
        );
        let mut no_values = good.clone();
        no_values.as_object_mut().unwrap().remove("output_details");
        let mut duplicate = good.clone();
        duplicate["output_details"] = serde_json::json!([
            {"output_index":0,"amount":300,"address":"purchase"},
            {"output_index":0,"amount":300,"address":"purchase"}
        ]);
        let mut conflicting = good.clone();
        conflicting["output_details"][0]["amount"] = serde_json::json!(1000);
        for body in [
            serde_json::json!({}),
            serde_json::json!({"transactions":[no_values]}),
            serde_json::json!({"transactions":[duplicate]}),
            serde_json::json!({"transactions":[good.clone(),conflicting]}),
        ] {
            assert!(confirmed_address_sats(&body, "purchase").is_err());
        }
        for amount in [
            serde_json::json!(-1),
            serde_json::json!("0.00000546"),
            serde_json::json!(546.5),
            serde_json::json!(null),
            serde_json::json!("18446744073709551616"),
            serde_json::json!("2100000000000001"),
        ] {
            let mut invalid = good.clone();
            invalid["output_details"][0]["amount"] = amount;
            assert!(confirmed_address_sats(
                &serde_json::json!({"transactions":[invalid]}),
                "purchase"
            )
            .is_err());
        }
    }

    #[test]
    fn onchain_purchase_has_no_rounding_or_accumulation_overflow() {
        let max = "2100000000000000";
        let first = payment_tx(
            "a",
            1,
            serde_json::json!([{"output_index":0,"amount":max,"address":"purchase"}]),
        );
        assert_eq!(
            confirmed_address_sats(
                &serde_json::json!({"transactions":[first.clone()]}),
                "purchase"
            )
            .unwrap(),
            2_100_000_000_000_000
        );
        let second = payment_tx(
            "b",
            1,
            serde_json::json!([{"output_index":0,"amount":"1","address":"purchase"}]),
        );
        assert!(confirmed_address_sats(
            &serde_json::json!({"transactions":[first,second]}),
            "purchase"
        )
        .is_err());
    }

    /// Build a minimal, genuinely unsigned one-input PSBT with no key origin on
    /// any input. Built programmatically rather than pasted as opaque base64 so
    /// the fixture states what it is.
    fn unsigned_one_input_psbt() -> bitcoin::psbt::Psbt {
        use bitcoin::{
            absolute::LockTime, transaction::Version, Amount, OutPoint, ScriptBuf, Sequence,
            Transaction, TxIn, TxOut, Witness,
        };

        let tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::new(),
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(10_000),
                script_pubkey: ScriptBuf::new(),
            }],
        };

        bitcoin::psbt::Psbt::from_unsigned_tx(tx).expect("unsigned tx is a valid PSBT")
    }

    fn psbt_to_base64(psbt: &bitcoin::psbt::Psbt) -> String {
        base64::engine::general_purpose::STANDARD.encode(psbt.serialize())
    }

    #[test]
    fn psbt_without_derivations_reports_no_key_origin() {
        let psbt = unsigned_one_input_psbt();
        let report = psbt_key_origin_report(&psbt_to_base64(&psbt)).expect("valid PSBT");

        assert_eq!(report.input_count, 1);
        assert_eq!(report.inputs_with_key_origin, 0);
        assert!(
            !report.all_inputs_have_key_origin,
            "an input with no bip32_derivation is one a hardware signer cannot sign"
        );
    }

    #[test]
    fn psbt_with_derivations_reports_key_origin() {
        use bitcoin::bip32::{DerivationPath, Fingerprint};
        use bitcoin::secp256k1::{PublicKey, Secp256k1, SecretKey};

        let secp = Secp256k1::new();
        let sk = SecretKey::from_slice(&[0x11u8; 32]).expect("valid secret key");
        let pk = PublicKey::from_secret_key(&secp, &sk);
        let path: DerivationPath = "m/84'/0'/0'/0/0".parse().expect("valid BIP-84 path");
        let fingerprint = Fingerprint::from([0xde, 0xad, 0xbe, 0xef]);

        let mut psbt = unsigned_one_input_psbt();
        psbt.inputs[0]
            .bip32_derivation
            .insert(pk, (fingerprint, path));

        let report = psbt_key_origin_report(&psbt_to_base64(&psbt)).expect("valid PSBT");

        assert_eq!(report.input_count, 1);
        assert_eq!(report.inputs_with_key_origin, 1);
        assert!(report.all_inputs_have_key_origin);
    }

    #[test]
    fn malformed_psbt_is_an_error_not_a_panic() {
        // Not base64 at all.
        assert!(psbt_key_origin_report("not a psbt!!!").is_err());

        // Valid base64, but truncated PSBT bytes.
        let psbt = unsigned_one_input_psbt();
        let serialized = psbt.serialize();
        let truncated =
            base64::engine::general_purpose::STANDARD.encode(&serialized[..serialized.len() / 2]);
        assert!(psbt_key_origin_report(&truncated).is_err());

        // Empty input.
        assert!(psbt_key_origin_report("").is_err());
    }

    #[test]
    fn invoice_request_body_always_sets_private_true() {
        let body = build_invoice_request_body(1_234, "test memo");
        assert_eq!(body["private"], serde_json::json!(true));
        assert_eq!(body["value"], serde_json::json!("1234"));
        assert_eq!(body["memo"], serde_json::json!("test memo"));
    }

    #[test]
    fn lnd_error_message_prefers_message_field() {
        let body = serde_json::json!({
            "error": "grpc proxy error",
            "message": "wallet locked",
        });

        assert_eq!(lnd_error_message(&body), "wallet locked");
    }

    #[test]
    fn lnd_error_message_falls_back_to_unknown() {
        assert_eq!(
            lnd_error_message(&serde_json::json!({})),
            "unknown LND error"
        );
    }

    #[test]
    fn classify_locked_wallet() {
        assert_eq!(
            classify_lnd_address_error("wallet locked, please unlock"),
            RECEIVE_WALLET_LOCKED
        );
    }

    #[test]
    fn classify_uninitialized_wallet() {
        assert_eq!(
            classify_lnd_address_error("wallet not found, create a wallet first"),
            RECEIVE_WALLET_UNINITIALIZED
        );
    }

    #[test]
    fn classify_syncing() {
        assert_eq!(
            classify_lnd_address_error("server is still in the process of starting"),
            RECEIVE_SYNCING
        );
    }

    #[test]
    fn classify_unknown_is_generic_error() {
        assert_eq!(
            classify_lnd_address_error("some other failure"),
            RECEIVE_LND_ERROR
        );
    }

    #[test]
    fn receive_error_starts_with_sanitizer_safe_prefix_and_embeds_code() {
        // Must start with "Bitcoin address" (survives the RPC error sanitizer)
        // and carry the [CODE] token the frontend parses.
        let err = receive_error(RECEIVE_REST_UNREACHABLE, "unreachable");
        let s = format!("{err}");
        assert!(s.starts_with("Bitcoin address"), "got: {s}");
        assert!(s.contains("[LND_REST_UNREACHABLE]"), "got: {s}");
    }
}

// LND REST uses decimal strings for int64 fields. Match the complete seller
// memo, not a substring supplied by a buyer or an arbitrary settled invoice.
fn json_u64(value: &serde_json::Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}
fn content_invoice_fully_settled(body: &serde_json::Value, price: u64) -> bool {
    let settled = match body.get("state").and_then(|v| v.as_str()) {
        Some(state) => state == "SETTLED",
        None => body.get("settled").and_then(|v| v.as_bool()) == Some(true),
    };
    settled
        && price > 0
        && body
            .get("amt_paid_sat")
            .and_then(json_u64)
            .is_some_and(|paid| paid >= price)
}
fn content_invoice_amount(body: &serde_json::Value, content_id: &str) -> Option<u64> {
    if body.get("memo")?.as_str()? != format!("Archipelago peer file {content_id}") {
        return None;
    }
    body.get("value").and_then(json_u64).filter(|v| *v > 0)
}

#[cfg(test)]
mod peer_file_invoice_tests {
    use super::*;
    #[test]
    fn settlement_requires_terminal_state_and_full_amount() {
        for state in ["OPEN", "ACCEPTED", "CANCELED", "unknown"] {
            assert!(!content_invoice_fully_settled(
                &serde_json::json!({"state":state,"settled":true,"amt_paid_sat":"100"}),
                7
            ));
        }
        for amount in [
            serde_json::json!(6),
            serde_json::json!("-1"),
            serde_json::json!(null),
            serde_json::json!("bad"),
        ] {
            assert!(!content_invoice_fully_settled(
                &serde_json::json!({"state":"SETTLED","amt_paid_sat":amount}),
                7
            ));
        }
        for amount in [serde_json::json!(7), serde_json::json!("8")] {
            assert!(content_invoice_fully_settled(
                &serde_json::json!({"state":"SETTLED","amt_paid_sat":amount}),
                7
            ));
        }
        assert!(content_invoice_fully_settled(
            &serde_json::json!({"settled":true,"amt_paid_sat":"7"}),
            7
        ));
        assert!(!content_invoice_fully_settled(
            &serde_json::json!({"state":"SETTLED","amt_paid_sat":"7"}),
            0
        ));
    }
    #[test]
    fn legacy_recovery_requires_exact_file_memo_and_positive_amount() {
        let invoice = serde_json::json!({"memo":"Archipelago peer file file-1", "value":"7"});
        assert_eq!(content_invoice_amount(&invoice, "file-1"), Some(7));
        assert_eq!(content_invoice_amount(&invoice, "file-2"), None);
        for value in [
            serde_json::json!("-1"),
            serde_json::json!(0),
            serde_json::json!("bad"),
        ] {
            let mut invalid = invoice.clone();
            invalid["value"] = value;
            assert_eq!(content_invoice_amount(&invalid, "file-1"), None);
        }
    }
}

fn content_invoice_lifecycle_body(body: &serde_json::Value, price: u64) -> serde_json::Value {
    let settled = content_invoice_fully_settled(body, price);
    let cancelled = !settled
        && body["state"] == "CANCELED"
        && body.get("settled").and_then(|value| value.as_bool()) != Some(true)
        && body.get("amt_paid_sat").and_then(json_u64) == Some(0)
        && body
            .get("amt_paid_msat")
            .is_none_or(|value| json_u64(value) == Some(0));
    let state = if settled {
        "settled"
    } else if cancelled {
        "canceled"
    } else {
        match body.get("state").and_then(|value| value.as_str()) {
            Some("OPEN") => "open",
            Some("ACCEPTED") => "accepted",
            _ => "unknown",
        }
    };
    let expires_at = body
        .get("creation_date")
        .and_then(json_u64)
        .zip(body.get("expiry").and_then(json_u64))
        .and_then(|(created, expiry)| created.checked_add(expiry));
    // Expiry is informational. Only LND's terminal canceled state releases the
    // cross-method block; wall-clock passage or lookup failure never does.
    serde_json::json!({"paid":settled,"state":state,"can_switch_method":cancelled,
        "expires_at":expires_at,"cancel_supported":false})
}

#[cfg(test)]
mod invoice_lifecycle_tests {
    use super::*;
    #[test]
    fn only_authoritative_zero_paid_cancel_unlocks_and_settlement_survives_expiry() {
        for state in ["OPEN", "ACCEPTED", "UNKNOWN", "CANCELED"] {
            for paid in [0u64, 1] {
                let result = content_invoice_lifecycle_body(
                    &serde_json::json!({
                    "state":state,"settled":false,"amt_paid_sat":paid.to_string(),
                    "creation_date":"1","expiry":"1"}),
                    8,
                );
                assert_eq!(
                    result["can_switch_method"],
                    state == "CANCELED" && paid == 0
                );
                assert_eq!(result["paid"], false);
            }
        }
        assert_eq!(
            content_invoice_lifecycle_body(&serde_json::json!({"state":"CANCELED"}), 8)
                ["can_switch_method"],
            false
        );
        assert_eq!(
            content_invoice_lifecycle_body(
                &serde_json::json!({"state":"CANCELED", "amt_paid_sat":"0", "amt_paid_msat":"1"}),
                8
            )["can_switch_method"],
            false
        );
        let paid = content_invoice_lifecycle_body(
            &serde_json::json!({
            "state":"SETTLED","settled":true,"amt_paid_sat":"8","value":"8",
            "creation_date":"1","expiry":"1"}),
            8,
        );
        assert_eq!(paid["paid"], true);
        assert_eq!(paid["can_switch_method"], false);
    }
}
