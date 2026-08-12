// Encrypted secrets management for containers
// Stores secrets encrypted with AES-256-GCM and injects them at runtime

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;

/// Prefix to identify encrypted files (magic bytes)
const ENCRYPTED_MAGIC: &[u8] = b"ARCHI_ENC1";

/// Metadata for a stored secret (stored alongside the encrypted data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMetadata {
    pub secret_id: String,
    pub key: String,
    pub app_id: String,
    pub created_at: DateTime<Utc>,
    pub rotated_at: Option<DateTime<Utc>>,
    pub rotation_count: u32,
}

/// Info about a secret that may need rotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpiringSecret {
    pub secret_id: String,
    pub key: String,
    pub app_id: String,
    pub age_days: i64,
    pub last_rotated: Option<DateTime<Utc>>,
}

pub struct SecretsManager {
    secrets_dir: PathBuf,
    cipher: Aes256Gcm,
    _raw_key: zeroize::Zeroizing<[u8; 32]>,
}

impl SecretsManager {
    /// Create a new SecretsManager with a 32-byte encryption key.
    /// In production, derive this key from the user's password via Argon2.
    pub fn new(secrets_dir: PathBuf, encryption_key: Vec<u8>) -> Result<Self> {
        anyhow::ensure!(
            encryption_key.len() == 32,
            "Encryption key must be exactly 32 bytes (256 bits), got {}",
            encryption_key.len()
        );
        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&encryption_key);
        let cipher = Aes256Gcm::new_from_slice(&key_array)
            .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;
        Ok(Self {
            secrets_dir,
            cipher,
            _raw_key: zeroize::Zeroizing::new(key_array),
        })
    }

    /// Encrypt a plaintext value using AES-256-GCM.
    /// Returns: MAGIC (10 bytes) + nonce (12 bytes) + ciphertext (variable)
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut nonce_bytes = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        let mut output = Vec::with_capacity(ENCRYPTED_MAGIC.len() + 12 + ciphertext.len());
        output.extend_from_slice(ENCRYPTED_MAGIC);
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&ciphertext);
        Ok(output)
    }

    /// Decrypt a previously encrypted value.
    fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let magic_len = ENCRYPTED_MAGIC.len();
        anyhow::ensure!(data.len() > magic_len + 12, "Encrypted data too short");
        anyhow::ensure!(
            &data[..magic_len] == ENCRYPTED_MAGIC,
            "Invalid encrypted data (bad magic bytes)"
        );

        let nonce = Nonce::from_slice(&data[magic_len..magic_len + 12]);
        let ciphertext = &data[magic_len + 12..];

        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption failed (wrong key or corrupted data): {}", e))
    }

    /// Store a secret for an app (encrypted at rest)
    pub async fn store_secret(&self, app_id: &str, key: &str, value: &str) -> Result<String> {
        let secret_id = Uuid::new_v4().to_string();
        let secret_path = self
            .secrets_dir
            .join(app_id)
            .join(format!("{}.secret", secret_id));

        let parent = secret_path.parent().ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid secret path: no parent directory for {:?}",
                secret_path
            )
        })?;
        fs::create_dir_all(parent).await?;

        let encrypted = self
            .encrypt(value.as_bytes())
            .context("Failed to encrypt secret")?;
        fs::write(&secret_path, &encrypted)
            .await
            .context("Failed to write secret")?;

        // Save metadata
        let metadata = SecretMetadata {
            secret_id: secret_id.clone(),
            key: key.to_string(),
            app_id: app_id.to_string(),
            created_at: Utc::now(),
            rotated_at: None,
            rotation_count: 0,
        };
        let meta_path = self
            .secrets_dir
            .join(app_id)
            .join(format!("{}.meta.json", secret_id));
        let meta_json = serde_json::to_string(&metadata).context("Failed to serialize metadata")?;
        fs::write(&meta_path, meta_json.as_bytes())
            .await
            .context("Failed to write metadata")?;

        // Set restrictive permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&secret_path).await?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&secret_path, perms).await?;
            let mut meta_perms = fs::metadata(&meta_path).await?.permissions();
            meta_perms.set_mode(0o600);
            fs::set_permissions(&meta_path, meta_perms).await?;
        }

        Ok(secret_id)
    }

    /// Read and decrypt a secret value
    pub async fn read_secret(&self, app_id: &str, secret_id: &str) -> Result<String> {
        let secret_path = self
            .secrets_dir
            .join(app_id)
            .join(format!("{}.secret", secret_id));

        let data = fs::read(&secret_path)
            .await
            .context("Failed to read secret file")?;

        // Support reading legacy plaintext secrets (no magic prefix)
        if data.len() < ENCRYPTED_MAGIC.len() || &data[..ENCRYPTED_MAGIC.len()] != ENCRYPTED_MAGIC {
            return String::from_utf8(data).context("Legacy secret is not valid UTF-8");
        }

        let plaintext = self.decrypt(&data)?;
        String::from_utf8(plaintext).context("Decrypted secret is not valid UTF-8")
    }

    /// Retrieve a secret (returns the secret ID path for volume mounting)
    pub fn get_secret_path(&self, app_id: &str, secret_id: &str) -> PathBuf {
        self.secrets_dir
            .join(app_id)
            .join(format!("{}.secret", secret_id))
    }

    /// List secrets for an app
    pub async fn list_secrets(&self, app_id: &str) -> Result<Vec<String>> {
        let app_secrets_dir = self.secrets_dir.join(app_id);

        if !app_secrets_dir.exists() {
            return Ok(vec![]);
        }

        let mut secrets = Vec::new();
        let mut entries = fs::read_dir(&app_secrets_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("secret") {
                if let Some(secret_id) = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
                {
                    secrets.push(secret_id);
                }
            }
        }

        Ok(secrets)
    }

    /// Rotate a secret: generate a new random value, re-encrypt, update metadata.
    /// Returns the new plaintext secret value.
    pub async fn rotate_secret(&self, app_id: &str, secret_id: &str) -> Result<String> {
        // Generate a new random secret (32 bytes, hex-encoded = 64 chars)
        let mut new_secret_bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut new_secret_bytes);
        let new_value = hex::encode(new_secret_bytes);

        let secret_path = self
            .secrets_dir
            .join(app_id)
            .join(format!("{}.secret", secret_id));

        anyhow::ensure!(
            secret_path.exists(),
            "Secret {} not found for app {}",
            secret_id,
            app_id
        );

        // Re-encrypt with new value
        let encrypted = self
            .encrypt(new_value.as_bytes())
            .context("Failed to encrypt rotated secret")?;
        fs::write(&secret_path, &encrypted)
            .await
            .context("Failed to write rotated secret")?;

        // Update metadata
        let meta_path = self
            .secrets_dir
            .join(app_id)
            .join(format!("{}.meta.json", secret_id));
        if meta_path.exists() {
            if let Ok(data) = fs::read_to_string(&meta_path).await {
                if let Ok(mut metadata) = serde_json::from_str::<SecretMetadata>(&data) {
                    metadata.rotated_at = Some(Utc::now());
                    metadata.rotation_count += 1;
                    if let Ok(json) = serde_json::to_string(&metadata) {
                        let _ = fs::write(&meta_path, json.as_bytes()).await;
                    }
                }
            }
        }

        Ok(new_value)
    }

    /// List secrets older than `max_age_days` that may need rotation.
    pub async fn list_expiring(&self, max_age_days: i64) -> Result<Vec<ExpiringSecret>> {
        let mut expiring = Vec::new();
        let now = Utc::now();

        if !self.secrets_dir.exists() {
            return Ok(expiring);
        }

        let mut app_dirs = fs::read_dir(&self.secrets_dir).await?;
        while let Some(app_entry) = app_dirs.next_entry().await? {
            let app_path = app_entry.path();
            if !app_path.is_dir() {
                continue;
            }
            let _app_id = app_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            let mut entries = fs::read_dir(&app_path).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if !name.ends_with(".meta.json") {
                    continue;
                }

                if let Ok(data) = fs::read_to_string(&path).await {
                    if let Ok(metadata) = serde_json::from_str::<SecretMetadata>(&data) {
                        let reference_time = metadata.rotated_at.unwrap_or(metadata.created_at);
                        let age = now.signed_duration_since(reference_time);
                        if age.num_days() >= max_age_days {
                            expiring.push(ExpiringSecret {
                                secret_id: metadata.secret_id,
                                key: metadata.key,
                                app_id: metadata.app_id,
                                age_days: age.num_days(),
                                last_rotated: metadata.rotated_at,
                            });
                        }
                    }
                }
            }
        }

        Ok(expiring)
    }

    /// Read metadata for a specific secret.
    pub async fn get_metadata(
        &self,
        app_id: &str,
        secret_id: &str,
    ) -> Result<Option<SecretMetadata>> {
        let meta_path = self
            .secrets_dir
            .join(app_id)
            .join(format!("{}.meta.json", secret_id));

        if !meta_path.exists() {
            return Ok(None);
        }

        let data = fs::read_to_string(&meta_path)
            .await
            .context("Failed to read metadata")?;
        let metadata: SecretMetadata =
            serde_json::from_str(&data).context("Failed to parse metadata")?;
        Ok(Some(metadata))
    }

    /// Delete a secret and its metadata
    pub async fn delete_secret(&self, app_id: &str, secret_id: &str) -> Result<()> {
        let secret_path = self
            .secrets_dir
            .join(app_id)
            .join(format!("{}.secret", secret_id));
        let meta_path = self
            .secrets_dir
            .join(app_id)
            .join(format!("{}.meta.json", secret_id));

        if secret_path.exists() {
            fs::remove_file(&secret_path).await?;
        }
        if meta_path.exists() {
            fs::remove_file(&meta_path).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> Vec<u8> {
        vec![0x42; 32] // 32-byte test key
    }

    #[tokio::test]
    async fn test_encrypt_decrypt_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SecretsManager::new(dir.path().to_path_buf(), test_key()).unwrap();

        let secret_id = mgr
            .store_secret("test-app", "db-password", "supersecret123")
            .await
            .unwrap();

        let decrypted = mgr.read_secret("test-app", &secret_id).await.unwrap();
        assert_eq!(decrypted, "supersecret123");
    }

    #[tokio::test]
    async fn test_wrong_key_fails() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SecretsManager::new(dir.path().to_path_buf(), test_key()).unwrap();

        let secret_id = mgr.store_secret("test-app", "key", "secret").await.unwrap();

        let wrong_key = vec![0x99; 32];
        let mgr2 = SecretsManager::new(dir.path().to_path_buf(), wrong_key).unwrap();
        assert!(mgr2.read_secret("test-app", &secret_id).await.is_err());
    }

    #[tokio::test]
    async fn test_list_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SecretsManager::new(dir.path().to_path_buf(), test_key()).unwrap();

        let id1 = mgr.store_secret("app1", "k1", "v1").await.unwrap();
        let _id2 = mgr.store_secret("app1", "k2", "v2").await.unwrap();

        let list = mgr.list_secrets("app1").await.unwrap();
        assert_eq!(list.len(), 2);

        mgr.delete_secret("app1", &id1).await.unwrap();
        let list = mgr.list_secrets("app1").await.unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_invalid_key_length() {
        let dir = tempfile::tempdir().unwrap();
        assert!(SecretsManager::new(dir.path().to_path_buf(), vec![0; 16]).is_err());
    }

    #[tokio::test]
    async fn test_file_is_encrypted_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SecretsManager::new(dir.path().to_path_buf(), test_key()).unwrap();

        let secret_id = mgr
            .store_secret("test-app", "key", "my_secret_value")
            .await
            .unwrap();

        let path = mgr.get_secret_path("test-app", &secret_id);
        let raw = std::fs::read(&path).unwrap();

        // File must NOT contain plaintext
        assert!(!raw.windows(15).any(|w| w == b"my_secret_value"));
        // File must start with our magic prefix
        assert_eq!(&raw[..ENCRYPTED_MAGIC.len()], ENCRYPTED_MAGIC);
    }

    #[tokio::test]
    async fn test_rotate_secret() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SecretsManager::new(dir.path().to_path_buf(), test_key()).unwrap();

        let secret_id = mgr
            .store_secret("test-app", "api-key", "original_value")
            .await
            .unwrap();

        let original = mgr.read_secret("test-app", &secret_id).await.unwrap();
        assert_eq!(original, "original_value");

        let new_value = mgr.rotate_secret("test-app", &secret_id).await.unwrap();
        assert_ne!(new_value, "original_value");
        assert_eq!(new_value.len(), 64); // 32 bytes hex-encoded

        let read_back = mgr.read_secret("test-app", &secret_id).await.unwrap();
        assert_eq!(read_back, new_value);
    }

    #[tokio::test]
    async fn test_rotate_updates_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SecretsManager::new(dir.path().to_path_buf(), test_key()).unwrap();

        let secret_id = mgr
            .store_secret("test-app", "db-pass", "initial")
            .await
            .unwrap();

        let meta_before = mgr
            .get_metadata("test-app", &secret_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(meta_before.rotation_count, 0);
        assert!(meta_before.rotated_at.is_none());

        mgr.rotate_secret("test-app", &secret_id).await.unwrap();

        let meta_after = mgr
            .get_metadata("test-app", &secret_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(meta_after.rotation_count, 1);
        assert!(meta_after.rotated_at.is_some());
    }

    #[tokio::test]
    async fn test_rotate_nonexistent_fails() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SecretsManager::new(dir.path().to_path_buf(), test_key()).unwrap();

        let result = mgr.rotate_secret("test-app", "nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_expiring_empty() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SecretsManager::new(dir.path().to_path_buf(), test_key()).unwrap();

        let expiring = mgr.list_expiring(90).await.unwrap();
        assert!(expiring.is_empty());
    }

    #[tokio::test]
    async fn test_list_expiring_fresh_secrets_not_listed() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SecretsManager::new(dir.path().to_path_buf(), test_key()).unwrap();

        mgr.store_secret("test-app", "key1", "val1").await.unwrap();

        // Fresh secret (0 days old) should not be listed for 90-day expiry
        let expiring = mgr.list_expiring(90).await.unwrap();
        assert!(expiring.is_empty());

        // But it should appear for 0-day threshold
        let expiring = mgr.list_expiring(0).await.unwrap();
        assert_eq!(expiring.len(), 1);
        assert_eq!(expiring[0].key, "key1");
    }

    #[tokio::test]
    async fn test_metadata_stored_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SecretsManager::new(dir.path().to_path_buf(), test_key()).unwrap();

        let secret_id = mgr
            .store_secret("myapp", "connection-string", "postgres://...")
            .await
            .unwrap();

        let meta = mgr
            .get_metadata("myapp", &secret_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(meta.key, "connection-string");
        assert_eq!(meta.app_id, "myapp");
        assert_eq!(meta.rotation_count, 0);
    }

    #[tokio::test]
    async fn test_delete_removes_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SecretsManager::new(dir.path().to_path_buf(), test_key()).unwrap();

        let secret_id = mgr.store_secret("test-app", "key", "val").await.unwrap();

        mgr.delete_secret("test-app", &secret_id).await.unwrap();

        let meta = mgr.get_metadata("test-app", &secret_id).await.unwrap();
        assert!(meta.is_none());
    }
}
