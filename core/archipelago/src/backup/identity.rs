//! Encrypted DID identity backup for onboarding.
//! Uses Argon2 for key derivation and ChaCha20-Poly1305 for encryption.

use anyhow::{Context, Result};
use argon2::Argon2;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chacha20poly1305::aead::{Aead, KeyInit};
use rand::RngCore;
use std::path::Path;
use tokio::fs;

const BACKUP_VERSION: u32 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// Create an encrypted backup of the node identity key.
/// Returns JSON-serializable backup metadata + encrypted blob (base64).
pub async fn create_encrypted_backup(
    identity_dir: &Path,
    passphrase: &str,
    did: &str,
    pubkey_hex: &str,
) -> Result<serde_json::Value> {
    let key_path = identity_dir.join("node_key");
    let key_bytes = fs::read(&key_path)
        .await
        .context("Failed to read node key")?;
    if key_bytes.len() != 32 {
        anyhow::bail!("Invalid node key length");
    }

    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    let argon2 = Argon2::default();
    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(passphrase.as_bytes(), &salt, &mut key)
        .map_err(|e| anyhow::anyhow!("Argon2 key derivation failed: {}", e))?;

    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("Cipher init: {}", e))?;
    let ciphertext = cipher
        .encrypt(
            chacha20poly1305::aead::generic_array::GenericArray::from_slice(&nonce),
            key_bytes.as_ref(),
        )
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    let mut blob = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(&salt);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    let blob_b64 = BASE64.encode(&blob);

    Ok(serde_json::json!({
        "version": BACKUP_VERSION,
        "did": did,
        "pubkey": pubkey_hex,
        "kid": format!("{}#key-1", did),
        "encrypted": true,
        "blob": blob_b64,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

/// Restore a node identity key from an encrypted backup.
/// Returns the DID and pubkey of the restored identity.
pub async fn restore_encrypted_backup(
    identity_dir: &Path,
    backup: &serde_json::Value,
    passphrase: &str,
) -> Result<(String, String)> {
    let blob_b64 = backup
        .get("blob")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'blob' in backup"))?;
    let blob = BASE64
        .decode(blob_b64)
        .context("Invalid base64 in backup blob")?;

    if blob.len() < SALT_LEN + NONCE_LEN {
        anyhow::bail!("Backup blob too short");
    }

    let salt = &blob[..SALT_LEN];
    let nonce = &blob[SALT_LEN..SALT_LEN + NONCE_LEN];
    let ciphertext = &blob[SALT_LEN + NONCE_LEN..];

    let argon2 = Argon2::default();
    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow::anyhow!("Argon2 key derivation failed: {}", e))?;

    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("Cipher init: {}", e))?;
    let plaintext = cipher
        .decrypt(
            chacha20poly1305::aead::generic_array::GenericArray::from_slice(nonce),
            ciphertext,
        )
        .map_err(|_| anyhow::anyhow!("Decryption failed — wrong passphrase"))?;

    if plaintext.len() != 32 {
        anyhow::bail!("Decrypted key is not 32 bytes");
    }

    // Write the restored key
    fs::create_dir_all(identity_dir).await?;
    let key_path = identity_dir.join("node_key");
    fs::write(&key_path, &plaintext)
        .await
        .context("Writing restored key")?;

    // Set restrictive permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        tokio::fs::set_permissions(&key_path, perms).await?;
    }

    // Derive DID and pubkey from the restored key
    let signing_key = ed25519_dalek::SigningKey::from_bytes(
        plaintext
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid key"))?,
    );
    let pubkey = signing_key.verifying_key();
    let pubkey_hex = hex::encode(pubkey.as_bytes());
    let did = crate::identity::did_key_from_pubkey_hex(&pubkey_hex)?;

    Ok((did, pubkey_hex))
}
