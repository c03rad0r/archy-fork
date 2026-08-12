use crate::monitoring::alerts::load_alert_rules;
use crate::monitoring::types::*;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::RwLock;

/// Thread-safe metrics store with ring buffers at two resolutions.
pub struct MetricsStore {
    minute_data: RwLock<VecDeque<MetricSnapshot>>,
    quarter_hour_data: RwLock<VecDeque<MetricSnapshot>>,
    minute_count: RwLock<u32>,
    rpc_latency: RwLock<(f64, u64)>,
    ws_connections: AtomicU32,
    pub(crate) alert_rules: RwLock<Vec<AlertRule>>,
    pub(crate) fired_alerts: RwLock<VecDeque<FiredAlert>>,
    pub(crate) data_dir: Option<PathBuf>,
}

impl MetricsStore {
    #[cfg(test)]
    pub fn new() -> Self {
        Self {
            minute_data: RwLock::new(VecDeque::with_capacity(MAX_1MIN_ENTRIES)),
            quarter_hour_data: RwLock::new(VecDeque::with_capacity(MAX_15MIN_ENTRIES)),
            minute_count: RwLock::new(0),
            rpc_latency: RwLock::new((0.0, 0)),
            ws_connections: AtomicU32::new(0),
            alert_rules: RwLock::new(AlertRule::default_rules()),
            fired_alerts: RwLock::new(VecDeque::with_capacity(MAX_ALERT_HISTORY)),
            data_dir: None,
        }
    }

    /// Create a MetricsStore that persists alert rules to disk.
    pub async fn with_data_dir(data_dir: PathBuf) -> Self {
        let rules = load_alert_rules(&data_dir).await;
        Self {
            minute_data: RwLock::new(VecDeque::with_capacity(MAX_1MIN_ENTRIES)),
            quarter_hour_data: RwLock::new(VecDeque::with_capacity(MAX_15MIN_ENTRIES)),
            minute_count: RwLock::new(0),
            rpc_latency: RwLock::new((0.0, 0)),
            ws_connections: AtomicU32::new(0),
            alert_rules: RwLock::new(rules),
            fired_alerts: RwLock::new(VecDeque::with_capacity(MAX_ALERT_HISTORY)),
            data_dir: Some(data_dir),
        }
    }

    /// Record a new metric snapshot (called every minute by collector).
    pub async fn push(&self, mut snapshot: MetricSnapshot) {
        // Fill in RPC latency from accumulated samples
        {
            let mut latency = self.rpc_latency.write().await;
            if latency.1 > 0 {
                snapshot.rpc_latency_ms = (latency.0 / latency.1 as f64 * 10.0).round() / 10.0;
                *latency = (0.0, 0);
            }
        }

        // Fill in current WS connection count
        snapshot.ws_connections = self.ws_connections.load(Ordering::Relaxed);

        // Push to 1-minute ring buffer
        {
            let mut buf = self.minute_data.write().await;
            if buf.len() >= MAX_1MIN_ENTRIES {
                buf.pop_front();
            }
            buf.push_back(snapshot.clone());
        }

        // Every 15 minutes, push to quarter-hour ring buffer
        {
            let mut count = self.minute_count.write().await;
            *count += 1;
            if *count >= 15 {
                *count = 0;
                let mut buf = self.quarter_hour_data.write().await;
                if buf.len() >= MAX_15MIN_ENTRIES {
                    buf.pop_front();
                }
                buf.push_back(snapshot);
            }
        }
    }

    /// Record an RPC request latency sample (milliseconds).
    pub async fn record_rpc_latency(&self, latency_ms: f64) {
        let mut data = self.rpc_latency.write().await;
        data.0 += latency_ms;
        data.1 += 1;
    }

    /// Increment WebSocket connection count (called on connect).
    pub fn increment_ws(&self) {
        self.ws_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement WebSocket connection count (called on disconnect).
    pub fn decrement_ws(&self) {
        // Use saturating semantics to avoid underflow
        let _ = self
            .ws_connections
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                if v > 0 {
                    Some(v - 1)
                } else {
                    Some(0)
                }
            });
    }

    /// Get the latest snapshot.
    pub async fn latest(&self) -> Option<MetricSnapshot> {
        self.minute_data.read().await.back().cloned()
    }

    /// Get minute-resolution data for the last N minutes.
    pub async fn history_minutes(&self, last_n: usize) -> Vec<MetricSnapshot> {
        let buf = self.minute_data.read().await;
        let start = buf.len().saturating_sub(last_n);
        buf.iter().skip(start).cloned().collect()
    }

    /// Get quarter-hour-resolution data for the last N entries.
    pub async fn history_quarter_hours(&self, last_n: usize) -> Vec<MetricSnapshot> {
        let buf = self.quarter_hour_data.read().await;
        let start = buf.len().saturating_sub(last_n);
        buf.iter().skip(start).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_store_new() {
        let store = MetricsStore::new();
        assert_eq!(store.ws_connections.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_ws_connection_tracking() {
        let store = MetricsStore::new();
        store.increment_ws();
        store.increment_ws();
        assert_eq!(store.ws_connections.load(Ordering::Relaxed), 2);
        store.decrement_ws();
        assert_eq!(store.ws_connections.load(Ordering::Relaxed), 1);
        store.decrement_ws();
        assert_eq!(store.ws_connections.load(Ordering::Relaxed), 0);
        // Decrement below zero should stay at 0
        store.decrement_ws();
        assert_eq!(store.ws_connections.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_push_and_latest() {
        let store = MetricsStore::new();
        assert!(store.latest().await.is_none());

        let snapshot = MetricSnapshot {
            timestamp: 1000,
            system: SystemMetrics {
                cpu_percent: 25.0,
                mem_used_bytes: 1_000_000,
                mem_total_bytes: 4_000_000,
                disk_used_bytes: 500_000,
                disk_total_bytes: 1_000_000,
                net_rx_bytes: 100,
                net_tx_bytes: 200,
                load_avg_1: 1.0,
                load_avg_5: 0.5,
                load_avg_15: 0.3,
            },
            containers: vec![],
            rpc_latency_ms: 0.0,
            ws_connections: 0,
        };

        store.push(snapshot).await;
        let latest = store.latest().await.unwrap();
        assert_eq!(latest.timestamp, 1000);
        assert_eq!(latest.system.cpu_percent, 25.0);
    }

    #[tokio::test]
    async fn test_rpc_latency_recording() {
        let store = MetricsStore::new();
        store.record_rpc_latency(10.0).await;
        store.record_rpc_latency(20.0).await;
        store.record_rpc_latency(30.0).await;

        let snapshot = MetricSnapshot {
            timestamp: 2000,
            system: SystemMetrics {
                cpu_percent: 0.0,
                mem_used_bytes: 0,
                mem_total_bytes: 0,
                disk_used_bytes: 0,
                disk_total_bytes: 0,
                net_rx_bytes: 0,
                net_tx_bytes: 0,
                load_avg_1: 0.0,
                load_avg_5: 0.0,
                load_avg_15: 0.0,
            },
            containers: vec![],
            rpc_latency_ms: 0.0,
            ws_connections: 0,
        };

        store.push(snapshot).await;
        let latest = store.latest().await.unwrap();
        assert_eq!(latest.rpc_latency_ms, 20.0); // average of 10+20+30 = 20
    }

    #[tokio::test]
    async fn test_history_minutes() {
        let store = MetricsStore::new();

        for i in 0..5 {
            let snapshot = MetricSnapshot {
                timestamp: i * 60,
                system: SystemMetrics {
                    cpu_percent: i as f64,
                    mem_used_bytes: 0,
                    mem_total_bytes: 0,
                    disk_used_bytes: 0,
                    disk_total_bytes: 0,
                    net_rx_bytes: 0,
                    net_tx_bytes: 0,
                    load_avg_1: 0.0,
                    load_avg_5: 0.0,
                    load_avg_15: 0.0,
                },
                containers: vec![],
                rpc_latency_ms: 0.0,
                ws_connections: 0,
            };
            store.push(snapshot).await;
        }

        let history = store.history_minutes(3).await;
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].timestamp, 120);
        assert_eq!(history[2].timestamp, 240);
    }

    #[tokio::test]
    async fn test_ring_buffer_eviction() {
        let store = MetricsStore::new();

        // Push more than MAX_1MIN_ENTRIES
        for i in 0..(MAX_1MIN_ENTRIES + 10) {
            let snapshot = MetricSnapshot {
                timestamp: i as i64,
                system: SystemMetrics {
                    cpu_percent: 0.0,
                    mem_used_bytes: 0,
                    mem_total_bytes: 0,
                    disk_used_bytes: 0,
                    disk_total_bytes: 0,
                    net_rx_bytes: 0,
                    net_tx_bytes: 0,
                    load_avg_1: 0.0,
                    load_avg_5: 0.0,
                    load_avg_15: 0.0,
                },
                containers: vec![],
                rpc_latency_ms: 0.0,
                ws_connections: 0,
            };
            store.push(snapshot).await;
        }

        let all = store.history_minutes(MAX_1MIN_ENTRIES + 100).await;
        assert_eq!(all.len(), MAX_1MIN_ENTRIES);
        // Oldest entry should be 10 (first 10 were evicted)
        assert_eq!(all[0].timestamp, 10);
    }

    #[tokio::test]
    async fn test_quarter_hour_downsampling() {
        let store = MetricsStore::new();

        // Push exactly 15 entries to trigger one quarter-hour sample
        for i in 0..15 {
            let snapshot = MetricSnapshot {
                timestamp: i * 60,
                system: SystemMetrics {
                    cpu_percent: 50.0,
                    mem_used_bytes: 0,
                    mem_total_bytes: 0,
                    disk_used_bytes: 0,
                    disk_total_bytes: 0,
                    net_rx_bytes: 0,
                    net_tx_bytes: 0,
                    load_avg_1: 0.0,
                    load_avg_5: 0.0,
                    load_avg_15: 0.0,
                },
                containers: vec![],
                rpc_latency_ms: 0.0,
                ws_connections: 0,
            };
            store.push(snapshot).await;
        }

        let qh = store.history_quarter_hours(10).await;
        assert_eq!(qh.len(), 1);
        assert_eq!(qh[0].timestamp, 14 * 60); // The 15th entry (index 14)
    }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_1MIN_ENTRIES, 1440);
        assert_eq!(MAX_15MIN_ENTRIES, 672);
    }
}
