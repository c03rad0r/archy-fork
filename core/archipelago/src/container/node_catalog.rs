//! Optional, release-signed app catalog restricted to exactly one node DID.
//! It is never fetched from public mirrors or merged into the global signed
//! bytes. Existing application IDs cannot be overridden by a demo catalog.
use super::app_catalog::{AppCatalog, AppCatalogEntry};
use anyhow::{Context, Result};
use serde_json::Value;
use std::{collections::HashMap, io::Read, os::unix::fs::OpenOptionsExt, path::Path};

pub const FILE: &str = "node-app-catalog.json";
const LIMIT: u64 = 1024 * 1024;

fn validate(raw: &Value, node_did: &str) -> Result<AppCatalog> {
    anyhow::ensure!(
        raw["schema"] == 1 && raw["scope"] == "single-node-demo",
        "Unsupported node catalog scope"
    );
    anyhow::ensure!(
        raw["target_node_did"].as_str() == Some(node_did),
        "Catalog belongs to another node"
    );
    let expires = chrono::DateTime::parse_from_rfc3339(
        raw["expires_at"]
            .as_str()
            .context("Missing catalog expiry")?,
    )?;
    anyhow::ensure!(expires > chrono::Utc::now(), "Node catalog has expired");
    anyhow::ensure!(
        matches!(
            crate::trust::verify_detached(raw)?,
            crate::trust::SignatureStatus::Verified { anchored: true, .. }
        ),
        "Node catalog requires the pinned release-root signature"
    );
    let catalog: AppCatalog = serde_json::from_value(raw.clone())?;
    anyhow::ensure!(
        !catalog.apps.is_empty() && catalog.apps.len() <= 16,
        "Invalid demo app count"
    );
    for (id, entry) in &catalog.apps {
        anyhow::ensure!(
            id.starts_with("node-demo-")
                && id.len() <= 64
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
            "Invalid demo app ID"
        );
        let value = entry
            .manifest
            .clone()
            .context("Node demo requires an embedded manifest")?;
        anyhow::ensure!(
            super::app_catalog::catalog_manifest_overlay(id, value).is_some(),
            "Invalid node demo manifest"
        );
    }
    Ok(catalog)
}

pub fn verified_body(data_dir: &Path) -> Result<Option<String>> {
    let path = data_dir.join(FILE);
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)
    {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let metadata = file.metadata()?;
    anyhow::ensure!(
        metadata.is_file() && metadata.len() <= LIMIT,
        "Invalid node catalog file"
    );
    let mut body = String::new();
    file.take(LIMIT + 1).read_to_string(&mut body)?;
    anyhow::ensure!(
        body.len() as u64 <= LIMIT,
        "Node catalog exceeds size limit"
    );
    let public_key = std::fs::read(data_dir.join("identity/node_key.pub"))?;
    anyhow::ensure!(public_key.len() == 32, "Invalid local node identity");
    let node_did = crate::identity::did_key_from_pubkey_hex(&hex::encode(public_key))?;
    validate(&serde_json::from_str(&body)?, &node_did)?;
    Ok(Some(body))
}

pub fn entries(data_dir: &Path) -> HashMap<String, AppCatalogEntry> {
    match verified_body(data_dir) {
        Ok(Some(body)) => serde_json::from_str::<AppCatalog>(&body)
            .map(|catalog| catalog.apps)
            .unwrap_or_default(),
        Ok(None) => HashMap::new(),
        Err(error) => {
            tracing::warn!("Ignoring invalid node demo catalog: {error}");
            HashMap::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn signed(mut raw: Value, byte: u8) -> Value {
        let anchor = ed25519_dalek::SigningKey::from_bytes(&[7; 32]);
        std::env::set_var(
            "ARCHY_RELEASE_ROOT_PUBKEY",
            hex::encode(anchor.verifying_key().to_bytes()),
        );
        let key = ed25519_dalek::SigningKey::from_bytes(&[byte; 32]);
        let (sig, did) = crate::trust::signed_doc::sign_detached(&key, &raw).unwrap();
        raw["signature"] = sig.into();
        raw["signed_by"] = did.into();
        raw
    }
    fn fixture() -> Value {
        serde_json::json!({"schema":1,"scope":"single-node-demo","target_node_did":"did:key:fixture",
            "expires_at":(chrono::Utc::now()+chrono::Duration::days(1)).to_rfc3339(),
            "apps":{"node-demo-v4v":{"version":"1","image":"docker.io/library/node:24-alpine",
                "manifest":{"app":{"id":"node-demo-v4v","name":"Sovereign Music demo","version":"1",
                    "container":{"image":"docker.io/library/node:24-alpine"}}}}}})
    }
    #[test]
    fn audience_signature_expiry_namespace_and_manifest_are_required() {
        assert!(validate(&signed(fixture(), 7), "did:key:fixture").is_ok());
        assert!(validate(&signed(fixture(), 7), "did:key:another").is_err());
        assert!(validate(&fixture(), "did:key:fixture").is_err());
        assert!(validate(&signed(fixture(), 11), "did:key:fixture").is_err());
        let mut tampered = signed(fixture(), 7);
        tampered["apps"]["node-demo-v4v"]["version"] = "tampered".into();
        assert!(validate(&tampered, "did:key:fixture").is_err());
        let mut expired = fixture();
        expired["expires_at"] = "2020-01-01T00:00:00Z".into();
        assert!(validate(&signed(expired, 7), "did:key:fixture").is_err());
        let mut override_app = fixture();
        let entry = override_app["apps"]
            .as_object_mut()
            .unwrap()
            .remove("node-demo-v4v")
            .unwrap();
        override_app["apps"]["gitea"] = entry;
        assert!(validate(&signed(override_app, 7), "did:key:fixture").is_err());
        let mut wrong_manifest = fixture();
        wrong_manifest["apps"]["node-demo-v4v"]["manifest"]["app"]["id"] = "node-demo-other".into();
        assert!(validate(&signed(wrong_manifest, 7), "did:key:fixture").is_err());
    }
}
