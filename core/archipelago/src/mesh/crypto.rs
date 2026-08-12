// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Mesh message encryption: X25519 ECDH key agreement + ChaCha20-Poly1305.
//!
//! Reuses Archipelago's existing Ed25519 identity infrastructure.
//! Ed25519 keys are converted to X25519 for Diffie-Hellman key exchange,
//! then ChaCha20-Poly1305 encrypts each message with a unique random nonce.

use anyhow::Result;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use rand::RngCore;

/// Nonce size for ChaCha20-Poly1305.
const NONCE_SIZE: usize = 12;

/// Auth tag size for ChaCha20-Poly1305.
const TAG_SIZE: usize = 16;

/// Minimum ciphertext size: nonce + at least 1 byte + tag.
const MIN_CIPHERTEXT_SIZE: usize = NONCE_SIZE + 1 + TAG_SIZE;

/// Convert an Ed25519 public key (32 bytes) to an X25519 public key (32 bytes).
/// Uses the standard Edwards-to-Montgomery conversion.
pub fn ed25519_pubkey_to_x25519(ed_pubkey: &[u8; 32]) -> Result<[u8; 32]> {
    let compressed = curve25519_dalek::edwards::CompressedEdwardsY(*ed_pubkey);
    let point = compressed
        .decompress()
        .ok_or_else(|| anyhow::anyhow!("Invalid Ed25519 public key: decompression failed"))?;
    let montgomery = point.to_montgomery();
    Ok(*montgomery.as_bytes())
}

/// Convert an Ed25519 signing key to an X25519 secret key.
/// Applies SHA-512 clamping as per RFC 7748.
pub fn ed25519_secret_to_x25519(signing_key: &ed25519_dalek::SigningKey) -> [u8; 32] {
    // The X25519 secret is derived from the first 32 bytes of SHA-512(ed25519_secret)
    // with clamping applied. ed25519-dalek's to_scalar() handles this.
    let hash = <sha2::Sha512 as sha2::Digest>::digest(signing_key.to_bytes());
    let mut x25519_secret = [0u8; 32];
    x25519_secret.copy_from_slice(&hash[..32]);
    // Clamp per RFC 7748
    x25519_secret[0] &= 248;
    x25519_secret[31] &= 127;
    x25519_secret[31] |= 64;
    x25519_secret
}

/// Perform X25519 Diffie-Hellman key agreement.
/// Returns a 32-byte shared secret.
pub fn x25519_shared_secret(our_secret: &[u8; 32], their_public: &[u8; 32]) -> [u8; 32] {
    use curve25519_dalek::montgomery::MontgomeryPoint;
    use curve25519_dalek::scalar::Scalar;

    let their_point = MontgomeryPoint(*their_public);
    let our_scalar = Scalar::from_bytes_mod_order(*our_secret);
    let shared = their_point * our_scalar;
    *shared.as_bytes()
}

/// Encrypt plaintext with ChaCha20-Poly1305 using a shared secret.
/// Output format: [nonce (12 bytes)] + [ciphertext + tag (16 bytes)]
///
/// Each call generates a fresh random 12-byte nonce via OsRng (CSPRNG).
pub fn encrypt(shared_secret: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new_from_slice(shared_secret)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    let mut output = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt ciphertext produced by `encrypt()`.
/// Input format: [nonce (12 bytes)] + [ciphertext + tag (16 bytes)]
pub fn decrypt(shared_secret: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < MIN_CIPHERTEXT_SIZE {
        anyhow::bail!(
            "Ciphertext too short: {} bytes (minimum {})",
            data.len(),
            MIN_CIPHERTEXT_SIZE
        );
    }

    let nonce = Nonce::from_slice(&data[..NONCE_SIZE]);
    let ciphertext = &data[NONCE_SIZE..];

    let cipher = ChaCha20Poly1305::new_from_slice(shared_secret)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("Decryption failed: invalid key or corrupted message"))
}

/// Maximum plaintext bytes that fit in a single encrypted LoRa message.
/// 160 (max LoRa payload) - 12 (nonce) - 16 (tag) = 132 bytes.
pub const MAX_ENCRYPTED_PLAINTEXT: usize = 160 - NONCE_SIZE - TAG_SIZE;

// ─── Phase 3: HKDF + Ephemeral Key Generation ─────────────────────────

/// HKDF-SHA256 key derivation.
/// Derives `okm_len` bytes from input key material with optional salt and info.
pub fn hkdf_sha256(salt: &[u8], ikm: &[u8], info: &[u8], okm_len: usize) -> Result<Vec<u8>> {
    use hkdf::Hkdf;
    use sha2::Sha256;

    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut okm = vec![0u8; okm_len];
    hk.expand(info, &mut okm)
        .map_err(|_| anyhow::anyhow!("HKDF expand failed (output too long)"))?;
    Ok(okm)
}

/// HKDF-SHA256 that returns exactly 32 bytes (one key).
pub fn hkdf_sha256_32(salt: &[u8], ikm: &[u8], info: &[u8]) -> Result<[u8; 32]> {
    let okm = hkdf_sha256(salt, ikm, info, 32)?;
    let mut key = [0u8; 32];
    key.copy_from_slice(&okm);
    Ok(key)
}

/// HKDF-SHA256 that returns exactly 64 bytes (two keys).
/// Used for Double Ratchet root key + chain key derivation.
pub fn hkdf_sha256_64(salt: &[u8], ikm: &[u8], info: &[u8]) -> Result<([u8; 32], [u8; 32])> {
    let okm = hkdf_sha256(salt, ikm, info, 64)?;
    let mut k1 = [0u8; 32];
    let mut k2 = [0u8; 32];
    k1.copy_from_slice(&okm[..32]);
    k2.copy_from_slice(&okm[32..]);
    Ok((k1, k2))
}

/// Generate an ephemeral X25519 keypair for DH ratchet steps.
/// Returns (secret, public) where both are 32 bytes.
pub fn generate_x25519_ephemeral() -> ([u8; 32], [u8; 32]) {
    let mut secret = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut secret);
    // Clamp per RFC 7748
    secret[0] &= 248;
    secret[31] &= 127;
    secret[31] |= 64;

    // Derive public key: secret * basepoint
    use curve25519_dalek::montgomery::MontgomeryPoint;
    use curve25519_dalek::scalar::Scalar;
    let scalar = Scalar::from_bytes_mod_order(secret);
    let public = MontgomeryPoint::mul_base(&scalar);
    (secret, *public.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let shared_secret = [42u8; 32];
        let plaintext = b"hello from mesh";

        let ciphertext = encrypt(&shared_secret, plaintext).unwrap();
        assert!(ciphertext.len() > plaintext.len()); // nonce + tag overhead

        let decrypted = decrypt(&shared_secret, &ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_wrong_key() {
        let secret1 = [1u8; 32];
        let secret2 = [2u8; 32];

        let ciphertext = encrypt(&secret1, b"secret").unwrap();
        assert!(decrypt(&secret2, &ciphertext).is_err());
    }

    #[test]
    fn test_decrypt_corrupted() {
        let secret = [42u8; 32];
        let mut ciphertext = encrypt(&secret, b"test").unwrap();
        // Flip a byte in the ciphertext (after nonce)
        let idx = NONCE_SIZE + 1;
        ciphertext[idx] ^= 0xFF;
        assert!(decrypt(&secret, &ciphertext).is_err());
    }

    #[test]
    fn test_decrypt_too_short() {
        let secret = [42u8; 32];
        assert!(decrypt(&secret, &[0u8; 10]).is_err());
    }

    #[test]
    fn test_unique_nonces() {
        let secret = [42u8; 32];
        let ct1 = encrypt(&secret, b"same").unwrap();
        let ct2 = encrypt(&secret, b"same").unwrap();
        // Nonces (first 12 bytes) should differ
        assert_ne!(&ct1[..NONCE_SIZE], &ct2[..NONCE_SIZE]);
    }

    #[test]
    fn test_ed25519_to_x25519_pubkey() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let ed_pubkey = signing_key.verifying_key().to_bytes();
        let x25519 = ed25519_pubkey_to_x25519(&ed_pubkey).unwrap();
        // Should produce 32 non-zero bytes
        assert_eq!(x25519.len(), 32);
        assert!(x25519.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_x25519_key_agreement() {
        // Generate two Ed25519 keypairs
        let alice_signing = SigningKey::generate(&mut OsRng);
        let bob_signing = SigningKey::generate(&mut OsRng);

        // Convert to X25519
        let alice_secret = ed25519_secret_to_x25519(&alice_signing);
        let bob_secret = ed25519_secret_to_x25519(&bob_signing);
        let alice_public =
            ed25519_pubkey_to_x25519(&alice_signing.verifying_key().to_bytes()).unwrap();
        let bob_public = ed25519_pubkey_to_x25519(&bob_signing.verifying_key().to_bytes()).unwrap();

        // Both sides should derive the same shared secret
        let shared_ab = x25519_shared_secret(&alice_secret, &bob_public);
        let shared_ba = x25519_shared_secret(&bob_secret, &alice_public);
        assert_eq!(shared_ab, shared_ba);
    }

    #[test]
    fn test_full_encrypt_decrypt_with_key_agreement() {
        let alice_signing = SigningKey::generate(&mut OsRng);
        let bob_signing = SigningKey::generate(&mut OsRng);

        let alice_secret = ed25519_secret_to_x25519(&alice_signing);
        let bob_secret = ed25519_secret_to_x25519(&bob_signing);
        let alice_public =
            ed25519_pubkey_to_x25519(&alice_signing.verifying_key().to_bytes()).unwrap();
        let bob_public = ed25519_pubkey_to_x25519(&bob_signing.verifying_key().to_bytes()).unwrap();

        let shared = x25519_shared_secret(&alice_secret, &bob_public);

        // Alice encrypts
        let plaintext = b"sats over mesh";
        let ciphertext = encrypt(&shared, plaintext).unwrap();

        // Bob decrypts with same shared secret
        let bob_shared = x25519_shared_secret(&bob_secret, &alice_public);
        let decrypted = decrypt(&bob_shared, &ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_max_encrypted_plaintext_fits() {
        let secret = [42u8; 32];
        let plaintext = vec![0xAB; MAX_ENCRYPTED_PLAINTEXT];
        let ciphertext = encrypt(&secret, &plaintext).unwrap();
        // Should fit within LoRa max message size (160 bytes)
        assert!(ciphertext.len() <= 160);
    }
}
