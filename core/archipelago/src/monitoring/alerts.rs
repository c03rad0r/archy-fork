use crate::monitoring::store::MetricsStore;
use crate::monitoring::types::*;
use tracing::{info, warn};

const ALERT_RULES_FILE: &str = "alert-rules.json";

impl AlertRule {
    pub(crate) fn default_rules() -> Vec<AlertRule> {
        vec![
            AlertRule {
                kind: AlertRuleKind::DiskUsage,
                threshold: 80.0,
                enabled: true,
                description: "Disk usage exceeds threshold".to_string(),
            },
            AlertRule {
                kind: AlertRuleKind::RamUsage,
                threshold: 80.0,
                enabled: true,
                description: "Total memory usage exceeds threshold".to_string(),
            },
            AlertRule {
                kind: AlertRuleKind::CpuLoad,
                threshold: 4.0,
                enabled: true,
                description: "CPU load exceeds 4x core count for 5 minutes".to_string(),
            },
            AlertRule {
                kind: AlertRuleKind::ContainerCrash,
                threshold: 1.0,
                enabled: true,
                description: "Container stopped unexpectedly".to_string(),
            },
            AlertRule {
                kind: AlertRuleKind::BackendErrorSpike,
                threshold: 500.0,
                enabled: true,
                description: "RPC latency exceeds threshold (ms)".to_string(),
            },
            AlertRule {
                kind: AlertRuleKind::SslCertExpiry,
                threshold: 30.0,
                enabled: true,
                description: "SSL certificate expires within N days".to_string(),
            },
        ]
    }
}

/// Alert-related methods on MetricsStore.
impl MetricsStore {
    /// Get the current alert rules.
    pub async fn get_alert_rules(&self) -> Vec<AlertRule> {
        self.alert_rules.read().await.clone()
    }

    /// Update an alert rule by kind and persist to disk.
    pub async fn update_alert_rule(
        &self,
        kind: &AlertRuleKind,
        enabled: Option<bool>,
        threshold: Option<f64>,
    ) {
        let mut rules = self.alert_rules.write().await;
        if let Some(rule) = rules.iter_mut().find(|r| &r.kind == kind) {
            if let Some(e) = enabled {
                rule.enabled = e;
            }
            if let Some(t) = threshold {
                rule.threshold = t;
            }
        }
        // Persist to disk so changes survive restarts
        if let Some(ref dir) = self.data_dir {
            if let Err(e) = save_alert_rules(dir, &rules).await {
                warn!("Failed to persist alert rules: {}", e);
            }
        }
    }

    /// Get fired alert history.
    pub async fn get_fired_alerts(&self, last_n: usize) -> Vec<FiredAlert> {
        let buf = self.fired_alerts.read().await;
        let start = buf.len().saturating_sub(last_n);
        buf.iter().skip(start).cloned().collect()
    }

    /// Acknowledge a fired alert by id.
    pub async fn acknowledge_alert(&self, alert_id: &str) -> bool {
        let mut buf = self.fired_alerts.write().await;
        if let Some(alert) = buf.iter_mut().find(|a| a.id == alert_id) {
            alert.acknowledged = true;
            true
        } else {
            false
        }
    }

    /// Evaluate alert rules against a snapshot and return any new alerts.
    pub async fn check_alerts(&self, snapshot: &MetricSnapshot) -> Vec<FiredAlert> {
        let rules = self.alert_rules.read().await;
        let mut new_alerts = Vec::new();
        let ts = snapshot.timestamp;

        for rule in rules.iter() {
            if !rule.enabled {
                continue;
            }

            match rule.kind {
                AlertRuleKind::DiskUsage => {
                    if snapshot.system.disk_total_bytes > 0 {
                        let pct = (snapshot.system.disk_used_bytes as f64
                            / snapshot.system.disk_total_bytes as f64)
                            * 100.0;
                        if pct > rule.threshold {
                            new_alerts.push(FiredAlert {
                                id: format!("disk-{}", ts),
                                kind: AlertRuleKind::DiskUsage,
                                message: format!(
                                    "Disk usage at {:.1}% (threshold: {:.0}%)",
                                    pct, rule.threshold
                                ),
                                value: pct,
                                threshold: rule.threshold,
                                timestamp: ts,
                                acknowledged: false,
                            });
                        }
                    }
                }
                AlertRuleKind::RamUsage => {
                    if snapshot.system.mem_total_bytes > 0 {
                        let pct = (snapshot.system.mem_used_bytes as f64
                            / snapshot.system.mem_total_bytes as f64)
                            * 100.0;
                        if pct > rule.threshold {
                            new_alerts.push(FiredAlert {
                                id: format!("ram-{}", ts),
                                kind: AlertRuleKind::RamUsage,
                                message: format!(
                                    "RAM usage at {:.1}% (threshold: {:.0}%)",
                                    pct, rule.threshold
                                ),
                                value: pct,
                                threshold: rule.threshold,
                                timestamp: ts,
                                acknowledged: false,
                            });
                        }
                    }
                }
                AlertRuleKind::CpuLoad => {
                    // Alert if 5-min load average exceeds threshold * core count
                    let cores = std::thread::available_parallelism()
                        .map(|n| n.get() as f64)
                        .unwrap_or(4.0);
                    let max_load = rule.threshold * cores;
                    if snapshot.system.load_avg_5 > max_load {
                        new_alerts.push(FiredAlert {
                            id: format!("cpu-{}", ts),
                            kind: AlertRuleKind::CpuLoad,
                            message: format!(
                                "CPU load at {:.1} (threshold: {:.0} = {:.0}x {} cores)",
                                snapshot.system.load_avg_5, max_load, rule.threshold, cores as u32
                            ),
                            value: snapshot.system.load_avg_5,
                            threshold: max_load,
                            timestamp: ts,
                            acknowledged: false,
                        });
                    }
                }
                AlertRuleKind::BackendErrorSpike => {
                    if snapshot.rpc_latency_ms > rule.threshold {
                        new_alerts.push(FiredAlert {
                            id: format!("latency-{}", ts),
                            kind: AlertRuleKind::BackendErrorSpike,
                            message: format!(
                                "RPC latency at {:.0}ms (threshold: {:.0}ms)",
                                snapshot.rpc_latency_ms, rule.threshold
                            ),
                            value: snapshot.rpc_latency_ms,
                            threshold: rule.threshold,
                            timestamp: ts,
                            acknowledged: false,
                        });
                    }
                }
                // ContainerCrash and SslCertExpiry are checked via dedicated paths
                _ => {}
            }
        }

        // Store fired alerts
        if !new_alerts.is_empty() {
            let mut buf = self.fired_alerts.write().await;
            for alert in &new_alerts {
                if buf.len() >= MAX_ALERT_HISTORY {
                    buf.pop_front();
                }
                buf.push_back(alert.clone());
            }
        }

        new_alerts
    }
}

/// Load alert rules from disk, falling back to defaults if file missing or corrupt.
pub(crate) async fn load_alert_rules(data_dir: &std::path::Path) -> Vec<AlertRule> {
    let path = data_dir.join(ALERT_RULES_FILE);
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => match serde_json::from_str::<Vec<AlertRule>>(&content) {
            Ok(saved) => {
                // Merge with defaults: use saved enabled/threshold, add any new rule kinds
                let defaults = AlertRule::default_rules();
                let mut merged = Vec::new();
                for default in &defaults {
                    if let Some(saved_rule) = saved.iter().find(|r| r.kind == default.kind) {
                        merged.push(AlertRule {
                            kind: default.kind.clone(),
                            threshold: saved_rule.threshold,
                            enabled: saved_rule.enabled,
                            description: default.description.clone(),
                        });
                    } else {
                        merged.push(default.clone());
                    }
                }
                info!("Loaded alert rules from {}", path.display());
                merged
            }
            Err(e) => {
                warn!("Failed to parse alert rules ({}), using defaults", e);
                AlertRule::default_rules()
            }
        },
        Err(_) => AlertRule::default_rules(),
    }
}

/// Save alert rules to disk.
pub(crate) async fn save_alert_rules(
    data_dir: &std::path::Path,
    rules: &[AlertRule],
) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(data_dir).await?;
    let content = serde_json::to_string_pretty(rules)?;
    tokio::fs::write(data_dir.join(ALERT_RULES_FILE), content).await?;
    Ok(())
}
