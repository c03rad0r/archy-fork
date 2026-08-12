// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! X3DH (Extended Triple Diffie-Hellman) key agreement for mesh sessions.
//!
//! Implements the Signal protocol's X3DH using existing Ed25519/X25519 identity
//! infrastructure. Produces a shared root key that initializes the Double Ratchet.
//!
//! Protocol flow:
//! 1. Alice publishes prekey bundle (identity key + signed prekey + one-time prekeys)
//! 2. Bob fetches bundle, performs 3-way ECDH, sends initial message
//! 3. Both derive identical root key via HKDF-SHA256

use super::crypto;
use anyhow::{Context, Result};
use ed25519_dalek::Signer;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

/// Info string for HKDF domain separation.
const X3DH_INFO: &[u8] = b"ArchipelagoX3DH_v1";

/// Salt for HKDF (all zeros per Signal spec).
const X3DH_SALT: [u8; 32] = [0u8; 32];

/// A signed prekey (rotated periodically).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedPrekey {
    pub id: u32,
    #[serde(with = "hex_array")]
    pub public: [u8; 32],
    /// Ed25519 signature of the public key bytes.
    #[serde(with = "hex_vec")]
    pub signature: Vec<u8>,
}

/// A one-time prekey (consumed on first use).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OneTimePrekey {
    pub id: u32,
    #[serde(with = "hex_array")]
    pub public: [u8; 32],
}

/// Published prekey bundle for initiating sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrekeyBundle {
    /// Ed25519 identity public key (verifying key).
    #[serde(with = "hex_array")]
    pub identity_key: [u8; 32],
    /// X25519 identity public key (derived from Ed25519).
    #[serde(with = "hex_array")]
    pub x25519_identity: [u8; 32],
    /// Signed prekey for DH.
    pub signed_prekey: SignedPrekey,
    /// Available one-time prekeys.
    pub one_time_prekeys: Vec<OneTimePrekey>,
}

/// X3DH output: shared root key for initializing Double Ratchet.
pub struct X3dhOutput {
    pub root_key: [u8; 32],
    /// The signed prekey used (needed for receiver to identify which session).
    pub signed_prekey_id: u32,
    /// The one-time prekey consumed (if any).
    pub one_time_prekey_id: Option<u32>,
}

impl Drop for X3dhOutput {
    fn drop(&mut self) {
        self.root_key.zeroize();
    }
}

/// Secret-side prekey data (kept by the bundle publisher).
pub struct PrekeySecrets {
    pub signed_prekey_secret: [u8; 32],
    pub signed_prekey_id: u32,
    pub one_time_secrets: Vec<(u32, [u8; 32])>,
}

impl Drop for PrekeySecrets {
    fn drop(&mut self) {
        self.signed_prekey_secret.zeroize();
        for (_, secret) in &mut self.one_time_secrets {
            secret.zeroize();
        }
    }
}

/// Generate a prekey bundle and corresponding secrets.
pub fn generate_prekey_bundle(
    identity_signing_key: &ed25519_dalek::SigningKey,
    num_one_time_prekeys: u32,
) -> Result<(PrekeyBundle, PrekeySecrets)> {
    let identity_key = identity_signing_key.verifying_key().to_bytes();
    let x25519_identity = crypto::ed25519_pubkey_to_x25519(&identity_key)?;

    // Generate signed prekey
    let (spk_secret, spk_public) = crypto::generate_x25519_ephemeral();
    // KEY-05: source named. This is a 4-byte prekey *identifier*, not key
    // material — the X25519 secret is the line above — so it is drawn unguarded:
    // the degenerate predicate's false-positive bound does not hold below 12
    // bytes. See the classification table in
    // docs/security/KEY-05-ENTROPY-ENFORCEMENT.md.
    let spk_id: u32 = rand::RngCore::next_u32(&mut rand::rngs::OsRng);
    let signature = identity_signing_key.sign(&spk_public);

    let signed_prekey = SignedPrekey {
        id: spk_id,
        public: spk_public,
        signature: signature.to_bytes().to_vec(),
    };

    // Generate one-time prekeys
    let mut one_time_prekeys = Vec::with_capacity(num_one_time_prekeys as usize);
    let mut one_time_secrets = Vec::with_capacity(num_one_time_prekeys as usize);
    for _ in 0..num_one_time_prekeys {
        let (otk_secret, otk_public) = crypto::generate_x25519_ephemeral();
        // KEY-05: source named; unguarded for the same reason as `spk_id` above.
        let otk_id: u32 = rand::RngCore::next_u32(&mut rand::rngs::OsRng);
        one_time_prekeys.push(OneTimePrekey {
            id: otk_id,
            public: otk_public,
        });
        one_time_secrets.push((otk_id, otk_secret));
    }

    let bundle = PrekeyBundle {
        identity_key,
        x25519_identity,
        signed_prekey,
        one_time_prekeys,
    };

    let secrets = PrekeySecrets {
        signed_prekey_secret: spk_secret,
        signed_prekey_id: spk_id,
        one_time_secrets,
    };

    Ok((bundle, secrets))
}

/// Verify a prekey bundle's signed prekey signature.
pub fn verify_bundle(bundle: &PrekeyBundle) -> Result<()> {
    use ed25519_dalek::{Signature, VerifyingKey};

    let verifying_key = VerifyingKey::from_bytes(&bundle.identity_key)
        .context("Invalid identity key in prekey bundle")?;
    let signature = Signature::from_slice(&bundle.signed_prekey.signature)
        .context("Invalid signature in prekey bundle")?;

    verifying_key
        .verify_strict(&bundle.signed_prekey.public, &signature)
        .context("Prekey bundle signature verification failed")?;

    Ok(())
}

/// Initiator side: perform X3DH to derive a shared root key.
///
/// Called by the party starting a new session (Bob initiates to Alice).
/// Returns the X3DH output and the ephemeral public key that must be sent
/// to the receiver alongside the first encrypted message.
pub fn initiate(
    our_x25519_secret: &[u8; 32],
    their_bundle: &PrekeyBundle,
) -> Result<(X3dhOutput, [u8; 32])> {
    // Verify the bundle's signed prekey signature
    verify_bundle(their_bundle)?;

    // Generate ephemeral keypair for this session
    let (eph_secret, eph_public) = crypto::generate_x25519_ephemeral();

    // Three (or four) DH operations:
    // DH1 = X25519(our_identity_x25519, their_signed_prekey)
    let dh1 = crypto::x25519_shared_secret(our_x25519_secret, &their_bundle.signed_prekey.public);
    // DH2 = X25519(ephemeral_secret, their_identity_x25519)
    let dh2 = crypto::x25519_shared_secret(&eph_secret, &their_bundle.x25519_identity);
    // DH3 = X25519(ephemeral_secret, their_signed_prekey)
    let dh3 = crypto::x25519_shared_secret(&eph_secret, &their_bundle.signed_prekey.public);

    // Concatenate DH results
    let mut ikm = Vec::with_capacity(32 * 4);
    ikm.extend_from_slice(&dh1);
    ikm.extend_from_slice(&dh2);
    ikm.extend_from_slice(&dh3);

    // DH4 with one-time prekey if available
    let otk_id = if let Some(otk) = their_bundle.one_time_prekeys.first() {
        let dh4 = crypto::x25519_shared_secret(&eph_secret, &otk.public);
        ikm.extend_from_slice(&dh4);
        Some(otk.id)
    } else {
        None
    };

    // Derive root key via HKDF
    let root_key = crypto::hkdf_sha256_32(&X3DH_SALT, &ikm, X3DH_INFO)?;

    // Zeroize intermediate material
    ikm.zeroize();

    let output = X3dhOutput {
        root_key,
        signed_prekey_id: their_bundle.signed_prekey.id,
        one_time_prekey_id: otk_id,
    };

    Ok((output, eph_public))
}

/// Receiver side: perform X3DH to derive the same shared root key.
///
/// Called when receiving the first message of a new session from an initiator.
pub fn respond(
    our_signed_prekey_secret: &[u8; 32],
    our_x25519_identity_secret: &[u8; 32],
    our_one_time_secret: Option<&[u8; 32]>,
    their_identity_x25519: &[u8; 32],
    their_ephemeral_public: &[u8; 32],
) -> Result<X3dhOutput> {
    // Mirror the initiator's DH operations:
    // DH1 = X25519(our_signed_prekey_secret, their_identity_x25519)
    let dh1 = crypto::x25519_shared_secret(our_signed_prekey_secret, their_identity_x25519);
    // DH2 = X25519(our_identity_x25519_secret, their_ephemeral)
    let dh2 = crypto::x25519_shared_secret(our_x25519_identity_secret, their_ephemeral_public);
    // DH3 = X25519(our_signed_prekey_secret, their_ephemeral)
    let dh3 = crypto::x25519_shared_secret(our_signed_prekey_secret, their_ephemeral_public);

    let mut ikm = Vec::with_capacity(32 * 4);
    ikm.extend_from_slice(&dh1);
    ikm.extend_from_slice(&dh2);
    ikm.extend_from_slice(&dh3);

    if let Some(otk_secret) = our_one_time_secret {
        let dh4 = crypto::x25519_shared_secret(otk_secret, their_ephemeral_public);
        ikm.extend_from_slice(&dh4);
    }

    let root_key = crypto::hkdf_sha256_32(&X3DH_SALT, &ikm, X3DH_INFO)?;
    ikm.zeroize();

    Ok(X3dhOutput {
        root_key,
        signed_prekey_id: 0, // Not needed on receiver side
        one_time_prekey_id: None,
    })
}

/// Encode a prekey bundle to CBOR bytes for mesh transmission.
pub fn encode_bundle(bundle: &PrekeyBundle) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    ciborium::into_writer(bundle, &mut buf).context("Failed to CBOR-encode prekey bundle")?;
    Ok(buf)
}

/// Decode a prekey bundle from CBOR bytes.
pub fn decode_bundle(data: &[u8]) -> Result<PrekeyBundle> {
    ciborium::from_reader(data).context("Failed to CBOR-decode prekey bundle")
}

// ─── Hex serialization helpers ──────────────────────────────────────────

mod hex_array {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("expected 32 bytes"));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }
}

mod hex_vec {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        hex::decode(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn test_generate_and_verify_bundle() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let (bundle, _secrets) = generate_prekey_bundle(&signing_key, 5).unwrap();

        assert_eq!(bundle.one_time_prekeys.len(), 5);
        assert!(verify_bundle(&bundle).is_ok());
    }

    #[test]
    fn test_x3dh_both_sides_derive_same_key() {
        let alice_signing = SigningKey::generate(&mut OsRng);
        let bob_signing = SigningKey::generate(&mut OsRng);

        // Alice publishes bundle
        let (bundle, secrets) = generate_prekey_bundle(&alice_signing, 3).unwrap();

        // Bob initiates X3DH
        let bob_x25519_secret = crypto::ed25519_secret_to_x25519(&bob_signing);
        let (bob_output, bob_ephemeral) = initiate(&bob_x25519_secret, &bundle).unwrap();

        // Alice responds
        let alice_x25519_secret = crypto::ed25519_secret_to_x25519(&alice_signing);
        let bob_x25519_public =
            crypto::ed25519_pubkey_to_x25519(&bob_signing.verifying_key().to_bytes()).unwrap();

        let otk_secret = secrets.one_time_secrets.first().map(|(_, s)| s);
        let alice_output = respond(
            &secrets.signed_prekey_secret,
            &alice_x25519_secret,
            otk_secret,
            &bob_x25519_public,
            &bob_ephemeral,
        )
        .unwrap();

        // Both should derive the same root key
        assert_eq!(bob_output.root_key, alice_output.root_key);
    }

    #[test]
    fn test_x3dh_without_one_time_prekey() {
        let alice_signing = SigningKey::generate(&mut OsRng);
        let bob_signing = SigningKey::generate(&mut OsRng);

        // Alice publishes bundle with zero one-time prekeys
        let (bundle, secrets) = generate_prekey_bundle(&alice_signing, 0).unwrap();

        let bob_x25519_secret = crypto::ed25519_secret_to_x25519(&bob_signing);
        let (bob_output, bob_ephemeral) = initiate(&bob_x25519_secret, &bundle).unwrap();

        let alice_x25519_secret = crypto::ed25519_secret_to_x25519(&alice_signing);
        let bob_x25519_public =
            crypto::ed25519_pubkey_to_x25519(&bob_signing.verifying_key().to_bytes()).unwrap();

        let alice_output = respond(
            &secrets.signed_prekey_secret,
            &alice_x25519_secret,
            None,
            &bob_x25519_public,
            &bob_ephemeral,
        )
        .unwrap();

        assert_eq!(bob_output.root_key, alice_output.root_key);
    }

    #[test]
    fn test_bundle_cbor_roundtrip() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let (bundle, _) = generate_prekey_bundle(&signing_key, 3).unwrap();

        let encoded = encode_bundle(&bundle).unwrap();
        let decoded = decode_bundle(&encoded).unwrap();

        assert_eq!(decoded.identity_key, bundle.identity_key);
        assert_eq!(decoded.signed_prekey.id, bundle.signed_prekey.id);
        assert_eq!(decoded.one_time_prekeys.len(), 3);
    }

    #[test]
    fn test_tampered_bundle_fails_verification() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let (mut bundle, _) = generate_prekey_bundle(&signing_key, 1).unwrap();

        // Tamper with signed prekey public key
        bundle.signed_prekey.public[0] ^= 0xFF;

        assert!(verify_bundle(&bundle).is_err());
    }

    /// KEY-05: the prekey bundle crosses the wire to other nodes, so the entropy
    /// migration must be provably source-only. This pins the serialised field
    /// **set, types and ordering** — a later refactor that reshapes the bundle
    /// while "just" touching the RNG fails here rather than silently breaking
    /// every peer that already holds the old shape.
    #[test]
    fn prekey_bundle_wire_shape_unchanged() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let (bundle, _secrets) = generate_prekey_bundle(&signing_key, 2).unwrap();
        let json = serde_json::to_string(&bundle).unwrap();

        // Ordering. Checked against the emitted *string*, not a parsed
        // `serde_json::Value`: `Value`'s map is a `BTreeMap` unless the
        // `preserve_order` feature happens to be unified on, so a `Value` would
        // silently assert alphabetical order instead of declaration order. The
        // serialised text is what actually goes on the wire.
        let pos = |k: &str| json.find(k).unwrap_or_else(|| panic!("missing field {k}"));
        assert!(pos("\"identity_key\"") < pos("\"x25519_identity\""));
        assert!(pos("\"x25519_identity\"") < pos("\"signed_prekey\""));
        assert!(pos("\"signed_prekey\"") < pos("\"one_time_prekeys\""));

        // Field set and types.
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let obj = value.as_object().expect("bundle serialises as an object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "identity_key",
                "one_time_prekeys",
                "signed_prekey",
                "x25519_identity"
            ]
        );

        // Both identity fields stay 32-byte values hex-encoded to 64 chars.
        assert_eq!(obj["identity_key"].as_str().unwrap().len(), 64);
        assert_eq!(obj["x25519_identity"].as_str().unwrap().len(), 64);

        let spk = obj["signed_prekey"].as_object().unwrap();
        let mut spk_keys: Vec<&str> = spk.keys().map(String::as_str).collect();
        spk_keys.sort_unstable();
        assert_eq!(spk_keys, vec!["id", "public", "signature"]);
        assert!(spk["id"].is_u64(), "prekey id must remain an unsigned int");
        assert!(
            u32::try_from(spk["id"].as_u64().unwrap()).is_ok(),
            "prekey id must still fit u32"
        );
        assert_eq!(spk["public"].as_str().unwrap().len(), 64);

        let otks = obj["one_time_prekeys"].as_array().unwrap();
        assert_eq!(otks.len(), 2);
        let mut otk_keys: Vec<&str> = otks[0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        otk_keys.sort_unstable();
        assert_eq!(otk_keys, vec!["id", "public"]);
        assert!(otks[0]["id"].is_u64());
        assert!(u32::try_from(otks[0]["id"].as_u64().unwrap()).is_ok());
        assert_eq!(otks[0]["public"].as_str().unwrap().len(), 64);
    }
}
