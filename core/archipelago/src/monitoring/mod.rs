pub(crate) mod alerts;
pub mod collector;
mod notifications;
pub mod store;
mod telemetry;
pub mod types;

// Re-export public types for external consumers
pub use store::MetricsStore;
pub use telemetry::spawn_telemetry_reporter;
pub use types::*;

use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, warn};

/// Spawn the background metrics collector (runs every 300 seconds / 5 minutes).
/// Evaluates alert rules on each snapshot and dispatches notifications.
/// Note: health_monitor.rs handles container state polling at 120s intervals.
/// This collector handles system-level metrics (CPU, disk, network) and only
/// calls podman stats every 5 minutes to avoid duplicate subprocess overhead.
pub fn spawn_metrics_collector(
    store: Arc<MetricsStore>,
    state: Option<Arc<crate::state::StateManager>>,
    data_dir: Option<PathBuf>,
) {
    tokio::spawn(async move {
        // Wait 60s for system to stabilize after boot
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            match collector::collect_snapshot().await {
                Ok(snapshot) => {
                    let alerts = store.check_alerts(&snapshot).await;
                    store.push(snapshot).await;
                    debug!("Metrics snapshot collected");

                    if !alerts.is_empty() {
                        if let Some(ref state_mgr) = state {
                            notifications::push_alert_notifications(state_mgr, &alerts).await;
                        }
                        if let Some(ref dir) = data_dir {
                            notifications::deliver_alert_webhooks(dir, &alerts).await;
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to collect metrics: {}", e);
                }
            }
        }
    });
}
