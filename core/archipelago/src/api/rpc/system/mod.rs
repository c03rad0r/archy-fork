mod handlers;

use crate::update::host_sudo;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::{debug, info};

/// Push the server name to all federation peers by syncing state.
pub(super) async fn push_name_to_peers(
    data_dir: &std::path::Path,
    state_manager: &std::sync::Arc<crate::state::StateManager>,
) -> Result<()> {
    use crate::{federation, identity};

    let nodes = federation::load_nodes(data_dir).await?;
    if nodes.is_empty() {
        return Ok(());
    }

    let (data, _) = state_manager.get_snapshot().await;
    let local_did = identity::did_key_from_pubkey_hex(&data.server_info.pubkey)?;
    let identity_dir = data_dir.join("identity");
    let node_identity = identity::NodeIdentity::load_or_create(&identity_dir).await?;

    let mut synced = 0u32;
    for node in &nodes {
        if node.trust_level == federation::TrustLevel::Untrusted {
            continue;
        }
        match federation::sync_with_peer(data_dir, node, &local_did, |bytes| {
            node_identity.sign(bytes)
        })
        .await
        {
            Ok(_) => synced += 1,
            Err(e) => debug!(
                "Sync with {} after rename: {}",
                node.did.chars().take(20).collect::<String>(),
                e
            ),
        }
    }
    info!("Pushed server name to {}/{} peers", synced, nodes.len());
    Ok(())
}

/// Read system uptime from /proc/uptime (seconds since boot).
pub(super) async fn read_uptime() -> Result<f64> {
    let content = tokio::fs::read_to_string("/proc/uptime")
        .await
        .context("Failed to read /proc/uptime")?;
    let uptime: f64 = content
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Empty /proc/uptime"))?
        .parse()
        .context("Failed to parse uptime")?;
    Ok(uptime)
}

/// Read load averages from /proc/loadavg.
pub(super) async fn read_loadavg() -> Result<(f64, f64, f64)> {
    let content = tokio::fs::read_to_string("/proc/loadavg")
        .await
        .context("Failed to read /proc/loadavg")?;
    let mut parts = content.split_whitespace();
    let l1: f64 = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Missing load1"))?
        .parse()
        .context("parse load1")?;
    let l5: f64 = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Missing load5"))?
        .parse()
        .context("parse load5")?;
    let l15: f64 = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Missing load15"))?
        .parse()
        .context("parse load15")?;
    Ok((l1, l5, l15))
}

/// Compute CPU usage by sampling /proc/stat twice with a 250ms gap.
pub(super) async fn read_cpu_usage() -> Result<f64> {
    let snap1 = read_cpu_jiffies().await?;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let snap2 = read_cpu_jiffies().await?;

    let total_delta = snap2.total.saturating_sub(snap1.total);
    let idle_delta = snap2.idle.saturating_sub(snap1.idle);

    if total_delta == 0 {
        return Ok(0.0);
    }

    let usage = 100.0 * (1.0 - (idle_delta as f64 / total_delta as f64));
    Ok((usage * 10.0).round() / 10.0) // one decimal
}

struct CpuJiffies {
    total: u64,
    idle: u64,
}

async fn read_cpu_jiffies() -> Result<CpuJiffies> {
    let content = tokio::fs::read_to_string("/proc/stat")
        .await
        .context("Failed to read /proc/stat")?;
    let cpu_line = content
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Empty /proc/stat"))?;
    // cpu  user nice system idle iowait irq softirq steal guest guest_nice
    let vals: Vec<u64> = cpu_line
        .split_whitespace()
        .skip(1) // skip "cpu"
        .filter_map(|v| v.parse().ok())
        .collect();
    if vals.len() < 4 {
        anyhow::bail!("Not enough fields in /proc/stat cpu line");
    }
    let idle = vals[3]; // idle column
    let total: u64 = vals.iter().sum();
    Ok(CpuJiffies { total, idle })
}

/// Read memory info from /proc/meminfo.
/// Returns (used_bytes, total_bytes).
pub(super) async fn read_meminfo() -> Result<(u64, u64)> {
    let content = tokio::fs::read_to_string("/proc/meminfo")
        .await
        .context("Failed to read /proc/meminfo")?;

    let mut total_kb: u64 = 0;
    let mut available_kb: u64 = 0;

    for line in content.lines() {
        if let Some(val) = line.strip_prefix("MemTotal:") {
            total_kb = parse_meminfo_kb(val)?;
        } else if let Some(val) = line.strip_prefix("MemAvailable:") {
            available_kb = parse_meminfo_kb(val)?;
        }
    }

    let used_bytes = total_kb.saturating_sub(available_kb) * 1024;
    let total_bytes = total_kb * 1024;
    Ok((used_bytes, total_bytes))
}

pub(super) fn parse_meminfo_kb(val: &str) -> Result<u64> {
    val.trim()
        .trim_end_matches("kB")
        .trim()
        .parse::<u64>()
        .context("parse meminfo value")
}

/// Read disk usage via `df` for the root filesystem.
/// Returns (used_bytes, total_bytes).
#[allow(dead_code)]
pub(super) async fn read_disk_usage() -> Result<(u64, u64)> {
    read_disk_usage_path("/").await
}

/// Read disk usage via `df` for a given path.
pub(super) async fn read_disk_usage_path(path: &str) -> Result<(u64, u64)> {
    let output = tokio::process::Command::new("df")
        .args(["--block-size=1", "--output=used,size", path])
        .output()
        .await
        .context("Failed to run df")?;

    if !output.status.success() {
        anyhow::bail!("df failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = String::from_utf8(output.stdout).context("df output not utf8")?;
    // Skip header line
    let data_line = stdout
        .lines()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("No data line from df"))?;
    let mut parts = data_line.split_whitespace();
    let used: u64 = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Missing used"))?
        .parse()
        .context("parse df used")?;
    let total: u64 = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Missing total"))?
        .parse()
        .context("parse df total")?;

    Ok((used, total))
}

/// Read top 10 processes by CPU from `ps`.
pub(super) async fn read_top_processes() -> Result<Vec<serde_json::Value>> {
    let output = tokio::process::Command::new("ps")
        .args(["--no-headers", "-eo", "pid,%cpu,%mem,comm", "--sort=-%cpu"])
        .output()
        .await
        .context("Failed to run ps")?;

    if !output.status.success() {
        anyhow::bail!("ps failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = String::from_utf8(output.stdout).context("ps output not utf8")?;
    let procs: Vec<serde_json::Value> = stdout
        .lines()
        .take(10)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pid = parts.next()?.parse::<u32>().ok()?;
            let cpu: f64 = parts.next()?.parse().ok()?;
            let mem: f64 = parts.next()?.parse().ok()?;
            let name = parts.collect::<Vec<_>>().join(" ");
            Some(serde_json::json!({
                "pid": pid,
                "cpu_percent": cpu,
                "mem_percent": mem,
                "name": name,
            }))
        })
        .collect();

    Ok(procs)
}

/// Known hardware wallet USB vendor IDs.
const KNOWN_HW_WALLETS: &[(u16, &str)] = &[
    (0xd13e, "ColdCard"),
    (0x534c, "Trezor"),
    (0x2c97, "Ledger"),
    (0x1209, "BitBox02"),
];

/// Scan /sys/bus/usb/devices/ for known hardware wallet vendor IDs.
pub(super) async fn detect_usb_hardware_wallets() -> Result<Vec<serde_json::Value>> {
    let usb_dir = std::path::Path::new("/sys/bus/usb/devices");
    if !usb_dir.exists() {
        return Ok(Vec::new());
    }

    let mut devices = Vec::new();
    let mut entries = tokio::fs::read_dir(usb_dir)
        .await
        .context("Failed to read /sys/bus/usb/devices")?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let vendor_path = path.join("idVendor");
        let product_path = path.join("idProduct");

        if !vendor_path.exists() {
            continue;
        }

        let vid_str = match tokio::fs::read_to_string(&vendor_path).await {
            Ok(s) => s.trim().to_string(),
            Err(_) => continue,
        };

        let vid = match u16::from_str_radix(&vid_str, 16) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some((_, name)) = KNOWN_HW_WALLETS
            .iter()
            .find(|(known_vid, _)| *known_vid == vid)
        {
            let pid_str = tokio::fs::read_to_string(&product_path)
                .await
                .map(|s| s.trim().to_string())
                .unwrap_or_default();

            let manufacturer = tokio::fs::read_to_string(path.join("manufacturer"))
                .await
                .map(|s| s.trim().to_string())
                .unwrap_or_default();

            let product = tokio::fs::read_to_string(path.join("product"))
                .await
                .map(|s| s.trim().to_string())
                .unwrap_or_default();

            devices.push(serde_json::json!({
                "type": name,
                "vendor_id": vid_str,
                "product_id": pid_str,
                "manufacturer": manufacturer,
                "product": product,
                "path": path.to_string_lossy(),
            }));
        }
    }

    Ok(devices)
}

/// Clean log files older than `max_age_days` from common log directories.
pub(super) async fn clean_old_logs(max_age_days: u64) -> Result<u64> {
    let output = tokio::process::Command::new("timeout")
        .args([
            "60s",
            "sudo",
            "find",
            "/var/log",
            "-type",
            "f",
            "-name",
            "*.log.*",
            "-mtime",
            &format!("+{}", max_age_days),
            "-delete",
            "-print",
        ])
        .output()
        .await
        .context("Failed to clean old logs")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let deleted_count = stdout.lines().filter(|l| !l.trim().is_empty()).count();
    // Also clean rotated/compressed logs
    let _ = tokio::process::Command::new("timeout")
        .args([
            "60s",
            "sudo",
            "find",
            "/var/log",
            "-type",
            "f",
            "-name",
            "*.gz",
            "-mtime",
            &format!("+{}", max_age_days),
            "-delete",
        ])
        .output()
        .await;

    Ok(deleted_count as u64 * 500_000) // rough estimate per log file
}

/// Vacuum systemd journals to a bounded size. Returns measured bytes freed.
pub(super) async fn vacuum_journal_logs(max_size: &str) -> Result<u64> {
    let before = journal_disk_usage().await.unwrap_or(0);
    let output = tokio::process::Command::new("timeout")
        .args(["60s", "sudo", "journalctl", "--vacuum-size", max_size])
        .output()
        .await
        .context("Failed to run journal vacuum")?;

    if !output.status.success() {
        anyhow::bail!(
            "journal vacuum failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let after = journal_disk_usage().await.unwrap_or(before);
    Ok(before.saturating_sub(after))
}

async fn journal_disk_usage() -> Result<u64> {
    let output = tokio::process::Command::new("sudo")
        .args(["-n", "journalctl", "--disk-usage"])
        .output()
        .await
        .context("Failed to read journal disk usage")?;

    if !output.status.success() {
        anyhow::bail!(
            "journalctl --disk-usage failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    parse_journal_disk_usage(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| anyhow::anyhow!("could not parse journal disk usage"))
}

fn parse_journal_disk_usage(output: &str) -> Option<u64> {
    let mut parts = output.split_whitespace();
    while let Some(part) = parts.next() {
        let (number, inline_unit) = split_number_unit(part);
        let Ok(value) = number.parse::<f64>() else {
            continue;
        };
        let unit = inline_unit.unwrap_or_else(|| parts.next().unwrap_or_default());
        let multiplier = match unit {
            "B" | "bytes" => 1.0,
            "K" | "KB" | "KiB" => 1024.0,
            "M" | "MB" | "MiB" => 1024.0 * 1024.0,
            "G" | "GB" | "GiB" => 1024.0 * 1024.0 * 1024.0,
            _ => continue,
        };
        return Some((value * multiplier) as u64);
    }
    None
}

fn split_number_unit(value: &str) -> (&str, Option<&str>) {
    let split_at = value
        .char_indices()
        .find_map(|(idx, ch)| (!ch.is_ascii_digit() && ch != '.').then_some(idx))
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(split_at);
    (number, (!unit.is_empty()).then_some(unit))
}

/// Remove stale temp files from /tmp and /var/tmp.
pub(super) async fn clean_temp_files() -> Result<u64> {
    let mut freed = 0u64;

    for dir in &["/tmp", "/var/tmp"] {
        let output = tokio::process::Command::new("timeout")
            .args([
                "45s", "sudo", "find", dir, "-type", "f", "-mtime", "+7", "-delete", "-print",
            ])
            .output()
            .await;

        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let count = stdout.lines().filter(|l| !l.trim().is_empty()).count();
            freed += count as u64 * 100_000; // rough estimate per temp file
        }
    }

    Ok(freed)
}

/// Keep the newest timestamped backend backups and remove older ones.
pub(super) async fn clean_backend_backups(keep: usize) -> Result<u64> {
    clean_backend_backups_in(Path::new("/usr/local/bin"), keep).await
}

/// Keep the newest legacy backend backups and remove older alpha-era deploy artifacts.
pub(super) async fn clean_legacy_backend_backups(keep: usize) -> Result<u64> {
    clean_named_backups_in(
        Path::new("/usr/local/bin"),
        keep,
        |name| name.starts_with("archipelago.bak") || name.starts_with("archipelago.before-"),
        false,
    )
    .await
}

/// Keep the newest web UI rollback backups and remove older copies.
pub(super) async fn clean_web_ui_backups(keep: usize) -> Result<u64> {
    clean_named_backups_in(
        Path::new("/opt/archipelago"),
        keep,
        |name| name.starts_with("web-ui.bak") || name == "web-ui.old",
        true,
    )
    .await
}

async fn clean_backend_backups_in(dir: &Path, keep: usize) -> Result<u64> {
    let mut backups = backend_backup_candidates(dir).await?;
    remove_old_backups(&mut backups, keep, false).await
}

async fn clean_named_backups_in(
    dir: &Path,
    keep: usize,
    matches_name: impl Fn(&str) -> bool,
    allow_dirs: bool,
) -> Result<u64> {
    let mut backups = named_backup_candidates(dir, matches_name, allow_dirs).await?;
    remove_old_backups(&mut backups, keep, allow_dirs).await
}

async fn remove_old_backups(
    backups: &mut Vec<BackupArtifact>,
    keep: usize,
    allow_dirs: bool,
) -> Result<u64> {
    backups.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| b.name.cmp(&a.name))
    });

    let mut freed = 0u64;
    for backup in backups.iter().skip(keep) {
        let remove_result = if backup.is_dir && allow_dirs {
            tokio::fs::remove_dir_all(&backup.path).await
        } else {
            tokio::fs::remove_file(&backup.path).await
        };
        match remove_result {
            Ok(()) => freed += backup.size,
            Err(_) => {
                remove_path_with_sudo(&backup.path, backup.is_dir && allow_dirs).await?;
                freed += backup.size;
            }
        }
    }
    Ok(freed)
}

async fn remove_path_with_sudo(path: &Path, recursive: bool) -> Result<()> {
    let path = path.to_string_lossy();
    let args = if recursive {
        vec!["rm", "-rf", path.as_ref()]
    } else {
        vec!["rm", "-f", path.as_ref()]
    };
    let status = host_sudo(&args)
        .await
        .with_context(|| format!("removing {path} via sudo"))?;
    if !status.success() {
        anyhow::bail!(
            "sudo rm {} {path} exited with {status}",
            if recursive { "-rf" } else { "-f" }
        );
    }
    Ok(())
}

#[derive(Debug)]
struct BackupArtifact {
    path: PathBuf,
    name: String,
    modified: SystemTime,
    size: u64,
    is_dir: bool,
}

async fn backend_backup_candidates(dir: &Path) -> Result<Vec<BackupArtifact>> {
    named_backup_candidates(
        dir,
        |name| {
            name.strip_prefix("archipelago.backup-")
                .is_some_and(|suffix| !suffix.is_empty() && !suffix.contains('/'))
        },
        false,
    )
    .await
}

async fn named_backup_candidates(
    dir: &Path,
    matches_name: impl Fn(&str) -> bool,
    allow_dirs: bool,
) -> Result<Vec<BackupArtifact>> {
    let mut backups = Vec::new();
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(backups),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };

    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if !matches_name(&name) {
            continue;
        }

        let meta = entry.metadata().await?;
        if !meta.is_file() && !(allow_dirs && meta.is_dir()) {
            continue;
        }

        backups.push(BackupArtifact {
            path: entry.path(),
            name: name.to_string(),
            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            size: path_size(&entry.path(), &meta).await.unwrap_or(meta.len()),
            is_dir: meta.is_dir(),
        });
    }
    Ok(backups)
}

async fn path_size(path: &Path, meta: &std::fs::Metadata) -> Result<u64> {
    if meta.is_file() {
        return Ok(meta.len());
    }
    if !meta.is_dir() {
        return Ok(0);
    }

    let output = tokio::process::Command::new("du")
        .args(["-sb", &path.to_string_lossy()])
        .output()
        .await
        .with_context(|| format!("du -sb {}", path.display()))?;
    if !output.status.success() {
        anyhow::bail!("du -sb {} failed", path.display());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("du output missing size for {}", path.display()))?
        .parse::<u64>()
        .with_context(|| format!("parse du size for {}", path.display()))
}

pub(super) fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn backend_backup_cleanup_keeps_newest_files() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "archipelago.backup-20260501",
            "archipelago.backup-20260502",
            "archipelago.backup-20260503",
            "archipelago.backup-20260504",
            "archipelago.backup-20260505",
            "archipelago.bak",
            "archipelago",
        ] {
            tokio::fs::write(dir.path().join(name), b"12345")
                .await
                .unwrap();
        }

        let freed = clean_backend_backups_in(dir.path(), 3).await.unwrap();

        assert_eq!(freed, 10);
        assert!(!dir.path().join("archipelago.backup-20260501").exists());
        assert!(!dir.path().join("archipelago.backup-20260502").exists());
        assert!(dir.path().join("archipelago.backup-20260503").exists());
        assert!(dir.path().join("archipelago.backup-20260504").exists());
        assert!(dir.path().join("archipelago.backup-20260505").exists());
        assert!(dir.path().join("archipelago.bak").exists());
        assert!(dir.path().join("archipelago").exists());
    }

    #[tokio::test]
    async fn legacy_backend_backup_cleanup_keeps_newest_matching_files() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "archipelago.bak-1",
            "archipelago.bak-2",
            "archipelago.before-3",
            "archipelago.backup-keep-separate",
            "archipelago",
        ] {
            tokio::fs::write(dir.path().join(name), b"12345")
                .await
                .unwrap();
        }

        let freed = clean_named_backups_in(
            dir.path(),
            1,
            |name| name.starts_with("archipelago.bak") || name.starts_with("archipelago.before-"),
            false,
        )
        .await
        .unwrap();

        assert_eq!(freed, 10);
        assert_eq!(
            [
                "archipelago.bak-1",
                "archipelago.bak-2",
                "archipelago.before-3"
            ]
            .into_iter()
            .filter(|name| dir.path().join(name).exists())
            .count(),
            1
        );
        assert!(dir.path().join("archipelago.backup-keep-separate").exists());
        assert!(dir.path().join("archipelago").exists());
    }

    #[test]
    fn hostname_from_server_name_derives_linux_safe_hostname() {
        assert_eq!(
            handlers::hostname_from_server_name("My Archipelago Node"),
            "my-archipelago-node"
        );
        assert_eq!(
            handlers::hostname_from_server_name("Kitchen_Node!! 01"),
            "kitchen-node-01"
        );
        assert_eq!(handlers::hostname_from_server_name("!!!"), "archipelago");
    }

    #[test]
    fn parses_journal_disk_usage() {
        assert_eq!(
            parse_journal_disk_usage(
                "Archived and active journals take up 463.9M in the file system."
            ),
            Some(486_434_406)
        );
    }
}

/// Read temperatures from /sys/class/thermal/thermal_zone*/temp.
pub(super) async fn read_temperatures() -> Result<Vec<serde_json::Value>> {
    let mut temps = Vec::new();
    let thermal_dir = std::path::Path::new("/sys/class/thermal");
    if !thermal_dir.exists() {
        return Ok(temps);
    }

    let mut entries = tokio::fs::read_dir(thermal_dir)
        .await
        .context("Failed to read /sys/class/thermal")?;

    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with("thermal_zone") {
            continue;
        }

        let temp_path = entry.path().join("temp");
        let type_path = entry.path().join("type");

        let millideg = match tokio::fs::read_to_string(&temp_path).await {
            Ok(s) => s.trim().parse::<i64>().unwrap_or(0),
            Err(_) => continue,
        };

        let zone_type = tokio::fs::read_to_string(&type_path)
            .await
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| name_str.to_string());

        temps.push(serde_json::json!({
            "zone": zone_type,
            "temp_celsius": millideg as f64 / 1000.0,
        }));
    }

    Ok(temps)
}
