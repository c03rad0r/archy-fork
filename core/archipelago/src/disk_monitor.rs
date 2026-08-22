// Disk Space Monitor
// Periodically checks disk usage and triggers automatic cleanup at 90%.

use anyhow::{Context, Result};
use tracing::{info, warn};

/// Parse df output into (used_bytes, usable_total_bytes, used_percent).
/// Expects `df --block-size=1 --output=used,size,avail <path>`: a header line
/// followed by used, size and avail.
///
/// `size` is deliberately NOT the denominator. ext4 reserves 5% of the
/// filesystem for root — 92 GiB on archi-dev-box's 1.8 TiB disk — which `size`
/// counts but no ordinary process can ever allocate. Dividing by `size`
/// under-reports usage by about five points: on 2026-08-22 that disk was
/// genuinely 90.8% full (159 GiB usable left) while this returned 86.2%, so the
/// 90% auto-cleanup below had never once fired and ~72 GB of dangling images
/// had accumulated. It also meant the dashboard advertised 251 GiB free when
/// only 159 GiB could actually be written. used/(used+avail) is what `df`
/// itself prints and what the operator can actually spend.
fn parse_df_output(stdout: &str) -> Result<(u64, u64, f64)> {
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
    // Parsed to keep the column contract explicit, then intentionally unused —
    // see the note above on why raw size is the wrong denominator.
    let _size: u64 = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Missing size"))?
        .parse()
        .context("parse df size")?;
    let avail: u64 = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Missing avail"))?
        .parse()
        .context("parse df avail")?;
    let total = used.saturating_add(avail);

    let percent = if total > 0 {
        (used as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    Ok((used, total, percent))
}

/// Check disk usage percentage for the data partition.
/// Uses /var/lib/archipelago (encrypted LUKS partition) if available, falls back to /.
/// Returns (used_bytes, total_bytes, used_percent).
pub async fn check_disk_usage() -> Result<(u64, u64, f64)> {
    // Prefer the encrypted data partition — this is where all user data lives
    let data_path = if std::path::Path::new("/var/lib/archipelago").exists() {
        "/var/lib/archipelago"
    } else {
        "/"
    };
    let output = tokio::process::Command::new("df")
        .args(["--block-size=1", "--output=used,size,avail", data_path])
        .output()
        .await
        .context("Failed to run df")?;

    if !output.status.success() {
        anyhow::bail!("df failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = String::from_utf8(output.stdout).context("df output not utf8")?;
    parse_df_output(&stdout)
}

/// Run automatic cleanup when disk usage exceeds 90%.
async fn auto_cleanup() -> Result<u64> {
    let mut freed: u64 = 0;

    // Prune dangling images
    let output = tokio::process::Command::new("podman")
        .args(["image", "prune", "-f"])
        .output()
        .await;
    if let Ok(out) = output {
        if out.status.success() {
            let count = String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .count();
            freed += count as u64 * 100_000_000;
        }
    }

    // Clean old rotated logs (> 14 days for auto-cleanup, more aggressive)
    let _ = tokio::process::Command::new("sudo")
        .args([
            "find", "/var/log", "-type", "f", "-name", "*.log.*", "-mtime", "+14", "-delete",
        ])
        .output()
        .await;

    let _ = tokio::process::Command::new("sudo")
        .args([
            "find", "/var/log", "-type", "f", "-name", "*.gz", "-mtime", "+14", "-delete",
        ])
        .output()
        .await;

    // Truncate large journal logs
    let _ = tokio::process::Command::new("sudo")
        .args(["journalctl", "--vacuum-size=100M"])
        .output()
        .await;

    Ok(freed)
}

/// Check for OOM kills in kernel logs.
/// Returns a list of process names that were OOM-killed since boot.
async fn check_oom_kills() -> Vec<String> {
    let output = tokio::process::Command::new("sudo")
        .args(["dmesg", "--level=err,crit", "--notime"])
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .filter(|l| l.contains("oom-kill") || l.contains("Out of memory"))
                .map(|l| l.to_string())
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Spawn a background task that monitors disk usage every 5 minutes.
/// Also checks for OOM kills and tracks disk growth rate.
pub fn spawn_disk_monitor(data_dir: std::path::PathBuf) {
    tokio::spawn(async move {
        // Initial delay to let system stabilize
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        let mut last_warning_level: Option<&str> = None;
        let mut last_disk_used: Option<u64> = None;
        let mut last_oom_count: usize = 0;
        let mut disk_samples: Vec<(std::time::Instant, u64)> = Vec::new();

        loop {
            interval.tick().await;

            // Check for OOM kills
            let oom_lines = check_oom_kills().await;
            if oom_lines.len() > last_oom_count {
                let new_kills = &oom_lines[last_oom_count..];
                for kill in new_kills {
                    warn!("OOM kill detected: {}", kill);
                }
                // Write OOM alert for frontend
                let alert_path = data_dir.join("oom-alert.json");
                let _ = tokio::fs::write(
                    &alert_path,
                    serde_json::json!({
                        "count": oom_lines.len(),
                        "latest": oom_lines.last(),
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    })
                    .to_string(),
                )
                .await;
                last_oom_count = oom_lines.len();
            }

            match check_disk_usage().await {
                Ok((used, _total, percent)) => {
                    // Track disk growth rate
                    let now = std::time::Instant::now();
                    disk_samples.push((now, used));
                    // Keep only last 288 samples (24h at 5min intervals)
                    if disk_samples.len() > 288 {
                        disk_samples.remove(0);
                    }
                    // Calculate daily growth rate from oldest to newest sample
                    if disk_samples.len() >= 12 {
                        let (oldest_time, oldest_used) = disk_samples.first().unwrap();
                        let elapsed_hours =
                            now.duration_since(*oldest_time).as_secs() as f64 / 3600.0;
                        if elapsed_hours > 0.5 {
                            let growth_bytes = used.saturating_sub(*oldest_used);
                            let daily_growth_gb =
                                (growth_bytes as f64 / 1_073_741_824.0) * (24.0 / elapsed_hours);
                            if daily_growth_gb > 1.0 {
                                warn!(
                                    "Disk growing at {:.1} GB/day — may fill up",
                                    daily_growth_gb
                                );
                            }
                        }
                    }
                    let _ = last_disk_used.insert(used);
                    if percent >= 90.0 {
                        if last_warning_level != Some("critical") {
                            warn!(
                                "Disk usage critical: {:.1}% — triggering automatic cleanup",
                                percent
                            );
                            last_warning_level = Some("critical");
                        }
                        match auto_cleanup().await {
                            Ok(freed) => {
                                if freed > 0 {
                                    info!("Auto-cleanup freed approximately {} bytes", freed);
                                }
                            }
                            Err(e) => warn!("Auto-cleanup failed: {}", e),
                        }
                        // Write disk warning file for the frontend to poll
                        let warning_path = data_dir.join("disk-warning.json");
                        let _ = tokio::fs::write(
                            &warning_path,
                            serde_json::json!({
                                "level": "critical",
                                "percent": (percent * 10.0).round() / 10.0,
                                "timestamp": chrono::Utc::now().to_rfc3339(),
                            })
                            .to_string(),
                        )
                        .await;
                    } else if percent >= 85.0 {
                        if last_warning_level != Some("warning") {
                            warn!(
                                "Disk usage warning: {:.1}% — approaching critical threshold",
                                percent
                            );
                            last_warning_level = Some("warning");
                        }
                        let warning_path = data_dir.join("disk-warning.json");
                        let _ = tokio::fs::write(
                            &warning_path,
                            serde_json::json!({
                                "level": "warning",
                                "percent": (percent * 10.0).round() / 10.0,
                                "timestamp": chrono::Utc::now().to_rfc3339(),
                            })
                            .to_string(),
                        )
                        .await;
                    } else {
                        // Clear warning file if disk is healthy
                        if last_warning_level.is_some() {
                            let warning_path = data_dir.join("disk-warning.json");
                            let _ = tokio::fs::remove_file(&warning_path).await;
                            last_warning_level = None;
                            info!("Disk usage back to normal: {:.1}%", percent);
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!("Disk usage check failed (non-fatal): {}", e);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_df_output_normal() {
        // df --block-size=1 --output=used,size,avail : used, size, avail
        let output = "      Used       Size      Avail\n 500000000000 1000000000000 500000000000\n";
        let (used, total, percent) = parse_df_output(output).unwrap();
        assert_eq!(used, 500_000_000_000);
        assert_eq!(total, 1_000_000_000_000);
        assert!((percent - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_df_output_high_usage() {
        let output = "      Used       Size      Avail\n 900000000000 1000000000000 100000000000\n";
        let (used, total, percent) = parse_df_output(output).unwrap();
        assert_eq!(used, 900_000_000_000);
        assert_eq!(total, 1_000_000_000_000);
        assert!((percent - 90.0).abs() < 0.01);
    }

    /// The bug this function existed to hide: reserved blocks are counted by
    /// `size` but are not available to anyone. Real numbers from archi-dev-box,
    /// 2026-08-22 — 1.8 TiB disk, ext4 5% reserve, genuinely 90.8% full. The old
    /// used/size math returned 86.2%, so the 90% auto-cleanup never triggered.
    #[test]
    fn reserved_blocks_are_not_counted_as_free() {
        let output = "Used Size Avail\n1681459122176 1951249276928 170581372928\n";
        let (used, total, percent) = parse_df_output(output).unwrap();
        assert_eq!(used, 1_681_459_122_176);
        // Total is what can actually be written, not the raw device size.
        assert_eq!(total, 1_852_040_495_104);
        assert!(
            total < 1_951_249_276_928,
            "raw size must not be the denominator"
        );
        assert!((percent - 90.8).abs() < 0.1, "got {percent}");
        assert!(percent >= 90.0, "must cross the auto-cleanup threshold");
    }

    #[test]
    fn test_parse_df_output_almost_full() {
        let output = "Used Size Avail\n999 1000 1\n";
        let (used, total, percent) = parse_df_output(output).unwrap();
        assert_eq!(used, 999);
        assert_eq!(total, 1000);
        assert!((percent - 99.9).abs() < 0.01);
    }

    #[test]
    fn test_parse_df_output_empty_disk() {
        let output = "Used Size Avail\n0 1000000000000 1000000000000\n";
        let (used, total, percent) = parse_df_output(output).unwrap();
        assert_eq!(used, 0);
        assert_eq!(total, 1_000_000_000_000);
        assert!((percent - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_df_output_zero_total() {
        // Edge case: total is 0 (should not happen but should not panic/divide-by-zero)
        let output = "Used Size Avail\n0 0 0\n";
        let (used, total, percent) = parse_df_output(output).unwrap();
        assert_eq!(used, 0);
        assert_eq!(total, 0);
        assert!((percent - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_df_output_no_data_line() {
        let output = "Used Size\n";
        let result = parse_df_output(output);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_df_output_empty_string() {
        let result = parse_df_output("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_df_output_single_header_only() {
        let output = "Header Only";
        let result = parse_df_output(output);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_df_output_non_numeric() {
        let output = "Used Size\nabc def\n";
        let result = parse_df_output(output);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_df_output_missing_second_field() {
        let output = "Used Size\n12345\n";
        let result = parse_df_output(output);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_df_output_extra_whitespace() {
        let output = "  Used    Size   Avail \n    123456    7890000   7766544  \n";
        let (used, total, _) = parse_df_output(output).unwrap();
        assert_eq!(used, 123456);
        assert_eq!(total, 7_890_000);
    }

    #[test]
    fn test_parse_df_output_real_world_format() {
        // Closer to real df output with header padding
        // Real df output carries a reserved-block gap: size here is 1.8 TB but
        // only 1.382 TB is available, so usable total is used + avail.
        let output = "          Used         Size        Avail\n  328000000000  1800000000000  1382000000000\n";
        let (used, total, percent) = parse_df_output(output).unwrap();
        assert_eq!(used, 328_000_000_000);
        assert_eq!(total, 1_710_000_000_000);
        // ~19.2% against usable space, not 18.2% against the raw device.
        assert!(percent > 19.0 && percent < 20.0, "got {percent}");
    }

    #[tokio::test]
    async fn test_disk_warning_json_format() {
        // Verify that the JSON structure we write for disk warnings is valid
        let percent: f64 = 92.3;
        let json = serde_json::json!({
            "level": "critical",
            "percent": (percent * 10.0).round() / 10.0,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        let s = json.to_string();
        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed["level"], "critical");
        assert_eq!(parsed["percent"], 92.3);
        assert!(parsed["timestamp"].is_string());
    }

    #[tokio::test]
    async fn test_disk_warning_json_warning_level() {
        let percent: f64 = 87.5;
        let json = serde_json::json!({
            "level": "warning",
            "percent": (percent * 10.0).round() / 10.0,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        let parsed: serde_json::Value = serde_json::from_str(&json.to_string()).unwrap();
        assert_eq!(parsed["level"], "warning");
        // 87.5 rounded to 1 decimal = 87.5
        assert_eq!(parsed["percent"], 87.5);
    }
}
