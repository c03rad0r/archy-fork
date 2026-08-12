use crate::manifest::HealthCheck;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    Unknown,
    Starting,
}

pub struct HealthMonitor {
    container_name: String,
    health_check: Option<HealthCheck>,
}

impl HealthMonitor {
    pub fn new(container_name: String, health_check: Option<HealthCheck>) -> Self {
        Self {
            container_name,
            health_check,
        }
    }

    pub async fn check_health(&self) -> Result<HealthStatus> {
        if let Some(ref check) = self.health_check {
            match check.check_type.as_str() {
                "http" => self.check_http_health(check).await,
                "exec" => self.check_exec_health(check).await,
                _ => {
                    warn!("Unknown health check type: {}", check.check_type);
                    Ok(HealthStatus::Unknown)
                }
            }
        } else {
            // No health check defined, assume healthy if container is running
            Ok(HealthStatus::Unknown)
        }
    }

    async fn check_http_health(&self, check: &HealthCheck) -> Result<HealthStatus> {
        let endpoint = check
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("HTTP health check missing endpoint"))?;

        let url = if let Some(path) = &check.path {
            format!("{}{}", endpoint, path)
        } else {
            endpoint.clone()
        };

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .context("Failed to create HTTP client")?;

        match client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    Ok(HealthStatus::Healthy)
                } else {
                    Ok(HealthStatus::Unhealthy)
                }
            }
            Err(e) => {
                warn!("Health check failed for {}: {}", self.container_name, e);
                Ok(HealthStatus::Unhealthy)
            }
        }
    }

    async fn check_exec_health(&self, check: &HealthCheck) -> Result<HealthStatus> {
        // Execute health check command in container
        let endpoint = check
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Exec health check missing endpoint"))?;

        use tokio::process::Command;

        let output = Command::new("podman")
            .arg("exec")
            .arg(&self.container_name)
            .arg("sh")
            .arg("-c")
            .arg(endpoint)
            .output()
            .await
            .context("Failed to execute health check")?;

        if output.status.success() {
            Ok(HealthStatus::Healthy)
        } else {
            Ok(HealthStatus::Unhealthy)
        }
    }

    pub async fn monitor_health(
        &self,
        mut shutdown: tokio::sync::broadcast::Receiver<()>,
        on_status_change: impl Fn(HealthStatus) + Send + 'static,
    ) -> Result<()> {
        let check = self.health_check.clone();
        let interval_duration = if let Some(ref check) = check {
            parse_duration(&check.interval).unwrap_or(Duration::from_secs(30))
        } else {
            Duration::from_secs(30)
        };

        let mut interval = interval(interval_duration);
        let mut consecutive_failures = 0;
        let max_failures = check.as_ref().map(|c| c.retries).unwrap_or(3);

        let mut last_status = HealthStatus::Unknown;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match self.check_health().await {
                        Ok(status) => {
                            if status != last_status {
                                info!("Health status changed for {}: {:?} -> {:?}",
                                    self.container_name, last_status, status);
                                on_status_change(status.clone());
                                last_status = status.clone();
                            }

                            match status {
                                HealthStatus::Healthy => {
                                    consecutive_failures = 0;
                                }
                                HealthStatus::Unhealthy => {
                                    consecutive_failures += 1;
                                    if consecutive_failures >= max_failures {
                                        error!("Container {} is unhealthy after {} failures",
                                            self.container_name, consecutive_failures);
                                        // Auto-restart is handled by the orchestrator-level health monitor
                                        // (core/archipelago/src/health_monitor.rs) which runs every 60s,
                                        // checks all container states via `podman ps`, and restarts
                                        // exited containers with exponential backoff (10s/30s/90s).
                                        // This per-container monitor is for manifest-driven health
                                        // tracking and status change callbacks only.
                                    }
                                }
                                _ => {}
                            }
                        }
                        Err(e) => {
                            error!("Health check error for {}: {}", self.container_name, e);
                            consecutive_failures += 1;
                        }
                    }
                }
                _ = shutdown.recv() => {
                    info!("Health monitoring stopped for {}", self.container_name);
                    break;
                }
            }
        }

        Ok(())
    }
}

fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim().to_lowercase();
    if s.ends_with('s') {
        let secs: u64 = s.trim_end_matches('s').parse().ok()?;
        Some(Duration::from_secs(secs))
    } else if s.ends_with('m') {
        let mins: u64 = s.trim_end_matches('m').parse().ok()?;
        Some(Duration::from_secs(mins * 60))
    } else if s.ends_with('h') {
        let hours: u64 = s.trim_end_matches('h').parse().ok()?;
        Some(Duration::from_secs(hours * 3600))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("30s"), Some(Duration::from_secs(30)));
        assert_eq!(parse_duration("5m"), Some(Duration::from_secs(300)));
        assert_eq!(parse_duration("1h"), Some(Duration::from_secs(3600)));
    }
}
