use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::operations::{is_revoked, verify_credential};
use super::types::*;

/// A Verifiable Presentation following W3C VC Data Model 2.0.
/// Bundles one or more VCs with a holder proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiablePresentation {
    #[serde(rename = "@context")]
    pub context: Vec<String>,
    pub id: String,
    #[serde(rename = "type")]
    pub presentation_type: Vec<String>,
    pub holder: String,
    pub verifiable_credential: Vec<VerifiableCredential>,
    pub proof: CredentialProof,
}

/// Create a Verifiable Presentation wrapping selected credentials.
/// The holder signs the presentation to prove they possess the credentials.
pub fn create_presentation(
    holder_did: &str,
    credential_ids: &[&str],
    credentials: &[VerifiableCredential],
    sign_fn: impl FnOnce(&[u8]) -> Result<String>,
) -> Result<VerifiablePresentation> {
    let selected: Vec<VerifiableCredential> = credentials
        .iter()
        .filter(|c| credential_ids.contains(&c.id.as_str()))
        .cloned()
        .collect();

    if selected.is_empty() {
        return Err(anyhow::anyhow!("No matching credentials found"));
    }

    let id = format!("urn:uuid:{}", uuid::Uuid::new_v4());
    let created = chrono::Utc::now().to_rfc3339();
    let key_id = format!("{}#key-1", holder_did);

    let body = serde_json::json!({
        "@context": [VC_CONTEXT_V2, ED25519_CONTEXT],
        "id": id,
        "type": ["VerifiablePresentation"],
        "holder": holder_did,
        "verifiableCredential": selected,
    });
    let body_bytes = serde_json::to_vec(&body)?;
    let signature = sign_fn(&body_bytes)?;

    Ok(VerifiablePresentation {
        context: vec![VC_CONTEXT_V2.to_string(), ED25519_CONTEXT.to_string()],
        id,
        presentation_type: vec!["VerifiablePresentation".to_string()],
        holder: holder_did.to_string(),
        verifiable_credential: selected,
        proof: CredentialProof {
            proof_type: "Ed25519Signature2020".to_string(),
            created,
            verification_method: key_id,
            proof_purpose: "authentication".to_string(),
            proof_value: signature,
        },
    })
}

/// Verify a Verifiable Presentation: check holder's proof signature,
/// then verify each embedded credential.
pub fn verify_presentation(
    vp: &VerifiablePresentation,
    verify_fn: impl Fn(&str, &[u8], &str) -> Result<bool>,
) -> Result<PresentationVerification> {
    let body = serde_json::json!({
        "@context": vp.context,
        "id": vp.id,
        "type": vp.presentation_type,
        "holder": vp.holder,
        "verifiableCredential": vp.verifiable_credential,
    });
    let body_bytes = serde_json::to_vec(&body)?;
    let holder_valid = verify_fn(&vp.holder, &body_bytes, &vp.proof.proof_value)?;

    let mut credential_results = Vec::new();
    for vc in &vp.verifiable_credential {
        let vc_valid = verify_credential(vc, |did, bytes, sig| verify_fn(did, bytes, sig))?;
        credential_results.push(CredentialVerificationResult {
            id: vc.id.clone(),
            valid: vc_valid,
            revoked: is_revoked(vc),
        });
    }

    let all_valid = holder_valid && credential_results.iter().all(|r| r.valid && !r.revoked);

    Ok(PresentationVerification {
        holder_valid,
        credentials: credential_results,
        valid: all_valid,
    })
}

/// Result of verifying a Verifiable Presentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresentationVerification {
    pub holder_valid: bool,
    pub credentials: Vec<CredentialVerificationResult>,
    pub valid: bool,
}

/// Result of verifying a single credential within a presentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialVerificationResult {
    pub id: String,
    pub valid: bool,
    pub revoked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_vc(id: &str, issuer: &str, subject: &str) -> VerifiableCredential {
        VerifiableCredential {
            context: vec![VC_CONTEXT_V2.to_string(), ED25519_CONTEXT.to_string()],
            id: id.to_string(),
            credential_type: vec!["VerifiableCredential".to_string(), "Test".to_string()],
            issuer: issuer.to_string(),
            credential_subject: CredentialSubject {
                id: subject.to_string(),
                claims: serde_json::json!({"role": "tester"}),
            },
            issuance_date: "2026-01-01T00:00:00Z".to_string(),
            expiration_date: None,
            proof: CredentialProof {
                proof_type: "Ed25519Signature2020".to_string(),
                created: "2026-01-01T00:00:00Z".to_string(),
                verification_method: format!("{}#key-1", issuer),
                proof_purpose: "assertionMethod".to_string(),
                proof_value: "mock-sig".to_string(),
            },
            credential_status: None,
        }
    }

    #[test]
    fn test_create_presentation() {
        let creds = vec![
            make_test_vc("urn:uuid:cred1", "did:key:issuer1", "did:key:holder"),
            make_test_vc("urn:uuid:cred2", "did:key:issuer2", "did:key:holder"),
        ];

        let vp = create_presentation("did:key:holder", &["urn:uuid:cred1"], &creds, |_bytes| {
            Ok("presentation-sig".to_string())
        })
        .unwrap();

        assert!(vp.id.starts_with("urn:uuid:"));
        assert_eq!(vp.presentation_type, vec!["VerifiablePresentation"]);
        assert_eq!(vp.holder, "did:key:holder");
        assert_eq!(vp.verifiable_credential.len(), 1);
        assert_eq!(vp.verifiable_credential[0].id, "urn:uuid:cred1");
        assert_eq!(vp.proof.proof_type, "Ed25519Signature2020");
        assert_eq!(vp.proof.proof_purpose, "authentication");
        assert_eq!(vp.proof.proof_value, "presentation-sig");
        assert_eq!(vp.context[0], VC_CONTEXT_V2);
    }

    #[test]
    fn test_create_presentation_multiple_credentials() {
        let creds = vec![
            make_test_vc("urn:uuid:c1", "did:key:i1", "did:key:holder"),
            make_test_vc("urn:uuid:c2", "did:key:i2", "did:key:holder"),
            make_test_vc("urn:uuid:c3", "did:key:i3", "did:key:other"),
        ];

        let vp = create_presentation(
            "did:key:holder",
            &["urn:uuid:c1", "urn:uuid:c2"],
            &creds,
            |_| Ok("sig".to_string()),
        )
        .unwrap();

        assert_eq!(vp.verifiable_credential.len(), 2);
    }

    #[test]
    fn test_create_presentation_no_matching_credentials() {
        let creds = vec![make_test_vc("urn:uuid:c1", "did:key:i", "did:key:h")];

        let result =
            create_presentation("did:key:holder", &["urn:uuid:nonexistent"], &creds, |_| {
                Ok("sig".to_string())
            });
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No matching credentials"));
    }

    #[test]
    fn test_verify_presentation_all_valid() {
        let creds = vec![make_test_vc(
            "urn:uuid:c1",
            "did:key:issuer",
            "did:key:holder",
        )];

        let vp = create_presentation("did:key:holder", &["urn:uuid:c1"], &creds, |_| {
            Ok("vp-sig".to_string())
        })
        .unwrap();

        let result = verify_presentation(&vp, |_did, _bytes, _sig| Ok(true)).unwrap();
        assert!(result.holder_valid);
        assert!(result.valid);
        assert_eq!(result.credentials.len(), 1);
        assert!(result.credentials[0].valid);
        assert!(!result.credentials[0].revoked);
    }

    #[test]
    fn test_verify_presentation_holder_invalid() {
        let creds = vec![make_test_vc(
            "urn:uuid:c1",
            "did:key:issuer",
            "did:key:holder",
        )];

        let vp = create_presentation("did:key:holder", &["urn:uuid:c1"], &creds, |_| {
            Ok("bad-sig".to_string())
        })
        .unwrap();

        let result =
            verify_presentation(&vp, |did, _bytes, _sig| Ok(did != "did:key:holder")).unwrap();

        assert!(!result.holder_valid);
        assert!(!result.valid);
    }

    #[test]
    fn test_presentation_serializes_as_jsonld() {
        let creds = vec![make_test_vc(
            "urn:uuid:c1",
            "did:key:issuer",
            "did:key:holder",
        )];

        let vp = create_presentation("did:key:holder", &["urn:uuid:c1"], &creds, |_| {
            Ok("sig".to_string())
        })
        .unwrap();

        let json = serde_json::to_value(&vp).unwrap();
        assert!(json["@context"].is_array());
        assert!(json["type"].is_array());
        assert_eq!(json["type"][0], "VerifiablePresentation");
        assert!(json["holder"].is_string());
        assert!(json["verifiableCredential"].is_array());
        assert!(json["proof"]["type"].is_string());
    }
}
