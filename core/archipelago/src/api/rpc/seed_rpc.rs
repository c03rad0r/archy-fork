//! RPC handlers for BIP-39 seed management.
//! Endpoints: seed.generate, seed.verify, seed.restore, seed.save-encrypted, seed.status

use super::RpcHandler;
use anyhow::{Context, Result};
use nostr_sdk::ToBech32;
use std::sync::Arc;
use tokio::sync::Mutex;
use zeroize::Zeroize;

/// In-memory storage for the mnemonic between generate and verify steps.
/// Auto-cleared after 10 minutes.
static ONBOARDING_MNEMONIC: std::sync::LazyLock<Arc<Mutex<Option<OnboardingMnemonicState>>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(None)));

struct OnboardingMnemonicState {
    words: String,
    created_at: std::time::Instant,
}

impl Drop for OnboardingMnemonicState {
    fn drop(&mut self) {
        self.words.zeroize();
    }
}

const MNEMONIC_TTL: std::time::Duration = std::time::Duration::from_secs(600); // 10 minutes

/// Persist the pending onboarding mnemonic as `identity/master_seed.enc`,
/// encrypted with `passphrase`. Called from `auth.setup` — the first moment a
/// user password exists — so "Reveal recovery phrase" works after onboarding
/// without the frontend having to remember a separate save step (it never
/// did, which left every onboarded node with no encrypted seed backup).
///
/// Deliberately ignores MNEMONIC_TTL: the mnemonic stays in memory until
/// overwritten regardless, so using it here widens nothing, and onboarding
/// legitimately takes longer than 10 minutes when the user carefully writes
/// down 24 words. Clears the in-memory copy on success — password setup is
/// the end of onboarding, so the plaintext no longer needs to linger.
///
/// Returns Ok(true) if a seed was saved, Ok(false) if none was pending.
pub(in crate::api::rpc) async fn save_pending_seed_encrypted(
    data_dir: &std::path::Path,
    passphrase: &str,
) -> Result<bool> {
    let mut state = ONBOARDING_MNEMONIC.lock().await;
    let Some(pending) = state.as_ref() else {
        return Ok(false);
    };
    let mnemonic: bip39::Mnemonic = pending
        .words
        .parse()
        .context("Invalid mnemonic in memory")?;
    crate::seed::save_seed_encrypted(data_dir, &mnemonic, passphrase).await?;
    *state = None;
    Ok(true)
}

/// Best-effort: install fips.yaml + start the available FIPS systemd unit after
/// seed onboarding has written the fips_key to disk. Runs in a detached task so
/// the user-facing RPC returns immediately — the systemctl calls can take a few
/// seconds the first time on slow hardware. Any failure is logged but does not
/// break onboarding; the user can still hit fips.install manually from the
/// dashboard as an escape hatch.
fn spawn_post_onboarding_fips_activate(data_dir: std::path::PathBuf) {
    tokio::spawn(async move {
        let identity_dir = data_dir.join("identity");
        if !crate::identity::fips_key_exists(&identity_dir) {
            return;
        }
        // Touch load_fips_keys first so any legacy raw-byte file is migrated
        // to bech32 before we copy it into /etc/fips/.
        if let Err(e) = crate::identity::load_fips_keys(&identity_dir).await {
            tracing::warn!("post-onboarding fips key load/migrate failed: {}", e);
            return;
        }
        if let Err(e) = crate::fips::config::install(&identity_dir).await {
            tracing::warn!("post-onboarding fips config install failed: {}", e);
            return;
        }
        let unit = crate::fips::service::activation_unit().await;
        if let Err(e) = crate::fips::service::activate(unit).await {
            tracing::warn!("post-onboarding FIPS activate failed via {}: {}", unit, e);
            return;
        }
        tracing::info!("FIPS auto-activated post-onboarding via {}", unit);
    });
}

/// Restore the node's identity from a 24-word BIP-39 mnemonic.
///
/// This is the production body of `seed.restore`, extracted out of
/// `RpcHandler` so the F-01 regression suite can drive the real path against a
/// temp data dir without constructing an `RpcHandler` (which needs an
/// orchestrator, port allocator, session store and metrics store).
///
/// **The gate is the first statement and must stay there.** `seed.restore` is
/// in `UNAUTHENTICATED_METHODS`, and everything below this line overwrites
/// `identity/node_key`, `identity/nostr_secret` and the FIPS transport key
/// unconditionally (`identity.rs:79-114`). Without the gate, one unauthenticated
/// POST with an attacker-chosen mnemonic hijacks a live node — F-01, Critical.
pub(in crate::api::rpc) async fn restore_node_identity_from_words(
    data_dir: &std::path::Path,
    auth: &crate::auth::AuthManager,
    words: &[String],
) -> Result<serde_json::Value> {
    super::onboarding_gate::ensure_onboarding_open(data_dir, auth).await?;

    let phrase = words.join(" ");
    let (_mnemonic, seed) = crate::seed::MasterSeed::from_mnemonic_words(&phrase)?;

    // Stash the restored words like seed.generate does, so auth.setup can
    // persist the encrypted backup once the user's password exists and
    // "Reveal recovery phrase" works on restored nodes too.
    {
        let mut state = ONBOARDING_MNEMONIC.lock().await;
        *state = Some(OnboardingMnemonicState {
            words: phrase.clone(),
            created_at: std::time::Instant::now(),
        });
    }

    // Derive and write node Ed25519 key.
    let identity_dir = data_dir.join("identity");
    crate::identity::NodeIdentity::from_seed(&identity_dir, &seed).await?;

    // Derive and write node-level Nostr key.
    let nostr_keys = crate::seed::derive_node_nostr_key(&seed)?;
    let secret_hex = nostr_keys.secret_key().display_secret().to_string();
    let pubkey_hex_nostr = nostr_keys.public_key().to_hex();
    tokio::fs::write(identity_dir.join("nostr_secret"), secret_hex.as_bytes()).await?;
    tokio::fs::write(
        identity_dir.join("nostr_pubkey"),
        pubkey_hex_nostr.as_bytes(),
    )
    .await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(
            identity_dir.join("nostr_secret"),
            std::fs::Permissions::from_mode(0o600),
        )
        .await?;
    }

    // Initialize identity index.
    crate::seed::save_identity_index(data_dir, 0).await?;

    // Create default identity from seed.
    let manager = crate::identity_manager::IdentityManager::new(data_dir).await?;
    manager
        .create_from_seed(
            "Personal".to_string(),
            crate::identity_manager::IdentityPurpose::Personal,
            &seed,
            data_dir,
        )
        .await?;

    // Get DID and npub for the response.
    let node_key = crate::seed::derive_node_ed25519(&seed)?;
    let pubkey_hex = hex::encode(node_key.verifying_key().as_bytes());
    let did = crate::identity::did_key_from_pubkey_hex(&pubkey_hex)?;
    let nostr_npub = nostr_keys.public_key().to_bech32().unwrap_or_default();

    // Same as seed.generate: the key is materialised, kick the FIPS
    // service up without user interaction.
    spawn_post_onboarding_fips_activate(data_dir.to_path_buf());

    Ok(serde_json::json!({
        "did": did,
        "nostr_npub": nostr_npub,
        "restored": true,
    }))
}

impl RpcHandler {
    /// Generate a new 24-word BIP-39 mnemonic, derive and persist node keys.
    /// Returns the words for the user to write down.
    pub(in crate::api::rpc) async fn handle_seed_generate(&self) -> Result<serde_json::Value> {
        // Gate BEFORE the lock and before the idempotent fast path. Ordering is
        // load-bearing in both directions:
        //   * Gate-first is REQUIRED because the fast path below returns the 24
        //     words to an unauthenticated caller. On a provisioned node whose
        //     in-memory mnemonic happened to survive (the encrypted save inside
        //     auth.setup is best-effort and can fail), the fast path is itself a
        //     disclosure — T-10-07.
        //   * Gate-first is SAFE for onboarding because all three gate signals
        //     are false throughout the seed steps: `auth.setup` runs at the very
        //     END of the wizard (router order: onboarding/seed →
        //     onboarding/seed-verify → onboarding/identity → onboarding/done →
        //     /login, where views/Login.vue:405-425 posts auth.setup).
        super::onboarding_gate::ensure_onboarding_open(&self.config.data_dir, &self.auth_manager)
            .await?;

        // Serialize concurrent / retried generate calls. The web client aborts
        // at 15s and retries internally (up to 3x), and the onboarding view
        // re-fires every 4s while the server is still booting on slow first-boot
        // hardware. Without this guard each hit would mint a brand-new seed and
        // overwrite the node keys mid-flight, leaving the words shown to the user
        // out of sync with what `seed.verify` expects — the classic "error at the
        // DID-creation screen". Holding the lock across the whole op fully
        // serializes them.
        let mut state = ONBOARDING_MNEMONIC.lock().await;

        // Idempotent fast-path: a fresh pending mnemonic already exists, so the
        // node keys are already on disk. Return the SAME words rather than
        // regenerating, so every retry yields a consistent result.
        if let Some(existing) = state.as_ref() {
            if existing.created_at.elapsed() < MNEMONIC_TTL {
                let words: Vec<String> = existing
                    .words
                    .split_whitespace()
                    .map(str::to_string)
                    .collect();
                return Ok(serde_json::json!({ "words": words }));
            }
        }

        let (mnemonic, seed) = crate::seed::MasterSeed::generate()?;

        // Derive and write node Ed25519 key.
        let identity_dir = self.config.data_dir.join("identity");
        crate::identity::NodeIdentity::from_seed(&identity_dir, &seed).await?;

        // Derive and write node-level Nostr key.
        let nostr_keys = crate::seed::derive_node_nostr_key(&seed)?;
        let nostr_secret_path = identity_dir.join("nostr_secret");
        let nostr_pub_path = identity_dir.join("nostr_pubkey");
        let secret_hex = nostr_keys.secret_key().display_secret().to_string();
        let pubkey_hex = nostr_keys.public_key().to_hex();
        tokio::fs::write(&nostr_secret_path, secret_hex.as_bytes()).await?;
        tokio::fs::write(&nostr_pub_path, pubkey_hex.as_bytes()).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&nostr_secret_path, std::fs::Permissions::from_mode(0o600))
                .await?;
        }

        // Initialize identity index at 0.
        crate::seed::save_identity_index(&self.config.data_dir, 0).await?;

        // fips_key is now on disk — auto-activate FIPS so the user doesn't
        // have to hit a manual Start button. Detached task;
        // the onboarding RPC returns immediately.
        spawn_post_onboarding_fips_activate(self.config.data_dir.clone());

        let words: Vec<String> = mnemonic.words().map(str::to_string).collect();

        // Hold mnemonic in memory for the verify step. We already own the lock
        // guard (`state`) from the top of the function, so just write through it.
        *state = Some(OnboardingMnemonicState {
            words: mnemonic.to_string(),
            created_at: std::time::Instant::now(),
        });

        Ok(serde_json::json!({
            "words": words,
        }))
    }

    /// Verify the user wrote down their seed correctly.
    /// Also confirms the mnemonic by re-deriving and returning DID + npub.
    ///
    /// D-04 verdict: **deliberately NOT gated.** It compares the submitted
    /// words against the in-memory copy and re-derives a DID and npub for
    /// display — it writes no file and mutates no identity (contrast
    /// `handle_seed_restore`, which calls `NodeIdentity::from_seed`). Leaving
    /// it open costs nothing, and gating it would break a legitimate retry:
    /// the view re-submits after a 15s client abort.
    pub(in crate::api::rpc) async fn handle_seed_verify(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let submitted_words: Vec<String> = serde_json::from_value(
            params
                .get("words")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing words"))?,
        )
        .context("Invalid words array")?;

        // Validate against the held mnemonic.
        let mnemonic_str = {
            let mut state = ONBOARDING_MNEMONIC.lock().await;
            match state.as_ref() {
                Some(s) if s.created_at.elapsed() < MNEMONIC_TTL => s.words.clone(),
                _ => {
                    *state = None;
                    anyhow::bail!(
                        "No pending seed generation or session expired. Please regenerate."
                    );
                }
            }
        };

        let expected_words: Vec<&str> = mnemonic_str.split_whitespace().collect();
        let submitted: Vec<&str> = submitted_words.iter().map(|s| s.as_str()).collect();
        if expected_words != submitted {
            anyhow::bail!("Submitted words do not match generated seed");
        }

        // Re-derive to get DID and npub.
        let (mnemonic, seed) = crate::seed::MasterSeed::from_mnemonic_words(&mnemonic_str)?;
        let node_key = crate::seed::derive_node_ed25519(&seed)?;
        let pubkey_hex = hex::encode(node_key.verifying_key().as_bytes());
        let did = crate::identity::did_key_from_pubkey_hex(&pubkey_hex)?;

        let nostr_keys = crate::seed::derive_node_nostr_key(&seed)?;
        let nostr_npub = nostr_keys.public_key().to_bech32().unwrap_or_default();

        // Intentionally DO NOT clear the mnemonic here. The web client aborts
        // slow requests at 15s and retries internally; if we wiped it on the
        // first (successful) verify, a retried request would fail with
        // "No pending seed generation or session expired" even though the user
        // did everything right. The mnemonic is bounded by MNEMONIC_TTL (10 min)
        // and is overwritten on the next generate, so leaving it makes verify
        // idempotent without meaningfully widening the in-memory window.

        // Save the encrypted seed for convenience backup.
        // Use empty passphrase placeholder — the real encrypted save happens via seed.save-encrypted.
        // For now we just confirm the mnemonic was valid.
        drop(mnemonic);

        Ok(serde_json::json!({
            "verified": true,
            "did": did,
            "nostr_npub": nostr_npub,
        }))
    }

    /// Restore node identity from a 24-word seed phrase.
    ///
    /// Thin wrapper: parses `params.words` and delegates to
    /// [`restore_node_identity_from_words`], which carries the onboarding gate
    /// and the whole restore body.
    pub(in crate::api::rpc) async fn handle_seed_restore(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let words: Vec<String> = serde_json::from_value(
            params
                .get("words")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing words"))?,
        )
        .context("Invalid words array")?;

        restore_node_identity_from_words(&self.config.data_dir, &self.auth_manager, &words).await
    }

    /// Encrypt and save the mnemonic to disk for convenience backup.
    ///
    /// D-04 note: this method has no UI caller today —
    /// `neode-ui/src/api/rpc-client.ts:334` exposes it, but no view calls it.
    /// The encrypted save that actually happens during onboarding is
    /// `save_pending_seed_encrypted`, called from INSIDE `auth.setup`
    /// (`api/rpc/auth.rs:239`) once a passphrase exists; that call is behind
    /// `auth.setup`'s own gate and is therefore not gated again here.
    pub(in crate::api::rpc) async fn handle_seed_save_encrypted(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        super::onboarding_gate::ensure_onboarding_open(&self.config.data_dir, &self.auth_manager)
            .await?;

        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let passphrase = params
            .get("passphrase")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing passphrase"))?;

        // Try to get mnemonic from in-memory state first.
        let mnemonic_str = {
            let state = ONBOARDING_MNEMONIC.lock().await;
            state
                .as_ref()
                .filter(|s| s.created_at.elapsed() < MNEMONIC_TTL)
                .map(|s| s.words.clone())
        };

        let mnemonic: bip39::Mnemonic = if let Some(words) = mnemonic_str {
            words.parse().context("Invalid mnemonic in memory")?
        } else {
            anyhow::bail!("No mnemonic available. Generate or restore a seed first.");
        };

        crate::seed::save_seed_encrypted(&self.config.data_dir, &mnemonic, passphrase).await?;

        Ok(serde_json::json!({ "saved": true }))
    }

    /// Return seed status information.
    pub(in crate::api::rpc) async fn handle_seed_status(&self) -> Result<serde_json::Value> {
        let has_seed = crate::seed::seed_exists(&self.config.data_dir);
        let has_node_key =
            crate::identity::NodeIdentity::key_exists(&self.config.data_dir.join("identity"));
        let is_legacy = has_node_key && !has_seed;
        let next_index = crate::seed::load_identity_index(&self.config.data_dir)
            .await
            .unwrap_or(0);

        let manager = crate::identity_manager::IdentityManager::new(&self.config.data_dir).await?;
        let (identities, _) = manager.list().await?;

        Ok(serde_json::json!({
            "has_seed": has_seed,
            "is_legacy": is_legacy,
            "identity_count": identities.len(),
            "next_index": next_index,
        }))
    }

    /// Reveal the node's 24-word recovery phrase after onboarding. Heavily
    /// gated, because this is the keys to the whole node:
    ///   - requires a full authenticated session (enforced upstream: this
    ///     method is NOT in the public auth whitelist),
    ///   - re-verifies the login password,
    ///   - requires a valid TOTP code when 2FA is enabled (replay-protected),
    ///   - decrypts `identity/master_seed.enc` with the backup passphrase
    ///     (defaults to the login password when the user used the same value).
    /// The words are returned to the caller only and never logged.
    pub(in crate::api::rpc) async fn handle_seed_reveal(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();

        // Nothing to reveal if this node never stored an encrypted seed.
        if !crate::seed::seed_exists(&self.config.data_dir) {
            anyhow::bail!(
                "This node has no encrypted seed backup, so the recovery phrase \
                 cannot be shown. It was only displayed once during setup."
            );
        }

        let mut password = self
            .verify_reveal_auth(&params, "the recovery phrase")
            .await?;

        // 3) Decrypt the stored seed. The backup passphrase may differ from the
        //    login password, so accept an explicit one and fall back to the
        //    password when the user used the same value for both.
        let passphrase = params
            .get("passphrase")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let secret_phrase = passphrase.unwrap_or_else(|| password.clone());
        let reveal = crate::seed::load_seed_encrypted(&self.config.data_dir, &secret_phrase).await;
        password.zeroize();
        let mnemonic = reveal.map_err(|_| {
            anyhow::anyhow!(
                "Could not decrypt the saved seed. If you set a separate backup \
                 passphrase during setup, enter that passphrase."
            )
        })?;

        let words: Vec<String> = mnemonic.words().map(|w| w.to_string()).collect();
        let word_count = words.len();
        Ok(serde_json::json!({ "words": words, "word_count": word_count }))
    }

    /// Re-authenticate a sensitive reveal: verify the login password from
    /// `params.password` and, when 2FA is enabled, require a valid
    /// replay-protected TOTP code from `params.code`. Returns the verified
    /// password (some callers also use it as a decryption passphrase); the
    /// caller must zeroize it. `what` names the secret in error messages,
    /// e.g. "the recovery phrase".
    pub(in crate::api::rpc) async fn verify_reveal_auth(
        &self,
        params: &serde_json::Value,
        what: &str,
    ) -> Result<String> {
        let mut password = params
            .get("password")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if password.is_empty() {
            anyhow::bail!("Password is required to reveal {what}");
        }

        if !self.auth_manager.verify_password(&password).await? {
            password.zeroize();
            anyhow::bail!("Incorrect password");
        }

        if self.auth_manager.is_totp_enabled().await.unwrap_or(false) {
            let code = params
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if code.is_empty() {
                password.zeroize();
                anyhow::bail!("A 2FA code is required to reveal {what}");
            }
            let totp_data = self
                .auth_manager
                .get_totp_data()
                .await?
                .ok_or_else(|| anyhow::anyhow!("2FA is enabled but no TOTP data found"))?;
            let secret = crate::totp::decrypt_secret(&totp_data, &password)
                .context("Could not unlock 2FA with this password")?;
            match crate::totp::verify_code(&secret, &code, &totp_data.used_steps)? {
                Some(step) => {
                    // Record the used step for replay protection, pruning old ones.
                    let mut data = totp_data;
                    data.used_steps.push(step);
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    let cutoff = (now / 30) - 10; // ~5 minutes
                    data.used_steps.retain(|s| *s > cutoff);
                    let _ = self.auth_manager.update_totp(data).await;
                }
                None => {
                    password.zeroize();
                    anyhow::bail!("Invalid 2FA code");
                }
            }
        }

        Ok(password)
    }
}
