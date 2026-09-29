//! filebrowser config bootstrap helper.
//!
//! Mirrors the legacy first-boot behavior that writes
//! `/var/lib/archipelago/filebrowser-data/.filebrowser.json` before
//! starting the container with `--config /data/.filebrowser.json`.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
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

/// Save `bytes` into FileBrowser's storage as a new file in `dir`, named
/// `name` or, if that's taken, `name (2)`, `name (3)`… Never overwrites.
/// Returns the path written.
///
/// FileBrowser's folders belong to its rootless container range (host uid
/// 100000, mode 755), so this service — host uid 1000, outside that range —
/// can read them but not write into them, and filing a purchase into Files
/// failed with EACCES (2026-09-29). When a direct write is refused, the file
/// is written through `podman unshare`, where that range is ours, and given
/// the folder's owner so FileBrowser manages it like its own uploads.
pub async fn save_new_file(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    save_new_file_with(dir, name, bytes, write_via_userns).await
}

async fn save_new_file_with<F, Fut>(
    dir: &Path,
    name: &str,
    bytes: &[u8],
    fallback: F,
) -> Result<PathBuf>
where
    F: FnOnce(PathBuf, Vec<u8>) -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let target = unused_name(dir, name);
    match write_direct(dir, &target, bytes).await {
        Ok(()) => Ok(target),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            fallback(target.clone(), bytes.to_vec())
                .await
                .with_context(|| format!("writing {} via podman unshare", target.display()))?;
            Ok(target)
        }
        Err(e) => Err(e).with_context(|| format!("writing {}", target.display())),
    }
}

/// `dir/name`, or the first free `dir/stem (n).ext` from n = 2.
fn unused_name(dir: &Path, name: &str) -> PathBuf {
    let mut target = dir.join(name);
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    let mut n = 2;
    while target.exists() {
        target = dir.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    target
}

async fn write_direct(dir: &Path, target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    fs::create_dir_all(dir).await?;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .await?;
    let written = async {
        f.write_all(bytes).await?;
        f.flush().await
    }
    .await;
    if written.is_err() {
        let _ = fs::remove_file(target).await;
    }
    written
}

/// Write `bytes` (piped on stdin) to `target` from inside the rootless user
/// namespace. It goes to a temp file first and is hard-linked into place, so
/// FileBrowser never sees a partial file and an existing file is never
/// replaced (`ln` refuses an existing name).
async fn write_via_userns(target: PathBuf, bytes: Vec<u8>) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    const SCRIPT: &str = r#"set -eu
dst=$1
dir=$(dirname -- "$dst")
if [ ! -d "$dir" ]; then
    mkdir -- "$dir"
    chown --reference="$(dirname -- "$dir")" -- "$dir"
fi
tmp="$dir/.archy-saving.$$"
trap 'rm -f -- "$tmp"' EXIT
cat > "$tmp"
chown --reference="$dir" -- "$tmp"
chmod 0644 -- "$tmp"
ln -- "$tmp" "$dst"
"#;
    let mut child = tokio::process::Command::new("podman")
        .args(["unshare", "sh", "-c", SCRIPT, "sh"])
        .arg(&target)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("Failed to run podman unshare")?;
    let mut stdin = child.stdin.take().context("podman unshare stdin")?;
    let fed = stdin.write_all(&bytes).await;
    drop(stdin);
    let out = child
        .wait_with_output()
        .await
        .context("Failed to wait for podman unshare")?;
    if !out.status.success() {
        anyhow::bail!(
            "podman unshare exited with {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    fed.context("Failed to pipe the file to podman unshare")?;
    Ok(())
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

    #[test]
    fn unused_name_numbers_duplicates_and_keeps_the_extension() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        assert_eq!(unused_name(d, "song.mp3"), d.join("song.mp3"));
        std::fs::write(d.join("song.mp3"), b"").unwrap();
        assert_eq!(unused_name(d, "song.mp3"), d.join("song (2).mp3"));
        std::fs::write(d.join("song (2).mp3"), b"").unwrap();
        assert_eq!(unused_name(d, "song.mp3"), d.join("song (3).mp3"));
        std::fs::write(d.join("README"), b"").unwrap();
        assert_eq!(unused_name(d, "README"), d.join("README (2)"));
        std::fs::write(d.join(".hidden"), b"").unwrap();
        assert_eq!(unused_name(d, ".hidden"), d.join(".hidden (2)"));
    }

    #[tokio::test]
    async fn save_new_file_writes_directly_into_a_writable_folder() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("Music");
        let path = save_new_file_with(&music, "a.mp3", b"abc", |_, _| async {
            anyhow::bail!("fallback must not run")
        })
        .await
        .unwrap();
        assert_eq!(path, music.join("a.mp3"));
        assert_eq!(std::fs::read(&path).unwrap(), b"abc");
    }

    #[tokio::test]
    async fn save_new_file_never_overwrites_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.mp3"), b"original").unwrap();
        let path = save_new_file_with(dir.path(), "a.mp3", b"new", |_, _| async {
            anyhow::bail!("fallback must not run")
        })
        .await
        .unwrap();
        assert_eq!(path, dir.path().join("a (2).mp3"));
        assert_eq!(
            std::fs::read(dir.path().join("a.mp3")).unwrap(),
            b"original"
        );
    }

    /// Regression (2026-09-29): filing a purchase into a FileBrowser folder
    /// owned by the container's uid range failed with EACCES. A refused
    /// write must go through the user-namespace fallback, with the same
    /// target and bytes.
    #[tokio::test]
    async fn a_refused_write_goes_through_the_userns_fallback() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("Music");
        std::fs::create_dir(&music).unwrap();
        std::fs::set_permissions(&music, std::fs::Permissions::from_mode(0o555)).unwrap();
        if std::fs::File::create(music.join("probe")).is_ok() {
            return; // running as root: mode bits don't refuse the write
        }

        let seen = std::sync::Mutex::new(None);
        let path = save_new_file_with(&music, "a.mp3", b"abc", |target, bytes| {
            *seen.lock().unwrap() = Some((target, bytes));
            async { Ok(()) }
        })
        .await
        .unwrap();
        assert_eq!(path, music.join("a.mp3"));
        assert_eq!(
            seen.into_inner().unwrap(),
            Some((music.join("a.mp3"), b"abc".to_vec()))
        );
        std::fs::set_permissions(&music, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[tokio::test]
    async fn a_failed_fallback_is_reported() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        if std::fs::File::create(dir.path().join("probe")).is_ok() {
            return;
        }
        let err = save_new_file_with(dir.path(), "a.mp3", b"abc", |_, _| async {
            anyhow::bail!("no podman")
        })
        .await
        .unwrap_err();
        assert!(format!("{err:#}").contains("no podman"));
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
