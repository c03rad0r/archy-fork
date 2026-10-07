//! Thin HTTP bridge to the `fedimint-clientd` sidecar container.
//!
//! Keeps the heavy, fast-moving Fedimint client SDK OUT of this binary: the
//! `fedimint-clientd` daemon (in `apps/fedimint-clientd`) holds the federation
//! clients and ecash notes; we just speak its REST API (`/v2/*`, Bearer auth),
//! mirroring how [`super::mint_client::MintClient`] speaks the Cashu NUT API.
//!
//! See `docs/dual-ecash-design.md`. Endpoint/JSON shapes target fedimint-clientd
//! v0.3.x and must be pinned to the vendored image tag.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::debug;

const CLIENTD_TIMEOUT_SECS: u64 = 15;
const CLIENTD_HEAVY_TIMEOUT_SECS: u64 = 60;

/// Default host port the `fedimint-clientd` container is mapped to (its own
/// default 8080 collides with LND REST, so the manifest maps it to 8178).
const DEFAULT_CLIENTD_URL: &str = "http://127.0.0.1:8178";

/// Federation joined out-of-the-box on every node. The fmcd container also
/// auto-joins this at boot (`FMCD_INVITE_CODE` in the manifest); keep in sync.
///
/// The preferred default federation (guardian on .116, iroh transport).
/// Validated: fmcd 0.8.2 joins it (federation_id 2debd071…73b76884). iroh does
/// NAT traversal, so it's reachable fleet-wide — the right fleet default.
/// CAVEAT: iroh is experimental and the connection can be flaky (esp. NAT
/// hairpin when fmcd runs on .116 itself reaching .116's own WAN IP); validate
/// reliability from a separate node. ensure_default_federation is best-effort.
/// See docs/dual-ecash-design.md.
pub const DEFAULT_FEDERATION_INVITE: &str = "fed11qgqyj3mfwfhksw309uuxywtxxfjrjc35xuexverpxdsnxcnrxucxvenzveskgc3kvvun2c34xp3k2ep38yunzdpexcekxe3hvd3rvvmx8pnrvdenx5mnzvtzqqqjqt0t6pc3s5z0ynqjw9s4njf6svwgu59kweawc0vvrddcjeemw6yyn4pcdp";

/// One joined federation, persisted locally so the list survives clientd being
/// temporarily down. Balances are always read live from clientd.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinedFederation {
    pub federation_id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FederationRegistry {
    pub federations: Vec<JoinedFederation>,
}

const REGISTRY_FILE: &str = "wallet/fedimint_federations.json";

/// Shared HTTP-Basic password between the fmcd container and this bridge. The
/// fedimint-clientd manifest generates it via `generated_secrets: [fmcd-password]`
/// and injects it through `secret_env`; the bridge reads the same file in
/// `from_node`. (Generation lives in `container::secrets`, not here — it's a
/// generic, manifest-declared concern, not fedimint-specific.)
const FMCD_PASSWORD_SECRET: &str = "fmcd-password";

pub async fn load_registry(data_dir: &Path) -> Result<FederationRegistry> {
    let path = data_dir.join(REGISTRY_FILE);
    if !path.exists() {
        return Ok(FederationRegistry::default());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read fedimint federation registry")?;
    Ok(serde_json::from_str(&content).unwrap_or_default())
}

pub async fn save_registry(data_dir: &Path, reg: &FederationRegistry) -> Result<()> {
    let dir = data_dir.join("wallet");
    fs::create_dir_all(&dir)
        .await
        .context("Failed to create wallet dir")?;
    let content = serde_json::to_string_pretty(reg).context("Failed to serialize registry")?;
    fs::write(data_dir.join(REGISTRY_FILE), content)
        .await
        .context("Failed to write fedimint federation registry")?;
    Ok(())
}

/// Local Fedimint transaction log. fmcd has no per-node history API, so we
/// record each redeem/spend ourselves and merge it with the Cashu history in
/// `wallet.ecash-history` — otherwise a Fedimint receive shows nowhere.
const FEDIMINT_TX_FILE: &str = "wallet/fedimint_transactions.json";

/// Load the local Fedimint transaction log (newest entries last). Empty on any
/// error — history is best-effort and must never block a wallet operation.
pub async fn load_fedimint_txs(data_dir: &Path) -> Vec<crate::wallet::ecash::EcashTransaction> {
    match fs::read_to_string(data_dir.join(FEDIMINT_TX_FILE)).await {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Append a Fedimint transaction to the local log so it appears in unified
/// history with meaningful data. Best-effort: write failures are logged, not
/// propagated, so they never fail the redeem/spend that produced the funds.
pub async fn record_fedimint_tx(
    data_dir: &Path,
    tx_type: crate::wallet::ecash::TransactionType,
    amount_sats: u64,
    federation_id: &str,
    description: &str,
) {
    let mut txs = load_fedimint_txs(data_dir).await;
    txs.push(crate::wallet::ecash::EcashTransaction {
        id: uuid::Uuid::new_v4().to_string(),
        tx_type,
        amount_sats,
        timestamp: chrono::Utc::now().to_rfc3339(),
        description: description.to_string(),
        // Cashu uses mint_url; for Fedimint we record the federation id in `peer`
        // so the UI can show which federation the funds moved through.
        mint_url: String::new(),
        peer: federation_id.to_string(),
        kind: "fedimint".to_string(),
    });
    // Cap the log so it can't grow unbounded.
    let len = txs.len();
    if len > 500 {
        txs.drain(0..len - 500);
    }
    if let Err(e) = fs::create_dir_all(data_dir.join("wallet")).await {
        tracing::warn!("fedimint tx log: could not create wallet dir: {e}");
        return;
    }
    match serde_json::to_string_pretty(&txs) {
        Ok(content) => {
            if let Err(e) = fs::write(data_dir.join(FEDIMINT_TX_FILE), content).await {
                tracing::warn!("fedimint tx log: write failed: {e}");
            }
        }
        Err(e) => tracing::warn!("fedimint tx log: serialize failed: {e}"),
    }
}

/// Idempotently ensure the node has joined the default federation and that it
/// is tracked in the local registry. Best-effort: silently no-ops if clientd
/// isn't installed/running yet. Joining is idempotent on the clientd side.
pub async fn ensure_default_federation(data_dir: &Path) -> Result<()> {
    let client = match FedimintClient::from_node(data_dir).await {
        Ok(c) => c,
        Err(_) => return Ok(()), // clientd not configured yet
    };

    // Fast path: if fmcd already reports a joined federation, do NOT re-issue the
    // POST /v2/admin/join. That call re-syncs federation config against the
    // guardians and adds seconds of latency — and ensure_default_federation runs
    // on every wallet.fedimint-list / spend / reissue, so the join was being paid
    // on each balance refresh (the "mints take ages to load" report). The cheap
    // GET /v2/admin/info is enough to confirm membership; just reconcile the local
    // registry against the live joined set and return.
    let joined = client.joined_federation_ids().await;
    if !joined.is_empty() {
        let mut reg = load_registry(data_dir).await?;
        let mut changed = false;
        for id in joined {
            if !reg.federations.iter().any(|f| f.federation_id == id) {
                reg.federations.push(JoinedFederation {
                    federation_id: id,
                    name: Some("Archipelago Federation".to_string()),
                });
                changed = true;
            }
        }
        if changed {
            save_registry(data_dir, &reg).await?;
        }
        return Ok(());
    }

    // Cold start only: nothing joined yet, so join the default federation once.
    let federation_id = match client.join(DEFAULT_FEDERATION_INVITE).await {
        Ok(id) => id,
        Err(e) => {
            debug!("default federation autojoin skipped: {e}");
            return Ok(());
        }
    };
    let mut reg = load_registry(data_dir).await?;
    if !reg
        .federations
        .iter()
        .any(|f| f.federation_id == federation_id)
    {
        reg.federations.push(JoinedFederation {
            federation_id,
            name: Some("Archipelago Federation".to_string()),
        });
        save_registry(data_dir, &reg).await?;
    }
    Ok(())
}

/// Spend `amount_sats` of Fedimint ecash from whichever joined federation can
/// cover it, returning the serialized notes (the X-Payment-Token a buyer hands
/// the seller) and the federation id that minted them. Federations are tried in
/// registry order (default first); only one with sufficient balance is used so
/// the resulting notes redeem cleanly on the other side. Errors clearly when no
/// federation is joined or none has the balance — the caller falls back to (or
/// from) the Cashu path.
pub async fn spend_from_any(data_dir: &Path, amount_sats: u64) -> Result<(String, String)> {
    if amount_sats == 0 {
        anyhow::bail!("payment amount must be greater than zero");
    }
    let _ = ensure_default_federation(data_dir).await;
    let client = FedimintClient::from_node(data_dir).await?;

    // Same union-of-sources approach as reissue_into_any: the persisted registry
    // and what fmcd actually reports joined can drift, so consider both.
    let mut fed_ids: Vec<String> = Vec::new();
    if let Ok(reg) = load_registry(data_dir).await {
        for f in reg.federations {
            if !fed_ids.contains(&f.federation_id) {
                fed_ids.push(f.federation_id);
            }
        }
    }
    for id in client.joined_federation_ids().await {
        if !fed_ids.contains(&id) {
            fed_ids.push(id);
        }
    }
    if fed_ids.is_empty() {
        anyhow::bail!("No Fedimint federation joined to spend from");
    }

    let (notes, federation) = spend_from_candidates(&client, &fed_ids, amount_sats).await?;
    record_fedimint_tx(
        data_dir,
        crate::wallet::ecash::TransactionType::Send,
        amount_sats,
        &federation,
        "Sent Fedimint ecash",
    )
    .await;
    Ok((notes, federation))
}

/// Balance lookup may try another federation; a dispatched spend never may.
/// A sidecar error (including missing notes or a lost response) can follow a
/// successful debit. Without its original operation receipt it is ambiguous.
async fn spend_from_candidates(
    client: &FedimintClient,
    fed_ids: &[String],
    amount_sats: u64,
) -> Result<(String, String)> {
    let mut last_err = None;
    for fed_id in fed_ids {
        match client.federation_balance_sats(fed_id).await {
            Ok(bal) if bal >= amount_sats => {}
            Ok(_) => continue,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        }
        let notes = client.spend(fed_id, amount_sats).await.context(
            "The selected federation did not confirm the spend; no other federation was charged. Recover its original operation before retrying"
        )?;
        return Ok((notes, fed_id.clone()));
    }
    Err(last_err
        .map(|e| anyhow::anyhow!("Could not check federation balances: {e}"))
        .unwrap_or_else(|| {
            anyhow::anyhow!("No joined Fedimint federation has {amount_sats} sats available")
        }))
}

/// Redeem received Fedimint notes into a joined federation. fmcd's reissue is
/// per-federation, but a token only validates against the federation that
/// minted it, so we try each joined federation (default first) and return the
/// first that accepts the notes, along with its id. Errors clearly when the
/// fmcd sidecar isn't installed or no federation is joined — the Cashu path is
/// handled separately by the caller.
pub async fn reissue_into_any(data_dir: &Path, notes: &str) -> Result<(u64, String)> {
    // Make sure at least the default federation is tracked before we try.
    let _ = ensure_default_federation(data_dir).await;

    let client = FedimintClient::from_node(data_dir).await?;

    // Build the set of federations to try: the locally-persisted registry PLUS
    // every federation the fmcd sidecar actually reports joined. The two can
    // drift (a federation joined directly, before tracking, or a registry that
    // wasn't written), and a note only validates against the federation that
    // minted it — so we must try EVERY connected federation before giving up,
    // or a perfectly valid token is wrongly reported as failed.
    let mut fed_ids: Vec<String> = Vec::new();
    if let Ok(reg) = load_registry(data_dir).await {
        for f in reg.federations {
            if !fed_ids.contains(&f.federation_id) {
                fed_ids.push(f.federation_id);
            }
        }
    }
    for id in client.joined_federation_ids().await {
        if !fed_ids.contains(&id) {
            fed_ids.push(id);
        }
    }
    if fed_ids.is_empty() {
        anyhow::bail!("No Fedimint federation joined to redeem these notes into");
    }

    let mut already_redeemed = false;
    let mut last_err = None;
    for fed_id in &fed_ids {
        match client.reissue(fed_id, notes).await {
            Ok(sats) => {
                // Record the receive so it appears in unified ecash history.
                record_fedimint_tx(
                    data_dir,
                    crate::wallet::ecash::TransactionType::Receive,
                    sats,
                    fed_id,
                    "Received Fedimint ecash",
                )
                .await;
                return Ok((sats, fed_id.clone()));
            }
            Err(e) => {
                let msg = e.to_string().to_ascii_lowercase();
                // fmcd reports already-claimed notes as "We already reissued
                // these notes" (or "already spent"). That means the funds were
                // already redeemed INTO this node's wallet — they're safe, just
                // not new — so surface that clearly instead of a raw 500.
                if msg.contains("already reissued") || msg.contains("already spent") {
                    already_redeemed = true;
                }
                last_err = Some(e);
            }
        }
    }

    if already_redeemed {
        anyhow::bail!(
            "This ecash was already redeemed into your wallet — the notes are \
             already claimed, so no new balance was added. Check your Fedimint balance."
        );
    }
    Err(last_err
        .map(|e| {
            anyhow::anyhow!(
                "These notes didn't match any of your {} connected Fedimint \
                 federation(s). You may need to join the federation that issued \
                 them first. (last error: {e})",
                fed_ids.len()
            )
        })
        .unwrap_or_else(|| anyhow::anyhow!("Fedimint reissue failed")))
}

/// HTTP client for a `fedimint-clientd` instance.
pub struct FedimintClient {
    base_url: String,
    password: String,
    client: reqwest::Client,
}

impl FedimintClient {
    pub fn new(base_url: &str, password: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(CLIENTD_HEAVY_TIMEOUT_SECS))
            .build()
            .context("Failed to build HTTP client for fedimint-clientd")?;
        Ok(Self::with_client(base_url, password, client))
    }

    pub fn with_client(base_url: &str, password: &str, client: reqwest::Client) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            password: password.to_string(),
            client,
        }
    }

    /// Resolve URL + password from env / node secret, with sane defaults.
    /// URL: `FEDIMINT_CLIENTD_URL` else the default mapped port.
    /// Password: `FEDIMINT_CLIENTD_PASSWORD` else `<data_dir>/fedimint-clientd/password`.
    pub async fn from_node(data_dir: &Path) -> Result<Self> {
        let base_url =
            std::env::var("FMCD_URL").unwrap_or_else(|_| DEFAULT_CLIENTD_URL.to_string());
        let password = match std::env::var("FMCD_PASSWORD") {
            Ok(p) if !p.is_empty() => p,
            _ => {
                // The shared secret the fmcd container also reads (manifest
                // secret_env: fmcd-password, resolved from <data_dir>/secrets).
                // Legacy <data_dir>/fmcd/password kept as a fallback.
                let shared = data_dir.join("secrets").join(FMCD_PASSWORD_SECRET);
                let legacy = data_dir.join("fmcd").join("password");
                let mut found = None;
                for candidate in [shared, legacy] {
                    if let Ok(s) = fs::read_to_string(&candidate).await {
                        let s = s.trim().to_string();
                        if !s.is_empty() {
                            found = Some(s);
                            break;
                        }
                    }
                }
                found.context(
                    "Fedimint client not configured (no FMCD_PASSWORD and no \
                     fmcd-password secret). Install the Fedimint client app.",
                )?
            }
        };
        Self::new(&base_url, &password)
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        // fmcd uses HTTP Basic auth with a fixed username `fmcd`.
        req.basic_auth("fmcd", Some(&self.password))
    }

    async fn post(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .auth(self.client.post(&url))
            .json(&body)
            .send()
            .await
            .with_context(|| format!("fedimint-clientd POST {path} failed (is it running?)"))?;
        Self::parse(resp, path).await
    }

    async fn get(&self, path: &str) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .auth(self.client.get(&url))
            .timeout(std::time::Duration::from_secs(CLIENTD_TIMEOUT_SECS))
            .send()
            .await
            .with_context(|| format!("fedimint-clientd GET {path} failed (is it running?)"))?;
        Self::parse(resp, path).await
    }

    async fn parse(resp: reqwest::Response, path: &str) -> Result<serde_json::Value> {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("fedimint-clientd {path} returned {status}: {text}");
        }
        if text.is_empty() {
            return Ok(serde_json::json!({}));
        }
        serde_json::from_str(&text)
            .with_context(|| format!("fedimint-clientd {path} returned non-JSON: {text}"))
    }

    /// `GET /v2/admin/info` — per-federation holdings keyed by federationId.
    pub async fn info(&self) -> Result<serde_json::Value> {
        self.get("/v2/admin/info").await
    }

    /// Every federation id the fmcd sidecar currently reports joined, read from
    /// `/v2/admin/info` (the authoritative live set — the locally-persisted
    /// registry can drift from it). Returns an empty vec on any error so callers
    /// can fall back to the registry rather than fail outright.
    pub async fn joined_federation_ids(&self) -> Vec<String> {
        match self.info().await {
            Ok(info) => info
                .as_object()
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// `POST /v2/admin/join` — join a federation by invite code; returns its federationId.
    pub async fn join(&self, invite_code: &str) -> Result<String> {
        let res = self
            .post(
                "/v2/admin/join",
                serde_json::json!({ "inviteCode": invite_code, "useManualSecret": false }),
            )
            .await?;
        let id = res
            .get("thisFederationId")
            .or_else(|| res.get("federationId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        match id {
            Some(id) => {
                debug!("joined fedimint federation {id}");
                Ok(id)
            }
            // Older/newer clientd may return the full info map; fall back to info().
            None => self.latest_federation_id().await,
        }
    }

    /// Total balance across all joined federations, in sats.
    pub async fn total_balance_sats(&self) -> Result<u64> {
        let info = self.info().await?;
        Ok(sum_msat(&info) / 1000)
    }

    /// Balance of one federation in sats (0 if unknown).
    pub async fn federation_balance_sats(&self, federation_id: &str) -> Result<u64> {
        let info = self.info().await?;
        let msat = info
            .get(federation_id)
            .and_then(federation_msat)
            .unwrap_or(0);
        Ok(msat / 1000)
    }

    /// `POST /v2/mint/spend` — prepare notes to send (ecash), in msat. Returns serialized notes.
    pub async fn spend(&self, federation_id: &str, amount_sats: u64) -> Result<String> {
        let res = self
            .post(
                "/v2/mint/spend",
                serde_json::json!({
                    "federationId": federation_id,
                    "amountMsat": amount_sats * 1000,
                    "allowOverpay": true,
                    "timeout": 3600,
                    "includeInvite": false,
                }),
            )
            .await?;
        res.get("notes")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("fedimint spend: no notes in response"))
    }

    /// `POST /v2/mint/reissue` — redeem received notes; returns reissued sats.
    pub async fn reissue(&self, federation_id: &str, notes: &str) -> Result<u64> {
        let res = self
            .post(
                "/v2/mint/reissue",
                serde_json::json!({ "federationId": federation_id, "notes": notes }),
            )
            .await?;
        let msat = res
            .get("amountMsat")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("fedimint reissue: no amountMsat in response"))?;
        Ok(msat / 1000)
    }

    async fn latest_federation_id(&self) -> Result<String> {
        let info = self.info().await?;
        info.as_object()
            .and_then(|m| m.keys().next_back().cloned())
            .ok_or_else(|| anyhow::anyhow!("joined federation but clientd reported none"))
    }
}

fn federation_msat(entry: &serde_json::Value) -> Option<u64> {
    entry
        .get("totalAmountMsat")
        .or_else(|| entry.get("totalMsat"))
        .and_then(|v| v.as_u64())
}

fn sum_msat(info: &serde_json::Value) -> u64 {
    info.as_object()
        .map(|m| m.values().filter_map(federation_msat).sum())
        .unwrap_or(0)
}

#[cfg(test)]
mod payment_edge_tests {
    use super::*;
    use hyper::{
        service::{make_service_fn, service_fn},
        Body, Response, Server, StatusCode,
    };
    use std::{
        convert::Infallible,
        sync::{Arc, Mutex},
    };

    // The mock records a completed sidecar spend BEFORE producing each fault.
    // No environment override, installed sidecar, wallet or live service is used.
    async fn sidecar(
        first_balance: u64,
        reply: &'static str,
        status: StatusCode,
    ) -> (
        FedimintClient,
        Arc<Mutex<Vec<String>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let spent = Arc::new(Mutex::new(Vec::new()));
        let recorded = spent.clone();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let service = make_service_fn(move |_| {
            let spent = recorded.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |request: hyper::Request<Body>| {
                    let spent = spent.clone();
                    async move {
                        if request.uri().path() == "/v2/admin/info" {
                            return Ok::<_, Infallible>(Response::new(Body::from(
                                serde_json::json!({
                                    "first":{"totalAmountMsat":first_balance * 1000},
                                    "second":{"totalAmountMsat":100000}
                                })
                                .to_string(),
                            )));
                        }
                        assert_eq!(request.uri().path(), "/v2/mint/spend");
                        let body = hyper::body::to_bytes(request.into_body()).await.unwrap();
                        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                        spent
                            .lock()
                            .unwrap()
                            .push(body["federationId"].as_str().unwrap().to_owned());
                        let response_body = if reply == "disconnect" {
                            Body::wrap_stream(futures_util::stream::once(async {
                                Err::<hyper::body::Bytes, _>(std::io::Error::new(
                                    std::io::ErrorKind::UnexpectedEof,
                                    "lost response after debit",
                                ))
                            }))
                        } else {
                            Body::from(reply)
                        };
                        Ok(Response::builder()
                            .status(status)
                            .body(response_body)
                            .unwrap())
                    }
                }))
            }
        });
        let server = Server::from_tcp(listener).unwrap().serve(service);
        let task = tokio::spawn(async move {
            server.await.unwrap();
        });
        (
            FedimintClient::new(&format!("http://{address}"), "test").unwrap(),
            spent,
            task,
        )
    }

    #[tokio::test]
    async fn dispatched_fedimint_spend_never_falls_through_after_ambiguous_reply() {
        for (reply, status) in [
            ("disconnect", StatusCode::OK),
            ("not-json", StatusCode::OK),
            ("{}", StatusCode::OK),
            ("unavailable", StatusCode::SERVICE_UNAVAILABLE),
        ] {
            let (client, spent, task) = sidecar(100, reply, status).await;
            let result =
                spend_from_candidates(&client, &["first".into(), "second".into()], 50).await;
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("no other federation was charged"));
            assert_eq!(*spent.lock().unwrap(), vec!["first".to_string()]);
            task.abort();
        }
    }

    #[tokio::test]
    async fn fedimint_selection_skips_insufficient_balance_before_dispatch() {
        let (client, spent, task) =
            sidecar(1, r#"{"notes":"original-notes"}"#, StatusCode::OK).await;
        let result = spend_from_candidates(&client, &["first".into(), "second".into()], 50)
            .await
            .unwrap();
        assert_eq!(result, ("original-notes".into(), "second".into()));
        assert_eq!(*spent.lock().unwrap(), vec!["second".to_string()]);
        task.abort();
    }
}
