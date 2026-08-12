//! filebrowser config bootstrap helper.
//!
//! Mirrors the legacy first-boot behavior that writes
//! `/var/lib/archipelago/filebrowser-data/.filebrowser.json` before
//! starting the container with `--config /data/.filebrowser.json`.

use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::fs;

use crate::update::host_sudo;

pub const DEFAULT_SRV_ROOT: &str = "/var/lib/archipelago/filebrowser";
pub const DEFAULT_DATA_DIR: &str = "/var/lib/archipelago/filebrowser-data";
pub const DEFAULT_CONFIG_PATH: &str = "/var/lib/archipelago/filebrowser-data/.filebrowser.json";

const DEFAULT_CONFIG_JSON: &str =
    "{\"port\":80,\"baseURL\":\"\",\"address\":\"0.0.0.0\",\"database\":\"/data/filebrowser.db\",\"root\":\"/srv\",\"log\":\"stdout\"}\n";

#[derive(Debug, Clone)]
pub struct EnsurePaths {
    pub srv_root: PathBuf,
    pub data_dir: PathBuf,
    pub config_path: PathBuf,
}

impl Default for EnsurePaths {
    fn default() -> Self {
        Self {
            srv_root: PathBuf::from(DEFAULT_SRV_ROOT),
            data_dir: PathBuf::from(DEFAULT_DATA_DIR),
            config_path: PathBuf::from(DEFAULT_CONFIG_PATH),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnsureOutcome {
    Written,
    Unchanged,
}

pub async fn ensure_config(paths: &EnsurePaths) -> Result<EnsureOutcome> {
    create_dir_all_or_sudo(&paths.srv_root).await?;
    create_dir_all_or_sudo(&paths.data_dir).await?;

    for d in ["Documents", "Photos", "Music", "Downloads", "Builds"] {
        create_dir_all_or_sudo(&paths.srv_root.join(d)).await?;
    }

    if paths.config_path.exists() {
        return Ok(EnsureOutcome::Unchanged);
    }

    let parent = paths
        .config_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("config_path has no parent directory"))?;
    create_dir_all_or_sudo(parent).await?;

    write_config_atomically(paths).await?;

    Ok(EnsureOutcome::Written)
}

async fn create_dir_all_or_sudo(path: &std::path::Path) -> Result<()> {
    match fs::create_dir_all(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let path = path.to_string_lossy();
            let status = host_sudo(&["mkdir", "-p", &path])
                .await
                .with_context(|| format!("creating {path} via sudo"))?;
            if !status.success() {
                anyhow::bail!("mkdir -p {path} via sudo exited with {status}");
            }
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("creating {}", path.display())),
    }
}

async fn write_config_atomically(paths: &EnsurePaths) -> Result<()> {
    let tmp = paths.config_path.with_extension("tmp");
    match fs::write(&tmp, DEFAULT_CONFIG_JSON).await {
        Ok(()) => {
            fs::rename(&tmp, &paths.config_path)
                .await
                .with_context(|| {
                    format!(
                        "renaming {} -> {}",
                        tmp.display(),
                        paths.config_path.display()
                    )
                })?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let script = format!(
                "set -eu\ncat > '{}' <<'FILEBROWSERCONF'\n{}FILEBROWSERCONF\n",
                shell_quote(&paths.config_path.to_string_lossy()),
                DEFAULT_CONFIG_JSON
            );
            let status = host_sudo(&["sh", "-lc", &script])
                .await
                .context("writing .filebrowser.json via sudo")?;
            if !status.success() {
                anyhow::bail!("writing .filebrowser.json via sudo exited with {status}");
            }
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("writing tmp {}", tmp.display())),
    }
}

fn shell_quote(s: &str) -> String {
    s.replace('\'', "'\\''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ensure_config_creates_dirs_and_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let paths = EnsurePaths {
            srv_root: tmp.path().join("filebrowser"),
            data_dir: tmp.path().join("filebrowser-data"),
            config_path: tmp.path().join("filebrowser-data/.filebrowser.json"),
        };

        let out = ensure_config(&paths).await.unwrap();
        assert_eq!(out, EnsureOutcome::Written);
        assert!(paths.config_path.exists());
        assert!(paths.srv_root.join("Documents").exists());
        assert!(paths.srv_root.join("Photos").exists());
    }

    #[tokio::test]
    async fn ensure_config_is_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let paths = EnsurePaths {
            srv_root: tmp.path().join("filebrowser"),
            data_dir: tmp.path().join("filebrowser-data"),
            config_path: tmp.path().join("filebrowser-data/.filebrowser.json"),
        };

        let first = ensure_config(&paths).await.unwrap();
        assert_eq!(first, EnsureOutcome::Written);
        let second = ensure_config(&paths).await.unwrap();
        assert_eq!(second, EnsureOutcome::Unchanged);
    }
}
