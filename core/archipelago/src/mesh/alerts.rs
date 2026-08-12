// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Emergency alert system and dead man's switch for mesh networking.
//!
//! The dead man's switch automatically broadcasts a signed alert with GPS
//! coordinates if the node operator hasn't interacted with the system for
//! a configurable interval (default 6 hours). Useful for remote/off-grid
//! deployments where physical safety is a concern.

use super::message_types::{
    self, AlertPayload, AlertType, Coordinate, MeshMessageType, TypedEnvelope,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Default dead man's switch interval: 6 hours.
const DEFAULT_INTERVAL_SECS: u64 = 21600;

/// How often the background task checks the switch (60 seconds).
const CHECK_INTERVAL_SECS: u64 = 60;

const ALERT_CONFIG_FILE: &str = "alert-config.json";

/// Alert system configuration (persisted to disk).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Whether the dead man's switch is enabled.
    pub dead_man_enabled: bool,
    /// Interval in seconds before the switch triggers.
    pub dead_man_interval_secs: u64,
    /// Last known GPS coordinates (for inclusion in alerts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_gps: Option<Coordinate>,
    /// DIDs of peers to alert directly (in addition to mesh broadcast).
    #[serde(default)]
    pub emergency_contacts: Vec<String>,
    /// Whether to automatically include GPS in dead man alerts.
    pub auto_include_gps: bool,
    /// Custom message to include in dead man alert.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_message: Option<String>,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            dead_man_enabled: false,
            dead_man_interval_secs: DEFAULT_INTERVAL_SECS,
            last_gps: None,
            emergency_contacts: Vec::new(),
            auto_include_gps: false,
            custom_message: None,
        }
    }
}

/// Load alert config from disk.
pub async fn load_config(data_dir: &Path) -> Result<AlertConfig> {
    let path = data_dir.join(ALERT_CONFIG_FILE);
    if !path.exists() {
        return Ok(AlertConfig::default());
    }
    let content = tokio::fs::read_to_string(&path)
        .await
        .context("Failed to read alert config")?;
    let config: AlertConfig = serde_json::from_str(&content).unwrap_or_default();
    Ok(config)
}

/// Save alert config to disk.
pub async fn save_config(data_dir: &Path, config: &AlertConfig) -> Result<()> {
    let content =
        serde_json::to_string_pretty(config).context("Failed to serialize alert config")?;
    tokio::fs::write(data_dir.join(ALERT_CONFIG_FILE), content)
        .await
        .context("Failed to write alert config")?;
    Ok(())
}

/// Dead man's switch state.
pub struct DeadManSwitch {
    config: RwLock<AlertConfig>,
    last_activity: RwLock<Instant>,
    triggered: RwLock<bool>,
    data_dir: PathBuf,
}

impl DeadManSwitch {
    /// Create a new dead man's switch.
    pub async fn new(data_dir: &Path) -> Result<Self> {
        let config = load_config(data_dir).await?;
        Ok(Self {
            config: RwLock::new(config),
            last_activity: RwLock::new(Instant::now()),
            triggered: RwLock::new(false),
            data_dir: data_dir.to_path_buf(),
        })
    }

    /// Record user activity (resets the timer).
    pub async fn check_in(&self) {
        *self.last_activity.write().await = Instant::now();
        *self.triggered.write().await = false;
    }

    /// Check if the switch has been triggered.
    pub async fn is_triggered(&self) -> bool {
        let config = self.config.read().await;
        if !config.dead_man_enabled {
            return false;
        }
        let last = *self.last_activity.read().await;
        let interval = Duration::from_secs(config.dead_man_interval_secs);
        last.elapsed() > interval
    }

    /// Update configuration.
    pub async fn configure(&self, config: AlertConfig) -> Result<()> {
        save_config(&self.data_dir, &config).await?;
        *self.config.write().await = config;
        Ok(())
    }

    /// Get current configuration.
    pub async fn get_config(&self) -> AlertConfig {
        self.config.read().await.clone()
    }

    /// Get time remaining before trigger (in seconds), or 0 if triggered.
    pub async fn time_remaining_secs(&self) -> u64 {
        let config = self.config.read().await;
        if !config.dead_man_enabled {
            return u64::MAX;
        }
        let last = *self.last_activity.read().await;
        let interval = Duration::from_secs(config.dead_man_interval_secs);
        let elapsed = last.elapsed();
        if elapsed > interval {
            0
        } else {
            (interval - elapsed).as_secs()
        }
    }

    /// Build the dead man alert payload.
    pub async fn build_alert(&self) -> AlertPayload {
        let config = self.config.read().await;
        let message = config.custom_message.clone().unwrap_or_else(|| {
            "Dead man's switch triggered — node operator unresponsive".to_string()
        });

        AlertPayload {
            alert_type: AlertType::DeadMan,
            message,
            coordinate: if config.auto_include_gps {
                config.last_gps.clone()
            } else {
                None
            },
        }
    }

    /// Build a signed alert envelope ready for mesh transmission.
    pub async fn build_signed_alert(
        &self,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Result<Vec<u8>> {
        let alert = self.build_alert().await;
        let payload = message_types::encode_payload(&alert)?;
        let envelope = TypedEnvelope::new_signed(MeshMessageType::Alert, payload, signing_key);
        envelope.to_wire()
    }

    /// Check if the alert has already been sent (prevents re-broadcasting every 60s).
    pub async fn triggered_flag(&self) -> tokio::sync::RwLockReadGuard<'_, bool> {
        self.triggered.read().await
    }

    /// Mark the switch as having fired (alert already sent).
    pub async fn mark_triggered(&self) {
        *self.triggered.write().await = true;
    }

    /// Get the list of emergency contact DIDs.
    pub async fn emergency_contacts(&self) -> Vec<String> {
        self.config.read().await.emergency_contacts.clone()
    }

    /// Update GPS coordinates.
    pub async fn update_gps(&self, coord: Coordinate) -> Result<()> {
        let mut config = self.config.write().await;
        config.last_gps = Some(coord);
        save_config(&self.data_dir, &config).await?;
        Ok(())
    }

    /// Get status info for RPC.
    pub async fn status(&self) -> AlertStatus {
        let config = self.config.read().await;
        let triggered = self.is_triggered().await;
        let remaining = self.time_remaining_secs().await;

        AlertStatus {
            dead_man_enabled: config.dead_man_enabled,
            dead_man_interval_secs: config.dead_man_interval_secs,
            triggered,
            time_remaining_secs: remaining,
            has_gps: config.last_gps.is_some(),
            emergency_contacts: config.emergency_contacts.len(),
        }
    }
}

/// Status info returned via RPC.
#[derive(Debug, Clone, Serialize)]
pub struct AlertStatus {
    pub dead_man_enabled: bool,
    pub dead_man_interval_secs: u64,
    pub triggered: bool,
    pub time_remaining_secs: u64,
    pub has_gps: bool,
    pub emergency_contacts: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_config_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let config = AlertConfig {
            dead_man_enabled: true,
            dead_man_interval_secs: 3600,
            last_gps: Some(Coordinate::from_degrees(
                30.2672,
                -97.7431,
                Some("Austin".into()),
            )),
            emergency_contacts: vec!["did:key:z6MkContact1".into()],
            auto_include_gps: true,
            custom_message: Some("Help!".into()),
        };
        save_config(dir.path(), &config).await.unwrap();
        let loaded = load_config(dir.path()).await.unwrap();
        assert!(loaded.dead_man_enabled);
        assert_eq!(loaded.dead_man_interval_secs, 3600);
        assert_eq!(loaded.emergency_contacts.len(), 1);
    }

    #[tokio::test]
    async fn test_dead_man_not_triggered_when_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let switch = DeadManSwitch::new(dir.path()).await.unwrap();
        // Default config has dead_man_enabled = false
        assert!(!switch.is_triggered().await);
    }

    #[tokio::test]
    async fn test_check_in_resets_timer() {
        let dir = tempfile::tempdir().unwrap();
        let switch = DeadManSwitch::new(dir.path()).await.unwrap();
        switch
            .configure(AlertConfig {
                dead_man_enabled: true,
                dead_man_interval_secs: 1, // 1 second for test
                ..Default::default()
            })
            .await
            .unwrap();

        // Wait for trigger
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(switch.is_triggered().await);

        // Check in
        switch.check_in().await;
        assert!(!switch.is_triggered().await);
    }

    #[tokio::test]
    async fn test_build_alert_payload() {
        let dir = tempfile::tempdir().unwrap();
        let switch = DeadManSwitch::new(dir.path()).await.unwrap();
        switch
            .configure(AlertConfig {
                dead_man_enabled: true,
                last_gps: Some(Coordinate::from_degrees(51.5074, -0.1278, None)),
                auto_include_gps: true,
                custom_message: Some("SOS".into()),
                ..Default::default()
            })
            .await
            .unwrap();

        let alert = switch.build_alert().await;
        assert_eq!(alert.alert_type, AlertType::DeadMan);
        assert_eq!(alert.message, "SOS");
        assert!(alert.coordinate.is_some());
    }
}
