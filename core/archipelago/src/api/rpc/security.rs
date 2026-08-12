use super::package::validate_app_id;
use super::RpcHandler;
use anyhow::Result;

impl RpcHandler {
    pub(super) async fn handle_security_rotate_secrets(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let app_id = params
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing app_id"))?;
        validate_app_id(app_id)?;

        let secrets_dir = self.config.data_dir.join("secrets");
        let encryption_key = self.get_secrets_key();
        let mgr = archipelago_security::SecretsManager::new(secrets_dir, encryption_key)?;

        let secret_ids = mgr.list_secrets(app_id).await?;
        let mut rotated = Vec::new();

        for secret_id in &secret_ids {
            mgr.rotate_secret(app_id, secret_id).await?;
            rotated.push(secret_id.clone());
        }

        Ok(serde_json::json!({
            "app_id": app_id,
            "rotated_count": rotated.len(),
            "rotated_ids": rotated,
        }))
    }

    pub(super) async fn handle_security_list_expiring(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let max_age_days = params
            .get("max_age_days")
            .and_then(|v| v.as_i64())
            .unwrap_or(90);

        let secrets_dir = self.config.data_dir.join("secrets");
        let encryption_key = self.get_secrets_key();
        let mgr = archipelago_security::SecretsManager::new(secrets_dir, encryption_key)?;

        let expiring = mgr.list_expiring(max_age_days).await?;

        Ok(serde_json::json!({
            "max_age_days": max_age_days,
            "expiring_count": expiring.len(),
            "secrets": expiring,
        }))
    }

    /// Derive a 32-byte encryption key for secrets.
    /// Uses a fixed derivation from the data directory path as a stable key.
    fn get_secrets_key(&self) -> Vec<u8> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"archipelago-secrets-v1-");
        hasher.update(self.config.data_dir.to_string_lossy().as_bytes());
        hasher.finalize().to_vec()
    }
}
