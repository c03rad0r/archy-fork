//! Full system backup — identity keys + app data + settings + DWN messages.
//!
//! Creates an encrypted tar.gz archive containing all critical node data.
//! Encryption: Argon2 key derivation + ChaCha20-Poly1305.

use anyhow::{Context, Result};
use argon2::Argon2;
use chacha20poly1305::aead::{Aead, KeyInit};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::{Archive, Builder};
use tokio::fs;
use tracing::{debug, info};

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// Directories within data_dir to include in a full backup.
const BACKUP_DIRS: &[&str] = &[
    "identity",
    "identities",
    "dwn",
    "credentials",
    "tor-config",
    "content",
    // Per-node service secrets. Without these a restored node can't
    // decrypt identity/lnd_aezeed.enc (the Lightning seed backup is
    // encrypted with secrets/lnd-wallet-password) or reuse its service
    // credentials. The archive itself is passphrase-encrypted.
    "secrets",
];

/// Files within data_dir to include in a full backup.
const BACKUP_FILES: &[&str] = &[
    "user.json",
    "peers.json",
    "names.json",
    "onboarding.json",
    "nostr_relays.json",
    "network_visibility",
    "port_allocations.json",
    "update_state.json",
];

/// Backup metadata stored alongside the archive.
#[derive(Debug, Serialize, Deserialize)]
pub struct BackupMetadata {
    pub id: String,
    pub version: u32,
    pub created_at: String,
    pub encrypted: bool,
    pub size_bytes: u64,
    pub description: Option<String>,
}

/// Result of a backup verification check.
#[derive(Debug, Serialize, Deserialize)]
pub struct VerifyResult {
    pub valid: bool,
    pub id: String,
    pub created_at: String,
    pub size_bytes: u64,
    pub error: Option<String>,
}

/// Create a full encrypted backup of all node data.
/// Returns the backup metadata and the path to the backup file.
pub async fn create_full_backup(
    data_dir: &Path,
    passphrase: &str,
    description: Option<&str>,
) -> Result<BackupMetadata> {
    let backups_dir = data_dir.join("backups");
    fs::create_dir_all(&backups_dir)
        .await
        .context("Failed to create backups dir")?;

    let backup_id = uuid::Uuid::new_v4().to_string();
    let timestamp = chrono::Utc::now().to_rfc3339();

    // Step 1: Create tar.gz archive in memory
    let tar_gz_data = tokio::task::spawn_blocking({
        let data_dir = data_dir.to_path_buf();
        move || create_tar_gz(&data_dir)
    })
    .await?
    .context("Failed to create tar archive")?;

    info!(size = tar_gz_data.len(), "Backup archive created");

    // Step 2: Encrypt the archive
    let encrypted = encrypt_data(&tar_gz_data, passphrase)?;

    // Step 3: Write to disk
    let backup_path = backups_dir.join(format!("{}.bak", backup_id));
    fs::write(&backup_path, &encrypted)
        .await
        .context("Failed to write backup file")?;

    // Step 4: Write metadata
    let metadata = BackupMetadata {
        id: backup_id,
        // v3: archive additionally carries the `secrets` dir (needed to
        // decrypt identity/lnd_aezeed.enc on a restored node).
        version: 3,
        created_at: timestamp,
        encrypted: true,
        size_bytes: encrypted.len() as u64,
        description: description.map(|s| s.to_string()),
    };

    let meta_path = backups_dir.join(format!("{}.meta.json", metadata.id));
    let meta_json =
        serde_json::to_string_pretty(&metadata).context("Failed to serialize metadata")?;
    fs::write(&meta_path, meta_json)
        .await
        .context("Failed to write metadata")?;

    info!(id = %metadata.id, size = metadata.size_bytes, "Full backup created");
    Ok(metadata)
}

/// Restore a full backup from an encrypted archive.
///
/// Uses atomic staging: extracts to a temporary directory first, validates,
/// then swaps into place with rollback on failure.
pub async fn restore_full_backup(data_dir: &Path, backup_id: &str, passphrase: &str) -> Result<()> {
    let backup_path = data_dir.join("backups").join(format!("{}.bak", backup_id));
    if !backup_path.exists() {
        anyhow::bail!("Backup not found: {}", backup_id);
    }

    let encrypted = fs::read(&backup_path)
        .await
        .context("Failed to read backup file")?;

    // Check disk space: need at least 2x backup size free
    let backup_size = encrypted.len() as u64;
    if let Ok(output) = tokio::process::Command::new("df")
        .args(["--output=avail", "-B1"])
        .arg(data_dir)
        .output()
        .await
    {
        if let Ok(stdout) = String::from_utf8(output.stdout) {
            if let Some(avail) = stdout
                .lines()
                .nth(1)
                .and_then(|l| l.trim().parse::<u64>().ok())
            {
                if avail < backup_size * 2 {
                    anyhow::bail!(
                        "Insufficient disk space for restore: need {}MB, have {}MB",
                        backup_size * 2 / (1024 * 1024),
                        avail / (1024 * 1024),
                    );
                }
            }
        }
    }

    let tar_gz_data = decrypt_data(&encrypted, passphrase)?;

    let staging_dir = data_dir.join(".restore-staging");
    let rollback_dir = data_dir.join(".restore-backup");

    // Clean up any previous failed restore
    let _ = fs::remove_dir_all(&staging_dir).await;
    let _ = fs::remove_dir_all(&rollback_dir).await;

    // Extract to staging directory
    fs::create_dir_all(&staging_dir)
        .await
        .context("Failed to create staging directory")?;

    let staging_clone = staging_dir.clone();
    if let Err(e) =
        tokio::task::spawn_blocking(move || extract_tar_gz(&staging_clone, &tar_gz_data)).await?
    {
        let _ = fs::remove_dir_all(&staging_dir).await;
        return Err(e).context("Failed to extract backup to staging");
    }

    // Validate staging has required files
    let has_identity = staging_dir.join("identity").exists();
    if !has_identity {
        let _ = fs::remove_dir_all(&staging_dir).await;
        anyhow::bail!("Invalid backup: missing identity directory");
    }

    // Move current data to rollback directory
    fs::create_dir_all(&rollback_dir)
        .await
        .context("Failed to create rollback directory")?;

    for dir_name in BACKUP_DIRS {
        // Only displace live data the backup will actually replace —
        // older archives don't contain every current BACKUP_DIRS entry
        // (e.g. `secrets` was added later), and moving a live dir to
        // rollback with nothing staged to take its place would DELETE it
        // at cleanup time.
        if !staging_dir.join(dir_name).exists() {
            continue;
        }
        let src = data_dir.join(dir_name);
        if src.exists() {
            let dst = rollback_dir.join(dir_name);
            if let Err(e) = fs::rename(&src, &dst).await {
                // Rollback: restore what we already moved
                info!("Restore failed during move, rolling back: {}", e);
                restore_from_rollback(data_dir, &rollback_dir).await;
                let _ = fs::remove_dir_all(&staging_dir).await;
                let _ = fs::remove_dir_all(&rollback_dir).await;
                return Err(e).context("Failed to move current data to rollback");
            }
        }
    }
    for file_name in BACKUP_FILES {
        if !staging_dir.join(file_name).exists() {
            continue;
        }
        let src = data_dir.join(file_name);
        if src.exists() {
            let dst = rollback_dir.join(file_name);
            let _ = fs::rename(&src, &dst).await;
        }
    }

    // Move staging contents to data_dir
    if let Err(e) = move_staging_to_data(data_dir, &staging_dir).await {
        info!("Restore failed during staging swap, rolling back: {}", e);
        restore_from_rollback(data_dir, &rollback_dir).await;
        let _ = fs::remove_dir_all(&staging_dir).await;
        let _ = fs::remove_dir_all(&rollback_dir).await;
        return Err(e).context("Failed to move staging data to data_dir");
    }

    // Clean up
    let _ = fs::remove_dir_all(&staging_dir).await;
    let _ = fs::remove_dir_all(&rollback_dir).await;

    info!(id = %backup_id, "Backup restored atomically");
    Ok(())
}

/// Move staging directory contents into data_dir.
async fn move_staging_to_data(data_dir: &Path, staging_dir: &Path) -> Result<()> {
    let mut entries = fs::read_dir(staging_dir)
        .await
        .context("Failed to read staging dir")?;
    while let Some(entry) = entries.next_entry().await? {
        let src = entry.path();
        let name = entry.file_name();
        let dst = data_dir.join(&name);
        fs::rename(&src, &dst)
            .await
            .with_context(|| format!("Failed to move {:?} from staging", name))?;
    }
    Ok(())
}

/// Restore data from rollback directory back to data_dir.
async fn restore_from_rollback(data_dir: &Path, rollback_dir: &Path) {
    if let Ok(mut entries) = fs::read_dir(rollback_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let src = entry.path();
            let dst = data_dir.join(entry.file_name());
            let _ = fs::rename(&src, &dst).await;
        }
    }
}

/// List available backups by reading metadata files.
pub async fn list_backups(data_dir: &Path) -> Result<Vec<BackupMetadata>> {
    let backups_dir = data_dir.join("backups");
    if !backups_dir.exists() {
        return Ok(Vec::new());
    }

    let mut backups = Vec::new();
    let mut entries = fs::read_dir(&backups_dir)
        .await
        .context("Failed to read backups dir")?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json")
            && path.to_str().is_some_and(|s| s.contains(".meta."))
        {
            let content = match fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(_) => continue,
            };
            if let Ok(meta) = serde_json::from_str::<BackupMetadata>(&content) {
                backups.push(meta);
            }
        }
    }

    // Sort newest first
    backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(backups)
}

/// Verify a backup's integrity by attempting decryption.
pub async fn verify_backup(
    data_dir: &Path,
    backup_id: &str,
    passphrase: &str,
) -> Result<VerifyResult> {
    let backup_path = data_dir.join("backups").join(format!("{}.bak", backup_id));
    let meta_path = data_dir
        .join("backups")
        .join(format!("{}.meta.json", backup_id));

    if !backup_path.exists() {
        return Ok(VerifyResult {
            valid: false,
            id: backup_id.to_string(),
            created_at: String::new(),
            size_bytes: 0,
            error: Some("Backup file not found".to_string()),
        });
    }

    let encrypted = fs::read(&backup_path)
        .await
        .context("Failed to read backup")?;

    let meta: BackupMetadata = if meta_path.exists() {
        let content = fs::read_to_string(&meta_path).await?;
        serde_json::from_str(&content)?
    } else {
        BackupMetadata {
            id: backup_id.to_string(),
            version: 0,
            created_at: String::new(),
            encrypted: true,
            size_bytes: encrypted.len() as u64,
            description: None,
        }
    };

    match decrypt_data(&encrypted, passphrase) {
        Ok(data) => {
            // Verify it's a valid gzip
            let mut decoder = GzDecoder::new(data.as_slice());
            let mut buf = [0u8; 512];
            match decoder.read(&mut buf) {
                Ok(_) => Ok(VerifyResult {
                    valid: true,
                    id: meta.id,
                    created_at: meta.created_at,
                    size_bytes: meta.size_bytes,
                    error: None,
                }),
                Err(e) => Ok(VerifyResult {
                    valid: false,
                    id: meta.id,
                    created_at: meta.created_at,
                    size_bytes: meta.size_bytes,
                    error: Some(format!("Archive corrupted: {}", e)),
                }),
            }
        }
        Err(e) => Ok(VerifyResult {
            valid: false,
            id: meta.id,
            created_at: meta.created_at,
            size_bytes: meta.size_bytes,
            error: Some(format!("Decryption failed: {}", e)),
        }),
    }
}

/// Get the backup file path for download.
pub fn backup_file_path(data_dir: &Path, backup_id: &str) -> PathBuf {
    data_dir.join("backups").join(format!("{}.bak", backup_id))
}

/// Info about a detected removable USB drive.
#[derive(Debug, Serialize, Deserialize)]
pub struct UsbDrive {
    pub device: String,
    pub mount_point: Option<String>,
    pub label: Option<String>,
    pub size_bytes: u64,
    pub removable: bool,
}

/// List removable USB drives on the system.
/// Scans /sys/block/sd* for removable devices.
pub async fn list_usb_drives() -> Result<Vec<UsbDrive>> {
    let mut drives = Vec::new();

    let mut entries = match fs::read_dir("/sys/block").await {
        Ok(e) => e,
        Err(_) => return Ok(drives),
    };

    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("sd") {
            continue;
        }

        // Check if removable
        let removable_path = format!("/sys/block/{}/removable", name);
        let removable = match fs::read_to_string(&removable_path).await {
            Ok(v) => v.trim() == "1",
            Err(_) => false,
        };
        if !removable {
            continue;
        }

        // Get size in 512-byte sectors
        let size_path = format!("/sys/block/{}/size", name);
        let size_bytes = match fs::read_to_string(&size_path).await {
            Ok(v) => v.trim().parse::<u64>().unwrap_or(0) * 512,
            Err(_) => 0,
        };

        let device = format!("/dev/{}", name);

        // Check mount point from /proc/mounts
        let mount_point = find_mount_point(&device).await;

        // Try to get label from the first partition
        let partition = format!("{}1", device);
        let label = get_fs_label(&partition).await;

        drives.push(UsbDrive {
            device,
            mount_point,
            label,
            size_bytes,
            removable: true,
        });
    }

    Ok(drives)
}

/// Copy a backup file to a mounted USB drive.
pub async fn backup_to_usb(data_dir: &Path, backup_id: &str, mount_point: &str) -> Result<PathBuf> {
    let src = backup_file_path(data_dir, backup_id);
    if !src.exists() {
        anyhow::bail!("Backup not found: {}", backup_id);
    }

    let mount_path = Path::new(mount_point);
    if !mount_path.exists() || !mount_path.is_dir() {
        anyhow::bail!("Mount point not accessible: {}", mount_point);
    }

    let dest_dir = mount_path.join("archipelago-backups");
    fs::create_dir_all(&dest_dir)
        .await
        .context("Failed to create backup dir on USB")?;

    let filename = format!("{}.bak", backup_id);
    let dest = dest_dir.join(&filename);

    fs::copy(&src, &dest)
        .await
        .context("Failed to copy backup to USB")?;

    // Also copy metadata
    let meta_src = data_dir
        .join("backups")
        .join(format!("{}.meta.json", backup_id));
    if meta_src.exists() {
        let meta_dest = dest_dir.join(format!("{}.meta.json", backup_id));
        let _ = fs::copy(&meta_src, &meta_dest).await;
    }

    info!(id = %backup_id, dest = %dest.display(), "Backup copied to USB");
    Ok(dest)
}

async fn find_mount_point(device: &str) -> Option<String> {
    let mounts = fs::read_to_string("/proc/mounts").await.ok()?;
    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[0].starts_with(device) {
            return Some(parts[1].to_string());
        }
    }
    None
}

async fn get_fs_label(partition: &str) -> Option<String> {
    let output = tokio::process::Command::new("blkid")
        .arg("-s")
        .arg("LABEL")
        .arg("-o")
        .arg("value")
        .arg(partition)
        .output()
        .await
        .ok()?;
    if output.status.success() {
        let label = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !label.is_empty() {
            return Some(label);
        }
    }
    None
}

// --- Internal helpers ---

fn create_tar_gz(data_dir: &Path) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let gz = GzEncoder::new(&mut buf, Compression::default());
    let mut tar = Builder::new(gz);

    // Add directories
    for dir_name in BACKUP_DIRS {
        let dir_path = data_dir.join(dir_name);
        if dir_path.exists() && dir_path.is_dir() {
            tar.append_dir_all(*dir_name, &dir_path)
                .with_context(|| format!("Failed to add dir {} to backup", dir_name))?;
            debug!(dir = %dir_name, "Added directory to backup");
        }
    }

    // Add individual files
    for file_name in BACKUP_FILES {
        let file_path = data_dir.join(file_name);
        if file_path.exists() && file_path.is_file() {
            let data = std::fs::read(&file_path)
                .with_context(|| format!("Failed to read {}", file_name))?;
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, *file_name, data.as_slice())
                .with_context(|| format!("Failed to add {} to backup", file_name))?;
            debug!(file = %file_name, "Added file to backup");
        }
    }

    tar.into_inner()
        .context("Failed to finalize tar")?
        .finish()
        .context("Failed to finalize gzip")?;

    Ok(buf)
}

fn extract_tar_gz(data_dir: &Path, tar_gz_data: &[u8]) -> Result<()> {
    let gz = GzDecoder::new(tar_gz_data);
    let mut archive = Archive::new(gz);
    let canonical_base = data_dir
        .canonicalize()
        .context("Failed to canonicalize data_dir")?;

    for entry_result in archive.entries().context("Failed to read tar entries")? {
        let mut entry = entry_result.context("Failed to read tar entry")?;
        let entry_path = entry
            .path()
            .context("Failed to get entry path")?
            .to_path_buf();

        // Reject entries with path traversal components
        for component in entry_path.components() {
            if matches!(component, std::path::Component::ParentDir) {
                anyhow::bail!(
                    "Tar entry contains path traversal: {}",
                    entry_path.display()
                );
            }
        }

        let target = data_dir.join(&entry_path);
        // Verify the resolved path stays within data_dir
        // For new files that don't exist yet, check the parent directory
        let check_path = if target.exists() {
            target.canonicalize()?
        } else if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
            parent
                .canonicalize()?
                .join(target.file_name().unwrap_or_default())
        } else {
            target.clone()
        };
        if !check_path.starts_with(&canonical_base) {
            anyhow::bail!(
                "Tar entry escapes target directory: {}",
                entry_path.display()
            );
        }

        entry
            .unpack(&target)
            .with_context(|| format!("Failed to extract: {}", entry_path.display()))?;
    }

    debug!("Backup extracted to {:?}", data_dir);
    Ok(())
}

fn encrypt_data(data: &[u8], passphrase: &str) -> Result<Vec<u8>> {
    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    let argon2 = Argon2::default();
    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(passphrase.as_bytes(), &salt, &mut key)
        .map_err(|e| anyhow::anyhow!("Key derivation failed: {}", e))?;

    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("Cipher init: {}", e))?;
    let ciphertext = cipher
        .encrypt(
            chacha20poly1305::aead::generic_array::GenericArray::from_slice(&nonce),
            data,
        )
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    // Format: salt || nonce || ciphertext
    let mut output = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

fn decrypt_data(data: &[u8], passphrase: &str) -> Result<Vec<u8>> {
    if data.len() < SALT_LEN + NONCE_LEN {
        anyhow::bail!("Backup data too short");
    }

    let salt = &data[..SALT_LEN];
    let nonce = &data[SALT_LEN..SALT_LEN + NONCE_LEN];
    let ciphertext = &data[SALT_LEN + NONCE_LEN..];

    let argon2 = Argon2::default();
    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow::anyhow!("Key derivation failed: {}", e))?;

    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("Cipher init: {}", e))?;
    let plaintext = cipher
        .decrypt(
            chacha20poly1305::aead::generic_array::GenericArray::from_slice(nonce),
            ciphertext,
        )
        .map_err(|_| anyhow::anyhow!("Decryption failed — wrong passphrase or corrupted data"))?;

    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_data_dir(dir: &Path) {
        // Create some test data
        std::fs::create_dir_all(dir.join("identity")).unwrap();
        std::fs::write(dir.join("identity/node_key"), vec![0xAB; 32]).unwrap();
        std::fs::write(dir.join("user.json"), r#"{"user":"test"}"#).unwrap();
        std::fs::write(dir.join("peers.json"), "[]").unwrap();
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let data = b"Hello, Archipelago backup!";
        let pass = "test-passphrase";
        let encrypted = encrypt_data(data, pass).unwrap();
        assert_ne!(&encrypted[..], data);
        let decrypted = decrypt_data(&encrypted, pass).unwrap();
        assert_eq!(&decrypted, data);
    }

    #[test]
    fn wrong_passphrase_fails() {
        let data = b"secret data";
        let encrypted = encrypt_data(data, "correct").unwrap();
        assert!(decrypt_data(&encrypted, "wrong").is_err());
    }

    #[test]
    fn tar_gz_roundtrip() {
        let dir = TempDir::new().unwrap();
        setup_data_dir(dir.path());

        let archive = create_tar_gz(dir.path()).unwrap();
        assert!(!archive.is_empty());

        // Extract to a new dir
        let restore_dir = TempDir::new().unwrap();
        extract_tar_gz(restore_dir.path(), &archive).unwrap();

        // Verify files exist
        assert!(restore_dir.path().join("identity/node_key").exists());
        assert!(restore_dir.path().join("user.json").exists());
        assert!(restore_dir.path().join("peers.json").exists());
    }

    #[tokio::test]
    async fn full_backup_and_list() {
        let dir = TempDir::new().unwrap();
        setup_data_dir(dir.path());

        let meta = create_full_backup(dir.path(), "backup-pass", Some("Test backup"))
            .await
            .unwrap();
        assert!(!meta.id.is_empty());
        assert!(meta.size_bytes > 0);
        assert_eq!(meta.description, Some("Test backup".to_string()));

        let backups = list_backups(dir.path()).await.unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].id, meta.id);
    }

    #[tokio::test]
    async fn backup_verify() {
        let dir = TempDir::new().unwrap();
        setup_data_dir(dir.path());

        let meta = create_full_backup(dir.path(), "my-pass", None)
            .await
            .unwrap();

        let result = verify_backup(dir.path(), &meta.id, "my-pass")
            .await
            .unwrap();
        assert!(result.valid);

        let bad_result = verify_backup(dir.path(), &meta.id, "wrong-pass")
            .await
            .unwrap();
        assert!(!bad_result.valid);
    }

    #[tokio::test]
    async fn secrets_dir_rides_backup_and_restore() {
        // The Lightning seed backup (identity/lnd_aezeed.enc) is encrypted
        // with secrets/lnd-wallet-password — a backup that omits it can't
        // recover the Lightning wallet on restore.
        let dir = TempDir::new().unwrap();
        setup_data_dir(dir.path());
        std::fs::create_dir_all(dir.path().join("secrets")).unwrap();
        std::fs::write(dir.path().join("secrets/lnd-wallet-password"), "s3cret").unwrap();

        let meta = create_full_backup(dir.path(), "pass", None).await.unwrap();

        std::fs::remove_dir_all(dir.path().join("secrets")).unwrap();
        restore_full_backup(dir.path(), &meta.id, "pass")
            .await
            .unwrap();

        let pw = std::fs::read_to_string(dir.path().join("secrets/lnd-wallet-password")).unwrap();
        assert_eq!(pw, "s3cret");
    }

    #[tokio::test]
    async fn restoring_old_backup_without_secrets_keeps_live_secrets() {
        // Archives created before `secrets` joined BACKUP_DIRS don't stage
        // one — the restore must NOT displace (and then delete) the node's
        // live secrets in that case.
        let dir = TempDir::new().unwrap();
        setup_data_dir(dir.path());

        // Backup taken while no secrets dir existed (mimics an old archive).
        let meta = create_full_backup(dir.path(), "pass", None).await.unwrap();

        // Live secrets appear afterwards.
        std::fs::create_dir_all(dir.path().join("secrets")).unwrap();
        std::fs::write(dir.path().join("secrets/lnd-wallet-password"), "keep-me").unwrap();

        restore_full_backup(dir.path(), &meta.id, "pass")
            .await
            .unwrap();

        let pw = std::fs::read_to_string(dir.path().join("secrets/lnd-wallet-password")).unwrap();
        assert_eq!(pw, "keep-me");
    }

    #[tokio::test]
    async fn backup_and_restore() {
        let dir = TempDir::new().unwrap();
        setup_data_dir(dir.path());

        let meta = create_full_backup(dir.path(), "restore-pass", None)
            .await
            .unwrap();

        // Delete the original data
        std::fs::remove_file(dir.path().join("user.json")).unwrap();
        assert!(!dir.path().join("user.json").exists());

        // Restore
        restore_full_backup(dir.path(), &meta.id, "restore-pass")
            .await
            .unwrap();

        // Verify restored
        assert!(dir.path().join("user.json").exists());
        let content = std::fs::read_to_string(dir.path().join("user.json")).unwrap();
        assert_eq!(content, r#"{"user":"test"}"#);
    }
}
