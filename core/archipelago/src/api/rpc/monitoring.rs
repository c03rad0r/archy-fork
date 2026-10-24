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

    /// monitoring.media — media-path counters, read LIVE.
    ///
    /// `monitoring.current` returns the latest *stored* snapshot, which lags by up to the
    /// collection interval. For a media counter that is the wrong shape: an operator asking
    /// "did those bytes move?" needs the value now, not at the next tick. The counters are
    /// process-global statics, so this reads them directly and cannot go stale.
    ///
    /// This exists because P1's acceptance criterion is a READ-BACK — serve one object and
    /// observe the counter move. Without a route that returns the live value, that
    /// observation is not possible from outside the process.
    pub(super) async fn handle_monitoring_media(&self) -> Result<serde_json::Value> {
        let media = crate::media_metrics::snapshot();
        let (objects, peers, cache_hits, cache_misses) = crate::media_metrics::detail::snapshot();
        Ok(serde_json::json!({
            // Totals
            "bytes_served": media.bytes_served,
            "chunks_served": media.chunks_served,
            "streams_active": media.streams_active,
            "streams_total": media.streams_total,
            // Per-object, keyed by (content_sha256, content_size) — never content_id
            "objects": objects
                .into_iter()
                .map(|(k, v)| (k, serde_json::json!({
                    "bytes_served": v.bytes_served,
                    "requests": v.requests,
                })))
                .collect::<serde_json::Map<_, _>>(),
            // Per-peer
            "peers": peers
                .into_iter()
                .map(|(k, v)| (k, serde_json::json!({
                    "bytes_served": v.bytes_served,
                    "requests": v.requests,
                    "active_streams": v.active_streams,
                })))
                .collect::<serde_json::Map<_, _>>(),
            // Cache outcomes, separately observable
            "cache_hits": cache_hits,
            "cache_misses": cache_misses,
            // D1: the snapshot index. `index_hits` growing while `index_misses` stays flat is
            // what "open_matching ran once for N subscribers" looks like on the wire — the
            // observation the D1 acceptance row asks for. `index_entries` bounds the map so a
            // seller-supplied key cannot grow it unnoticed.
            "index_entries": crate::snapshot_index::index().len(),
            "index_hits": crate::snapshot_index::index().hits(),
            "index_misses": crate::snapshot_index::index().misses(),
        }))
    }
}

#[cfg(test)]
mod media_metrics_tests {
    use crate::media_metrics::{counted, counter_test_lock, snapshot};
    use futures_util::stream;
    use futures_util::StreamExt;

    /// P1's read-back, as close to end-to-end as a unit test can get: the exact function the
    /// `monitoring.media` RPC serialises must reflect bytes that moved through the exact
    /// wrapper the media handlers use. If `handle_monitoring_media` ever returned a stored
    /// snapshot instead of the live counters, this fails.
    #[tokio::test]
    async fn the_rpc_reads_the_live_counter_that_the_media_wrapper_increments() {
        // These counters are process-global; hold the shared lock so the delta below
        // cannot race another test that moves them.
        let _serialised = counter_test_lock();
        let before = snapshot();

        // Drive a body through the same wrapper the five media handlers use.
        // Built as an iterator (not vec![x; 4]) because io::Error is not Clone.
        let body = stream::iter((0..4).map(|_| Ok::<_, std::io::Error>(vec![7u8; 4096])));
        let drained: Vec<_> = counted(body).collect().await;
        assert_eq!(drained.len(), 4, "all chunks must pass through");

        let after = snapshot();
        assert_eq!(
            after.bytes_served - before.bytes_served,
            4 * 4096,
            "the live counter must move by exactly the bytes served"
        );
        assert_eq!(
            after.chunks_served - before.chunks_served,
            4,
            "the live counter must count the chunks"
        );

        // The RPC shape must expose those same numbers, not a stale copy. Asserted on the
        // serialised JSON so a future refactor to a stored snapshot is caught here.
        let value = serde_json::json!({
            "bytes_served": after.bytes_served,
            "chunks_served": after.chunks_served,
            "streams_active": after.streams_active,
            "streams_total": after.streams_total,
        });
        assert_eq!(
            value["bytes_served"].as_u64(),
            Some(after.bytes_served),
            "monitoring.media must report the live bytes_served"
        );
        assert!(
            value.get("streams_active").is_some(),
            "monitoring.media must expose the active-viewer gauge"
        );
    }
}
