//! WalletKit BumpFee is CPFP for new wallet outputs, RBF only for sweeper inputs.
//! Never feed an ordinary payment input to it and call that a replacement.
use super::LND_REST_BASE_URL;
use crate::api::rpc::RpcHandler;
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, path::Path, sync::LazyLock};
use tokio::{io::AsyncWriteExt, sync::Mutex};

static QUOTES: LazyLock<Mutex<HashMap<String, Quote>>> = LazyLock::new(Default::default);
// Serialize check/register/persist across dashboard clients. The create_new receipt
// additionally survives process restarts and prevents retries of ambiguous results.
static SUBMIT: Mutex<()> = Mutex::const_new(());
const QUOTE_SECONDS: u64 = 60;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Plan {
    txid: String,
    method: String,
    input_txid: String,
    input_index: u32,
    parent_txid: String,
    recipient_sats: u64,
    rate_sat_vb: u64,
    current_fee_sats: u64,
    additional_fee_sats: u64,
    total_fee_sats: u64,
    budget_sats: u64,
    input_sats: u64,
    parent_vsize: u64,
    sweep_vsize_bound: u64,
    tip: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Quote {
    quote_id: String,
    expires_at: u64,
    custom_rate: Option<u64>,
    #[serde(flatten)]
    plan: Plan,
}
#[derive(Serialize, Deserialize)]
struct Operation {
    quote: Quote,
    status: String,
    message: String,
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn number(v: &Value) -> Result<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .context("Missing or invalid wallet amount")
}
fn txid_param(p: &Value) -> Result<String> {
    let s = p["txid"].as_str().context("Missing transaction ID")?;
    ensure!(
        s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()),
        "Invalid transaction ID"
    );
    Ok(s.to_ascii_lowercase())
}
fn btc_sats(v: &Value) -> Result<u64> {
    let n = v.as_f64().context("Missing Bitcoin fee")? * 100_000_000.0;
    ensure!(
        n.is_finite() && n >= 0.0 && n <= 2_100_000_000_000_000.0,
        "Invalid Bitcoin fee"
    );
    Ok(n.round() as u64)
}
fn outpoint_matches(v: &Value, txid: &str, index: u32) -> bool {
    v["txid_str"].as_str() == Some(txid) && v["output_index"].as_u64() == Some(index as u64)
}
fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    v[key]
        .as_array()
        .with_context(|| format!("Missing wallet field: {key}"))
}
fn sweep_size(output: &Value) -> Result<u64> {
    // One native input, one wallet taproot output, including signature rounding.
    match output["output_type"].as_str() {
        Some("SCRIPT_TYPE_WITNESS_V1_TAPROOT") => Ok(112),
        Some("SCRIPT_TYPE_WITNESS_V0_PUBKEY_HASH") => Ok(123),
        _ => bail!("This output type is not supported for fee bumping yet"),
    }
}
fn fee_budget(
    rate: u64,
    parent_size: u64,
    parent_fee: u64,
    size: u64,
    old_fee: u64,
    relay: u64,
    input: u64,
) -> Result<u64> {
    ensure!(
        (1..=5000).contains(&rate),
        "Fee rate must be a whole number from 1 to 5000 sat/vB"
    );
    ensure!(
        parent_size <= 100_000 && size <= 100_000 && relay <= 5000,
        "Unsupported package size or relay fee"
    );
    let required = rate * (parent_size + size);
    let mut budget = required.saturating_sub(parent_fee).max(relay * size);
    if old_fee > 0 {
        budget = budget.max(old_fee + relay * size + 1);
    }
    ensure!(budget > old_fee, "Choose a higher fee rate");
    // Conservative dust buffer; never attach unrelated wallet inputs to fund fees.
    ensure!(
        budget.checked_add(1000).is_some_and(|v| v <= input),
        "Not enough wallet change for this fee; choose a lower rate"
    );
    Ok(budget)
}

async fn lnd(
    client: &reqwest::Client,
    macaroon: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value> {
    let url = format!("{LND_REST_BASE_URL}{path}");
    let req = match body {
        Some(v) => client.post(url).json(&v),
        None => client.get(url),
    };
    let response = req
        .header("Grpc-Metadata-macaroon", macaroon)
        .send()
        .await?;
    let status = response.status();
    let value: Value = response.json().await.context("Invalid LND response")?;
    ensure!(
        status.is_success() && value.get("code").is_none(),
        "{}",
        value["message"].as_str().unwrap_or("LND request failed")
    );
    Ok(value)
}

fn validate_quote(quote: &Quote, fresh: &Plan, timestamp: u64) -> Result<()> {
    ensure!(
        quote.expires_at > timestamp && quote.plan == *fresh,
        "Transaction or fees changed; review a fresh quote"
    );
    Ok(())
}
fn bump_body(plan: &Plan) -> Value {
    json!({"outpoint":{"txid_str":plan.input_txid,"output_index":plan.input_index},
        "sat_per_vbyte":plan.rate_sat_vb.to_string(), "budget":plan.budget_sats.to_string(),
        "deadline_delta":1, "immediate":true})
}

async fn reserve(path: &Path, op: &Operation) -> Result<()> {
    let parent = path.parent().context("Invalid operation path")?;
    tokio::fs::create_dir_all(parent).await?;
    let mut f = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .await
        .context("A bump already exists for this transaction; check its status")?;
    f.write_all(&serde_json::to_vec(op)?).await?;
    f.sync_all().await?;
    // Sync directory entry too: a crash must not make a submitted operation vanish.
    tokio::fs::File::open(parent).await?.sync_all().await?;
    Ok(())
}

// Only a recorded Archy CPFP with one owned input and no external outputs may
// be folded into a payment. Labels and a fee-sized delta alone are not evidence.
fn fee_child_matches(tx: &Value, plan: &Plan) -> bool {
    let input = format!("{}:{}", plan.input_txid, plan.input_index);
    let amount = tx["amount"]
        .as_i64()
        .or_else(|| tx["amount"].as_str()?.parse().ok());
    let fee = number(&tx["total_fees"])
        .ok()
        .and_then(|n| i64::try_from(n).ok());
    tx["tx_hash"]
        .as_str()
        .is_some_and(|id| id.len() == 64 && id.bytes().all(|c| c.is_ascii_hexdigit()))
        && amount
            .zip(fee)
            .is_some_and(|(amount, fee)| fee > 0 && amount == -fee)
        && tx["previous_outpoints"].as_array().is_some_and(|inputs| {
            inputs.len() == 1
                && inputs[0]["outpoint"] == input
                && inputs[0]["is_our_output"] == true
        })
        && tx["output_details"].as_array().is_some_and(|outputs| {
            !outputs.is_empty() && outputs.iter().all(|o| o["is_our_address"] == true)
        })
}

impl RpcHandler {
    pub(super) async fn group_fee_bump_history(
        &self,
        raw: &[Value],
        normalized: &mut Vec<Value>,
        client: &reqwest::Client,
    ) {
        let mut hidden = std::collections::HashSet::new();
        for parent in normalized.iter_mut() {
            if parent["direction"] != "outgoing" {
                continue;
            }
            let Some(id) = parent["tx_hash"].as_str().map(str::to_owned) else {
                continue;
            };
            if id.len() != 64 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }
            let path = self
                .config
                .data_dir
                .join("wallet/fee-bumps")
                .join(format!("{id}.json"));
            let Ok(bytes) = tokio::fs::read(path).await else {
                continue;
            };
            let Ok(op) = serde_json::from_slice::<Operation>(&bytes) else {
                continue;
            };
            let plan = &op.quote.plan;
            if plan.method != "cpfp"
                || plan.txid != id
                || plan.parent_txid != id
                || plan.input_txid != id
            {
                continue;
            }
            let candidates: Vec<_> = raw
                .iter()
                .filter(|tx| fee_child_matches(tx, plan))
                .collect();
            let mut active = Vec::new();
            for child in &candidates {
                let child_id = child["tx_hash"].as_str().unwrap();
                if child["num_confirmations"].as_i64().unwrap_or(0) > 0
                    || self
                        .bitcoin_rpc_call::<Value>(client, "getmempoolentry", &[json!(child_id)])
                        .await
                        .is_ok()
                {
                    active.push(*child);
                }
            }
            // Ambiguous or unavailable chain state must not hide wallet history.
            if active.len() != 1 {
                continue;
            }
            let current = active[0];
            parent["bump_fee_sats"] = json!(number(&current["total_fees"]).unwrap());
            parent["fee_bump_txid"] = current["tx_hash"].clone();
            parent["fee_bump_confirmations"] = current["num_confirmations"].clone();
            parent["fee_bump_history"] = json!(candidates.iter().map(|child| {
                let child_id = child["tx_hash"].as_str().unwrap();
                hidden.insert(child_id.to_owned());
                json!({"tx_hash":child_id,"fee_sats":number(&child["total_fees"]).unwrap(),
                    "status":if child["tx_hash"] != current["tx_hash"] { "replaced" }
                    else if child["num_confirmations"].as_i64().unwrap_or(0) > 0 { "confirmed" } else { "mempool" }})
            }).collect::<Vec<_>>());
        }
        normalized.retain(|tx| !tx["tx_hash"].as_str().is_some_and(|id| hidden.contains(id)));
    }

    async fn bump_plan(&self, txid: &str, custom_rate: Option<u64>) -> Result<Plan> {
        let (client, macaroon) = self.lnd_client().await?;
        let info = lnd(&client, &macaroon, "/v1/getinfo", None).await?;
        ensure!(
            info["synced_to_chain"] == true,
            "Wait for the wallet to finish syncing"
        );
        let version = info["version"]
            .as_str()
            .context("LND version is unavailable")?;
        let mut parts = version.trim_start_matches('v').split('.');
        let major: u32 = parts
            .next()
            .unwrap_or("")
            .parse()
            .context("Invalid LND version")?;
        let minor: u32 = parts
            .next()
            .unwrap_or("")
            .parse()
            .context("Invalid LND version")?;
        ensure!(
            major > 0 || minor >= 21,
            "This fee-bump interface requires LND 0.21 or newer"
        );
        let history = lnd(&client, &macaroon, "/v1/transactions", None).await?;
        let txs = array(&history, "transactions")?;
        let tx = txs
            .iter()
            .find(|t| t["tx_hash"] == txid)
            .context("Transaction is not in this wallet")?;
        ensure!(
            tx["num_confirmations"].as_i64() == Some(0),
            "This transaction is no longer pending"
        );
        let entry: Value = self
            .bitcoin_rpc_call(&client, "getmempoolentry", &[json!(txid)])
            .await
            .context("Transaction is not currently in the node's mempool")?;
        ensure!(
            number(&entry["descendantcount"])? == 1,
            "This transaction already has a child; open the child's Bump options instead"
        );
        let pending = lnd(&client, &macaroon, "/v2/wallet/sweeps/pending", None).await?;
        let sweeps = array(&pending, "pending_sweeps")?;
        let published = lnd(
            &client,
            &macaroon,
            "/v2/wallet/sweeps?verbose=false&start_height=-1",
            None,
        )
        .await?;
        let is_sweep = published["transaction_ids"]["transaction_ids"]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|id| id == txid));
        let outputs = array(tx, "output_details")?;
        ensure!(
            tx["amount"]
                .as_str()
                .and_then(|v| v.parse::<i64>().ok())
                .or_else(|| tx["amount"].as_i64())
                .is_some_and(|v| v < 0),
            "Bump is available for outgoing payments and wallet fee sweeps"
        );
        let (
            method,
            input_txid,
            input_index,
            input_sats,
            parent_txid,
            parent_size,
            parent_fee,
            old_fee,
            size,
            recipient_sats,
        ) = if is_sweep {
            // Only a simple wallet CPFP sweep is replaceable here. Anchor/HTLC,
            // batched sweeps and arbitrary signed payments need different previews.
            let raw: Value = self
                .bitcoin_rpc_call(&client, "getrawtransaction", &[json!(txid), json!(true)])
                .await?;
            let inputs = array(&raw, "vin")?;
            ensure!(
                inputs.len() == 1 && outputs.len() == 1 && outputs[0]["is_our_address"] == true,
                "RBF for batched or channel sweeps is not supported here yet"
            );
            let input_txid = inputs[0]["txid"]
                .as_str()
                .context("Missing sweep input")?
                .to_string();
            let index = u32::try_from(number(&inputs[0]["vout"])?)?;
            ensure!(
                sweeps.len() == 1 && outpoint_matches(&sweeps[0]["outpoint"], &input_txid, index),
                "RBF is unavailable while other wallet sweeps are active"
            );
            let parent = txs
                .iter()
                .find(|t| t["tx_hash"] == input_txid)
                .context("Sweep parent is unavailable")?;
            let parent_output = array(parent, "output_details")?
                .iter()
                .find(|o| {
                    number(&o["output_index"]).ok() == Some(index as u64)
                        && o["is_our_address"] == true
                })
                .context("RBF requires a wallet-owned change input")?;
            let parent_entry: Value = self
                .bitcoin_rpc_call(&client, "getmempoolentry", &[json!(input_txid)])
                .await
                .context("Only unconfirmed CPFP sweep replacements are supported here")?;
            ensure!(
                number(&parent_entry["ancestorcount"])? == 1
                    && number(&parent_entry["descendantcount"])? == 2,
                "Complex sweep package cannot be quoted safely"
            );
            let recipients = recipient_amount(parent)?;
            (
                "rbf",
                input_txid.clone(),
                index,
                number(&parent_output["amount"])?,
                input_txid,
                number(&parent_entry["vsize"])?,
                btc_sats(&parent_entry["fees"]["base"])?,
                btc_sats(&entry["fees"]["base"])?,
                sweep_size(parent_output)?.max(number(&entry["vsize"])?),
                recipients,
            )
        } else {
            ensure!(
                sweeps.is_empty(),
                "Another wallet sweep is active; wait for it before creating a CPFP bump"
            );
            ensure!(
                number(&entry["ancestorcount"])? == 1,
                "Fee bumping a chain of unconfirmed payments is not supported yet"
            );
            let unspent = lnd(
                &client,
                &macaroon,
                "/v2/wallet/utxos",
                Some(json!({"unconfirmed_only":true})),
            )
            .await?;
            let utxos = array(&unspent, "utxos")?;
            let leases = lnd(
                &client,
                &macaroon,
                "/v2/wallet/utxos/leases",
                Some(json!({})),
            )
            .await?;
            let locked = array(&leases, "locked_utxos")?;
            let output = outputs
                .iter()
                .filter(|o| o["is_our_address"] == true && sweep_size(o).is_ok())
                .filter(|o| {
                    number(&o["output_index"]).ok().is_some_and(|i| {
                        utxos
                            .iter()
                            .any(|u| outpoint_matches(&u["outpoint"], txid, i as u32))
                            && !locked
                                .iter()
                                .any(|u| outpoint_matches(&u["outpoint"], txid, i as u32))
                    })
                })
                .max_by_key(|o| number(&o["amount"]).unwrap_or(0))
                .context(
                    "RBF is unavailable for this payment. CPFP needs spendable wallet-owned change",
                )?;
            let index = u32::try_from(number(&output["output_index"])?)?;
            let available: Value = self
                .bitcoin_rpc_call(
                    &client,
                    "gettxout",
                    &[json!(txid), json!(index), json!(true)],
                )
                .await?;
            ensure!(
                available.is_object()
                    && number(&available["confirmations"])? == 0
                    && btc_sats(&available["value"])? == number(&output["amount"])?,
                "Change is no longer available"
            );
            (
                "cpfp",
                txid.to_string(),
                index,
                number(&output["amount"])?,
                txid.to_string(),
                number(&entry["vsize"])?,
                btc_sats(&entry["fees"]["base"])?,
                0,
                sweep_size(output)?,
                recipient_amount(tx)?,
            )
        };
        let mempool: Value = self
            .bitcoin_rpc_call(&client, "getmempoolinfo", &[])
            .await?;
        let relay = btc_sats(&mempool["incrementalrelayfee"])?
            .div_ceil(1000)
            .max(1);
        let floor = btc_sats(&mempool["mempoolminfee"])?
            .max(btc_sats(&mempool["minrelaytxfee"])?)
            .div_ceil(1000)
            .max(1);
        let rate = match custom_rate {
            Some(rate) => {
                ensure!(
                    rate >= floor,
                    "Custom rate is below the current mempool minimum"
                );
                rate
            }
            None => {
                let estimate = lnd(&client, &macaroon, "/v2/wallet/estimatefee/1", None).await?;
                number(&estimate["sat_per_kw"])?.div_ceil(250).max(floor)
            }
        };
        let budget = fee_budget(
            rate,
            parent_size,
            parent_fee,
            size,
            old_fee,
            relay.max(floor),
            input_sats,
        )?;
        let tip: String = self
            .bitcoin_rpc_call(&client, "getbestblockhash", &[])
            .await?;
        Ok(Plan {
            txid: txid.to_string(),
            method: method.into(),
            input_txid,
            input_index,
            parent_txid,
            recipient_sats,
            rate_sat_vb: rate,
            current_fee_sats: parent_fee + old_fee,
            additional_fee_sats: budget - old_fee,
            total_fee_sats: parent_fee + budget,
            budget_sats: budget,
            input_sats,
            parent_vsize: parent_size,
            sweep_vsize_bound: size,
            tip,
        })
    }

    pub(in crate::api::rpc) async fn handle_lnd_bump_quote(
        &self,
        params: Option<Value>,
    ) -> Result<Value> {
        let p = params.unwrap_or_default();
        let txid = txid_param(&p)?;
        let path = self
            .config
            .data_dir
            .join("wallet/fee-bumps")
            .join(format!("{txid}.json"));
        ensure!(
            !path.try_exists()?,
            "A bump was already submitted for this transaction. Check its status"
        );
        let custom = p
            .get("sat_per_vbyte")
            .map(|v| v.as_u64().context("Custom rate must be a whole number"))
            .transpose()?;
        if let Some(rate) = custom {
            ensure!(
                (1..=5000).contains(&rate),
                "Custom rate must be 1–5000 sat/vB"
            );
        }
        let plan = self.bump_plan(&txid, custom).await?;
        let quote = Quote {
            quote_id: uuid::Uuid::new_v4().to_string(),
            expires_at: now() + QUOTE_SECONDS,
            custom_rate: custom,
            plan,
        };
        let mut quotes = QUOTES.lock().await;
        quotes.retain(|_, q| q.expires_at > now());
        ensure!(quotes.len() < 128, "Too many fee quotes; try again shortly");
        quotes.insert(quote.quote_id.clone(), quote.clone());
        Ok(serde_json::to_value(quote)?)
    }

    pub(in crate::api::rpc) async fn handle_lnd_bump_submit(
        &self,
        params: Option<Value>,
    ) -> Result<Value> {
        let p = params.unwrap_or_default();
        let txid = txid_param(&p)?;
        let _guard = SUBMIT.lock().await;
        let path = self
            .config
            .data_dir
            .join("wallet/fee-bumps")
            .join(format!("{txid}.json"));
        if path.try_exists()? {
            return self
                .handle_lnd_bump_status(Some(json!({"txid":txid})))
                .await;
        }
        let id = p["quote_id"]
            .as_str()
            .context("A reviewed fee quote is required")?;
        let quote = QUOTES
            .lock()
            .await
            .get(id)
            .cloned()
            .context("Quote expired; review the fee again")?;
        ensure!(
            quote.plan.txid == txid && quote.expires_at > now(),
            "Quote expired; review the fee again"
        );
        let fresh = self.bump_plan(&txid, quote.custom_rate).await?;
        validate_quote(&quote, &fresh, now())?;
        let op = Operation {
            quote: quote.clone(),
            status: "unknown".into(),
            message: "Submission recorded; checking the wallet. Do not submit another bump.".into(),
        };
        reserve(&path, &op).await?;
        QUOTES.lock().await.remove(id);
        let (client, macaroon) = self.lnd_client().await?;
        // At a one-block deadline LND may spend ALL this explicitly previewed
        // budget. It is always below input value, so no extra funding is requested.
        let result = lnd(
            &client,
            &macaroon,
            "/v2/wallet/bumpfee",
            Some(bump_body(&fresh)),
        )
        .await;
        // Keep the write-ahead record even for an RPC error: a lost response can
        // conceal an accepted bump. Status reconciles from wallet/mempool evidence.
        match result {
            Ok(_) => Ok(
                json!({"status":"registered", "message":"Bump registered with the wallet. Waiting for broadcast.", "quote":quote}),
            ),
            Err(_) => Ok(
                json!({"status":"unknown", "message":"The wallet response was not confirmed. Check status; do not submit again.", "quote":quote}),
            ),
        }
    }

    pub(in crate::api::rpc) async fn handle_lnd_bump_status(
        &self,
        params: Option<Value>,
    ) -> Result<Value> {
        let txid = txid_param(&params.unwrap_or_default())?;
        let path = self
            .config
            .data_dir
            .join("wallet/fee-bumps")
            .join(format!("{txid}.json"));
        let bytes = match tokio::fs::read(path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(json!({"status":"none"}))
            }
            Err(e) => return Err(e.into()),
        };
        let op: Operation = serde_json::from_slice(&bytes)
            .context("Bump receipt needs recovery; do not resubmit")?;
        let (client, macaroon) = self.lnd_client().await?;
        let history = lnd(&client, &macaroon, "/v1/transactions", None).await?;
        let plan = &op.quote.plan;
        let input = format!("{}:{}", plan.input_txid, plan.input_index);
        let mut candidates: Vec<&Value> = array(&history, "transactions")?
            .iter()
            .filter(|t| {
                t["tx_hash"] != txid
                    && t["output_details"].as_array().is_some_and(|outputs| {
                        !outputs.is_empty() && outputs.iter().all(|o| o["is_our_address"] == true)
                    })
                    && t["previous_outpoints"]
                        .as_array()
                        .is_some_and(|inputs| inputs.iter().any(|i| i["outpoint"] == input))
            })
            .collect();
        candidates.sort_by_key(|t| std::cmp::Reverse(number(&t["time_stamp"]).unwrap_or(0)));
        for t in candidates {
            let id = t["tx_hash"]
                .as_str()
                .context("Missing bump transaction ID")?;
            let confirmed = t["num_confirmations"].as_i64().unwrap_or(0) > 0;
            let accepted = if confirmed {
                false
            } else {
                self.bitcoin_rpc_call::<Value>(&client, "getmempoolentry", &[json!(id)])
                    .await
                    .is_ok()
            };
            if confirmed || accepted {
                return Ok(json!({"status":if confirmed {"confirmed"} else {"mempool"},
                    "message":if confirmed {"Fee bump confirmed."} else {"Fee bump accepted in the node's mempool; awaiting confirmation."},
                    "bump_txid":id,"confirmations":t["num_confirmations"],"actual_sweep_fee_sats":number(&t["total_fees"])?,"quote":op.quote}));
            }
        }
        let pending = lnd(&client, &macaroon, "/v2/wallet/sweeps/pending", None).await?;
        let registered = array(&pending, "pending_sweeps")?.iter().any(|s| {
            outpoint_matches(&s["outpoint"], &plan.input_txid, plan.input_index)
                && number(&s["budget"]).ok() == Some(plan.budget_sats)
                && number(&s["requested_sat_per_vbyte"]).ok() == Some(plan.rate_sat_vb)
        });
        Ok(
            json!({"status":if registered {"registered"} else {"unknown"},
            "message":if registered {"Bump registered; waiting for a verified broadcast."} else {"Submission outcome is unknown. Do not submit again; check wallet status."}, "quote":op.quote}),
        )
    }
}
fn recipient_amount(tx: &Value) -> Result<u64> {
    array(tx, "output_details")?
        .iter()
        .filter(|o| o["is_our_address"] == false)
        .try_fold(0u64, |sum, o| {
            sum.checked_add(number(&o["amount"])?)
                .context("Recipient amount overflow")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample_plan() -> Plan {
        serde_json::from_value(json!({"txid":"a","method":"cpfp","input_txid":"a","input_index":0,"parent_txid":"a","recipient_sats":161650,"rate_sat_vb":3,"current_fee_sats":144,"additional_fee_sats":618,"total_fee_sats":762,"budget_sats":618,"input_sats":21126,"parent_vsize":142,"sweep_vsize_bound":112,"tip":"tip"})).unwrap()
    }
    #[test]
    fn history_requires_owned_simple_fee_only_child() {
        let plan = sample_plan();
        let tx = json!({"tx_hash":"b".repeat(64),"amount":"-200","total_fees":"200",
            "previous_outpoints":[{"outpoint":"a:0","is_our_output":true}],
            "output_details":[{"is_our_address":true}]});
        assert!(fee_child_matches(&tx, &plan));
        for bad in [
            json!({"amount":"-201"}),
            json!({"amount":"200"}),
            json!({"total_fees":"0"}),
            json!({"previous_outpoints":[{"outpoint":"a:1","is_our_output":true}]}),
            json!({"previous_outpoints":[{"outpoint":"a:0","is_our_output":false}]}),
            json!({"previous_outpoints":[{"outpoint":"a:0","is_our_output":true},{"outpoint":"c:0","is_our_output":true}]}),
            json!({"output_details":[{"is_our_address":false}]}),
            json!({"output_details":[]}),
            json!({"tx_hash":"../../invalid"}),
        ] {
            let mut changed = tx.clone();
            for (key, value) in bad.as_object().unwrap() {
                changed[key] = value.clone();
            }
            assert!(!fee_child_matches(&changed, &plan), "{bad}");
        }
    }
    #[test]
    fn stale_quotes_cannot_silently_change_approved_fee_or_transaction() {
        let plan = sample_plan();
        let q = Quote {
            quote_id: "q".into(),
            expires_at: 100,
            custom_rate: None,
            plan: plan.clone(),
        };
        assert!(validate_quote(&q, &plan, 99).is_ok());
        assert!(validate_quote(&q, &plan, 100).is_err());
        let mut changed = plan.clone();
        changed.budget_sats += 1;
        assert!(validate_quote(&q, &changed, 99).is_err());
        changed = plan.clone();
        changed.input_index += 1;
        assert!(validate_quote(&q, &changed, 99).is_err());
        changed = plan.clone();
        changed.recipient_sats -= 1;
        assert!(validate_quote(&q, &changed, 99).is_err());
        changed = plan.clone();
        changed.tip = "new block".into();
        assert!(validate_quote(&q, &changed, 99).is_err());
    }
    #[test]
    fn mutation_always_has_explicit_budget_and_does_not_send_a_second_payment() {
        assert_eq!(
            bump_body(&sample_plan()),
            json!({"outpoint":{"txid_str":"a","output_index":0},"sat_per_vbyte":"3","budget":"618","deadline_delta":1,"immediate":true})
        );
        let mut rbf = sample_plan();
        rbf.method = "rbf".into();
        rbf.txid = "child".into();
        // RBF uses the already-registered input, not the child's output.
        assert_eq!(bump_body(&rbf)["outpoint"]["txid_str"], "a");
    }
    #[test]
    fn outpoint_ownership_and_recipient_exclude_wallet_change() {
        assert!(outpoint_matches(
            &json!({"txid_str":"a","output_index":2}),
            "a",
            2
        ));
        assert!(!outpoint_matches(
            &json!({"txid_str":"b","output_index":2}),
            "a",
            2
        ));
        assert!(!outpoint_matches(
            &json!({"txid_str":"a","output_index":3}),
            "a",
            2
        ));
        assert_eq!(recipient_amount(&json!({"output_details":[{"is_our_address":true,"amount":"21126"},{"is_our_address":false,"amount":"161650"}]})).unwrap(), 161650);
        assert!(recipient_amount(
            &json!({"output_details":[{"is_our_address":false,"amount":"bad"}]})
        )
        .is_err());
    }
    #[test]
    fn cpfp_budget_covers_parent_and_preserves_change() {
        assert_eq!(fee_budget(3, 142, 144, 112, 0, 1, 21126).unwrap(), 618);
        assert!(fee_budget(5000, 142, 144, 112, 0, 1, 21126).is_err());
        assert!(fee_budget(0, 142, 144, 112, 0, 1, 21126).is_err());
    }
    #[test]
    fn rbf_pays_incremental_relay_cost_and_counts_only_extra_cost() {
        let fee = fee_budget(3, 142, 144, 112, 650, 1, 21126).unwrap();
        assert_eq!(fee, 763);
        assert_eq!(fee - 650, 113);
    }
    #[test]
    fn unsupported_outputs_and_malformed_ids_fail_closed() {
        assert!(sweep_size(&json!({"output_type":"SCRIPT_TYPE_WITNESS_V0_SCRIPT_HASH"})).is_err());
        assert!(txid_param(&json!({"txid":"../../file"})).is_err());
        assert!(number(&json!(-1)).is_err());
        assert!(btc_sats(&json!(-0.1)).is_err());
        assert_eq!(btc_sats(&json!(0.00000650)).unwrap(), 650);
    }
    #[tokio::test]
    async fn receipt_prevents_duplicate_submission_after_restart() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let path = dir.join("receipt.json");
        let plan: Plan = serde_json::from_value(json!({"txid":"a","method":"cpfp","input_txid":"a","input_index":0,"parent_txid":"a","recipient_sats":1000,"rate_sat_vb":3,"current_fee_sats":144,"additional_fee_sats":618,"total_fee_sats":762,"budget_sats":618,"input_sats":21126,"parent_vsize":142,"sweep_vsize_bound":112,"tip":"tip"})).unwrap();
        let op = Operation {
            quote: Quote {
                quote_id: "q".into(),
                expires_at: now() + 60,
                custom_rate: None,
                plan,
            },
            status: "unknown".into(),
            message: "pending".into(),
        };
        reserve(&path, &op).await.unwrap();
        assert!(reserve(&path, &op).await.is_err());
        let restored: Operation =
            serde_json::from_slice(&tokio::fs::read(&path).await.unwrap()).unwrap();
        assert_eq!(restored.quote.plan.budget_sats, 618);
        tokio::fs::remove_dir_all(dir).await.unwrap();
    }
}
