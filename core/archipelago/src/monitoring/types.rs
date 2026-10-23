use serde::{Deserialize, Serialize};

/// Maximum entries at 1-minute resolution (24 hours = 1440 minutes)
pub const MAX_1MIN_ENTRIES: usize = 1440;

/// Maximum entries at 15-minute resolution (7 days = 672 quarter-hours)
pub const MAX_15MIN_ENTRIES: usize = 672;

/// Maximum number of fired alerts to keep in history.
pub const MAX_ALERT_HISTORY: usize = 100;

/// A single metrics snapshot collected at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSnapshot {
    pub timestamp: i64,
    pub system: SystemMetrics,
    pub containers: Vec<ContainerMetrics>,
    pub rpc_latency_ms: f64,
    pub ws_connections: u32,
    /// Media-path counters (P1). Without these, D1/D3/D5 have nothing to read back.
    /// `#[serde(default)]` so snapshots persisted before this field existed still parse.
    #[serde(default)]
    pub media: crate::media_metrics::MediaMetrics,
}

/// System-wide resource metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_percent: f64,
    pub mem_used_bytes: u64,
    pub mem_total_bytes: u64,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    pub load_avg_1: f64,
    pub load_avg_5: f64,
    pub load_avg_15: f64,
}

/// Per-container resource metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerMetrics {
    pub name: String,
    pub cpu_percent: f64,
    pub mem_used_bytes: u64,
    pub mem_limit_bytes: u64,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    pub block_read_bytes: u64,
    pub block_write_bytes: u64,
}

/// Types of alert rules the system can evaluate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AlertRuleKind {
    DiskUsage,
    RamUsage,
    CpuLoad,
    ContainerCrash,
    BackendErrorSpike,
    SslCertExpiry,
}

/// A configured alert rule with threshold and enabled state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub kind: AlertRuleKind,
    pub threshold: f64,
    pub enabled: bool,
    pub description: String,
}

/// A fired alert instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiredAlert {
    pub id: String,
    pub kind: AlertRuleKind,
    pub message: String,
    pub value: f64,
    pub threshold: f64,
    pub timestamp: i64,
    pub acknowledged: bool,
}
