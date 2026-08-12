use crate::data_model::{DataModel, WebSocketMessage};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::debug;

/// Manages the application state and broadcasts updates to WebSocket clients
pub struct StateManager {
    data: Arc<RwLock<DataModel>>,
    revision: Arc<RwLock<u32>>,
    broadcast_tx: broadcast::Sender<WebSocketMessage>,
}

impl StateManager {
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(100);
        Self {
            data: Arc::new(RwLock::new(DataModel::new())),
            revision: Arc::new(RwLock::new(0)),
            broadcast_tx,
        }
    }

    /// Get the current data model and revision
    pub async fn get_snapshot(&self) -> (DataModel, u32) {
        let data = self.data.read().await.clone();
        let rev = *self.revision.read().await;
        (data, rev)
    }

    /// Subscribe to state updates
    pub fn subscribe(&self) -> broadcast::Receiver<WebSocketMessage> {
        self.broadcast_tx.subscribe()
    }

    /// Update the data model and broadcast to all connected clients
    pub async fn update_data(&self, new_data: DataModel) {
        let mut data = self.data.write().await;
        let mut rev = self.revision.write().await;

        *data = new_data.clone();
        *rev += 1;

        debug!("Data model updated to revision {}", *rev);

        // Broadcast full data dump to all connected clients
        // In the future, we can optimize this by computing and sending JSON patches
        let message = WebSocketMessage {
            rev: *rev,
            data: Some(new_data),
            patch: None,
        };

        // Ignore errors if no receivers are connected
        let _ = self.broadcast_tx.send(message);
    }

    /// Get a WebSocket message with the current state
    pub async fn get_initial_message(&self) -> WebSocketMessage {
        let (data, rev) = self.get_snapshot().await;
        WebSocketMessage {
            rev,
            data: Some(data),
            patch: None,
        }
    }
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_model::DataModel;

    #[test]
    fn test_state_manager_new() {
        let sm = StateManager::new();
        // Should be constructible without panic
        let _ = sm.subscribe();
    }

    #[test]
    fn test_state_manager_default() {
        let sm = StateManager::default();
        let _ = sm.subscribe();
    }

    #[tokio::test]
    async fn test_get_snapshot_initial_revision_zero() {
        let sm = StateManager::new();
        let (data, rev) = sm.get_snapshot().await;
        assert_eq!(rev, 0);
        // DataModel::new() sets version from CARGO_PKG_VERSION
        assert_eq!(data.server_info.version, env!("CARGO_PKG_VERSION"));
        assert!(data.package_data.is_empty());
        assert!(data.notifications.is_empty());
    }

    #[tokio::test]
    async fn test_update_data_increments_revision() {
        let sm = StateManager::new();

        let (_, rev0) = sm.get_snapshot().await;
        assert_eq!(rev0, 0);

        sm.update_data(DataModel::new()).await;
        let (_, rev1) = sm.get_snapshot().await;
        assert_eq!(rev1, 1);

        sm.update_data(DataModel::new()).await;
        let (_, rev2) = sm.get_snapshot().await;
        assert_eq!(rev2, 2);
    }

    #[tokio::test]
    async fn test_update_data_stores_new_data() {
        let sm = StateManager::new();

        let mut new_data = DataModel::new();
        new_data.server_info.name = Some("TestNode".to_string());
        new_data.ui.theme = "light".to_string();
        sm.update_data(new_data).await;

        let (snapshot, _) = sm.get_snapshot().await;
        assert_eq!(snapshot.server_info.name, Some("TestNode".to_string()));
        assert_eq!(snapshot.ui.theme, "light");
    }

    #[tokio::test]
    async fn test_subscribe_receives_updates() {
        let sm = StateManager::new();
        let mut rx = sm.subscribe();

        let mut data = DataModel::new();
        data.server_info.name = Some("BroadcastTest".to_string());
        sm.update_data(data).await;

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.rev, 1);
        assert!(msg.data.is_some());
        let received_data = msg.data.unwrap();
        assert_eq!(
            received_data.server_info.name,
            Some("BroadcastTest".to_string())
        );
    }

    #[tokio::test]
    async fn test_multiple_subscribers_receive_same_update() {
        let sm = StateManager::new();
        let mut rx1 = sm.subscribe();
        let mut rx2 = sm.subscribe();

        sm.update_data(DataModel::new()).await;

        let msg1 = rx1.recv().await.unwrap();
        let msg2 = rx2.recv().await.unwrap();
        assert_eq!(msg1.rev, msg2.rev);
        assert_eq!(msg1.rev, 1);
    }

    #[tokio::test]
    async fn test_get_initial_message() {
        let sm = StateManager::new();

        let mut data = DataModel::new();
        data.server_info.name = Some("InitMsg".to_string());
        sm.update_data(data).await;

        let msg = sm.get_initial_message().await;
        assert_eq!(msg.rev, 1);
        assert!(msg.data.is_some());
        assert!(msg.patch.is_none());
        assert_eq!(
            msg.data.unwrap().server_info.name,
            Some("InitMsg".to_string())
        );
    }

    #[tokio::test]
    async fn test_update_without_subscribers_does_not_panic() {
        let sm = StateManager::new();
        // No subscribers — send should silently succeed
        sm.update_data(DataModel::new()).await;
        let (_, rev) = sm.get_snapshot().await;
        assert_eq!(rev, 1);
    }
}
