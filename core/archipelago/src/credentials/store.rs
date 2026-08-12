use anyhow::{Context, Result};
use std::path::Path;
use tokio::fs;

use super::types::{CredentialStore, CREDENTIALS_DIR};

async fn ensure_dir(data_dir: &Path) -> Result<()> {
    let dir = data_dir.join(CREDENTIALS_DIR);
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .await
            .context("Creating credentials dir")?;
    }
    Ok(())
}

fn store_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(CREDENTIALS_DIR).join("credentials.json")
}

/// Magic prefix marking an encrypted credential store written by this version
/// onward.
///
/// Historically the on-disk format was detected by sniffing the first byte for
/// `[`/`{`. Encrypted blobs begin with a *random* 12-byte nonce, so roughly 2 in
/// 256 (~1 in 128) saves produced a valid encrypted file whose first byte was
/// `[` (0x5B) or `{` (0x7B); those files were misread as plaintext JSON and
/// failed to load forever after. A fixed multi-byte marker cannot collide with a
/// random nonce, so detection is now exact rather than probabilistic.
const ENCRYPTED_MAGIC: &[u8] = b"ARCHYCRED1";

pub async fn load_credentials(data_dir: &Path) -> Result<CredentialStore> {
    ensure_dir(data_dir).await?;
    let path = store_path(data_dir);
    if !path.exists() {
        return Ok(CredentialStore::default());
    }
    let raw = fs::read(&path).await.context("Reading credentials")?;
    decode_credentials(data_dir, &raw).await
}

/// Decode any of the three on-disk credential formats that exist in the fleet.
///
/// Detection order, and why each step is unambiguous:
///
/// 1. **Current format** — `MAGIC ‖ nonce ‖ ciphertext`. The magic is a fixed
///    10-byte literal, so this test has no false positives and no false
///    negatives.
/// 2. **Legacy encrypted** (no magic) — `nonce ‖ ciphertext`. Detected by
///    *successful AEAD decryption*, not by byte shape. ChaCha20-Poly1305 is
///    authenticated: a successful `decrypt` means the 16-byte Poly1305 tag
///    verified under the node key, which a non-ciphertext file passes only with
///    probability ~2^-128. This is a cryptographic discriminator, strictly
///    stronger than any structural sniff.
/// 3. **Legacy plaintext JSON** (original migration path) — reached only when
///    the bytes did not authenticate, then parsed strictly as a whole document.
///
/// If none match we return an error rather than a default store, so a
/// transiently unreadable file is never silently replaced with empty
/// credentials on the next save (CLAUDE.md: migrations never destroy data).
async fn decode_credentials(data_dir: &Path, raw: &[u8]) -> Result<CredentialStore> {
    // 1. Current format: explicit marker.
    if let Some(body) = raw.strip_prefix(ENCRYPTED_MAGIC) {
        let key = load_encryption_key(data_dir).await?;
        let plaintext = decrypt_credentials(body, &key)?;
        return serde_json::from_slice(&plaintext).context("Parsing decrypted credentials");
    }

    // 2. Legacy encrypted, unmarked. The node key may legitimately be absent on
    //    a node that only ever wrote plaintext, so a key-load failure falls
    //    through to the plaintext path instead of aborting.
    if let Ok(key) = load_encryption_key(data_dir).await {
        if let Ok(plaintext) = decrypt_credentials(raw, &key) {
            return serde_json::from_slice(&plaintext)
                .context("Parsing decrypted credentials (legacy unmarked)");
        }
    }

    // 3. Legacy plaintext JSON migration path.
    serde_json::from_slice(raw).context(
        "Credentials file is not magic-prefixed encrypted data, does not authenticate \
         as a legacy encrypted blob, and is not valid plaintext JSON — refusing to \
         treat it as empty",
    )
}

pub async fn save_credentials(data_dir: &Path, store: &CredentialStore) -> Result<()> {
    ensure_dir(data_dir).await?;
    let path = store_path(data_dir);
    let data = serde_json::to_vec(store)?;
    // Encrypt using node key. Always written in the current, magic-prefixed
    // format — this is how legacy files are opportunistically upgraded: they are
    // read in whatever format they are on disk, and the next save re-emits them
    // marked. Nothing is ever rewritten from a read path.
    let key = load_encryption_key(data_dir).await?;
    let encrypted = encrypt_credentials(&data, &key)?;
    let mut output = Vec::with_capacity(ENCRYPTED_MAGIC.len() + encrypted.len());
    output.extend_from_slice(ENCRYPTED_MAGIC);
    output.extend_from_slice(&encrypted);
    fs::write(&path, output)
        .await
        .context("Writing credentials")
}

/// Derive a 32-byte encryption key from the node's identity key via SHA-256.
async fn load_encryption_key(data_dir: &Path) -> Result<[u8; 32]> {
    let node_key_path = data_dir.join("identity").join("node_key");
    let key_bytes = fs::read(&node_key_path)
        .await
        .context("Reading node key for credential encryption")?;
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"archipelago-credential-store-v1");
    hasher.update(&key_bytes);
    let hash = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&hash);
    Ok(key)
}

fn encrypt_credentials(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    // KEY-05: the nonce names `OsRng` and is inspected before use. Nonce reuse
    // under ChaCha20-Poly1305 recovers the keystream and forges the Poly1305 tag,
    // so this draw is guarded even though it is exactly at `MIN_GUARDED_LEN`.
    // The deterministic-nonce seam below is untouched — only the *source* of the
    // random nonce changed.
    let mut nonce_bytes = [0u8; 12];
    crate::entropy::draw_key_bytes(&mut rand::rngs::OsRng, &mut nonce_bytes)
        .map_err(|e| anyhow::anyhow!("Refusing to encrypt with degenerate nonce entropy: {}", e))?;
    encrypt_credentials_with_nonce(data, key, nonce_bytes)
}

/// Encrypt with a caller-supplied nonce, returning `nonce ‖ ciphertext` (no
/// magic prefix — `save_credentials` adds that).
///
/// Split out from [`encrypt_credentials`] so tests can construct a blob whose
/// first byte is a specific value and exercise format detection deterministically
/// instead of waiting on a 1-in-128 random draw.
fn encrypt_credentials_with_nonce(
    data: &[u8],
    key: &[u8; 32],
    nonce_bytes: [u8; 12],
) -> Result<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("Cipher init: {}", e))?;
    let ciphertext = cipher
        .encrypt(
            chacha20poly1305::aead::generic_array::GenericArray::from_slice(&nonce_bytes),
            data,
        )
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;
    let mut output = Vec::with_capacity(12 + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

fn decrypt_credentials(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    if data.len() < 12 {
        anyhow::bail!("Encrypted credentials too short");
    }
    let nonce = &data[..12];
    let ciphertext = &data[12..];
    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("Cipher init: {}", e))?;
    cipher
        .decrypt(
            chacha20poly1305::aead::generic_array::GenericArray::from_slice(nonce),
            ciphertext,
        )
        .map_err(|_| anyhow::anyhow!("Credential decryption failed — key mismatch or corruption"))
}

#[cfg(test)]
mod tests {
    use super::super::types::{CredentialProof, CredentialSubject, VerifiableCredential};
    use super::*;

    /// Tempdir with a deterministic `identity/node_key` so the encryption key
    /// can be derived. Never a real key.
    fn test_dir_with_node_key() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let identity_dir = dir.path().join("identity");
        std::fs::create_dir_all(&identity_dir).unwrap();
        std::fs::write(identity_dir.join("node_key"), [0xAB; 32]).unwrap();
        std::fs::create_dir_all(dir.path().join(CREDENTIALS_DIR)).unwrap();
        dir
    }

    fn sample_store(marker: &str) -> CredentialStore {
        CredentialStore {
            credentials: vec![VerifiableCredential {
                context: vec!["https://www.w3.org/ns/credentials/v2".to_string()],
                id: format!("urn:uuid:{marker}"),
                credential_type: vec![
                    "VerifiableCredential".to_string(),
                    "NodeOperator".to_string(),
                ],
                issuer: "did:key:issuer".to_string(),
                credential_subject: CredentialSubject {
                    id: "did:key:subject".to_string(),
                    claims: serde_json::json!({"role": "admin"}),
                },
                issuance_date: "2026-01-01T00:00:00Z".to_string(),
                expiration_date: None,
                proof: CredentialProof {
                    proof_type: "Ed25519Signature2020".to_string(),
                    created: "2026-01-01T00:00:00Z".to_string(),
                    verification_method: "did:key:issuer#key-1".to_string(),
                    proof_purpose: "assertionMethod".to_string(),
                    proof_value: "sig".to_string(),
                },
                credential_status: None,
            }],
        }
    }

    #[tokio::test]
    async fn test_load_credentials_returns_empty_when_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = load_credentials(dir.path()).await.unwrap();
        assert!(store.credentials.is_empty());
        assert!(dir.path().join(CREDENTIALS_DIR).exists());
    }

    /// Regression: an encrypted blob whose random nonce happens to start with
    /// `[` (0x5B) or `{` (0x7B) used to be misdetected as plaintext JSON, so
    /// `String::from_utf8` failed and the store became permanently unreadable.
    /// ~1 in 128 saves hit this. Both colliding bytes are exercised here
    /// deterministically via an explicit nonce.
    #[tokio::test]
    async fn test_legacy_encrypted_blob_with_json_first_byte_still_loads() {
        for (first_byte, label) in [(0x5Bu8, "open-bracket"), (0x7Bu8, "open-brace")] {
            let dir = test_dir_with_node_key();
            let key = load_encryption_key(dir.path()).await.unwrap();

            // Force the collision: nonce[0] is exactly the byte the old sniffer
            // treated as "this file is plaintext JSON".
            let mut nonce = [0u8; 12];
            nonce[0] = first_byte;
            let plaintext = serde_json::to_vec(&sample_store(label)).unwrap();
            let blob = encrypt_credentials_with_nonce(&plaintext, &key, nonce).unwrap();
            assert_eq!(
                blob[0], first_byte,
                "test must actually trigger the collision"
            );

            // Written WITHOUT magic: this is the legacy on-disk population.
            std::fs::write(store_path(dir.path()), &blob).unwrap();

            let loaded = load_credentials(dir.path())
                .await
                .unwrap_or_else(|e| panic!("{label}: colliding blob failed to load: {e:#}"));
            assert_eq!(loaded.credentials.len(), 1, "{label}");
            assert_eq!(loaded.credentials[0].id, format!("urn:uuid:{label}"));
        }
    }

    /// Same collision, but in the current magic-prefixed format.
    #[tokio::test]
    async fn test_new_format_with_colliding_nonce_loads() {
        for first_byte in [0x5Bu8, 0x7Bu8] {
            let dir = test_dir_with_node_key();
            let key = load_encryption_key(dir.path()).await.unwrap();
            let mut nonce = [0u8; 12];
            nonce[0] = first_byte;
            let plaintext = serde_json::to_vec(&sample_store("magic")).unwrap();
            let body = encrypt_credentials_with_nonce(&plaintext, &key, nonce).unwrap();
            let mut blob = ENCRYPTED_MAGIC.to_vec();
            blob.extend_from_slice(&body);
            std::fs::write(store_path(dir.path()), &blob).unwrap();

            let loaded = load_credentials(dir.path()).await.unwrap();
            assert_eq!(loaded.credentials[0].id, "urn:uuid:magic");
        }
    }

    /// Population 1: legacy plaintext JSON from the original migration path.
    #[tokio::test]
    async fn test_legacy_plaintext_json_still_loads() {
        let dir = test_dir_with_node_key();
        let json = serde_json::to_vec(&sample_store("plaintext")).unwrap();
        assert_eq!(json[0], b'{');
        std::fs::write(store_path(dir.path()), &json).unwrap();

        let loaded = load_credentials(dir.path()).await.unwrap();
        assert_eq!(loaded.credentials[0].id, "urn:uuid:plaintext");
    }

    /// Legacy plaintext must still load on a node that has no node key at all
    /// (pre-onboarding), where the encrypted path cannot even derive a key.
    #[tokio::test]
    async fn test_legacy_plaintext_json_loads_without_node_key() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(CREDENTIALS_DIR)).unwrap();
        let json = serde_json::to_vec(&sample_store("nokey")).unwrap();
        std::fs::write(store_path(dir.path()), &json).unwrap();

        let loaded = load_credentials(dir.path()).await.unwrap();
        assert_eq!(loaded.credentials[0].id, "urn:uuid:nokey");
    }

    /// Population 2: legacy encrypted with an ordinary (non-colliding) nonce.
    #[tokio::test]
    async fn test_legacy_encrypted_without_magic_still_loads() {
        let dir = test_dir_with_node_key();
        let key = load_encryption_key(dir.path()).await.unwrap();
        let plaintext = serde_json::to_vec(&sample_store("legacy-enc")).unwrap();
        let blob = encrypt_credentials_with_nonce(&plaintext, &key, [0x01; 12]).unwrap();
        std::fs::write(store_path(dir.path()), &blob).unwrap();

        let loaded = load_credentials(dir.path()).await.unwrap();
        assert_eq!(loaded.credentials[0].id, "urn:uuid:legacy-enc");
    }

    /// Population 3: current format round-trips and is actually marked on disk.
    #[tokio::test]
    async fn test_new_format_roundtrip_and_is_magic_prefixed() {
        let dir = test_dir_with_node_key();
        save_credentials(dir.path(), &sample_store("current"))
            .await
            .unwrap();

        let on_disk = std::fs::read(store_path(dir.path())).unwrap();
        assert!(
            on_disk.starts_with(ENCRYPTED_MAGIC),
            "save must mark the file"
        );
        // Still genuinely encrypted, not plaintext.
        assert!(!on_disk.windows(4).any(|w| w == b"did:"));

        let loaded = load_credentials(dir.path()).await.unwrap();
        assert_eq!(loaded.credentials[0].id, "urn:uuid:current");
    }

    /// Opportunistic upgrade happens on write, never on read: reading a legacy
    /// file must leave it byte-identical; the next save re-emits it marked.
    #[tokio::test]
    async fn test_legacy_file_upgraded_on_write_not_on_read() {
        let dir = test_dir_with_node_key();
        let json = serde_json::to_vec(&sample_store("upgrade")).unwrap();
        std::fs::write(store_path(dir.path()), &json).unwrap();

        let loaded = load_credentials(dir.path()).await.unwrap();
        // Read path must not have rewritten anything.
        let after_read = std::fs::read(store_path(dir.path())).unwrap();
        assert_eq!(after_read, json, "read path must not rewrite the file");

        save_credentials(dir.path(), &loaded).await.unwrap();
        let after_write = std::fs::read(store_path(dir.path())).unwrap();
        assert!(after_write.starts_with(ENCRYPTED_MAGIC));

        let reloaded = load_credentials(dir.path()).await.unwrap();
        assert_eq!(reloaded.credentials[0].id, "urn:uuid:upgrade");
    }

    /// An unreadable file must surface an error, never a silent empty store —
    /// otherwise the next save would overwrite recoverable user data.
    #[tokio::test]
    async fn test_undecodable_file_errors_instead_of_returning_empty() {
        let dir = test_dir_with_node_key();
        std::fs::write(store_path(dir.path()), b"\x00\x01\x02 not json, not ours").unwrap();
        assert!(load_credentials(dir.path()).await.is_err());
    }

    /// KEY-05 regression: a credential blob written before the entropy migration
    /// must still open after it.
    ///
    /// **Hardcoded on purpose.** Every other test in this module seals and opens
    /// in the same process, which passes even if the envelope layout changed,
    /// because both halves changed together. This one pins the on-disk format
    /// `MAGIC ‖ nonce ‖ ciphertext ‖ tag` against bytes this crate did not
    /// produce: they come from an independent RFC 8439 ChaCha20-Poly1305
    /// implementation, validated first against the RFC's own §2.8.2 vector.
    ///
    /// Key derivation pinned too: `SHA-256("archipelago-credential-store-v1" ‖
    /// [0xAB; 32])`, i.e. the key `test_dir_with_node_key` produces. A change to
    /// the domain separator or the derivation would fail this test, which is the
    /// point — that would strand every credential store in the fleet.
    #[tokio::test]
    async fn opens_pre_migration_ciphertext_vector() {
        const VECTOR: [u8; 56] = [
            0x41, 0x52, 0x43, 0x48, 0x59, 0x43, 0x52, 0x45, 0x44, 0x31, 0x07, 0x07, 0x07, 0x07,
            0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x75, 0x51, 0xf7, 0x32, 0x46, 0x8c,
            0xeb, 0x45, 0x1d, 0xe4, 0x20, 0x8f, 0x02, 0xaf, 0x56, 0xfe, 0x70, 0x8d, 0xc8, 0xf8,
            0x7e, 0xf2, 0xdb, 0xa2, 0x53, 0x23, 0xdb, 0x20, 0xfe, 0x15, 0x5f, 0x8e, 0x48, 0x95,
        ];
        let dir = test_dir_with_node_key();
        std::fs::write(store_path(dir.path()), VECTOR).unwrap();

        let loaded = load_credentials(dir.path())
            .await
            .expect("pre-migration credential blob must still decrypt");
        assert!(loaded.credentials.is_empty());

        // And the vector really is the marked format, not something that fell
        // through to the plaintext path.
        assert!(VECTOR.starts_with(ENCRYPTED_MAGIC));
    }

    /// A magic-prefixed file that fails authentication (tampered / wrong key)
    /// must error rather than fall through to another format.
    #[tokio::test]
    async fn test_tampered_magic_file_errors() {
        let dir = test_dir_with_node_key();
        save_credentials(dir.path(), &sample_store("tamper"))
            .await
            .unwrap();
        let mut blob = std::fs::read(store_path(dir.path())).unwrap();
        *blob.last_mut().unwrap() ^= 0x01;
        std::fs::write(store_path(dir.path()), &blob).unwrap();

        assert!(load_credentials(dir.path()).await.is_err());
    }
}
