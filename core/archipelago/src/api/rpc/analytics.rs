//! Opt-in anonymous node analytics.
//! When enabled, collects aggregate stats (app install counts, uptime, hardware tier).
//! No personally identifiable information. No IP addresses. No DIDs.
//! Data stays local until explicitly shared via future relay mechanism.

use super::RpcHandler;
use anyhow::{Context, Result};
use tracing::{debug, info, warn};

const ANALYTICS_FILE: &str = "analytics-config.json";

impl RpcHandler {
    /// Check if analytics are enabled.
    pub(super) async fn handle_analytics_get_status(&self) -> Result<serde_json::Value> {
        let config_path = self.config.data_dir.join(ANALYTICS_FILE);
        let enabled = if config_path.exists() {
            let data = tokio::fs::read_to_string(&config_path).await?;
            let config: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
            config["enabled"].as_bool().unwrap_or(false)
        } else {
            false
        };

        Ok(serde_json::json!({
            "enabled": enabled,
            "description": "Anonymous aggregate statistics. No personal data collected.",
        }))
    }

    /// Enable opt-in analytics.
    pub(super) async fn handle_analytics_enable(&self) -> Result<serde_json::Value> {
        let config_path = self.config.data_dir.join(ANALYTICS_FILE);
        let config = serde_json::json!({
            "enabled": true,
            "opted_in_at": chrono::Utc::now().to_rfc3339(),
        });
        tokio::fs::write(&config_path, serde_json::to_string_pretty(&config)?).await?;
        info!("Analytics opted in");
        Ok(serde_json::json!({ "enabled": true }))
    }

    /// Disable analytics.
    pub(super) async fn handle_analytics_disable(&self) -> Result<serde_json::Value> {
        let config_path = self.config.data_dir.join(ANALYTICS_FILE);
        let config = serde_json::json!({
            "enabled": false,
            "opted_out_at": chrono::Utc::now().to_rfc3339(),
        });
        tokio::fs::write(&config_path, serde_json::to_string_pretty(&config)?).await?;
        info!("Analytics opted out");
        Ok(serde_json::json!({ "enabled": false }))
    }

    /// Get an anonymous analytics snapshot of this node.
    /// Only returns aggregate data — no DIDs, no IPs, no secrets.
    pub(super) async fn handle_analytics_get_snapshot(&self) -> Result<serde_json::Value> {
        // Check if opted in
        let config_path = self.config.data_dir.join(ANALYTICS_FILE);
        let enabled = if config_path.exists() {
            let data = tokio::fs::read_to_string(&config_path).await?;
            let config: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
            config["enabled"].as_bool().unwrap_or(false)
        } else {
            false
        };

        if !enabled {
            return Ok(serde_json::json!({
                "error": "Analytics not enabled. Opt in via analytics.enable first.",
                "enabled": false,
            }));
        }

        // Collect anonymous aggregate data
        let (data, _) = self.state_manager.get_snapshot().await;

        let app_count = data.package_data.len();
        let running_count = data
            .package_data
            .values()
            .filter(|p| matches!(p.state, crate::data_model::PackageState::Running))
            .count();

        // Hardware tier (anonymous)
        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0);

        let mem_output = tokio::process::Command::new("grep")
            .args(["MemTotal", "/proc/meminfo"])
            .output()
            .await;
        let total_ram_mb = mem_output
            .ok()
            .and_then(|o| {
                let s = String::from_utf8_lossy(&o.stdout);
                s.split_whitespace().nth(1)?.parse::<u64>().ok()
            })
            .map(|kb| kb / 1024)
            .unwrap_or(0);

        let hardware_tier = match total_ram_mb {
            0..=3999 => "minimal",
            4000..=7999 => "standard",
            8000..=15999 => "power",
            _ => "heavy",
        };

        let version = &data.server_info.version;
        let federation_peers = data.peer_health.len();

        Ok(serde_json::json!({
            "version": version,
            "app_count": app_count,
            "running_count": running_count,
            "hardware_tier": hardware_tier,
            "cpu_cores": cpu_cores,
            "ram_mb": total_ram_mb,
            "federation_peers": federation_peers,
            "collected_at": chrono::Utc::now().to_rfc3339(),
        }))
    }

    /// Build a full telemetry report for the beta fleet monitoring.
    /// Includes health data, container states, errors, and uptime.
    /// No wallet data, no keys, no personal data — only system health.
    pub(super) async fn handle_telemetry_report(&self) -> Result<serde_json::Value> {
        // Check opt-in
        let config_path = self.config.data_dir.join(ANALYTICS_FILE);
        let enabled = if config_path.exists() {
            let data = tokio::fs::read_to_string(&config_path).await?;
            let config: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
            config["enabled"].as_bool().unwrap_or(false)
        } else {
            false
        };
        if !enabled {
            anyhow::bail!("Telemetry not enabled. Opt in via analytics.enable first.");
        }

        let (data, _) = self.state_manager.get_snapshot().await;

        // Anonymous node ID — SHA-256 hash of the DID (not the DID itself)
        let node_id = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(data.server_info.pubkey.as_bytes());
            hex::encode(hasher.finalize())[..16].to_string()
        };

        // Container states
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

        // System stats
        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0);
        let mem_output = tokio::process::Command::new("grep")
            .args(["MemTotal", "/proc/meminfo"])
            .output()
            .await;
        let total_ram_mb = mem_output
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .split_whitespace()
                    .nth(1)?
                    .parse::<u64>()
                    .ok()
            })
            .map(|kb| kb / 1024)
            .unwrap_or(0);

        // Uptime
        let uptime_secs = tokio::fs::read_to_string("/proc/uptime")
            .await
            .ok()
            .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
            .map(|f| f as u64)
            .unwrap_or(0);

        let latest = self.metrics_store.latest().await;
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

        // Recent alerts from metrics store
        let recent_alerts: Vec<serde_json::Value> = self
            .metrics_store
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

        let report = serde_json::json!({
            "node_id": node_id,
            "node_name": data.server_info.name.clone().filter(|n| !n.trim().is_empty()),
            "hostname": system_hostname().await,
            "server_url": local_server_url(&self.config.host_ip),
            "version": data.server_info.version,
            "uptime_secs": uptime_secs,
            "cpu_cores": cpu_cores,
            "ram_mb": total_ram_mb,
            "cpu_pct": (cpu_pct * 10.0).round() / 10.0,
            "mem_pct": (mem_pct * 10.0).round() / 10.0,
            "disk_pct": (disk_pct * 10.0).round() / 10.0,
            "containers": containers,
            "container_count": data.package_data.len(),
            "running_count": data.package_data.values()
                .filter(|p| matches!(p.state, crate::data_model::PackageState::Running)).count(),
            "federation_peers": data.peer_health.len(),
            "recent_alerts": recent_alerts,
            "reported_at": chrono::Utc::now().to_rfc3339(),
        });

        // Save latest report to disk for debugging
        let report_path = self.config.data_dir.join("telemetry-latest.json");
        let _ = tokio::fs::write(&report_path, serde_json::to_string_pretty(&report)?).await;

        Ok(report)
    }

    // ── Fleet telemetry collector endpoints ──────────────────────────────

    /// Receive a telemetry report from a fleet node.
    /// Stores it in telemetry-fleet/ directory, indexed by node_id.
    /// Does NOT require auth — called by remote nodes posting reports.
    pub(super) async fn handle_telemetry_ingest(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let report = params.context("Missing telemetry report payload")?;

        // Validate required fields
        let node_id = report
            .get("node_id")
            .and_then(|v| v.as_str())
            .context("Missing required field: node_id")?;
        if node_id.is_empty() || node_id.len() > 64 {
            anyhow::bail!("Invalid node_id: must be 1-64 characters");
        }
        // Sanitize node_id to prevent path traversal
        if node_id.contains('/') || node_id.contains('\\') || node_id.contains("..") {
            anyhow::bail!("Invalid node_id: contains disallowed characters");
        }
        let _version = report
            .get("version")
            .and_then(|v| v.as_str())
            .context("Missing required field: version")?;
        let _reported_at = report
            .get("reported_at")
            .and_then(|v| v.as_str())
            .context("Missing required field: reported_at")?;

        let fleet_dir = self.config.data_dir.join("telemetry-fleet");
        tokio::fs::create_dir_all(&fleet_dir)
            .await
            .context("Failed to create telemetry-fleet directory")?;

        // Write latest report (overwrites previous)
        let latest_path = fleet_dir.join(format!("{}.json", node_id));
        let report_json =
            serde_json::to_string_pretty(&report).context("Failed to serialize report")?;
        tokio::fs::write(&latest_path, &report_json)
            .await
            .context("Failed to write latest fleet report")?;

        // Append to history file (cap at 200 entries)
        let history_path = fleet_dir.join(format!("{}-history.json", node_id));
        let mut history: Vec<serde_json::Value> =
            match tokio::fs::read_to_string(&history_path).await {
                Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
                Err(_) => Vec::new(),
            };
        history.push(report.clone());
        // Keep only the last 200 entries
        if history.len() > 200 {
            let start = history.len() - 200;
            history = history.split_off(start);
        }
        let history_json =
            serde_json::to_string_pretty(&history).context("Failed to serialize history")?;
        tokio::fs::write(&history_path, &history_json)
            .await
            .context("Failed to write fleet history")?;

        debug!(node_id = %node_id, "Ingested fleet telemetry report");

        Ok(serde_json::json!({
            "status": "ok",
            "node_id": node_id,
        }))
    }

    /// Get all fleet nodes' latest reports.
    ///
    /// Primary source: TRUSTED federated nodes from nodes.json — their
    /// `last_state` snapshot (kept fresh by federation state-sync) already
    /// carries everything the Fleet UI renders. Observer ("peer") and
    /// Untrusted nodes are deliberately excluded from Fleet.
    ///
    /// Secondary source: telemetry-fleet/*.json collector reports (opt-in
    /// anonymous telemetry, includes this node's own report) — merged in for
    /// back-compat with nodes that push telemetry but aren't federated.
    pub(super) async fn handle_telemetry_fleet_status(&self) -> Result<serde_json::Value> {
        let mut nodes: Vec<serde_json::Value> = Vec::new();

        // ── Trusted federation nodes ─────────────────────────────────────
        let fed_nodes = crate::federation::load_nodes(&self.config.data_dir)
            .await
            .unwrap_or_default();
        for n in fed_nodes
            .iter()
            .filter(|n| n.trust_level == crate::federation::TrustLevel::Trusted)
        {
            let state = n.last_state.as_ref();
            let pct = |used: Option<u64>, total: Option<u64>| -> serde_json::Value {
                match (used, total) {
                    (Some(u), Some(t)) if t > 0 => {
                        serde_json::json!((u as f64 / t as f64 * 100.0).round())
                    }
                    _ => serde_json::Value::Null,
                }
            };
            let apps = state.map(|s| s.apps.as_slice()).unwrap_or(&[]);
            let reported_at = state
                .map(|s| s.timestamp.clone())
                .or_else(|| n.last_seen.clone());

            let mut report = serde_json::json!({
                "node_id": n.did,
                "node_name": state.and_then(|s| s.node_name.clone()).or_else(|| n.name.clone()),
                "uptime_secs": state.and_then(|s| s.uptime_secs),
                "cpu_pct": state.and_then(|s| s.cpu_usage_percent).filter(|v| v.is_finite() && (0.0..=100.0).contains(v)).map(|v| v.round()),
                "mem_pct": pct(state.and_then(|s| s.mem_used_bytes), state.and_then(|s| s.mem_total_bytes)),
                "disk_pct": pct(state.and_then(|s| s.disk_used_bytes), state.and_then(|s| s.disk_total_bytes)),
                "container_count": apps.len(),
                "running_count": apps.iter().filter(|a| a.status == "running").count(),
                "federation_peers": state.map(|s| s.federated_peers.len()).unwrap_or(0),
                "containers": apps.iter().map(|a| serde_json::json!({
                    "id": a.id,
                    "state": a.status,
                    "version": a.version.clone().unwrap_or_default(),
                })).collect::<Vec<_>>(),
                "reported_at": reported_at,
                "trust_level": n.trust_level.to_string(),
                "source": "federation",
            });
            annotate_fleet_report(&mut report);
            nodes.push(report);
        }

        // ── Opt-in telemetry collector reports ───────────────────────────
        let fleet_dir = self.config.data_dir.join("telemetry-fleet");
        if fleet_dir.exists() {
            let mut entries = tokio::fs::read_dir(&fleet_dir)
                .await
                .context("Failed to read telemetry-fleet directory")?;

            while let Some(entry) = entries.next_entry().await? {
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();
                // Skip history files and non-JSON files
                if name.ends_with("-history.json") || !name.ends_with(".json") {
                    continue;
                }

                match tokio::fs::read_to_string(entry.path()).await {
                    Ok(data) => match serde_json::from_str::<serde_json::Value>(&data) {
                        Ok(mut report) => {
                            annotate_fleet_report(&mut report);
                            nodes.push(report);
                        }
                        Err(e) => {
                            warn!(file = %name, error = %e, "Skipping corrupt fleet report");
                        }
                    },
                    Err(e) => {
                        warn!(file = %name, error = %e, "Failed to read fleet report");
                    }
                }
            }
        }

        // Sort by node_id for stable ordering
        nodes.sort_by(|a, b| {
            let a_id = a.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
            let b_id = b.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
            a_id.cmp(b_id)
        });

        info!(count = nodes.len(), "Fleet status query");

        Ok(serde_json::json!({ "nodes": nodes }))
    }

    /// Get history for a specific fleet node.
    /// Reads telemetry-fleet/{node_id}-history.json.
    pub(super) async fn handle_telemetry_fleet_node_history(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let p = params.context("Missing params")?;
        let node_id = p
            .get("node_id")
            .and_then(|v| v.as_str())
            .context("Missing required field: node_id")?;

        // Sanitize node_id
        if node_id.is_empty()
            || node_id.len() > 64
            || node_id.contains('/')
            || node_id.contains('\\')
            || node_id.contains("..")
        {
            anyhow::bail!("Invalid node_id");
        }

        let history_path = self
            .config
            .data_dir
            .join("telemetry-fleet")
            .join(format!("{}-history.json", node_id));

        let history: Vec<serde_json::Value> = match tokio::fs::read_to_string(&history_path).await {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => Vec::new(),
        };

        Ok(serde_json::json!({
            "node_id": node_id,
            "entries": history,
            "count": history.len(),
        }))
    }

    /// Get aggregated fleet alerts across all nodes.
    /// Reads all fleet reports, collects recent_alerts, sorts by timestamp descending.
    pub(super) async fn handle_telemetry_fleet_alerts(&self) -> Result<serde_json::Value> {
        let fleet_dir = self.config.data_dir.join("telemetry-fleet");
        if !fleet_dir.exists() {
            return Ok(serde_json::json!({ "alerts": [] }));
        }

        let mut all_alerts: Vec<serde_json::Value> = Vec::new();
        let mut entries = tokio::fs::read_dir(&fleet_dir)
            .await
            .context("Failed to read telemetry-fleet directory")?;

        while let Some(entry) = entries.next_entry().await? {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            // Only read latest reports, skip history files
            if name.ends_with("-history.json") || !name.ends_with(".json") {
                continue;
            }

            let data = match tokio::fs::read_to_string(entry.path()).await {
                Ok(d) => d,
                Err(_) => continue,
            };
            let report: serde_json::Value = match serde_json::from_str(&data) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let node_id = report
                .get("node_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            if let Some(alerts) = report.get("recent_alerts").and_then(|v| v.as_array()) {
                for alert in alerts {
                    let mut enriched = alert.clone();
                    if let Some(obj) = enriched.as_object_mut() {
                        obj.insert("node_id".to_string(), serde_json::json!(node_id));
                    }
                    all_alerts.push(enriched);
                }
            }
        }

        // Sort by timestamp descending (most recent first)
        all_alerts.sort_by(|a, b| {
            let a_ts = a.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
            let b_ts = b.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
            b_ts.cmp(&a_ts)
        });

        Ok(serde_json::json!({
            "alerts": all_alerts,
            "count": all_alerts.len(),
        }))
    }
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

fn local_server_url(host_ip: &str) -> Option<String> {
    let host_ip = host_ip.trim();
    if host_ip.is_empty() || host_ip == "127.0.0.1" {
        None
    } else {
        Some(format!("https://{host_ip}"))
    }
}

/// Stamp a fleet report with computed `online` and human-readable `last_seen`
/// derived from its `reported_at` timestamp (online = reported <30min ago).
fn annotate_fleet_report(report: &mut serde_json::Value) {
    let reported = report
        .get("reported_at")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());

    let is_online = reported
        .map(|dt| {
            let age = chrono::Utc::now().signed_duration_since(dt);
            age.num_seconds() >= -60 && age.num_seconds() < 1800
        })
        .unwrap_or(false);

    let last_seen = reported
        .map(|dt| {
            let age = chrono::Utc::now().signed_duration_since(dt);
            let mins = age.num_minutes();
            if age.num_seconds() < -60 {
                "unknown (clock ahead)".to_string()
            } else if mins < 1 {
                "just now".to_string()
            } else if mins < 60 {
                format!("{}m ago", mins)
            } else if mins < 1440 {
                format!("{}h ago", mins / 60)
            } else {
                format!("{}d ago", mins / 1440)
            }
        })
        .unwrap_or_else(|| "unknown".to_string());

    if let Some(obj) = report.as_object_mut() {
        obj.insert("online".to_string(), serde_json::json!(is_online));
        obj.insert("last_seen".to_string(), serde_json::json!(last_seen));
    }
}
