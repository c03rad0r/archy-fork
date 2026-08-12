//! Usage metering engine for streaming data payments.
//!
//! Tracks resource consumption (bytes, time, requests) per session
//! and enforces allotment limits.

use super::pricing::Metric;
use super::session::{self, StreamingSession};
use anyhow::Result;
use std::path::Path;
use tracing::debug;

/// Result of checking whether a request should be allowed.
#[derive(Debug)]
pub enum MeterDecision {
    /// Request allowed — session has sufficient allotment.
    Allow { session_id: String, remaining: u64 },
    /// Request denied — session exhausted or expired.
    Exhausted { session_id: String },
    /// No active session found for this peer+service.
    NoSession,
    /// Service is not configured for metering (free access).
    NotMetered,
}

/// Record usage for a peer's session and check if they can continue.
pub async fn record_and_check(
    data_dir: &Path,
    peer_id: &str,
    service_id: &str,
    usage_amount: u64,
) -> Result<MeterDecision> {
    let mut store = session::load_sessions(data_dir).await?;

    let decision = match store.find_active_mut(peer_id, service_id) {
        Some(session) => {
            let still_active = session.record_usage(usage_amount);
            let session_id = session.id.clone();
            let remaining = session.remaining();

            if still_active {
                debug!(
                    "Meter: peer={} service={} used={} remaining={}",
                    peer_id, service_id, usage_amount, remaining
                );
                MeterDecision::Allow {
                    session_id,
                    remaining,
                }
            } else {
                debug!(
                    "Meter: peer={} service={} exhausted (used={})",
                    peer_id, service_id, session.used
                );
                session.close();
                MeterDecision::Exhausted { session_id }
            }
        }
        None => MeterDecision::NoSession,
    };

    session::save_sessions(data_dir, &store).await?;
    Ok(decision)
}

/// Check if a peer has an active session for a service without recording usage.
pub async fn check_access(
    data_dir: &Path,
    peer_id: &str,
    service_id: &str,
    required_amount: u64,
) -> Result<MeterDecision> {
    let store = session::load_sessions(data_dir).await?;

    match store.find_active(peer_id, service_id) {
        Some(session) => {
            if session.can_serve(required_amount) {
                Ok(MeterDecision::Allow {
                    session_id: session.id.clone(),
                    remaining: session.remaining(),
                })
            } else {
                Ok(MeterDecision::Exhausted {
                    session_id: session.id.clone(),
                })
            }
        }
        None => Ok(MeterDecision::NoSession),
    }
}

/// Get usage summary for a session.
pub async fn get_usage(data_dir: &Path, session_id: &str) -> Result<Option<UsageSummary>> {
    let store = session::load_sessions(data_dir).await?;
    Ok(store.get(session_id).map(UsageSummary::from_session))
}

/// Get usage summary for a peer's active session on a service.
pub async fn get_peer_usage(
    data_dir: &Path,
    peer_id: &str,
    service_id: &str,
) -> Result<Option<UsageSummary>> {
    let store = session::load_sessions(data_dir).await?;
    Ok(store
        .find_active(peer_id, service_id)
        .map(UsageSummary::from_session))
}

/// Usage summary for display.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UsageSummary {
    pub session_id: String,
    pub metric: Metric,
    pub allotment: u64,
    pub used: u64,
    pub remaining: u64,
    pub paid_sats: u64,
    pub active: bool,
    /// Human-readable usage string (e.g., "5.2 MB / 10 MB").
    pub display: String,
}

impl UsageSummary {
    pub fn from_session(session: &StreamingSession) -> Self {
        let remaining = session.remaining();
        let display = format_usage(session.metric, session.used, session.allotment);

        Self {
            session_id: session.id.clone(),
            metric: session.metric,
            allotment: session.allotment,
            used: session.used,
            remaining,
            paid_sats: session.paid_sats,
            active: session.active && !session.is_expired(),
            display,
        }
    }
}

/// Format usage as a human-readable string.
fn format_usage(metric: Metric, used: u64, allotment: u64) -> String {
    match metric {
        Metric::Bytes => {
            format!("{} / {}", format_bytes(used), format_bytes(allotment))
        }
        Metric::Milliseconds => {
            format!(
                "{} / {}",
                format_duration_ms(used),
                format_duration_ms(allotment)
            )
        }
        Metric::Requests => {
            format!("{} / {} requests", used, allotment)
        }
    }
}

/// Format bytes as human-readable (KB, MB, GB).
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1_048_576 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1_073_741_824 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    }
}

/// Format milliseconds as human-readable duration.
fn format_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else {
        format!("{:.1}h", ms as f64 / 3_600_000.0)
    }
}

/// Run periodic maintenance: close expired sessions, prune old records.
pub async fn maintenance(data_dir: &Path) -> Result<usize> {
    let mut store = session::load_sessions(data_dir).await?;
    let closed = store.close_expired();
    store.prune_old();
    session::save_sessions(data_dir, &store).await?;

    if closed > 0 {
        debug!("Meter maintenance: closed {} expired sessions", closed);
    }
    Ok(closed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(5_242_880), "5.0 MB");
        assert_eq!(format_bytes(1_610_612_736), "1.50 GB");
    }

    #[test]
    fn test_format_duration_ms() {
        assert_eq!(format_duration_ms(500), "500ms");
        assert_eq!(format_duration_ms(2500), "2.5s");
        assert_eq!(format_duration_ms(90_000), "1.5m");
        assert_eq!(format_duration_ms(5_400_000), "1.5h");
    }

    #[test]
    fn test_format_usage_bytes() {
        let display = format_usage(Metric::Bytes, 5_242_880, 10_485_760);
        assert_eq!(display, "5.0 MB / 10.0 MB");
    }

    #[test]
    fn test_format_usage_requests() {
        let display = format_usage(Metric::Requests, 3, 10);
        assert_eq!(display, "3 / 10 requests");
    }
}
