use super::RpcHandler;
use crate::monitoring::AlertRuleKind;
use anyhow::Result;
use tracing::debug;

impl RpcHandler {
    /// monitoring.current — latest metrics snapshot
    pub(super) async fn handle_monitoring_current(&self) -> Result<serde_json::Value> {
        debug!("Getting current metrics");

        match self.metrics_store.latest().await {
            Some(snapshot) => Ok(serde_json::to_value(snapshot)?),
            None => Ok(
                serde_json::json!({ "status": "collecting", "message": "No metrics collected yet" }),
            ),
        }
    }

    /// monitoring.history — historical metrics at given resolution
    pub(super) async fn handle_monitoring_history(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        debug!("Getting metrics history");

        let resolution = params
            .as_ref()
            .and_then(|p| p.get("resolution"))
            .and_then(|v| v.as_str())
            .unwrap_or("minute");

        let count = params
            .as_ref()
            .and_then(|p| p.get("count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(60) as usize;

        // Clamp count to reasonable limits
        let count = count.min(1440);

        let data = match resolution {
            "quarter_hour" | "15min" => self.metrics_store.history_quarter_hours(count).await,
            _ => self.metrics_store.history_minutes(count).await,
        };

        Ok(serde_json::json!({
            "resolution": resolution,
            "count": data.len(),
            "data": data,
        }))
    }

    /// monitoring.containers — latest per-container metrics
    pub(super) async fn handle_monitoring_containers(&self) -> Result<serde_json::Value> {
        debug!("Getting container metrics");

        match self.metrics_store.latest().await {
            Some(snapshot) => Ok(serde_json::json!({
                "timestamp": snapshot.timestamp,
                "containers": snapshot.containers,
            })),
            None => Ok(serde_json::json!({ "containers": [] })),
        }
    }

    /// monitoring.alerts — get fired alert history
    pub(super) async fn handle_monitoring_alerts(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        debug!("Getting alert history");

        let count = params
            .as_ref()
            .and_then(|p| p.get("count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        let alerts = self.metrics_store.get_fired_alerts(count).await;

        Ok(serde_json::json!({
            "count": alerts.len(),
            "alerts": alerts,
        }))
    }

    /// monitoring.alert-rules — get current alert rules
    pub(super) async fn handle_monitoring_alert_rules(&self) -> Result<serde_json::Value> {
        debug!("Getting alert rules");

        let rules = self.metrics_store.get_alert_rules().await;
        Ok(serde_json::json!({ "rules": rules }))
    }

    /// monitoring.configure-alert — update an alert rule
    pub(super) async fn handle_monitoring_configure_alert(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let kind_str = params
            .get("kind")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'kind' parameter"))?;

        let kind: AlertRuleKind = serde_json::from_value(serde_json::json!(kind_str))
            .map_err(|_| anyhow::anyhow!("Invalid alert kind: {}", kind_str))?;

        let enabled = params.get("enabled").and_then(|v| v.as_bool());
        let threshold = params.get("threshold").and_then(|v| v.as_f64());

        self.metrics_store
            .update_alert_rule(&kind, enabled, threshold)
            .await;

        debug!("Updated alert rule: {:?}", kind);
        Ok(serde_json::json!({ "updated": true, "kind": kind_str }))
    }

    /// monitoring.export — export metrics as CSV or JSON
    pub(super) async fn handle_monitoring_export(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        debug!("Exporting metrics");

        let format = params
            .as_ref()
            .and_then(|p| p.get("format"))
            .and_then(|v| v.as_str())
            .unwrap_or("csv");

        let resolution = params
            .as_ref()
            .and_then(|p| p.get("resolution"))
            .and_then(|v| v.as_str())
            .unwrap_or("minute");

        let count = params
            .as_ref()
            .and_then(|p| p.get("count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1440) as usize;

        let count = count.min(1440);

        let data = match resolution {
            "quarter_hour" | "15min" => self.metrics_store.history_quarter_hours(count).await,
            _ => self.metrics_store.history_minutes(count).await,
        };

        match format {
            "json" => Ok(serde_json::json!({
                "format": "json",
                "resolution": resolution,
                "count": data.len(),
                "data": data,
            })),
            _ => {
                // CSV format
                let mut csv = String::from(
                    "timestamp,cpu_percent,mem_used_bytes,mem_total_bytes,disk_used_bytes,disk_total_bytes,net_rx_bytes,net_tx_bytes,load_avg_1,load_avg_5,load_avg_15,rpc_latency_ms,ws_connections\n"
                );
                for snapshot in &data {
                    csv.push_str(&format!(
                        "{},{:.1},{},{},{},{},{},{},{:.2},{:.2},{:.2},{:.1},{}\n",
                        snapshot.timestamp,
                        snapshot.system.cpu_percent,
                        snapshot.system.mem_used_bytes,
                        snapshot.system.mem_total_bytes,
                        snapshot.system.disk_used_bytes,
                        snapshot.system.disk_total_bytes,
                        snapshot.system.net_rx_bytes,
                        snapshot.system.net_tx_bytes,
                        snapshot.system.load_avg_1,
                        snapshot.system.load_avg_5,
                        snapshot.system.load_avg_15,
                        snapshot.rpc_latency_ms,
                        snapshot.ws_connections,
                    ));
                }
                Ok(serde_json::json!({
                    "format": "csv",
                    "resolution": resolution,
                    "count": data.len(),
                    "csv": csv,
                }))
            }
        }
    }

    /// monitoring.acknowledge-alert — acknowledge a fired alert
    pub(super) async fn handle_monitoring_acknowledge_alert(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let alert_id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'id' parameter"))?;

        let found = self.metrics_store.acknowledge_alert(alert_id).await;

        Ok(serde_json::json!({ "acknowledged": found, "id": alert_id }))
    }
}
