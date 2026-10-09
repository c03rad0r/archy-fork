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

/// Spawn the background metrics collector at the store's one-minute resolution.
/// Evaluates alert rules on each snapshot and dispatches notifications.
/// Note: health_monitor.rs handles container state polling at 120s intervals.
/// Runtime commands have deadlines; unavailable container stats cannot hold
/// system readings indefinitely. Missed ticks are skipped, never replayed.
pub fn spawn_metrics_collector(
    store: Arc<MetricsStore>,
    state: Option<Arc<crate::state::StateManager>>,
    data_dir: Option<PathBuf>,
) {
    tokio::spawn(async move {
        // Start promptly without competing with the very first boot tasks.
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            match collector::collect_snapshot().await {
                Ok(snapshot) => {
                    let alerts = store.check_alerts(&snapshot).await;
                    store.push(snapshot).await;
                    debug!("Metrics snapshot collected");

                    // Reconcile even when all thresholds have recovered, so
                    // the previous disk/RAM/CPU warning disappears promptly.
                    if let Some(ref state_mgr) = state {
                        notifications::push_alert_notifications(state_mgr, &alerts).await;
                    }
                    if !alerts.is_empty() {
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
