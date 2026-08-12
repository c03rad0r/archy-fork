//! did:dht — Decentralized Identifier method using BitTorrent Mainline DHT.
//!
//! Implements creation, publication, and resolution of did:dht identifiers
//! using BEP-44 mutable items on the Mainline DHT.
//!
//! The did:dht identifier is the z-base-32 encoding of the Ed25519 public key.

use crate::network::zbase32;
use anyhow::{Context, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};
use std::collections::HashMap;
use std::path::Path;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Cache for resolved did:dht documents (1 hour TTL).
pub struct DhtDidCache {
    entries: RwLock<HashMap<String, (std::time::Instant, serde_json::Value)>>,
    ttl: std::time::Duration,
}

impl DhtDidCache {
    pub async fn get(&self, did: &str) -> Option<serde_json::Value> {
        let entries = self.entries.read().await;
        if let Some((ts, doc)) = entries.get(did) {
            if ts.elapsed() < self.ttl {
                return Some(doc.clone());
            }
        }
        None
    }

    pub async fn set(&self, did: String, doc: serde_json::Value) {
        let mut entries = self.entries.write().await;
        entries.insert(did, (std::time::Instant::now(), doc));
    }
}

/// Generate a did:dht identifier from an Ed25519 public key.
pub fn did_from_pubkey(pubkey: &VerifyingKey) -> String {
    let encoded = zbase32::encode_full_bytes(pubkey.as_bytes());
    format!("did:dht:{}", encoded)
}

/// Round-trip guard: a `did:dht` identifier IS the z-base-32 encoding of the
/// key, so a change in that encoding silently rotates every node's DID and
/// orphans its published DHT records. Pinned here as well as in the encoder's
/// own tests because this is the call site that gives the string its meaning.
#[cfg(test)]
const DID_FOR_TEST_MNEMONIC_NODE_KEY: &str =
    "did:dht:1o96jdeaigue33hrds3ph6shicehgssx6q6c88yo638curyx7i4y";

/// Extract the Ed25519 public key bytes from a did:dht identifier.
pub fn pubkey_from_did(did: &str) -> Result<[u8; 32]> {
    let id = did
        .strip_prefix("did:dht:")
        .ok_or_else(|| anyhow::anyhow!("Not a did:dht identifier: {}", did))?;
    let bytes = zbase32::decode_full_bytes_str(id)
        .map_err(|e| anyhow::anyhow!("Invalid z-base-32: {e}"))?;
    if bytes.len() != 32 {
        anyhow::bail!("Expected 32-byte pubkey, got {} bytes", bytes.len());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Build a DID Document JSON for an Ed25519 key.
fn build_did_document(did: &str, pubkey: &VerifyingKey) -> serde_json::Value {
    let pubkey_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        pubkey.as_bytes(),
    );

    serde_json::json!({
        "@context": [
            "https://www.w3.org/ns/did/v1",
            "https://w3id.org/security/suites/ed2020/v1"
        ],
        "id": did,
        "verificationMethod": [{
            "id": format!("{}#key-0", did),
            "type": "Ed25519VerificationKey2020",
            "controller": did,
            "publicKeyMultibase": format!("z{}", pubkey_b64),
        }],
        "authentication": [format!("{}#key-0", did)],
        "assertionMethod": [format!("{}#key-0", did)],
        "capabilityInvocation": [format!("{}#key-0", did)],
        "capabilityDelegation": [format!("{}#key-0", did)],
    })
}

/// Encode the DID Document as bytes for DHT storage.
fn encode_for_dht(did_doc: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(did_doc).unwrap_or_default()
}

/// Create and publish a did:dht to the Mainline DHT.
pub async fn create_and_publish(
    signing_key: &SigningKey,
    _services: &[(&str, &str)],
) -> Result<String> {
    let pubkey = signing_key.verifying_key();
    let did = did_from_pubkey(&pubkey);
    let did_doc = build_did_document(&did, &pubkey);
    let payload = encode_for_dht(&did_doc);

    // Publish to DHT using BEP-44 mutable item
    let dht = mainline::Dht::client().context("Failed to create DHT client")?;

    let signer = mainline::SigningKey::from_bytes(&signing_key.to_bytes());
    let item = mainline::MutableItem::new(signer, bytes::Bytes::from(payload), 0, None);

    dht.put_mutable(item).context("Failed to publish to DHT")?;

    info!(did = %did, "Published did:dht to Mainline DHT");
    Ok(did)
}

/// Resolve a did:dht from the Mainline DHT.
pub async fn resolve(did: &str, cache: Option<&DhtDidCache>) -> Result<serde_json::Value> {
    // Check cache first
    if let Some(cache) = cache {
        if let Some(doc) = cache.get(did).await {
            debug!(did = %did, "Resolved did:dht from cache");
            return Ok(doc);
        }
    }

    let pubkey_bytes = pubkey_from_did(did)?;

    let dht = mainline::Dht::client().context("Failed to create DHT client")?;

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::task::spawn_blocking(move || match dht.get_mutable(&pubkey_bytes, None, None) {
            Ok(mut iter) => iter.next(),
            Err(_) => None,
        }),
    )
    .await
    .context("DHT resolution timed out")?
    .context("DHT task panicked")?;

    match response {
        Some(item) => {
            let doc: serde_json::Value = serde_json::from_slice(item.value())
                .context("Failed to parse DID Document from DHT")?;

            if let Some(cache) = cache {
                cache.set(did.to_string(), doc.clone()).await;
            }

            debug!(did = %did, "Resolved did:dht from DHT");
            Ok(doc)
        }
        None => {
            anyhow::bail!("did:dht not found in DHT: {}", did)
        }
    }
}

/// Store the did:dht identifier for an identity record.
pub async fn save_dht_did(data_dir: &Path, identity_id: &str, dht_did: &str) -> Result<()> {
    let path = data_dir
        .join("identities")
        .join(format!("{}.json", identity_id));
    if !path.exists() {
        anyhow::bail!("Identity not found: {}", identity_id);
    }
    let content = tokio::fs::read_to_string(&path).await?;
    let mut record: serde_json::Value = serde_json::from_str(&content)?;
    record["dht_did"] = serde_json::json!(dht_did);
    let updated = serde_json::to_string_pretty(&record)?;
    tokio::fs::write(&path, updated).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_did_roundtrip() {
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let pubkey = key.verifying_key();
        let did = did_from_pubkey(&pubkey);
        assert!(did.starts_with("did:dht:"));

        let recovered = pubkey_from_did(&did).unwrap();
        assert_eq!(recovered, *pubkey.as_bytes());
    }

    #[test]
    fn test_invalid_did() {
        assert!(pubkey_from_did("did:key:z123").is_err());
    }

    /// The identifier for a known key must not move. Guards the LGPL `zbase32`
    /// → in-tree encoder swap, and any future change to it.
    #[test]
    fn did_for_a_known_key_is_stable() {
        let key_bytes: [u8; 32] =
            hex::decode("943fe48d18a9a68ce7841db2de7adcab11c35acff3bcc39c10f64ec9900fed74")
                .unwrap()
                .try_into()
                .unwrap();
        let pubkey = VerifyingKey::from_bytes(&key_bytes).unwrap();

        assert_eq!(did_from_pubkey(&pubkey), DID_FOR_TEST_MNEMONIC_NODE_KEY);
        assert_eq!(
            pubkey_from_did(DID_FOR_TEST_MNEMONIC_NODE_KEY).unwrap(),
            key_bytes
        );
    }

    #[test]
    fn rejects_a_did_whose_body_is_not_zbase32() {
        // `l` and `v` are not in the z-base-32 alphabet.
        assert!(pubkey_from_did("did:dht:lllvvv").is_err());
    }

    #[test]
    fn test_build_did_document() {
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        let pubkey = key.verifying_key();
        let did = did_from_pubkey(&pubkey);
        let doc = build_did_document(&did, &pubkey);

        assert_eq!(doc["id"], did);
        assert!(!doc["verificationMethod"].as_array().unwrap().is_empty());
    }
}
