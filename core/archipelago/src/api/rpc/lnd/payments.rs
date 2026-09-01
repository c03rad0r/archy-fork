use crate::api::rpc::RpcHandler;
use anyhow::{Context, Result};
use tracing::info;

use super::LND_REST_BASE_URL;

fn router_error_message(body: &serde_json::Value) -> Option<&str> {
    body.get("error")
        .and_then(|e| e.get("message"))
        .and_then(|v| v.as_str())
        .or_else(|| body.get("message").and_then(|v| v.as_str()))
}

fn payment_error(message: &str) -> anyhow::Error {
    if message.to_ascii_lowercase().contains("invoice expired") {
        anyhow::anyhow!(
            "Payment failed: this invoice has expired ({}). Ask the recipient for a fresh invoice and try again.",
            message.trim_start_matches("invoice expired. ")
        )
    } else {
        anyhow::anyhow!("Payment failed: {message}")
    }
}

fn payment_failure_reason(reason: &str) -> &'static str {
    match reason {
        "FAILURE_REASON_NO_ROUTE" => "No route to the recipient",
        "FAILURE_REASON_INSUFFICIENT_BALANCE" => "Insufficient channel balance",
        "FAILURE_REASON_TIMEOUT" => "Payment timed out in the network",
        "FAILURE_REASON_INCORRECT_PAYMENT_DETAILS" => {
            "Recipient rejected the payment (wrong details or expired invoice)"
        }
        _ => "Payment failed",
    }
}

fn json_i64(value: &serde_json::Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|v| {
        v.as_str()
            .and_then(|s| s.parse().ok())
            .or_else(|| v.as_i64())
    })
}

/// Fee budget for a send, matching lncli's own default: the payment amount
/// (100%). Zero-amount invoices take the payer-supplied amount; fixed invoices
/// take the invoice's own amount. Falls back to a nominal 1,000 sats only when
/// both are somehow absent — the limit must never be left at LND's zero
/// default, which rejects every fee-carrying route as "no route".
fn fee_limit_sats(amount_sats: Option<u64>, decoded_amt: i64) -> i64 {
    if let Some(amt) = amount_sats {
        return amt as i64;
    }
    if decoded_amt > 0 {
        return decoded_amt;
    }
    1_000
}

impl RpcHandler {
    /// Pay a Lightning invoice.
    pub(in crate::api::rpc) async fn handle_lnd_payinvoice(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let payment_request = params
            .get("payment_request")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'payment_request' parameter"))?;

        // Basic validation: Lightning invoices start with lnbc/lntb/lnbcrt
        if payment_request.len() < 10 || payment_request.len() > 2048 {
            return Err(anyhow::anyhow!("Invalid payment request length"));
        }
        let lower = payment_request.to_lowercase();
        if !lower.starts_with("lnbc") && !lower.starts_with("lntb") && !lower.starts_with("lnbcrt")
        {
            return Err(anyhow::anyhow!(
                "Invalid payment request: must be a Lightning invoice (lnbc...)"
            ));
        }

        // Zero-amount invoices need the amount supplied by the payer; LND's
        // REST API takes it as an `amt` string alongside the payment request.
        let amount_sats = params.get("amount_sats").and_then(|v| v.as_u64());

        info!("Paying Lightning invoice");

        let (client, macaroon_hex) = self.lnd_client().await?;

        // Decode the invoice up front (fast, local) so we know its payment
        // hash BEFORE handing it to LND. If the payment outlives our wait
        // below, the hash is what lets the UI keep tracking it instead of
        // declaring a false failure. Best-effort: a decode hiccup must not
        // block the payment itself.
        let (decoded_hash, decoded_amt) = match client
            .get(format!("{LND_REST_BASE_URL}/v1/payreq/{payment_request}"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
        {
            Ok(r) => match r.json::<serde_json::Value>().await {
                Ok(d) => (
                    d.get("payment_hash")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    d.get("num_satoshis")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<i64>().ok())
                        .unwrap_or(0),
                ),
                Err(_) => (String::new(), 0),
            },
            Err(_) => (String::new(), 0),
        };

        let mut pay_body = serde_json::json!({
            "payment_request": payment_request,
            // Suppress intermediate stream records: one terminal Payment is
            // enough, and it makes grpc-gateway's response a single JSON value.
            "no_inflight_updates": true,
            "timeout_seconds": 120,
            // Router.SendPaymentV2 treats an ABSENT fee limit as ZERO — every
            // real route carries a routing fee, so the pathfinder rejects
            // them all and the wallet gets "No route to the recipient" on
            // every send (fleet-wide, 2026-09-01: the v1.8.9 switch to the v2
            // route shipped without this, and a manual lncli test that set
            // --fee_limit masked it). lncli's own default is the payment
            // amount (100%), which is what we send here.
            "fee_limit_sat": fee_limit_sats(amount_sats, decoded_amt),
        });
        if let Some(amt) = amount_sats {
            pay_body["amt"] = serde_json::json!(amt.to_string());
        }

        // LND 0.21 removed the deprecated Lightning.SendPaymentSync REST route
        // (`/v1/channels/transactions`). Router.SendPaymentV2 is its supported
        // replacement. The old route now returns literal 404 "Not Found" on
        // every payment — the fleet failure seen immediately after the 0.21.2
        // update. Keep the short browser-facing wait: after LND accepts a slow
        // payment we return pending and the UI follows it through
        // lnd.paymentstatus instead of declaring a transport timeout a failure.
        let pay_client = reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(8))
            .danger_accept_invalid_certs(true)
            .build()
            .context("Failed to create HTTP client")?;

        let resp = match pay_client
            .post(format!("{LND_REST_BASE_URL}/v2/router/send"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .json(&pay_body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) if e.is_connect() => {
                // Never reached LND — nothing was sent; this IS a hard error.
                return Err(anyhow::anyhow!("Could not reach LND to pay: {e}"));
            }
            Err(_) => {
                // Timed out (or lost the connection) AFTER the payment was
                // handed to LND — it may well still succeed. Report pending
                // with the hash so the caller can poll lnd.paymentstatus.
                info!("payinvoice wait elapsed; payment still in flight");
                return Ok(serde_json::json!({
                    "status": "pending",
                    "payment_hash": decoded_hash,
                    "amount_sats": decoded_amt,
                }));
            }
        };

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse Router.SendPaymentV2 response")?;

        // grpc-gateway wraps server-streaming records as {"result": ...} and
        // transport/RPC failures as {"error": {"message": ...}}. Do not look
        // only for the old endpoint's top-level `message`: that turns useful
        // LND errors into "Unknown error".
        if !status.is_success() {
            let msg = router_error_message(&body).unwrap_or("Unknown error");
            return Err(payment_error(msg));
        }
        let payment = body.get("result").unwrap_or(&body);
        match payment.get("status").and_then(|v| v.as_str()).unwrap_or("") {
            "SUCCEEDED" => {}
            "FAILED" => {
                let reason = payment
                    .get("failure_reason")
                    .and_then(|v| v.as_str())
                    .map(payment_failure_reason)
                    .unwrap_or("Payment failed");
                return Err(anyhow::anyhow!("Payment failed: {reason}"));
            }
            _ => {
                return Ok(serde_json::json!({
                    "status": "pending",
                    "payment_hash": decoded_hash,
                    "amount_sats": decoded_amt,
                }));
            }
        }

        let amount_sat = json_i64(payment, "value_sat").unwrap_or(decoded_amt);
        Ok(serde_json::json!({
            "status": "succeeded",
            // The decode endpoint returns the canonical hex hash used by our
            // polling/list APIs. Router's bytes field is base64 in REST JSON.
            "payment_hash": decoded_hash,
            "amount_sats": amount_sat,
        }))
    }

    /// Status of an outgoing Lightning payment by hex payment hash. Lets the
    /// UI resolve a payinvoice that outlived its synchronous wait (`status:
    /// "pending"`) to a real terminal state instead of guessing.
    pub(in crate::api::rpc) async fn handle_lnd_paymentstatus(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let payment_hash = params
            .get("payment_hash")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'payment_hash' parameter"))?;
        if payment_hash.len() != 64 || !payment_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(anyhow::anyhow!("Invalid payment hash"));
        }

        let (client, macaroon_hex) = self.lnd_client().await?;
        let resp = client
            .get(format!(
                "{LND_REST_BASE_URL}/v1/payments?include_incomplete=true&max_payments=100&reversed=true"
            ))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .context("LND REST connection failed")?;
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse payments response")?;

        let hash_lower = payment_hash.to_lowercase();
        let found = body
            .get("payments")
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                arr.iter().find(|p| {
                    p.get("payment_hash").and_then(|v| v.as_str()) == Some(hash_lower.as_str())
                })
            });

        let Some(p) = found else {
            // Not in the latest window — either very old or LND never saw it.
            return Ok(serde_json::json!({ "status": "unknown" }));
        };

        let lnd_status = p.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let status = match lnd_status {
            "SUCCEEDED" => "succeeded",
            "FAILED" => "failed",
            _ => "in_flight",
        };
        let failure_reason = match p
            .get("failure_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("")
        {
            "FAILURE_REASON_NO_ROUTE" => "No route to the recipient",
            "FAILURE_REASON_INSUFFICIENT_BALANCE" => "Insufficient channel balance",
            "FAILURE_REASON_TIMEOUT" => "Payment timed out in the network",
            "FAILURE_REASON_INCORRECT_PAYMENT_DETAILS" => {
                "Recipient rejected the payment (wrong details or expired invoice)"
            }
            "FAILURE_REASON_ERROR" => "Payment failed",
            _ => "",
        };

        fn amt(p: &serde_json::Value, key: &str) -> i64 {
            p.get(key)
                .and_then(|f| f.as_str())
                .and_then(|s| s.parse().ok())
                .or_else(|| p.get(key).and_then(|f| f.as_i64()))
                .unwrap_or(0)
        }

        Ok(serde_json::json!({
            "status": status,
            "failure_reason": failure_reason,
            "amount_sats": amt(p, "value_sat"),
            "fee_sats": amt(p, "fee_sat"),
        }))
    }

    /// List on-chain transactions from LND.
    /// Returns all transactions, with incoming (amount > 0) flagged.
    pub(in crate::api::rpc) async fn handle_lnd_gettransactions(
        &self,
    ) -> Result<serde_json::Value> {
        let (client, macaroon_hex) = self.lnd_client().await?;

        let resp = client
            .get(format!("{LND_REST_BASE_URL}/v1/transactions"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .context("LND REST connection failed")?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse transactions response")?;

        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("Failed to list transactions: {}", msg));
        }

        let empty_vec = vec![];
        let raw_txs = body
            .get("transactions")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty_vec);

        let mut transactions: Vec<serde_json::Value> = Vec::new();
        for tx in raw_txs {
            let amount: i64 = tx
                .get("amount")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .or_else(|| tx.get("amount").and_then(|v| v.as_i64()))
                .unwrap_or(0);

            let num_confirmations: i64 = tx
                .get("num_confirmations")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            let tx_hash = tx
                .get("tx_hash")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let time_stamp: i64 = tx
                .get("time_stamp")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .or_else(|| tx.get("time_stamp").and_then(|v| v.as_i64()))
                .unwrap_or(0);

            let total_fees: i64 = tx
                .get("total_fees")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .or_else(|| tx.get("total_fees").and_then(|v| v.as_i64()))
                .unwrap_or(0);

            let dest_addresses: Vec<String> = tx
                .get("dest_addresses")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|a| a.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            let label = tx
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let block_height: i64 = tx.get("block_height").and_then(|v| v.as_i64()).unwrap_or(0);

            let direction = if amount > 0 { "incoming" } else { "outgoing" };

            transactions.push(serde_json::json!({
                "tx_hash": tx_hash,
                "amount_sats": amount.abs(),
                "direction": direction,
                "num_confirmations": num_confirmations,
                "time_stamp": time_stamp,
                "total_fees": total_fees,
                "dest_addresses": dest_addresses,
                "label": label,
                "block_height": block_height,
            }));
        }

        // Sort by timestamp descending (most recent first)
        transactions.sort_by(|a, b| {
            let ta = a.get("time_stamp").and_then(|v| v.as_i64()).unwrap_or(0);
            let tb = b.get("time_stamp").and_then(|v| v.as_i64()).unwrap_or(0);
            tb.cmp(&ta)
        });

        let incoming_pending: usize = transactions
            .iter()
            .filter(|t| {
                t.get("direction").and_then(|v| v.as_str()) == Some("incoming")
                    && t.get("num_confirmations").and_then(|v| v.as_i64()) == Some(0)
            })
            .count();

        Ok(serde_json::json!({
            "transactions": transactions,
            "incoming_pending_count": incoming_pending,
        }))
    }

    /// Unified Lightning history: settled invoices (incoming) + succeeded
    /// payments (outgoing), normalized to the wallet-transaction shape the
    /// UI already renders. On-chain history stays in lnd.gettransactions.
    pub(in crate::api::rpc) async fn handle_lnd_lightning_history(
        &self,
    ) -> Result<serde_json::Value> {
        use base64::Engine;

        fn field_i64(v: &serde_json::Value, key: &str) -> i64 {
            v.get(key)
                .and_then(|f| f.as_str())
                .and_then(|s| s.parse().ok())
                .or_else(|| v.get(key).and_then(|f| f.as_i64()))
                .unwrap_or(0)
        }

        let (client, macaroon_hex) = self.lnd_client().await?;
        let mut transactions: Vec<serde_json::Value> = Vec::new();

        // Outgoing: succeeded payments only (include_incomplete=false)
        let payments_resp = client
            .get(format!(
                "{LND_REST_BASE_URL}/v1/payments?include_incomplete=false&max_payments=100&reversed=true"
            ))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .context("LND REST connection failed")?;
        if payments_resp.status().is_success() {
            let body: serde_json::Value = payments_resp
                .json()
                .await
                .context("Failed to parse payments response")?;
            for p in body
                .get("payments")
                .and_then(|v| v.as_array())
                .unwrap_or(&vec![])
            {
                let amount = field_i64(p, "value_sat");
                if amount == 0 {
                    continue;
                }
                transactions.push(serde_json::json!({
                    "tx_hash": p.get("payment_hash").and_then(|v| v.as_str()).unwrap_or(""),
                    "amount_sats": amount,
                    "direction": "outgoing",
                    "num_confirmations": 1,
                    "time_stamp": field_i64(p, "creation_date"),
                    "total_fees": field_i64(p, "fee_sat"),
                    "dest_addresses": [],
                    "label": "",
                    "block_height": 0,
                    "kind": "lightning",
                }));
            }
        }

        // Incoming: settled invoices only
        let invoices_resp = client
            .get(format!(
                "{LND_REST_BASE_URL}/v1/invoices?num_max_invoices=100&reversed=true"
            ))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .context("LND REST connection failed")?;
        if invoices_resp.status().is_success() {
            let body: serde_json::Value = invoices_resp
                .json()
                .await
                .context("Failed to parse invoices response")?;
            for inv in body
                .get("invoices")
                .and_then(|v| v.as_array())
                .unwrap_or(&vec![])
            {
                let settled = inv.get("state").and_then(|v| v.as_str()) == Some("SETTLED")
                    || inv.get("settled").and_then(|v| v.as_bool()) == Some(true);
                if !settled {
                    continue;
                }
                // r_hash arrives base64 from REST; the UI shows hex
                let r_hash_hex = inv
                    .get("r_hash")
                    .and_then(|v| v.as_str())
                    .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok())
                    .map(hex::encode)
                    .unwrap_or_default();
                transactions.push(serde_json::json!({
                    "tx_hash": r_hash_hex,
                    "amount_sats": field_i64(inv, "amt_paid_sat"),
                    "direction": "incoming",
                    "num_confirmations": 1,
                    "time_stamp": field_i64(inv, "settle_date"),
                    "total_fees": 0,
                    "dest_addresses": [],
                    "label": inv.get("memo").and_then(|v| v.as_str()).unwrap_or(""),
                    "block_height": 0,
                    "kind": "lightning",
                }));
            }
        }

        transactions.sort_by(|a, b| {
            let ta = a.get("time_stamp").and_then(|v| v.as_i64()).unwrap_or(0);
            let tb = b.get("time_stamp").and_then(|v| v.as_i64()).unwrap_or(0);
            tb.cmp(&ta)
        });

        Ok(serde_json::json!({ "transactions": transactions }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_grpc_gateway_router_success() {
        let body = serde_json::json!({
            "result": { "status": "SUCCEEDED", "value_sat": "1000" }
        });
        let payment = body.get("result").unwrap_or(&body);
        assert_eq!(
            payment.get("status").and_then(|v| v.as_str()),
            Some("SUCCEEDED")
        );
        assert_eq!(json_i64(payment, "value_sat"), Some(1000));
    }

    #[test]
    fn reads_nested_router_error() {
        let body = serde_json::json!({
            "error": { "code": 2, "message": "invoice expired. valid until yesterday" }
        });
        let msg = router_error_message(&body).unwrap();
        assert!(payment_error(msg).to_string().contains("fresh invoice"));
    }

    #[test]
    fn router_failure_reasons_are_actionable() {
        assert_eq!(
            payment_failure_reason("FAILURE_REASON_NO_ROUTE"),
            "No route to the recipient"
        );
        assert_eq!(
            payment_failure_reason("FAILURE_REASON_INSUFFICIENT_BALANCE"),
            "Insufficient channel balance"
        );
    }

    #[test]
    fn fee_limit_never_falls_back_to_zero() {
        // SendPaymentV2 defaults an ABSENT fee limit to zero — which rejects
        // every fee-carrying route as "no route". The budget must always be
        // positive: the payer-supplied amount for zero-amount invoices, the
        // invoice's own amount otherwise.
        assert_eq!(fee_limit_sats(Some(20_000), 0), 20_000);
        assert_eq!(fee_limit_sats(None, 20_000), 20_000);
        assert_eq!(fee_limit_sats(None, 0), 1_000);
    }
}
