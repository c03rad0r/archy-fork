//! Serialize complete wallet mutations, independently for each node data directory.
//! This prevents lost updates; durable recovery of interrupted remote operations
//! requires the separate operation journal.
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as RegistryMutex, OnceLock, Weak};
use tokio::sync::{Mutex, OwnedMutexGuard};

type Registry = HashMap<PathBuf, Weak<Mutex<()>>>;
static LOCKS: OnceLock<RegistryMutex<Registry>> = OnceLock::new();

async fn canonical_lock(data_dir: &Path) -> Result<(PathBuf, Arc<Mutex<()>>)> {
    tokio::fs::create_dir_all(data_dir)
        .await
        .context("Could not prepare wallet data directory")?;
    let canonical = tokio::fs::canonicalize(data_dir)
        .await
        .context("Could not resolve wallet data directory")?;
    let mut registry = LOCKS
        .get_or_init(|| RegistryMutex::new(HashMap::new()))
        .lock()
        .map_err(|_| anyhow::anyhow!("Wallet mutation lock registry is unavailable"))?;
    registry.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = registry.get(&canonical).and_then(Weak::upgrade) {
        return Ok((canonical, lock));
    }
    let lock = Arc::new(Mutex::new(()));
    registry.insert(canonical.clone(), Arc::downgrade(&lock));
    Ok((canonical, lock))
}

pub(super) struct WalletMutation {
    pub(super) data_dir: PathBuf,
    _held: OwnedMutexGuard<()>,
}

pub(super) async fn guard(data_dir: &Path) -> Result<WalletMutation> {
    let (data_dir, lock) = canonical_lock(data_dir).await?;
    Ok(WalletMutation {
        data_dir,
        _held: lock.lock_owned().await,
    })
}

#[cfg(test)]
async fn lock_for(data_dir: &Path) -> Result<Arc<Mutex<()>>> {
    Ok(canonical_lock(data_dir).await?.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn aliases_share_a_lock_but_different_nodes_do_not() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let held = guard(first.path()).await.unwrap();
        let alias = lock_for(&first.path().join(".")).await.unwrap();
        assert!(alias.try_lock().is_err());
        let independent = lock_for(second.path()).await.unwrap();
        assert!(independent.try_lock().is_ok());
        drop(held);
        assert!(alias.try_lock().is_ok());
    }

    #[tokio::test]
    async fn symlink_aliases_cannot_bypass_serialization() {
        let parent = tempfile::tempdir().unwrap();
        let actual = parent.path().join("node");
        std::fs::create_dir(&actual).unwrap();
        let alias = parent.path().join("alias");
        std::os::unix::fs::symlink(&actual, &alias).unwrap();
        let _held = guard(&actual).await.unwrap();
        assert!(lock_for(&alias).await.unwrap().try_lock().is_err());
    }

    #[tokio::test]
    async fn invalid_directory_fails_without_replacing_existing_data() {
        let parent = tempfile::tempdir().unwrap();
        let file = parent.path().join("node");
        std::fs::write(&file, b"preserve").unwrap();
        assert!(guard(&file).await.is_err());
        assert_eq!(std::fs::read(file).unwrap(), b"preserve");
    }
}
