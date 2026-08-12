use crate::monitoring::store::MetricsStore;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Spawn the periodic telemetry reporter (runs every 15 minutes when opt-in enabled).
/// Collects anonymous health data and saves to disk. Posts to central collector if configured.
pub fn spawn_telemetry_reporter(
    store: Arc<MetricsStore>,
    state: Option<Arc<crate::state::StateManager>>,
    data_dir: PathBuf,
) {
    tokio::spawn(async move {
        // Wait 60s for system to fully stabilize
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(900)); // 15 min

        loop {
            interval.tick().await;

            // Check if telemetry is opted in
            let config_path = data_dir.join("analytics-config.json");
            let enabled = match tokio::fs::read_to_string(&config_path).await {
                Ok(data) => serde_json::from_str::<serde_json::Value>(&data)
                    .ok()
                    .and_then(|c| c["enabled"].as_bool())
                    .unwrap_or(false),
                Err(_) => false,
            };
            if !enabled {
                debug!("Telemetry disabled — skipping report");
                continue;
            }

            match build_telemetry_report(&store, &state, &data_dir).await {
                Ok(report) => {
                    // Save latest report to disk
                    let report_path = data_dir.join("telemetry-latest.json");
                    if let Ok(json) = serde_json::to_string_pretty(&report) {
                        let _ = tokio::fs::write(&report_path, &json).await;
                    }

                    // Always save to local fleet directory so this node appears
                    // in its own fleet view
                    save_report_to_fleet_dir(&data_dir, &report).await;

                    // POST to central collector if configured
                    let collector_url = std::env::var("TELEMETRY_COLLECTOR_URL").ok();
                    if let Some(url) = collector_url {
                        match post_telemetry_report(&url, &report).await {
                            Ok(_) => info!("Telemetry report sent to collector"),
                            Err(e) => warn!("Failed to send telemetry report: {}", e),
                        }
                    } else {
                        debug!("Telemetry report saved locally (no collector URL configured)");
                    }
                }
                Err(e) => {
                    warn!("Failed to build telemetry report: {}", e);
                }
            }
        }
    });
}

/// Build an anonymous telemetry report from current system state.
async fn build_telemetry_report(
    store: &Arc<MetricsStore>,
    state: &Option<Arc<crate::state::StateManager>>,
    data_dir: &std::path::Path,
) -> anyhow::Result<serde_json::Value> {
    // Anonymous node ID — truncated SHA-256 hash of pubkey
    let (node_id, node_name, version, container_count, running_count, peer_count, containers) =
        if let Some(ref sm) = state {
            let (data, _) = sm.get_snapshot().await;
            let id = {
                use sha2::{Digest, Sha256};
                let mut h = Sha256::new();
                h.update(data.server_info.pubkey.as_bytes());
                hex::encode(h.finalize())[..16].to_string()
            };
            let containers: Vec<serde_json::Value> = data
                .package_data
                .iter()
                .map(|(id, pkg)| {
                    serde_json::json!({
                        "id": id,
                        "state": format!("{:?}", pkg.state),
                        "version": pkg.manifest.version,
                    })
                })
                .collect();
            let running = data
                .package_data
                .values()
                .filter(|p| matches!(p.state, crate::data_model::PackageState::Running))
                .count();
            (
                id,
                data.server_info
                    .name
                    .clone()
                    .filter(|n| !n.trim().is_empty()),
                data.server_info.version.clone(),
                data.package_data.len(),
                running,
                data.peer_health.len(),
                containers,
            )
        } else {
            (
                "unknown".to_string(),
                None,
                "unknown".to_string(),
                0,
                0,
                0,
                Vec::new(),
            )
        };

    // System info
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(0);
    let uptime_secs = tokio::fs::read_to_string("/proc/uptime")
        .await
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
        .map(|f| f as u64)
        .unwrap_or(0);
    let hostname = system_hostname().await;
    let server_url = local_server_url(data_dir).await;

    // Latest metrics snapshot
    let latest = store.latest().await;
    let (cpu_pct, mem_pct, disk_pct): (f64, f64, f64) = latest
        .map(|s| {
            let mem_total = s.system.mem_total_bytes as f64;
            let disk_total = s.system.disk_total_bytes as f64;
            (
                s.system.cpu_percent,
                if mem_total > 0.0 {
                    (s.system.mem_used_bytes as f64 / mem_total) * 100.0
                } else {
                    0.0
                },
                if disk_total > 0.0 {
                    (s.system.disk_used_bytes as f64 / disk_total) * 100.0
                } else {
                    0.0
                },
            )
        })
        .unwrap_or((0.0, 0.0, 0.0));

    // Recent alerts
    let recent_alerts: Vec<serde_json::Value> = store
        .get_fired_alerts(10)
        .await
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "rule": format!("{:?}", a.kind),
                "message": a.message,
                "timestamp": a.timestamp,
            })
        })
        .collect();

    let _ = data_dir; // used for future per-app telemetry

    Ok(serde_json::json!({
        "node_id": node_id,
        "node_name": node_name,
        "hostname": hostname,
        "server_url": server_url,
        "version": version,
        "uptime_secs": uptime_secs,
        "cpu_cores": cpu_cores,
        "cpu_pct": (cpu_pct * 10.0).round() / 10.0,
        "mem_pct": (mem_pct * 10.0).round() / 10.0,
        "disk_pct": (disk_pct * 10.0).round() / 10.0,
        "containers": containers,
        "container_count": container_count,
        "running_count": running_count,
        "federation_peers": peer_count,
        "recent_alerts": recent_alerts,
        "reported_at": chrono::Utc::now().to_rfc3339(),
    }))
}

async fn system_hostname() -> Option<String> {
    let output = tokio::process::Command::new("hostname")
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let hostname = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!hostname.is_empty()).then_some(hostname)
}

async fn local_server_url(data_dir: &std::path::Path) -> Option<String> {
    let _ = data_dir;
    let output = tokio::process::Command::new("hostname")
        .arg("-I")
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let ip = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .find(|ip| !ip.starts_with("127.") && ip.contains('.'))?
        .to_string();
    Some(format!("https://{ip}"))
}

/// POST a telemetry report to the central collector.
async fn post_telemetry_report(url: &str, report: &serde_json::Value) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let payload = serde_json::json!({
        "method": "telemetry.ingest",
        "params": report,
    });
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "Archipelago-Telemetry/1.0")
        .json(&payload)
        .send()
        .await?;
    if !response.status().is_success() {
        anyhow::bail!("Collector returned {}", response.status());
    }
    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap_or_default();
    if let Some(error) = body.get("error") {
        anyhow::bail!("Collector RPC error: {}", error);
    }
    if body.get("result").is_none() {
        anyhow::bail!("Collector returned {} without RPC result", status);
    }
    Ok(())
}

/// Save a telemetry report into the local fleet directory.
/// This makes the node's own report visible in the fleet dashboard.
async fn save_report_to_fleet_dir(data_dir: &std::path::Path, report: &serde_json::Value) {
    let node_id = match report.get("node_id").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id,
        _ => {
            warn!("Telemetry report missing node_id — skipping fleet save");
            return;
        }
    };

    let fleet_dir = data_dir.join("telemetry-fleet");
    if let Err(e) = tokio::fs::create_dir_all(&fleet_dir).await {
        warn!("Failed to create telemetry-fleet directory: {}", e);
        return;
    }

    // Write latest report (overwrites previous)
    let latest_path = fleet_dir.join(format!("{}.json", node_id));
    match serde_json::to_string_pretty(report) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&latest_path, &json).await {
                warn!("Failed to write fleet report for {}: {}", node_id, e);
            }
        }
        Err(e) => {
            warn!("Failed to serialize fleet report: {}", e);
            return;
        }
    }

    // Append to history file (cap at 200 entries)
    let history_path = fleet_dir.join(format!("{}-history.json", node_id));
    let mut history: Vec<serde_json::Value> = match tokio::fs::read_to_string(&history_path).await {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    history.push(report.clone());
    if history.len() > 200 {
        let start = history.len() - 200;
        history = history.split_off(start);
    }
    match serde_json::to_string_pretty(&history) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&history_path, &json).await {
                warn!("Failed to write fleet history for {}: {}", node_id, e);
            }
        }
        Err(e) => {
            warn!("Failed to serialize fleet history: {}", e);
        }
    }

    debug!(
        "Saved own telemetry report to fleet directory (node_id={})",
        node_id
    );
}
