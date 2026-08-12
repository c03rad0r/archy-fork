//! `did:key` <-> Ed25519 public key, mirroring the encoding already used by
//! `identity_manager` so release-root DIDs are interchangeable with node DIDs.
//!
//! Format: `did:key:z<base58btc(0xed01 || 32-byte-pubkey)>`
//! (`0xed01` is the multicodec varint prefix for an Ed25519 public key.)

use anyhow::{anyhow, Context, Result};
use ed25519_dalek::VerifyingKey;

const ED25519_MULTICODEC: [u8; 2] = [0xed, 0x01];

/// Encode an Ed25519 public key as a `did:key` string.
pub fn did_key_for_ed25519(key: &VerifyingKey) -> String {
    let mut bytes = Vec::with_capacity(34);
    bytes.extend_from_slice(&ED25519_MULTICODEC);
    bytes.extend_from_slice(key.as_bytes());
    format!("did:key:z{}", bs58::encode(bytes).into_string())
}

/// Decode a `did:key` string into an Ed25519 verifying key.
pub fn ed25519_pubkey_from_did_key(did: &str) -> Result<VerifyingKey> {
    let z_part = did
        .strip_prefix("did:key:z")
        .ok_or_else(|| anyhow!("invalid did:key format: {}", did))?;
    let decoded = bs58::decode(z_part)
        .into_vec()
        .context("invalid base58 in did:key")?;
    if decoded.len() != 34 || decoded[0..2] != ED25519_MULTICODEC {
        return Err(anyhow!("not an Ed25519 did:key (bad multicodec prefix)"));
    }
    let arr: [u8; 32] = decoded[2..].try_into().expect("length checked above");
    VerifyingKey::from_bytes(&arr).context("invalid Ed25519 public key in did:key")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[test]
    fn roundtrip() {
        let key = SigningKey::from_bytes(&[3u8; 32]).verifying_key();
        let did = did_key_for_ed25519(&key);
        assert!(did.starts_with("did:key:z6Mk"), "got {}", did);
        let back = ed25519_pubkey_from_did_key(&did).unwrap();
        assert_eq!(key.as_bytes(), back.as_bytes());
    }

    #[test]
    fn rejects_non_ed25519() {
        assert!(ed25519_pubkey_from_did_key("did:key:zQ3shazz").is_err());
        assert!(ed25519_pubkey_from_did_key("not-a-did").is_err());
    }
}
