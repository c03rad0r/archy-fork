//! Detached Ed25519 signatures over canonical JSON documents.
//!
//! A *signed document* is any JSON object carrying two reserved top-level
//! fields:
//!
//!   * `signed_by` — the signer's `did:key` (Ed25519), e.g. the release-root.
//!   * `signature` — hex-encoded Ed25519 signature over the canonical JSON of
//!     the document with **both** reserved fields removed.
//!
//! Removing the fields before canonicalizing makes the signature *detached*:
//! the signer signs the payload, then attaches the proof, without a
//! chicken-and-egg dependency on the signature's own bytes.
//!
//! Authenticity ≠ integrity. Content addressing (SHA-256/BLAKE3 in the
//! manifest) proves the bytes are intact; this signature proves *we authorized
//! them*. The DHT plan requires both.

use anyhow::{anyhow, bail, Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey};
use serde_json::Value;

use super::anchor;
use super::canonical;
use super::did;

pub const SIGNATURE_FIELD: &str = "signature";
pub const SIGNED_BY_FIELD: &str = "signed_by";

/// Outcome of inspecting a document for a detached signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureStatus {
    /// No `signature` field present. Caller decides whether to accept
    /// (during the migration window we still accept unsigned documents).
    Unsigned,
    /// Signature verified. `anchored` is true when `signed_by` matched the
    /// pinned release-root anchor (full authenticity); false means the
    /// signature is internally consistent but the signer key is not yet
    /// pinned, so it only proves the document wasn't tampered relative to its
    /// own claimed key.
    Verified { signer_did: String, anchored: bool },
}

/// Verify a document's detached signature *if present*.
///
/// Returns `Ok(Unsigned)` when there is no signature. Returns `Ok(Verified)`
/// when a present signature checks out. Returns `Err` when a signature is
/// present but malformed, fails verification, or names a signer that
/// contradicts the pinned anchor — callers MUST reject the document on `Err`.
pub fn verify_detached(doc: &Value) -> Result<SignatureStatus> {
    let obj = doc
        .as_object()
        .ok_or_else(|| anyhow!("signed document must be a JSON object"))?;

    let signature_hex = match obj.get(SIGNATURE_FIELD) {
        None | Some(Value::Null) => return Ok(SignatureStatus::Unsigned),
        Some(Value::String(s)) => s,
        Some(_) => bail!("`{}` must be a string", SIGNATURE_FIELD),
    };

    let signed_by = obj
        .get(SIGNED_BY_FIELD)
        .and_then(Value::as_str)
        .ok_or_else(|| {
            anyhow!(
                "signed document has `{}` but no `{}`",
                SIGNATURE_FIELD,
                SIGNED_BY_FIELD
            )
        })?;

    let signer = did::ed25519_pubkey_from_did_key(signed_by)
        .with_context(|| format!("invalid `{}` did:key", SIGNED_BY_FIELD))?;

    // If the fleet has a pinned release-root, the signer MUST be it. This is
    // what stops a mirror from swapping in its own keypair and re-signing.
    let anchored = match anchor::release_root_pubkey() {
        Some(pinned) => {
            if pinned != signer {
                bail!("signed_by does not match the pinned release-root anchor");
            }
            true
        }
        None => false,
    };

    let signature = parse_signature_hex(signature_hex)?;
    let preimage = signing_preimage(obj)?;
    signer
        .verify_strict(&preimage, &signature)
        .map_err(|_| anyhow!("release-root signature verification failed"))?;

    Ok(SignatureStatus::Verified {
        signer_did: signed_by.to_string(),
        anchored,
    })
}

/// Produce a detached signature for `payload` (the document WITHOUT the
/// reserved fields). Used by the signing ceremony and round-trip tests.
/// Returns `(signature_hex, signed_by_did)`.
pub fn sign_detached(key: &SigningKey, payload: &Value) -> Result<(String, String)> {
    let obj = payload
        .as_object()
        .ok_or_else(|| anyhow!("payload must be a JSON object"))?;
    if obj.contains_key(SIGNATURE_FIELD) || obj.contains_key(SIGNED_BY_FIELD) {
        bail!("payload must not already contain reserved signature fields");
    }
    let preimage = signing_preimage(obj)?;
    let signature = key.sign(&preimage);
    let did = did::did_key_for_ed25519(&key.verifying_key());
    Ok((hex::encode(signature.to_bytes()), did))
}

/// Canonical bytes the signature covers: the object minus the reserved fields.
fn signing_preimage(obj: &serde_json::Map<String, Value>) -> Result<Vec<u8>> {
    let mut payload = obj.clone();
    payload.remove(SIGNATURE_FIELD);
    payload.remove(SIGNED_BY_FIELD);
    let value = Value::Object(payload);
    if canonical::contains_float(&value) {
        bail!("signed documents must not contain floating-point numbers");
    }
    Ok(canonical::to_canonical_bytes(&value))
}

fn parse_signature_hex(s: &str) -> Result<Signature> {
    let bytes = hex::decode(s).context("signature is not valid hex")?;
    let arr: [u8; 64] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("signature must be 64 bytes, got {}", bytes.len()))?;
    Ok(Signature::from_bytes(&arr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn sign_into(key: &SigningKey, mut doc: Value) -> Value {
        let (sig, did) = sign_detached(key, &doc).unwrap();
        let obj = doc.as_object_mut().unwrap();
        obj.insert(SIGNED_BY_FIELD.into(), json!(did));
        obj.insert(SIGNATURE_FIELD.into(), json!(sig));
        doc
    }

    /// Pin `test_key` as the release-root anchor for this process via the env
    /// override. Needed because the baked-in `RELEASE_ROOT_PUBKEY_HEX` is the
    /// real ceremony key, which no unit test can produce signatures for — so to
    /// exercise the anchored-verification path we pin a key we can sign with.
    /// Every call sets the same value, so parallel tests stay consistent.
    fn pin_test_key_as_anchor() {
        std::env::set_var(
            "ARCHY_RELEASE_ROOT_PUBKEY",
            hex::encode(test_key().verifying_key().to_bytes()),
        );
    }

    #[test]
    fn unsigned_document_reports_unsigned() {
        let doc = json!({"schema": 1, "apps": {}});
        assert_eq!(verify_detached(&doc).unwrap(), SignatureStatus::Unsigned);
    }

    #[test]
    fn roundtrip_verifies_and_anchors_to_pinned_key() {
        // With the anchor pinned to the signer, verification succeeds AND
        // reports anchored == true (signer identity confirmed).
        pin_test_key_as_anchor();
        let signed = sign_into(&test_key(), json!({"schema": 1, "n": 42}));
        match verify_detached(&signed).unwrap() {
            SignatureStatus::Verified { anchored, .. } => assert!(anchored),
            other => panic!("expected Verified, got {:?}", other),
        }
    }

    #[test]
    fn signature_from_non_anchor_key_is_rejected() {
        // A self-consistent signature from a key that is NOT the pinned anchor
        // must hard-reject — this is what stops a mirror swapping in its own key.
        pin_test_key_as_anchor();
        let other_key = SigningKey::from_bytes(&[11u8; 32]);
        let signed = sign_into(&other_key, json!({"schema": 1, "n": 42}));
        assert!(verify_detached(&signed).is_err());
    }

    #[test]
    fn signature_survives_key_reordering() {
        // Re-emitting the document with shuffled keys must not break the sig.
        pin_test_key_as_anchor();
        let signed = sign_into(&test_key(), json!({"b": 2, "a": 1}));
        let reparsed: Value =
            serde_json::from_str(&serde_json::to_string(&signed).unwrap()).unwrap();
        assert!(matches!(
            verify_detached(&reparsed).unwrap(),
            SignatureStatus::Verified { .. }
        ));
    }

    #[test]
    fn tampered_payload_is_rejected() {
        // Pin the signer so verification reaches the signature check (not an
        // anchor-identity short-circuit), proving tamper detection itself.
        pin_test_key_as_anchor();
        let mut signed = sign_into(&test_key(), json!({"schema": 1, "n": 42}));
        signed
            .as_object_mut()
            .unwrap()
            .insert("n".into(), json!(43));
        assert!(verify_detached(&signed).is_err());
    }

    #[test]
    fn missing_signed_by_is_rejected() {
        let doc = json!({"schema": 1, "signature": "00"});
        assert!(verify_detached(&doc).is_err());
    }

    #[test]
    fn float_payload_cannot_be_signed() {
        assert!(sign_detached(&test_key(), &json!({"x": 1.5})).is_err());
    }
}
