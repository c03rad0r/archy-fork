use super::RpcHandler;
use anyhow::Result;

impl RpcHandler {
    /// Begin 2FA setup: generate TOTP secret, return QR code + base32 secret.
    /// The secret is cached in a pending setup session (in memory only).
    pub(super) async fn handle_totp_setup_begin(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let password = params
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing password"))?;

        // Re-verify password before setup
        if !self.auth_manager.verify_password(password).await? {
            anyhow::bail!("Password Incorrect");
        }

        // Check 2FA isn't already enabled
        if self.auth_manager.is_totp_enabled().await? {
            anyhow::bail!("2FA is already enabled. Disable it first.");
        }

        let setup = crate::totp::setup(password)?;

        // Cache the setup result in a pending session so confirm can use it
        // We store the encrypted TotpData and backup codes temporarily
        let setup_json = serde_json::json!({
            "totp_data": setup.totp_data,
            "backup_codes": setup.backup_codes,
        });
        let setup_bytes = serde_json::to_vec(&setup_json)?;
        let pending_token = self.session_store.create_pending(setup_bytes).await;

        // Return QR + secret for display (the pending token is set as a cookie by mod.rs)
        Ok(serde_json::json!({
            "qr_svg": setup.qr_svg,
            "secret_base32": setup.secret_base32,
            "pending_token": pending_token,
        }))
    }

    /// Confirm 2FA setup: verify the user's first TOTP code, enable 2FA, return backup codes.
    pub(super) async fn handle_totp_setup_confirm(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let code = params
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing code"))?;
        let pending_token = params
            .get("pendingToken")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing pendingToken"))?;
        let password = params
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing password"))?;

        // Re-verify password
        if !self.auth_manager.verify_password(password).await? {
            anyhow::bail!("Password Incorrect");
        }

        // Retrieve the pending setup data
        let setup_bytes = self
            .session_store
            .get_pending_secret(pending_token)
            .await
            .ok_or_else(|| {
                anyhow::anyhow!("Setup session expired or invalid. Please start again.")
            })?;

        let setup_json: serde_json::Value = serde_json::from_slice(&setup_bytes)?;
        let totp_data: crate::totp::TotpData =
            serde_json::from_value(setup_json["totp_data"].clone())?;
        let backup_codes: Vec<String> = serde_json::from_value(setup_json["backup_codes"].clone())?;

        // Decrypt and verify the TOTP code
        let secret = crate::totp::decrypt_secret(&totp_data, password)?;
        let step = crate::totp::verify_code(&secret, code, &[])?;
        if step.is_none() {
            anyhow::bail!("Invalid code. Please check your authenticator app and try again.");
        }

        // Persist TOTP data
        self.auth_manager.save_totp(totp_data).await?;

        // Clean up the pending session
        self.session_store.remove(pending_token).await;

        tracing::info!("2FA enabled successfully");

        Ok(serde_json::json!({
            "enabled": true,
            "backup_codes": backup_codes,
        }))
    }

    /// Disable 2FA. Requires password + current TOTP code.
    pub(super) async fn handle_totp_disable(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let password = params
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing password"))?;
        let code = params
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing code"))?;

        // Verify password
        if !self.auth_manager.verify_password(password).await? {
            anyhow::bail!("Password Incorrect");
        }

        // Get and verify TOTP
        let totp_data = self
            .auth_manager
            .get_totp_data()
            .await?
            .ok_or_else(|| anyhow::anyhow!("2FA is not enabled"))?;
        let secret = crate::totp::decrypt_secret(&totp_data, password)?;
        let step = crate::totp::verify_code(&secret, code, &totp_data.used_steps)?;
        if step.is_none() {
            anyhow::bail!("Invalid TOTP code");
        }

        self.auth_manager.remove_totp().await?;
        tracing::info!("2FA disabled successfully");

        Ok(serde_json::json!({ "disabled": true }))
    }

    /// Get 2FA status.
    pub(super) async fn handle_totp_status(&self) -> Result<serde_json::Value> {
        let enabled = self.auth_manager.is_totp_enabled().await?;
        Ok(serde_json::json!({ "enabled": enabled }))
    }

    /// Step 2 of login: verify TOTP code using the cached secret from the pending session.
    pub(super) async fn handle_login_totp(
        &self,
        params: Option<serde_json::Value>,
        session_token: &Option<String>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let code = params
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing code"))?;

        let token = session_token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No pending session"))?;

        let secret = self
            .session_store
            .get_pending_secret(token)
            .await
            .ok_or_else(|| {
                anyhow::anyhow!("Session expired or too many attempts. Please log in again.")
            })?;

        // Get used steps from stored data for replay protection
        let totp_data = self.auth_manager.get_totp_data().await?;
        let used_steps = totp_data
            .as_ref()
            .map(|d| d.used_steps.clone())
            .unwrap_or_default();

        let step = crate::totp::verify_code(&secret, code, &used_steps)?;
        match step {
            Some(used_step) => {
                // Record the used step for replay protection
                if let Some(mut data) = totp_data {
                    data.used_steps.push(used_step);
                    // Prune old steps (keep only last 5 minutes worth)
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64;
                    let cutoff = (now / 30) - 10; // ~5 minutes
                    data.used_steps.retain(|s| *s > cutoff);
                    let _ = self.auth_manager.update_totp(data).await;
                }

                // Upgrade pending session to full (rotates token)
                let new_token = self
                    .session_store
                    .upgrade_to_full(token)
                    .await
                    .ok_or_else(|| anyhow::anyhow!("Session expired. Please log in again."))?;

                Ok(serde_json::json!({ "success": true, "new_session_token": new_token }))
            }
            None => {
                anyhow::bail!("Invalid code. Please try again.");
            }
        }
    }

    /// Step 2 of login (alternative): verify backup code.
    pub(super) async fn handle_login_backup(
        &self,
        params: Option<serde_json::Value>,
        session_token: &Option<String>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let code = params
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing code"))?;

        let token = session_token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No pending session"))?;

        // Verify the pending session is valid (increments attempts)
        let _secret = self
            .session_store
            .get_pending_secret(token)
            .await
            .ok_or_else(|| {
                anyhow::anyhow!("Session expired or too many attempts. Please log in again.")
            })?;

        // Verify backup code against stored hashes
        let mut totp_data = self
            .auth_manager
            .get_totp_data()
            .await?
            .ok_or_else(|| anyhow::anyhow!("2FA is not enabled"))?;

        match crate::totp::verify_backup_code(&totp_data.backup_codes, code)? {
            Some(idx) => {
                // Remove the used backup code (one-time use)
                totp_data.backup_codes.remove(idx);
                self.auth_manager.update_totp(totp_data).await?;

                // Upgrade pending session to full (rotates token)
                let new_token = self
                    .session_store
                    .upgrade_to_full(token)
                    .await
                    .ok_or_else(|| anyhow::anyhow!("Session expired. Please log in again."))?;

                tracing::info!(
                    "Login via backup code (codes remaining: {})",
                    self.auth_manager
                        .get_totp_data()
                        .await?
                        .map(|d| d.backup_codes.len())
                        .unwrap_or(0)
                );

                Ok(serde_json::json!({ "success": true, "new_session_token": new_token }))
            }
            None => {
                anyhow::bail!("Invalid backup code");
            }
        }
    }
}
