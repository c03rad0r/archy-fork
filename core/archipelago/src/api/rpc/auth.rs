use super::RpcHandler;
#[cfg(debug_assertions)]
use super::DEV_DEFAULT_PASSWORD;
use anyhow::Result;

impl RpcHandler {
    pub(super) async fn handle_auth_login(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        // Companion device-token login: minted via auth.createDeviceToken and
        // carried by the pairing QR. Verified here so it shares the login rate
        // limiter with password attempts.
        if let Some(token) = params.get("token").and_then(|v| v.as_str()) {
            return match crate::device_tokens::verify(&self.config.data_dir, token).await {
                Some(device) => {
                    tracing::info!("[onboarding] device-token login ({device})");
                    Ok(serde_json::Value::Null)
                }
                None => {
                    tracing::warn!("[onboarding] device-token login failed");
                    Err(anyhow::anyhow!("Invalid device token"))
                }
            };
        }

        let password = params
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing password"))?;

        let is_setup = self.auth_manager.is_setup().await?;
        if !is_setup {
            // Dev BUILDS only: allow the default password so the UI can log
            // in without running setup. cfg-gated so no release binary can
            // carry the bypass, whatever its runtime config says.
            #[cfg(debug_assertions)]
            if self.config.dev_mode && password == DEV_DEFAULT_PASSWORD {
                tracing::info!("[onboarding] login via dev default password");
                return Ok(serde_json::Value::Null);
            }
            tracing::warn!("[onboarding] login attempt before setup complete");
            return Err(anyhow::anyhow!(
                "User not set up. Please complete setup first."
            ));
        }

        let valid = self.auth_manager.verify_password(password).await?;
        if !valid {
            // The companion app sends its device token through the password
            // field (it reuses the whole password auto-login path, including
            // the WebView form). Accept a valid token here so that path works.
            if let Some(device) =
                crate::device_tokens::verify(&self.config.data_dir, password).await
            {
                tracing::info!("[onboarding] device-token login via password field ({device})");
                return Ok(serde_json::Value::Null);
            }
            tracing::warn!("[onboarding] login failed — wrong password");
            return Err(anyhow::anyhow!("Password Incorrect"));
        }

        tracing::info!("[onboarding] login successful");

        // Best-effort: heal a LOCKED LND wallet created with an unknown/legacy
        // password by rotating it onto the per-node secret, using the password
        // the user just authenticated with as a candidate. Non-blocking so login
        // is never slowed or broken when LND isn't installed / already unlocked.
        let candidate = password.to_string();
        tokio::spawn(async move {
            match crate::container::lnd::migrate_locked_wallet(&[candidate]).await {
                Ok(true) => tracing::info!("[login] LND wallet healed / auto-unlocked"),
                Ok(false) => {} // not locked, or seed-recovery required
                Err(e) => tracing::debug!("[login] LND wallet migration skipped: {e}"),
            }
        });

        // Ensure NostrVPN config exists — covers the case where onboardingComplete
        // was never called (e.g., user took the "already set up" shortcut).
        let data_dir = self.config.data_dir.clone();
        tokio::spawn(async move {
            // Quick check: if config.toml already exists, skip
            let config_path = data_dir.join("nostr-vpn/.config/nvpn/config.toml");
            if config_path.exists() {
                return;
            }
            // Identity must exist for VPN config
            if !data_dir.join("identity/nostr_pubkey").exists() {
                return;
            }
            match crate::vpn::configure_nostr_vpn(&data_dir).await {
                Ok(()) => tracing::info!("[login] NostrVPN auto-configured on first login"),
                Err(e) => tracing::debug!("[login] NostrVPN auto-config skipped: {}", e),
            }
        });

        Ok(serde_json::Value::Null)
    }

    /// Mint a device token for the companion pairing QR. Session-gated by the
    /// dispatcher (not in UNAUTHENTICATED_METHODS), so only a logged-in web UI
    /// can mint one. The plaintext token is returned exactly once.
    pub(super) async fn handle_auth_create_device_token(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let mut name = params
            .as_ref()
            .and_then(|p| p.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("companion")
            .trim()
            .to_string();
        if name.is_empty() || name.len() > 64 {
            return Err(anyhow::anyhow!("Device name must be 1-64 characters"));
        }
        // The default name was a single shared slot: every pairing popup
        // replaced the previous phone's token, silently logging out the
        // first phone the moment a second one paired. Default-named mints
        // get a unique suffix so each device keeps its own credential;
        // explicitly named devices keep replace-in-place semantics.
        if name == "companion" {
            // KEY-05: source named. Two bytes of display-name disambiguation, not
            // key material — the credential itself is minted by
            // `device_tokens::create`, which is guarded. Unguarded here because a
            // degenerate predicate on a 2-byte draw false-positives once in 256.
            let mut suffix = [0u8; 2];
            rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut suffix);
            name = format!("companion-{}", hex::encode(suffix));
        }
        let token = crate::device_tokens::create(&self.config.data_dir, &name).await?;
        Ok(serde_json::json!({ "name": name, "token": token }))
    }

    pub(super) async fn handle_auth_list_device_tokens(&self) -> Result<serde_json::Value> {
        let tokens = crate::device_tokens::list(&self.config.data_dir).await;
        Ok(serde_json::json!(tokens
            .iter()
            .map(|t| serde_json::json!({ "name": t.name, "created": t.created }))
            .collect::<Vec<_>>()))
    }

    pub(super) async fn handle_auth_revoke_device_token(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let name = params
            .as_ref()
            .and_then(|p| p.get("name"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing name"))?;
        let removed = crate::device_tokens::remove(&self.config.data_dir, name).await?;
        Ok(serde_json::json!({ "removed": removed }))
    }

    pub(super) async fn handle_auth_logout(&self) -> Result<serde_json::Value> {
        tracing::info!("[onboarding] logout");
        Ok(serde_json::Value::Null)
    }

    pub(super) async fn handle_auth_change_password(
        &self,
        params: Option<serde_json::Value>,
        session_token: &Option<String>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let current_password = params
            .get("currentPassword")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing currentPassword"))?;
        let new_password = params
            .get("newPassword")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing newPassword"))?;
        let also_change_ssh = params
            .get("alsoChangeSsh")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let outcome = self
            .auth_manager
            .change_password(current_password, new_password, also_change_ssh)
            .await?;

        // Session rotation: invalidate all other sessions, rotate the caller's session
        if let Some(token) = session_token {
            self.session_store.invalidate_all_except(token).await;
        }

        Ok(serde_json::json!({
            "success": true,
            "session_rotated": true,
            "ssh_updated": outcome.ssh_updated,
            "ssh_error": outcome.ssh_error,
        }))
    }

    pub(super) async fn handle_auth_is_setup(&self) -> Result<serde_json::Value> {
        let is_setup = self.auth_manager.is_setup().await?;
        Ok(serde_json::json!(is_setup))
    }

    /// Create the node's user account. Unauthenticated by necessity: no account
    /// exists yet when the onboarding wizard reaches the password screen.
    ///
    /// D-04 verdict: **gated, in addition to the pre-existing `is_setup()`
    /// rejection.** The `is_setup()` check alone fails open in a drift case: on
    /// a provisioned node whose `user.json` is missing or was deleted it would
    /// still run — and it does more than create an account, it also rewrites the
    /// OS login password via `crate::auth::change_ssh_password` (below), which
    /// is an unauthenticated privilege escalation on a live node (T-10-08). The
    /// gate closes that case using the seed/onboarding signals, which survive a
    /// deleted `user.json`.
    pub(super) async fn handle_auth_setup(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // Prevent re-setup if already set up. Kept ahead of the gate so the
        // existing, more specific message survives for this common case.
        let is_setup = self.auth_manager.is_setup().await?;
        if is_setup {
            tracing::warn!("[onboarding] setup rejected — already set up");
            return Err(anyhow::anyhow!(
                "Already set up. Use auth.changePassword to change."
            ));
        }

        super::onboarding_gate::ensure_onboarding_open(&self.config.data_dir, &self.auth_manager)
            .await?;

        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let password = params
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing password"))?;

        if password.len() < 8 {
            tracing::warn!("[onboarding] setup rejected — password too short");
            return Err(anyhow::anyhow!("Password must be at least 8 characters"));
        }

        self.auth_manager.setup_user(password).await?;
        tracing::info!("[onboarding] user setup complete");

        // The install-time password must also become the OS login for the
        // archipelago user — otherwise the console/SSH keeps the image default
        // ("archipelago") after the user has picked a real password (#97).
        // Best-effort: a failure here must not break onboarding.
        match crate::auth::change_ssh_password(password).await {
            Ok(()) => tracing::info!("[onboarding] system login password synced"),
            Err(e) => tracing::warn!("[onboarding] system login password sync failed: {e}"),
        }

        // Persist the pending onboarding seed as the encrypted backup now that
        // a passphrase (the login password) finally exists — otherwise "Reveal
        // recovery phrase" has nothing to decrypt on this node, ever.
        // Best-effort: a failure here must not break password setup.
        match super::seed_rpc::save_pending_seed_encrypted(&self.config.data_dir, password).await {
            Ok(true) => tracing::info!("[onboarding] encrypted seed backup saved"),
            Ok(false) => tracing::info!(
                "[onboarding] no pending mnemonic to back up (restored earlier or legacy node)"
            ),
            Err(e) => tracing::warn!("[onboarding] encrypted seed backup failed: {e:#}"),
        }

        Ok(serde_json::json!(true))
    }

    /// Mark onboarding complete.
    ///
    /// This one takes the OPPOSITE guard to the rest of the D-04 sweep, and it
    /// is the most important addition in it. The method is unauthenticated
    /// (`middleware.rs:12`) and it SETS the very flag
    /// `onboarding_gate::ensure_onboarding_open` reads. Without a guard, one
    /// unauthenticated call against a fresh node marks it onboarded and
    /// permanently locks it out of its own onboarding — a denial of service
    /// created BY the gate (T-10-04). So: refuse until a user account exists.
    ///
    /// Verified safe against the real wizard before shipping:
    ///   * The live flow never calls this before `auth.setup`. It is
    ///     `/onboarding/intro → path → seed → seed-verify → identity → done →
    ///     /login`, and `views/Login.vue:405-425` posts `auth.setup` from that
    ///     last screen. The onboarding flag is then set by
    ///     `auth.rs:203-217`'s auto-heal inference, not by this RPC.
    ///   * The only caller of this method is `OnboardingVerify.vue:157`, on the
    ///     `/onboarding/verify` route — reachable only from
    ///     `/onboarding/backup`, which nothing in the app navigates to any more.
    ///   * Even on that dead path the refusal is invisible: `completeOnboarding`
    ///     wraps the call in `callWithRetry` (`useOnboarding.ts:64-68`), which
    ///     returns `null` on a non-retryable error instead of throwing, and
    ///     `proceed()` catches anyway.
    pub(super) async fn handle_auth_onboarding_complete(&self) -> Result<serde_json::Value> {
        super::onboarding_gate::ensure_user_account_exists(&self.auth_manager).await?;

        self.auth_manager.complete_onboarding().await?;
        tracing::info!("[onboarding] onboarding marked complete");

        // Auto-configure NostrVPN with the node's Nostr identity
        let data_dir = self.config.data_dir.clone();
        tokio::spawn(async move {
            match crate::vpn::configure_nostr_vpn(&data_dir).await {
                Ok(()) => tracing::info!("[onboarding] NostrVPN configured and started"),
                Err(e) => tracing::warn!("[onboarding] NostrVPN setup (non-fatal): {}", e),
            }
        });

        Ok(serde_json::json!(true))
    }

    pub(super) async fn handle_auth_is_onboarding_complete(&self) -> Result<serde_json::Value> {
        let complete = self.auth_manager.is_onboarding_complete().await?;
        tracing::debug!("[onboarding] isOnboardingComplete={}", complete);
        Ok(serde_json::json!(complete))
    }

    pub(super) async fn handle_auth_reset_onboarding(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let password = params
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing password — re-authentication required"))?;

        let valid = self.auth_manager.verify_password(password).await?;
        if !valid {
            tracing::warn!("[onboarding] reset rejected — wrong password");
            return Err(anyhow::anyhow!("Password Incorrect"));
        }

        self.auth_manager.reset_onboarding().await?;
        tracing::info!("[onboarding] onboarding reset");
        Ok(serde_json::json!(true))
    }
}
