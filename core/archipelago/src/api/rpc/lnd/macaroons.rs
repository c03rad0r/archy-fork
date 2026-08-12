//! LND macaroon rotation, driven from the dashboard.
//!
//! A macaroon is a bearer token: whoever holds it can spend from this node's
//! Lightning wallet. Anything that ever read one — a leaked endpoint, a shared
//! screenshot, a paired phone that has since been lost, a BTCPay instance that
//! ran a version with a published vulnerability — keeps that ability until the
//! macaroons are rotated. Rotation is therefore a routine operator action, and
//! before this module the only way to perform it was to SSH into the node and
//! run `scripts/security/rotate-lnd-macaroon.sh` by hand.
//!
//! ## What rotation actually does
//!
//! LND derives every macaroon it issues from a root key in `macaroons.db`.
//! Remove that root key and the issued macaroon files, restart, and LND mints a
//! fresh root key and a fresh set of macaroons on unlock. Every previously
//! issued macaroon — including any an attacker holds — stops verifying.
//!
//! ## Why funds and channels survive
//!
//! Macaroons are bearer tokens, not keys. Coins live in `wallet.db` and channel
//! state in `channel.db`; channels are secured by the node's identity and
//! channel keys, none of which derive from the macaroon root key. This code
//! never opens, moves or deletes either database. What it does instead is
//! *prove* they survived: it records the node's identity pubkey and channel
//! census before rotating and refuses to report success if either changed.
//!
//! Deliberately NOT asserted: that `wallet.db` is byte-identical. btcwallet
//! records chain-sync progress inside it, so the file legitimately changes on
//! every start — asserting byte-identity would fire a frightening false alarm
//! on a completely healthy rotation.
//!
//! ## What it never does
//!
//! No macaroon *content* is read into a response, an error, a log line or the
//! progress feed the UI polls. Everything reported is a SHA-256 digest or a
//! byte count — enough to prove the material changed without disclosing it to
//! whoever is looking at the screen.
//!
//! It also cannot reach LND's destructive wallet-recovery path: the restart
//! unlocks via `unlock_existing_wallet_no_wipe`, so a wallet whose password
//! this node does not hold surfaces as a failed rotation, never as a wipe.

use crate::api::rpc::RpcHandler;
use anyhow::{Context, Result};
use serde::Serialize;
use std::sync::{Arc, Mutex, OnceLock};

use super::LND_REST_BASE_URL;

/// LND's mainnet macaroon directory. 0700 and owned by the container's mapped
/// uid, so every read/write below goes through `sudo -n`.
const LND_MAINNET_DIR: &str = "/var/lib/archipelago/lnd/data/chain/bitcoin/mainnet";

/// Quadlet service for the core LND app. Older nodes run LND as a plain podman
/// container with no unit; `restart_lnd` handles both.
const LND_SERVICE: &str = "lnd.service";
const LND_CONTAINER: &str = "lnd";

/// Where the orchestrator materialises app secrets. Hardcoded to match
/// `prod_orchestrator`'s own default rather than derived from `config.data_dir`
/// — writing the BTCPay connection string anywhere the orchestrator does not
/// read it would be worse than not writing it at all, because it would look
/// like it worked.
const SECRETS_DIR: &str = "/var/lib/archipelago/secrets";

/// Longest we wait for LND to mint a fresh `admin.macaroon` after the restart.
/// Generous on purpose: LND opens its databases before serving anything, which
/// is minutes on a loaded node.
const MACAROON_WAIT_SECS: u64 = 900;

// ── Progress the UI polls ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum StepState {
    Pending,
    Running,
    Done,
    Failed,
    /// Ran, decided there was nothing to do, and said so. Distinct from `Done`
    /// so "BTCPay has no internal Lightning node" never reads as "BTCPay was
    /// reconnected".
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
struct RotationStep {
    key: &'static str,
    label: &'static str,
    state: StepState,
    detail: Option<String>,
}

/// Ordered because the UI renders it as a checklist and an operator watching a
/// credential rotation should be able to see exactly how far it got.
const STEPS: &[(&str, &str)] = &[
    (
        "preflight",
        "Check LND is healthy and record what must survive",
    ),
    ("backup", "Back up the current macaroon material"),
    ("stop", "Stop Lightning"),
    ("remove", "Remove the old root key and issued macaroons"),
    ("start", "Start Lightning and unlock the wallet"),
    ("verify", "Confirm the node and its channels are unchanged"),
    ("btcpay", "Reconnect BTCPay Server to the new credentials"),
];

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RotationProgress {
    running: bool,
    /// `None` while running, then the verdict. Split from `running` so the UI
    /// can tell "in progress" from "finished and failed".
    ok: Option<bool>,
    started_at: Option<String>,
    finished_at: Option<String>,
    error: Option<String>,
    steps: Vec<RotationStep>,
    /// Where the old material was copied. Still secret — it is the old root key
    /// — so the UI tells the operator to delete it once clients are re-paired.
    backup_path: Option<String>,
    identity_pubkey: Option<String>,
    channels_before: Option<u32>,
    channels_after: Option<u32>,
    /// Digest of the freshly minted admin macaroon. A digest, never the token.
    new_admin_macaroon_sha256: Option<String>,
}

impl Default for RotationProgress {
    fn default() -> Self {
        Self {
            running: false,
            ok: None,
            started_at: None,
            finished_at: None,
            error: None,
            steps: STEPS
                .iter()
                .map(|(key, label)| RotationStep {
                    key,
                    label,
                    state: StepState::Pending,
                    detail: None,
                })
                .collect(),
            backup_path: None,
            identity_pubkey: None,
            channels_before: None,
            channels_after: None,
            new_admin_macaroon_sha256: None,
        }
    }
}

impl RotationProgress {
    fn set(&mut self, key: &str, state: StepState, detail: Option<String>) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.key == key) {
            step.state = state;
            if detail.is_some() {
                step.detail = detail;
            }
        }
    }
}

/// One rotation at a time, process-wide. Two concurrent rotations would race on
/// the same files with LND stopped underneath them.
fn progress() -> &'static Mutex<RotationProgress> {
    static PROGRESS: OnceLock<Mutex<RotationProgress>> = OnceLock::new();
    PROGRESS.get_or_init(|| Mutex::new(RotationProgress::default()))
}

fn with_progress<F: FnOnce(&mut RotationProgress)>(f: F) {
    if let Ok(mut guard) = progress().lock() {
        f(&mut guard);
    }
}

fn snapshot() -> RotationProgress {
    progress()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|e| e.into_inner().clone())
}

// ── Host helpers ─────────────────────────────────────────────────────────────

/// `sudo -n <args>`, capturing output. Non-interactive: a node whose sudoers
/// does not permit this fails loudly here rather than hanging on a prompt.
async fn sudo(args: &[&str]) -> Result<std::process::Output> {
    let mut cmd = tokio::process::Command::new("sudo");
    cmd.arg("-n").args(args);
    cmd.output()
        .await
        .with_context(|| format!("sudo -n {}", args.join(" ")))
}

async fn sudo_ok(args: &[&str]) -> Result<()> {
    let out = sudo(args).await?;
    if !out.status.success() {
        anyhow::bail!(
            "sudo {} exited {}: {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// SHA-256 of a root-owned file, or `None` if it isn't there. Only ever the
/// digest — the file's bytes never enter this process.
async fn digest_as_root(path: &str) -> Option<String> {
    let out = sudo(&["sha256sum", path]).await.ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .map(str::to_string)
}

/// Every file rotation replaces: the issued macaroons plus the root key they
/// derive from.
///
/// Enumerated with `sudo find` rather than a shell glob for a reason worth
/// keeping: the directory is 0700 owned by the container's mapped uid, so a
/// glob evaluated by this (unprivileged) process expands to nothing. It would
/// silently make both the backup and the removal no-ops while every surrounding
/// step still reported success.
async fn macaroon_files() -> Result<Vec<String>> {
    let out = sudo(&[
        "find",
        LND_MAINNET_DIR,
        "-maxdepth",
        "1",
        "(",
        "-name",
        "*.macaroon",
        "-o",
        "-name",
        "macaroons.db",
        ")",
    ])
    .await?;
    if !out.status.success() {
        anyhow::bail!(
            "listing macaroon material failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

/// True when LND is managed by a generated Quadlet unit on this node. Nodes
/// predating the Quadlet migration run a bare podman container instead, and
/// stopping the wrong way there means either a no-op or an orphan.
async fn lnd_has_quadlet_unit() -> bool {
    crate::container::quadlet::is_active(LND_SERVICE).await
        || tokio::process::Command::new("systemctl")
            .args(["--user", "cat", LND_SERVICE])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
}

async fn stop_lnd() -> Result<()> {
    if lnd_has_quadlet_unit().await {
        return crate::container::quadlet::stop_service(LND_SERVICE)
            .await
            .context("stopping lnd.service");
    }
    podman_scoped(&["stop", LND_CONTAINER]).await
}

async fn start_lnd() -> Result<()> {
    if lnd_has_quadlet_unit().await {
        return crate::container::quadlet::enable_now(LND_SERVICE)
            .await
            .context("starting lnd.service");
    }
    podman_scoped(&["start", LND_CONTAINER]).await
}

/// `podman` inside a transient user scope, matching how the orchestrator and
/// health monitor drive rootless containers (keeps it out of the archipelago
/// service's cgroup, so an archipelago restart doesn't take LND with it).
async fn podman_scoped(args: &[&str]) -> Result<()> {
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

// ── LND facts ────────────────────────────────────────────────────────────────

/// Identity and channel census — the two things that must be identical either
/// side of a rotation.
#[derive(Debug, Clone, Copy, Default)]
struct LndCensus {
    channels_open: u32,
    channels_pending: u32,
}

fn lnd_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(15))
        // LND serves its own self-signed cert on loopback; the macaroon, not
        // the certificate, is what authenticates this call.
        .danger_accept_invalid_certs(true)
        .build()
        .context("building LND REST client")
}

/// `getinfo` using whatever macaroon is on disk right now. Returns the identity
/// pubkey and census, or an error describing why LND could not answer.
async fn read_census() -> Result<(String, LndCensus)> {
    let macaroon = super::read_lnd_admin_macaroon()
        .await
        .context("reading LND admin macaroon")?;
    let resp = lnd_client()?
        .get(format!("{LND_REST_BASE_URL}/v1/getinfo"))
        .header("Grpc-Metadata-macaroon", hex::encode(&macaroon))
        .send()
        .await
        .context("LND is not answering on its REST port")?;
    let body: serde_json::Value = resp
        .json()
        .await
        .context("LND returned a response that is not JSON")?;
    let pubkey = body
        .get("identity_pubkey")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "LND did not report an identity — it is most likely still starting or locked ({})",
                body.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("no detail")
            )
        })?;
    let num = |k: &str| body.get(k).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    Ok((
        pubkey,
        LndCensus {
            // Active + inactive, summed deliberately. `num_active_channels`
            // counts channels whose peer is currently online, so it legitimately
            // dips for minutes after ANY restart while peers reconnect —
            // asserting on it alone would abort a perfectly healthy rotation.
            // The total number of channels held is the real safety property.
            channels_open: num("num_active_channels") + num("num_inactive_channels"),
            channels_pending: num("num_pending_channels"),
        },
    ))
}

/// Poll until LND answers `getinfo` with a fresh macaroon, or the budget runs
/// out. Used after the restart, so "not ready yet" is the expected case for
/// most of the wait.
async fn wait_for_serving(deadline: std::time::Instant) -> Result<(String, LndCensus)> {
    let mut last = String::from("LND did not become reachable");
    while std::time::Instant::now() < deadline {
        match read_census().await {
            Ok(v) => return Ok(v),
            Err(e) => last = format!("{e:#}"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
    anyhow::bail!("timed out waiting for LND to serve again: {last}")
}

// ── Status ───────────────────────────────────────────────────────────────────

impl RpcHandler {
    /// Read-only picture of this node's Lightning credentials: when they were
    /// issued, what depends on them, and whether anything is already out of
    /// step. Never returns macaroon content.
    pub(in crate::api::rpc) async fn handle_lnd_macaroon_status(
        &self,
    ) -> Result<serde_json::Value> {
        let admin_path = format!("{LND_MAINNET_DIR}/admin.macaroon");
        let installed = digest_as_root(&admin_path).await;

        // `stat -c %y` on the macaroon is when LND last minted it, which is the
        // one date an operator actually wants ("am I still carrying credentials
        // from before that incident?").
        let issued_at = match sudo(&["stat", "-c", "%y", &admin_path]).await {
            Ok(out) if out.status.success() => Some(
                String::from_utf8_lossy(&out.stdout)
                    .trim()
                    .chars()
                    .take(19)
                    .collect::<String>(),
            ),
            _ => None,
        };

        let (identity_pubkey, census, lnd_error) = match read_census().await {
            Ok((pk, c)) => (Some(pk), Some(c), None),
            Err(e) => (None, None, Some(format!("{e:#}"))),
        };

        // Whether BTCPay's inline copy still matches. `None` = BTCPay has no
        // internal Lightning node configured, which is a normal state and not a
        // problem to report.
        let btcpay_current = crate::container::lnd::btcpay_lnd_connection_is_current(
            std::path::Path::new(SECRETS_DIR),
        )
        .await;

        Ok(serde_json::json!({
            "installed": installed.is_some(),
            "admin_macaroon_sha256": installed,
            "issued_at": issued_at,
            "identity_pubkey": identity_pubkey,
            "channels_open": census.map(|c| c.channels_open),
            "channels_pending": census.map(|c| c.channels_pending),
            "lnd_error": lnd_error,
            "btcpay_uses_internal_lnd": btcpay_current.is_some(),
            "btcpay_credential_current": btcpay_current,
            "rotation": snapshot(),
        }))
    }

    /// Start a rotation. Password-gated and asynchronous.
    ///
    /// Password-gated because invalidating every credential a wallet app holds
    /// is an operator action, and a session cookie only proves a browser was
    /// once logged in — the same reasoning as `node.rotate-identity` and TOTP
    /// setup, which both re-verify.
    ///
    /// Asynchronous because the work takes minutes (LND's databases have to
    /// close and reopen); the HTTP request returns immediately and the UI polls
    /// `lnd.macaroon-rotation-progress`.
    pub(in crate::api::rpc) async fn handle_lnd_rotate_macaroons(
        self: &Arc<Self>,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let password = params
            .as_ref()
            .and_then(|p| p.get("password"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if password.is_empty() {
            anyhow::bail!("Node password required to rotate Lightning credentials");
        }
        if !self.auth_manager.verify_password(password).await? {
            anyhow::bail!("Password verification failed");
        }

        // Claim the slot and publish a fresh feed in one critical section, so a
        // second click cannot observe a half-reset progress object.
        {
            let mut guard = progress()
                .lock()
                .map_err(|_| anyhow::anyhow!("rotation state poisoned"))?;
            if guard.running {
                anyhow::bail!("A macaroon rotation is already running on this node");
            }
            *guard = RotationProgress {
                running: true,
                started_at: Some(chrono::Utc::now().to_rfc3339()),
                ..Default::default()
            };
        }

        let orchestrator = self.orchestrator.clone();
        tokio::spawn(async move {
            let outcome = run_rotation(orchestrator).await;
            with_progress(|p| {
                p.running = false;
                p.finished_at = Some(chrono::Utc::now().to_rfc3339());
                match &outcome {
                    Ok(()) => p.ok = Some(true),
                    Err(e) => {
                        p.ok = Some(false);
                        p.error = Some(format!("{e:#}"));
                    }
                }
            });
            match outcome {
                Ok(()) => tracing::info!("LND macaroon rotation completed"),
                Err(e) => {
                    tracing::error!(error = %format!("{e:#}"), "LND macaroon rotation failed")
                }
            }
        });

        Ok(serde_json::json!({ "status": "started" }))
    }

    /// Poll the running (or last) rotation.
    pub(in crate::api::rpc) async fn handle_lnd_macaroon_rotation_progress(
        &self,
    ) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(snapshot())?)
    }
}

// ── The rotation itself ──────────────────────────────────────────────────────

/// Hold LND's lifecycle lock for the whole rotation, then do the work.
///
/// Between "stop LND" and "start LND" this owns a stopped container with its
/// credential material deleted — the single worst moment for another actor to
/// step in. Two would, unasked: the health monitor restarts any container it
/// finds stopped, and the reconciler starts one whose unit is enabled. Either
/// brings LND back up mid-deletion, and LND re-mints `macaroons.db` on unlock —
/// so the deletion loop would race a live process writing that file, or
/// "succeed" against material that had already been regenerated, leaving the
/// operator told they had rotated while the old root key was still in service.
///
/// `app_ops::op_lock` is the mechanism both of those actors already consult
/// (`lifecycle_op_in_flight`, via `lifecycle_op_covers_container` in the health
/// monitor), and it also serialises against the package.start/stop/restart
/// workers, so an operator hitting "Restart" on Lightning mid-rotation queues
/// instead of interleaving.
///
/// Chosen over the `user-stopped` marker that `recreate_wallet_destructively`
/// uses for its own window: that marker is a file on disk, so a rotation that
/// died between marking and clearing would leave Lightning suppressed
/// *permanently*, fixable only by finding and editing JSON on the node. This
/// guard releases when it drops, on every path including a panic.
async fn run_rotation(
    orchestrator: Option<Arc<dyn crate::container::ContainerOrchestrator>>,
) -> Result<()> {
    let lock = crate::app_ops::op_lock(LND_CONTAINER);
    // Fail fast rather than queue. This is a button someone just pressed: a
    // silent wait behind a start/stop/restart that may itself take minutes reads
    // as "nothing happened", and the honest answer is short.
    let _guard = lock.try_lock().map_err(|_| {
        anyhow::anyhow!(
            "another Lightning start/stop/restart is in progress on this node — \
             wait for it to finish and try again"
        )
    })?;
    rotate_with_lnd_pinned(orchestrator).await
}

async fn rotate_with_lnd_pinned(
    orchestrator: Option<Arc<dyn crate::container::ContainerOrchestrator>>,
) -> Result<()> {
    // 1. Preflight — establish what must survive, while LND can still be asked.
    with_progress(|p| p.set("preflight", StepState::Running, None));
    let files = macaroon_files().await?;
    if files.is_empty() {
        with_progress(|p| p.set("preflight", StepState::Failed, None));
        anyhow::bail!(
            "no macaroon material found in {LND_MAINNET_DIR} — nothing to rotate, and \
             restarting Lightning for no reason would be a pointless outage"
        );
    }
    let (pubkey_before, census_before) = read_census().await.context(
        "refusing to rotate: LND is not answering, so there would be no baseline to prove your \
         channels survived. Start Lightning, wait for it to sync, and try again",
    )?;
    with_progress(|p| {
        p.identity_pubkey = Some(pubkey_before.clone());
        p.channels_before = Some(census_before.channels_open);
        p.set(
            "preflight",
            StepState::Done,
            Some(format!(
                "{} channel(s) open, {} pending — these must be identical afterwards",
                census_before.channels_open, census_before.channels_pending
            )),
        );
    });

    // 2. Back up, so a mistake is recoverable. Verified by count: a backup that
    //    silently copied nothing is the one failure that makes the deletion
    //    below unrecoverable.
    with_progress(|p| p.set("backup", StepState::Running, None));
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let backup = format!("/var/lib/archipelago/lnd/macaroon-rotation-{stamp}");
    sudo_ok(&["mkdir", "-p", &backup]).await?;
    sudo_ok(&["chmod", "700", &backup]).await?;
    for f in &files {
        sudo_ok(&["cp", "-a", f, &backup])
            .await
            .with_context(|| format!("backing up {f} — aborting before any deletion"))?;
    }
    let backed_up = sudo(&["find", &backup, "-maxdepth", "1", "-type", "f"])
        .await?
        .stdout;
    let backed_up = String::from_utf8_lossy(&backed_up)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    if backed_up != files.len() {
        with_progress(|p| p.set("backup", StepState::Failed, None));
        anyhow::bail!(
            "backup incomplete — {backed_up} of {} files in {backup}. Refusing to delete anything",
            files.len()
        );
    }
    with_progress(|p| {
        p.backup_path = Some(backup.clone());
        p.set(
            "backup",
            StepState::Done,
            Some(format!("{backed_up} file(s) copied to {backup}")),
        );
    });

    // 3. Stop. Nothing may restart LND from here until step 5 — see the lock
    //    `run_rotation` holds around this whole function.
    with_progress(|p| p.set("stop", StepState::Running, None));
    stop_lnd().await.context("stopping LND")?;
    with_progress(|p| p.set("stop", StepState::Done, None));

    // 4. Remove the credential material — and only now, with a verified backup.
    with_progress(|p| p.set("remove", StepState::Running, None));
    for f in &files {
        sudo_ok(&["rm", "-f", f])
            .await
            .with_context(|| format!("removing {f} — restore from {backup}"))?;
    }
    with_progress(|p| {
        p.set(
            "remove",
            StepState::Done,
            Some(format!("{} file(s) removed", files.len())),
        )
    });

    // 5. Start, and unlock. The unlock is explicit rather than left to the next
    //    reconcile tick: LND does not mint macaroons until the wallet opens, so
    //    without this the rotation would sit waiting for a file that cannot
    //    appear. `_no_wipe` keeps the destructive recovery path out of reach.
    with_progress(|p| p.set("start", StepState::Running, None));
    start_lnd().await.with_context(|| {
        format!("starting LND after removing its macaroons — old material is in {backup}")
    })?;
    crate::container::lnd::unlock_existing_wallet_no_wipe()
        .await
        .with_context(|| format!("unlocking the wallet — old material is in {backup}"))?;

    let mint_deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(MACAROON_WAIT_SECS);
    let admin_path = format!("{LND_MAINNET_DIR}/admin.macaroon");
    let mut new_digest = None;
    while std::time::Instant::now() < mint_deadline {
        if let Some(d) = digest_as_root(&admin_path).await {
            new_digest = Some(d);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
    let Some(new_digest) = new_digest else {
        with_progress(|p| p.set("start", StepState::Failed, None));
        anyhow::bail!(
            "LND did not mint a new admin.macaroon within {} minutes. The old material is intact \
             in {backup} — restore it there and investigate before retrying",
            MACAROON_WAIT_SECS / 60
        );
    };
    with_progress(|p| {
        p.new_admin_macaroon_sha256 = Some(new_digest.clone());
        p.set(
            "start",
            StepState::Done,
            Some("Lightning is up with freshly minted credentials".into()),
        );
    });

    // 6. Verify the things that must NOT have changed.
    //
    // Its own budget, deliberately not the mint deadline. Sharing one would mean
    // a rotation that legitimately spent 14 of its 15 minutes waiting for LND to
    // mint gets 1 minute to prove the channels came back, and then reports
    // FAILURE on a node that is perfectly healthy — the most alarming possible
    // way to be wrong about someone's Lightning wallet.
    with_progress(|p| p.set("verify", StepState::Running, None));
    let verify_deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(MACAROON_WAIT_SECS);
    let (pubkey_after, census_after) =
        wait_for_serving(verify_deadline).await.with_context(|| {
            format!("verifying the node after rotation — old material is in {backup}")
        })?;
    with_progress(|p| p.channels_after = Some(census_after.channels_open));
    if pubkey_after != pubkey_before {
        with_progress(|p| p.set("verify", StepState::Failed, None));
        anyhow::bail!(
            "NODE IDENTITY CHANGED — this is not the same Lightning node. Old material is in \
             {backup}. Do not use this node until you understand why"
        );
    }
    if census_after.channels_open != census_before.channels_open
        || census_after.channels_pending != census_before.channels_pending
    {
        with_progress(|p| p.set("verify", StepState::Failed, None));
        anyhow::bail!(
            "channel count changed ({} open/{} pending before, {} open/{} pending after). Old \
             material is in {backup}",
            census_before.channels_open,
            census_before.channels_pending,
            census_after.channels_open,
            census_after.channels_pending
        );
    }
    with_progress(|p| {
        p.set(
            "verify",
            StepState::Done,
            Some(format!(
                "same node, same {} channel(s)",
                census_after.channels_open
            )),
        )
    });

    // 7. BTCPay. Its connection string embeds the macaroon inline and cannot
    //    self-heal — see `rewrite_btcpay_lnd_connection_secret`. Left undone,
    //    the node looks healthy while every Lightning invoice BTCPay creates
    //    fails, which is precisely the failure this step exists to prevent.
    with_progress(|p| p.set("btcpay", StepState::Running, None));
    match crate::container::lnd::rewrite_btcpay_lnd_connection_secret(std::path::Path::new(
        SECRETS_DIR,
    ))
    .await
    {
        Ok(true) => {
            // Writing the secret is only half of it. btcpay-server is
            // restart-sensitive, so reconcile sees the drift and deliberately
            // leaves the running container alone — which would strand it on the
            // dead macaroon indefinitely. This is the flag that overrides that,
            // and without it this whole step is cosmetic.
            match &orchestrator {
                Some(orch) => {
                    orch.mark_credential_rotated("btcpay-server").await;
                    with_progress(|p| {
                        p.set(
                            "btcpay",
                            StepState::Done,
                            Some(
                                "Connection string updated. BTCPay restarts itself within a \
                                 minute or two to pick it up."
                                    .into(),
                            ),
                        )
                    });
                }
                // Only reachable in builds without an orchestrator (tests). Say
                // what is left for a human rather than implying it is handled.
                None => with_progress(|p| {
                    p.set(
                        "btcpay",
                        StepState::Skipped,
                        Some(
                            "Connection string updated, but no orchestrator is available to \
                             restart BTCPay — restart it yourself to pick up the new credentials."
                                .into(),
                        ),
                    )
                }),
            }
        }
        Ok(false) => with_progress(|p| {
            p.set(
                "btcpay",
                StepState::Skipped,
                Some("No internal Lightning node is configured for BTCPay on this node.".into()),
            )
        }),
        // Not fatal: the macaroons ARE rotated by this point, and reporting the
        // whole rotation as failed would be a lie that invites a needless retry.
        // Say exactly what is left undone instead.
        Err(e) => {
            tracing::warn!(error = %format!("{e:#}"), "btcpay connection string not updated after macaroon rotation");
            with_progress(|p| {
                p.set(
                    "btcpay",
                    StepState::Failed,
                    Some(format!(
                        "Your macaroons ARE rotated, but BTCPay's stored copy could not be \
                         updated, so its Lightning payments will fail until it is: {e:#}"
                    )),
                )
            })
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_starts_with_every_step_pending() {
        let p = RotationProgress::default();
        assert_eq!(p.steps.len(), STEPS.len());
        assert!(p.steps.iter().all(|s| s.state == StepState::Pending));
        assert!(!p.running);
        assert!(p.ok.is_none());
    }

    #[test]
    fn set_updates_only_the_named_step() {
        let mut p = RotationProgress::default();
        p.set("stop", StepState::Done, Some("stopped".into()));
        let stop = p.steps.iter().find(|s| s.key == "stop").unwrap();
        assert_eq!(stop.state, StepState::Done);
        assert_eq!(stop.detail.as_deref(), Some("stopped"));
        assert!(p
            .steps
            .iter()
            .filter(|s| s.key != "stop")
            .all(|s| s.state == StepState::Pending));
    }

    #[test]
    fn set_on_an_unknown_step_is_a_no_op_not_a_panic() {
        let mut p = RotationProgress::default();
        p.set("not-a-step", StepState::Failed, None);
        assert!(p.steps.iter().all(|s| s.state == StepState::Pending));
    }

    /// A detail is informational; passing `None` must not wipe one already set,
    /// or a later state transition would erase the explanation the operator is
    /// reading.
    #[test]
    fn set_without_a_detail_keeps_the_existing_one() {
        let mut p = RotationProgress::default();
        p.set("btcpay", StepState::Running, Some("working".into()));
        p.set("btcpay", StepState::Done, None);
        let step = p.steps.iter().find(|s| s.key == "btcpay").unwrap();
        assert_eq!(step.state, StepState::Done);
        assert_eq!(step.detail.as_deref(), Some("working"));
    }

    /// The serialized shape is a UI contract: the frontend renders `state`
    /// as a lowercase discriminant.
    #[test]
    fn step_states_serialize_lowercase() {
        let json = serde_json::to_string(&StepState::Skipped).unwrap();
        assert_eq!(json, "\"skipped\"");
    }

    /// Macaroon *content* must never reach the progress feed the UI polls.
    #[test]
    fn progress_carries_digests_not_tokens() {
        let mut p = RotationProgress::default();
        p.new_admin_macaroon_sha256 = Some("a".repeat(64));
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("new_admin_macaroon_sha256"));
        assert!(!json.contains("macaroon_hex"));
    }
}
