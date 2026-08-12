use anyhow::Result;
use std::path::Path;
use tracing::debug;

use super::store::{load_credentials, save_credentials};
use super::types::*;

/// Issue a new Verifiable Credential following W3C VC Data Model 2.0.
/// Uses Ed25519Signature2020 proof format.
pub async fn issue_credential(
    data_dir: &Path,
    issuer_did: &str,
    subject_did: &str,
    credential_type: &str,
    claims: serde_json::Value,
    expires_at: Option<&str>,
    sign_fn: impl FnOnce(&[u8]) -> Result<String>,
) -> Result<VerifiableCredential> {
    let id = format!("urn:uuid:{}", uuid::Uuid::new_v4());
    let issued_at = chrono::Utc::now().to_rfc3339();
    let key_id = format!("{}#key-1", issuer_did);

    // Build the credential body for signing (without proof)
    let body = serde_json::json!({
        "@context": [VC_CONTEXT_V2, ED25519_CONTEXT],
        "id": id,
        "type": ["VerifiableCredential", credential_type],
        "issuer": issuer_did,
        "credentialSubject": {
            "id": subject_did,
        },
        "issuanceDate": issued_at,
    });
    let body_bytes = serde_json::to_vec(&body)?;
    let signature = sign_fn(&body_bytes)?;

    let vc = VerifiableCredential {
        context: vec![VC_CONTEXT_V2.to_string(), ED25519_CONTEXT.to_string()],
        id: id.clone(),
        credential_type: vec![
            "VerifiableCredential".to_string(),
            credential_type.to_string(),
        ],
        issuer: issuer_did.to_string(),
        credential_subject: CredentialSubject {
            id: subject_did.to_string(),
            claims,
        },
        issuance_date: issued_at.clone(),
        expiration_date: expires_at.map(|s| s.to_string()),
        proof: CredentialProof {
            proof_type: "Ed25519Signature2020".to_string(),
            created: issued_at,
            verification_method: key_id,
            proof_purpose: "assertionMethod".to_string(),
            proof_value: signature,
        },
        credential_status: None,
    };

    let mut store = load_credentials(data_dir).await?;
    debug!(id = %vc.id, "Issued W3C VC");
    store.credentials.push(vc.clone());
    save_credentials(data_dir, &store).await?;
    Ok(vc)
}

/// Verify a credential's signature against the issuer DID.
pub fn verify_credential(
    vc: &VerifiableCredential,
    verify_fn: impl FnOnce(&str, &[u8], &str) -> Result<bool>,
) -> Result<bool> {
    let body = serde_json::json!({
        "@context": vc.context,
        "id": vc.id,
        "type": vc.credential_type,
        "issuer": vc.issuer,
        "credentialSubject": {
            "id": vc.credential_subject.id,
        },
        "issuanceDate": vc.issuance_date,
    });
    let body_bytes = serde_json::to_vec(&body)?;
    verify_fn(&vc.issuer, &body_bytes, &vc.proof.proof_value)
}

/// Revoke a credential by ID.
pub async fn revoke_credential(data_dir: &Path, credential_id: &str) -> Result<()> {
    let mut store = load_credentials(data_dir).await?;
    let vc = store
        .credentials
        .iter_mut()
        .find(|c| c.id == credential_id)
        .ok_or_else(|| anyhow::anyhow!("Credential not found: {}", credential_id))?;
    vc.credential_status = Some(CredentialStatusEntry {
        id: format!("{}#status", credential_id),
        status_type: "CredentialStatusList2021".to_string(),
        status: "revoked".to_string(),
    });
    save_credentials(data_dir, &store).await
}

/// List all credentials, optionally filtering by issuer or subject DID.
pub async fn list_credentials(
    data_dir: &Path,
    filter_did: Option<&str>,
) -> Result<Vec<VerifiableCredential>> {
    let store = load_credentials(data_dir).await?;
    let creds = if let Some(did) = filter_did {
        store
            .credentials
            .into_iter()
            .filter(|c| c.issuer == did || c.credential_subject.id == did)
            .collect()
    } else {
        store.credentials
    };
    Ok(creds)
}

/// Check if a credential is revoked.
pub fn is_revoked(vc: &VerifiableCredential) -> bool {
    vc.credential_status
        .as_ref()
        .is_some_and(|s| s.status == "revoked")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a tempdir with a dummy `identity/node_key` so that the
    /// credential store's encrypt/decrypt path can derive a key.
    /// Returns the tempdir guard (drop it to clean up).
    fn test_dir_with_node_key() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let identity_dir = dir.path().join("identity");
        std::fs::create_dir_all(&identity_dir).unwrap();
        // 32 bytes of deterministic test material; never a real key.
        std::fs::write(identity_dir.join("node_key"), [0xAB; 32]).unwrap();
        dir
    }

    #[tokio::test]
    async fn test_issue_credential_w3c_format() {
        let dir = test_dir_with_node_key();
        let vc = issue_credential(
            dir.path(),
            "did:key:issuer",
            "did:key:subject",
            "NodeOperator",
            serde_json::json!({"role": "admin"}),
            Some("2027-12-31T23:59:59Z"),
            |_bytes| Ok("mock-signature".to_string()),
        )
        .await
        .unwrap();

        assert!(vc.id.starts_with("urn:uuid:"));
        assert_eq!(vc.context[0], VC_CONTEXT_V2);
        assert_eq!(vc.context[1], ED25519_CONTEXT);
        assert_eq!(
            vc.credential_type,
            vec!["VerifiableCredential", "NodeOperator"]
        );
        assert_eq!(vc.issuer, "did:key:issuer");
        assert_eq!(vc.credential_subject.id, "did:key:subject");
        assert_eq!(vc.proof.proof_type, "Ed25519Signature2020");
        assert_eq!(vc.proof.proof_purpose, "assertionMethod");
        assert_eq!(vc.proof.verification_method, "did:key:issuer#key-1");
        assert_eq!(vc.proof.proof_value, "mock-signature");
        assert_eq!(vc.expiration_date, Some("2027-12-31T23:59:59Z".to_string()));
        assert!(vc.credential_status.is_none());
    }

    #[tokio::test]
    async fn test_issue_credential_serializes_as_jsonld() {
        let dir = test_dir_with_node_key();
        let vc = issue_credential(
            dir.path(),
            "did:key:issuer",
            "did:key:subject",
            "TestCred",
            serde_json::json!({"level": "gold"}),
            None,
            |_| Ok("sig".to_string()),
        )
        .await
        .unwrap();

        let json = serde_json::to_value(&vc).unwrap();
        assert!(json["@context"].is_array());
        assert!(json["type"].is_array());
        assert!(json["credentialSubject"]["id"].is_string());
        assert_eq!(json["proof"]["type"], "Ed25519Signature2020");
    }

    #[tokio::test]
    async fn test_save_and_load_roundtrip() {
        let dir = test_dir_with_node_key();
        issue_credential(
            dir.path(),
            "did:key:a",
            "did:key:b",
            "Type1",
            serde_json::json!({"k": "v"}),
            None,
            |_| Ok("s1".to_string()),
        )
        .await
        .unwrap();

        let loaded = load_credentials(dir.path()).await.unwrap();
        assert_eq!(loaded.credentials.len(), 1);
        assert_eq!(loaded.credentials[0].credential_type[1], "Type1");
    }

    #[tokio::test]
    async fn test_issue_credential_sign_fn_failure_propagates() {
        let dir = test_dir_with_node_key();
        let result = issue_credential(
            dir.path(),
            "did:key:issuer",
            "did:key:subject",
            "TestCredential",
            serde_json::json!({}),
            None,
            |_bytes| Err(anyhow::anyhow!("Signing failed")),
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Signing failed"));
    }

    #[test]
    fn test_verify_credential_calls_verify_fn() {
        let vc = VerifiableCredential {
            context: vec![VC_CONTEXT_V2.to_string()],
            id: "urn:uuid:test".to_string(),
            credential_type: vec!["VerifiableCredential".to_string(), "Test".to_string()],
            issuer: "did:key:issuer".to_string(),
            credential_subject: CredentialSubject {
                id: "did:key:subject".to_string(),
                claims: serde_json::json!({"foo": "bar"}),
            },
            issuance_date: "2025-06-01T00:00:00Z".to_string(),
            expiration_date: None,
            proof: CredentialProof {
                proof_type: "Ed25519Signature2020".to_string(),
                created: "2025-06-01T00:00:00Z".to_string(),
                verification_method: "did:key:issuer#key-1".to_string(),
                proof_purpose: "assertionMethod".to_string(),
                proof_value: "valid-sig".to_string(),
            },
            credential_status: None,
        };

        let result = verify_credential(&vc, |issuer, _data, sig| {
            assert_eq!(issuer, "did:key:issuer");
            assert_eq!(sig, "valid-sig");
            Ok(true)
        })
        .unwrap();
        assert!(result);

        let result = verify_credential(&vc, |_issuer, _data, _sig| Ok(false)).unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_revoke_credential() {
        let dir = test_dir_with_node_key();
        let vc = issue_credential(
            dir.path(),
            "did:key:issuer",
            "did:key:subject",
            "Revocable",
            serde_json::json!({}),
            None,
            |_| Ok("sig".to_string()),
        )
        .await
        .unwrap();

        assert!(!is_revoked(&vc));

        revoke_credential(dir.path(), &vc.id).await.unwrap();

        let store = load_credentials(dir.path()).await.unwrap();
        assert!(is_revoked(&store.credentials[0]));
        assert_eq!(
            store.credentials[0]
                .credential_status
                .as_ref()
                .unwrap()
                .status,
            "revoked"
        );
    }

    #[tokio::test]
    async fn test_revoke_nonexistent_credential_fails() {
        let dir = test_dir_with_node_key();
        let result = revoke_credential(dir.path(), "urn:uuid:does-not-exist").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Credential not found"));
    }

    #[tokio::test]
    async fn test_list_credentials_no_filter() {
        let dir = test_dir_with_node_key();
        issue_credential(
            dir.path(),
            "did:key:a",
            "did:key:b",
            "Type1",
            serde_json::json!({}),
            None,
            |_| Ok("s1".to_string()),
        )
        .await
        .unwrap();
        issue_credential(
            dir.path(),
            "did:key:c",
            "did:key:d",
            "Type2",
            serde_json::json!({}),
            None,
            |_| Ok("s2".to_string()),
        )
        .await
        .unwrap();

        let all = list_credentials(dir.path(), None).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_list_credentials_filter_by_did() {
        let dir = test_dir_with_node_key();
        issue_credential(
            dir.path(),
            "did:key:alice",
            "did:key:bob",
            "Type1",
            serde_json::json!({}),
            None,
            |_| Ok("s1".to_string()),
        )
        .await
        .unwrap();
        issue_credential(
            dir.path(),
            "did:key:carol",
            "did:key:alice",
            "Type2",
            serde_json::json!({}),
            None,
            |_| Ok("s2".to_string()),
        )
        .await
        .unwrap();
        issue_credential(
            dir.path(),
            "did:key:carol",
            "did:key:dave",
            "Type3",
            serde_json::json!({}),
            None,
            |_| Ok("s3".to_string()),
        )
        .await
        .unwrap();

        let filtered = list_credentials(dir.path(), Some("did:key:alice"))
            .await
            .unwrap();
        assert_eq!(filtered.len(), 2);
    }
}
