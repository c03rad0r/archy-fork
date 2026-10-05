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

/// One atomically published record shared by setup, Cloud and credentials UI.
/// Deliberately has no Debug implementation: the password must never be logged.
#[derive(serde::Deserialize)]
pub struct CloudCredentials {
    pub schema: u32,
    pub username: String,
    pub password: String,
}

pub async fn cloud_credentials(directory: &Path) -> Result<CloudCredentials> {
    let path = directory.join("credentials.json");
    match fs::read(&path).await {
        Ok(bytes) => {
            let value: CloudCredentials = serde_json::from_slice(&bytes)
                .context("Invalid private File Browser credential record")?;
            let suffix = value.username.strip_prefix("archy-").unwrap_or("");
            anyhow::ensure!(
                value.schema == 1
                    && suffix.len() == 32
                    && suffix.bytes().all(|c| c.is_ascii_hexdigit())
                    && value.password.len() == 64
                    && value.password.bytes().all(|c| c.is_ascii_hexdigit()),
                "Invalid managed File Browser credentials"
            );
            Ok(value)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Compatibility during staged upgrades only. Never invent admin/admin
            // when a secret is missing; the pre-start provisioner repairs legacy DBs.
            let password = fs::read_to_string(directory.join("password"))
                .await
                .context("File Browser secure Cloud login has not been provisioned")?;
            let password = password.trim().to_owned();
            anyhow::ensure!(
                !password.is_empty() && password != "admin",
                "File Browser default credentials must be migrated before Cloud login"
            );
            Ok(CloudCredentials {
                schema: 0,
                username: "admin".into(),
                password,
            })
        }
        Err(error) => Err(error).context("Cannot read private File Browser credentials"),
    }
}

/// Prepare a stopped server using the same helper used by Quadlet and the ISO.
pub async fn prepare_credentials(
    paths: &EnsurePaths,
    secret_dir: &Path,
    image: &str,
    runtime: &str,
) -> Result<()> {
    let output = tokio::process::Command::new("python3")
        .args([
            "-c",
            include_str!("../../../../scripts/filebrowser-credentials.py"),
            "--image",
            image,
            "--runtime",
            runtime,
            "--data-dir",
            &paths.data_dir.to_string_lossy(),
            "--srv-root",
            &paths.srv_root.to_string_lossy(),
            "--secrets-dir",
            &secret_dir.to_string_lossy(),
        ])
        .kill_on_drop(true)
        .output()
        .await
        .context("Running File Browser credential setup")?;
    anyhow::ensure!(output.status.success(),
        "File Browser secure login setup failed; existing state and private rollback backup retained");
    Ok(())
}

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
    let legacy = paths.data_dir.join("database.db").exists();
    let canonical = paths.data_dir.join("filebrowser.db").exists();
    anyhow::ensure!(
        !(legacy && canonical),
        "Multiple File Browser databases need explicit config selection"
    );
    let config = if legacy {
        DEFAULT_CONFIG_JSON.replace("/data/filebrowser.db", "/data/database.db")
    } else {
        DEFAULT_CONFIG_JSON.to_string()
    };
    match fs::write(&tmp, &config).await {
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
                config
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

/// Save a complete purchase without overwriting any existing directory entry.
/// Both host and rootless-namespace paths publish with a no-clobber hard link.
pub async fn save_new_file(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    save_new_file_with(dir, name, bytes, write_via_userns).await
}

fn validate_filename(name: &str) -> Result<()> {
    anyhow::ensure!(
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains(['/', '\\', '\0'])
            && name.len() <= 255,
        "Invalid purchased filename"
    );
    Ok(())
}

async fn save_new_file_with<F, Fut>(
    dir: &Path,
    name: &str,
    bytes: &[u8],
    fallback: F,
) -> Result<PathBuf>
where
    F: FnOnce(PathBuf, String, Vec<u8>) -> Fut,
    Fut: std::future::Future<Output = Result<PathBuf>>,
{
    validate_filename(name)?;
    // Never follow a user-created destination directory symlink.
    match fs::symlink_metadata(dir).await {
        Ok(meta) => anyhow::ensure!(meta.is_dir(), "Files destination is not a directory"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    save_after_direct_result(
        write_direct(dir, name, bytes).await,
        dir,
        name,
        bytes,
        fallback,
    )
    .await
}

async fn save_after_direct_result<F, Fut>(
    result: std::io::Result<PathBuf>,
    dir: &Path,
    name: &str,
    bytes: &[u8],
    fallback: F,
) -> Result<PathBuf>
where
    F: FnOnce(PathBuf, String, Vec<u8>) -> Fut,
    Fut: std::future::Future<Output = Result<PathBuf>>,
{
    match result {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            fallback(dir.to_owned(), name.to_owned(), bytes.to_vec())
                .await
                .context("Saving purchase in Files user namespace")
        }
        Err(error) => Err(error).context("Saving purchase in Files"),
    }
}

fn numbered_name(name: &str, attempt: usize) -> String {
    if attempt == 1 {
        return name.to_owned();
    }
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => format!("{stem} ({attempt}).{extension}"),
        _ => format!("{name} ({attempt})"),
    }
}

struct PendingFile(PathBuf);
impl Drop for PendingFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn write_direct(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncWriteExt;
    fs::create_dir_all(dir).await?;
    let temp_path = dir.join(format!(".archy-saving-{}", uuid::Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp_path)
        .await?;
    let temp = PendingFile(temp_path);
    file.write_all(bytes).await?;
    file.set_permissions(std::fs::Permissions::from_mode(0o644))
        .await?;
    file.sync_all().await?;
    for attempt in 1..=100 {
        let target = dir.join(numbered_name(name, attempt));
        match fs::hard_link(&temp.0, &target).await {
            Ok(()) => return Ok(target),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "Too many existing copies; purchase cache retained",
    ))
}

// Positional arguments carry all user-controlled text. mktemp prevents temp-name
// collisions; ln -T refuses files, symlinks and directories, including races.
const WRITE_VIA_USERNS: &str = r#"set -eu
dir=$1
name=$2
expected=$3
[ ! -L "$dir" ] || exit 1
if [ ! -d "$dir" ]; then
    mkdir -p -- "$dir"
    chown --reference="$(dirname -- "$dir")" -- "$dir"
fi
tmp=$(mktemp "$dir/.archy-saving.XXXXXXXXXX")
trap 'rm -f -- "$tmp"' EXIT HUP INT TERM
cat > "$tmp"
[ "$(wc -c < "$tmp")" -eq "$expected" ] || exit 1
chown --reference="$dir" -- "$tmp"
chmod 0644 -- "$tmp"
sync -f -- "$tmp"
stem=$name
ext=
case "$name" in
    *.*) prefix=${name%.*}; if [ -n "$prefix" ]; then stem=$prefix; ext=.${name##*.}; fi ;;
esac
n=1
while [ "$n" -le 100 ]; do
    candidate=$name
    if [ "$n" -gt 1 ]; then candidate="$stem ($n)$ext"; fi
    dst="$dir/$candidate"
    if ln -T -- "$tmp" "$dst" 2>/dev/null; then
        printf '%s' "$candidate"
        exit 0
    fi
    # A conflict may be a dangling symlink; never follow it or overwrite it.
    if [ ! -e "$dst" ] && [ ! -L "$dst" ]; then exit 1; fi
    n=$((n + 1))
done
exit 1
"#;

async fn write_via_userns(dir: PathBuf, name: String, bytes: Vec<u8>) -> Result<PathBuf> {
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new("podman")
        .args(["unshare", "sh", "-c", WRITE_VIA_USERNS, "sh"])
        .arg(&dir)
        .arg(&name)
        .arg(bytes.len().to_string())
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("Starting Files namespace writer")?;
    let mut stdin = child.stdin.take().context("Files writer stdin missing")?;
    let operation = async {
        let fed = stdin.write_all(&bytes).await;
        drop(stdin);
        let output = child.wait_with_output().await?;
        anyhow::ensure!(
            output.status.success(),
            "Files namespace writer failed: {}",
            output.status
        );
        fed.context("Sending purchase bytes to Files")?;
        let chosen =
            String::from_utf8(output.stdout).context("Files writer returned an invalid name")?;
        validate_filename(&chosen)?;
        anyhow::ensure!(
            (1..=100).any(|n| numbered_name(&name, n) == chosen),
            "Files writer returned an unexpected name"
        );
        Ok(dir.join(chosen))
    };
    tokio::time::timeout(std::time::Duration::from_secs(120), operation)
        .await
        .context("Files namespace writer timed out")?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cloud_credentials_use_unique_record_and_never_default_password() {
        let dir = tempfile::tempdir().unwrap();
        assert!(cloud_credentials(dir.path()).await.is_err());
        fs::write(dir.path().join("password"), "admin")
            .await
            .unwrap();
        assert!(cloud_credentials(dir.path()).await.is_err());
        fs::write(dir.path().join("password"), "legacy-unique-password")
            .await
            .unwrap();
        assert_eq!(cloud_credentials(dir.path()).await.unwrap().schema, 0);
        let value = serde_json::json!({"schema":1,"username":format!("archy-{}", "a".repeat(32)),"password":"b".repeat(64)});
        fs::write(dir.path().join("credentials.json"), value.to_string())
            .await
            .unwrap();
        let loaded = cloud_credentials(dir.path()).await.unwrap();
        assert_eq!(loaded.username, value["username"].as_str().unwrap());
        assert_eq!(loaded.password, value["password"].as_str().unwrap());
        fs::write(dir.path().join("credentials.json"), "{}")
            .await
            .unwrap();
        assert!(
            cloud_credentials(dir.path()).await.is_err(),
            "damaged managed record must not fall back to old credentials"
        );
    }

    #[tokio::test]
    async fn missing_config_preserves_legacy_database_and_refuses_ambiguity() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = EnsurePaths {
            srv_root: tmp.path().join("srv"),
            data_dir: tmp.path().join("data"),
            config_path: tmp.path().join("data/.filebrowser.json"),
        };
        fs::create_dir_all(&paths.data_dir).await.unwrap();
        fs::write(paths.data_dir.join("database.db"), b"legacy fixture")
            .await
            .unwrap();
        ensure_config(&paths).await.unwrap();
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(&paths.config_path).await.unwrap()).unwrap();
        assert_eq!(config["database"], "/data/database.db");
        fs::remove_file(&paths.config_path).await.unwrap();
        fs::write(paths.data_dir.join("filebrowser.db"), b"other fixture")
            .await
            .unwrap();
        assert!(ensure_config(&paths).await.is_err());
        assert!(!paths.config_path.exists());
        assert_eq!(
            fs::read(paths.data_dir.join("database.db")).await.unwrap(),
            b"legacy fixture"
        );
    }

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

#[cfg(test)]
mod purchase_write_tests {
    use super::*;
    use std::{
        collections::HashSet,
        os::unix::fs::{symlink, PermissionsExt},
    };

    fn no_temps(dir: &Path) {
        assert!(std::fs::read_dir(dir).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".archy-saving")));
    }

    #[tokio::test]
    async fn direct_write_uses_complete_bytes_and_preserves_originals() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("song.mp3"), b"original")
            .await
            .unwrap();
        let target = save_new_file(dir.path(), "song.mp3", b"new").await.unwrap();
        assert_eq!(target.file_name().unwrap(), "song (2).mp3");
        assert_eq!(fs::read(target).await.unwrap(), b"new");
        assert_eq!(
            fs::read(dir.path().join("song.mp3")).await.unwrap(),
            b"original"
        );
        no_temps(dir.path());
    }

    #[tokio::test]
    async fn simultaneous_saves_publish_unique_complete_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut tasks = Vec::new();
        for n in 0..24u8 {
            let dir = dir.path().to_owned();
            tasks.push(tokio::spawn(async move {
                let bytes = vec![n; 32768];
                let path = save_new_file(&dir, "same.bin", &bytes).await.unwrap();
                assert_eq!(fs::read(&path).await.unwrap(), bytes);
                path
            }));
        }
        let mut paths = HashSet::new();
        for task in tasks {
            assert!(paths.insert(task.await.unwrap()));
        }
        assert_eq!(paths.len(), 24);
        no_temps(dir.path());
    }

    #[tokio::test]
    async fn existing_directories_and_dangling_symlinks_are_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("name")).await.unwrap();
        symlink("missing", dir.path().join("name (2)")).unwrap();
        let path = save_new_file(dir.path(), "name", b"new").await.unwrap();
        assert_eq!(path.file_name().unwrap(), "name (3)");
        assert!(dir.path().join("name").is_dir());
        assert!(fs::symlink_metadata(dir.path().join("name (2)"))
            .await
            .unwrap()
            .is_symlink());
        no_temps(dir.path());
    }

    #[tokio::test]
    async fn invalid_names_and_symlink_destination_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "",
            ".",
            "..",
            "../escape",
            "/absolute",
            "a/b",
            "a\\b",
            "a\0b",
        ] {
            assert!(save_new_file(dir.path(), name, b"bytes").await.is_err());
        }
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), dir.path().join("Music")).unwrap();
        assert!(save_new_file(&dir.path().join("Music"), "song", b"bytes")
            .await
            .is_err());
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn collision_limit_preserves_all_files_and_cleans_temporary_data() {
        let dir = tempfile::tempdir().unwrap();
        for n in 1..=100 {
            fs::write(dir.path().join(numbered_name("a.txt", n)), b"keep")
                .await
                .unwrap();
        }
        assert!(save_new_file(dir.path(), "a.txt", b"new").await.is_err());
        for n in 1..=100 {
            assert_eq!(
                fs::read(dir.path().join(numbered_name("a.txt", n)))
                    .await
                    .unwrap(),
                b"keep"
            );
        }
        no_temps(dir.path());
    }

    #[tokio::test]
    async fn permission_fallback_is_exercised_without_skipping_as_root() {
        let dir = tempfile::tempdir().unwrap();
        let result = save_after_direct_result(
            Err(std::io::ErrorKind::PermissionDenied.into()),
            dir.path(),
            "a",
            b"abc",
            |dir, name, bytes| async move {
                assert_eq!(bytes, b"abc");
                Ok(dir.join(name))
            },
        )
        .await
        .unwrap();
        assert_eq!(result, dir.path().join("a"));
        assert!(save_after_direct_result(
            Err(std::io::ErrorKind::PermissionDenied.into()),
            dir.path(),
            "a",
            b"abc",
            |_, _, _| async { anyhow::bail!("namespace unavailable") }
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("namespace"));
        assert!(save_after_direct_result(
            Err(std::io::ErrorKind::StorageFull.into()),
            dir.path(),
            "a",
            b"abc",
            |_, _, _| async { panic!("disk full must not trigger permission fallback") }
        )
        .await
        .is_err());
    }

    async fn run_script(
        dir: &Path,
        name: &str,
        bytes: &[u8],
        expected: usize,
    ) -> std::process::Output {
        use tokio::io::AsyncWriteExt;
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", WRITE_VIA_USERNS, "sh"])
            .arg(dir)
            .arg(name)
            .arg(expected.to_string())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        input.write_all(bytes).await.unwrap();
        drop(input);
        child.wait_with_output().await.unwrap()
    }

    #[tokio::test]
    async fn namespace_script_preserves_names_bytes_modes_and_existing_entries() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Music");
        let name = "song ' $() ; #.mp3";
        for n in 1..=2 {
            let output = run_script(&folder, name, b"abc", 3).await;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let chosen = String::from_utf8(output.stdout).unwrap();
            assert_eq!(chosen, numbered_name(name, n));
            let path = folder.join(chosen);
            assert_eq!(fs::read(&path).await.unwrap(), b"abc");
            assert_eq!(
                fs::metadata(path).await.unwrap().permissions().mode() & 0o777,
                0o644
            );
        }
        no_temps(&folder);
    }

    #[tokio::test]
    async fn namespace_script_refuses_truncated_input_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let output = run_script(dir.path(), "never.bin", b"partial", 100).await;
        assert!(!output.status.success());
        assert!(!dir.path().join("never.bin").exists());
        no_temps(dir.path());
    }

    #[tokio::test]
    async fn namespace_script_does_not_link_inside_existing_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("name")).await.unwrap();
        symlink("missing", dir.path().join("name (2)")).unwrap();
        let output = run_script(dir.path(), "name", b"abc", 3).await;
        assert!(output.status.success());
        assert_eq!(output.stdout, b"name (3)");
        assert_eq!(
            std::fs::read_dir(dir.path().join("name")).unwrap().count(),
            0
        );
        no_temps(dir.path());
    }

    #[test]
    fn names_keep_extensions_and_dotfiles() {
        assert_eq!(numbered_name("a.tar.gz", 2), "a.tar (2).gz");
        assert_eq!(numbered_name(".hidden", 2), ".hidden (2)");
        assert_eq!(numbered_name("README", 2), "README (2)");
    }
}
