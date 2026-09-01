//! lnd config bootstrap helper.

use anyhow::{Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

use crate::update::host_sudo;

pub const DEFAULT_DATA_DIR: &str = "/var/lib/archipelago/lnd";
pub const DEFAULT_CONF_PATH: &str = "/var/lib/archipelago/lnd/lnd.conf";
const LND_REST_BASE_URL: &str = "https://127.0.0.1:18080";

/// Per-node LND wallet password file (random, 0600). Replaces the old
/// fleet-wide hardcoded constant: each node's wallet password is now unique,
/// high-entropy, and recorded here so the unattended boot path can auto-unlock.
const WALLET_PASSWORD_SECRET: &str = "/var/lib/archipelago/secrets/lnd-wallet-password";

/// Legacy fleet-wide wallet password (builds that hardcoded it). Kept ONLY as an
/// unlock fallback so wallets created by those builds still open; new wallets
/// never use it, and the login-path migration rotates away from it.
const LEGACY_WALLET_PASSWORD: &str = "hellohello";

/// How many one-second passes `unlock_existing_wallet_via_rest` will make while
/// LND's unlocker is still not listening (~10 minutes). See the comment at the
/// retry loop for why this is measured in minutes rather than seconds.
const UNLOCK_NOT_READY_ATTEMPTS: u32 = 600;

#[derive(Debug, Clone)]
pub struct EnsurePaths {
    pub data_dir: PathBuf,
    pub conf_path: PathBuf,
}

impl Default for EnsurePaths {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from(DEFAULT_DATA_DIR),
            conf_path: PathBuf::from(DEFAULT_CONF_PATH),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnsureOutcome {
    Written,
    Unchanged,
}

pub async fn ensure_config(
    paths: &EnsurePaths,
    rpc_pass: &str,
    bitcoin_host: &str,
) -> Result<EnsureOutcome> {
    fs::create_dir_all(&paths.data_dir)
        .await
        .with_context(|| format!("creating {}", paths.data_dir.display()))?;

    if paths.conf_path.exists() {
        let existing = fs::read_to_string(&paths.conf_path)
            .await
            .with_context(|| format!("reading {}", paths.conf_path.display()))?;
        if has_required_lnd_flags(&existing, rpc_pass, bitcoin_host) {
            return Ok(EnsureOutcome::Unchanged);
        }
    }

    let conf = format!(
        "debuglevel=info\n\
maxpendingchannels=10\n\
alias=Archipelago Node\n\
color=#f7931a\n\
listen=0.0.0.0:9735\n\
rpclisten=0.0.0.0:10009\n\
restlisten=0.0.0.0:8080\n\
bitcoin.active=true\n\
bitcoin.mainnet=true\n\
bitcoin.node=bitcoind\n\
bitcoind.rpchost={bitcoin_host}:8332\n\
bitcoind.rpcuser=archipelago\n\
bitcoind.rpcpass={rpc_pass}\n\
bitcoind.rpcpolling=true\n\
bitcoind.estimatemode=ECONOMICAL\n"
    );

    write_config_atomically(paths, &conf).await?;

    Ok(EnsureOutcome::Written)
}

pub async fn ensure_wallet_initialized() -> Result<()> {
    let admin_macaroon = "/var/lib/archipelago/lnd/data/chain/bitcoin/mainnet/admin.macaroon";
    let wallet_db = "/var/lib/archipelago/lnd/data/chain/bitcoin/mainnet/wallet.db";
    if file_exists_as_root(wallet_db).await {
        if file_exists_as_root(admin_macaroon).await && lnd_getinfo_ready(admin_macaroon).await {
            return Ok(());
        }
        match unlock_existing_wallet().await? {
            true => {
                wait_for_admin_macaroon(admin_macaroon).await?;
                return Ok(());
            }
            false => {
                // Every candidate password was actively rejected: this wallet was
                // created with a password this node no longer has, so it can never
                // auto-unlock unattended. Alpha nodes hold no real funds and a wallet
                // locked with an unknown password is already inaccessible, so wipe +
                // recreate it on the per-node secret to self-heal at boot.
                recreate_wallet_destructively().await?;
                wait_for_admin_macaroon(admin_macaroon).await?;
                return Ok(());
            }
        }
    }

    init_wallet_via_rest().await?;
    wait_for_admin_macaroon(admin_macaroon).await
}

/// LND data subdirectories holding wallet + channel + graph state. Removing them
/// returns LND to a NON_EXISTING wallet state. Funds-bearing data lives here too,
/// so deletion is destructive — only done once the wallet is already unrecoverable.
const LND_STATE_DIRS: &[&str] = &[
    "/var/lib/archipelago/lnd/data/chain",
    "/var/lib/archipelago/lnd/data/graph",
];

/// Podman container name for the core LND app (see `compute_container_name`:
/// non-UI core apps keep their bare id). LND runs as a plain bridge-network
/// container, not a Quadlet unit, so it is restarted via `podman`, not systemctl.
const LND_CONTAINER: &str = "lnd";

/// Canonical on-host admin macaroon — same path the RPC layer reads.
const LND_ADMIN_MACAROON: &str =
    "/var/lib/archipelago/lnd/data/chain/bitcoin/mainnet/admin.macaroon";

/// Archipelago data dir (default; not overridden in prod). Holds the
/// `user-stopped.json` that gates health-monitor auto-restart.
const ARCHY_DATA_DIR: &str = "/var/lib/archipelago";

/// Destroy an unrecoverable LND wallet and recreate a fresh one keyed to the
/// per-node secret. Suppresses health-monitor auto-restart for the wipe window,
/// stops LND, deletes its wallet/chain/graph state as root, restarts it, waits
/// for NON_EXISTING, then inits a fresh wallet. Destructive — only called when no
/// candidate password can open the existing wallet.
async fn recreate_wallet_destructively() -> Result<()> {
    tracing::warn!(
        "[lnd] wallet is locked with an unknown password and cannot auto-unlock; \
         wiping and recreating it on the per-node secret (DESTRUCTIVE)"
    );

    // The health monitor restarts any container it sees stopped; mark LND
    // user-stopped so it doesn't re-launch (and re-open the wallet) mid-wipe.
    // Always cleared below so LND auto-recovers normally afterwards.
    let data_dir = std::path::Path::new(ARCHY_DATA_DIR);
    crate::crash_recovery::mark_user_stopped(data_dir, LND_CONTAINER).await;
    let result = wipe_and_reinit_wallet().await;
    crate::crash_recovery::clear_user_stopped(data_dir, LND_CONTAINER).await;
    result
}

async fn wipe_and_reinit_wallet() -> Result<()> {
    podman_user_scoped(&["stop", LND_CONTAINER])
        .await
        .context("stopping lnd before wallet wipe")?;

    for dir in LND_STATE_DIRS {
        let status = host_sudo(&["rm", "-rf", dir])
            .await
            .with_context(|| format!("removing {dir}"))?;
        if !status.success() {
            anyhow::bail!("removing {dir} exited with {status}");
        }
    }

    podman_user_scoped(&["start", LND_CONTAINER])
        .await
        .context("restarting lnd after wallet wipe")?;

    wait_for_wallet_state("NON_EXISTING").await?;
    init_wallet_via_rest().await
}

/// Run `podman <args>` inside a transient `systemd-run --user --scope`, matching
/// how the orchestrator/health-monitor manage rootless containers (keeps the
/// container out of the archipelago service's cgroup).
async fn podman_user_scoped(args: &[&str]) -> Result<()> {
    let out = tokio::process::Command::new("systemd-run")
        .args(["--user", "--scope", "--quiet", "--collect", "podman"])
        .args(args)
        .output()
        .await
        .with_context(|| format!("systemd-run --user --scope podman {}", args.join(" ")))?;
    if !out.status.success() {
        anyhow::bail!(
            "podman {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Poll `/v1/state` until LND reports `target`, or time out after ~120s.
async fn wait_for_wallet_state(target: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .build()
        .context("building LND REST client")?;
    for _ in 0..120 {
        if wallet_state(&client).await.as_deref() == Some(target) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    anyhow::bail!("LND did not reach state {target} after wallet wipe")
}

async fn file_exists_as_root(path: &str) -> bool {
    if std::path::Path::new(path).exists() {
        return true;
    }
    tokio::process::Command::new("sudo")
        .args(["test", "-f", path])
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn read_file_as_root(path: &str) -> Result<Vec<u8>> {
    match fs::read(path).await {
        Ok(bytes) => Ok(bytes),
        Err(direct_err) => {
            let out = tokio::process::Command::new("sudo")
                .args(["cat", path])
                .output()
                .await
                .with_context(|| format!("reading {path} via sudo"))?;
            if out.status.success() {
                Ok(out.stdout)
            } else {
                anyhow::bail!(
                    "reading {path} failed (direct: {direct_err}; sudo: {})",
                    String::from_utf8_lossy(&out.stderr).trim()
                )
            }
        }
    }
}

/// Read the per-node wallet password from the secrets file, if present.
/// Never generates one — absence means "fall back to legacy / not set yet".
async fn read_wallet_password() -> Option<String> {
    let bytes = fs::read(WALLET_PASSWORD_SECRET).await.ok()?;
    let pw = String::from_utf8_lossy(&bytes).trim().to_string();
    (!pw.is_empty()).then_some(pw)
}

/// Return the per-node wallet password, generating and persisting a fresh
/// 256-bit one (base64, 0600) if none exists. Use ONLY when creating a NEW
/// wallet — calling it merely to unlock an existing wallet would record a
/// password that doesn't match it.
pub(crate) async fn ensure_wallet_password() -> Result<String> {
    if let Some(pw) = read_wallet_password().await {
        return Ok(pw);
    }
    use rand::RngCore;
    let mut raw = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut raw);
    let pw = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
    let path = std::path::Path::new(WALLET_PASSWORD_SECRET);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .await
            .with_context(|| format!("creating {}", dir.display()))?;
    }
    fs::write(path, &pw)
        .await
        .with_context(|| format!("writing {WALLET_PASSWORD_SECRET}"))?;
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await;
    Ok(pw)
}

/// Candidate passwords to try when unlocking an EXISTING wallet, in order: the
/// per-node secret (current scheme) first, then the legacy constant so wallets
/// created by older builds still open.
async fn unlock_password_candidates() -> Vec<String> {
    let mut v = Vec::new();
    if let Some(pw) = read_wallet_password().await {
        v.push(pw);
    }
    v.push(LEGACY_WALLET_PASSWORD.to_string());
    v
}

/// Outcome of a single unlock attempt — lets the caller fail fast on a wrong
/// password (no point retrying) vs keep waiting for LND to come up.
enum UnlockAttempt {
    Unlocked,
    WrongPassword,
    NotReady,
}

/// One unlock POST, no internal retry. Distinguishes "invalid passphrase"
/// (WrongPassword — try the next candidate, don't retry) from transient
/// not-ready / connection errors (NotReady — worth retrying).
async fn try_unlock_once(client: &reqwest::Client, password: &str) -> UnlockAttempt {
    let body = serde_json::json!({
        "wallet_password": base64::engine::general_purpose::STANDARD.encode(password)
    });
    match client
        .post(format!("{LND_REST_BASE_URL}/v1/unlockwallet"))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            if status.is_success() || text.contains("already unlocked") {
                UnlockAttempt::Unlocked
            } else if text.contains("invalid passphrase") {
                UnlockAttempt::WrongPassword
            } else {
                UnlockAttempt::NotReady
            }
        }
        Err(_) => UnlockAttempt::NotReady,
    }
}

/// Unlock an existing wallet. Ok(true) = unlocked; Ok(false) = every candidate
/// password was actively rejected (unrecoverable — caller should recreate);
/// Err = transient (LND not ready / timeout — caller should retry, NOT wipe).
async fn unlock_existing_wallet() -> Result<bool> {
    unlock_existing_wallet_via_rest().await
}

async fn unlock_existing_wallet_via_rest() -> Result<bool> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(20))
        .danger_accept_invalid_certs(true)
        .build()
        .context("building LND REST client")?;

    let candidates = unlock_password_candidates().await;
    // Retry only while LND's unlocker isn't ready yet. If every candidate is
    // *actively rejected* (invalid passphrase), retrying can't help — fail fast
    // with a clear message instead of hanging the boot path for 60s+ (the wallet
    // was created with a password this node doesn't have → migration/recovery).
    //
    // The not-ready budget is deliberately generous. LND opens channel.db,
    // graph.db and wallet.db before it starts serving the unlocker at all, and
    // on a busy node that is genuinely slow — observed at 2m38s on a box running
    // 30 containers, where a 60s budget could never succeed. Timing out here is
    // not a harmless retry: reconcile records the post-start hook as failed,
    // which restarts LND, which starts the slow database open over again. The
    // result is a restart loop that leaves the wallet permanently locked and
    // every LND-dependent app (BTCPay's internal node included) broken, on
    // exactly the nodes least able to afford it. Waiting longer costs nothing —
    // a wrong password still exits on the first pass via `all_rejected`.
    for _ in 0..UNLOCK_NOT_READY_ATTEMPTS {
        let mut all_rejected = true;
        for pw in &candidates {
            match try_unlock_once(&client, pw).await {
                UnlockAttempt::Unlocked => return Ok(true),
                UnlockAttempt::WrongPassword => {}
                UnlockAttempt::NotReady => all_rejected = false,
            }
        }
        if all_rejected {
            tracing::warn!(
                "[lnd] none of the {} candidate password(s) unlock the wallet — it was created \
                 with a password this node does not have",
                candidates.len()
            );
            return Ok(false);
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    anyhow::bail!(
        "LND wallet unlock timed out after ~{}s waiting for the unlocker to become ready",
        UNLOCK_NOT_READY_ATTEMPTS
    )
}

/// Unlock an existing wallet WITHOUT the destructive fallback.
///
/// `ensure_wallet_initialized` wipes and recreates a wallet no candidate
/// password can open — correct for a boot path that must self-heal, and exactly
/// wrong for macaroon rotation, which restarts LND against a wallet the operator
/// still wants. Rotation calls this instead, so there is no code path from
/// "rotate my credentials" to "delete my wallet": a rejected password surfaces
/// as an error the caller reports, never as a wipe.
pub(crate) async fn unlock_existing_wallet_no_wipe() -> Result<()> {
    match unlock_existing_wallet().await? {
        true => Ok(()),
        false => anyhow::bail!(
            "LND rejected every candidate wallet password — refusing to touch the wallet. \
             The wallet is intact and still locked; its password is not one this node holds."
        ),
    }
}

/// Current LND wallet state via the unauthenticated `/v1/state` endpoint
/// (NON_EXISTING / LOCKED / UNLOCKED / RPC_ACTIVE / …). None if unreachable.
async fn wallet_state(client: &reqwest::Client) -> Option<String> {
    let resp = client
        .get(format!("{LND_REST_BASE_URL}/v1/state"))
        .send()
        .await
        .ok()?;
    let v: serde_json::Value = resp.json().await.ok()?;
    v.get("state")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
}

/// ChangePassword via WalletUnlocker (wallet must be LOCKED). Both passwords are
/// base64-encoded. Ok(true) = current accepted and rotated; Ok(false) = current
/// rejected (wrong password — try the next candidate); Err = transport/other.
async fn change_wallet_password(
    client: &reqwest::Client,
    current: &str,
    new: &str,
) -> Result<bool> {
    let body = serde_json::json!({
        "current_password": base64::engine::general_purpose::STANDARD.encode(current),
        "new_password": base64::engine::general_purpose::STANDARD.encode(new),
    });
    let resp = client
        .post(format!("{LND_REST_BASE_URL}/v1/changepassword"))
        .json(&body)
        .send()
        .await
        .context("calling LND changepassword")?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status.is_success() {
        Ok(true)
    } else if text.contains("invalid passphrase") {
        Ok(false)
    } else {
        anyhow::bail!("LND changepassword returned {status}: {text}")
    }
}

/// Best-effort migration of a LOCKED wallet onto the per-node secret. Called at
/// login, when the onboarding password is available as a candidate. If the
/// per-node secret already opens the wallet, just unlock. Otherwise try each
/// candidate as the CURRENT password and ChangePassword it to a fresh per-node
/// secret so all future boots auto-unlock. Ok(true) = healed/unlocked;
/// Ok(false) = not locked, or no candidate worked (seed-recovery required).
pub(crate) async fn migrate_locked_wallet(candidates: &[String]) -> Result<bool> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(20))
        .danger_accept_invalid_certs(true)
        .build()
        .context("building LND REST client")?;

    // Only act on a wallet that is actually LOCKED.
    if wallet_state(&client).await.as_deref() != Some("LOCKED") {
        return Ok(false);
    }

    // If the per-node secret already opens it, nothing to rotate — just unlock.
    if let Some(secret) = read_wallet_password().await {
        if matches!(
            try_unlock_once(&client, &secret).await,
            UnlockAttempt::Unlocked
        ) {
            return Ok(true);
        }
    }

    // The wallet's new password becomes the per-node secret (generate if absent).
    let new_secret = ensure_wallet_password().await?;

    // ChangePassword requires LOCKED; bail out if a prior step already unlocked.
    if wallet_state(&client).await.as_deref() != Some("LOCKED") {
        return Ok(true);
    }

    for cand in candidates {
        if cand.is_empty() || *cand == new_secret {
            continue;
        }
        match change_wallet_password(&client, cand, &new_secret).await {
            Ok(true) => {
                tracing::info!("[lnd-migrate] rotated locked wallet onto the per-node secret");
                return Ok(true);
            }
            Ok(false) => continue, // wrong current password — try next candidate
            Err(e) => tracing::debug!("[lnd-migrate] changepassword error: {e}"),
        }
    }
    tracing::warn!(
        "[lnd-migrate] no candidate password opened the wallet — seed-recovery required"
    );
    Ok(false)
}

#[derive(Debug, Deserialize)]
struct GenSeedResponse {
    cipher_seed_mnemonic: Vec<String>,
}

#[derive(Debug)]
enum UnlockerResponse<T> {
    Value(T),
    WalletAlreadyExists,
}

#[derive(Debug, Serialize)]
struct InitWalletRequest {
    wallet_password: String,
    cipher_seed_mnemonic: Vec<String>,
}

async fn init_wallet_via_rest() -> Result<()> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(20))
        .danger_accept_invalid_certs(true)
        .build()
        .context("building LND REST client")?;

    let seed: GenSeedResponse = match get_lnd_unlocker_json(&client, "/v1/genseed")
        .await
        .context("generating LND wallet seed")?
    {
        UnlockerResponse::Value(seed) => seed,
        UnlockerResponse::WalletAlreadyExists => {
            unlock_existing_wallet().await?;
            return Ok(());
        }
    };
    if seed.cipher_seed_mnemonic.is_empty() {
        anyhow::bail!("LND genseed returned no seed words");
    }

    let node_secret = ensure_wallet_password().await?;
    let wallet_password = base64::engine::general_purpose::STANDARD.encode(&node_secret);
    let req = InitWalletRequest {
        wallet_password,
        cipher_seed_mnemonic: seed.cipher_seed_mnemonic.clone(),
    };
    match post_lnd_unlocker_json::<serde_json::Value>(
        &client,
        "/v1/initwallet",
        serde_json::to_value(req)?,
    )
    .await
    .context("initializing LND wallet")?
    {
        UnlockerResponse::Value(_) => {
            persist_aezeed_backup(
                std::path::Path::new(ARCHY_DATA_DIR),
                &seed.cipher_seed_mnemonic,
                &node_secret,
            )
            .await;
        }
        UnlockerResponse::WalletAlreadyExists => {
            unlock_existing_wallet().await?;
        }
    }

    Ok(())
}

/// Persist the just-created wallet's aezeed words encrypted with the per-node
/// secret so they can be revealed later (`lnd.seed-reveal`). aezeed cannot be
/// re-derived after init, so this is the only capture point on the unattended
/// boot path. Best-effort: a failed backup must not fail wallet creation.
pub(crate) async fn persist_aezeed_backup(
    data_dir: &std::path::Path,
    words: &[String],
    node_secret: &str,
) {
    match crate::seed::save_lnd_aezeed_encrypted(data_dir, words, node_secret).await {
        Ok(()) => {
            // A fresh wallet means a fresh seed — any previous "user backed it
            // up" acknowledgment no longer applies.
            crate::seed::clear_lnd_aezeed_acknowledged(data_dir).await;
            tracing::info!("[lnd] aezeed backup saved (encrypted with per-node secret)");
        }
        Err(e) => tracing::warn!("[lnd] failed to save aezeed backup: {e:#}"),
    }
}

/// The per-node wallet password if one has been persisted; never generates.
/// Used by the reveal RPC to decrypt the aezeed backup.
pub(crate) async fn wallet_password_if_exists() -> Option<String> {
    read_wallet_password().await
}

async fn get_lnd_unlocker_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    path: &str,
) -> Result<UnlockerResponse<T>> {
    let url = format!("{LND_REST_BASE_URL}{path}");
    let mut last_err = None;
    for _ in 0..60 {
        match client.get(&url).send().await {
            Ok(resp) => match decode_lnd_unlocker_response(resp, path).await {
                Ok(value) => return Ok(value),
                Err(e) => last_err = Some(e.to_string()),
            },
            Err(e) => last_err = Some(e.to_string()),
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    anyhow::bail!(
        "LND REST {path} unavailable: {}",
        last_err.unwrap_or_else(|| "unknown error".to_string())
    )
}

async fn post_lnd_unlocker_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    path: &str,
    body: serde_json::Value,
) -> Result<UnlockerResponse<T>> {
    let url = format!("{LND_REST_BASE_URL}{path}");
    let mut last_err = None;
    for _ in 0..60 {
        match client.post(&url).json(&body).send().await {
            Ok(resp) => match decode_lnd_unlocker_response(resp, path).await {
                Ok(value) => return Ok(value),
                Err(e) => last_err = Some(e.to_string()),
            },
            Err(e) => last_err = Some(e.to_string()),
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    anyhow::bail!(
        "LND REST {path} unavailable: {}",
        last_err.unwrap_or_else(|| "unknown error".to_string())
    )
}

async fn decode_lnd_unlocker_response<T: for<'de> Deserialize<'de>>(
    resp: reqwest::Response,
    path: &str,
) -> Result<UnlockerResponse<T>> {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status.is_success() {
        let value = serde_json::from_str(&text)
            .with_context(|| format!("parsing LND REST response from {path}"))?;
        return Ok(UnlockerResponse::Value(value));
    }
    if text.contains("wallet already exists") {
        return Ok(UnlockerResponse::WalletAlreadyExists);
    }
    anyhow::bail!("LND REST {path} returned {status}: {text}")
}

async fn lnd_getinfo_ready(admin_macaroon: &str) -> bool {
    let Ok(macaroon) = read_file_as_root(admin_macaroon).await else {
        return false;
    };
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .build()
    else {
        return false;
    };
    client
        .get(format!("{LND_REST_BASE_URL}/v1/getinfo"))
        .header("Grpc-Metadata-macaroon", hex::encode(macaroon))
        .send()
        .await
        .map(|resp| resp.status().is_success())
        .unwrap_or(false)
}

async fn wait_for_admin_macaroon(admin_macaroon: &str) -> Result<()> {
    for _ in 0..60 {
        if file_exists_as_root(admin_macaroon).await {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    anyhow::bail!("LND admin macaroon not created after wallet init")
}

async fn write_config_atomically(paths: &EnsurePaths, conf: &str) -> Result<()> {
    let tmp = paths.conf_path.with_extension("tmp");
    match fs::write(&tmp, conf).await {
        Ok(()) => {
            fs::rename(&tmp, &paths.conf_path).await.with_context(|| {
                format!(
                    "renaming {} -> {}",
                    tmp.display(),
                    paths.conf_path.display()
                )
            })?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let script = format!(
                "set -eu\ncat > '{}' <<'LNDCONF'\n{}LNDCONF\n",
                shell_quote(&paths.conf_path.to_string_lossy()),
                conf
            );
            let status = host_sudo(&["sh", "-lc", &script])
                .await
                .context("writing lnd.conf via sudo")?;
            if !status.success() {
                anyhow::bail!("writing lnd.conf via sudo exited with {status}");
            }
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("writing tmp {}", tmp.display())),
    }
}

fn shell_quote(s: &str) -> String {
    s.replace('\'', "'\\''")
}

fn has_required_lnd_flags(conf: &str, rpc_pass: &str, bitcoin_host: &str) -> bool {
    let rpc_pass_line = format!("bitcoind.rpcpass={rpc_pass}");
    let rpc_host_line = format!("bitcoind.rpchost={bitcoin_host}:8332");
    [
        "bitcoin.active=true",
        "bitcoin.mainnet=true",
        "bitcoin.node=bitcoind",
        rpc_host_line.as_str(),
        rpc_pass_line.as_str(),
    ]
    .iter()
    .all(|needle| conf.lines().any(|line| line.trim() == *needle))
}

/// Secret file consumed by btcpay-server's optional `BTCPAY_BTCLIGHTNING`
/// secret_env (see apps/btcpay-server/manifest.yml).
const BTCPAY_LND_CONNECTION_SECRET: &str = "btcpay-lnd-connection";

/// Materialise the BTCPay→internal-LND connection-string secret.
///
/// LND's datadir is owned by its container subuid (100999 on a stock node),
/// so btcpay cannot bind-mount the macaroon — EACCES across the userns
/// boundary. The connection string therefore carries the macaroon inline as
/// hex, delivered by reference through the podman secret store.
///
/// No-op when LND isn't provisioned yet (missing tls.cert or macaroon) —
/// btcpay's secret_env entry is `optional`, so it simply starts without an
/// internal Lightning node and picks it up on a later reconcile tick.
/// Rewrites when the pinned cert thumbprint no longer matches (LND TLS cert
/// rotation). Macaroon rotation without cert rotation is not auto-detected here
/// (reading the macaroon needs sudo; probing it every tick is not worth the
/// churn) — the rotation path calls `rewrite_btcpay_lnd_connection_secret`
/// instead, and deleting the secret file also forces regeneration.
pub async fn ensure_btcpay_lnd_connection_secret(secrets_dir: &std::path::Path) -> Result<()> {
    build_btcpay_lnd_connection_secret(secrets_dir, false)
        .await
        .map(|_| ())
}

/// Rewrite the BTCPay→LND connection secret unconditionally, ignoring the
/// cert-thumbprint fast path.
///
/// Rotating LND's macaroons invalidates the one embedded in this secret, and it
/// is embedded *inline* rather than referenced by path — LND's datadir is owned
/// by its container subuid, so btcpay cannot bind-mount the file and the string
/// cannot self-heal. Nothing else notices: the TLS cert is untouched by macaroon
/// rotation, so `ensure_…` takes its fast path forever and BTCPay keeps
/// presenting a credential LND no longer honours. A node in that state looks
/// entirely healthy — btcpay is up, LND is up — while every Lightning invoice it
/// tries to create fails.
///
/// Writing the new value makes the change *visible*: `secret_env_hash` is
/// derived from the resolved secret contents, so a changed file reads as label
/// drift on the running container. It is not sufficient on its own — btcpay is
/// restart-sensitive, and boot reconcile deliberately leaves running
/// restart-sensitive apps untouched on drift. The caller must also call
/// `ContainerOrchestrator::mark_credential_rotated("btcpay-server")`, which is
/// the carve-out for exactly this case: a container that is up and healthy while
/// holding a credential that no longer works. The orchestrator's own recreate
/// path then rebuilds it around an unchanged data directory. No teardown here,
/// deliberately — a hand-rolled remove-and-run is the anti-pattern CLAUDE.md
/// names.
///
/// Returns `false` when LND isn't provisioned enough to derive a value.
pub async fn rewrite_btcpay_lnd_connection_secret(secrets_dir: &std::path::Path) -> Result<bool> {
    build_btcpay_lnd_connection_secret(secrets_dir, true).await
}

/// Shared body. `force` skips the "already pins the current cert" fast path.
/// Returns whether a value was written.
async fn build_btcpay_lnd_connection_secret(
    secrets_dir: &std::path::Path,
    force: bool,
) -> Result<bool> {
    let cert_path = format!("{DEFAULT_DATA_DIR}/tls.cert");
    let pem = match fs::read_to_string(&cert_path).await {
        Ok(s) => s,
        Err(_) => return Ok(false), // LND not installed/provisioned yet
    };
    let thumbprint = cert_sha256_thumbprint(&pem).context("computing LND tls.cert thumbprint")?;

    let target = secrets_dir.join(BTCPAY_LND_CONNECTION_SECRET);
    // Fast path (no sudo): existing secret already pins the current cert.
    if !force {
        if let Ok(existing) = fs::read_to_string(&target).await {
            if !existing.trim().is_empty()
                && existing.contains(&format!("certthumbprint={thumbprint}"))
            {
                return Ok(false);
            }
        }
    }

    let macaroon_path = format!("{DEFAULT_DATA_DIR}/data/chain/bitcoin/mainnet/admin.macaroon");
    if !file_exists_as_root(&macaroon_path).await {
        return Ok(false); // wallet not created yet; next tick retries
    }
    let macaroon = read_file_as_root(&macaroon_path).await?;
    let value = format!(
        "type=lnd-rest;server=https://lnd:8080/;macaroon={};certthumbprint={}",
        hex::encode(macaroon),
        thumbprint
    );
    crate::container::secrets::write_secret_file(&target, &value)
        .context("writing btcpay-lnd-connection secret")?;
    Ok(true)
}

/// Does the on-disk BTCPay connection secret still carry the macaroon LND is
/// currently issuing? `None` when there is nothing to compare — no secret file
/// (BTCPay has no internal node configured) or no macaroon (LND unprovisioned).
///
/// Compares only hex text that is already on this host; the value is never
/// logged, returned over RPC, or placed in an error.
pub(crate) async fn btcpay_lnd_connection_is_current(
    secrets_dir: &std::path::Path,
) -> Option<bool> {
    let target = secrets_dir.join(BTCPAY_LND_CONNECTION_SECRET);
    let existing = fs::read_to_string(&target).await.ok()?;
    let embedded = existing
        .split("macaroon=")
        .nth(1)?
        .split(';')
        .next()?
        .to_string();
    let macaroon_path = format!("{DEFAULT_DATA_DIR}/data/chain/bitcoin/mainnet/admin.macaroon");
    let current = read_file_as_root(&macaroon_path).await.ok()?;
    Some(embedded.eq_ignore_ascii_case(&hex::encode(current)))
}

/// SHA256 over the DER certificate body (matches
/// `openssl x509 -fingerprint -sha256` without colons) — the format BTCPay's
/// `certthumbprint=` connection-string parameter expects.
fn cert_sha256_thumbprint(pem: &str) -> Result<String> {
    use sha2::{Digest, Sha256};
    let b64: String = pem
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect::<Vec<_>>()
        .join("");
    let der = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .context("decoding tls.cert PEM body")?;
    Ok(hex::encode_upper(Sha256::digest(&der)))
}

// ── Channel-peer watchdog ──────────────────────────────────────────────────

/// Every open channel's remote peer that is NOT currently connected.
/// Pure over LND's REST JSON so the selection can be unit-tested.
///
/// `/v1/peers` uses `pub_key`; `/v1/channels` uses `remote_pubkey` — the
/// asymmetry is LND's, not ours.
fn select_reconnect_targets(
    channels: &serde_json::Value,
    peers: &serde_json::Value,
) -> Vec<String> {
    let connected: std::collections::HashSet<&str> = peers
        .get("peers")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.get("pub_key").and_then(|v| v.as_str()))
                .collect()
        })
        .unwrap_or_default();
    let mut targets: Vec<String> = channels
        .get("channels")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("remote_pubkey").and_then(|v| v.as_str()))
                .filter(|pk| !connected.contains(pk))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    targets.sort();
    targets.dedup();
    targets
}

/// Reconnect peers of open channels that LND has not re-established on its
/// own. Returns the number of peers reconnected this pass.
///
/// LND normally reconnects channel peers after a restart — but not reliably:
/// when the restart outages are long or repeated (an app update, a node
/// reboot, reconciler churn), the peer link can stay down for hours while
/// BOTH endpoints keep flagging the channel `disabled` in the routing
/// graph. The node itself looks perfectly healthy and every payment in
/// either direction fails "no route to the recipient" — observed live on
/// framework-pt (2026-09-01): its only channel sat disabled on both policy
/// sides for ~17h after the LND 0.21.2 update, while the wallet showed
/// plenty of outbound. The channel graph is desired state; this keeps it.
///
/// Quietly returns Ok(0) when LND is not installed or its wallet is locked —
/// that is every node without LND, on every pass.
///
/// `last_attempt` throttles retries per peer (`min_retry`) so an unreachable
/// peer is not hammered every pass; the caller owns the map so the pass
/// itself stays stateless and testable.
pub(crate) async fn reconnect_disconnected_channel_peers(
    last_attempt: &mut std::collections::HashMap<String, std::time::Instant>,
    min_retry: std::time::Duration,
) -> Result<usize> {
    let Ok(macaroon) = read_file_as_root(LND_ADMIN_MACAROON).await else {
        return Ok(0); // LND not installed (or not initialized yet)
    };
    let macaroon_hex = hex::encode(macaroon);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(8))
        .danger_accept_invalid_certs(true)
        .build()
        .context("building LND REST client for the channel-peer watchdog")?;

    let channels: serde_json::Value = client
        .get(format!("{LND_REST_BASE_URL}/v1/channels"))
        .header("Grpc-Metadata-macaroon", &macaroon_hex)
        .send()
        .await
        .context("LND REST: listing channels for the peer watchdog")?
        .json()
        .await
        .context("parsing LND channel list")?;
    // A locked wallet answers 503 with an error body — it parses as JSON
    // with no "channels" key, which selects nothing. That is a quiet pass.
    let peers: serde_json::Value = client
        .get(format!("{LND_REST_BASE_URL}/v1/peers"))
        .header("Grpc-Metadata-macaroon", &macaroon_hex)
        .send()
        .await
        .context("LND REST: listing peers for the peer watchdog")?
        .json()
        .await
        .context("parsing LND peer list")?;

    let mut reconnected = 0usize;
    for pubkey in select_reconnect_targets(&channels, &peers) {
        if last_attempt
            .get(&pubkey)
            .is_some_and(|t| t.elapsed() < min_retry)
        {
            continue;
        }
        last_attempt.insert(pubkey.clone(), std::time::Instant::now());

        // Where does the peer live? Its advertised addresses in the public
        // graph. A peer with none (fully private) cannot be dialed from here
        // — LND itself may still find it; we only log the gap once per pass.
        // Unknown to the public graph (or the graph query failed) — nothing
        // to dial on.
        let Ok(node) = client
            .get(format!("{LND_REST_BASE_URL}/v1/graph/node/{pubkey}"))
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        else {
            continue;
        };
        let Ok(node) = node.json::<serde_json::Value>().await else {
            continue;
        };
        let addresses: Vec<String> = node
            .get("node")
            .and_then(|n| n.get("addresses"))
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a.get("addr").and_then(|v| v.as_str()))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if addresses.is_empty() {
            tracing::warn!(
                peer = %pubkey,
                "LND channel peer is disconnected and advertises no address — cannot dial it; payments through this channel stay unroutable"
            );
            continue;
        }

        for addr in addresses {
            let Some((host, port)) = addr.rsplit_once(':') else {
                continue;
            };
            let Ok(port) = port.parse::<u32>() else {
                continue;
            };
            let body = serde_json::json!({
                "perm": false,
                "timeout": "15s",
                "addr": { "pubkey": pubkey, "host": host, "port": port },
            });
            match client
                .post(format!("{LND_REST_BASE_URL}/v1/peers"))
                .header("Grpc-Metadata-macaroon", &macaroon_hex)
                .json(&body)
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    reconnected += 1;
                    tracing::info!(
                        peer = %pubkey,
                        addr = %addr,
                        "reconnected a disconnected channel peer (channel was unroutable)"
                    );
                    break;
                }
                Ok(resp) => {
                    let msg = resp.text().await.unwrap_or_default();
                    // Already connected between our list call and now — success.
                    if msg.contains("already connected") {
                        break;
                    }
                    tracing::debug!(peer = %pubkey, addr = %addr, %msg, "channel-peer connect attempt failed");
                }
                Err(e) => {
                    tracing::debug!(peer = %pubkey, addr = %addr, error = %e, "channel-peer connect attempt failed");
                }
            }
        }
    }
    Ok(reconnected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ensure_config_writes_required_bitcoin_network_flags() {
        let tmp = tempfile::TempDir::new().unwrap();
        let paths = EnsurePaths {
            data_dir: tmp.path().join("lnd"),
            conf_path: tmp.path().join("lnd/lnd.conf"),
        };

        let out = ensure_config(&paths, "secret", "bitcoin-knots")
            .await
            .unwrap();
        assert_eq!(out, EnsureOutcome::Written);
        let conf = fs::read_to_string(&paths.conf_path).await.unwrap();
        assert!(conf.contains("bitcoin.active=true"));
        assert!(conf.contains("bitcoin.mainnet=true"));
        assert!(conf.contains("bitcoin.node=bitcoind"));
        assert!(conf.contains("bitcoind.rpchost=bitcoin-knots:8332"));
        assert!(conf.contains("bitcoind.rpcpass=secret"));
    }

    #[tokio::test]
    async fn ensure_config_repairs_rpc_password_drift() {
        let tmp = tempfile::TempDir::new().unwrap();
        let paths = EnsurePaths {
            data_dir: tmp.path().join("lnd"),
            conf_path: tmp.path().join("lnd/lnd.conf"),
        };

        assert_eq!(
            ensure_config(&paths, "first", "bitcoin-knots")
                .await
                .unwrap(),
            EnsureOutcome::Written
        );
        assert_eq!(
            ensure_config(&paths, "second", "bitcoin-knots")
                .await
                .unwrap(),
            EnsureOutcome::Written
        );
        let conf = fs::read_to_string(&paths.conf_path).await.unwrap();
        assert!(conf.contains("bitcoind.rpcpass=second"));
    }

    #[tokio::test]
    async fn ensure_config_repairs_bitcoin_host_drift() {
        // A conf written against bitcoin-knots must be rewritten when the
        // node's Bitcoin variant is bitcoin-core, or LND dials a hostname
        // that doesn't exist on archy-net and dies on startup.
        let tmp = tempfile::TempDir::new().unwrap();
        let paths = EnsurePaths {
            data_dir: tmp.path().join("lnd"),
            conf_path: tmp.path().join("lnd/lnd.conf"),
        };

        assert_eq!(
            ensure_config(&paths, "pw", "bitcoin-knots").await.unwrap(),
            EnsureOutcome::Written
        );
        assert_eq!(
            ensure_config(&paths, "pw", "bitcoin-core").await.unwrap(),
            EnsureOutcome::Written
        );
        let conf = fs::read_to_string(&paths.conf_path).await.unwrap();
        assert!(conf.contains("bitcoind.rpchost=bitcoin-core:8332"));
        assert!(!conf.contains("bitcoind.rpchost=bitcoin-knots:8332"));

        assert_eq!(
            ensure_config(&paths, "pw", "bitcoin-core").await.unwrap(),
            EnsureOutcome::Unchanged
        );
    }

    #[tokio::test]
    async fn ensure_config_repairs_incomplete_existing_config() {
        let tmp = tempfile::TempDir::new().unwrap();
        let paths = EnsurePaths {
            data_dir: tmp.path().join("lnd"),
            conf_path: tmp.path().join("lnd/lnd.conf"),
        };
        fs::create_dir_all(&paths.data_dir).await.unwrap();
        fs::write(&paths.conf_path, "debuglevel=info\n")
            .await
            .unwrap();

        assert_eq!(
            ensure_config(&paths, "repaired", "bitcoin-knots")
                .await
                .unwrap(),
            EnsureOutcome::Written
        );
        let conf = fs::read_to_string(&paths.conf_path).await.unwrap();
        assert!(conf.contains("bitcoin.mainnet=true"));
        assert!(conf.contains("bitcoind.rpcpass=repaired"));
    }

    #[test]
    fn legacy_wallet_password_is_valid_for_lncli() {
        // Legacy fallback must still be a valid lncli passphrase (>8 chars).
        assert!(LEGACY_WALLET_PASSWORD.len() > 8);
    }

    #[tokio::test]
    async fn unlock_candidates_always_include_legacy_fallback() {
        // With no per-node secret on disk in the test env, candidates fall back
        // to the legacy constant so old wallets still open.
        let cands = unlock_password_candidates().await;
        assert!(cands.iter().any(|p| p == LEGACY_WALLET_PASSWORD));
    }

    #[test]
    fn reconnect_targets_pick_disconnected_channel_peers_only() {
        // Shape captured from a live node: /v1/channels uses remote_pubkey,
        // /v1/peers uses pub_key, and an offline channel's peer is simply
        // absent from the peer list — that absence is the whole signal.
        let channels = serde_json::json!({
            "channels": [
                { "remote_pubkey": "AAA", "active": true },
                { "remote_pubkey": "BBB", "active": false },
                { "remote_pubkey": "AAA" }
            ]
        });
        let peers = serde_json::json!({ "peers": [ { "pub_key": "AAA" } ] });

        let targets = select_reconnect_targets(&channels, &peers);
        assert_eq!(targets, vec!["BBB".to_string()]);
    }

    #[test]
    fn reconnect_targets_empty_without_channels_or_peers() {
        // No LND wallet (503 error body), locked wallet, or an empty node:
        // selects nothing, quietly.
        let error_body = serde_json::json!({ "message": "locked" });
        assert!(select_reconnect_targets(&error_body, &serde_json::json!({})).is_empty());
        assert!(select_reconnect_targets(
            &serde_json::json!({ "channels": [] }),
            &serde_json::json!({ "peers": [] })
        )
        .is_empty());
    }
}
