// Authentication module for Archipelago
// Handles user setup, onboarding, and login

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

use crate::totp::TotpData;

/// User role for multi-user RBAC (Year 3 feature).
/// - Admin: full access to all operations
/// - Viewer: read-only dashboard, container status, monitoring
/// - AppUser: access specific apps, no system configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum UserRole {
    #[default]
    Admin,
    Viewer,
    AppUser,
}

impl UserRole {
    /// Check if this role allows a given RPC method.
    pub fn can_access(&self, method: &str) -> bool {
        match self {
            UserRole::Admin => true,
            UserRole::Viewer => {
                // Read-only system methods (explicit allowlist — NOT prefix "system."
                // which would grant access to system.factory-reset, system.shutdown, etc.)
                method == "system.stats"
                    || method == "system.processes"
                    || method == "system.temperature"
                    || method == "system.disk-status"
                    || method == "system.detect-usb-devices"
                    || method == "node.did"
                    || method == "node.tor-address"
                    || method == "node.nostr-pubkey"
                    || method.starts_with("federation.list")
                    || method.starts_with("dwn.status")
                    || method.starts_with("dwn.list")
                    || method.starts_with("dwn.query")
                    || method.starts_with("identity.list")
                    || method.starts_with("identity.get")
                    || method.starts_with("backup.list")
                    || method == "container-list"
                    || method == "container-status"
                    || method == "container-health"
                    || method == "health"
                    || method == "auth.logout"
            }
            UserRole::AppUser => {
                // App access + basic read
                method.starts_with("system.stats")
                    || method == "node.did"
                    || method == "container-list"
                    || method == "container-status"
                    || method == "health"
                    || method == "auth.logout"
                    || method == "auth.changePassword"
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnboardingState {
    complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub password_hash: String,
    pub setup_complete: bool,
    pub onboarding_complete: bool,
    #[serde(default)]
    pub totp: Option<TotpData>,
    /// User role for RBAC (defaults to Admin for backward compatibility)
    #[serde(default)]
    pub role: UserRole,
}

/// Cloneable: it holds only the data dir, and the app gate needs its own
/// handle to verify passwords on a different port from the JSON-RPC path.
#[derive(Clone)]
pub struct AuthManager {
    data_dir: PathBuf,
}

pub struct ChangePasswordOutcome {
    pub ssh_updated: bool,
    pub ssh_error: Option<String>,
}

impl AuthManager {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// Ensure a default user exists on first boot.
    /// Called once at startup — creates user with default password if none exists.
    #[allow(dead_code)]
    pub async fn ensure_default_user(&self) -> Result<()> {
        if self.is_setup().await? {
            return Ok(());
        }
        tracing::info!(
            "[onboarding] no user found — creating default user (password: password123)"
        );
        self.setup_user("password123").await?;
        tracing::info!(
            "[onboarding] default user created — user should change password after login"
        );
        Ok(())
    }

    pub async fn is_setup(&self) -> Result<bool> {
        let user_file = self.data_dir.join("user.json");
        Ok(user_file.exists())
    }

    pub async fn get_user(&self) -> Result<Option<User>> {
        let user_file = self.data_dir.join("user.json");
        if !user_file.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&user_file).await?;
        let user: User = serde_json::from_str(&content)?;
        Ok(Some(user))
    }

    pub async fn setup_user(&self, password: &str) -> Result<()> {
        let password_hash = argon2id_hash(password)?;

        // If onboarding was already completed (before setup), preserve that
        let onboarding_complete = self.is_onboarding_complete().await?;

        let user = User {
            password_hash,
            setup_complete: true,
            onboarding_complete,
            totp: None,
            role: UserRole::default(),
        };

        let user_file = self.data_dir.join("user.json");
        let content = serde_json::to_string_pretty(&user)?;
        fs::write(&user_file, content).await?;

        Ok(())
    }

    pub async fn complete_onboarding(&self) -> Result<()> {
        // Persist to onboarding.json (works even before user/setup exists)
        let onboarding_file = self.data_dir.join("onboarding.json");
        let state = OnboardingState { complete: true };
        fs::write(&onboarding_file, serde_json::to_string_pretty(&state)?).await?;
        // Also update user.json if it exists (keeps them in sync)
        if let Some(mut user) = self.get_user().await? {
            user.onboarding_complete = true;
            let user_file = self.data_dir.join("user.json");
            let content = serde_json::to_string_pretty(&user)?;
            fs::write(&user_file, content).await?;
        }
        Ok(())
    }

    /// Reset onboarding state so the user can go through onboarding again (dev/testing).
    pub async fn reset_onboarding(&self) -> Result<()> {
        let onboarding_file = self.data_dir.join("onboarding.json");
        let state = OnboardingState { complete: false };
        fs::write(&onboarding_file, serde_json::to_string_pretty(&state)?).await?;
        if let Some(mut user) = self.get_user().await? {
            user.onboarding_complete = false;
            let user_file = self.data_dir.join("user.json");
            let content = serde_json::to_string_pretty(&user)?;
            fs::write(&user_file, content).await?;
        }
        Ok(())
    }

    pub async fn is_onboarding_complete(&self) -> Result<bool> {
        // Check onboarding.json first (persisted before user setup)
        let onboarding_file = self.data_dir.join("onboarding.json");
        if onboarding_file.exists() {
            let content = fs::read_to_string(&onboarding_file).await?;
            if let Ok(state) = serde_json::from_str::<OnboardingState>(&content) {
                if state.complete {
                    return Ok(true);
                }
            }
        }
        // Fallback: user.json. A node that has a password set AND
        // setup_complete=true has been through onboarding by
        // definition — you can't reach the password-set step any
        // other way. The separate `onboarding_complete` flag can drift
        // out of sync (e.g. the completion RPC never reached disk, or
        // the node was seeded from a backup pre-dating the flag), so
        // auto-heal by inferring from setup_complete + password_hash.
        // Without this, a fully-onboarded node whose `onboarding_complete`
        // is stuck false will force its user back through the intro
        // wizard on every cleared browser cache.
        if let Some(u) = self.get_user().await? {
            if u.onboarding_complete {
                return Ok(true);
            }
            if u.setup_complete && !u.password_hash.is_empty() {
                // Persist the healed state so subsequent calls skip this
                // inference. Ignore write errors — returning true is
                // still correct even if we can't persist.
                let healed = OnboardingState { complete: true };
                if let Ok(json) = serde_json::to_string_pretty(&healed) {
                    let _ = fs::write(&onboarding_file, json).await;
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Check if 2FA is enabled for the user.
    pub async fn is_totp_enabled(&self) -> Result<bool> {
        Ok(self
            .get_user()
            .await?
            .map(|u| u.totp.is_some())
            .unwrap_or(false))
    }

    /// Get the TOTP data (if 2FA is enabled).
    pub async fn get_totp_data(&self) -> Result<Option<TotpData>> {
        Ok(self.get_user().await?.and_then(|u| u.totp))
    }

    /// Save TOTP data to user.json (enable 2FA).
    pub async fn save_totp(&self, totp_data: TotpData) -> Result<()> {
        let mut user = self
            .get_user()
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not set up"))?;
        user.totp = Some(totp_data);
        let user_file = self.data_dir.join("user.json");
        fs::write(&user_file, serde_json::to_string_pretty(&user)?).await?;
        Ok(())
    }

    /// Remove TOTP data from user.json (disable 2FA).
    pub async fn remove_totp(&self) -> Result<()> {
        let mut user = self
            .get_user()
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not set up"))?;
        user.totp = None;
        let user_file = self.data_dir.join("user.json");
        fs::write(&user_file, serde_json::to_string_pretty(&user)?).await?;
        Ok(())
    }

    /// Update TOTP data in place (e.g. after consuming a backup code or recording a used step).
    pub async fn update_totp(&self, totp_data: TotpData) -> Result<()> {
        self.save_totp(totp_data).await
    }

    pub async fn verify_password(&self, password: &str) -> Result<bool> {
        if let Some(user) = self.get_user().await? {
            // Detect hash format and verify accordingly
            if user.password_hash.starts_with("$2") {
                // Legacy bcrypt hash — verify then auto-upgrade to Argon2id
                let valid = bcrypt::verify(password, &user.password_hash)?;
                if valid {
                    // Transparent upgrade: re-hash with Argon2id on successful login
                    let new_hash = argon2id_hash(password)?;
                    let mut upgraded = user.clone();
                    upgraded.password_hash = new_hash;
                    let user_file = self.data_dir.join("user.json");
                    fs::write(&user_file, serde_json::to_string_pretty(&upgraded)?).await?;
                    tracing::info!("Upgraded password hash from bcrypt to Argon2id");
                }
                Ok(valid)
            } else {
                // Argon2id hash (PHC string format: $argon2id$...)
                Ok(argon2id_verify(password, &user.password_hash))
            }
        } else {
            Ok(false)
        }
    }

    /// Change password: verify current, validate new, update user.json and optionally SSH.
    /// New password must be 12+ chars with upper, lower, digit, and special character.
    pub async fn change_password(
        &self,
        current_password: &str,
        new_password: &str,
        also_change_ssh: bool,
    ) -> Result<ChangePasswordOutcome> {
        if !self.verify_password(current_password).await? {
            anyhow::bail!("Current password is incorrect");
        }

        validate_password_strength(new_password)?;

        let password_hash = argon2id_hash(new_password)?;

        let mut user = self
            .get_user()
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not set up"))?;

        user.password_hash = password_hash;

        // Re-encrypt TOTP MEK under new password if 2FA is enabled
        if let Some(ref totp_data) = user.totp {
            let rekeyed = crate::totp::rekey(totp_data, current_password, new_password)?;
            user.totp = Some(rekeyed);
        }

        let user_file = self.data_dir.join("user.json");
        let content = serde_json::to_string_pretty(&user)?;
        fs::write(&user_file, content).await?;

        let mut outcome = ChangePasswordOutcome {
            ssh_updated: false,
            ssh_error: None,
        };
        if also_change_ssh {
            match change_ssh_password(new_password).await {
                Ok(()) => outcome.ssh_updated = true,
                Err(e) => {
                    tracing::warn!("Web password changed but SSH password update failed: {}", e);
                    outcome.ssh_error = Some(e.to_string());
                }
            }
        }

        Ok(outcome)
    }
}

/// Validate password strength: 12+ chars, upper, lower, digit, special.
fn validate_password_strength(password: &str) -> Result<()> {
    if password.len() < 12 {
        anyhow::bail!("Password must be at least 12 characters");
    }
    if !password.chars().any(|c| c.is_ascii_uppercase()) {
        anyhow::bail!("Password must contain at least one uppercase letter");
    }
    if !password.chars().any(|c| c.is_ascii_lowercase()) {
        anyhow::bail!("Password must contain at least one lowercase letter");
    }
    if !password.chars().any(|c| c.is_ascii_digit()) {
        anyhow::bail!("Password must contain at least one digit");
    }
    if !password.chars().any(|c| !c.is_ascii_alphanumeric()) {
        anyhow::bail!("Password must contain at least one special character (!@#$%^&* etc.)");
    }
    Ok(())
}

/// Change the archipelago user's SSH/login password.
/// Uses usermod + openssl to bypass PAM (avoids "Authentication token manipulation" errors).
/// Uses absolute paths (/usr/bin/openssl, /usr/sbin/usermod) for systemd's minimal PATH.
pub(crate) async fn change_ssh_password(new_password: &str) -> Result<()> {
    let ssh_user =
        std::env::var("ARCHIPELAGO_SSH_USER").unwrap_or_else(|_| "archipelago".to_string());

    // Generate crypt hash via openssl (SHA-512, compatible with /etc/shadow)
    // Use /usr/bin/openssl - systemd services often have minimal PATH
    let mut hash_child = tokio::process::Command::new("/usr/bin/openssl")
        .args(["passwd", "-6", "-stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to run openssl: {}. Is openssl installed?", e))?;

    {
        use tokio::io::AsyncWriteExt;
        let mut stdin = hash_child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to open openssl stdin"))?;
        stdin.write_all(new_password.as_bytes()).await?;
        stdin.flush().await?;
    }

    let hash_result = hash_child.wait_with_output().await?;
    if !hash_result.status.success() {
        let stderr = String::from_utf8_lossy(&hash_result.stderr);
        anyhow::bail!("openssl passwd failed: {}", stderr);
    }
    let hash = String::from_utf8(hash_result.stdout)?.trim().to_string();
    if hash.is_empty() {
        anyhow::bail!("openssl passwd produced empty hash");
    }

    // usermod -p writes directly to /etc/shadow, bypassing PAM
    // Use /usr/sbin/usermod - not always in systemd's PATH
    let status = tokio::process::Command::new("/usr/bin/sudo")
        .args(["-n", "/usr/sbin/usermod", "-p", &hash, &ssh_user])
        .output()
        .await?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        anyhow::bail!("sudo usermod failed: {}", stderr);
    }

    tracing::info!("SSH password updated for user {}", ssh_user);
    Ok(())
}

/// Hash a password with Argon2id (memory-hard, GPU/ASIC resistant).
/// Uses PHC string format ($argon2id$v=19$m=65536,t=3,p=4$...) for self-describing storage.
fn argon2id_hash(password: &str) -> Result<String> {
    use argon2::password_hash::SaltString;
    use argon2::{Argon2, Params, PasswordHasher};
    use rand::rngs::OsRng;

    let salt = SaltString::generate(&mut OsRng);
    let params = Params::new(65536, 3, 4, Some(32))
        .map_err(|e| anyhow::anyhow!("Invalid Argon2 params: {}", e))?;
    let hasher = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let hash = hasher
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Argon2id hash failed: {}", e))?;
    Ok(hash.to_string())
}

/// Verify a password against an Argon2id PHC string hash.
fn argon2id_verify(password: &str, hash: &str) -> bool {
    use argon2::password_hash::PasswordHash;
    use argon2::{Argon2, PasswordVerifier};

    let parsed = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_setup_user_and_verify_password() {
        let dir = tempfile::tempdir().unwrap();
        let auth = AuthManager::new(dir.path().to_path_buf());

        assert!(!auth.is_setup().await.unwrap());

        auth.setup_user("password123").await.unwrap();

        assert!(auth.is_setup().await.unwrap());
        assert!(auth.verify_password("password123").await.unwrap());
        assert!(!auth.verify_password("wrong").await.unwrap());
    }

    #[tokio::test]
    async fn test_verify_password_no_user() {
        let dir = tempfile::tempdir().unwrap();
        let auth = AuthManager::new(dir.path().to_path_buf());

        assert!(!auth.verify_password("anything").await.unwrap());
    }

    #[tokio::test]
    async fn test_onboarding_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let auth = AuthManager::new(dir.path().to_path_buf());

        assert!(!auth.is_onboarding_complete().await.unwrap());

        auth.complete_onboarding().await.unwrap();
        assert!(auth.is_onboarding_complete().await.unwrap());

        auth.reset_onboarding().await.unwrap();
        assert!(!auth.is_onboarding_complete().await.unwrap());
    }

    #[tokio::test]
    async fn test_onboarding_persists_to_user() {
        let dir = tempfile::tempdir().unwrap();
        let auth = AuthManager::new(dir.path().to_path_buf());

        auth.setup_user("password123").await.unwrap();
        let user = auth.get_user().await.unwrap().unwrap();
        assert!(!user.onboarding_complete);

        auth.complete_onboarding().await.unwrap();
        let user = auth.get_user().await.unwrap().unwrap();
        assert!(user.onboarding_complete);
    }

    #[test]
    fn test_validate_password_strength_valid() {
        assert!(validate_password_strength("MyP@ssw0rd!123").is_ok());
    }

    #[tokio::test]
    async fn test_change_password_updates_web_password_without_ssh() {
        let dir = tempfile::tempdir().unwrap();
        let auth = AuthManager::new(dir.path().to_path_buf());
        auth.setup_user("password123").await.unwrap();

        let outcome = auth
            .change_password("password123", "MyP@ssw0rd!123", false)
            .await
            .unwrap();

        assert!(!outcome.ssh_updated);
        assert!(outcome.ssh_error.is_none());
        assert!(auth.verify_password("MyP@ssw0rd!123").await.unwrap());
        assert!(!auth.verify_password("password123").await.unwrap());
    }

    #[test]
    fn test_validate_password_strength_too_short() {
        assert!(validate_password_strength("Ab1!").is_err());
    }

    #[test]
    fn test_validate_password_strength_no_uppercase() {
        assert!(validate_password_strength("mypassword1!xx").is_err());
    }

    #[test]
    fn test_validate_password_strength_no_digit() {
        assert!(validate_password_strength("MyPassword!!xx").is_err());
    }

    #[test]
    fn test_validate_password_strength_no_special() {
        assert!(validate_password_strength("MyPassword1234").is_err());
    }
}
