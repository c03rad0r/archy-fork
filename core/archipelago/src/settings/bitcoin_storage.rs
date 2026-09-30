//! Install-time pruning preference, shared by Bitcoin Core and Knots.
//! Missing preference preserves the existing disk-based automatic selection.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Default, Serialize, Deserialize)]
pub struct BitcoinStorage {
    pub prune: bool,
}

pub async fn load(data_dir: &Path) -> Result<BitcoinStorage> {
    match tokio::fs::read(data_dir.join("settings/bitcoin-storage.json")).await {
        Ok(bytes) => serde_json::from_slice(&bytes).context("Invalid Bitcoin storage settings"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BitcoinStorage::default()),
        Err(e) => Err(e.into()),
    }
}

pub async fn save(data_dir: &Path, prune: bool) -> Result<()> {
    let dir = data_dir.join("settings");
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join("bitcoin-storage.json");
    let temporary = dir.join("bitcoin-storage.json.tmp");
    tokio::fs::write(&temporary, serde_json::to_vec(&BitcoinStorage { prune })?).await?;
    tokio::fs::rename(temporary, path).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn missing_setting_keeps_auto_and_explicit_pruning_survives_reload() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!load(dir.path()).await.unwrap().prune);
        save(dir.path(), true).await.unwrap();
        assert!(load(dir.path()).await.unwrap().prune);
        save(dir.path(), false).await.unwrap();
        assert!(!load(dir.path()).await.unwrap().prune);
    }
    #[tokio::test]
    async fn corrupt_setting_is_not_silently_changed_to_archival() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), true).await.unwrap();
        tokio::fs::write(dir.path().join("settings/bitcoin-storage.json"), "broken")
            .await
            .unwrap();
        assert!(load(dir.path()).await.is_err());
    }
}
