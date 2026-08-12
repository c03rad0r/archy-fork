use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::fs;

pub struct DevDataManager {
    dev_data_dir: PathBuf,
}

impl DevDataManager {
    pub fn new(dev_data_dir: PathBuf) -> Self {
        Self { dev_data_dir }
    }

    /// Get the dev data directory for an app
    pub fn get_app_data_dir(&self, app_id: &str) -> PathBuf {
        self.dev_data_dir.join(app_id)
    }

    /// Create data directory for an app
    pub async fn create_app_data_dir(&self, app_id: &str) -> Result<PathBuf> {
        let app_dir = self.get_app_data_dir(app_id);
        fs::create_dir_all(&app_dir)
            .await
            .with_context(|| format!("Failed to create app data directory: {:?}", app_dir))?;
        Ok(app_dir)
    }

    /// Map a volume source path to dev path
    pub fn map_volume_path(&self, app_id: &str, volume_source: &str) -> PathBuf {
        // If volume source is already in dev_data_dir, use it as-is
        if volume_source.starts_with(self.dev_data_dir.to_str().unwrap_or("")) {
            PathBuf::from(volume_source)
        } else {
            // Map production path to dev path
            // e.g., /var/lib/archipelago/bitcoin -> /tmp/archipelago-dev/bitcoin
            let app_dir = self.get_app_data_dir(app_id);

            // Extract the relative path from the production path
            if let Some(relative) = volume_source.strip_prefix("/var/lib/archipelago/") {
                app_dir.join(relative)
            } else if let Some(relative) = volume_source.strip_prefix("/var/lib/archipelago") {
                app_dir.join(relative)
            } else {
                // If it doesn't match expected pattern, use app_id as base
                app_dir.join(volume_source.trim_start_matches('/'))
            }
        }
    }

    /// Clean up app data directory
    pub async fn cleanup_app_data(&self, app_id: &str) -> Result<()> {
        let app_dir = self.get_app_data_dir(app_id);
        if app_dir.exists() {
            fs::remove_dir_all(&app_dir)
                .await
                .with_context(|| format!("Failed to remove app data directory: {:?}", app_dir))?;
        }
        Ok(())
    }

    /// Preserve app data (no-op for cleanup, used when removing container)
    pub async fn preserve_app_data(&self, _app_id: &str) -> Result<()> {
        // In dev mode, we might want to preserve data between container removals
        // This is a no-op by default, but can be extended
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_map_volume_path() {
        let temp_dir = std::env::temp_dir().join("test-archipelago");
        let manager = DevDataManager::new(temp_dir.clone());

        let dev_path = manager.map_volume_path("bitcoin-core", "/var/lib/archipelago/bitcoin");
        assert!(dev_path.to_string_lossy().contains("bitcoin-core"));

        // Cleanup
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_create_app_data_dir() {
        let temp_dir = std::env::temp_dir().join("test-archipelago-2");
        let manager = DevDataManager::new(temp_dir.clone());

        let app_dir = manager.create_app_data_dir("test-app").await.unwrap();
        assert!(app_dir.exists());

        // Cleanup
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }
}
