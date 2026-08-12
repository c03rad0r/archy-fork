//! Companion device tokens — long-lived bearer credentials minted from an
//! authenticated session, so the pairing QR can log a phone in without
//! carrying the admin password (which the browser never has anyway).
//!
//! Only the SHA-256 of each token is persisted (`device-tokens.json` in the
//! data dir); the plaintext is returned exactly once at mint time and rides
//! the QR as the `tok` param. Verification goes through `auth.login`'s
//! `token` param and is covered by the same login rate limiter as passwords.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs;

const TOKENS_FILE: &str = "device-tokens.json";

/// Cap on stored tokens; re-pairing the same device name replaces its entry,
/// so this only limits the number of *distinct* device names.
const MAX_TOKENS: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceToken {
    pub name: String,
    /// Hex SHA-256 of the plaintext token.
    pub hash: String,
    /// Unix seconds at mint time.
    pub created: u64,
    /// App ids this token may reach through the app gate.
    ///
    /// `None` means node-wide, which is what every companion pairing token
    /// is and what tokens minted before scoping existed remain — the field
    /// is absent from their stored JSON and deserialises to `None`. A
    /// migration that guessed a scope for them would silently revoke access
    /// the operator never asked to revoke.
    ///
    /// `Some(list)` restricts the token to exactly those apps, which is the
    /// point of scoping: a token handed to Home Assistant so it can poll one
    /// app's API should not also open every other app on the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apps: Option<Vec<String>>,
}

impl DeviceToken {
    /// Whether this token may reach `app_id`.
    pub fn allows_app(&self, app_id: &str) -> bool {
        match &self.apps {
            None => true,
            Some(apps) => apps.iter().any(|a| a == app_id),
        }
    }
}

fn tokens_path(data_dir: &Path) -> PathBuf {
    data_dir.join(TOKENS_FILE)
}

async fn load(data_dir: &Path) -> Vec<DeviceToken> {
    match fs::read(tokens_path(data_dir)).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

async fn save(data_dir: &Path, tokens: &[DeviceToken]) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(tokens)?;
    fs::write(tokens_path(data_dir), bytes)
        .await
        .context("write device-tokens.json")
}

fn hash_hex(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Mint a new token for `name`. An existing token with the same name is
/// replaced, so re-showing the pairing QR never piles up stale entries.
/// Returns the plaintext token — the only time it ever exists outside the QR.
pub async fn create(data_dir: &Path, name: &str) -> Result<String> {
    create_scoped(data_dir, name, None).await
}

/// Mint a token limited to `apps`, for a machine client that needs one app's
/// HTTP API and nothing else. `None` mints the node-wide token `create` does.
pub async fn create_scoped(
    data_dir: &Path,
    name: &str,
    apps: Option<Vec<String>>,
) -> Result<String> {
    // An empty list would be indistinguishable from "no restriction" to a
    // careless reader while actually authorising nothing — reject it rather
    // than mint a token whose behaviour nobody can predict from its record.
    if apps.as_ref().is_some_and(|a| a.is_empty()) {
        anyhow::bail!("a scoped device token must name at least one app");
    }
    // KEY-05: a device token is a bearer credential — its unpredictability is
    // the whole of its security — so the source is named and the draw guarded.
    let mut token_bytes = [0u8; 32];
    crate::entropy::draw_key_bytes(&mut rand::rngs::OsRng, &mut token_bytes).map_err(|e| {
        anyhow::anyhow!("Refusing to mint a device token from degenerate entropy: {e}")
    })?;
    let token = hex::encode(token_bytes);

    let mut tokens = load(data_dir).await;
    tokens.retain(|t| t.name != name);
    if tokens.len() >= MAX_TOKENS {
        tokens.remove(0);
    }
    tokens.push(DeviceToken {
        name: name.to_string(),
        hash: hash_hex(&token),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        apps,
    });
    save(data_dir, &tokens).await?;
    Ok(token)
}

/// Verify a candidate token. Returns the device name it was minted for.
pub async fn verify(data_dir: &Path, candidate: &str) -> Option<String> {
    let candidate_hash = hash_hex(candidate);
    load(data_dir)
        .await
        .iter()
        .find(|t| ct_eq(t.hash.as_bytes(), candidate_hash.as_bytes()))
        .map(|t| t.name.clone())
}

/// Verify a candidate token **for a specific app**, as the app gate does.
/// Returns the device name when the token is valid *and* in scope.
///
/// Separate from `verify` on purpose: `verify` answers "is this a real
/// token", which is the right question for node login, and would be the
/// wrong question here — a token scoped to one app would otherwise open
/// every app.
pub async fn verify_for_app(data_dir: &Path, candidate: &str, app_id: &str) -> Option<String> {
    let candidate_hash = hash_hex(candidate);
    load(data_dir)
        .await
        .iter()
        .find(|t| ct_eq(t.hash.as_bytes(), candidate_hash.as_bytes()) && t.allows_app(app_id))
        .map(|t| t.name.clone())
}

/// List stored tokens (hashes only — plaintexts are unrecoverable).
pub async fn list(data_dir: &Path) -> Vec<DeviceToken> {
    load(data_dir).await
}

/// Remove the token minted for `name`. Returns whether one existed.
pub async fn remove(data_dir: &Path, name: &str) -> Result<bool> {
    let mut tokens = load(data_dir).await;
    let before = tokens.len();
    tokens.retain(|t| t.name != name);
    let removed = tokens.len() != before;
    if removed {
        save(data_dir, &tokens).await?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mint_verify_replace_remove() {
        let dir = tempfile::tempdir().unwrap();
        let token = create(dir.path(), "phone").await.unwrap();
        assert_eq!(verify(dir.path(), &token).await.as_deref(), Some("phone"));
        assert!(verify(dir.path(), "not-a-token").await.is_none());

        // Re-minting the same name invalidates the old token.
        let token2 = create(dir.path(), "phone").await.unwrap();
        assert!(verify(dir.path(), &token).await.is_none());
        assert_eq!(verify(dir.path(), &token2).await.as_deref(), Some("phone"));
        assert_eq!(list(dir.path()).await.len(), 1);

        assert!(remove(dir.path(), "phone").await.unwrap());
        assert!(verify(dir.path(), &token2).await.is_none());
    }
}
