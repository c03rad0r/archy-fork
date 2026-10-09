use crate::monitoring::types::{AlertRuleKind, FiredAlert};
use crate::webhooks::{self, WebhookEvent, WebhookPayload};
use chrono::Utc;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

const NOTIFICATION_MAX_AGE_SECS: i64 = 30 * 60;

/// Push fired alerts as notifications to the state manager (broadcast via WebSocket).
pub(crate) async fn push_alert_notifications(
    state_mgr: &Arc<crate::state::StateManager>,
    alerts: &[FiredAlert],
) {
    let (mut data, _rev) = state_mgr.get_snapshot().await;
    let previous_count = data.notifications.len();
    prune_stale_alert_notifications(&mut data.notifications, alerts);
    if alerts.is_empty() && data.notifications.len() == previous_count {
        return;
    }
    for alert in alerts {
        let level = match alert.kind {
            AlertRuleKind::DiskUsage | AlertRuleKind::RamUsage => {
                if alert.value > 95.0 {
                    crate::data_model::NotificationLevel::Error
                } else {
                    crate::data_model::NotificationLevel::Warning
                }
            }
            AlertRuleKind::ContainerCrash => crate::data_model::NotificationLevel::Error,
            _ => crate::data_model::NotificationLevel::Warning,
        };
        let notification = crate::data_model::Notification {
            id: alert.id.clone(),
            level,
            title: format!("{:?} Alert", alert.kind),
            message: alert.message.clone(),
            timestamp: Utc::now().to_rfc3339(),
            app_id: None,
        };
        data.notifications.push(notification);
    }
    // Keep max 20 notifications
    while data.notifications.len() > 20 {
        data.notifications.remove(0);
    }
    state_mgr.update_data(data).await;
    if !alerts.is_empty() {
        info!("Fired {} alert(s)", alerts.len());
    }
}

fn is_metrics_alert_id(id: &str) -> bool {
    id.split_once('-').is_some_and(|(kind, timestamp)| {
        matches!(kind, "disk" | "ram" | "cpu" | "latency")
            && !timestamp.is_empty()
            && timestamp.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn prune_stale_alert_notifications(
    notifications: &mut Vec<crate::data_model::Notification>,
    alerts: &[FiredAlert],
) {
    let now = Utc::now();
    let active_ids: HashSet<&str> = alerts.iter().map(|alert| alert.id.as_str()).collect();
    notifications.retain(|notification| {
        if active_ids.contains(notification.id.as_str()) {
            return false;
        }
        // A fresh timestamp is not evidence that an earlier threshold is
        // still exceeded. Remove resolved monitoring alerts immediately.
        if is_metrics_alert_id(&notification.id) {
            return false;
        }
        if notification.app_id.is_some() || notification.id.starts_with("health-") {
            return true;
        }
        match chrono::DateTime::parse_from_rfc3339(&notification.timestamp) {
            Ok(ts) => {
                now.signed_duration_since(ts.with_timezone(&Utc))
                    .num_seconds()
                    <= NOTIFICATION_MAX_AGE_SECS
            }
            Err(_) => false,
        }
    });
}

/// Deliver webhook notifications for alerts that map to webhook events.
pub(crate) async fn deliver_alert_webhooks(data_dir: &Path, alerts: &[FiredAlert]) {
    for alert in alerts {
        let event = match alert.kind {
            AlertRuleKind::DiskUsage => Some(WebhookEvent::DiskWarning),
            AlertRuleKind::ContainerCrash => Some(WebhookEvent::ContainerCrash),
            _ => None,
        };
        if let Some(event) = event {
            let payload = WebhookPayload {
                event,
                title: format!("{:?} Alert", alert.kind),
                message: alert.message.clone(),
                timestamp: Utc::now().to_rfc3339(),
                node_id: String::new(),
                details: Some(serde_json::json!({
                    "value": alert.value,
                    "threshold": alert.threshold,
                })),
            };
            webhooks::send_webhook(data_dir, payload).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_model::{Notification, NotificationLevel};

    fn notification(id: &str, timestamp: String, app_id: Option<&str>) -> Notification {
        Notification {
            id: id.to_string(),
            level: NotificationLevel::Warning,
            title: "DiskUsage Alert".to_string(),
            message: "Disk warning".to_string(),
            timestamp,
            app_id: app_id.map(str::to_string),
        }
    }

    #[test]
    fn prune_stale_alert_notifications_removes_duplicate_and_old_generic_alerts() {
        let active_alert = FiredAlert {
            id: "alert-active".to_string(),
            kind: AlertRuleKind::DiskUsage,
            message: "Disk warning".to_string(),
            value: 90.0,
            threshold: 85.0,
            timestamp: Utc::now().timestamp(),
            acknowledged: false,
        };
        let old_timestamp = (Utc::now() - chrono::Duration::minutes(45)).to_rfc3339();
        let fresh_timestamp = (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
        let mut notifications = vec![
            notification("alert-active", fresh_timestamp.clone(), None),
            notification("alert-old", old_timestamp, None),
            notification("alert-fresh", fresh_timestamp.clone(), None),
            notification("disk-123", fresh_timestamp.clone(), None),
            notification("ram-123", fresh_timestamp.clone(), None),
            notification("health-indeedhub-1", fresh_timestamp, Some("indeedhub")),
        ];

        prune_stale_alert_notifications(&mut notifications, &[active_alert]);

        let ids: Vec<&str> = notifications.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, vec!["alert-fresh", "health-indeedhub-1"]);
    }

    #[test]
    fn clears_resolved_metrics_alerts_without_touching_other_notifications() {
        let fresh = Utc::now().to_rfc3339();
        let mut notifications = vec![
            notification("disk-123", fresh.clone(), None),
            notification("latency-456", fresh.clone(), None),
            notification("disk-not-a-timestamp", fresh.clone(), None),
            notification("health-indeedhub-1", fresh.clone(), Some("indeedhub")),
            notification("app-notice", fresh, Some("indeedhub")),
        ];
        prune_stale_alert_notifications(&mut notifications, &[]);
        let ids: Vec<&str> = notifications.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["disk-not-a-timestamp", "health-indeedhub-1", "app-notice"]
        );
    }
}
