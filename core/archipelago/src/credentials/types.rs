use serde::{Deserialize, Serialize};

pub(super) const CREDENTIALS_DIR: &str = "credentials";

/// W3C VC Data Model 2.0 context URI
pub(super) const VC_CONTEXT_V2: &str = "https://www.w3.org/ns/credentials/v2";
/// Ed25519 signature suite context
pub(super) const ED25519_CONTEXT: &str = "https://w3id.org/security/suites/ed25519-2020/v1";

/// A Verifiable Credential following W3C VC Data Model 2.0.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiableCredential {
    #[serde(rename = "@context")]
    pub context: Vec<String>,
    pub id: String,
    #[serde(rename = "type")]
    pub credential_type: Vec<String>,
    pub issuer: String,
    pub credential_subject: CredentialSubject,
    pub issuance_date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_date: Option<String>,
    pub proof: CredentialProof,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_status: Option<CredentialStatusEntry>,
}

/// The subject of a credential with their DID and claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialSubject {
    pub id: String,
    #[serde(flatten)]
    pub claims: serde_json::Value,
}

/// Ed25519Signature2020 proof format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialProof {
    #[serde(rename = "type")]
    pub proof_type: String,
    pub created: String,
    pub verification_method: String,
    pub proof_purpose: String,
    pub proof_value: String,
}

/// Credential status for revocation tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatusEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub status_type: String,
    pub status: String,
}

/// Status of a verifiable credential.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CredentialStatus {
    Active,
    Revoked,
    Expired,
}

impl std::fmt::Display for CredentialStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Revoked => write!(f, "revoked"),
            Self::Expired => write!(f, "expired"),
        }
    }
}

/// Stored credentials index.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CredentialStore {
    pub credentials: Vec<VerifiableCredential>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credential_status_display() {
        assert_eq!(CredentialStatus::Active.to_string(), "active");
        assert_eq!(CredentialStatus::Revoked.to_string(), "revoked");
        assert_eq!(CredentialStatus::Expired.to_string(), "expired");
    }

    #[test]
    fn test_credential_status_serde_roundtrip() {
        let json = serde_json::to_string(&CredentialStatus::Revoked).unwrap();
        assert_eq!(json, "\"revoked\"");
        let parsed: CredentialStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, CredentialStatus::Revoked);
    }

    #[test]
    fn test_credential_store_default_is_empty() {
        let store = CredentialStore::default();
        assert!(store.credentials.is_empty());
    }
}
