//! The fleet's pinned **release-root** trust anchor.
//!
//! Every node ships the release-root *public* key. Signed manifests and the app
//! catalog must be signed by the corresponding private key (derived once, in
//! the signing ceremony, via `seed::derive_release_root_ed25519`). Pinning the
//! key in the binary is what makes a swapped-in mirror key detectable.
//!
//! Until the ceremony runs against the real release master seed, the pinned
//! constant is `None`. While `None`, signature verification still runs and
//! still rejects tampered documents, but it cannot enforce signer *identity*
//! (see `signed_doc::SignatureStatus::anchored`). Set
//! `ARCHY_RELEASE_ROOT_PUBKEY` (64-char hex) to pin a key at runtime for
//! staging/test fleets before the constant is baked in.

use ed25519_dalek::VerifyingKey;

/// Hex of the pinned Ed25519 release-root public key (32 bytes / 64 hex chars).
///
/// ROTATED 2026-08-04 to did:key:z6Mkfu5LT8d4DjETtrkATvHh9Dvcbnr7zBCUwfau8Sw7DLWT.
///
/// The previous root (z6Mkkid…q7ur, pinned 2026-07-02) was exposed in a chat
/// transcript and is treated as compromised.
///
/// Rotation is ORDERING-CRITICAL. Nodes pin the OLD key, so the release that
/// carries this change must itself be signed with the OLD key — that is the
/// only signature a node running the previous binary will accept. Only the
/// release AFTER it may be signed with the new key. Signing the rotation
/// release with the new key makes every node reject it and ends OTA
/// fleet-wide, recoverable only by touching each node by hand.
///
/// Verified before pinning: this hex and the did:key above are the same
/// keypair (the did:key encodes exactly these 32 bytes), checked with a
/// decoder round-tripped against the previous known-good pair. An earlier
/// candidate hex was rejected because it did not match the stated DID.
/// The
/// corresponding mnemonic is held offline by the publisher — see
/// `docs/workstream-b-signing-runbook.md`. Regenerate/verify with:
/// `RELEASE_MASTER_MNEMONIC=… archipelago ceremony pubkey`.
pub const RELEASE_ROOT_PUBKEY_HEX: Option<&str> =
    Some("1578adccf137024159dd936f44a56e8869ac7775785962f7e92e2faf2c034418");

const ENV_OVERRIDE: &str = "ARCHY_RELEASE_ROOT_PUBKEY";

/// Resolve the pinned release-root public key, if any.
///
/// Runtime env override wins over the baked-in constant so a test fleet can pin
/// a ceremony key without a rebuild. Malformed values are ignored (treated as
/// "not pinned") rather than crashing the node.
pub fn release_root_pubkey() -> Option<VerifyingKey> {
    if let Ok(hex_str) = std::env::var(ENV_OVERRIDE) {
        if let Some(key) = parse_pubkey_hex(hex_str.trim()) {
            return Some(key);
        }
        tracing::warn!(
            "{} is set but not a valid 32-byte hex Ed25519 key; ignoring",
            ENV_OVERRIDE
        );
    }
    RELEASE_ROOT_PUBKEY_HEX.and_then(parse_pubkey_hex)
}

fn parse_pubkey_hex(s: &str) -> Option<VerifyingKey> {
    let bytes = hex::decode(s).ok()?;
    let arr: [u8; 32] = bytes.as_slice().try_into().ok()?;
    VerifyingKey::from_bytes(&arr).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_constant_parses_to_a_valid_key() {
        // The release-root anchor is pinned (ceremony 2026-07-02); it must be
        // present and a well-formed 32-byte Ed25519 key.
        let hex = RELEASE_ROOT_PUBKEY_HEX.expect("release-root anchor must be pinned");
        assert!(
            parse_pubkey_hex(hex).is_some(),
            "pinned RELEASE_ROOT_PUBKEY_HEX is not a valid Ed25519 key"
        );
    }

    #[test]
    fn parses_valid_hex() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]).verifying_key();
        let parsed = parse_pubkey_hex(&hex::encode(key.to_bytes())).unwrap();
        assert_eq!(parsed.as_bytes(), key.as_bytes());
    }

    #[test]
    fn rejects_malformed_hex() {
        assert!(parse_pubkey_hex("nothex").is_none());
        assert!(parse_pubkey_hex("abcd").is_none());
    }
}
