use super::*;
use crate::api::rpc::RpcHandler;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

impl RpcHandler {
    /// server.set-name — Rename the server (persisted to data_dir/server-name)
    pub(in crate::api::rpc) async fn handle_server_set_name(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?
            .trim()
            .to_string();

        if name.is_empty() || name.len() > 64 {
            anyhow::bail!("Name must be 1-64 characters");
        }

        // Persist to file
        let name_file = self.config.data_dir.join("server-name");
        tokio::fs::write(&name_file, &name)
            .await
            .context("Failed to write server name")?;

        // Update live state
        let (mut data, _) = self.state_manager.get_snapshot().await;
        data.server_info.name = Some(name.clone());
        self.state_manager.update_data(data).await;

        let hostname = hostname_from_server_name(&name);
        let hostname_result = set_system_hostname(&hostname).await;
        let (hostname_updated, hostname_error) = match hostname_result {
            Ok(()) => (true, None),
            Err(e) => {
                warn!(
                    name = %name,
                    hostname = %hostname,
                    "Server name persisted but OS hostname update failed: {}",
                    e
                );
                (false, Some(e.to_string()))
            }
        };

        // Keep the self-signed HTTPS cert's SAN in sync with the new hostname —
        // best-effort, never blocks the rename itself. Without this the cert
        // stays pinned to whatever name was set at install time, so browsers
        // hit a hostname-mismatch warning on top of the usual self-signed one
        // the moment a node is renamed.
        if hostname_updated {
            sync_hostname_side_effects(&hostname).await;
            if let Err(e) = regenerate_tls_cert(&hostname).await {
                warn!(hostname = %hostname, "TLS cert regen after rename failed: {}", e);
            }
        }

        info!("Server name updated to: {}", name);

        // Propagate to the mesh: the listener advertises the server name (when
        // no explicit mesh advert_name overrides it), but it was only read at
        // process startup — a rename never reached the radio/RNS until the
        // next full restart. Push it into the service and bounce the listener
        // in the background (the restart re-probes the radio, which can take
        // seconds — don't block the rename response on it).
        {
            let mesh_arc = self.mesh_service_arc();
            let name_for_mesh = name.clone();
            tokio::spawn(async move {
                let mut guard = mesh_arc.write().await;
                if let Some(svc) = guard.as_mut() {
                    svc.set_server_name(Some(name_for_mesh));
                    if svc.config().advert_name.is_none() {
                        if let Err(e) = svc.restart_listener_if_running().await {
                            warn!("Mesh listener restart after rename failed: {}", e);
                        }
                    }
                }
            });
        }

        // Push the new name to federation peers in background
        let data_dir = self.config.data_dir.clone();
        let state_manager = self.state_manager.clone();
        tokio::spawn(async move {
            if let Err(e) = push_name_to_peers(&data_dir, &state_manager).await {
                debug!("Federation name push (non-fatal): {}", e);
            }
        });

        Ok(serde_json::json!({
            "name": name,
            "hostname": hostname,
            "hostname_updated": hostname_updated,
            "hostname_error": hostname_error,
        }))
    }

    /// server.set-location — Set this node's own lat/lon + whether to share
    /// it with trusted federation peers (for the Mesh Map). `lat`/`lon` are
    /// optional so a caller can flip `share` off without clearing the saved
    /// position, or clear the position by passing nulls.
    pub(in crate::api::rpc) async fn handle_server_set_location(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let lat = params.get("lat").and_then(|v| v.as_f64());
        let lon = params.get("lon").and_then(|v| v.as_f64());
        let share_location = params
            .get("share")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: share"))?;

        if let (Some(lat), Some(lon)) = (lat, lon) {
            if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
                anyhow::bail!("Invalid lat/lon");
            }
        }

        let location_file = self.config.data_dir.join("server-location.json");
        let payload =
            serde_json::json!({ "lat": lat, "lon": lon, "share_location": share_location });
        tokio::fs::write(&location_file, serde_json::to_vec(&payload)?)
            .await
            .context("Failed to write server location")?;

        let (mut data, _) = self.state_manager.get_snapshot().await;
        data.server_info.lat = lat;
        data.server_info.lon = lon;
        data.server_info.share_location = share_location;
        self.state_manager.update_data(data).await;

        info!(share_location, "Server location updated");

        // Push the new location to federation peers in background, same as
        // a rename — trusted peers' next state sync picks it up.
        let data_dir = self.config.data_dir.clone();
        let state_manager = self.state_manager.clone();
        tokio::spawn(async move {
            if let Err(e) = push_name_to_peers(&data_dir, &state_manager).await {
                debug!("Federation location push (non-fatal): {}", e);
            }
        });

        Ok(serde_json::json!({ "lat": lat, "lon": lon, "share_location": share_location }))
    }

    /// system.get-hostname — Current OS hostname + the mDNS `.local` name it
    /// resolves to on the LAN (avahi-daemon advertises `<hostname>.local`).
    /// Lets Settings show users where to reach this node over HTTPS for
    /// features (mic/camera access) that require a secure context.
    pub(in crate::api::rpc) async fn handle_system_get_hostname(
        &self,
    ) -> Result<serde_json::Value> {
        let hostname = tokio::fs::read_to_string("/etc/hostname")
            .await
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "archipelago".to_string());
        // LAN IPv4 rides along for the companion pairing QR: when the
        // operator's browser reaches this node over Tailscale/VPN or
        // localhost, that origin is useless to a phone on the LAN — the QR
        // must advertise an address the phone can actually dial
        // (2026-07-22: a pairing QR carried a tailnet 100.x IP and the
        // companion could never connect).
        let lan_ip = crate::host_ip::primary_host_ipv4().await;
        Ok(serde_json::json!({
            "hostname": hostname,
            "mdns_hostname": format!("{hostname}.local"),
            "lan_ip": lan_ip,
        }))
    }

    /// system.stats — CPU usage, RAM used/total, disk used/total, uptime, load average
    pub(in crate::api::rpc) async fn handle_system_stats(&self) -> Result<serde_json::Value> {
        debug!("Getting system stats");

        let uptime = read_uptime().await.unwrap_or(0.0);
        let load = read_loadavg().await.unwrap_or((0.0, 0.0, 0.0));
        let cpu = read_cpu_usage().await.unwrap_or(0.0);
        let (mem_used, mem_total) = read_meminfo().await.unwrap_or((0, 0));
        // Prefer encrypted data partition if it exists
        let data_path = std::path::Path::new("/var/lib/archipelago");
        let df_target = if data_path.exists() {
            "/var/lib/archipelago"
        } else {
            "/"
        };
        let (disk_used, disk_total) = read_disk_usage_path(df_target).await.unwrap_or((0, 0));

        // F-03 verdict. Surfacing it here rather than only in a log file is the
        // point: an operator must be able to see that a node is running
        // fleet-shared host keys without having to SSH into it — and SSH into
        // it is exactly the thing a shared host key compromises.
        let host_secrets = host_secrets_status(Path::new(HOST_SECRETS_STATE_DIR)).await;

        Ok(serde_json::json!({
            "host_secrets": host_secrets,
            "uptime_secs": uptime as u64,
            "load_avg_1": load.0,
            "load_avg_5": load.1,
            "load_avg_15": load.2,
            "cpu_usage_percent": cpu,
            "mem_used_bytes": mem_used,
            "mem_total_bytes": mem_total,
            "disk_used_bytes": disk_used,
            "disk_total_bytes": disk_total,
        }))
    }

    /// system.processes — top 10 processes by CPU
    pub(in crate::api::rpc) async fn handle_system_processes(&self) -> Result<serde_json::Value> {
        debug!("Getting top processes");

        let procs = read_top_processes().await.unwrap_or_default();

        Ok(serde_json::json!({ "processes": procs }))
    }

    /// system.temperature — thermal zone readings
    pub(in crate::api::rpc) async fn handle_system_temperature(&self) -> Result<serde_json::Value> {
        debug!("Getting system temperature");

        let temps = read_temperatures().await.unwrap_or_default();

        Ok(serde_json::json!({ "temperatures": temps }))
    }

    /// system.detect-usb-devices — scan for known hardware wallet USB devices
    pub(in crate::api::rpc) async fn handle_system_detect_usb_devices(
        &self,
    ) -> Result<serde_json::Value> {
        debug!("Scanning for USB hardware wallets");

        let devices = detect_usb_hardware_wallets().await.unwrap_or_default();

        Ok(serde_json::json!({ "devices": devices }))
    }

    /// system.disk-status — Disk usage with warning/critical thresholds.
    pub(in crate::api::rpc) async fn handle_system_disk_status(&self) -> Result<serde_json::Value> {
        // Prefer the encrypted data partition if it exists
        let data_path = std::path::Path::new("/var/lib/archipelago");
        let df_target = if data_path.exists() {
            "/var/lib/archipelago"
        } else {
            "/"
        };

        let (used, total) = read_disk_usage_path(df_target).await.unwrap_or((0, 0));
        let percent = if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        let percent_rounded = (percent * 10.0).round() / 10.0;

        let level = if percent >= 90.0 {
            "critical"
        } else if percent >= 85.0 {
            "warning"
        } else {
            "ok"
        };

        // Detect LUKS encryption (device name varies by install)
        let encrypted = std::path::Path::new("/dev/mapper/archipelago_crypt").exists()
            || std::path::Path::new("/dev/mapper/archipelago-data").exists()
            || std::path::Path::new("/dev/mapper/archipelago_data").exists();

        Ok(serde_json::json!({
            "used_bytes": used,
            "total_bytes": total,
            "free_bytes": total.saturating_sub(used),
            "used_percent": percent_rounded,
            "level": level,
            "encrypted": encrypted,
            "partition": df_target,
        }))
    }

    /// system.disk-cleanup — Remove old container images, stale logs, and temp files.
    pub(in crate::api::rpc) async fn handle_system_disk_cleanup(
        &self,
    ) -> Result<serde_json::Value> {
        tracing::info!("Starting disk cleanup");
        let mut freed_bytes: u64 = 0;
        let mut actions: Vec<String> = Vec::new();

        // 1. Clean old log files (> 30 days)
        match clean_old_logs(30).await {
            Ok(bytes) => {
                if bytes > 0 {
                    freed_bytes += bytes;
                    actions.push(format!("Cleaned old logs: {} freed", format_bytes(bytes)));
                }
            }
            Err(e) => actions.push(format!("Log cleanup failed: {}", e)),
        }

        match vacuum_journal_logs("200M").await {
            Ok(bytes) => {
                if bytes > 0 {
                    freed_bytes += bytes;
                    actions.push(format!(
                        "Vacuumed journal logs: {} freed",
                        format_bytes(bytes)
                    ));
                }
            }
            Err(e) => actions.push(format!("Journal cleanup failed: {}", e)),
        }

        // 2. Remove stale temp files
        match clean_temp_files().await {
            Ok(bytes) => {
                if bytes > 0 {
                    freed_bytes += bytes;
                    actions.push(format!("Removed temp files: {} freed", format_bytes(bytes)));
                }
            }
            Err(e) => actions.push(format!("Temp cleanup failed: {}", e)),
        }

        // 3. Keep only the most recent backend deploy backups. These are useful
        // for rollback, but a long-lived alpha node can accumulate gigabytes of
        // old binaries under /usr/local/bin.
        match clean_backend_backups(3).await {
            Ok(bytes) => {
                if bytes > 0 {
                    freed_bytes += bytes;
                    actions.push(format!(
                        "Removed old backend backups: {} freed",
                        format_bytes(bytes)
                    ));
                }
            }
            Err(e) => actions.push(format!("Backend backup cleanup failed: {}", e)),
        }

        match clean_legacy_backend_backups(3).await {
            Ok(bytes) => {
                if bytes > 0 {
                    freed_bytes += bytes;
                    actions.push(format!(
                        "Removed old legacy backend backups: {} freed",
                        format_bytes(bytes)
                    ));
                }
            }
            Err(e) => actions.push(format!("Legacy backend backup cleanup failed: {}", e)),
        }

        match clean_web_ui_backups(3).await {
            Ok(bytes) => {
                if bytes > 0 {
                    freed_bytes += bytes;
                    actions.push(format!(
                        "Removed old web UI backups: {} freed",
                        format_bytes(bytes)
                    ));
                }
            }
            Err(e) => actions.push(format!("Web UI backup cleanup failed: {}", e)),
        }

        actions.push(
            "Skipped Podman image/volume prune: Podman store commands can block app health on busy nodes"
                .to_string(),
        );

        tracing::info!(
            "Disk cleanup complete: {} freed ({} actions)",
            format_bytes(freed_bytes),
            actions.len()
        );

        Ok(serde_json::json!({
            "freed_bytes": freed_bytes,
            "freed_human": format_bytes(freed_bytes),
            "actions": actions,
        }))
    }
}

pub(super) fn hostname_from_server_name(name: &str) -> String {
    let mut hostname = String::with_capacity(name.len());
    let mut previous_dash = false;

    for c in name.trim().chars().flat_map(char::to_lowercase) {
        let valid = c.is_ascii_lowercase() || c.is_ascii_digit();
        if valid {
            hostname.push(c);
            previous_dash = false;
        } else if !previous_dash {
            hostname.push('-');
            previous_dash = true;
        }
        if hostname.len() >= 63 {
            break;
        }
    }

    let hostname = hostname.trim_matches('-').to_string();
    if hostname.is_empty() {
        "archipelago".to_string()
    } else {
        hostname
    }
}

/// Post-rename side effects that keep the OS consistent with the new
/// hostname — all best-effort, the rename itself has already succeeded.
/// Debian resolves the local hostname via the 127.0.1.1 line in /etc/hosts;
/// leaving the old name there breaks `sudo` ("unable to resolve host") and
/// `hostname -f`. And while avahi eventually follows the kernel hostname,
/// re-announcing immediately makes http(s)://<hostname>.local links work
/// right after the rename instead of minutes later.
async fn sync_hostname_side_effects(hostname: &str) {
    // hostname_from_server_name guarantees [a-z0-9-], safe to interpolate.
    let script = format!(
        "if grep -q '^127\\.0\\.1\\.1' /etc/hosts; then sed -i 's/^127\\.0\\.1\\.1.*/127.0.1.1\\t{h}/' /etc/hosts; else printf '127.0.1.1\\t{h}\\n' >> /etc/hosts; fi",
        h = hostname
    );
    match tokio::process::Command::new("/usr/bin/sudo")
        .args(["-n", "/bin/sh", "-c", &script])
        .output()
        .await
    {
        Ok(o) if o.status.success() => {}
        Ok(o) => warn!(
            "/etc/hosts hostname sync failed: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(e) => warn!("/etc/hosts hostname sync failed: {}", e),
    }

    // The kiosk Chromium's profile lock is a symlink encoding <hostname>-<pid>;
    // after a rename the stale lock reads as "another computer" holding the
    // profile, Chromium refuses to start (--noerrdialogs hides the dialog), and
    // the kiosk black-screens on the next boot (#98). Clear it here — Chromium
    // recreates the files on launch, and the kiosk launcher pkills any running
    // instance before starting a new one.
    for f in ["SingletonLock", "SingletonCookie", "SingletonSocket"] {
        let _ = tokio::fs::remove_file(format!("/var/lib/archipelago/chromium-kiosk/{f}")).await;
    }

    let republished = tokio::process::Command::new("/usr/bin/sudo")
        .args(["-n", "/usr/bin/avahi-set-host-name", hostname])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !republished {
        let _ = tokio::process::Command::new("/usr/bin/sudo")
            .args(["-n", "/usr/bin/systemctl", "try-restart", "avahi-daemon"])
            .output()
            .await;
    }
}

async fn set_system_hostname(hostname: &str) -> Result<()> {
    let output = tokio::process::Command::new("/usr/bin/sudo")
        .args(["-n", "/usr/bin/hostnamectl", "set-hostname", hostname])
        .output()
        .await
        .context("Failed to run hostnamectl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!(
            "{}",
            if stderr.is_empty() {
                "hostnamectl failed".to_string()
            } else {
                stderr
            }
        );
    }

    Ok(())
}

const TLS_SSL_DIR: &str = "/etc/archipelago/ssl";
const TLS_KEY_NAME: &str = "archipelago.key";
const TLS_CRT_NAME: &str = "archipelago.crt";
const TLS_KEY_STAGING_NAME: &str = "archipelago.key.new";
const TLS_CRT_STAGING_NAME: &str = "archipelago.crt.new";

const SUDO_BIN: &str = "/usr/bin/sudo";
const OPENSSL_BIN: &str = "/usr/bin/openssl";
const STAT_BIN: &str = "/usr/bin/stat";
const INSTALL_BIN: &str = "/usr/bin/install";
const MKDIR_BIN: &str = "/usr/bin/mkdir";
const MV_BIN: &str = "/usr/bin/mv";
const RM_BIN: &str = "/usr/bin/rm";

/// Modes used only when there is no existing file to copy them from (a node
/// whose TLS material has somehow gone missing entirely). The key is private
/// material; the cert is public.
const TLS_KEY_FALLBACK_MODE: &str = "600";
const TLS_CRT_FALLBACK_MODE: &str = "644";

/// Where `scripts/security/host-secrets-audit.sh` leaves its verdict. Hardcoded
/// rather than derived from `config.data_dir` because the script is a systemd
/// unit with no view of the daemon's configuration — this is the path both
/// sides agree on.
const HOST_SECRETS_STATE_DIR: &str = "/var/lib/archipelago";
const HOST_SECRETS_AUDIT_FILE: &str = "host-secrets-audit.json";
const HOST_KEY_ROTATION_FILE: &str = "host-key-rotation.json";

/// The `host_secrets` object carried by `system.stats`: has this node been
/// judged to be running the fleet-shared, image-baked SSH host keys and TLS
/// private key (audit F-03), and has it been rotated since?
///
/// Three properties this must hold, because `system.stats` is in
/// `CACHEABLE_METHODS` (`api/rpc/middleware.rs:41`) and the dashboard polls it:
///
/// 1. **It never errors.** A missing, truncated or unparseable verdict file
///    yields `{"verdict":"unknown"}`. A node that has not run the audit yet —
///    every node, until the OTA carrying the unit lands — must not turn its own
///    stats call into a failure.
/// 2. **It is cheap.** Two small file reads, no process spawn, no key parsing.
///    The expensive work (ssh-keygen, openssl) happens in the boot unit, once.
/// 3. **It carries no fingerprints.** Those live in the on-disk record only.
///    They are public data, but there is no reason to put them in a payload
///    that is polled every few seconds — the dashboard needs the verdict, and
///    an operator who needs the digests is already on the node.
async fn host_secrets_status(dir: &Path) -> serde_json::Value {
    let unknown = || serde_json::json!({ "verdict": "unknown" });

    let audit: serde_json::Value =
        match tokio::fs::read_to_string(dir.join(HOST_SECRETS_AUDIT_FILE))
            .await
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
        {
            Some(v) => v,
            None => return unknown(),
        };

    let verdict = audit
        .get("verdict")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let mut out = serde_json::json!({ "verdict": verdict });

    if let Some(checked_at) = audit.get("checked_at").and_then(|v| v.as_str()) {
        out["checked_at"] = serde_json::json!(checked_at);
    }
    if let Some(evidence) = audit.get("evidence").and_then(|v| v.as_array()) {
        out["evidence"] = serde_json::json!(evidence);
    }

    // A rotation record only exists on a node an operator has actually
    // rotated, so its absence is the normal case and is not reported.
    if let Some(rotated_at) = tokio::fs::read_to_string(dir.join(HOST_KEY_ROTATION_FILE))
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| {
            v.get("rotated_at")
                .and_then(|r| r.as_str())
                .map(|s| s.to_string())
        })
    {
        out["rotated_at"] = serde_json::json!(rotated_at);
    }

    out
}

/// Where this node's TLS material lives, and how we are allowed to touch it.
///
/// This type exists for two reasons. The first is that every step of the
/// generate/validate/swap dance below has to run as root — the live key is
/// root-owned 0600 and the daemon is not root — so each step needs the same
/// `sudo -n` prefix and the same error handling. The second is testability:
/// production uses [`TlsMaterial::production`], while tests point `ssl_dir` at
/// a temp dir, drop the `sudo` prefix, and can substitute `openssl_bin` for a
/// stub that fails partway through generation. That is the only seam added
/// here; nothing else about the module changed.
#[derive(Clone, Debug)]
struct TlsMaterial {
    ssl_dir: PathBuf,
    openssl_bin: PathBuf,
    /// Prefix every command with `sudo -n`.
    privileged: bool,
}

impl TlsMaterial {
    fn production() -> Self {
        Self {
            ssl_dir: PathBuf::from(TLS_SSL_DIR),
            openssl_bin: PathBuf::from(OPENSSL_BIN),
            privileged: true,
        }
    }

    fn key(&self) -> PathBuf {
        self.ssl_dir.join(TLS_KEY_NAME)
    }

    fn crt(&self) -> PathBuf {
        self.ssl_dir.join(TLS_CRT_NAME)
    }

    // The staging files are siblings of their destinations on purpose: same
    // directory means same filesystem, which means the final `mv` is a
    // rename(2) and therefore atomic.
    fn key_staging(&self) -> PathBuf {
        self.ssl_dir.join(TLS_KEY_STAGING_NAME)
    }

    fn crt_staging(&self) -> PathBuf {
        self.ssl_dir.join(TLS_CRT_STAGING_NAME)
    }

    fn cmd<S: AsRef<std::ffi::OsStr>>(&self, program: S) -> tokio::process::Command {
        if self.privileged {
            let mut cmd = tokio::process::Command::new(SUDO_BIN);
            cmd.arg("-n").arg(program);
            cmd
        } else {
            tokio::process::Command::new(program)
        }
    }

    /// `(mode, uid, gid)` of an existing file, or `None` if it is not there.
    async fn stat_attrs(&self, path: &Path) -> Option<(String, String, String)> {
        let mut cmd = self.cmd(STAT_BIN);
        cmd.arg("-c").arg("%a:%u:%g").arg(path);
        let out = cmd.output().await.ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let mut parts = text.split(':');
        let mode = parts.next()?.to_string();
        let uid = parts.next()?.to_string();
        let gid = parts.next()?.to_string();
        if mode.is_empty() || uid.is_empty() || gid.is_empty() {
            return None;
        }
        Some((mode, uid, gid))
    }

    /// Create `dest` as an empty file that *already* carries `mode` (and the
    /// live file's owner, when we know it).
    ///
    /// Pre-creating the staging file is the whole trick for permissions:
    /// openssl's `-keyout` truncates an existing file rather than recreating
    /// it, so the mode set here is the mode the new key has from its very
    /// first byte. There is no window — not even a microsecond — in which the
    /// freshly generated private key sits on disk group- or world-readable
    /// waiting for a follow-up `chmod`.
    async fn create_empty(
        &self,
        dest: &Path,
        mode: &str,
        owner: Option<(&str, &str)>,
    ) -> Result<()> {
        let mut cmd = self.cmd(INSTALL_BIN);
        cmd.arg("-m").arg(mode);
        if let Some((uid, gid)) = owner {
            cmd.arg("-o").arg(uid).arg("-g").arg(gid);
        }
        cmd.arg("/dev/null").arg(dest);
        run_checked(cmd, "install (create staging file)").await?;
        Ok(())
    }

    /// Best-effort removal of both staging files. Called before an attempt (to
    /// clear anything a previous crash left) and after every attempt, success
    /// or failure, so staging artefacts never accumulate next to the live cert.
    async fn clear_staging(&self) {
        let mut cmd = self.cmd(RM_BIN);
        cmd.arg("-f")
            .arg(self.key_staging())
            .arg(self.crt_staging());
        let _ = cmd.output().await;
    }

    /// Regenerate the keypair, validate it, and swap it into place.
    ///
    /// The contract this upholds: on *any* failure the existing key and cert
    /// are left byte-for-byte untouched and an error is returned. A rename
    /// whose cert regeneration fails is a no-op on disk, never a partial write
    /// that leaves nginx serving a truncated key.
    async fn regenerate(&self, hostname: &str) -> Result<()> {
        self.clear_staging().await;
        let result = self.stage_validate_and_swap(hostname).await;
        self.clear_staging().await;
        result
    }

    async fn stage_validate_and_swap(&self, hostname: &str) -> Result<()> {
        let mut mkdir = self.cmd(MKDIR_BIN);
        mkdir.arg("-p").arg(&self.ssl_dir);
        run_checked(mkdir, "mkdir -p ssl dir").await?;

        let key_attrs = self.stat_attrs(&self.key()).await;
        let crt_attrs = self.stat_attrs(&self.crt()).await;

        let key_mode = private_key_mode(key_attrs.as_ref().map(|(m, _, _)| m.as_str()));
        let crt_mode = crt_attrs
            .as_ref()
            .map(|(m, _, _)| m.clone())
            .unwrap_or_else(|| TLS_CRT_FALLBACK_MODE.to_string());

        self.create_empty(
            &self.key_staging(),
            &key_mode,
            key_attrs.as_ref().map(|(_, u, g)| (u.as_str(), g.as_str())),
        )
        .await?;
        self.create_empty(
            &self.crt_staging(),
            &crt_mode,
            crt_attrs.as_ref().map(|(_, u, g)| (u.as_str(), g.as_str())),
        )
        .await?;

        // Cert parameters are deliberately identical to what this function has
        // always produced: same subject, same SAN construction, same rsa:2048,
        // same 3650 days. Only *where* openssl writes has changed.
        let subj = format!("/C=XX/ST=Bitcoin/L=Node/O=Archipelago/CN={hostname}");
        let san = format!(
            "subjectAltName=DNS:{hostname},DNS:{hostname}.local,DNS:localhost,IP:127.0.0.1"
        );
        let mut gen = self.cmd(&self.openssl_bin);
        gen.arg("req")
            .arg("-x509")
            .arg("-nodes")
            .arg("-days")
            .arg("3650")
            .arg("-newkey")
            .arg("rsa:2048")
            .arg("-keyout")
            .arg(self.key_staging())
            .arg("-out")
            .arg(self.crt_staging())
            .arg("-subj")
            .arg(&subj)
            .arg("-addext")
            .arg(&san);
        run_checked(gen, "openssl cert regen").await?;

        // Parse both halves back before trusting either. A zero-exit openssl
        // that somehow produced an empty or truncated artefact still fails
        // here — nginx is never handed material we have not read back
        // ourselves. Extracting the public key from each half also proves the
        // two belong together, so a stale cert can never be paired with a
        // fresh key.
        let mut key_pub = self.cmd(&self.openssl_bin);
        key_pub
            .arg("pkey")
            .arg("-in")
            .arg(self.key_staging())
            .arg("-pubout");
        let key_pub = run_checked(key_pub, "openssl pkey (validate new key)").await?;

        let mut crt_pub = self.cmd(&self.openssl_bin);
        crt_pub
            .arg("x509")
            .arg("-in")
            .arg(self.crt_staging())
            .arg("-noout")
            .arg("-pubkey");
        let crt_pub = run_checked(crt_pub, "openssl x509 (validate new cert)").await?;

        if key_pub.trim_ascii().is_empty() || key_pub.trim_ascii() != crt_pub.trim_ascii() {
            anyhow::bail!(
                "regenerated key and certificate do not match — refusing to install them"
            );
        }

        // nginx and the app gate share the leaf. Validate the daemon's read
        // permission on the staged key before replacing either live file.
        if self.privileged {
            let mut repair = self.cmd("/usr/bin/python3");
            repair.args([
                "-c",
                include_str!("../../../../../../scripts/repair-app-gate-tls-permissions.py"),
                "--key-name",
                TLS_KEY_STAGING_NAME,
            ]);
            run_checked(repair, "repair staged app TLS key permissions").await?;
        }

        // rename(2) within one directory: a reader sees either the whole old
        // file or the whole new one, never a partial write. Two files cannot
        // be swapped in a single atomic step, so there is a sub-millisecond
        // window between the two renames in which cert and key are from
        // different generations — but both files are fully written and
        // validated by this point, so the only way to land in that window is a
        // rename failure on an already-created sibling, which does not need
        // space or allocation and effectively cannot fail here.
        self.swap_into_place(&self.crt_staging(), &self.crt())
            .await?;
        self.swap_into_place(&self.key_staging(), &self.key())
            .await?;
        Ok(())
    }

    async fn swap_into_place(&self, staging: &Path, live: &Path) -> Result<()> {
        let mut cmd = self.cmd(MV_BIN);
        cmd.arg("-f").arg(staging).arg(live);
        run_checked(cmd, "mv (install new TLS material)").await?;
        Ok(())
    }
}

/// The mode to give the freshly generated private key.
///
/// Copy the live key's mode so a node that deliberately tightened it keeps
/// that, but never copy a mode that grants group or other any access — a
/// world-readable private key is a bug we should not faithfully reproduce, and
/// widening is never allowed.
fn private_key_mode(existing: Option<&str>) -> String {
    match existing.and_then(|m| u32::from_str_radix(m, 8).ok()) {
        Some(mode) if mode & 0o077 == 0 => format!("{mode:o}"),
        _ => TLS_KEY_FALLBACK_MODE.to_string(),
    }
}

/// Run a command, failing with its stderr if it exits non-zero.
async fn run_checked(mut cmd: tokio::process::Command, what: &str) -> Result<Vec<u8>> {
    let out = cmd
        .output()
        .await
        .with_context(|| format!("failed to run {what}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if stderr.is_empty() {
            anyhow::bail!("{what} failed");
        }
        anyhow::bail!("{what} failed: {stderr}");
    }
    Ok(out.stdout)
}

/// Regenerate the self-signed HTTPS cert (`/etc/archipelago/ssl/archipelago.{crt,key}`)
/// with a SAN covering `hostname`, `hostname.local`, `localhost`, and 127.0.0.1, then
/// reload nginx so it picks up the new cert. Still self-signed (browsers will warn
/// on first visit regardless), but avoids stacking a hostname-mismatch warning on
/// top once a node has been renamed away from the install-time default.
///
/// Generation goes to staging siblings and is parsed back before being swapped
/// in — see [`TlsMaterial::regenerate`]. A failed regeneration leaves the live
/// key and cert exactly as they were, because a routine rename must never be
/// able to take HTTPS down.
async fn regenerate_tls_cert(hostname: &str) -> Result<()> {
    TlsMaterial::production().regenerate(hostname).await?;

    let reload = tokio::process::Command::new("/usr/bin/sudo")
        .args(["-n", "/usr/bin/systemctl", "reload", "nginx"])
        .output()
        .await
        .context("Failed to reload nginx")?;
    if !reload.status.success() {
        let stderr = String::from_utf8_lossy(&reload.stderr).trim().to_string();
        anyhow::bail!("nginx reload failed: {}", stderr);
    }

    Ok(())
}

impl RpcHandler {
    /// system.factory-reset — Wipe all user data, remove containers, and restart.
    /// Only preserves the data_dir itself (recreated empty on restart).
    /// system.reboot — Reboot the machine. Requires password re-verification.
    pub(in crate::api::rpc) async fn handle_system_reboot(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let password = params
            .as_ref()
            .and_then(|p| p.get("password"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing password — re-authentication required"))?;

        let valid = self.auth_manager.verify_password(password).await?;
        if !valid {
            return Err(anyhow::anyhow!("Password incorrect"));
        }

        info!("System reboot initiated by user");

        // Schedule reboot in 2 seconds (gives time for the RPC response to reach the client)
        // Uses the tor-helper path unit pattern (writes action file, systemd triggers root service)
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let action = serde_json::json!({"action": "reboot"});
            let _ = tokio::fs::write(
                "/var/lib/archipelago/tor-config/tor-action",
                serde_json::to_string(&action).unwrap_or_default(),
            )
            .await;
        });

        Ok(serde_json::json!({ "rebooting": true }))
    }

    pub(in crate::api::rpc) async fn handle_system_factory_reset(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // Safety check: require { confirm: true }
        let confirmed = params
            .as_ref()
            .and_then(|p| p.get("confirm"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !confirmed {
            anyhow::bail!("Factory reset requires {{ \"confirm\": true }}");
        }

        // Require password re-authentication for destructive operations
        let password = params
            .as_ref()
            .and_then(|p| p.get("password"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing password — re-authentication required"))?;

        let valid = self.auth_manager.verify_password(password).await?;
        if !valid {
            return Err(anyhow::anyhow!("Password Incorrect"));
        }

        // Overwrite our Nostr presence with a tombstone BEFORE the wipe: the
        // discovery keys die with the identity dir, and once they're gone the
        // stale presence event can never be replaced by anyone — it would
        // list this dead install to the whole network until relays expire it.
        // Best-effort with a hard cap so a dead relay can't stall the reset.
        {
            let identity_dir = self.config.data_dir.join("identity");
            let relays = crate::nostr_relays::merged_relay_list(
                &self.config.data_dir,
                &self.config.nostr_relays,
            )
            .await;
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(15),
                crate::nostr_handshake::publish_tombstone(
                    &identity_dir,
                    &relays,
                    self.config.nostr_tor_proxy.as_deref(),
                ),
            )
            .await;
        }

        tracing::warn!("Factory reset initiated — wiping ALL user data and containers");

        let data_dir = &self.config.data_dir;

        // 1. Stop and remove ALL containers (force)
        let client = archipelago_container::PodmanClient::new("archipelago".to_string());
        if let Ok(containers) = client.list_containers().await {
            for c in &containers {
                tracing::info!("Factory reset: removing container {}", c.name);
                let _ = client.stop_container(&c.name).await;
                let _ = client.remove_container(&c.name).await;
            }
        }

        // 2. Remove all container images
        tracing::info!("Factory reset: pruning all container images");
        let _ = tokio::process::Command::new("podman")
            .args(["rmi", "--all", "--force"])
            .output()
            .await;

        // 3. Prune volumes and build cache
        let _ = tokio::process::Command::new("podman")
            .args(["volume", "prune", "-f"])
            .output()
            .await;
        let _ = tokio::process::Command::new("podman")
            .args(["system", "prune", "-af"])
            .output()
            .await;

        // 4. Wipe the entire data directory contents
        //    Delete everything inside data_dir, then recreate the empty dir.
        if let Ok(mut entries) = tokio::fs::read_dir(data_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                // Skip the tor directory (managed by system debian-tor user)
                if name_str == "tor" {
                    continue;
                }

                tracing::info!("Factory reset: removing {}", path.display());
                if path.is_dir() {
                    let _ = tokio::fs::remove_dir_all(&path).await;
                } else {
                    let _ = tokio::fs::remove_file(&path).await;
                }
            }
        }

        // 5. Clear all sessions
        self.session_store.invalidate_all_except("").await;

        tracing::warn!("Factory reset complete — all data wiped, restarting service");

        // Restart the service via systemd
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let _ = tokio::process::Command::new("sudo")
                .args(["systemctl", "restart", "archipelago"])
                .spawn();
        });

        Ok(serde_json::json!({ "status": "resetting" }))
    }

    /// system.settings.get — Read a settings value
    pub(in crate::api::rpc) async fn handle_system_settings_get(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let key = params
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing key"))?;

        match key {
            "claude_api_key_set" | "openai_api_key_set" => {
                let provider = if key == "claude_api_key_set" {
                    "claude"
                } else {
                    "openai"
                };
                Ok(
                    serde_json::json!({ "value": crate::settings::model_provider::has_key(&self.config.data_dir, provider).await }),
                )
            }
            "ai_provider" => {
                let settings =
                    crate::settings::model_provider::ModelProvider::load(&self.config.data_dir)
                        .await?;
                Ok(serde_json::json!({ "value": settings }))
            }
            "ai_provider_status" => {
                let settings =
                    crate::settings::model_provider::ModelProvider::load(&self.config.data_dir)
                        .await?;
                let local = tokio::time::timeout(std::time::Duration::from_secs(4), async {
                    let (detected, _) = crate::api::rpc::mesh::assistant::detect_ollama().await;
                    detected
                        && crate::assistant::backends::ollama::model_supports_tools(
                            crate::assistant::backends::ollama::OLLAMA_BASE_URL,
                            crate::assistant::backends::ollama::OLLAMA_DEFAULT_MODEL,
                        )
                        .await
                });
                let (claude, openai, local) = tokio::join!(
                    crate::settings::model_provider::has_key(&self.config.data_dir, "claude"),
                    crate::settings::model_provider::has_key(&self.config.data_dir, "openai"),
                    local,
                );
                let budget = crate::assistant::AssistantBudget::load(&self.config.data_dir).await;
                Ok(serde_json::json!({ "value": {
                    "schema": 1, "settings": settings, "claude_configured": claude,
                    "openai_configured": openai, "local_ready": local.ok(),
                    "routstr_remaining_sats": budget.remaining_sats(),
                }}))
            }
            _ => Ok(serde_json::json!({ "value": null })),
        }
    }

    /// ai.permissions.get — what the assistant may read about this node.
    ///
    /// Node-side because these grants were in per-origin localStorage, so they
    /// vanished whenever the operator reached the node by a different address.
    pub(in crate::api::rpc) async fn handle_ai_permissions_get(&self) -> Result<serde_json::Value> {
        let granted = Self::ai_grants_unified(&self.config.data_dir).await;
        Ok(serde_json::json!({ "granted": granted }))
    }

    /// ai.permissions.set — replace the grant set.
    ///
    /// Whole-set replacement rather than per-category toggles: the UI owns a
    /// checkbox list and always knows the complete desired state, and a
    /// toggle API would race itself when two tabs are open. The stored value
    /// is echoed back so the caller sees exactly what was kept after
    /// sanitising, which makes a dropped malformed entry visible instead of
    /// silent.
    pub(in crate::api::rpc) async fn handle_ai_permissions_set(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or(serde_json::json!({}));
        let granted: Vec<String> = params
            .get("granted")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .ok_or_else(|| anyhow::anyhow!("Missing granted (array of category ids)"))?;

        // ONE store, not two kept in sync. The assistant's grants file is the
        // authority — it is what actually gates the tool list, so a value that
        // does not reach it is a setting that does nothing. That was the live
        // bug: Settings wrote only the UI store, assistant/grants.json stayed
        // {"apps","system"}, and the model truthfully answered "I don't have a
        // tool for that" no matter what the operator toggled.
        let mut grants = crate::assistant::grants::Grants::default_closed();
        for name in &granted {
            if let Ok(cat) = serde_json::from_value::<crate::assistant::PermissionCategory>(
                serde_json::Value::String(name.clone()),
            ) {
                grants.set(cat, true);
            }
        }
        grants.save(&self.config.data_dir).await?;

        // Keep the legacy file in step so a downgrade does not lose grants.
        let _ = crate::settings::ai_permissions::save(
            &self.config.data_dir,
            crate::settings::ai_permissions::AiPermissions {
                granted: granted.clone(),
            },
        )
        .await;

        let saved = Self::ai_grants_unified(&self.config.data_dir).await;
        Ok(serde_json::json!({ "granted": saved }))
    }

    /// The node's AI grants, as category id strings.
    ///
    /// The assistant's grants file is the AUTHORITY — it is what gates the
    /// tools, so it alone answers whenever it exists. The older
    /// settings/ai_permissions.json is consulted only when no assistant
    /// file exists yet (upgrade from pre-unification versions), and that
    /// read migrates forward: the legacy set is persisted as assistant
    /// grants, ending the union. A permanent union resurrected a revoked
    /// category on every read whenever the two files drifted — observed
    /// live on archi-dev-box, where legacy said all-ten and grants.json
    /// said four, so the UI showed every toggle ON while the assistant
    /// refused six of them.
    async fn ai_grants_unified(data_dir: &std::path::Path) -> Vec<String> {
        if crate::assistant::grants::Grants::exists(data_dir).await {
            let grants = crate::assistant::grants::Grants::load(data_dir).await;
            let mut out: Vec<String> = grants
                .categories()
                .iter()
                .filter_map(|c| serde_json::to_value(c).ok()?.as_str().map(str::to_string))
                .collect();
            out.sort();
            return out;
        }

        // One-time upgrade fold: no assistant file yet — carry the legacy
        // UI-store grants forward so pre-unification grants are not silently
        // revoked, persist them as the authoritative file, and let the
        // legacy file become inert (every set path rewrites it in step for
        // downgrade-safety).
        let legacy = crate::settings::ai_permissions::load(data_dir).await;
        if legacy.granted.is_empty() {
            return Vec::new();
        }
        let mut grants = crate::assistant::grants::Grants::default_closed();
        let mut out: Vec<String> = Vec::new();
        for name in &legacy.granted {
            if let Ok(cat) = serde_json::from_value::<crate::assistant::PermissionCategory>(
                serde_json::Value::String(name.clone()),
            ) {
                grants.set(cat, true);
                out.push(name.clone());
            }
        }
        let _ = grants.save(data_dir).await;
        out.sort();
        out.dedup();
        out
    }

    /// auth.session-policy.get — how long a login lasts on this node.
    pub(in crate::api::rpc) async fn handle_session_policy_get(&self) -> Result<serde_json::Value> {
        let policy = crate::settings::session_policy::load(&self.config.data_dir).await;
        Ok(serde_json::json!({
            "idle_timeout_secs": policy.idle_timeout_secs,
            "absolute_timeout_secs": policy.absolute_timeout_secs,
            "reauth_for_funds": policy.reauth_for_funds,
        }))
    }

    /// auth.session-policy.set — change it.
    ///
    /// Values are clamped rather than rejected: the caller learns what was
    /// actually stored from the reply, which is friendlier than an error and
    /// makes the bounds discoverable. Fields are individually optional so the
    /// UI can change one control without having to send the others back.
    pub(in crate::api::rpc) async fn handle_session_policy_set(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or(serde_json::json!({}));
        let current = crate::settings::session_policy::load(&self.config.data_dir).await;
        let policy = crate::settings::session_policy::SessionPolicy {
            idle_timeout_secs: params
                .get("idle_timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(current.idle_timeout_secs),
            absolute_timeout_secs: match params.get("absolute_timeout_secs") {
                // Explicit null means "no absolute cap", which is different
                // from the field being absent (leave it as it is).
                Some(serde_json::Value::Null) => None,
                Some(v) => v.as_u64().or(current.absolute_timeout_secs),
                None => current.absolute_timeout_secs,
            },
            reauth_for_funds: params
                .get("reauth_for_funds")
                .and_then(|v| v.as_bool())
                .unwrap_or(current.reauth_for_funds),
        };
        let saved = crate::settings::session_policy::save(&self.config.data_dir, policy).await?;
        tracing::info!(
            idle = saved.idle_timeout_secs,
            absolute = ?saved.absolute_timeout_secs,
            reauth_for_funds = saved.reauth_for_funds,
            "session policy updated"
        );
        Ok(serde_json::json!({
            "idle_timeout_secs": saved.idle_timeout_secs,
            "absolute_timeout_secs": saved.absolute_timeout_secs,
            "reauth_for_funds": saved.reauth_for_funds,
        }))
    }

    /// system.settings.set — Write a settings value
    pub(in crate::api::rpc) async fn handle_system_settings_set(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let key = params
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing key"))?;
        let value = params.get("value").and_then(|v| v.as_str()).unwrap_or("");

        match key {
            "claude_api_key" | "openai_api_key" => {
                let provider = if key == "claude_api_key" {
                    "claude"
                } else {
                    "openai"
                };
                crate::settings::model_provider::save_key(&self.config.data_dir, provider, value)
                    .await?;
                info!(provider, "AI provider credential updated");
                Ok(serde_json::json!({ "saved": true }))
            }
            "ai_provider" => {
                let settings: crate::settings::model_provider::ModelProvider =
                    serde_json::from_str(value).context("Invalid AI provider settings")?;
                settings.save(&self.config.data_dir).await?;
                Ok(serde_json::json!({ "saved": true }))
            }
            _ => anyhow::bail!("Unknown setting: {}", key),
        }
    }

    /// system.node-ca.generate — run the node's (idempotent) CA setup so the
    /// dashboard can offer certificate generation as a button. WebUI rule:
    /// users must never be pointed at a terminal; the script reuses an
    /// existing CA and only reissues the leaf, so re-running is safe.
    pub(in crate::api::rpc) async fn handle_system_node_ca_generate(
        &self,
    ) -> Result<serde_json::Value> {
        let script = "/opt/archipelago/scripts/setup-node-ca.sh";
        if tokio::fs::metadata(script).await.is_err() {
            anyhow::bail!(
                "The certificate setup script is not on this node yet — it arrives with the next update."
            );
        }
        let status = host_sudo(&["/usr/bin/bash", script]).await?;
        if !status.success() {
            anyhow::bail!(
                "Certificate generation failed (exit {:?}) — see the node log for detail",
                status.code()
            );
        }
        info!("Node CA generated/reissued via dashboard");
        Ok(serde_json::json!({ "generated": true }))
    }

    /// system.kiosk-display.get — Current kiosk display preset + whether this
    /// node has a kiosk at all (no kiosk unit -> the Settings section hides).
    pub(in crate::api::rpc) async fn handle_system_kiosk_display_get(
        &self,
    ) -> Result<serde_json::Value> {
        let has_kiosk = tokio::fs::metadata("/etc/systemd/system/archipelago-kiosk.service")
            .await
            .is_ok();
        let conf = tokio::fs::read_to_string(KIOSK_DISPLAY_CONF)
            .await
            .unwrap_or_default();
        let preset = if conf.contains("ARCHIPELAGO_KIOSK_SCALE=1") {
            "native"
        } else if conf.contains("ARCHIPELAGO_KIOSK_TARGET_CSS_WIDTH=1920") {
            "balanced"
        } else if conf.contains("ARCHIPELAGO_KIOSK_TARGET_CSS_WIDTH=1280") {
            "large"
        } else {
            "auto"
        };
        let graphics = if conf.contains("KIOSK_GRAPHICS=performance") {
            "performance"
        } else if conf.contains("KIOSK_GRAPHICS=quality") {
            "quality"
        } else {
            "auto"
        };
        Ok(serde_json::json!({ "has_kiosk": has_kiosk, "preset": preset, "graphics": graphics }))
    }

    /// system.kiosk-display.set — Write the kiosk display preset and restart
    /// the kiosk (only if it is running) so it takes effect immediately. The
    /// launcher sources /etc/archipelago/kiosk-display.conf at startup.
    pub(in crate::api::rpc) async fn handle_system_kiosk_display_set(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let preset = params.get("preset").and_then(|v| v.as_str());
        let graphics = params.get("graphics").and_then(|v| v.as_str());
        if preset.is_none() && graphics.is_none() {
            anyhow::bail!("Missing preset or graphics");
        }

        // The conf carries two independent settings (display scale preset +
        // graphics tier). A set of one must not clobber the other, so the
        // half not being changed is carried over from the file as-is.
        let existing = tokio::fs::read_to_string(KIOSK_DISPLAY_CONF)
            .await
            .unwrap_or_default();

        let display_part = match preset {
            // Resolution-derived default: 4K -> 2.0 (1920-wide layout),
            // 1080p TV -> 1.5, laptop panels -> 1.0.
            Some("auto") => String::new(),
            // Biggest UI: every panel targets a 1280-wide layout.
            Some("large") => "ARCHIPELAGO_KIOSK_TARGET_CSS_WIDTH=1280\n".to_string(),
            // Full-HD layout on any panel that can carry it.
            Some("balanced") => "ARCHIPELAGO_KIOSK_TARGET_CSS_WIDTH=1920\n".to_string(),
            // No scaling: native CSS viewport, most content, smallest UI.
            Some("native") => "ARCHIPELAGO_KIOSK_SCALE=1\n".to_string(),
            Some(other) => anyhow::bail!("Unknown display preset: {other}"),
            None => existing
                .lines()
                .filter(|l| l.starts_with("ARCHIPELAGO_KIOSK_"))
                .map(|l| format!("{l}\n"))
                .collect(),
        };

        let graphics_part = match graphics {
            // Auto: the launcher classifies the hardware itself (CPU/iGPU
            // generation) — legacy boxes keep the choppy-audio-safe flags,
            // modern iGPUs get GPU rasterization.
            Some("auto") => String::new(),
            // Force the conservative legacy flag set (troubleshooting).
            Some("performance") => "KIOSK_GRAPHICS=performance\n".to_string(),
            // Force the modern flag set even on unclassified hardware.
            Some("quality") => "KIOSK_GRAPHICS=quality\n".to_string(),
            Some(other) => anyhow::bail!("Unknown graphics mode: {other}"),
            None => existing
                .lines()
                .find(|l| l.starts_with("KIOSK_GRAPHICS="))
                .map(|l| format!("{l}\n"))
                .unwrap_or_default(),
        };

        let conf = format!("{display_part}{graphics_part}");

        host_sudo(&["/usr/bin/mkdir", "-p", "/etc/archipelago"]).await?;
        if conf.is_empty() {
            let _ = host_sudo(&["/usr/bin/rm", "-f", KIOSK_DISPLAY_CONF]).await;
        } else {
            // tee via sudo — the backend runs unprivileged and /etc is root's.
            let mut child = tokio::process::Command::new("/usr/bin/sudo")
                .args(["-n", "/usr/bin/tee", KIOSK_DISPLAY_CONF])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .context("spawn sudo tee for kiosk display conf")?;
            use tokio::io::AsyncWriteExt;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(conf.as_bytes()).await?;
            }
            let out = child.wait_with_output().await?;
            if !out.status.success() {
                anyhow::bail!(
                    "writing {} failed: {}",
                    KIOSK_DISPLAY_CONF,
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
        }

        // try-restart: only bounces a kiosk that is actually running, so this
        // never starts a kiosk an operator disabled.
        let _ = host_sudo(&[
            "/usr/bin/systemctl",
            "try-restart",
            "archipelago-kiosk.service",
        ])
        .await;

        info!(?preset, ?graphics, "Kiosk display settings applied");
        Ok(serde_json::json!({ "preset": preset, "graphics": graphics, "applied": true }))
    }
}

const KIOSK_DISPLAY_CONF: &str = "/etc/archipelago/kiosk-display.conf";

#[cfg(test)]
mod tls_regen_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A `TlsMaterial` pointed at a temp dir, running unprivileged so the test
    /// needs no sudo. Everything else — the command sequence, the staging
    /// paths, the validation, the swap — is the production code path.
    fn material(dir: &Path, openssl_bin: &Path) -> TlsMaterial {
        TlsMaterial {
            ssl_dir: dir.to_path_buf(),
            openssl_bin: openssl_bin.to_path_buf(),
            privileged: false,
        }
    }

    /// Seed the directory the way an installed node looks: a real keypair with
    /// the key at 0600.
    fn seed_live_material(dir: &Path) {
        let key = dir.join(TLS_KEY_NAME);
        let crt = dir.join(TLS_CRT_NAME);
        let status = std::process::Command::new(OPENSSL_BIN)
            .args([
                "req", "-x509", "-nodes", "-days", "3650", "-newkey", "rsa:2048",
            ])
            .arg("-keyout")
            .arg(&key)
            .arg("-out")
            .arg(&crt)
            .arg("-subj")
            .arg("/C=XX/ST=Bitcoin/L=Node/O=Archipelago/CN=oldname")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("seed openssl");
        assert!(status.success(), "seeding the live keypair failed");
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    /// Write an executable stand-in for openssl. It intercepts `req` (the
    /// generation step) and delegates everything else — the `pkey`/`x509`
    /// validation calls — to the real binary.
    fn stub_openssl(dir: &Path, name: &str, exit_code: u8) -> PathBuf {
        let path = dir.join(name);
        let script = format!(
            r#"#!/bin/sh
# Stand-in for openssl that produces the wreckage a killed/ENOSPC openssl
# leaves behind: partial PEM in both output files. Exits {exit_code}.
if [ "$1" = "req" ]; then
    key=""; out=""
    while [ $# -gt 0 ]; do
        case "$1" in
            -keyout) key="$2"; shift ;;
            -out) out="$2"; shift ;;
        esac
        shift
    done
    [ -n "$key" ] && printf -- '-----BEGIN PRIVATE KEY-----\ntruncated' > "$key"
    [ -n "$out" ] && printf -- '-----BEGIN CERTIFICATE-----\ntruncated' > "$out"
    echo "simulated openssl failure" >&2
    exit {exit_code}
fi
exec {OPENSSL_BIN} "$@"
"#
        );
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn snapshot(dir: &Path) -> (Vec<u8>, Vec<u8>, u32) {
        let key = dir.join(TLS_KEY_NAME);
        let mode = std::fs::metadata(&key).unwrap().permissions().mode() & 0o777;
        (
            std::fs::read(&key).unwrap(),
            std::fs::read(dir.join(TLS_CRT_NAME)).unwrap(),
            mode,
        )
    }

    fn assert_no_staging_left(dir: &Path) {
        assert!(
            !dir.join(TLS_KEY_STAGING_NAME).exists(),
            "staging key was left behind"
        );
        assert!(
            !dir.join(TLS_CRT_STAGING_NAME).exists(),
            "staging cert was left behind"
        );
    }

    /// The defect this whole change exists to fix: when openssl dies partway
    /// through, the live key and cert must be byte-identical to what they were
    /// before. Against the previous implementation (openssl writing straight
    /// to the live paths) this fails — the live key is truncated to a few
    /// bytes of PEM header and nginx has nothing to serve.
    #[tokio::test]
    async fn failed_generation_leaves_live_key_and_cert_untouched() {
        let dir = tempfile::tempdir().unwrap();
        seed_live_material(dir.path());
        let before = snapshot(dir.path());

        let openssl = stub_openssl(dir.path(), "openssl-crash", 1);
        let err = material(dir.path(), &openssl)
            .regenerate("newname")
            .await
            .expect_err("a failing openssl must surface as an error");
        assert!(
            err.to_string().contains("openssl cert regen failed"),
            "unexpected error: {err}"
        );

        let after = snapshot(dir.path());
        assert_eq!(before.0, after.0, "live private key was modified");
        assert_eq!(before.1, after.1, "live certificate was modified");
        assert_eq!(before.2, after.2, "live private key mode changed");
        assert_no_staging_left(dir.path());
    }

    /// The other half of the guard: openssl exiting 0 is not proof it produced
    /// anything usable. Truncated output must be caught by the parse-back and
    /// must likewise leave the live material alone.
    #[tokio::test]
    async fn unparseable_output_is_rejected_and_live_material_untouched() {
        let dir = tempfile::tempdir().unwrap();
        seed_live_material(dir.path());
        let before = snapshot(dir.path());

        let openssl = stub_openssl(dir.path(), "openssl-garbage", 0);
        let err = material(dir.path(), &openssl)
            .regenerate("newname")
            .await
            .expect_err("unparseable material must be rejected");
        assert!(
            err.to_string().contains("validate new key"),
            "unexpected error: {err}"
        );

        let after = snapshot(dir.path());
        assert_eq!(before.0, after.0, "live private key was modified");
        assert_eq!(before.1, after.1, "live certificate was modified");
        assert_no_staging_left(dir.path());
    }

    /// Happy path: a real regeneration swaps in a matching pair carrying the
    /// new hostname, and the key keeps its 0600 mode across the swap.
    #[tokio::test]
    async fn successful_regeneration_swaps_in_a_matching_pair_and_keeps_mode() {
        let dir = tempfile::tempdir().unwrap();
        seed_live_material(dir.path());
        let before = snapshot(dir.path());

        material(dir.path(), Path::new(OPENSSL_BIN))
            .regenerate("newname")
            .await
            .expect("regeneration should succeed");

        let after = snapshot(dir.path());
        assert_ne!(before.0, after.0, "the key should have been replaced");
        assert_eq!(after.2, 0o600, "the new key must still be 0600");
        assert_no_staging_left(dir.path());

        // The installed pair parses and the two halves belong together.
        let key_pub = std::process::Command::new(OPENSSL_BIN)
            .arg("pkey")
            .arg("-in")
            .arg(dir.path().join(TLS_KEY_NAME))
            .arg("-pubout")
            .output()
            .unwrap();
        let crt_pub = std::process::Command::new(OPENSSL_BIN)
            .arg("x509")
            .arg("-in")
            .arg(dir.path().join(TLS_CRT_NAME))
            .arg("-noout")
            .arg("-pubkey")
            .output()
            .unwrap();
        assert!(key_pub.status.success() && crt_pub.status.success());
        assert_eq!(
            key_pub.stdout, crt_pub.stdout,
            "installed pair is mismatched"
        );

        // And the SAN carries the new hostname, which is why we regenerate.
        let text = std::process::Command::new(OPENSSL_BIN)
            .arg("x509")
            .arg("-in")
            .arg(dir.path().join(TLS_CRT_NAME))
            .arg("-noout")
            .arg("-text")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&text.stdout);
        assert!(text.contains("DNS:newname"), "SAN missing hostname: {text}");
        assert!(text.contains("DNS:newname.local"));
        assert!(text.contains("IP Address:127.0.0.1"));
    }

    /// The staging private key must never exist world- or group-readable, not
    /// even transiently: it is created pre-moded and openssl only truncates it.
    #[tokio::test]
    async fn staging_key_is_created_already_locked_down() {
        let dir = tempfile::tempdir().unwrap();
        let m = material(dir.path(), Path::new(OPENSSL_BIN));
        m.create_empty(&m.key_staging(), TLS_KEY_FALLBACK_MODE, None)
            .await
            .unwrap();
        let mode = std::fs::metadata(m.key_staging())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn private_key_mode_preserves_tight_modes_and_never_widens() {
        assert_eq!(private_key_mode(Some("600")), "600");
        assert_eq!(private_key_mode(Some("400")), "400");
        // A live key that is somehow group/world readable is not reproduced.
        assert_eq!(private_key_mode(Some("644")), "600");
        assert_eq!(private_key_mode(Some("640")), "600");
        // No existing file, or an unparseable mode, falls back to 0600.
        assert_eq!(private_key_mode(None), "600");
        assert_eq!(private_key_mode(Some("garbage")), "600");
    }
}

/// The `host_secrets` object in `system.stats` (phase 10 KEY-02 / audit F-03).
///
/// These pin the contract that matters for a *cacheable, polled* method: the
/// absent and malformed cases must be indistinguishable from a plain "not
/// judged yet", never an error and never a panic. Every fleet node is in the
/// absent case until the OTA carrying the audit unit reaches it, so the absent
/// case is the common one, not the edge one.
#[cfg(test)]
mod host_secrets_tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    #[tokio::test]
    async fn verdict_is_unknown_when_the_audit_file_is_absent() {
        let dir = tempfile::tempdir().unwrap();
        let v = host_secrets_status(dir.path()).await;
        assert_eq!(v["verdict"], "unknown");
        assert!(v.get("checked_at").is_none());
        assert!(v.get("rotated_at").is_none());
    }

    #[tokio::test]
    async fn verdict_is_unknown_when_the_audit_file_is_unparseable() {
        let dir = tempfile::tempdir().unwrap();
        // A half-written file: the script writes to .tmp and renames, so this
        // should not happen — which is exactly why the daemon must survive it
        // if it ever does.
        write(dir.path(), HOST_SECRETS_AUDIT_FILE, "{\"verdict\": \"shar");
        let v = host_secrets_status(dir.path()).await;
        assert_eq!(v["verdict"], "unknown");
    }

    #[tokio::test]
    async fn recorded_verdict_and_evidence_are_surfaced() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            HOST_SECRETS_AUDIT_FILE,
            r#"{
              "verdict": "shared",
              "checked_at": "2026-08-02T12:00:00Z",
              "evidence": ["shared: /etc/ssh/ssh_host_rsa_key mtime is older than the anchor"],
              "ssh_host_key_fingerprints": ["/etc/ssh/ssh_host_rsa_key.pub: 3072 SHA256:abc"],
              "tls_cert_sha256": "AA:BB:CC"
            }"#,
        );
        let v = host_secrets_status(dir.path()).await;
        assert_eq!(v["verdict"], "shared");
        assert_eq!(v["checked_at"], "2026-08-02T12:00:00Z");
        assert_eq!(v["evidence"].as_array().unwrap().len(), 1);

        // The digests stay on disk. A payload polled every few seconds does not
        // carry them, and a future edit that "helpfully" forwards the whole
        // file should fail here.
        assert!(v.get("ssh_host_key_fingerprints").is_none());
        assert!(v.get("tls_cert_sha256").is_none());
    }

    #[tokio::test]
    async fn rotated_at_is_surfaced_only_when_a_rotation_was_recorded() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            HOST_SECRETS_AUDIT_FILE,
            r#"{"verdict":"per-node","checked_at":"2026-08-02T12:05:00Z","evidence":[]}"#,
        );
        assert!(host_secrets_status(dir.path())
            .await
            .get("rotated_at")
            .is_none());

        write(
            dir.path(),
            HOST_KEY_ROTATION_FILE,
            r#"{"rotated_at":"2026-08-02T12:04:00Z","old_ssh_fingerprints":[],"old_tls_sha256":"AA"}"#,
        );
        let v = host_secrets_status(dir.path()).await;
        assert_eq!(v["verdict"], "per-node");
        assert_eq!(v["rotated_at"], "2026-08-02T12:04:00Z");
    }
}

#[cfg(test)]
mod ai_grants_tests {
    use super::*;

    /// The assistant grants file is the authority: when it exists, the
    /// legacy UI-store file must NOT widen it. Regression test for the
    /// live archi-dev-box desync — legacy said all-ten, grants.json said
    /// four, and the union resurrected the six closed categories into the
    /// UI on every read.
    #[tokio::test]
    async fn existing_grants_file_outranks_legacy() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("assistant")).unwrap();
        std::fs::create_dir_all(dir.path().join("settings")).unwrap();
        std::fs::write(
            dir.path().join("assistant/grants.json"),
            r#"{"categories":["apps"]}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("settings/ai_permissions.json"),
            r#"{"granted":["apps","wallet"]}"#,
        )
        .unwrap();

        let got = RpcHandler::ai_grants_unified(dir.path()).await;
        assert_eq!(
            got,
            vec!["apps".to_string()],
            "legacy file must not widen the authority"
        );
    }

    /// With no assistant grants file yet (pre-unification upgrade), the
    /// legacy set folds forward and is persisted as the authoritative
    /// file, so subsequent reads answer from it.
    #[tokio::test]
    async fn missing_grants_file_migrates_legacy_forward() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("settings")).unwrap();
        std::fs::write(
            dir.path().join("settings/ai_permissions.json"),
            r#"{"granted":["apps","wallet"]}"#,
        )
        .unwrap();

        let got = RpcHandler::ai_grants_unified(dir.path()).await;
        assert_eq!(got, vec!["apps".to_string(), "wallet".to_string()]);
        assert!(
            dir.path().join("assistant/grants.json").exists(),
            "migration must persist the authoritative file"
        );
        // Second read answers from the migrated file even if legacy is gone.
        std::fs::remove_file(dir.path().join("settings/ai_permissions.json")).unwrap();
        let got2 = RpcHandler::ai_grants_unified(dir.path()).await;
        assert_eq!(got2, vec!["apps".to_string(), "wallet".to_string()]);
    }
}
