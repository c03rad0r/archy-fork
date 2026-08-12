//! Blind Diffie-Hellman Key Exchange (BDHKE) for Cashu ecash.
//!
//! Implements NUT-00 cryptographic operations:
//! - hash_to_curve: deterministic point derivation from secret
//! - blind: create blinded message for mint signing
//! - unblind: remove blinding factor from mint signature
//! - verify: verify unblinded signature against mint pubkey

use anyhow::{Context, Result};
use bitcoin::secp256k1::{PublicKey, Scalar, Secp256k1, SecretKey};
use sha2::{Digest, Sha256};

/// Domain separator for hash_to_curve per NUT-00 spec.
const DOMAIN_SEPARATOR: &[u8] = b"Secp256k1_HashToCurve_Cashu_";

/// Hash a message to a secp256k1 curve point (NUT-00).
///
/// Iteratively hashes `sha256(sha256(domain_separator || msg) || counter)` until
/// the result is a valid x-coordinate on secp256k1. Prepends 0x02 to try as
/// a compressed public key.
pub fn hash_to_curve(message: &[u8]) -> Result<PublicKey> {
    let msg_hash = {
        let mut hasher = Sha256::new();
        hasher.update(DOMAIN_SEPARATOR);
        hasher.update(message);
        hasher.finalize()
    };

    for counter in 0u32..65536 {
        let mut hasher = Sha256::new();
        hasher.update(msg_hash);
        hasher.update(counter.to_le_bytes());
        let hash = hasher.finalize();

        // Try to construct a point: 0x02 || hash (compressed even-y format)
        let mut point_bytes = [0u8; 33];
        point_bytes[0] = 0x02;
        point_bytes[1..].copy_from_slice(&hash);

        if let Ok(pk) = PublicKey::from_slice(&point_bytes) {
            return Ok(pk);
        }
    }

    Err(anyhow::anyhow!(
        "hash_to_curve: no valid point found after 65536 iterations"
    ))
}

/// Blinded message output from the client.
pub struct BlindedMessage {
    /// The blinded point B_ = Y + r*G
    pub b_prime: PublicKey,
    /// The blinding factor (kept secret by client)
    pub r: SecretKey,
    /// The original secret
    pub secret: Vec<u8>,
}

/// Create a blinded message for the mint to sign.
///
/// Given a secret, computes Y = hash_to_curve(secret), picks random r,
/// and returns B_ = Y + r*G along with the blinding factor r.
pub fn blind_message(secret: &[u8], blinding_factor: &SecretKey) -> Result<BlindedMessage> {
    let secp = Secp256k1::new();

    // Y = hash_to_curve(secret)
    let y = hash_to_curve(secret)?;

    // r*G
    let r_pub = PublicKey::from_secret_key(&secp, blinding_factor);

    // B_ = Y + r*G
    let b_prime = PublicKey::combine_keys(&[&y, &r_pub])
        .context("Failed to compute blinded message B_ = Y + r*G")?;

    Ok(BlindedMessage {
        b_prime,
        r: *blinding_factor,
        secret: secret.to_vec(),
    })
}

/// Unblind a mint's blind signature to get the real signature.
///
/// Given C_ (blind signature from mint), r (our blinding factor), and K (mint's pubkey):
/// C = C_ - r*K
pub fn unblind_signature(
    c_prime: &PublicKey,
    r: &SecretKey,
    mint_pubkey: &PublicKey,
) -> Result<PublicKey> {
    let secp = Secp256k1::new();

    // Compute r*K
    let r_scalar =
        Scalar::from_be_bytes(r.secret_bytes()).expect("valid secret key is valid scalar");
    let r_times_k = mint_pubkey
        .mul_tweak(&secp, &r_scalar)
        .context("Failed to compute r*K")?;

    // Negate to get -(r*K)
    let neg_r_times_k = r_times_k.negate(&secp);

    // C = C_ + (-(r*K)) = C_ - r*K
    let c = PublicKey::combine_keys(&[c_prime, &neg_r_times_k])
        .context("Failed to compute C = C_ - r*K")?;

    Ok(c)
}

/// Verify that a proof (secret, C) is valid against a mint's public key K.
///
/// Checks: C == k * hash_to_curve(secret) — but since we don't have k (the mint's
/// private key), we verify by checking that the DLEQ proof is valid, or by
/// attempting to swap the token at the mint. This function provides a basic
/// structural check that the proof components are well-formed.
pub fn verify_proof_structure(secret: &[u8], c: &PublicKey) -> Result<bool> {
    // Verify that hash_to_curve(secret) produces a valid point
    let _y = hash_to_curve(secret)?;
    // Verify C is a valid public key (already guaranteed by type, but check non-identity)
    let c_bytes = c.serialize();
    if c_bytes.iter().all(|&b| b == 0) {
        return Ok(false);
    }
    Ok(true)
}

/// Construct the secret string for a Cashu proof.
/// NUT-10 defines secret as a JSON array: ["P2PK", {nonce, data, tags}]
/// For basic (non-P2PK) proofs, the secret is just a random hex string.
pub fn generate_secret() -> Vec<u8> {
    // KEY-05: genuine ecash key material — the secret behind a Cashu proof.
    // Source named, draw guarded. A degenerate secret is unspendable at best and
    // predictable at worst, so refusing is strictly better than emitting one.
    let mut random_bytes = [0u8; 32];
    crate::entropy::draw_key_bytes(&mut rand::rngs::OsRng, &mut random_bytes).unwrap_or_else(|e| {
        panic!("refusing to mint a Cashu proof secret from degenerate entropy: {e} (KEY-05)")
    });
    hex::encode(random_bytes).into_bytes()
}

/// Generate a random blinding factor.
///
/// KEY-05: the RNG is named (`OsRng`) instead of inherited from `thread_rng()`.
/// The degenerate-entropy guard is **deliberately not applied** here — see
/// `docs/security/KEY-05-ENTROPY-ENFORCEMENT.md` § *Deliberate non-applications
/// of the guard*. `SecretKey::new` performs rejection sampling into the secp256k1
/// group order; intercepting the bytes to inspect them would mean reimplementing
/// that sampling, which is a larger correctness risk than the guard buys.
pub fn random_blinding_factor() -> SecretKey {
    let mut rng = rand::rngs::OsRng;
    SecretKey::new(&mut rng)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_to_curve_deterministic() {
        let msg = b"test_message";
        let p1 = hash_to_curve(msg).unwrap();
        let p2 = hash_to_curve(msg).unwrap();
        assert_eq!(p1, p2);
    }

    #[test]
    fn test_hash_to_curve_different_messages() {
        let p1 = hash_to_curve(b"message_a").unwrap();
        let p2 = hash_to_curve(b"message_b").unwrap();
        assert_ne!(p1, p2);
    }

    #[test]
    fn test_blind_unblind_roundtrip() {
        let secp = Secp256k1::new();
        let secret = b"test_secret";
        let r = random_blinding_factor();

        // Simulate mint: k is mint's private key, K = k*G is public key
        let k = SecretKey::new(&mut rand::rngs::OsRng);
        let k_pub = PublicKey::from_secret_key(&secp, &k);

        // Client blinds
        let blinded = blind_message(secret, &r).unwrap();

        // Mint signs: C_ = k * B_
        let k_scalar = Scalar::from_be_bytes(k.secret_bytes()).unwrap();
        let c_prime = blinded.b_prime.mul_tweak(&secp, &k_scalar).unwrap();

        // Client unblinds: C = C_ - r*K
        let c = unblind_signature(&c_prime, &r, &k_pub).unwrap();

        // Verify: C should equal k * hash_to_curve(secret)
        let y = hash_to_curve(secret).unwrap();
        let expected_c = y.mul_tweak(&secp, &k_scalar).unwrap();
        assert_eq!(c, expected_c);
    }

    #[test]
    fn test_generate_secret_length() {
        let secret = generate_secret();
        // 32 bytes hex-encoded = 64 chars
        assert_eq!(secret.len(), 64);
    }

    #[test]
    fn test_generate_secret_unique() {
        let s1 = generate_secret();
        let s2 = generate_secret();
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_verify_proof_structure_valid() {
        let secret = generate_secret();
        let secp = Secp256k1::new();
        let k = SecretKey::new(&mut rand::rngs::OsRng);
        let y = hash_to_curve(&secret).unwrap();
        let k_scalar = Scalar::from_be_bytes(k.secret_bytes()).unwrap();
        let c = y.mul_tweak(&secp, &k_scalar).unwrap();

        assert!(verify_proof_structure(&secret, &c).unwrap());
    }

    /// KEY-05: the blinding factor is the one genuine generic-over-RNG key
    /// generation seam outside `seed.rs`, and it is the site where the degenerate
    /// guard is deliberately NOT applied. Prove the migration to `OsRng` still
    /// yields a usable secp256k1 scalar and that successive calls differ, so a
    /// rebinding to a constant source would fail here rather than silently
    /// producing correlated ecash.
    #[test]
    fn blinding_factor_is_valid_and_varies() {
        let a = random_blinding_factor();
        let b = random_blinding_factor();
        assert_ne!(
            a.secret_bytes(),
            b.secret_bytes(),
            "two blinding factors must not collide"
        );

        // "Valid" means secp256k1 itself accepts it: a round trip through
        // `from_slice` is the library's own range check against the group order.
        let reparsed = SecretKey::from_slice(&a.secret_bytes())
            .expect("blinding factor must be a valid secp256k1 scalar");
        assert_eq!(reparsed.secret_bytes(), a.secret_bytes());

        // And it is usable as a scalar in the BDHKE operation it exists for.
        let secp = Secp256k1::new();
        let y = hash_to_curve(b"key-05 blinding factor probe").unwrap();
        let scalar = Scalar::from_be_bytes(a.secret_bytes()).unwrap();
        assert!(y.mul_tweak(&secp, &scalar).is_ok());
    }
}
