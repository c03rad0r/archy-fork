use super::RpcHandler;
use crate::backup::full;
use anyhow::{Context, Result};
use std::net::IpAddr;
use tracing::info;

/// Validate an S3 endpoint URL: require https, reject private/loopback IPs and localhost.
fn validate_s3_endpoint(endpoint: &str) -> Result<()> {
    // Require HTTPS scheme
    if !endpoint.starts_with("https://") {
        anyhow::bail!("S3 endpoint must use https://");
    }

    // Extract host from URL (strip scheme, path, port)
    let after_scheme = &endpoint["https://".len()..];
    let host_port = after_scheme.split('/').next().unwrap_or("");
    // Strip port if present (handle IPv6 bracket notation)
    let host = if host_port.starts_with('[') {
        // IPv6: [::1]:443
        host_port
            .split(']')
            .next()
            .unwrap_or("")
            .trim_start_matches('[')
    } else {
        host_port.split(':').next().unwrap_or("")
    };

    if host.is_empty() {
        anyhow::bail!("S3 endpoint missing host");
    }

    // Reject localhost
    if host == "localhost" || host.ends_with(".localhost") {
        anyhow::bail!("S3 endpoint must not point to localhost");
    }

    // Parse as IP and reject private/reserved ranges
    if let Ok(ip) = host.parse::<IpAddr>() {
        let is_private = match ip {
            IpAddr::V4(v4) => {
                v4.is_loopback()                                                 // 127.0.0.0/8
                    || v4.octets()[0] == 10                                      // 10.0.0.0/8
                    || (v4.octets()[0] == 172 && (v4.octets()[1] & 0xf0) == 16)  // 172.16.0.0/12
                    || (v4.octets()[0] == 192 && v4.octets()[1] == 168)          // 192.168.0.0/16
                    || (v4.octets()[0] == 169 && v4.octets()[1] == 254)          // 169.254.0.0/16
                    || v4.is_unspecified() // 0.0.0.0
            }
            IpAddr::V6(v6) => {
                v6.is_loopback()                                                 // ::1
                    || (v6.segments()[0] & 0xfe00) == 0xfc00                     // fc00::/7
                    || v6.is_unspecified() // ::
            }
        };
        if is_private {
            anyhow::bail!("S3 endpoint must not point to a private or reserved IP address");
        }
    }

    Ok(())
}

impl RpcHandler {
    /// Create a full encrypted backup. Params: { passphrase, description? }
    pub(super) async fn handle_backup_create(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let passphrase = params["passphrase"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'passphrase' parameter"))?;
        let description = params["description"].as_str();

        let meta = full::create_full_backup(&self.config.data_dir, passphrase, description).await?;

        Ok(serde_json::json!({
            "id": meta.id,
            "created_at": meta.created_at,
            "size_bytes": meta.size_bytes,
            "encrypted": meta.encrypted,
            "description": meta.description,
        }))
    }

    /// List available backups.
    pub(super) async fn handle_backup_list(&self) -> Result<serde_json::Value> {
        let backups = full::list_backups(&self.config.data_dir).await?;
        let list: Vec<serde_json::Value> = backups
            .iter()
            .map(|b| {
                serde_json::json!({
                    "id": b.id,
                    "created_at": b.created_at,
                    "size_bytes": b.size_bytes,
                    "encrypted": b.encrypted,
                    "description": b.description,
                })
            })
            .collect();
        Ok(serde_json::json!({ "backups": list }))
    }

    /// Verify a backup's integrity. Params: { id, passphrase }
    pub(super) async fn handle_backup_verify(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = params["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'id' parameter"))?;
        let passphrase = params["passphrase"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'passphrase' parameter"))?;

        // Validate backup ID to prevent path traversal
        if id.is_empty()
            || id.len() > 128
            || id.contains('/')
            || id.contains('\\')
            || id.contains("..")
            || id.contains('\0')
        {
            anyhow::bail!("Invalid backup ID");
        }

        let result = full::verify_backup(&self.config.data_dir, id, passphrase).await?;

        Ok(serde_json::json!({
            "valid": result.valid,
            "id": result.id,
            "created_at": result.created_at,
            "size_bytes": result.size_bytes,
            "error": result.error,
        }))
    }

    /// Restore from a backup. Params: { id, passphrase }
    pub(super) async fn handle_backup_restore(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = params["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'id' parameter"))?;
        let passphrase = params["passphrase"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'passphrase' parameter"))?;

        // Validate backup ID to prevent path traversal
        if id.is_empty()
            || id.len() > 128
            || id.contains('/')
            || id.contains('\\')
            || id.contains("..")
            || id.contains('\0')
        {
            anyhow::bail!("Invalid backup ID");
        }

        full::restore_full_backup(&self.config.data_dir, id, passphrase).await?;

        Ok(serde_json::json!({ "restored": true, "id": id }))
    }

    /// Delete a backup. Params: { id }
    pub(super) async fn handle_backup_delete(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = params["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'id' parameter"))?;

        // Validate backup ID to prevent path traversal
        if id.is_empty()
            || id.len() > 128
            || id.contains('/')
            || id.contains('\\')
            || id.contains("..")
            || id.contains('\0')
        {
            anyhow::bail!("Invalid backup ID");
        }

        let bak_path = full::backup_file_path(&self.config.data_dir, id);
        let meta_path = self
            .config
            .data_dir
            .join("backups")
            .join(format!("{}.meta.json", id));

        let mut deleted = false;
        if bak_path.exists() {
            tokio::fs::remove_file(&bak_path).await?;
            deleted = true;
        }
        if meta_path.exists() {
            tokio::fs::remove_file(&meta_path).await?;
        }

        Ok(serde_json::json!({ "deleted": deleted, "id": id }))
    }

    /// List removable USB drives.
    pub(super) async fn handle_backup_list_drives(&self) -> Result<serde_json::Value> {
        let drives = full::list_usb_drives().await?;
        let list: Vec<serde_json::Value> = drives
            .iter()
            .map(|d| {
                serde_json::json!({
                    "device": d.device,
                    "mount_point": d.mount_point,
                    "label": d.label,
                    "size_bytes": d.size_bytes,
                    "removable": d.removable,
                })
            })
            .collect();
        Ok(serde_json::json!({ "drives": list }))
    }

    /// Copy a backup to a mounted USB drive. Params: { id, mount_point }
    pub(super) async fn handle_backup_to_usb(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = params["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'id' parameter"))?;
        let mount_point = params["mount_point"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'mount_point' parameter"))?;

        let dest = full::backup_to_usb(&self.config.data_dir, id, mount_point).await?;

        Ok(serde_json::json!({
            "copied": true,
            "id": id,
            "destination": dest.to_string_lossy(),
        }))
    }

    /// Upload a backup to S3-compatible storage.
    /// Params: { id, endpoint, bucket, access_key, secret_key, region? }
    pub(super) async fn handle_backup_upload_s3(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = params["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'id' parameter"))?;
        let endpoint = params["endpoint"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'endpoint' parameter"))?;
        let bucket = params["bucket"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'bucket' parameter"))?;
        let access_key = params["access_key"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'access_key' parameter"))?;
        let secret_key = params["secret_key"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'secret_key' parameter"))?;
        let _region = params["region"].as_str().unwrap_or("us-east-1");

        // Validate backup ID
        if id.is_empty()
            || id.len() > 128
            || id.contains('/')
            || id.contains('\\')
            || id.contains("..")
            || id.contains('\0')
        {
            anyhow::bail!("Invalid backup ID");
        }

        // Validate endpoint to prevent SSRF against internal services
        validate_s3_endpoint(endpoint)?;

        let bak_path = full::backup_file_path(&self.config.data_dir, id);
        if !bak_path.exists() {
            anyhow::bail!("Backup not found: {}", id);
        }

        let file_bytes = tokio::fs::read(&bak_path)
            .await
            .context("Failed to read backup file")?;
        let key = format!("archipelago-backups/{}.tar.gz.enc", id);
        let size = file_bytes.len();

        // Upload via HTTP PUT to S3-compatible endpoint
        let url = format!("{}/{}/{}", endpoint.trim_end_matches('/'), bucket, key);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()?;

        // Simple S3 PUT (works with MinIO, Backblaze B2 S3-compatible, Wasabi)
        // For full AWS S3, proper SigV4 signing would be needed
        let response = client
            .put(&url)
            .basic_auth(access_key, Some(secret_key))
            .header("Content-Type", "application/octet-stream")
            .body(file_bytes)
            .send()
            .await
            .context("S3 upload failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "S3 upload failed ({}): {}",
                status,
                &body[..200.min(body.len())]
            );
        }

        info!(id = %id, bucket = %bucket, size = %size, "Backup uploaded to S3");

        Ok(serde_json::json!({
            "uploaded": true,
            "id": id,
            "bucket": bucket,
            "key": key,
            "size_bytes": size,
        }))
    }

    /// Download a backup from S3-compatible storage.
    /// Params: { id, endpoint, bucket, access_key, secret_key, region? }
    pub(super) async fn handle_backup_download_s3(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = params["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'id' parameter"))?;
        let endpoint = params["endpoint"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'endpoint' parameter"))?;
        let bucket = params["bucket"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'bucket' parameter"))?;
        let access_key = params["access_key"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'access_key' parameter"))?;
        let secret_key = params["secret_key"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'secret_key' parameter"))?;

        if id.is_empty()
            || id.len() > 128
            || id.contains('/')
            || id.contains('\\')
            || id.contains("..")
            || id.contains('\0')
        {
            anyhow::bail!("Invalid backup ID");
        }

        // Validate endpoint to prevent SSRF against internal services
        validate_s3_endpoint(endpoint)?;

        let key = format!("archipelago-backups/{}.tar.gz.enc", id);
        let url = format!("{}/{}/{}", endpoint.trim_end_matches('/'), bucket, key);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()?;

        let response = client
            .get(&url)
            .basic_auth(access_key, Some(secret_key))
            .send()
            .await
            .context("S3 download failed")?;

        if !response.status().is_success() {
            let status = response.status();
            anyhow::bail!("S3 download failed ({})", status);
        }

        let bytes = response
            .bytes()
            .await
            .context("Failed to read S3 response")?;
        let size = bytes.len();

        // Save to backups directory
        let bak_dir = self.config.data_dir.join("backups");
        tokio::fs::create_dir_all(&bak_dir).await?;
        let bak_path = full::backup_file_path(&self.config.data_dir, id);
        tokio::fs::write(&bak_path, &bytes)
            .await
            .context("Failed to write backup file")?;

        info!(id = %id, bucket = %bucket, size = %size, "Backup downloaded from S3");

        Ok(serde_json::json!({
            "downloaded": true,
            "id": id,
            "size_bytes": size,
        }))
    }

    /// Restore identity from an encrypted DID backup JSON.
    /// Params: { backup: { version, blob, ... }, passphrase }
    ///
    /// D-04 verdict: **gated.** This is unauthenticated
    /// (`middleware.rs:30`) and reaches
    /// `backup::identity::restore_encrypted_backup`, which writes
    /// `identity/node_key` unconditionally at `backup/identity.rs:113-117` —
    /// the same overwrite primitive F-01 names, behind a different door.
    /// Fixing `seed.restore` alone would have moved the door, not closed it.
    pub(super) async fn handle_backup_restore_identity(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        super::onboarding_gate::ensure_onboarding_open(&self.config.data_dir, &self.auth_manager)
            .await?;

        let backup = params
            .get("backup")
            .ok_or_else(|| anyhow::anyhow!("Missing 'backup' parameter"))?;
        let passphrase = params
            .get("passphrase")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'passphrase' parameter"))?;

        let identity_dir = self.config.data_dir.join("identity");
        let (did, pubkey) =
            crate::backup::restore_encrypted_backup(&identity_dir, backup, passphrase)
                .await
                .context("Identity restore failed")?;

        info!(did = %did, "Identity restored from backup");

        Ok(serde_json::json!({
            "did": did,
            "pubkey": pubkey,
        }))
    }
}
