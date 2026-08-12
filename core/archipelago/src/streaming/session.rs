//! Streaming session management.
//!
//! Tracks active metered sessions: which peer has how much allotment remaining
//! for which service. Supports incremental top-ups (TollGate-style).

use super::pricing::{Metric, ServicePricing};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

const SESSIONS_FILE: &str = "streaming/sessions.json";

/// A single streaming session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingSession {
    /// Unique session ID.
    pub id: String,
    /// Peer identifier (Nostr pubkey, DID, or onion address).
    pub peer_id: String,
    /// Service this session is for.
    pub service_id: String,
    /// Metric type for this session.
    pub metric: Metric,
    /// Total allotment granted (in metric units).
    pub allotment: u64,
    /// Amount consumed so far (in metric units).
    pub used: u64,
    /// Total sats paid for this session.
    pub paid_sats: u64,
    /// When the session was created.
    pub created_at: String,
    /// When the session was last topped up.
    pub last_topup_at: String,
    /// When the session expires (for time-based: created_at + allotment_ms).
    /// Empty string for non-time-based sessions.
    #[serde(default)]
    pub expires_at: String,
    /// Whether the session is still active.
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_true() -> bool {
    true
}

impl StreamingSession {
    /// Create a new session from a payment.
    pub fn new(peer_id: &str, service_id: &str, pricing: &ServicePricing, paid_sats: u64) -> Self {
        let allotment = pricing.calculate_allotment(paid_sats);
        let now = chrono::Utc::now();
        let now_str = now.to_rfc3339();

        let expires_at = if pricing.metric == Metric::Milliseconds {
            let expires = now + chrono::Duration::milliseconds(allotment as i64);
            expires.to_rfc3339()
        } else {
            String::new()
        };

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            peer_id: peer_id.to_string(),
            service_id: service_id.to_string(),
            metric: pricing.metric,
            allotment,
            used: 0,
            paid_sats,
            created_at: now_str.clone(),
            last_topup_at: now_str,
            expires_at,
            active: true,
        }
    }

    /// Add more allotment from an additional payment (top-up).
    pub fn topup(&mut self, pricing: &ServicePricing, additional_sats: u64) {
        let additional_allotment = pricing.calculate_allotment(additional_sats);
        self.allotment += additional_allotment;
        self.paid_sats += additional_sats;
        self.last_topup_at = chrono::Utc::now().to_rfc3339();

        // For time-based: extend expiry
        if self.metric == Metric::Milliseconds {
            let current_expires = chrono::DateTime::parse_from_rfc3339(&self.expires_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            let new_expires =
                current_expires + chrono::Duration::milliseconds(additional_allotment as i64);
            self.expires_at = new_expires.to_rfc3339();
        }

        // Reactivate if it was closed
        self.active = true;
    }

    /// Record usage and check if the session is still within its allotment.
    pub fn record_usage(&mut self, amount: u64) -> bool {
        self.used += amount;
        self.remaining() > 0 && !self.is_expired()
    }

    /// Remaining allotment.
    pub fn remaining(&self) -> u64 {
        self.allotment.saturating_sub(self.used)
    }

    /// Check if a time-based session has expired.
    pub fn is_expired(&self) -> bool {
        if !self.active {
            return true;
        }
        if self.metric == Metric::Milliseconds && !self.expires_at.is_empty() {
            if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(&self.expires_at) {
                return chrono::Utc::now() > expires.with_timezone(&chrono::Utc);
            }
        }
        // For non-time-based: expired when allotment consumed
        self.used >= self.allotment
    }

    /// Check if this session can serve a request of the given cost.
    pub fn can_serve(&self, cost: u64) -> bool {
        self.active && !self.is_expired() && self.remaining() >= cost
    }

    /// Close the session.
    pub fn close(&mut self) {
        self.active = false;
    }
}

/// All active and recent sessions.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SessionStore {
    pub sessions: Vec<StreamingSession>,
}

impl SessionStore {
    /// Find an active session for a peer and service.
    pub fn find_active(&self, peer_id: &str, service_id: &str) -> Option<&StreamingSession> {
        self.sessions.iter().find(|s| {
            s.peer_id == peer_id && s.service_id == service_id && s.active && !s.is_expired()
        })
    }

    /// Find a mutable active session for a peer and service.
    pub fn find_active_mut(
        &mut self,
        peer_id: &str,
        service_id: &str,
    ) -> Option<&mut StreamingSession> {
        self.sessions.iter_mut().find(|s| {
            s.peer_id == peer_id && s.service_id == service_id && s.active && !s.is_expired()
        })
    }

    /// Get a session by ID.
    pub fn get(&self, session_id: &str) -> Option<&StreamingSession> {
        self.sessions.iter().find(|s| s.id == session_id)
    }

    /// Get a mutable session by ID.
    pub fn get_mut(&mut self, session_id: &str) -> Option<&mut StreamingSession> {
        self.sessions.iter_mut().find(|s| s.id == session_id)
    }

    /// List all active sessions.
    pub fn active_sessions(&self) -> Vec<&StreamingSession> {
        self.sessions
            .iter()
            .filter(|s| s.active && !s.is_expired())
            .collect()
    }

    /// List all sessions for a peer.
    pub fn sessions_for_peer(&self, peer_id: &str) -> Vec<&StreamingSession> {
        self.sessions
            .iter()
            .filter(|s| s.peer_id == peer_id)
            .collect()
    }

    /// Close expired sessions and return how many were closed.
    pub fn close_expired(&mut self) -> usize {
        let mut closed = 0;
        for session in &mut self.sessions {
            if session.active && session.is_expired() {
                session.active = false;
                closed += 1;
            }
        }
        closed
    }

    /// Prune inactive sessions older than 7 days.
    pub fn prune_old(&mut self) {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
        self.sessions.retain(|s| s.active || s.created_at > cutoff);
    }

    /// Create or top-up a session for a peer+service.
    pub fn create_or_topup(
        &mut self,
        peer_id: &str,
        service_id: &str,
        pricing: &ServicePricing,
        paid_sats: u64,
    ) -> &StreamingSession {
        // Check for existing active session
        if let Some(session) = self.find_active_mut(peer_id, service_id) {
            session.topup(pricing, paid_sats);
            let id = session.id.clone();
            return self.get(&id).unwrap();
        }

        // Create new session
        let session = StreamingSession::new(peer_id, service_id, pricing, paid_sats);
        self.sessions.push(session);
        self.sessions.last().unwrap()
    }

    /// Total revenue from all sessions.
    pub fn total_revenue(&self) -> u64 {
        self.sessions.iter().map(|s| s.paid_sats).sum()
    }

    /// Total revenue by service.
    pub fn revenue_by_service(&self) -> HashMap<String, u64> {
        let mut map = HashMap::new();
        for session in &self.sessions {
            *map.entry(session.service_id.clone()).or_insert(0) += session.paid_sats;
        }
        map
    }
}

/// Load sessions from disk.
pub async fn load_sessions(data_dir: &Path) -> Result<SessionStore> {
    let path = data_dir.join(SESSIONS_FILE);
    if !path.exists() {
        return Ok(SessionStore::default());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read sessions file")?;
    let store: SessionStore = serde_json::from_str(&content).unwrap_or_default();
    Ok(store)
}

/// Save sessions to disk.
pub async fn save_sessions(data_dir: &Path, store: &SessionStore) -> Result<()> {
    let dir = data_dir.join("streaming");
    fs::create_dir_all(&dir)
        .await
        .context("Failed to create streaming dir")?;
    let path = data_dir.join(SESSIONS_FILE);
    let content = serde_json::to_string_pretty(store).context("Failed to serialize sessions")?;
    fs::write(&path, content)
        .await
        .context("Failed to write sessions file")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_pricing(metric: Metric) -> ServicePricing {
        ServicePricing {
            service_id: "test".into(),
            name: "Test".into(),
            metric,
            step_size: match metric {
                Metric::Bytes => 1_048_576,
                Metric::Milliseconds => 60_000,
                Metric::Requests => 1,
            },
            price_per_step: 1,
            min_steps: 0,
            enabled: true,
            description: String::new(),
            accepted_mints: vec![],
        }
    }

    #[test]
    fn test_new_session_bytes() {
        let pricing = test_pricing(Metric::Bytes);
        let session = StreamingSession::new("peer1", "test", &pricing, 10);

        assert_eq!(session.allotment, 10_485_760); // 10 MB
        assert_eq!(session.used, 0);
        assert_eq!(session.paid_sats, 10);
        assert!(session.active);
        assert!(session.expires_at.is_empty());
    }

    #[test]
    fn test_new_session_time() {
        let pricing = test_pricing(Metric::Milliseconds);
        let session = StreamingSession::new("peer1", "test", &pricing, 5);

        assert_eq!(session.allotment, 300_000); // 5 minutes
        assert!(!session.expires_at.is_empty());
        assert!(!session.is_expired());
    }

    #[test]
    fn test_session_topup() {
        let pricing = test_pricing(Metric::Bytes);
        let mut session = StreamingSession::new("peer1", "test", &pricing, 10);
        assert_eq!(session.allotment, 10_485_760);

        session.topup(&pricing, 5);
        assert_eq!(session.allotment, 15_728_640); // 15 MB
        assert_eq!(session.paid_sats, 15);
    }

    #[test]
    fn test_session_record_usage() {
        let pricing = test_pricing(Metric::Requests);
        let mut session = StreamingSession::new("peer1", "test", &pricing, 5);
        assert_eq!(session.allotment, 5);

        assert!(session.record_usage(1));
        assert!(session.record_usage(1));
        assert!(session.record_usage(1));
        assert!(session.record_usage(1));
        assert!(!session.record_usage(1)); // 5th consumes last
        assert_eq!(session.remaining(), 0);
    }

    #[test]
    fn test_session_can_serve() {
        let pricing = test_pricing(Metric::Requests);
        let session = StreamingSession::new("peer1", "test", &pricing, 3);

        assert!(session.can_serve(1));
        assert!(session.can_serve(3));
        assert!(!session.can_serve(4));
    }

    #[test]
    fn test_session_close() {
        let pricing = test_pricing(Metric::Requests);
        let mut session = StreamingSession::new("peer1", "test", &pricing, 5);
        assert!(session.active);

        session.close();
        assert!(!session.active);
        assert!(session.is_expired());
    }

    #[test]
    fn test_session_store_create_or_topup() {
        let pricing = test_pricing(Metric::Requests);
        let mut store = SessionStore::default();

        // First payment creates session
        let s1 = store.create_or_topup("peer1", "test", &pricing, 10);
        let s1_id = s1.id.clone();
        assert_eq!(s1.allotment, 10);
        assert_eq!(s1.paid_sats, 10);

        // Second payment tops up
        let s2 = store.create_or_topup("peer1", "test", &pricing, 5);
        assert_eq!(s2.id, s1_id); // Same session
        assert_eq!(s2.allotment, 15);
        assert_eq!(s2.paid_sats, 15);
    }

    #[test]
    fn test_session_store_different_peers() {
        let pricing = test_pricing(Metric::Requests);
        let mut store = SessionStore::default();

        store.create_or_topup("peer1", "test", &pricing, 10);
        store.create_or_topup("peer2", "test", &pricing, 20);

        assert_eq!(store.active_sessions().len(), 2);
    }

    #[test]
    fn test_close_expired() {
        let pricing = test_pricing(Metric::Requests);
        let mut store = SessionStore::default();

        store.create_or_topup("peer1", "test", &pricing, 1);
        // Consume the allotment
        if let Some(s) = store.find_active_mut("peer1", "test") {
            s.record_usage(1);
        }

        let closed = store.close_expired();
        assert_eq!(closed, 1);
    }

    #[test]
    fn test_revenue_tracking() {
        let pricing = test_pricing(Metric::Requests);
        let mut store = SessionStore::default();

        store.create_or_topup("peer1", "test", &pricing, 100);
        store.create_or_topup("peer2", "test", &pricing, 200);

        assert_eq!(store.total_revenue(), 300);
        let by_service = store.revenue_by_service();
        assert_eq!(*by_service.get("test").unwrap(), 300);
    }

    #[tokio::test]
    async fn test_load_save_sessions() {
        let tmp = TempDir::new().unwrap();
        let pricing = test_pricing(Metric::Bytes);
        let mut store = SessionStore::default();
        store.create_or_topup("peer1", "test", &pricing, 42);

        save_sessions(tmp.path(), &store).await.unwrap();
        let loaded = load_sessions(tmp.path()).await.unwrap();
        assert_eq!(loaded.sessions.len(), 1);
        assert_eq!(loaded.sessions[0].paid_sats, 42);
    }
}
