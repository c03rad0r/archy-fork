//! At-rest encryption for local state stores (chat messages, mesh contacts).
//!
//! Best-practice envelope, matching `credentials::store`:
//! - **Key**: SHA-256(domain-separator ‖ node identity key). The node key is
//!   seed-derived and never leaves the device, so each store is bound to this
//!   node's identity — a stolen disk image is unreadable without it, and the
//!   per-domain separator means one store's key can't open another.
//! - **Cipher**: ChaCha20-Poly1305 AEAD with a fresh random 96-bit nonce per
//!   write (`nonce ‖ ciphertext` on disk). The Poly1305 tag makes it
//!   tamper-evident — any on-disk modification fails to open.
//! - **Migration**: legacy plaintext JSON is detected and read transparently,
//!   then re-written encrypted on the next save. No data is stranded.

use anyhow::{Context, Result};
use std::path::Path;

/// Domain separators — one per store so keys never overlap.
pub const DOMAIN_MESSAGES: &[u8] = b"archipelago-message-store-v1";
pub const DOMAIN_MESH_CONTACTS: &[u8] = b"archipelago-mesh-contacts-v1";

/// Derive a 32-byte key bound to this node's identity for a given store domain.
pub async fn derive_key(data_dir: &Path, domain: &[u8]) -> Result<[u8; 32]> {
    let node_key_path = data_dir.join("identity").join("node_key");
    let key_bytes = tokio::fs::read(&node_key_path)
        .await
        .context("reading node key for at-rest encryption")?;
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(&key_bytes);
    let mut key = [0u8; 32];
    key.copy_from_slice(&hasher.finalize());
    Ok(key)
}

/// Encrypt `plaintext`, returning `nonce ‖ ciphertext`.
pub fn seal(plaintext: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    // KEY-05: the nonce names `OsRng` and is inspected before use. Nonce reuse
    // under ChaCha20-Poly1305 is a keystream recovery *and* a Poly1305 forgery,
    // so this is the highest-consequence twelve bytes in the module. Only the
    // *source* changed — the envelope below is byte-identical to what every blob
    // already on a fleet node was written with.
    let mut nonce_bytes = [0u8; 12];
    crate::entropy::draw_key_bytes(&mut rand::rngs::OsRng, &mut nonce_bytes)
        .map_err(|e| anyhow::anyhow!("refusing to seal with degenerate nonce entropy: {e}"))?;
    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("cipher init: {e}"))?;
    let ct = cipher
        .encrypt(
            chacha20poly1305::aead::generic_array::GenericArray::from_slice(&nonce_bytes),
            plaintext,
        )
        .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;
    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt `nonce ‖ ciphertext`.
pub fn open(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    if data.len() < 12 {
        anyhow::bail!("ciphertext too short");
    }
    let (nonce, ct) = data.split_at(12);
    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("cipher init: {e}"))?;
    cipher
        .decrypt(
            chacha20poly1305::aead::generic_array::GenericArray::from_slice(nonce),
            ct,
        )
        .map_err(|_| anyhow::anyhow!("decryption failed — key mismatch or corruption"))
}

/// Heuristic: does this look like legacy plaintext JSON (starts with `{`/`[`)?
/// Encrypted blobs start with a random nonce byte, so a `{`/`[` first byte is a
/// reliable migration signal.
pub fn is_plaintext_json(raw: &[u8]) -> bool {
    matches!(raw.first(), Some(b'{') | Some(b'['))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_round_trips() {
        let key = [7u8; 32];
        let msg = br#"{"messages":[{"m":"hi"}]}"#;
        let sealed = seal(msg, &key).unwrap();
        // Encrypted output must NOT be readable plaintext.
        assert!(!is_plaintext_json(&sealed));
        assert_ne!(&sealed[12..], &msg[..]);
        assert_eq!(open(&sealed, &key).unwrap(), msg);
    }

    #[test]
    fn open_fails_on_wrong_key_or_tamper() {
        let sealed = seal(b"secret", &[1u8; 32]).unwrap();
        assert!(open(&sealed, &[2u8; 32]).is_err());
        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 0x01;
        assert!(open(&tampered, &[1u8; 32]).is_err());
    }

    #[test]
    fn detects_plaintext_vs_ciphertext() {
        assert!(is_plaintext_json(b"{\"a\":1}"));
        assert!(is_plaintext_json(b"[]"));
        assert!(!is_plaintext_json(&seal(b"x", &[3u8; 32]).unwrap()));
    }

    /// KEY-05 regression: a blob written by the pre-migration `seal` must still
    /// open after the migration.
    ///
    /// The vector is **hardcoded**, deliberately. A seal-then-open round trip in
    /// the same process passes even if the envelope layout changed, because both
    /// halves changed together — it proves self-consistency, not compatibility.
    /// Every at-rest message store and mesh-contact store on every fleet node was
    /// written by the old code, and CLAUDE.md's "migrations never destroy data"
    /// invariant means this must decrypt.
    ///
    /// The bytes were produced by an **independent** RFC 8439 ChaCha20-Poly1305
    /// implementation (validated first against the RFC's own §2.8.2 test vector),
    /// not captured from this crate — so it pins the documented envelope
    /// `nonce ‖ ciphertext` as a *format*, rather than pinning whatever this
    /// implementation happened to emit.
    ///
    /// key = `[0x42; 32]`, nonce = `[0x07; 12]`, no AAD.
    #[test]
    fn opens_pre_migration_ciphertext_vector() {
        const VECTOR: [u8; 81] = [
            0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0xb1, 0x19,
            0x96, 0xcc, 0xc8, 0x41, 0x2b, 0xf4, 0x25, 0x2a, 0x0f, 0x8b, 0xdc, 0xcb, 0xbb, 0x4d,
            0x90, 0xe9, 0x99, 0xc5, 0xeb, 0x67, 0xd6, 0x5e, 0x1d, 0xcb, 0x61, 0x2c, 0xd1, 0xe4,
            0x91, 0xe1, 0x44, 0x29, 0xaa, 0x0b, 0x1d, 0xfd, 0xf0, 0x88, 0xe4, 0x5a, 0x65, 0x5b,
            0x29, 0x53, 0xc9, 0xf7, 0x85, 0xd1, 0xa1, 0xec, 0xb7, 0xd7, 0xd6, 0x12, 0xf2, 0x88,
            0x77, 0xed, 0x2d, 0x72, 0x90, 0xff, 0x9a, 0x5b, 0x98, 0xea, 0xec,
        ];
        let key = [0x42u8; 32];
        assert_eq!(
            open(&VECTOR, &key).expect("pre-migration blob must still decrypt"),
            b"archipelago storage_crypto pre-KEY-05 envelope vector".to_vec()
        );
    }

    /// The envelope shape itself, pinned so a later refactor cannot reshape it:
    /// 12-byte nonce prefix, 16-byte Poly1305 tag suffix, and a *fresh* nonce per
    /// seal.
    #[test]
    fn seal_envelope_layout_unchanged() {
        let key = [9u8; 32];
        let plaintext = b"twelve plus n plus sixteen";
        let a = seal(plaintext, &key).unwrap();
        let b = seal(plaintext, &key).unwrap();

        assert_eq!(a.len(), 12 + plaintext.len() + 16);
        assert_eq!(b.len(), 12 + plaintext.len() + 16);
        assert_ne!(
            a[..12],
            b[..12],
            "two seals of the same plaintext must not share a nonce"
        );
        assert_eq!(open(&a, &key).unwrap(), plaintext);
        assert_eq!(open(&b, &key).unwrap(), plaintext);
    }
}
