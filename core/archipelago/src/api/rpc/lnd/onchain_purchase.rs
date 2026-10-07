//! Draft adapter for exact-template on-chain purchases. No owner RPC uses it yet.
//! No FundPsbt call: all inputs/outputs are saved before individually leasing.
use crate::content_onchain::{
    self as engine, FeePolicy, Funded, Journal, Lease, Record, Signed, Wallet,
};
use anyhow::{Context, Result};
use base64::Engine;
use bitcoin::{
    absolute::LockTime, consensus, psbt::Psbt, transaction::Version, Amount, OutPoint, ScriptBuf,
    Sequence, Transaction, TxIn, TxOut, Witness,
};
use serde_json::{json, Value};
use std::collections::HashSet;

const MAX_SATS: u64 = 2_100_000_000_000_000;
const MAX_INPUTS: usize = 32;
const LEASE_SECONDS: u64 = 600;
/// Change address must already be durably prepared by the caller and confirmed
/// as a local internal address. This adapter never silently derives/reuses one.
pub(crate) struct PlanRequest {
    pub change_address: String,
    pub max_fee_sats: u64,
    pub sat_per_vbyte: Option<u64>,
}
pub(crate) struct LndPurchaseWallet {
    client: reqwest::Client,
    base: String,
    macaroon: String,
}
impl LndPurchaseWallet {
    pub(crate) fn new(client: reqwest::Client, base: String, macaroon: String) -> Self {
        Self {
            client,
            base: base.trim_end_matches('/').into(),
            macaroon,
        }
    }
    async fn raw(&self, path: &str, body: Option<Value>) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base, path);
        let request = if let Some(body) = body {
            self.client.post(url).json(&body)
        } else {
            self.client.get(url)
        };
        Ok(request
            .header("Grpc-Metadata-macaroon", &self.macaroon)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await?)
    }
    async fn json(&self, path: &str, body: Option<Value>) -> Result<Value> {
        let response = self.raw(path, body).await?;
        Self::read_json(response).await
    }
    async fn read_json(mut response: reqwest::Response) -> Result<Value> {
        anyhow::ensure!(
            response.status().is_success(),
            "Wallet operation unavailable ({})",
            response.status()
        );
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(
                bytes.len() + chunk.len() <= 4 * 1024 * 1024,
                "Wallet response too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
    async fn all_leases(&self) -> Result<Vec<Lease>> {
        let body = self
            .json("/v2/wallet/utxos/leases", Some(json!({})))
            .await?;
        body["locked_utxos"]
            .as_array()
            .context("Wallet omitted leases")?
            .iter()
            .map(parse_lease)
            .collect()
    }
    async fn available(&self) -> Result<Vec<Lease>> {
        let body = self
            .json(
                "/v2/wallet/utxos",
                Some(json!({"min_confs":1,"max_confs":2147483647,"account":"default"})),
            )
            .await?;
        let leased: HashSet<_> = self
            .all_leases()
            .await?
            .iter()
            .map(|l| l.outpoint())
            .collect::<Result<_>>()?;
        let mut seen = HashSet::new();
        let mut result = vec![];
        for row in body["utxos"].as_array().context("Wallet omitted UTXOs")? {
            let outpoint = parse_outpoint(&row["outpoint"])?;
            anyhow::ensure!(seen.insert(outpoint), "Duplicate wallet output");
            if leased.contains(&outpoint) {
                continue;
            }
            if integer(&row["confirmations"])? < 1 {
                continue;
            }
            let script = row["pk_script"].as_str().context("Missing UTXO script")?;
            let decoded = ScriptBuf::from_bytes(hex::decode(script)?);
            if !decoded.is_p2wpkh() && !decoded.is_p2tr() {
                continue;
            }
            let value = integer(&row["amount_sat"])?;
            anyhow::ensure!(
                value > 0 && value <= MAX_SATS,
                "Invalid wallet output value"
            );
            result.push(Lease {
                lock_id: String::new(),
                txid: outpoint.txid.to_string(),
                vout: outpoint.vout,
                value_sats: value,
                script: script.to_ascii_lowercase(),
                expires_at: 0,
            });
        }
        result.sort_by_key(|l| (l.value_sats, l.txid.clone(), l.vout));
        Ok(result)
    }
    async fn reserve(&self) -> Result<u64> {
        let reserve = integer(
            &self
                .json("/v2/wallet/reserve?additional_public_channels=0", None)
                .await?["required_reserve"],
        )?;
        anyhow::ensure!(reserve <= MAX_SATS, "Invalid channel reserve");
        Ok(reserve)
    }
    async fn verify_wallet(&self, record: &Record) -> Result<()> {
        let expected_network = record.network().context("Missing original network")?;
        let info = self.json("/v1/getinfo", None).await?;
        anyhow::ensure!(
            info["synced_to_chain"] == true,
            "Wallet is not synced; no payment prepared"
        );
        let network = match expected_network {
            engine::ChainNetwork::Mainnet => "mainnet",
            engine::ChainNetwork::Testnet => "testnet",
            engine::ChainNetwork::Signet => "signet",
            engine::ChainNetwork::Regtest => "regtest",
        };
        let chains = info["chains"]
            .as_array()
            .context("Missing wallet network")?;
        anyhow::ensure!(
            chains.len() == 1 && chains[0]["chain"] == "bitcoin" && chains[0]["network"] == network,
            "Wallet and original payment networks differ"
        );
        let accounts = self.json("/v2/wallet/accounts?name=default", None).await?;
        let accounts = accounts["accounts"]
            .as_array()
            .context("Wallet account information unavailable")?;
        anyhow::ensure!(
            !accounts.is_empty()
                && accounts
                    .iter()
                    .all(|a| a["name"] == "default" && a["watch_only"] == false),
            "Native purchase signing requires a local spending account"
        );
        Ok(())
    }
    async fn verify_change(&self, record: &Record, address: &str) -> Result<ScriptBuf> {
        let network = record.network().context("Missing original network")?;
        let parsed = address
            .parse::<bitcoin::Address<bitcoin::address::NetworkUnchecked>>()?
            .require_network(network.bitcoin())?;
        let script = parsed.script_pubkey();
        anyhow::ensure!(
            script.is_p2wpkh() || script.is_p2tr(),
            "Unsupported change address type"
        );
        anyhow::ensure!(
            record
                .quote
                .as_ref()
                .map(|q| q.script().map(|s| s != script))
                .transpose()?
                .unwrap_or(true),
            "Change cannot be the seller address"
        );
        let addresses = self
            .json("/v2/wallet/addresses?account_name=default", None)
            .await?;
        let mut owned = false;
        for account in addresses["account_with_addresses"]
            .as_array()
            .context("Wallet address ownership unavailable")?
        {
            if account["name"] != "default" {
                continue;
            }
            for candidate in account["addresses"]
                .as_array()
                .context("Wallet omitted addresses")?
            {
                if candidate["is_internal"] == true
                    && candidate["address"].as_str() == Some(address)
                {
                    owned = true;
                }
            }
        }
        anyhow::ensure!(
            owned,
            "Change address is not a verified internal wallet address"
        );
        Ok(script)
    }
    /// Persist allocation uncertainty before NextAddr. Never retry an unknown
    /// allocation: LND provides no caller idempotency key for this operation.
    pub(crate) async fn prepare_change(&self, journal: &Journal) -> Result<String> {
        let mut record = journal.load()?.context("Missing original purchase")?;
        if let Some(engine::ChangeAddress::Ready { address }) = &record.change_address {
            self.verify_change(&record, address).await?;
            return Ok(address.clone());
        }
        anyhow::ensure!(record.change_address.is_none(), "Original change address allocation is ambiguous; preserve this operation for recovery, do not allocate another");
        anyhow::ensure!(
            matches!(
                record.phase,
                engine::Phase::OfferPrepared | engine::Phase::Quoted
            ) && record.retirement.is_none()
                && !record.externally_exposed
                && !record.settled,
            "Original payment cannot allocate change"
        );
        self.verify_wallet(&record).await?;
        record.change_address = Some(engine::ChangeAddress::Dispatched);
        record.mutations = record
            .mutations
            .checked_add(1)
            .context("Mutation sequence exhausted")?;
        journal.save(&record)?;
        let reply = self
            .json(
                "/v2/wallet/address/next",
                Some(json!({"account":"default","type":"WITNESS_PUBKEY_HASH","change":true})),
            )
            .await?;
        let address = reply["addr"]
            .as_str()
            .context("Original change allocation reply omitted address")?
            .to_owned();
        self.verify_change(&record, &address).await?;
        record.change_address = Some(engine::ChangeAddress::Ready {
            address: address.clone(),
        });
        journal.save(&record)?;
        Ok(address)
    }
    /// Review produces only typed inputs/amounts/scripts and fee policy. There
    /// is no recipient address, raw transaction, or signable PSBT at this stage.
    pub(crate) async fn prepare_plan(
        &self,
        journal: &Journal,
        request: PlanRequest,
    ) -> Result<Record> {
        use crate::content_onchain_plan::{FundingPlan, PlanInput};
        let record = journal.load()?.context("Original purchase missing")?;
        anyhow::ensure!(
            record.phase == engine::Phase::OfferPrepared
                && record.retirement.is_none()
                && !record.externally_exposed,
            "Original offer cannot prepare a new plan"
        );
        let offer = record.offer.as_ref().context("Original offer missing")?;
        offer.validate()?;
        anyhow::ensure!(
            request.max_fee_sats > 0 && request.max_fee_sats <= MAX_SATS,
            "Explicit fee budget required"
        );
        self.verify_wallet(&record).await?;
        let change = self.verify_change(&record, &request.change_address).await?;
        let rate = match request.sat_per_vbyte {
            Some(rate) => {
                anyhow::ensure!((1..=5000).contains(&rate), "Unsupported fee rate");
                rate
            }
            None => super::fee_policy::estimated_sat_per_vbyte(
                &self.json("/v2/wallet/estimatefee/1", None).await?,
            )?,
        };
        let available = self.available().await?;
        let reserve = self.reserve().await?;
        let available_sats = total(&available)?;
        let mut selected = vec![];
        let mut choice = None;
        for input in available.iter().take(MAX_INPUTS) {
            selected.push(input.clone());
            let selected_sats = total(&selected)?;
            if available_sats
                .checked_sub(selected_sats)
                .context("Invalid available balance")?
                < reserve
            {
                break;
            }
            let fee = rate
                .checked_mul(crate::content_onchain_plan::max_vsize(
                    &selected,
                    Some(&change),
                )?)
                .context("Fee overflow")?;
            let Some(available_fee) = selected_sats.checked_sub(record.binding.price_sats) else {
                continue;
            };
            let (change_sats, fee) =
                if let Some(change_sats) = available_fee.checked_sub(fee).filter(|n| *n >= 546) {
                    (change_sats, fee)
                } else {
                    let minimum = rate
                        .checked_mul(crate::content_onchain_plan::max_vsize(&selected, None)?)
                        .context("Fee overflow")?;
                    if available_fee < minimum {
                        continue;
                    }
                    (0, available_fee)
                };
            anyhow::ensure!(fee<=request.max_fee_sats,"Fast fee exceeds maximum; no seller address or inputs allocated, cancel or review another method");
            choice = Some((selected.clone(), change_sats, fee));
            break;
        }
        let (selected,change_sats,fee_sats)=choice.context("Insufficient supported confirmed funds after channel reserve; seller address is not allocated")?;
        let mut inputs = vec![];
        for mut lease in selected {
            lease.lock_id = record.lock_id.clone();
            let previous = self
                .json(&format!("/v2/wallet/tx?txid={}", lease.txid), None)
                .await?;
            inputs.push(PlanInput {
                lease,
                previous_tx_hex: previous["raw_tx_hex"]
                    .as_str()
                    .context("Original input transaction unavailable")?
                    .into(),
            });
        }
        let plan = FundingPlan {
            offer_sha256: offer.hash()?,
            inputs,
            change_address: request.change_address,
            change_script: hex::encode(change.as_bytes()),
            change_sats,
            fee_sats,
            fee_rate_sat_vbyte: rate,
            max_fee_sats: request.max_fee_sats,
        };
        engine::save_plan(journal, plan)
    }
    /// Read-only preparation. The caller's explicit fee budget is mandatory;
    /// omitted rate uses the existing Fast/next-block estimate, never a fixed rate.
    pub(crate) async fn prepare_exact(
        &self,
        journal: &Journal,
        request: PlanRequest,
    ) -> Result<Record> {
        let record = journal.load()?.context("Missing original purchase")?;
        anyhow::ensure!(
            record.phase == engine::Phase::Quoted && !record.externally_exposed && !record.settled,
            "Original payment already started or address exposed"
        );
        anyhow::ensure!(
            request.max_fee_sats > 0 && request.max_fee_sats <= MAX_SATS,
            "Explicit fee budget required"
        );
        if let Some(change) = &record.change_address {
            anyhow::ensure!(
                matches!(change, engine::ChangeAddress::Ready { address } if address == &request.change_address),
                "Recover original change allocation before preparing payment"
            );
        }
        self.verify_wallet(&record).await?;
        let change = self.verify_change(&record, &request.change_address).await?;
        let rate = match request.sat_per_vbyte {
            Some(rate) => {
                anyhow::ensure!(
                    (1..=5000).contains(&rate),
                    "Fee rate outside supported bounds"
                );
                rate
            }
            None => super::fee_policy::estimated_sat_per_vbyte(
                &self.json("/v2/wallet/estimatefee/1", None).await?,
            )?,
        };
        let available = self.available().await?;
        let reserve = self.reserve().await?;
        let (mut psbt, mut inputs, fee) = select_template(
            &record,
            &change,
            &available,
            reserve,
            rate,
            request.max_fee_sats,
        )?;
        // Include exact previous transactions as well as witness UTXOs. Never
        // rely on aggregate transaction amounts or unverified supplied values.
        for (metadata, input) in psbt.inputs.iter_mut().zip(&inputs) {
            let previous = self
                .json(&format!("/v2/wallet/tx?txid={}", input.txid), None)
                .await?;
            let raw = previous["raw_tx_hex"]
                .as_str()
                .context("Original input transaction unavailable")?;
            let tx: Transaction = consensus::deserialize(&hex::decode(raw)?)?;
            anyhow::ensure!(
                tx.compute_txid().to_string() == input.txid,
                "Input transaction changed"
            );
            let output = tx
                .output
                .get(input.vout as usize)
                .context("Input output missing")?;
            anyhow::ensure!(
                output.value.to_sat() == input.value_sats
                    && hex::encode(output.script_pubkey.as_bytes()) == input.script,
                "Original input output changed"
            );
            metadata.non_witness_utxo = Some(tx);
        }
        for input in &mut inputs {
            input.lock_id = record.lock_id.clone();
        }
        // The absolute approved fee is authoritative. This conservative rate
        // bound derived from unsigned size also bounds every finalized size.
        let max_rate = fee.div_ceil(psbt.unsigned_tx.vsize() as u64);
        let policy = FeePolicy {
            change_script: hex::encode(change.as_bytes()),
            max_fee_sats: request.max_fee_sats,
            max_fee_rate_sat_vbyte: max_rate,
        };
        let template = Funded {
            psbt_base64: base64::engine::general_purpose::STANDARD.encode(psbt.serialize()),
            leases: inputs,
        };
        engine::save_exact_template(journal, policy, template)
    }
}
fn integer(value: &Value) -> Result<u64> {
    match value {
        Value::String(s) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) => {
            s.parse().context("Invalid wallet integer")
        }
        value => value.as_u64().context("Missing wallet integer"),
    }
}
fn parse_outpoint(value: &Value) -> Result<OutPoint> {
    let txid = if let Some(s) = value["txid_str"].as_str().filter(|s| !s.is_empty()) {
        s.parse()?
    } else {
        let mut raw = base64::engine::general_purpose::STANDARD
            .decode(value["txid_bytes"].as_str().context("Missing outpoint")?)?;
        anyhow::ensure!(raw.len() == 32, "Invalid outpoint hash");
        raw.reverse();
        hex::encode(raw).parse()?
    };
    Ok(OutPoint {
        txid,
        vout: u32::try_from(integer(&value["output_index"])?).context("Invalid output index")?,
    })
}
fn parse_lease(value: &Value) -> Result<Lease> {
    let id = base64::engine::general_purpose::STANDARD
        .decode(value["id"].as_str().context("Missing lease owner")?)?;
    anyhow::ensure!(id.len() == 32, "Invalid lease owner");
    let point = parse_outpoint(&value["outpoint"])?;
    Ok(Lease {
        lock_id: hex::encode(id),
        txid: point.txid.to_string(),
        vout: point.vout,
        value_sats: integer(&value["value"])?,
        script: hex::encode(
            base64::engine::general_purpose::STANDARD.decode(
                value["pk_script"]
                    .as_str()
                    .context("Missing lease script")?,
            )?,
        ),
        expires_at: integer(&value["expiration"])?,
    })
}
fn total(inputs: &[Lease]) -> Result<u64> {
    inputs.iter().try_fold(0u64, |sum, input| {
        sum.checked_add(input.value_sats)
            .filter(|v| *v <= MAX_SATS)
            .context("Wallet amount overflow")
    })
}
fn transaction(
    record: &Record,
    change: &ScriptBuf,
    inputs: &[Lease],
    change_value: Option<u64>,
) -> Result<Transaction> {
    let quote = record.quote.as_ref().context("Missing original quote")?;
    let mut output = vec![TxOut {
        value: Amount::from_sat(quote.binding.price_sats),
        script_pubkey: quote.script()?,
    }];
    if let Some(value) = change_value {
        output.push(TxOut {
            value: Amount::from_sat(value),
            script_pubkey: change.clone(),
        });
    }
    Ok(Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: inputs
            .iter()
            .map(|input| {
                Ok(TxIn {
                    previous_output: input.outpoint()?,
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                    witness: Witness::new(),
                })
            })
            .collect::<Result<_>>()?,
        output,
    })
}
fn max_signed_vsize(mut tx: Transaction, inputs: &[Lease]) -> Result<u64> {
    for (input, lease) in tx.input.iter_mut().zip(inputs) {
        let script = ScriptBuf::from_bytes(hex::decode(&lease.script)?);
        input.witness = if script.is_p2wpkh() {
            Witness::from_slice(&[vec![0; 73], vec![0; 33]])
        } else if script.is_p2tr() {
            Witness::from_slice(&[vec![0; 65]])
        } else {
            anyhow::bail!("Unsupported signing input")
        };
    }
    Ok(tx.vsize() as u64)
}
fn select_template(
    record: &Record,
    change: &ScriptBuf,
    available: &[Lease],
    reserve: u64,
    rate: u64,
    max_fee: u64,
) -> Result<(Psbt, Vec<Lease>, u64)> {
    let quote = record.quote.as_ref().context("Missing original quote")?;
    let available_sats = total(available)?;
    let mut selected = vec![];
    for input in available.iter().take(MAX_INPUTS) {
        selected.push(input.clone());
        let selected_sats = total(&selected)?;
        // Leave channel reserve in confirmed, unselected outputs. Do not count
        // the proposed (not-yet-confirmed) change as reserve.
        if available_sats
            .checked_sub(selected_sats)
            .context("Invalid available amount")?
            < reserve
        {
            break;
        }
        let skeleton = transaction(record, change, &selected, Some(546))?;
        let fee = rate
            .checked_mul(max_signed_vsize(skeleton, &selected)?)
            .context("Fee overflow")?;
        let Some(remainder) = selected_sats
            .checked_sub(quote.binding.price_sats)
            .and_then(|v| v.checked_sub(fee))
        else {
            continue;
        };
        let (tx, fee) = if remainder >= 546 {
            (
                transaction(record, change, &selected, Some(remainder))?,
                fee,
            )
        } else {
            let tx = transaction(record, change, &selected, None)?;
            let minimum = rate
                .checked_mul(max_signed_vsize(tx.clone(), &selected)?)
                .context("Fee overflow")?;
            let fee = selected_sats - quote.binding.price_sats;
            if fee < minimum {
                continue;
            }
            (tx, fee)
        };
        anyhow::ensure!(
            fee <= max_fee,
            "Estimated Fast fee exceeds the approved fee cap; no inputs leased"
        );
        let mut psbt = Psbt::from_unsigned_tx(tx)?;
        for (metadata, lease) in psbt.inputs.iter_mut().zip(&selected) {
            metadata.witness_utxo = Some(TxOut {
                value: Amount::from_sat(lease.value_sats),
                script_pubkey: ScriptBuf::from_bytes(hex::decode(&lease.script)?),
            });
        }
        return Ok((psbt, selected, fee));
    }
    anyhow::bail!("Insufficient supported confirmed outputs after channel reserve (maximum 32 inputs); no inputs leased")
}
impl Wallet for LndPurchaseWallet {
    async fn prepare_funding(&self, record: &Record) -> Result<()> {
        self.verify_wallet(record).await?;
        if let Some(engine::ChangeAddress::Ready { address }) = &record.change_address {
            self.verify_change(record, address).await?;
        }
        let planned_inputs: Vec<Lease> = if let Some(template) = &record.template {
            engine::validate_funded(record, template)?;
            template.leases.clone()
        } else {
            let plan = record
                .plan
                .as_ref()
                .context("Exact funding plan must be saved before leasing")?;
            plan.validate(record)?;
            plan.inputs.iter().map(|i| i.lease.clone()).collect()
        };
        let planned: HashSet<_> = planned_inputs
            .iter()
            .map(|l| l.outpoint())
            .collect::<Result<_>>()?;
        let available = self.available().await?;
        let owned = self.leases(&record.lock_id).await?;
        for expected in &planned_inputs {
            anyhow::ensure!(available.iter().chain(&owned).any(|actual|actual.txid==expected.txid&&actual.vout==expected.vout&&actual.value_sats==expected.value_sats&&actual.script==expected.script),"Original planned input is unavailable; no replacement input or seller address requested");
        }
        let other: Vec<_> = available
            .into_iter()
            .filter(|l| l.outpoint().is_ok_and(|p| !planned.contains(&p)))
            .collect();
        anyhow::ensure!(
            total(&other)? >= self.reserve().await?,
            "Channel reserve changed; original inputs remain reserved, no transaction signed"
        );
        Ok(())
    }
    async fn fund(&self, _: &Record) -> Result<Funded> {
        anyhow::bail!(
            "Automatic FundPsbt is disabled; persist an exact template and lease its inputs"
        )
    }
    async fn leases(&self, lock_id: &str) -> Result<Vec<Lease>> {
        Ok(self
            .all_leases()
            .await?
            .into_iter()
            .filter(|l| l.lock_id == lock_id)
            .collect())
    }
    async fn lease(&self, lease: &Lease) -> Result<()> {
        let id = base64::engine::general_purpose::STANDARD.encode(hex::decode(&lease.lock_id)?);
        let response=self.json("/v2/wallet/utxos/lease",Some(json!({"id":id,"outpoint":{"txid_str":lease.txid,"output_index":lease.vout},"expiration_seconds":LEASE_SECONDS.to_string()}))).await?;
        anyhow::ensure!(
            integer(&response["expiration"])? > u64::try_from(chrono::Utc::now().timestamp())?,
            "Wallet did not confirm original input lease"
        );
        Ok(())
    }
    async fn sign(&self, funded: &Funded) -> Result<Signed> {
        let response = self
            .json(
                "/v2/wallet/psbt/finalize",
                Some(json!({"funded_psbt":funded.psbt_base64,"account":"default"})),
            )
            .await?;
        let raw = base64::engine::general_purpose::STANDARD.decode(
            response["raw_final_tx"]
                .as_str()
                .context("Missing original signed transaction")?,
        )?;
        let tx: Transaction = consensus::deserialize(&raw)?;
        Ok(Signed {
            raw_hex: hex::encode(raw),
            txid: tx.compute_txid().to_string(),
        })
    }
    async fn publish(&self, signed: &Signed) -> Result<()> {
        let raw = base64::engine::general_purpose::STANDARD.encode(hex::decode(&signed.raw_hex)?);
        let response = self
            .json("/v2/wallet/tx", Some(json!({"tx_hex":raw})))
            .await?;
        anyhow::ensure!(
            response
                .get("publish_error")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty),
            "Original transaction publication remains unconfirmed"
        );
        Ok(())
    }
    async fn transaction_known(&self, txid: &str) -> Result<bool> {
        let _: bitcoin::Txid = txid.parse()?;
        let response = self
            .raw(&format!("/v2/wallet/tx?txid={txid}"), None)
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        anyhow::ensure!(
            response.status().is_success(),
            "Original transaction lookup unavailable"
        );
        let body = Self::read_json(response).await?;
        anyhow::ensure!(
            body["tx_hash"].as_str() == Some(txid),
            "Wallet returned a different transaction"
        );
        Ok(true)
    }
}

impl super::RpcHandler {
    pub(crate) async fn onchain_purchase_wallet(&self) -> Result<LndPurchaseWallet> {
        let (client, macaroon) = self.lnd_client().await?;
        Ok(LndPurchaseWallet::new(
            client,
            super::LND_REST_BASE_URL.into(),
            macaroon,
        ))
    }
}
impl LndPurchaseWallet {
    pub(crate) async fn network(&self) -> Result<engine::ChainNetwork> {
        let info = self.json("/v1/getinfo", None).await?;
        anyhow::ensure!(info["synced_to_chain"] == true, "Wallet is not synced");
        let chains = info["chains"]
            .as_array()
            .context("Wallet network unavailable")?;
        anyhow::ensure!(
            chains.len() == 1 && chains[0]["chain"] == "bitcoin",
            "Unsupported wallet chain"
        );
        match chains[0]["network"].as_str() {
            Some("mainnet") => Ok(engine::ChainNetwork::Mainnet),
            Some("testnet") => Ok(engine::ChainNetwork::Testnet),
            Some("signet") => Ok(engine::ChainNetwork::Signet),
            Some("regtest") => Ok(engine::ChainNetwork::Regtest),
            _ => anyhow::bail!("Unsupported wallet network"),
        }
    }
}
impl crate::content_onchain_seller::Wallet for LndPurchaseWallet {
    async fn network(&self) -> Result<engine::ChainNetwork> {
        LndPurchaseWallet::network(self).await
    }
    async fn preflight(&self, network: engine::ChainNetwork) -> Result<()> {
        anyhow::ensure!(
            self.network().await? == network,
            "Original seller network changed"
        );
        let accounts = self.json("/v2/wallet/accounts?name=default", None).await?;
        let accounts = accounts["accounts"]
            .as_array()
            .context("Wallet account unavailable")?;
        anyhow::ensure!(
            !accounts.is_empty()
                && accounts
                    .iter()
                    .all(|a| a["name"] == "default" && a["watch_only"] == false),
            "Seller wallet account unavailable"
        );
        Ok(())
    }
    async fn allocate(&self) -> Result<String> {
        let body = self
            .json(
                "/v2/wallet/address/next",
                Some(json!({"account":"default","type":"WITNESS_PUBKEY_HASH","change":false})),
            )
            .await?;
        Ok(body["addr"]
            .as_str()
            .context("Receive allocation reply omitted address")?
            .to_owned())
    }
    async fn received(&self, address: &str, amount: u64) -> Result<bool> {
        let body = self.json("/v1/transactions", None).await?;
        Ok(super::wallet::confirmed_address_sats(&body, address)? >= amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_lightning::{Binding, RetainedFile};
    use hyper::{
        service::{make_service_fn, service_fn},
        Body, Response, Server, StatusCode,
    };
    use std::{
        convert::Infallible,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        },
    };
    fn script(byte: u8) -> ScriptBuf {
        let mut bytes = vec![0, 20];
        bytes.extend([byte; 20]);
        ScriptBuf::from_bytes(bytes)
    }
    fn address(byte: u8) -> String {
        bitcoin::Address::from_script(&script(byte), bitcoin::Network::Regtest)
            .unwrap()
            .to_string()
    }
    fn previous(byte: u8, value: u64) -> Transaction {
        Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::from_bytes(vec![1, byte]),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(value),
                script_pubkey: script(byte),
            }],
        }
    }
    struct Mock {
        calls: Mutex<Vec<(String, Value)>>,
        leases: Mutex<Vec<Lease>>,
        previous: Vec<Transaction>,
        published: Mutex<Option<Transaction>>,
        lose_change: AtomicBool,
        lose_lease: AtomicBool,
        lose_sign: AtomicBool,
        lose_publish: AtomicBool,
        reserve: u64,
        rate: u64,
        record_path: std::path::PathBuf,
        locked_id: String,
    }
    impl Mock {
        fn count(&self, path: &str) -> usize {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(p, _)| p == path)
                .count()
        }
        fn saved(&self) -> Record {
            let envelope: Value =
                serde_json::from_slice(&std::fs::read(&self.record_path).unwrap()).unwrap();
            serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap()
        }
    }
    fn lease_value(lease: &Lease) -> Value {
        json!({"id":base64::engine::general_purpose::STANDARD.encode(hex::decode(&lease.lock_id).unwrap()),
        "outpoint":{"txid_str":lease.txid,"output_index":lease.vout},"value":lease.value_sats.to_string(),
        "pk_script":base64::engine::general_purpose::STANDARD.encode(hex::decode(&lease.script).unwrap()),"expiration":lease.expires_at.to_string()})
    }
    async fn fixture(
        rate: u64,
        reserve: u64,
    ) -> (
        tempfile::TempDir,
        Journal,
        LndPurchaseWallet,
        Arc<Mock>,
        tokio::task::JoinHandle<()>,
    ) {
        fixture_mode(rate, reserve, false).await
    }
    async fn fixture_mode(
        rate: u64,
        reserve: u64,
        offer_only: bool,
    ) -> (
        tempfile::TempDir,
        Journal,
        LndPurchaseWallet,
        Arc<Mock>,
        tokio::task::JoinHandle<()>,
    ) {
        let data = tempfile::tempdir().unwrap();
        let binding = Binding {
            id: uuid::Uuid::new_v4().to_string(),
            buyer_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([7; 32])).unwrap(),
            seller_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([8; 32])).unwrap(),
            content_id: "file".into(),
            price_sats: 546,
        };
        let journal = Journal::open(data.path(), &binding.id).await.unwrap();
        let record = Record::new(binding.clone(), format!("{}.onion", "a".repeat(56))).unwrap();
        let lock_id = record.lock_id.clone();
        journal.save(&record).unwrap();
        if offer_only {
            engine::accept_offer(
                &journal,
                crate::content_onchain_plan::Offer {
                    binding: binding.clone(),
                    network: engine::ChainNetwork::Regtest,
                    recipient_script_type: "p2wpkh".into(),
                    source: RetainedFile {
                        sha256: "a".repeat(64),
                        size: 4,
                        filename: "file".into(),
                        mime_type: "text/plain".into(),
                    },
                },
            )
            .unwrap();
        } else {
            engine::accept_quote(
                &journal,
                engine::Quote {
                    binding: binding.clone(),
                    address: address(1),
                    network: engine::ChainNetwork::Regtest,
                    source: RetainedFile {
                        sha256: "a".repeat(64),
                        size: 4,
                        filename: "file".into(),
                        mime_type: "text/plain".into(),
                    },
                },
            )
            .unwrap();
        }
        let state = Arc::new(Mock {
            calls: Mutex::new(vec![]),
            leases: Mutex::new(vec![]),
            previous: vec![previous(3, 1500), previous(4, 2500), previous(5, 10000)],
            published: Mutex::new(None),
            lose_change: AtomicBool::new(false),
            lose_lease: AtomicBool::new(false),
            lose_sign: AtomicBool::new(false),
            lose_publish: AtomicBool::new(false),
            rate,
            reserve,
            record_path: data
                .path()
                .join("content-onchain")
                .join(format!("{}.json", binding.id)),
            locked_id: lock_id,
        });
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let factory = state.clone();
        let service = make_service_fn(move |_| {
            let state = factory.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |request: hyper::Request<Body>| {
                    let state = state.clone();
                    async move {
                        let path = request.uri().path().to_string();
                        let query = request.uri().query().unwrap_or("").to_string();
                        let bytes = hyper::body::to_bytes(request.into_body()).await.unwrap();
                        let body: Value = if bytes.is_empty() {
                            json!({})
                        } else {
                            serde_json::from_slice(&bytes).unwrap()
                        };
                        state
                            .calls
                            .lock()
                            .unwrap()
                            .push((path.clone(), body.clone()));
                        let mut status = StatusCode::OK;
                        let mut lost = false;
                        let value = match path.as_str() {
                            "/v1/getinfo" => {
                                json!({"synced_to_chain":true,"chains":[{"chain":"bitcoin","network":"regtest"}]})
                            }
                            "/v2/wallet/accounts" => {
                                json!({"accounts":[{"name":"default","watch_only":false}]})
                            }
                            "/v2/wallet/address/next" => {
                                let saved = state.saved();
                                assert_eq!(
                                    saved.change_address,
                                    Some(engine::ChangeAddress::Dispatched)
                                );
                                assert_eq!(body["change"], true);
                                lost = state.lose_change.swap(false, Ordering::SeqCst);
                                json!({"addr":address(2)})
                            }
                            "/v2/wallet/addresses" => {
                                json!({"account_with_addresses":[{"name":"default","addresses":[{"address":address(2),"is_internal":true}]}]})
                            }
                            "/v2/wallet/estimatefee/1" => {
                                json!({"sat_per_kw":(state.rate*250).to_string()})
                            }
                            "/v2/wallet/reserve" => {
                                json!({"required_reserve":state.reserve.to_string()})
                            }
                            "/v2/wallet/utxos" => {
                                json!({"utxos":state.previous.iter().map(|tx|json!({"outpoint":{"txid_str":tx.compute_txid().to_string(),"output_index":0},
                        "confirmations":6,"amount_sat":tx.output[0].value.to_sat().to_string(),"pk_script":hex::encode(tx.output[0].script_pubkey.as_bytes())})).collect::<Vec<_>>()})
                            }
                            "/v2/wallet/utxos/leases" => {
                                json!({"locked_utxos":state.leases.lock().unwrap().iter().map(lease_value).collect::<Vec<_>>()})
                            }
                            "/v2/wallet/utxos/lease" => {
                                let saved = state.saved();
                                assert!(matches!(
                                    saved.phase,
                                    engine::Phase::LeaseDispatched
                                        | engine::Phase::PlanLeaseDispatched
                                        | engine::Phase::InputsLeased
                                        | engine::Phase::AddressAllocationDispatched
                                        | engine::Phase::Funded
                                        | engine::Phase::SigningDispatched
                                ));
                                assert!(saved.template.is_some() || saved.plan.is_some());
                                let id = hex::encode(
                                    base64::engine::general_purpose::STANDARD
                                        .decode(body["id"].as_str().unwrap())
                                        .unwrap(),
                                );
                                assert_eq!(id, state.locked_id);
                                let point = parse_outpoint(&body["outpoint"]).unwrap();
                                let planned: Vec<Lease> = saved
                                    .template
                                    .as_ref()
                                    .map(|t| t.leases.clone())
                                    .unwrap_or_else(|| {
                                        saved
                                            .plan
                                            .as_ref()
                                            .unwrap()
                                            .inputs
                                            .iter()
                                            .map(|i| i.lease.clone())
                                            .collect()
                                    });
                                assert!(planned.iter().any(|l| l.outpoint().unwrap() == point));
                                let tx = state
                                    .previous
                                    .iter()
                                    .find(|t| t.compute_txid() == point.txid)
                                    .unwrap();
                                let mut leases = state.leases.lock().unwrap();
                                if leases
                                    .iter()
                                    .any(|l| l.outpoint().unwrap() == point && l.lock_id != id)
                                {
                                    status = StatusCode::CONFLICT;
                                    json!({"error":"already locked"})
                                } else {
                                    leases.retain(|l| l.outpoint().unwrap() != point);
                                    let expiration = chrono::Utc::now().timestamp() as u64 + 600;
                                    leases.push(Lease {
                                        lock_id: id,
                                        txid: point.txid.to_string(),
                                        vout: point.vout,
                                        value_sats: tx.output[0].value.to_sat(),
                                        script: hex::encode(tx.output[0].script_pubkey.as_bytes()),
                                        expires_at: expiration,
                                    });
                                    lost = state.lose_lease.swap(false, Ordering::SeqCst);
                                    json!({"expiration":expiration.to_string()})
                                }
                            }
                            "/v2/wallet/psbt/finalize" => {
                                let saved = state.saved();
                                assert_eq!(saved.phase, engine::Phase::SigningDispatched);
                                assert_eq!(
                                    body["funded_psbt"].as_str(),
                                    Some(saved.funded.as_ref().unwrap().psbt_base64.as_str())
                                );
                                let mut tx = engine::decode_psbt(saved.funded.as_ref().unwrap())
                                    .unwrap()
                                    .unsigned_tx;
                                for input in &mut tx.input {
                                    input.witness =
                                        Witness::from_slice(&[vec![1; 72], vec![2; 33]]);
                                }
                                lost = state.lose_sign.swap(false, Ordering::SeqCst);
                                json!({"raw_final_tx":base64::engine::general_purpose::STANDARD.encode(consensus::serialize(&tx))})
                            }
                            "/v2/wallet/tx" if body.get("tx_hex").is_some() => {
                                let saved = state.saved();
                                assert!(matches!(
                                    saved.phase,
                                    engine::Phase::BroadcastDispatched | engine::Phase::Published
                                ));
                                let raw = base64::engine::general_purpose::STANDARD
                                    .decode(body["tx_hex"].as_str().unwrap())
                                    .unwrap();
                                assert_eq!(
                                    hex::encode(&raw),
                                    saved.signed.as_ref().unwrap().raw_hex
                                );
                                *state.published.lock().unwrap() =
                                    Some(consensus::deserialize(&raw).unwrap());
                                lost = state.lose_publish.swap(false, Ordering::SeqCst);
                                json!({})
                            }
                            "/v2/wallet/tx" => {
                                let requested = query.strip_prefix("txid=").unwrap();
                                let tx = state
                                    .previous
                                    .iter()
                                    .find(|t| t.compute_txid().to_string() == requested)
                                    .cloned()
                                    .or_else(|| {
                                        state
                                            .published
                                            .lock()
                                            .unwrap()
                                            .as_ref()
                                            .filter(|t| t.compute_txid().to_string() == requested)
                                            .cloned()
                                    });
                                if let Some(tx) = tx {
                                    json!({"tx_hash":tx.compute_txid().to_string(),"raw_tx_hex":hex::encode(consensus::serialize(&tx))})
                                } else {
                                    status = StatusCode::NOT_FOUND;
                                    json!({"error":"not found"})
                                }
                            }
                            _ => panic!("Unexpected wallet route {path}"),
                        };
                        let response_body = if lost {
                            Body::wrap_stream(futures_util::stream::once(async {
                                Err::<hyper::body::Bytes, _>(std::io::Error::new(
                                    std::io::ErrorKind::UnexpectedEof,
                                    "lost reply after mutation",
                                ))
                            }))
                        } else {
                            Body::from(value.to_string())
                        };
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(status)
                                .body(response_body)
                                .unwrap(),
                        )
                    }
                }))
            }
        });
        let server = Server::from_tcp(listener).unwrap().serve(service);
        let task = tokio::spawn(async move {
            server.await.unwrap();
        });
        let wallet = LndPurchaseWallet::new(
            reqwest::Client::new(),
            format!("http://{addr}"),
            "test".into(),
        );
        (data, journal, wallet, state, task)
    }
    fn request() -> PlanRequest {
        PlanRequest {
            change_address: address(2),
            max_fee_sats: 1000,
            sat_per_vbyte: None,
        }
    }
    #[tokio::test]
    async fn exact_template_is_saved_before_lease_and_all_lost_replies_recover_same_transaction() {
        let (data, j, wallet, state, server) = fixture(2, 2000).await;
        let planned = wallet.prepare_exact(&j, request()).await.unwrap();
        let original = planned.template.clone().unwrap();
        let id = planned.binding.id.clone();
        assert_eq!(planned.phase, engine::Phase::TemplatePrepared);
        assert_eq!(state.count("/v2/wallet/utxos/lease"), 0);
        assert_eq!(state.count("/v2/wallet/estimatefee/1"), 1);
        state.lose_lease.store(true, Ordering::SeqCst);
        assert!(engine::drive(&j, &wallet, engine::Action::Lease, None)
            .await
            .is_err());
        drop(j);
        let j = Journal::open(data.path(), &id).await.unwrap();
        assert_eq!(j.load().unwrap().unwrap().template, Some(original.clone()));
        let leased = engine::drive(&j, &wallet, engine::Action::Lease, None)
            .await
            .unwrap();
        assert_eq!(leased.phase, engine::Phase::Funded);
        assert_eq!(state.count("/v2/wallet/utxos/lease"), 1);
        state.lose_sign.store(true, Ordering::SeqCst);
        assert!(engine::drive(&j, &wallet, engine::Action::Sign, None)
            .await
            .is_err());
        drop(j);
        let j = Journal::open(data.path(), &id).await.unwrap();
        let signed = engine::drive(&j, &wallet, engine::Action::Sign, None)
            .await
            .unwrap();
        assert_eq!(
            signed.funded.as_ref().unwrap().psbt_base64,
            original.psbt_base64
        );
        {
            let calls = state.calls.lock().unwrap();
            let signs: Vec<_> = calls
                .iter()
                .filter(|(p, _)| p == "/v2/wallet/psbt/finalize")
                .collect();
            assert_eq!(signs.len(), 2);
            assert_eq!(signs[0].1, signs[1].1);
        }
        state.lose_publish.store(true, Ordering::SeqCst);
        assert!(engine::drive(&j, &wallet, engine::Action::Publish, None)
            .await
            .is_err());
        drop(j);
        let j = Journal::open(data.path(), &id).await.unwrap();
        let recovered = engine::drive(&j, &wallet, engine::Action::Status, None)
            .await
            .unwrap();
        assert_eq!(recovered.phase, engine::Phase::Published);
        assert_eq!(recovered.signed, signed.signed);
        engine::drive(&j, &wallet, engine::Action::Publish, None)
            .await
            .unwrap();
        assert_eq!(
            state
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(p, b)| p == "/v2/wallet/tx" && b.get("tx_hex").is_some())
                .count(),
            1
        );
        assert_eq!(state.count("/v2/wallet/psbt/fund"), 0);
        server.abort();
    }
    #[tokio::test]
    async fn fee_cap_and_unknown_change_fail_before_any_lease_or_signing() {
        let (_data, j, wallet, state, server) = fixture(2, 2000).await;
        let mut too_low = request();
        too_low.max_fee_sats = 1;
        assert!(wallet.prepare_exact(&j, too_low).await.is_err());
        assert_eq!(j.load().unwrap().unwrap().phase, engine::Phase::Quoted);
        let mut foreign = request();
        foreign.change_address = address(9);
        assert!(wallet.prepare_exact(&j, foreign).await.is_err());
        assert_eq!(state.count("/v2/wallet/utxos/lease"), 0);
        assert_eq!(state.count("/v2/wallet/psbt/finalize"), 0);
        server.abort();
    }
    #[tokio::test]
    async fn leased_outputs_and_channel_reserve_are_excluded_and_explicit_rate_wins() {
        let (_data, j, wallet, state, server) = fixture(5000, 2000).await;
        let first = &state.previous[0];
        state.leases.lock().unwrap().push(Lease {
            lock_id: "f".repeat(64),
            txid: first.compute_txid().to_string(),
            vout: 0,
            value_sats: 1500,
            script: hex::encode(script(3).as_bytes()),
            expires_at: 4_000_000_000,
        });
        let mut explicit = request();
        explicit.sat_per_vbyte = Some(1);
        let prepared = wallet.prepare_exact(&j, explicit).await.unwrap();
        let leases = &prepared.template.as_ref().unwrap().leases;
        assert_eq!(leases.len(), 1);
        assert_eq!(leases[0].value_sats, 2500);
        assert_eq!(state.count("/v2/wallet/estimatefee/1"), 0);
        assert_eq!(state.count("/v2/wallet/utxos/lease"), 0);
        server.abort();
    }
    #[tokio::test]
    async fn channel_reserve_and_foreign_lease_races_cannot_silently_choose_new_inputs() {
        let (_data, j, wallet, state, server) = fixture(2, 14000).await;
        assert!(wallet.prepare_exact(&j, request()).await.is_err());
        assert_eq!(state.count("/v2/wallet/utxos/lease"), 0);
        server.abort();
        let (_data, j, wallet, state, server) = fixture(2, 2000).await;
        let prepared = wallet.prepare_exact(&j, request()).await.unwrap();
        let mut taken = prepared.template.as_ref().unwrap().leases[0].clone();
        taken.lock_id = "f".repeat(64);
        taken.expires_at = 4_000_000_000;
        state.leases.lock().unwrap().push(taken);
        assert!(engine::drive(&j, &wallet, engine::Action::Lease, None)
            .await
            .is_err());
        assert_eq!(state.count("/v2/wallet/utxos/lease"), 0);
        assert_eq!(state.count("/v2/wallet/psbt/finalize"), 0);
        assert_eq!(j.load().unwrap().unwrap().template, prepared.template);
        server.abort();
    }
    #[tokio::test]
    async fn change_allocation_lost_reply_is_durable_and_never_repeated() {
        let (data, j, wallet, state, server) = fixture(2, 2000).await;
        let id = j.load().unwrap().unwrap().binding.id;
        state.lose_change.store(true, Ordering::SeqCst);
        assert!(wallet.prepare_change(&j).await.is_err());
        drop(j);
        let j = Journal::open(data.path(), &id).await.unwrap();
        assert_eq!(
            j.load().unwrap().unwrap().change_address,
            Some(engine::ChangeAddress::Dispatched)
        );
        assert!(wallet
            .prepare_change(&j)
            .await
            .unwrap_err()
            .to_string()
            .contains("ambiguous"));
        assert!(wallet.prepare_exact(&j, request()).await.is_err());
        assert_eq!(state.count("/v2/wallet/address/next"), 1);
        assert_eq!(state.count("/v2/wallet/utxos/lease"), 0);
        server.abort();
    }
    #[tokio::test]
    async fn original_change_survives_reload_and_stale_record_cannot_replace_it() {
        let (data, j, wallet, state, server) = fixture(2, 2000).await;
        let stale = j.load().unwrap().unwrap();
        assert_eq!(wallet.prepare_change(&j).await.unwrap(), address(2));
        assert!(j.save(&stale).is_err());
        drop(j);
        let j = Journal::open(data.path(), &stale.binding.id).await.unwrap();
        assert_eq!(wallet.prepare_change(&j).await.unwrap(), address(2));
        assert_eq!(state.count("/v2/wallet/address/next"), 1);
        wallet.prepare_exact(&j, request()).await.unwrap();
        server.abort();
    }
    #[tokio::test]
    async fn funded_lease_renewal_lost_reply_recovers_before_signing_same_psbt() {
        let (data, j, wallet, state, server) = fixture(2, 2000).await;
        wallet.prepare_change(&j).await.unwrap();
        wallet.prepare_exact(&j, request()).await.unwrap();
        let funded = engine::drive(&j, &wallet, engine::Action::Lease, None)
            .await
            .unwrap();
        for lease in state.leases.lock().unwrap().iter_mut() {
            lease.expires_at = 1;
        }
        state.lose_lease.store(true, Ordering::SeqCst);
        assert!(engine::drive(&j, &wallet, engine::Action::Sign, None)
            .await
            .is_err());
        assert_eq!(j.load().unwrap().unwrap().phase, engine::Phase::Funded);
        assert_eq!(state.count("/v2/wallet/psbt/finalize"), 0);
        drop(j);
        let j = Journal::open(data.path(), &funded.binding.id)
            .await
            .unwrap();
        let signed = engine::drive(&j, &wallet, engine::Action::Sign, None)
            .await
            .unwrap();
        assert_eq!(signed.funded, funded.funded);
        assert_eq!(state.count("/v2/wallet/utxos/lease"), 2);
        assert_eq!(state.count("/v2/wallet/psbt/finalize"), 1);
        assert_eq!(state.count("/v2/wallet/address/next"), 1);
        server.abort();
    }
    #[tokio::test]
    async fn stolen_expired_input_blocks_original_signing_without_reselection() {
        let (_data, j, wallet, state, server) = fixture(2, 2000).await;
        wallet.prepare_exact(&j, request()).await.unwrap();
        let funded = engine::drive(&j, &wallet, engine::Action::Lease, None)
            .await
            .unwrap();
        for lease in state.leases.lock().unwrap().iter_mut() {
            lease.lock_id = "f".repeat(64);
        }
        assert!(engine::drive(&j, &wallet, engine::Action::Sign, None)
            .await
            .is_err());
        let retained = j.load().unwrap().unwrap();
        assert_eq!(retained.phase, engine::Phase::Funded);
        assert_eq!(retained.funded, funded.funded);
        assert_eq!(state.count("/v2/wallet/psbt/finalize"), 0);
        assert_eq!(state.count("/v2/wallet/estimatefee/1"), 1);
        server.abort();
    }
    #[tokio::test]
    async fn offer_review_is_not_signable_and_fee_failure_can_retire_without_seller_allocation() {
        let (_data, j, wallet, state, server) = fixture_mode(2, 0, true).await;
        let change = wallet.prepare_change(&j).await.unwrap();
        let mut low = request();
        low.change_address = change.clone();
        low.max_fee_sats = 1;
        assert!(wallet.prepare_plan(&j, low).await.is_err());
        let before = j.load().unwrap().unwrap();
        assert!(before.can_retire_unallocated());
        assert!(before.quote.is_none());
        assert!(before.template.is_none());
        assert_eq!(state.count("/v2/wallet/utxos/lease"), 0);
        assert_eq!(state.count("/v2/wallet/psbt/finalize"), 0);
        let mut req = request();
        req.change_address = change;
        let planned = wallet.prepare_plan(&j, req).await.unwrap();
        assert_eq!(planned.phase, engine::Phase::PlanPrepared);
        assert!(planned.template.is_none());
        assert!(planned.quote.is_none());
        assert!(planned.can_retire_unallocated());
        let ack = crate::content_onchain_seller::UnallocatedAck {
            binding: planned.binding.clone(),
            state: "cancelled_unallocated".into(),
            address: Value::Null,
            allocation_dispatched: false,
            can_switch_method: true,
        };
        let retired = engine::retire_unallocated(&j, ack).unwrap();
        assert!(retired.retirement.is_some());
        assert!(engine::lease_plan(&j, &wallet).await.is_err());
        server.abort();
    }
    #[tokio::test]
    async fn plan_leases_recover_before_real_recipient_binding_and_never_change_inputs() {
        let (data, j, wallet, state, server) = fixture_mode(2, 0, true).await;
        let mut req = request();
        req.change_address = wallet.prepare_change(&j).await.unwrap();
        let plan = wallet.prepare_plan(&j, req).await.unwrap();
        assert!(engine::mark_address_allocation(&j, false).is_err());
        state.lose_lease.store(true, Ordering::SeqCst);
        assert!(engine::lease_plan(&j, &wallet).await.is_err());
        let id = plan.binding.id.clone();
        drop(j);
        let j = Journal::open(data.path(), &id).await.unwrap();
        let leased = engine::lease_plan(&j, &wallet).await.unwrap();
        assert_eq!(leased.phase, engine::Phase::InputsLeased);
        assert_eq!(leased.plan, plan.plan);
        assert!(leased.quote.is_none());
        assert_eq!(state.count("/v2/wallet/utxos/lease"), 1);
        engine::mark_address_allocation(&j, false).unwrap();
        let offer = plan.offer.unwrap();
        engine::accept_quote(
            &j,
            engine::Quote {
                binding: offer.binding,
                address: address(1),
                network: offer.network,
                source: offer.source,
            },
        )
        .unwrap();
        let bound = engine::bind_plan(&j).unwrap();
        let tx = engine::decode_psbt(bound.template.as_ref().unwrap())
            .unwrap()
            .unsigned_tx;
        assert_eq!(tx.output[0].script_pubkey, script(1));
        assert_eq!(tx.output[0].value.to_sat(), 546);
        assert_eq!(bound.plan, leased.plan);
        engine::drive(&j, &wallet, engine::Action::Lease, None)
            .await
            .unwrap();
        assert_eq!(state.count("/v2/wallet/utxos/lease"), 1);
        assert_eq!(state.count("/v2/wallet/psbt/fund"), 0);
        server.abort();
    }
    #[tokio::test]
    async fn no_change_plan_does_not_require_funding_an_unused_change_output() {
        let (_data, j, wallet, state, server) = fixture_mode(8, 0, true).await;
        let mut req = request();
        req.change_address = wallet.prepare_change(&j).await.unwrap();
        let planned = wallet.prepare_plan(&j, req).await.unwrap();
        let plan = planned.plan.unwrap();
        assert_eq!(plan.inputs.len(), 1);
        assert_eq!(plan.change_sats, 0);
        assert_eq!(plan.fee_sats, 954);
        assert!(plan.fee_sats <= plan.max_fee_sats);
        assert_eq!(state.count("/v2/wallet/utxos/lease"), 0);
        server.abort();
    }
}
