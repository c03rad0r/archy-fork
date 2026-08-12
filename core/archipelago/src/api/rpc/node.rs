use super::RpcHandler;
use crate::container::docker_packages;
use crate::{backup, identity, nostr_discovery};
use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
use nostr_sdk::ToBech32;
use rand::rngs::OsRng;
use tokio::fs;

impl RpcHandler {
    pub(super) async fn handle_node_did(&self) -> Result<serde_json::Value> {
        let (data, _) = self.state_manager.get_snapshot().await;
        let did = identity::did_key_from_pubkey_hex(&data.server_info.pubkey)?;
        let identity_dir = self.config.data_dir.join("identity");
        let nostr_pubkey = nostr_discovery::get_nostr_pubkey(&identity_dir).await.ok();
        let nostr_npub = nostr_pubkey.as_ref().and_then(|hex| {
            nostr_sdk::PublicKey::from_hex(hex)
                .ok()
                .and_then(|pk| pk.to_bech32().ok())
        });
        Ok(serde_json::json!({
            "did": did,
            "pubkey": data.server_info.pubkey,
            "nostr_pubkey": nostr_pubkey,
            "nostr_npub": nostr_npub,
        }))
    }

    /// Sign a challenge to prove control of the node DID (proof-of-control for onboarding).
    pub(super) async fn handle_node_sign_challenge(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let challenge = params
            .get("challenge")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing challenge string"))?;

        let identity_dir = self.config.data_dir.join("identity");
        let identity = identity::NodeIdentity::load_or_create(&identity_dir).await?;
        let signature = identity.sign(challenge.as_bytes());

        Ok(serde_json::json!({ "signature": signature }))
    }

    /// Create an encrypted backup of the node identity (for onboarding).
    pub(super) async fn handle_node_create_backup(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let passphrase = params
            .get("passphrase")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing passphrase"))?;

        let (data, _) = self.state_manager.get_snapshot().await;
        let did = identity::did_key_from_pubkey_hex(&data.server_info.pubkey)?;
        let identity_dir = self.config.data_dir.join("identity");

        let backup = backup::create_encrypted_backup(
            &identity_dir,
            passphrase,
            &did,
            &data.server_info.pubkey,
        )
        .await?;

        Ok(backup)
    }

    pub(super) async fn handle_node_tor_address(&self) -> Result<serde_json::Value> {
        let tor_address = docker_packages::read_tor_address("archipelago").await;
        Ok(serde_json::json!({ "tor_address": tor_address }))
    }

    pub(super) async fn handle_node_nostr_publish(&self) -> Result<serde_json::Value> {
        // Publishing node identity (including Tor addresses) to public Nostr relays is disabled
        // for security. Nodes connect via federation ID, not public discovery.
        anyhow::bail!("Nostr identity publishing is disabled — nodes connect via federation ID")
    }

    pub(super) async fn handle_node_nostr_pubkey(&self) -> Result<serde_json::Value> {
        let identity_dir = self.config.data_dir.join("identity");
        let pubkey_hex = nostr_discovery::get_nostr_pubkey(&identity_dir).await?;
        let npub = nostr_sdk::PublicKey::from_hex(&pubkey_hex)
            .ok()
            .and_then(|pk| pk.to_bech32().ok());
        Ok(serde_json::json!({
            "nostr_pubkey": pubkey_hex,
            "nostr_npub": npub,
        }))
    }

    /// Sign a Nostr event with the node's Nostr key.
    /// Accepts full event object, computes NIP-01 hash, returns signed event.
    pub(super) async fn handle_node_nostr_sign(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let identity_dir = self.config.data_dir.join("identity");
        let pubkey_hex = nostr_discovery::get_nostr_pubkey(&identity_dir).await?;

        let event = params
            .get("event")
            .ok_or_else(|| anyhow::anyhow!("Missing 'event' parameter"))?;

        let kind = event.get("kind").and_then(|v| v.as_u64()).unwrap_or(1);
        let content = event.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let created_at = event
            .get("created_at")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            });
        let tags = event
            .get("tags")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));

        // NIP-01 serialization: [0, pubkey, created_at, kind, tags, content]
        let serialized = serde_json::json!([0, pubkey_hex, created_at, kind, tags, content]);
        let serialized_str = serde_json::to_string(&serialized)?;

        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(serialized_str.as_bytes());
        let event_hash_hex = hex::encode(hash);

        let signature = nostr_discovery::nostr_sign_hash(&identity_dir, &event_hash_hex).await?;

        Ok(serde_json::json!({
            "id": event_hash_hex,
            "pubkey": pubkey_hex,
            "created_at": created_at,
            "kind": kind,
            "tags": tags,
            "content": content,
            "sig": signature,
        }))
    }

    pub(super) async fn handle_node_nostr_verify_revoked(&self) -> Result<serde_json::Value> {
        let identity_dir = self.config.data_dir.join("identity");
        let status = nostr_discovery::verify_revocation(
            &identity_dir,
            self.config.nostr_tor_proxy.as_deref(),
        )
        .await?;
        Ok(serde_json::json!({
            "revoked": status.revoked,
            "nostr_pubkey": status.nostr_pubkey,
            "latest_content": status.latest_content,
            "error": status.error,
        }))
    }

    /// Rotate the node's Ed25519 identity keypair.
    /// Requires password re-verification. Returns a signed proof that peers can
    /// use to verify the rotation was authorized by the holder of the old key.
    pub(super) async fn handle_node_rotate_did(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let password = params
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'password' parameter"))?;

        // Re-verify password before allowing key rotation
        if !self.auth_manager.verify_password(password).await? {
            anyhow::bail!("Password verification failed");
        }

        let identity_dir = self.config.data_dir.join("identity");

        // Load the current identity to get old DID and signing key
        let old_identity = identity::NodeIdentity::load_or_create(&identity_dir).await?;
        let old_pubkey_hex = old_identity.pubkey_hex();
        let old_did = identity::did_key_from_pubkey_hex(&old_pubkey_hex)?;

        // Generate a new Ed25519 keypair
        let new_signing_key = SigningKey::generate(&mut OsRng);
        let new_pubkey_hex = hex::encode(new_signing_key.verifying_key().as_bytes());
        let new_did = identity::did_key_from_pubkey_hex(&new_pubkey_hex)?;

        // Create a rotation proof signed by the OLD key:
        // "did-rotate:{old_did}:{new_did}:{timestamp}"
        let timestamp = chrono::Utc::now().to_rfc3339();
        let proof_message = format!("did-rotate:{}:{}:{}", old_did, new_did, timestamp);
        let proof_signature = old_identity.sign(proof_message.as_bytes());

        // Write the new key files, overwriting the old ones
        let key_path = identity_dir.join("node_key");
        let pub_path = identity_dir.join("node_key.pub");

        fs::write(&key_path, new_signing_key.to_bytes())
            .await
            .context("Failed to write new node key")?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
                .await
                .context("Failed to set key permissions")?;
        }

        fs::write(&pub_path, new_signing_key.verifying_key().as_bytes())
            .await
            .context("Failed to write new node public key")?;

        // Update in-memory state so the new pubkey is reflected immediately
        let (mut data, _) = self.state_manager.get_snapshot().await;
        data.server_info.pubkey = new_pubkey_hex.clone();
        self.state_manager.update_data(data).await;

        tracing::info!(
            old_did = %old_did,
            new_did = %new_did,
            "Node DID rotated successfully"
        );

        Ok(serde_json::json!({
            "old_did": old_did,
            "new_did": new_did,
            "new_pubkey": new_pubkey_hex,
            "proof_signature": proof_signature,
            "proof_message": proof_message,
            "timestamp": timestamp,
        }))
    }
}
