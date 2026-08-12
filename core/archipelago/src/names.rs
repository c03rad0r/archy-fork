//! Bitcoin domain names management using Nostr NIP-05 verification.
//! Allows users to register human-readable names linked to their DIDs
//! and verify names of peers via the NIP-05 protocol.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::debug;

const NAMES_FILE: &str = "names.json";

/// A registered name linked to an identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredName {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub identity_id: String,
    pub did: String,
    pub nostr_pubkey: Option<String>,
    pub status: NameStatus,
    pub registered_at: String,
    pub expires_at: Option<String>,
    pub nip05: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NameStatus {
    Active,
    Pending,
    Expired,
    Failed,
}

impl std::fmt::Display for NameStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NameStatus::Active => write!(f, "active"),
            NameStatus::Pending => write!(f, "pending"),
            NameStatus::Expired => write!(f, "expired"),
            NameStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Stored names data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NamesStore {
    pub names: Vec<RegisteredName>,
}

/// NIP-05 verification result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nip05Resolution {
    pub name: String,
    pub domain: String,
    pub nostr_pubkey: Option<String>,
    pub relays: Vec<String>,
    pub verified: bool,
}

pub async fn load_names(data_dir: &Path) -> Result<NamesStore> {
    let path = data_dir.join(NAMES_FILE);
    if !path.exists() {
        return Ok(NamesStore::default());
    }
    let data = fs::read_to_string(&path)
        .await
        .context("Reading names store")?;
    serde_json::from_str(&data).context("Parsing names store")
}

pub async fn save_names(data_dir: &Path, store: &NamesStore) -> Result<()> {
    let path = data_dir.join(NAMES_FILE);
    let data = serde_json::to_string_pretty(store)?;
    fs::write(&path, data).await.context("Writing names store")
}

/// Register a new name linked to an identity.
pub async fn register_name(
    data_dir: &Path,
    name: &str,
    domain: &str,
    identity_id: &str,
    did: &str,
    nostr_pubkey: Option<&str>,
) -> Result<RegisteredName> {
    let mut store = load_names(data_dir).await?;

    let nip05 = format!("{}@{}", name, domain);

    // Check for duplicates
    if store.names.iter().any(|n| n.nip05 == nip05) {
        return Err(anyhow::anyhow!("Name {} is already registered", nip05));
    }

    let record = RegisteredName {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        domain: domain.to_string(),
        identity_id: identity_id.to_string(),
        did: did.to_string(),
        nostr_pubkey: nostr_pubkey.map(|s| s.to_string()),
        status: NameStatus::Active,
        registered_at: chrono::Utc::now().to_rfc3339(),
        expires_at: None,
        nip05,
    };

    debug!(name = %record.nip05, "Registered new name");
    store.names.push(record.clone());
    save_names(data_dir, &store).await?;
    Ok(record)
}

/// Remove a registered name.
pub async fn remove_name(data_dir: &Path, name_id: &str) -> Result<()> {
    let mut store = load_names(data_dir).await?;
    let original_len = store.names.len();
    store.names.retain(|n| n.id != name_id);
    if store.names.len() == original_len {
        return Err(anyhow::anyhow!("Name not found: {}", name_id));
    }
    save_names(data_dir, &store).await
}

/// Resolve a NIP-05 identifier (user@domain) to verify it.
pub async fn resolve_nip05(identifier: &str) -> Result<Nip05Resolution> {
    let parts: Vec<&str> = identifier.split('@').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!(
            "Invalid NIP-05 identifier: expected user@domain"
        ));
    }
    let name = parts[0];
    let domain = parts[1];

    let url = format!("https://{}/.well-known/nostr.json?name={}", domain, name);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let response = client.get(&url).send().await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = resp.json().await?;
            let pubkey = body
                .get("names")
                .and_then(|names| names.get(name))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let relays = if let Some(pk) = &pubkey {
                body.get("relays")
                    .and_then(|r| r.get(pk.as_str()))
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                vec![]
            };

            Ok(Nip05Resolution {
                name: name.to_string(),
                domain: domain.to_string(),
                nostr_pubkey: pubkey.clone(),
                relays,
                verified: pubkey.is_some(),
            })
        }
        Ok(resp) => Err(anyhow::anyhow!(
            "NIP-05 verification failed: HTTP {}",
            resp.status()
        )),
        Err(_) => Ok(Nip05Resolution {
            name: name.to_string(),
            domain: domain.to_string(),
            nostr_pubkey: None,
            relays: vec![],
            verified: false,
        }),
    }
}

/// Link a registered name to a DID by updating its identity association.
pub async fn link_name_to_did(
    data_dir: &Path,
    name_id: &str,
    did: &str,
    identity_id: &str,
) -> Result<RegisteredName> {
    let mut store = load_names(data_dir).await?;
    let name = store
        .names
        .iter_mut()
        .find(|n| n.id == name_id)
        .ok_or_else(|| anyhow::anyhow!("Name not found: {}", name_id))?;

    name.did = did.to_string();
    name.identity_id = identity_id.to_string();
    let updated = name.clone();
    save_names(data_dir, &store).await?;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_status_display() {
        assert_eq!(NameStatus::Active.to_string(), "active");
        assert_eq!(NameStatus::Pending.to_string(), "pending");
        assert_eq!(NameStatus::Expired.to_string(), "expired");
        assert_eq!(NameStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn test_name_status_serde_roundtrip() {
        let json = serde_json::to_string(&NameStatus::Active).unwrap();
        assert_eq!(json, "\"active\"");
        let deserialized: NameStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, NameStatus::Active);
    }

    #[test]
    fn test_names_store_default_is_empty() {
        let store = NamesStore::default();
        assert!(store.names.is_empty());
    }

    #[tokio::test]
    async fn test_load_names_returns_empty_when_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = load_names(dir.path()).await.unwrap();
        assert!(store.names.is_empty());
    }

    #[tokio::test]
    async fn test_save_and_load_names_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = NamesStore {
            names: vec![RegisteredName {
                id: "test-id-1".to_string(),
                name: "satoshi".to_string(),
                domain: "example.com".to_string(),
                identity_id: "identity-1".to_string(),
                did: "did:key:z123".to_string(),
                nostr_pubkey: Some("npub1abc".to_string()),
                status: NameStatus::Active,
                registered_at: "2025-01-01T00:00:00Z".to_string(),
                expires_at: None,
                nip05: "satoshi@example.com".to_string(),
            }],
        };
        save_names(dir.path(), &store).await.unwrap();
        let loaded = load_names(dir.path()).await.unwrap();
        assert_eq!(loaded.names.len(), 1);
        assert_eq!(loaded.names[0].name, "satoshi");
        assert_eq!(loaded.names[0].nip05, "satoshi@example.com");
    }

    #[tokio::test]
    async fn test_register_name_creates_record() {
        let dir = tempfile::tempdir().unwrap();
        let result = register_name(
            dir.path(),
            "alice",
            "bitcoin.org",
            "id-1",
            "did:key:zabc",
            Some("npub1xyz"),
        )
        .await
        .unwrap();

        assert_eq!(result.name, "alice");
        assert_eq!(result.domain, "bitcoin.org");
        assert_eq!(result.nip05, "alice@bitcoin.org");
        assert_eq!(result.did, "did:key:zabc");
        assert_eq!(result.nostr_pubkey, Some("npub1xyz".to_string()));
        assert_eq!(result.status, NameStatus::Active);
        assert!(!result.id.is_empty());

        // Verify persisted
        let store = load_names(dir.path()).await.unwrap();
        assert_eq!(store.names.len(), 1);
    }

    #[tokio::test]
    async fn test_register_duplicate_name_fails() {
        let dir = tempfile::tempdir().unwrap();
        register_name(dir.path(), "bob", "test.com", "id-1", "did:key:z1", None)
            .await
            .unwrap();

        let result = register_name(dir.path(), "bob", "test.com", "id-2", "did:key:z2", None).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("already registered"));
    }

    #[tokio::test]
    async fn test_register_same_name_different_domain_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        register_name(
            dir.path(),
            "alice",
            "domain1.com",
            "id-1",
            "did:key:z1",
            None,
        )
        .await
        .unwrap();
        let result = register_name(
            dir.path(),
            "alice",
            "domain2.com",
            "id-1",
            "did:key:z1",
            None,
        )
        .await;
        assert!(result.is_ok());

        let store = load_names(dir.path()).await.unwrap();
        assert_eq!(store.names.len(), 2);
    }

    #[tokio::test]
    async fn test_remove_name_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let registered = register_name(
            dir.path(),
            "charlie",
            "example.com",
            "id-1",
            "did:key:z1",
            None,
        )
        .await
        .unwrap();

        remove_name(dir.path(), &registered.id).await.unwrap();
        let store = load_names(dir.path()).await.unwrap();
        assert!(store.names.is_empty());
    }

    #[tokio::test]
    async fn test_remove_nonexistent_name_fails() {
        let dir = tempfile::tempdir().unwrap();
        let result = remove_name(dir.path(), "nonexistent-id").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Name not found"));
    }

    #[tokio::test]
    async fn test_link_name_to_did() {
        let dir = tempfile::tempdir().unwrap();
        let registered = register_name(
            dir.path(),
            "dave",
            "example.com",
            "old-identity",
            "did:key:old",
            None,
        )
        .await
        .unwrap();

        let updated = link_name_to_did(dir.path(), &registered.id, "did:key:new", "new-identity")
            .await
            .unwrap();

        assert_eq!(updated.did, "did:key:new");
        assert_eq!(updated.identity_id, "new-identity");
        // Name itself should be unchanged
        assert_eq!(updated.name, "dave");
    }
}
