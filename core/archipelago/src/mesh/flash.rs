// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Firmware flashing for LoRa mesh radios — Heltec V3/V4 in v1, across all
//! three firmware families the mesh module already knows how to detect (see
//! `mesh::types::DeviceType`). Firmware is always fetched from upstream at
//! flash time (never bundled/pinned in the repo), and every flash defaults
//! to a full chip erase before write.
//!
//! MeshCore and Meshtastic are flashed the same way: download a released
//! image, verify it, then run `write_flash --erase-all 0x0 <image>` with
//! the packaged esptool executable.
//! Reticulum/RNode is different: `archy-rnodeconf --autoinstall` owns the
//! whole fetch+erase+flash+EEPROM-bootstrap sequence itself (confirmed live
//! via `archy-rnodeconf --help` — there is no raw esptool path exposed for
//! this family, so we deliberately don't resolve a firmware URL ourselves
//! for Reticulum; rnodeconf already knows how).

use super::serial::DetectedDeviceInfo;
use super::types::DeviceType;
use super::MeshService;
use anyhow::{Context, Result};
use regex::Regex;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Boards supported for v1. Both are ESP32-S3 (a single `--chip esp32s3`
/// esptool target covers both), but ship different USB identities and
/// different per-board firmware assets upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FlashBoard {
    HeltecV3,
    HeltecV4,
}

impl FlashBoard {
    /// Meshtastic's board id (matches the release manifest's `board` field
    /// and its per-board asset naming, e.g. `firmware-heltec-v3-<ver>.factory.bin`).
    fn meshtastic_id(self) -> &'static str {
        match self {
            Self::HeltecV3 => "heltec-v3",
            Self::HeltecV4 => "heltec-v4",
        }
    }
}

/// Generic CP2102 and native ESP32-S3 USB IDs identify adapters/chips, not
/// board wiring. Require an explicit board until a board-specific identity is
/// available; guessing a Heltec model can write incompatible firmware.
pub fn resolve_flash_board(_info: &DetectedDeviceInfo) -> Option<FlashBoard> {
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FlashStage {
    Downloading,
    Preparing,
    Erasing,
    Writing,
    Autoinstalling,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlashJobStatus {
    pub board: FlashBoard,
    pub family: DeviceType,
    pub path: String,
    pub stage: FlashStage,
    pub percent: Option<u8>,
    pub log_tail: Vec<String>,
    pub done: bool,
    pub error: Option<String>,
}

const LOG_TAIL_MAX: usize = 200;

/// How long to wait after a successful flash before resuming the mesh
/// listener, so the board finishes its own post-flash boot/reset before we
/// start opening the port (which itself toggles DTR/RTS) again.
const POST_FLASH_SETTLE_DELAY: std::time::Duration = std::time::Duration::from_secs(5);

/// Deadline for preparation and warning threshold for the active flasher.
/// A download can be cancelled safely. An active write retains ownership until
/// its subprocess exits, even after this threshold: reporting an aborted job
/// while a detached writer continues would let a retry corrupt the device.
const MAX_JOB_DURATION: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// How long to wait for MeshService::stop() to release the serial port
/// before giving up. Confirmed live 2026-07-23: the listener's own
/// reconnect/multi-candidate-probe loop doesn't check its shutdown signal
/// between candidates, so stop() can take a while (or, if the loop is
/// wedged, never return) — 20s comfortably covers a normal handshake-probe
/// cycle without leaving a flash request hanging indefinitely if the
/// listener genuinely won't let go.
const STOP_LISTENER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// How long to keep retrying the port-free check before giving up.
const PORT_FREE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Confirm nothing else has `path` open by actually opening (and immediately
/// closing) it ourselves. Retries across the timeout since a just-stopped
/// listener's fd can take a moment to actually release even after `stop()`
/// returns (task abort is a request, not an instant guarantee the OS-level
/// resource is gone yet).
async fn wait_for_port_free(path: &str) -> Result<()> {
    let deadline = tokio::time::Instant::now() + PORT_FREE_TIMEOUT;
    let mut last_err = None;
    loop {
        match serial2_tokio::SerialPort::open(path, 115200) {
            Ok(_) => return Ok(()),
            Err(e) => last_err = Some(e),
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Err(anyhow::anyhow!(
        "{path} is still held open by something else after {}s (last error: {}) — refusing to start the flasher against a contended port",
        PORT_FREE_TIMEOUT.as_secs(),
        last_err.map(|e| e.to_string()).unwrap_or_default()
    ))
}

/// Live state for the one flash job that can run at a time. A single global
/// slot is sufficient because flashing needs exclusive serial access to the
/// one port being flashed — there is no meaningful concept of two concurrent
/// flash jobs on this node.
pub struct FlashJob {
    status: RwLock<FlashJobStatus>,
    /// Set once the background task is spawned. Only used while `stage` is
    /// still `Downloading` — an interrupted erase/write can leave the chip
    /// in a worse state than either finished or unstarted, so cancellation
    /// is refused once erase begins (see `cancel()`).
    abort_handle: RwLock<Option<tokio::task::AbortHandle>>,
}

impl FlashJob {
    fn new(board: FlashBoard, family: DeviceType, path: String) -> Arc<Self> {
        Arc::new(Self {
            abort_handle: RwLock::new(None),
            status: RwLock::new(FlashJobStatus {
                board,
                family,
                path,
                stage: FlashStage::Downloading,
                percent: None,
                log_tail: Vec::new(),
                done: false,
                error: None,
            }),
        })
    }

    pub async fn snapshot(&self) -> FlashJobStatus {
        self.status.read().await.clone()
    }

    async fn set_stage(&self, stage: FlashStage) {
        let mut s = self.status.write().await;
        s.stage = stage;
        s.percent = None;
    }

    async fn set_percent(&self, percent: u8) {
        self.status.write().await.percent = Some(percent.min(100));
    }

    async fn push_log(&self, line: impl Into<String>) {
        let mut s = self.status.write().await;
        s.log_tail.push(line.into());
        let overflow = s.log_tail.len().saturating_sub(LOG_TAIL_MAX);
        if overflow > 0 {
            s.log_tail.drain(0..overflow);
        }
    }

    async fn fail(&self, err: &anyhow::Error) {
        let mut s = self.status.write().await;
        s.stage = FlashStage::Failed;
        s.error = Some(format!("{err:#}"));
        s.done = true;
    }

    async fn finish(&self) {
        let mut s = self.status.write().await;
        s.stage = FlashStage::Done;
        s.done = true;
    }

    /// Best-effort cancel: only honored before erase/write/autoinstall has
    /// started (i.e. still in `Downloading`). Once a stage that touches the
    /// chip begins, this refuses — interrupting an erase or write can leave
    /// the flash in a state worse than either finished or unstarted.
    pub async fn cancel(&self) -> Result<()> {
        let mut s = self.status.write().await;
        if s.done {
            anyhow::bail!("Flash job already finished");
        }
        if s.stage != FlashStage::Downloading {
            anyhow::bail!(
                "Cannot cancel once {:?} has started — let it finish or fail on its own",
                s.stage
            );
        }
        if let Some(handle) = self.abort_handle.write().await.take() {
            handle.abort();
        }
        s.stage = FlashStage::Failed;
        s.error = Some("Cancelled by user".to_string());
        s.done = true;
        Ok(())
    }
}

/// Shared handle held by `RpcHandler`, sibling to `mesh_service`.
pub type FlashJobHandle = Arc<RwLock<Option<Arc<FlashJob>>>>;

pub fn new_job_handle() -> FlashJobHandle {
    Arc::new(RwLock::new(None))
}

fn firmware_cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("mesh").join("firmware-cache")
}

async fn invalidate_radio_settings_marker(data_dir: &Path) -> Result<()> {
    // A full-chip flash erased the device's preferences. A previous host-side
    // marker is no longer evidence that this radio has the requested settings.
    match tokio::fs::remove_file(data_dir.join("meshcore-radio-params.json")).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("Firmware written, but old radio-settings marker could not be cleared; reconnect is paused"),
    }
}

/// No blanket `.timeout()` here on purpose: reqwest's request timeout covers
/// the *entire* request including streaming the response body, which would
/// kill a legitimate large download partway through (Meshtastic's esp32s3
/// zip is ~170MB) — not just a hung connection. `download_to_file` instead
/// applies a per-chunk stall timeout, and metadata calls (small JSON
/// responses) get their own short timeout at the call site.
fn github_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("archipelago-mesh-flash")
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .context("Failed to build HTTP client")
}

/// Applied per-chunk while streaming a firmware download — if the transfer
/// stalls (no bytes for this long) it's treated as a failure, but a slow
/// download that's still making progress is never killed just for taking a
/// while.
const DOWNLOAD_STALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Applied to metadata calls (GitHub release JSON) — these are small
/// responses with no reason to ever take this long.
const METADATA_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Resolve what firmware is available for a board+family. v1 only ever
/// offers "latest" — MeshCore/Meshtastic latest GitHub release, or, for
/// Reticulum, "latest" meaning "whatever archy-rnodeconf --autoinstall
/// resolves on its own" (it does its own version checking upstream).
pub async fn list_firmware(family: DeviceType) -> Result<Vec<String>> {
    match family {
        DeviceType::Reticulum => Ok(vec!["latest".to_string()]),
        DeviceType::Meshtastic => {
            let client = github_client()?;
            let release: GithubRelease = client
                .get("https://api.github.com/repos/meshtastic/firmware/releases/latest")
                .send()
                .await
                .context("Fetching Meshtastic release list")?
                .error_for_status()
                .context("Meshtastic releases API error")?
                .json()
                .await
                .context("Parsing Meshtastic release JSON")?;
            Ok(vec![release.tag_name])
        }
        DeviceType::Meshcore => {
            let client = github_client()?;
            let release: GithubRelease = client
                .get("https://api.github.com/repos/meshcore-dev/MeshCore/releases/latest")
                .send()
                .await
                .context("Fetching MeshCore release list")?
                .error_for_status()
                .context("MeshCore releases API error")?
                .json()
                .await
                .context("Parsing MeshCore release JSON")?;
            Ok(vec![release.tag_name])
        }
        DeviceType::Unknown => anyhow::bail!("Pick a firmware family before listing versions"),
    }
}

#[derive(serde::Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(serde::Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

async fn register_flash_job(handle: &FlashJobHandle, job: &Arc<FlashJob>) -> Result<()> {
    // Keep checking and registering under one lock: concurrent RPCs must
    // never start two writers on the same device.
    let mut existing = handle.try_write().map_err(|_| anyhow::anyhow!(
            "The radio is being inspected or reconfigured; wait for that operation to finish before flashing"
        ))?;
    if let Some(current) = existing.as_ref() {
        if !current.snapshot().await.done {
            anyhow::bail!("A firmware flash is already in progress on this node");
        }
    }
    *existing = Some(Arc::clone(job));
    Ok(())
}

/// Start a flash job in the background. Returns as soon as the job has been
/// registered — preparation and listener shutdown run in the background.
/// Callers poll `FlashJobHandle` via
/// `mesh.flash-status` for progress. Only one job may be in flight at a time.
pub async fn start_flash_job(
    handle: &FlashJobHandle,
    mesh_service: &Arc<RwLock<Option<MeshService>>>,
    data_dir: PathBuf,
    path: String,
    board: FlashBoard,
    family: DeviceType,
) -> Result<()> {
    // Missing/corrupt tooling must fail before stopping a working radio or
    // registering a job that can never reach the serial device.
    if matches!(family, DeviceType::Meshtastic | DeviceType::Meshcore) {
        preflight_esptool().await?;
    }
    let job = FlashJob::new(board, family, path.clone());
    register_flash_job(handle, &job).await?;

    let bg_job = Arc::clone(&job);
    let bg_service = Arc::clone(mesh_service);
    let task = tokio::spawn(async move {
        // Downloads and checksum checks do not need exclusive serial access.
        // Keep a working listener alive if upstream/download verification fails.
        let prepared_image = if matches!(family, DeviceType::Meshcore | DeviceType::Meshtastic) {
            match tokio::time::timeout(
                MAX_JOB_DURATION,
                fetch_esptool_image(board, family, &data_dir, &bg_job),
            )
            .await
            {
                Ok(Ok(image)) => Some(image),
                result => {
                    let error = match result {
                        Ok(Err(error)) => error,
                        _ => anyhow::anyhow!(
                            "Firmware download exceeded the time limit; radio was not changed"
                        ),
                    };
                    bg_job.fail(&error).await;
                    return;
                }
            }
        } else {
            None
        };
        // Cancellation is safe during download. Once listener shutdown starts,
        // allow that bounded operation to finish instead of orphaning its task.
        bg_job.set_stage(FlashStage::Preparing).await;
        // esptool/archy-rnodeconf need exclusive serial access — release
        // the listener's hold on the port before touching it. This USED
        // TO run synchronously in start_flash_job before the job was even
        // spawned, blocking the RPC call itself on s.stop().await — a real
        // 2026-07-23 incident: the mesh listener was mid a multi-candidate
        // reconnect/probe sequence that doesn't check its shutdown signal
        // between candidates, so stop() never returned. The HTTP request
        // timed out client-side ("Operation failed"), while the job
        // (already inserted into `handle`) was permanently wedged — nothing
        // had been spawned yet to ever mark it done, so every later flash
        // attempt failed with "already in progress" until a full restart.
        // Now this runs inside the spawned task with its own bounded
        // timeout, so the RPC call always returns immediately regardless,
        // and a slow-to-stop listener fails the job cleanly instead of
        // hanging everything downstream of it forever.
        let stop_result = tokio::time::timeout(STOP_LISTENER_TIMEOUT, async {
            let mut svc = bg_service.write().await;
            if let Some(s) = svc.as_mut() {
                s.stop().await;
            }
        })
        .await;
        if stop_result.is_err() {
            let err = anyhow::anyhow!(
                "Mesh listener did not release the serial port within {}s — it may still be mid a reconnect attempt. Try again once mesh.status shows the device idle, or restart the archipelago service if this persists.",
                STOP_LISTENER_TIMEOUT.as_secs()
            );
            bg_job.push_log(format!("ERROR: {err:#}")).await;
            bg_job.fail(&err).await;
            return;
        }

        // Belt-and-suspenders port-free check. `stop()` above should have
        // fully released the port, but esptool/rnodeconf run as external
        // subprocesses for minutes outside our own async runtime — if
        // ANYTHING else still has it open (a racing probe, a not-yet-dropped
        // fd from an aborted task, anything we haven't anticipated), handing
        // the port to the flasher anyway risks exactly the corruption
        // confirmed live 2026-07-23: esptool's "device disconnected or
        // multiple access on port?" and rnodeconf's raw `OSError: [Errno 71]
        // Protocol error` on an RTS ioctl are both textbook two-openers-on-
        // one-fd symptoms. Verify by actually opening it ourselves — cheap,
        // and definitive — before ever starting the flasher.
        if let Err(e) = wait_for_port_free(&path).await {
            bg_job.push_log(format!("ERROR: {e:#}")).await;
            bg_job.fail(&e).await;
            return;
        }

        // Outer ceiling on top of run_flash's own internal timeouts —
        // belt-and-suspenders so that no future hang (network, subprocess,
        // anything) can ever wedge the single-flash-job guard permanently
        // again the way a stuck download did on 2026-07-23 (every
        // subsequent mesh.flash-device call failed with "already in
        // progress" until the service was restarted). Generous enough that
        // a legitimately slow multi-hundred-MB transfer still completes.
        let flash = run_flash(
            board,
            family,
            &data_dir,
            &path,
            prepared_image.as_deref(),
            &bg_job,
        );
        tokio::pin!(flash);
        let result = match tokio::time::timeout(MAX_JOB_DURATION, &mut flash).await {
            Ok(inner) => inner,
            Err(_) if bg_job.snapshot().await.stage == FlashStage::Downloading => Err(
                anyhow::anyhow!("Firmware download exceeded the time limit; radio was not written"),
            ),
            Err(_) => {
                // Dropping this future does NOT stop its flash subprocess.
                // Keep the job busy until it exits, rather than allowing a
                // second writer while the first one still owns the device.
                bg_job.push_log("Flashing is taking longer than expected; waiting for the active tool to exit before allowing another operation").await;
                flash.await
            }
        };
        let result = match result {
            Ok(()) => invalidate_radio_settings_marker(&data_dir).await,
            error => error,
        };
        let succeeded = result.is_ok();

        match &result {
            Ok(()) => {
                bg_job
                    .push_log("Flash completed successfully".to_string())
                    .await;
                info!(path = %path, board = ?board, family = %family, "LoRa firmware flash succeeded");
            }
            Err(e) => {
                // {:#} (alternate Display) walks the full anyhow context
                // chain — plain {} / %e only prints the outermost .context()
                // message, which made a real 2026-07-23 esptool failure
                // undiagnosable from journalctl alone (just "esptool
                // erase_flash failed", no actual esptool stderr).
                warn!(path = %path, error = %format!("{e:#}"), "LoRa firmware flash failed");
                bg_job.push_log(format!("ERROR: {e:#}")).await;
            }
        }

        // The board's firmware may now differ from whatever was pinned
        // before — clear the pin either way so a later reconnect's strict
        // auto-detect order picks up reality instead of getting wedged
        // trying the old protocol first.
        if let Ok(mut config) = super::load_config(&data_dir).await {
            config.device_kind = None;
            if let Err(e) = super::save_config(&data_dir, &config).await {
                warn!(error = %e, "Failed to clear device_kind pin after flash");
            }
        }

        if !succeeded {
            if let Err(error) = &result {
                bg_job.fail(error).await;
            }
            // Deliberately do NOT auto-restart the listener here. A failed
            // flash means we can't vouch for the board's state — reopening
            // the port immediately (esptool/rnodeconf's own reset sequence
            // plus our open() toggling DTR/RTS again right after) risks
            // hammering a marginal device with reconnect attempts. Confirmed
            // live 2026-07-23: exactly this sequence left a real Heltec V3
            // boot-looping for 5+ minutes after a failed flash. Leave mesh
            // stopped; the user reconnects explicitly via the hot-swap
            // modal/Mesh page once they've confirmed the board is alive.
            warn!(
                path = %path,
                "Leaving mesh listener stopped after failed flash — reconnect manually once the board is confirmed responsive"
            );
            return;
        }

        // On success, give the board a moment to finish booting after the
        // flash tool's own reset sequence before we start hammering it with
        // connection attempts — same reasoning as above, just the
        // lower-risk (successful-flash) side of it.
        tokio::time::sleep(POST_FLASH_SETTLE_DELAY).await;

        let mut svc = bg_service.write().await;
        if let Some(s) = svc.as_mut() {
            match super::load_config(&data_dir).await {
                Ok(config) => {
                    // Only resume if mesh is actually still enabled per the
                    // CURRENT persisted config — confirmed live 2026-07-23:
                    // unconditionally forcing a restart here, regardless of
                    // `enabled`, overrode a user's own concurrent "disable
                    // mesh" toggle and left the listener running while
                    // config said disabled. That inconsistent state is what
                    // made a later legitimate "Keep As Is" click (which
                    // correctly tries to start on a false→true transition)
                    // fail with "already running" — the listener had already
                    // been force-started behind the config's back.
                    let should_run = config.enabled;
                    if let Err(e) = s.configure(config).await {
                        warn!(error = %e, "Failed to resume mesh listener after flash");
                    }
                    if should_run {
                        if let Err(e) = s.start() {
                            warn!(error = %e, "Failed to restart mesh listener after flash");
                        }
                    }
                }
                Err(e) => warn!(error = %e, "Failed to load mesh config after flash"),
            }
        }
        bg_job.finish().await;
    });
    *job.abort_handle.write().await = Some(task.abort_handle());

    Ok(())
}

async fn run_flash(
    board: FlashBoard,
    family: DeviceType,
    data_dir: &Path,
    path: &str,
    prepared_image: Option<&Path>,
    job: &Arc<FlashJob>,
) -> Result<()> {
    match family {
        DeviceType::Meshtastic | DeviceType::Meshcore => {
            let image = prepared_image.context("Firmware was not prepared before serial access")?;
            esptool_erase_and_write(path, image, job).await
        }
        DeviceType::Reticulum => {
            let lora_region = super::load_config(data_dir)
                .await
                .ok()
                .and_then(|c| c.lora_region);
            rnodeconf_autoinstall(path, board, lora_region.as_deref(), job).await
        }
        DeviceType::Unknown => anyhow::bail!("Pick a firmware family before flashing"),
    }
}

// ─── MeshCore / Meshtastic: esptool ─────────────────────────────────────

async fn fetch_esptool_image(
    board: FlashBoard,
    family: DeviceType,
    data_dir: &Path,
    job: &Arc<FlashJob>,
) -> Result<PathBuf> {
    let cache = firmware_cache_dir(data_dir);
    tokio::fs::create_dir_all(&cache)
        .await
        .context("Creating firmware cache dir")?;
    let client = github_client()?;

    match family {
        DeviceType::Meshtastic => fetch_meshtastic_image(&client, board, &cache, job).await,
        DeviceType::Meshcore => fetch_meshcore_image(&client, board, &cache, job).await,
        _ => anyhow::bail!("{family} is not flashed via esptool"),
    }
}

async fn fetch_meshtastic_image(
    client: &reqwest::Client,
    board: FlashBoard,
    cache: &Path,
    job: &Arc<FlashJob>,
) -> Result<PathBuf> {
    let release: GithubRelease = client
        .get("https://api.github.com/repos/meshtastic/firmware/releases/latest")
        .timeout(METADATA_TIMEOUT)
        .send()
        .await
        .context("Fetching Meshtastic release list")?
        .error_for_status()
        .context("Meshtastic releases API error")?
        .json()
        .await
        .context("Parsing Meshtastic release JSON")?;

    // Meshtastic bundles all esp32s3 boards' images inside one per-platform
    // zip rather than shipping per-board top-level assets — both Heltec V3
    // and V4 are esp32s3, so this is the right zip for both (confirmed live
    // against v2.7.26.54e0d8d).
    let zip_asset = release
        .assets
        .iter()
        .find(|a| a.name.starts_with("firmware-esp32s3-") && a.name.ends_with(".zip"))
        .ok_or_else(|| anyhow::anyhow!("No esp32s3 firmware zip in latest Meshtastic release"))?;

    let version = zip_asset
        .name
        .strip_prefix("firmware-esp32s3-")
        .and_then(|s| s.strip_suffix(".zip"))
        .ok_or_else(|| anyhow::anyhow!("Unexpected Meshtastic asset name: {}", zip_asset.name))?
        .to_string();

    let zip_path = cache.join(&zip_asset.name);
    if tokio::fs::metadata(&zip_path).await.is_err() {
        download_to_file(client, &zip_asset.browser_download_url, &zip_path, job).await?;
    } else {
        job.push_log(format!("Using cached {}", zip_asset.name))
            .await;
    }

    // "*.factory.bin" is Meshtastic's full merged image (bootloader +
    // partition table + app) meant to be written at offset 0x0 on a freshly
    // erased chip — confirmed by inspecting the real zip's contents, as
    // opposed to the plain "*.bin" OTA-update image which assumes an
    // existing bootloader/partition table already on the chip.
    let entry_name = format!("firmware-{}-{}.factory.bin", board.meshtastic_id(), version);
    let out_path = cache.join(&entry_name);
    if tokio::fs::metadata(&out_path).await.is_ok() {
        return Ok(out_path);
    }

    job.push_log(format!("Extracting {entry_name} from {}", zip_asset.name))
        .await;
    let zip_path_owned = zip_path.clone();
    let entry_name_owned = entry_name.clone();
    let out_path_owned = out_path.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let file =
            std::fs::File::open(&zip_path_owned).context("Opening downloaded firmware zip")?;
        let mut archive = zip::ZipArchive::new(file).context("Reading firmware zip")?;
        let mut entry = archive
            .by_name(&entry_name_owned)
            .with_context(|| format!("{entry_name_owned} not found in firmware zip"))?;
        let mut out =
            std::fs::File::create(&out_path_owned).context("Creating extracted firmware file")?;
        std::io::copy(&mut entry, &mut out).context("Extracting firmware image")?;
        Ok(())
    })
    .await
    .context("Firmware extraction task panicked")??;

    Ok(out_path)
}

async fn fetch_meshcore_image(
    client: &reqwest::Client,
    board: FlashBoard,
    cache: &Path,
    job: &Arc<FlashJob>,
) -> Result<PathBuf> {
    let release: GithubRelease = client
        .get("https://api.github.com/repos/meshcore-dev/MeshCore/releases/latest")
        .timeout(METADATA_TIMEOUT)
        .send()
        .await
        .context("Fetching MeshCore release list")?
        .error_for_status()
        .context("MeshCore releases API error")?
        .json()
        .await
        .context("Parsing MeshCore release JSON")?;

    // Upstream's casing differs between boards (Heltec_v3_... vs
    // heltec_v4_...) — match case-insensitively on the exact per-board
    // substring so V4 isn't accidentally matched by "heltec_v4_tft_..."
    // variants (there's a "_tft_" in between, so a straight substring match
    // on "heltec_v4_companion_radio_usb" is already safe).
    let needle = match board {
        FlashBoard::HeltecV3 => "heltec_v3_companion_radio_usb",
        FlashBoard::HeltecV4 => "heltec_v4_companion_radio_usb",
    };
    let asset = release
        .assets
        .iter()
        .find(|a| {
            let lower = a.name.to_lowercase();
            lower.contains(needle) && lower.ends_with("-merged.bin")
        })
        .ok_or_else(|| {
            anyhow::anyhow!("No matching MeshCore image in release {}", release.tag_name)
        })?;

    anyhow::ensure!(
        Path::new(&asset.name)
            .file_name()
            .and_then(|name| name.to_str())
            == Some(asset.name.as_str()),
        "Invalid firmware asset filename"
    );
    let out_path = cache.join(&asset.name);
    if tokio::fs::metadata(&out_path).await.is_ok() {
        if verify_meshcore_asset(&out_path, asset).await.is_ok() {
            job.push_log(format!("Using verified cached {}", asset.name))
                .await;
            return Ok(out_path);
        }
        tokio::fs::remove_file(&out_path)
            .await
            .context("Removing invalid cached firmware")?;
        job.push_log("Cached firmware failed verification; downloading a fresh copy")
            .await;
    }
    download_to_file(client, &asset.browser_download_url, &out_path, job).await?;
    if let Err(error) = verify_meshcore_asset(&out_path, asset).await {
        // A partial/unverified download must not become next attempt's cache.
        let _ = tokio::fs::remove_file(&out_path).await;
        return Err(error);
    }
    Ok(out_path)
}

async fn verify_meshcore_asset(path: &Path, asset: &GithubAsset) -> Result<()> {
    use sha2::{Digest, Sha256};
    let size = asset
        .size
        .context("MeshCore release did not provide firmware size")?;
    anyhow::ensure!(
        (1..=16 * 1024 * 1024).contains(&size),
        "Invalid MeshCore firmware size"
    );
    anyhow::ensure!(
        tokio::fs::metadata(path).await?.len() == size,
        "Firmware size mismatch; radio was not written"
    );
    let expected = asset
        .digest
        .as_deref()
        .and_then(|value| value.strip_prefix("sha256:"))
        .context("MeshCore release did not provide a SHA-256 checksum")?;
    anyhow::ensure!(
        expected.len() == 64 && expected.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Invalid MeshCore release checksum"
    );
    let actual = hex::encode(Sha256::digest(tokio::fs::read(path).await?));
    anyhow::ensure!(
        actual.eq_ignore_ascii_case(expected),
        "Firmware checksum mismatch; radio was not written"
    );
    Ok(())
}

async fn download_to_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    job: &Arc<FlashJob>,
) -> Result<()> {
    job.set_stage(FlashStage::Downloading).await;
    // Bound only the wait for the response to *start* (headers) — NOT a
    // request-level `.timeout()`, which would cap the whole body transfer
    // again (the bug this replaced: a blanket 30s client timeout killed
    // large downloads mid-stream). If the server never responds at all,
    // this is what stops the job from hanging forever; the per-chunk stall
    // timeout below is what guards the body once streaming starts. Without
    // this, a server that accepts the TCP connection but never sends
    // headers back hangs this call indefinitely — confirmed live
    // 2026-07-23: a stuck `.send()` here wedged the single-flash-job guard
    // for good, permanently blocking every subsequent flash attempt with
    // "already in progress" until the service was restarted.
    let resp = tokio::time::timeout(METADATA_TIMEOUT, client.get(url).send())
        .await
        .context("Firmware download server did not respond")?
        .context("Starting firmware download")?
        .error_for_status()
        .context("Firmware download returned an error status")?;
    let total = resp.content_length();
    let tmp = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .context("Creating firmware download file")?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    use futures_util::StreamExt;
    loop {
        let next = tokio::time::timeout(DOWNLOAD_STALL_TIMEOUT, stream.next())
            .await
            .context("Firmware download stalled")?;
        let Some(chunk) = next else { break };
        let chunk = chunk.context("Reading firmware download stream")?;
        file.write_all(&chunk)
            .await
            .context("Writing firmware download")?;
        downloaded += chunk.len() as u64;
        if let Some(total) = total {
            if total > 0 {
                job.set_percent(((downloaded.saturating_mul(100)) / total) as u8)
                    .await;
            }
        }
    }
    file.flush().await.context("Flushing firmware download")?;
    file.sync_all().await.context("Saving firmware download")?;
    tokio::fs::rename(&tmp, dest)
        .await
        .context("Finalizing firmware download")?;
    job.push_log(format!(
        "Downloaded {} ({downloaded} bytes)",
        dest.display()
    ))
    .await;
    Ok(())
}

/// Both Heltec V3 and V4 are ESP32-S3 boards.
const ESPTOOL_CHIP: &str = "esp32s3";

/// esptool's auto-reset-into-bootloader handshake (toggling DTR/RTS in a
/// specific timed pattern) is well-known to be flaky on some CP2102/CH340
/// board+adapter combinations — esptool's own docs recommend retrying at a
/// lower baud rate when this happens. Rather than fail the whole job on the
/// first hiccup, retry once at a conservative baud before giving up.
const ESPTOOL_FALLBACK_BAUD: &str = "115200";

/// `write_flash --erase-all` erases the whole chip before writing, in one
/// esptool invocation. This needs the esp32s3 stub flasher loaded (see
/// esptool_global_args' doc comment) — without it, --erase-all hits the
/// exact same ROM limitation a standalone `erase_flash` does ("ESP32-S3 ROM
/// does not support function erase_flash", confirmed live 2026-07-23), since
/// esptool's --erase-all is implemented as the same full-chip-erase command,
/// not a per-sector loop.
async fn esptool_erase_and_write(path: &str, image: &Path, job: &Arc<FlashJob>) -> Result<()> {
    job.set_stage(FlashStage::Writing).await;
    let image_str = image.to_string_lossy().to_string();
    esptool_with_retry(
        path,
        &["write_flash", "--erase-all", "0x0", &image_str],
        job,
    )
    .await
    .context("esptool write_flash failed")?;

    Ok(())
}

/// esptool's global flags (--chip/--port/--baud) MUST precede the subcommand
/// token (erase_flash/write_flash/...) — confirmed live 2026-07-23:
/// appending `--baud 115200` after the subcommand on the retry path
/// produced "esptool: error: unrecognized arguments: --baud 115200" every
/// time, so the fallback-baud retry never actually got a chance to run.
/// Building global args separately from subcommand args keeps this correct
/// by construction instead of relying on call-site ordering.
///
/// Normal stub-loader mode is required for full-chip erase. The packaged
/// archy-esptool includes and self-tests Espressif's ESP32-S3 stub. Debian's
/// stripped esptool package alone did not provide that resource, so changing
/// flags to --no-stub cannot repair this operation.
fn esptool_global_args<'a>(path: &'a str, baud: Option<&'a str>) -> Vec<&'a str> {
    let mut args = vec!["--chip", ESPTOOL_CHIP, "--port", path];
    if let Some(b) = baud {
        args.push("--baud");
        args.push(b);
    }
    args
}

fn esptool_executable() -> Result<PathBuf> {
    let mut candidates = vec![PathBuf::from("/usr/local/bin/archy-esptool")];
    if let Some(paths) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&paths) {
            for name in ["esptool", "esptool.py"] {
                candidates.push(directory.join(name));
            }
        }
    }
    executable_from_candidates(candidates)
}

fn executable_from_candidates(candidates: impl IntoIterator<Item = PathBuf>) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    candidates.into_iter().find(|path| {
        path.is_file() && path.metadata().is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    }).ok_or_else(|| anyhow::anyhow!(
        "Radio flashing tools are missing. Install the complete Archipelago update and retry; the radio has not been changed."
    ))
}

async fn preflight_esptool() -> Result<PathBuf> {
    let executable = esptool_executable()?;
    check_esptool(&executable).await?;
    Ok(executable)
}

async fn check_esptool(executable: &Path) -> Result<()> {
    let mut command = Command::new(executable);
    command
        .arg(
            if executable
                .file_name()
                .is_some_and(|name| name == "archy-esptool")
            {
                "--archy-self-test"
            } else {
                "version"
            },
        )
        .kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(20), command.output())
        .await
        .context("Radio flashing tool did not respond; the radio has not been changed")?
        .context("Radio flashing tool could not start; check its installation and permissions")?;
    anyhow::ensure!(
        output.status.success(),
        "Radio flashing tool self-check failed; reinstall the complete update before retrying"
    );
    Ok(())
}

fn retryable_flash_error(error: &anyhow::Error) -> bool {
    if error
        .chain()
        .any(|cause| cause.downcast_ref::<std::io::Error>().is_some())
    {
        return false;
    }
    let detail = format!("{error:#}").to_lowercase();
    [
        "failed to connect",
        "timed out waiting",
        "invalid head of packet",
        "serial data stream stopped",
    ]
    .iter()
    .any(|message| detail.contains(message))
}

async fn esptool_with_retry(path: &str, subcommand: &[&str], job: &Arc<FlashJob>) -> Result<()> {
    let executable = preflight_esptool().await?;
    let mut cmd = Command::new(&executable);
    cmd.args(esptool_global_args(path, None));
    cmd.args(subcommand);
    match run_streamed(cmd, None, job).await {
        Ok(()) => Ok(()),
        Err(first_err) if retryable_flash_error(&first_err) => {
            job.push_log(format!(
                "First attempt failed ({first_err:#}); retrying once at {ESPTOOL_FALLBACK_BAUD} baud"
            ))
            .await;
            let mut retry = Command::new(&executable);
            retry.args(esptool_global_args(path, Some(ESPTOOL_FALLBACK_BAUD)));
            retry.args(subcommand);
            run_streamed(retry, None, job)
                .await
                .context(format!("retry also failed (first attempt: {first_err:#})"))
        }
        Err(error) => Err(error),
    }
}

// ─── Reticulum/RNode: archy-rnodeconf ───────────────────────────────────

fn rnodeconf_bin() -> String {
    std::env::var("ARCHY_RNODECONF_BIN")
        .unwrap_or_else(|_| "/usr/local/bin/archy-rnodeconf".to_string())
}

/// True when `name` resolves to an executable on PATH.
fn which_on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(name);
                candidate.is_file()
            })
        })
        .unwrap_or(false)
}

/// `--autoinstall`'s "which board is this" step is interactive by design —
/// confirmed live against a real Heltec V4 (2026-07-23): even with a board
/// given on the command line, rnodeconf can't always tell V3 from V4 apart
/// (their bootstrap-time USB identity is often generic, same root cause as
/// `resolve_flash_board`'s doc comment), so it always asks. The full prompt
/// sequence observed for a Heltec board that already has *some* RNode
/// firmware installed (the common case — a truly blank chip likely skips
/// straight to the same "Device Selection" menu):
///   1. numbered device-type menu  → answer with the menu number
///   2. "Hit enter to continue"    → answer with a blank line
///   3. numbered band menu         → answer with the menu number
///   4. "Is the above correct? [y/N]" → answer "y"
/// Feeding all four answers up front (rather than watching stdout for each
/// prompt text) works because the menu is always asked in this fixed order
/// for every board that needs (re)provisioning — verified by driving it
/// through an unprovisioned real V4 end-to-end (erase → flash → EEPROM
/// bootstrap → "Device signature validated" on the next probe).
fn rnodeconf_device_menu_number(board: FlashBoard) -> &'static str {
    match board {
        FlashBoard::HeltecV3 => "8",
        FlashBoard::HeltecV4 => "9",
    }
}

/// rnodeconf's band choice is a coarse RF-frontend bootstrap parameter
/// (868/915/923 MHz), not the final operating frequency — that's still
/// configured later via the daemon's interface config, same as today. This
/// is a best-effort mapping from the node's persisted Meshtastic-style
/// region code (see `mesh::meshtastic::region_name_to_code`) down to
/// rnodeconf's 3-way menu; regions with no exact 868/923 match fall back to
/// 915 MHz as the broadest-compatibility default.
fn rnodeconf_band_menu_number(lora_region: Option<&str>) -> &'static str {
    match lora_region.map(|s| s.trim().to_uppercase()) {
        Some(r) if r.contains("868") => "1",
        Some(r) if r.contains("923") => "3",
        _ => "2",
    }
}

/// `--autoinstall` fetches, erases, flashes, and bootstraps the EEPROM for
/// a detected board as one atomic step (confirmed via `archy-rnodeconf
/// --help` AND a real end-to-end flash on real hardware) — this is the
/// RNode-side equivalent of our "always erase before write" default, since
/// autoinstall doesn't try to preserve any existing on-device state.
async fn rnodeconf_autoinstall(
    path: &str,
    board: FlashBoard,
    lora_region: Option<&str>,
    job: &Arc<FlashJob>,
) -> Result<()> {
    job.set_stage(FlashStage::Autoinstalling).await;
    let bin = rnodeconf_bin();
    let mut cmd = if Path::new(&bin).exists() {
        Command::new(bin)
    } else if which_on_path("rnodeconf") {
        // Dev fallback if only a plain venv/system rnodeconf is on PATH.
        Command::new("rnodeconf")
    } else {
        // Older ISOs/OTAs never shipped the tool — say so instead of the
        // bare "No such file or directory" the spawn would produce.
        anyhow::bail!(
            "{bin} is not installed on this node — RNode flashing needs the packaged \
             archy-rnodeconf tool, which ships with the v1.7.118+ update (or can be \
             sideloaded from a dev box). Update the node, then retry."
        );
    };
    cmd.args(["--autoinstall", path]);
    let stdin = format!(
        "{}\n\n{}\ny\n",
        rnodeconf_device_menu_number(board),
        rnodeconf_band_menu_number(lora_region)
    );
    run_streamed(cmd, Some(stdin.into_bytes()), job)
        .await
        .context("archy-rnodeconf --autoinstall failed")
}

// ─── Subprocess streaming ────────────────────────────────────────────────

fn percent_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\((\d{1,3})\s*%\)").expect("valid regex"))
}

async fn run_streamed(mut cmd: Command, stdin: Option<Vec<u8>>, job: &Arc<FlashJob>) -> Result<()> {
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    // Deliberately NOT kill_on_drop: an interrupted erase/write can leave
    // the chip in a worse state than either finished or unstarted (see the
    // cancellation-safety note in mesh flashing docs). The job is expected
    // to run to completion or fail on its own.
    let mut child = cmd.spawn().context("Failed to start subprocess")?;

    if let Some(bytes) = stdin {
        if let Some(mut child_stdin) = child.stdin.take() {
            child_stdin
                .write_all(&bytes)
                .await
                .context("Writing to subprocess stdin")?;
        }
    }

    let mut tasks = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        let job = Arc::clone(job);
        tasks.push(tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(cap) = percent_regex().captures(&line) {
                    if let Ok(pct) = cap[1].parse::<u8>() {
                        job.set_percent(pct).await;
                    }
                }
                job.push_log(line).await;
            }
        }));
    }
    if let Some(stderr) = child.stderr.take() {
        let job = Arc::clone(job);
        tasks.push(tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                job.push_log(line).await;
            }
        }));
    }

    let status = child.wait().await.context("Waiting for subprocess")?;
    for t in tasks {
        let _ = t.await;
    }
    if !status.success() {
        // Exit status alone isn't diagnosable — the actual esptool/rnodeconf
        // stderr (already captured into job.log_tail by the reader tasks
        // above) is what actually explains a failure. Confirmed live
        // 2026-07-23: a bare "Command exited with exit status: 1" told us
        // nothing when esptool's real error was sitting in the log tail the
        // whole time, only visible via the UI's live poll, not journald.
        let tail: Vec<String> = job
            .snapshot()
            .await
            .log_tail
            .iter()
            .rev()
            .take(10)
            .rev()
            .cloned()
            .collect();
        anyhow::bail!("Command exited with {status}\n{}", tail.join("\n"));
    }
    Ok(())
}

#[cfg(test)]
mod flashing_regression_tests {
    use super::*;

    #[tokio::test]
    async fn full_flash_invalidates_only_radio_settings_marker() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("meshcore-radio-params.json");
        tokio::fs::write(&marker, b"old settings").await.unwrap();
        let other = dir.path().join("mesh-config.json");
        tokio::fs::write(&other, b"preserved").await.unwrap();
        invalidate_radio_settings_marker(dir.path()).await.unwrap();
        assert!(!marker.exists());
        assert_eq!(tokio::fs::read(&other).await.unwrap(), b"preserved");
        invalidate_radio_settings_marker(dir.path()).await.unwrap();
        tokio::fs::create_dir(&marker).await.unwrap();
        assert!(invalidate_radio_settings_marker(dir.path()).await.is_err());
    }
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn meshcore_cache_requires_release_size_and_checksum() {
        use sha2::{Digest, Sha256};
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("fixture.bin");
        tokio::fs::write(&file, b"firmware fixture").await.unwrap();
        let mut asset = GithubAsset {
            name: "fixture.bin".into(),
            browser_download_url: "https://example.invalid/fixture.bin".into(),
            size: Some(16),
            digest: Some(format!(
                "sha256:{}",
                hex::encode(Sha256::digest(b"firmware fixture"))
            )),
        };
        assert!(verify_meshcore_asset(&file, &asset).await.is_ok());
        tokio::fs::write(&file, b"tampered fixture").await.unwrap();
        assert!(verify_meshcore_asset(&file, &asset).await.is_err());
        tokio::fs::write(&file, b"short").await.unwrap();
        assert!(verify_meshcore_asset(&file, &asset).await.is_err());
        tokio::fs::write(&file, b"firmware fixture").await.unwrap();
        asset.digest = None;
        assert!(verify_meshcore_asset(&file, &asset).await.is_err());
    }

    #[test]
    fn generic_usb_ids_do_not_select_firmware() {
        for (vid, pid) in [("10c4", "ea60"), ("303a", "1001")] {
            let info = DetectedDeviceInfo {
                path: "/dev/fixture".into(),
                vid: Some(vid.into()),
                pid: Some(pid.into()),
                product: None,
                manufacturer: None,
                plugged_at: None,
            };
            assert_eq!(resolve_flash_board(&info), None);
        }
    }

    #[test]
    fn absent_nonexecutable_and_directory_candidates_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("flasher");
        std::fs::write(&file, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(executable_from_candidates([
            dir.path().join("absent"),
            file.clone(),
            dir.path().to_path_buf()
        ])
        .is_err());
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(executable_from_candidates([file.clone()]).unwrap(), file);
    }

    #[tokio::test]
    async fn flasher_preflight_reports_missing_interpreter_and_failed_self_check() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("archy-esptool");
        for script in ["#!/missing/python\n", "#!/bin/sh\nexit 7\n"] {
            std::fs::write(&file, script).unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).unwrap();
            assert!(check_esptool(&file).await.is_err());
        }
        std::fs::write(&file, "#!/bin/sh\n[ \"$1\" = --archy-self-test ]\n").unwrap();
        assert!(check_esptool(&file).await.is_ok());
    }

    #[tokio::test]
    async fn flash_registration_excludes_other_writers_and_active_probes() {
        let handle = new_job_handle();
        let first = FlashJob::new(
            FlashBoard::HeltecV3,
            DeviceType::Meshcore,
            "/dev/fixture".into(),
        );
        let second = FlashJob::new(
            FlashBoard::HeltecV3,
            DeviceType::Meshcore,
            "/dev/fixture".into(),
        );
        let probe = handle.read().await;
        assert!(register_flash_job(&handle, &first).await.is_err());
        drop(probe);
        let (a, b) = tokio::join!(
            register_flash_job(&handle, &first),
            register_flash_job(&handle, &second)
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        handle.read().await.as_ref().unwrap().finish().await;
        let next = FlashJob::new(
            FlashBoard::HeltecV3,
            DeviceType::Meshcore,
            "/dev/fixture".into(),
        );
        assert!(register_flash_job(&handle, &next).await.is_ok());
    }

    #[test]
    fn retries_only_known_serial_transport_failures() {
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
        ] {
            assert!(!retryable_flash_error(
                &anyhow::Error::from(std::io::Error::from(kind))
                    .context("Failed to start subprocess")
            ));
        }
        for message in [
            "Wrong chip",
            "Invalid image",
            "module missing",
            "permission denied",
            "port is busy",
        ] {
            assert!(!retryable_flash_error(&anyhow::anyhow!(message)));
        }
        assert!(retryable_flash_error(&anyhow::anyhow!(
            "Failed to connect to ESP32-S3: timed out waiting for packet header"
        )));
        assert_eq!(
            esptool_global_args("/dev/fixture", Some("115200")),
            [
                "--chip",
                "esp32s3",
                "--port",
                "/dev/fixture",
                "--baud",
                "115200"
            ]
        );
    }
}
