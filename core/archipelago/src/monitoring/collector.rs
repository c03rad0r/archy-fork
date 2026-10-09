use super::{ContainerMetrics, MetricSnapshot, SystemMetrics};
use anyhow::{Context, Result};

/// Collect a full metrics snapshot from the system.
pub async fn collect_snapshot() -> Result<MetricSnapshot> {
    let timestamp = chrono::Utc::now().timestamp();

    let (cpu, mem, disk, net, load) = tokio::join!(
        read_cpu_usage(),
        read_meminfo(),
        read_disk_usage(),
        read_network_totals(),
        read_loadavg(),
    );

    let cpu = cpu?;
    let (mem_used, mem_total) = mem?;
    let (disk_used, disk_total) = disk?;
    let (net_rx, net_tx) = net?;
    let (l1, l5, l15) = load?;

    let system = SystemMetrics {
        cpu_percent: cpu,
        mem_used_bytes: mem_used,
        mem_total_bytes: mem_total,
        disk_used_bytes: disk_used,
        disk_total_bytes: disk_total,
        net_rx_bytes: net_rx,
        net_tx_bytes: net_tx,
        load_avg_1: l1,
        load_avg_5: l5,
        load_avg_15: l15,
    };

    let containers = read_container_stats().await.unwrap_or_default();

    Ok(MetricSnapshot {
        timestamp,
        system,
        containers,
        rpc_latency_ms: 0.0, // filled in by MetricsStore::push
        ws_connections: 0,   // filled in by MetricsStore::push
    })
}

/// Compute CPU usage by sampling /proc/stat twice with a 250ms gap.
async fn read_cpu_usage() -> Result<f64> {
    let snap1 = read_cpu_jiffies().await?;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let snap2 = read_cpu_jiffies().await?;

    let total_delta = snap2.0.saturating_sub(snap1.0);
    let idle_delta = snap2.1.saturating_sub(snap1.1);

    if total_delta == 0 {
        return Ok(0.0);
    }

    let usage = 100.0 * (1.0 - (idle_delta as f64 / total_delta as f64));
    Ok((usage * 10.0).round() / 10.0)
}

/// Returns (total_jiffies, idle_jiffies) from /proc/stat.
async fn read_cpu_jiffies() -> Result<(u64, u64)> {
    let content = tokio::fs::read_to_string("/proc/stat")
        .await
        .context("Failed to read /proc/stat")?;
    let cpu_line = content
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Empty /proc/stat"))?;
    let vals: Vec<u64> = cpu_line
        .split_whitespace()
        .skip(1)
        .filter_map(|v| v.parse().ok())
        .collect();
    if vals.len() < 4 {
        anyhow::bail!("Not enough fields in /proc/stat cpu line");
    }
    let idle = vals[3];
    let total: u64 = vals.iter().sum();
    Ok((total, idle))
}

/// Read memory used/total from /proc/meminfo.
async fn read_meminfo() -> Result<(u64, u64)> {
    let content = tokio::fs::read_to_string("/proc/meminfo")
        .await
        .context("Failed to read /proc/meminfo")?;

    let mut total_kb: u64 = 0;
    let mut available_kb: u64 = 0;

    for line in content.lines() {
        if let Some(val) = line.strip_prefix("MemTotal:") {
            total_kb = parse_kb(val)?;
        } else if let Some(val) = line.strip_prefix("MemAvailable:") {
            available_kb = parse_kb(val)?;
        }
    }

    Ok((
        total_kb.saturating_sub(available_kb) * 1024,
        total_kb * 1024,
    ))
}

fn parse_kb(val: &str) -> Result<u64> {
    val.trim()
        .trim_end_matches("kB")
        .trim()
        .parse::<u64>()
        .context("parse meminfo kB value")
}

/// Read disk used/total via `df`, preferring the encrypted data partition.
async fn read_disk_usage() -> Result<(u64, u64)> {
    let target = if std::path::Path::new("/var/lib/archipelago").exists() {
        "/var/lib/archipelago"
    } else {
        "/"
    };
    let mut command = tokio::process::Command::new("df");
    command.args(["--block-size=1", "--output=used,size", target]);
    let output = bounded_output(command, std::time::Duration::from_secs(3))
        .await
        .context("Failed to run df")?;

    if !output.status.success() {
        anyhow::bail!("df failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = String::from_utf8(output.stdout).context("df output not utf8")?;
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

/// Read load averages from /proc/loadavg.
async fn read_loadavg() -> Result<(f64, f64, f64)> {
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

/// Read total network RX/TX bytes from /proc/net/dev (sum of all real interfaces).
async fn read_network_totals() -> Result<(u64, u64)> {
    let content = tokio::fs::read_to_string("/proc/net/dev")
        .await
        .context("Failed to read /proc/net/dev")?;

    let mut rx_total: u64 = 0;
    let mut tx_total: u64 = 0;

    for line in content.lines().skip(2) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Split on colon to separate interface name from data
        let (iface, data) = match line.split_once(':') {
            Some((i, d)) => (i.trim(), d),
            None => continue,
        };

        // Skip loopback
        if iface == "lo" {
            continue;
        }

        let parts: Vec<&str> = data.split_whitespace().collect();
        // Fields: rx_bytes rx_packets rx_errs ... (8 fields) tx_bytes tx_packets ...
        if parts.len() >= 10 {
            if let Ok(rx) = parts[0].parse::<u64>() {
                rx_total += rx;
            }
            if let Ok(tx) = parts[8].parse::<u64>() {
                tx_total += tx;
            }
        }
    }

    Ok((rx_total, tx_total))
}

/// A wedged runtime or filesystem must not freeze every monitoring snapshot.
/// Dropping a timed-out child kills it, so repeated polls cannot leak processes.
async fn bounded_output(
    mut command: tokio::process::Command,
    timeout: std::time::Duration,
) -> Result<std::process::Output> {
    command.kill_on_drop(true);
    tokio::time::timeout(timeout, command.output())
        .await
        .context("Metrics subprocess timed out")?
        .context("Metrics subprocess failed")
}

/// Get per-container resource stats via `podman stats --no-stream --format json`.
async fn read_container_stats() -> Result<Vec<ContainerMetrics>> {
    let mut command = tokio::process::Command::new("podman");
    command.args(["stats", "--no-stream", "--format", "json"]);
    let output = bounded_output(command, std::time::Duration::from_secs(8))
        .await
        .context("Failed to run podman stats")?;

    if !output.status.success() {
        anyhow::bail!(
            "podman stats failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let entries: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap_or_default();

    Ok(entries
        .iter()
        .filter_map(|e| {
            let name = e
                .get("name")
                .or_else(|| e.get("Name"))
                .and_then(|v| v.as_str())?
                .to_string();

            Some(ContainerMetrics {
                name,
                cpu_percent: parse_percent_field(e, "cpu_percent")
                    .or_else(|| parse_percent_field(e, "CPUPerc"))
                    .unwrap_or(0.0),
                mem_used_bytes: parse_bytes_field(e, "mem_usage")
                    .or_else(|| parse_bytes_field(e, "MemUsage"))
                    .unwrap_or(0),
                mem_limit_bytes: parse_bytes_field(e, "mem_limit")
                    .or_else(|| parse_bytes_field(e, "MemLimit"))
                    .unwrap_or(0),
                net_rx_bytes: parse_bytes_field(e, "net_input")
                    .or_else(|| parse_bytes_field(e, "NetInput"))
                    .unwrap_or(0),
                net_tx_bytes: parse_bytes_field(e, "net_output")
                    .or_else(|| parse_bytes_field(e, "NetOutput"))
                    .unwrap_or(0),
                block_read_bytes: parse_bytes_field(e, "block_input")
                    .or_else(|| parse_bytes_field(e, "BlockInput"))
                    .unwrap_or(0),
                block_write_bytes: parse_bytes_field(e, "block_output")
                    .or_else(|| parse_bytes_field(e, "BlockOutput"))
                    .unwrap_or(0),
            })
        })
        .collect())
}

/// Parse a percentage field that may be a number or a string like "12.5%".
fn parse_percent_field(obj: &serde_json::Value, key: &str) -> Option<f64> {
    let val = obj.get(key)?;
    if let Some(n) = val.as_f64() {
        return Some(n);
    }
    val.as_str()?
        .trim_end_matches('%')
        .trim()
        .parse::<f64>()
        .ok()
}

/// Parse a bytes field that may be a number or a human-readable string.
fn parse_bytes_field(obj: &serde_json::Value, key: &str) -> Option<u64> {
    let val = obj.get(key)?;
    if let Some(n) = val.as_u64() {
        return Some(n);
    }
    parse_human_bytes(val.as_str()?)
}

/// Parse human-readable byte strings like "1.5GiB", "256MiB", "100MiB / 16GiB".
fn parse_human_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    if s == "--" || s.is_empty() {
        return Some(0);
    }

    // Handle "X / Y" format — take only the first part
    let s = s.split('/').next()?.trim();

    let (num_str, multiplier) = if let Some(n) = s.strip_suffix("GiB") {
        (n, 1024u64 * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("MiB") {
        (n, 1024u64 * 1024)
    } else if let Some(n) = s.strip_suffix("KiB") {
        (n, 1024u64)
    } else if let Some(n) = s.strip_suffix("GB") {
        (n, 1_000_000_000u64)
    } else if let Some(n) = s.strip_suffix("MB") {
        (n, 1_000_000u64)
    } else if let Some(n) = s.strip_suffix("kB") {
        (n, 1000u64)
    } else if let Some(n) = s.strip_suffix('B') {
        (n, 1u64)
    } else {
        (s, 1u64)
    };

    let num: f64 = num_str.trim().parse().ok()?;
    Some((num * multiplier as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_human_bytes_gib() {
        assert_eq!(parse_human_bytes("1GiB"), Some(1073741824));
        assert_eq!(parse_human_bytes("1.5GiB"), Some(1610612736));
    }

    #[test]
    fn test_parse_human_bytes_mib() {
        assert_eq!(parse_human_bytes("256MiB"), Some(268435456));
    }

    #[test]
    fn test_parse_human_bytes_kib() {
        assert_eq!(parse_human_bytes("1024KiB"), Some(1048576));
    }

    #[test]
    fn test_parse_human_bytes_si() {
        assert_eq!(parse_human_bytes("1000MB"), Some(1000000000));
        assert_eq!(parse_human_bytes("100B"), Some(100));
    }

    #[test]
    fn test_parse_human_bytes_empty() {
        assert_eq!(parse_human_bytes("--"), Some(0));
        assert_eq!(parse_human_bytes(""), Some(0));
    }

    #[test]
    fn test_parse_human_bytes_slash_format() {
        assert_eq!(parse_human_bytes("100MiB / 16GiB"), Some(104857600));
    }

    #[test]
    fn test_parse_percent_string() {
        let obj = serde_json::json!({"cpu": "12.5%"});
        assert_eq!(parse_percent_field(&obj, "cpu"), Some(12.5));
    }

    #[test]
    fn test_parse_percent_number() {
        let obj = serde_json::json!({"cpu": 12.5});
        assert_eq!(parse_percent_field(&obj, "cpu"), Some(12.5));
    }

    #[test]
    fn test_parse_percent_missing() {
        let obj = serde_json::json!({"other": 1});
        assert_eq!(parse_percent_field(&obj, "cpu"), None);
    }

    #[test]
    fn test_parse_bytes_field_number() {
        let obj = serde_json::json!({"mem": 1048576});
        assert_eq!(parse_bytes_field(&obj, "mem"), Some(1048576));
    }

    #[test]
    fn test_parse_bytes_field_string() {
        let obj = serde_json::json!({"mem": "256MiB"});
        assert_eq!(parse_bytes_field(&obj, "mem"), Some(268435456));
    }
}

#[cfg(test)]
mod subprocess_deadline_tests {
    use super::*;

    #[tokio::test]
    async fn a_stalled_metrics_command_is_bounded_and_killed() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let mut command = tokio::process::Command::new("sh");
        command
            .arg("-c")
            .arg("echo $$ > \"$1\"; exec sleep 30")
            .arg("metrics-test")
            .arg(&pid_file);
        let start = std::time::Instant::now();
        let error = bounded_output(command, std::time::Duration::from_millis(500))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(start.elapsed() < std::time::Duration::from_secs(3));
        let pid = tokio::fs::read_to_string(pid_file).await.unwrap();
        for _ in 0..40 {
            if !std::path::Path::new(&format!("/proc/{}", pid.trim())).exists() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!("Timed-out metrics subprocess was not reaped");
    }

    #[tokio::test]
    async fn successful_metrics_output_is_preserved() {
        let mut command = tokio::process::Command::new("printf");
        command.arg("metrics-ok");
        let output = bounded_output(command, std::time::Duration::from_secs(1))
            .await
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"metrics-ok");
    }
}
