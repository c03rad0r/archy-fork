//! BIP-39 master seed: generation, storage, and deterministic key derivation.
//!
//! One 24-word mnemonic derives ALL Archipelago keys:
//!
//!   BIP-39 Mnemonic (24 words, 256-bit entropy)
//!     → PBKDF2-HMAC-SHA512 (2048 rounds, empty passphrase)
//!     → Master Seed (64 bytes)
//!       ├── HKDF(seed, "archipelago/node/ed25519/v1")             → Node Ed25519 → did:key
//!       ├── HKDF(seed, "archipelago/nostr-node/secp256k1/v1")     → Node Nostr key
//!       ├── HKDF(seed, "archipelago/fips/secp256k1/v1")           → FIPS mesh transport key
//!       ├── HKDF(seed, "archipelago/release/root/ed25519/v1")     → Release-root signing key
//!       │      (publisher-only; nodes pin the PUBLIC key — see trust::anchor)
//!       ├── HKDF(seed, "archipelago/identity/{i}/ed25519/v1")     → Identity i Ed25519
//!       ├── BIP-32 m/44'/1237'/0'/0/{i}                           → Identity i Nostr (NIP-06)
//!       ├── BIP-32 m/84'/0'/0'                                    → Bitcoin Core wallet
//!       └── HKDF(seed, "archipelago/lnd/entropy/v1")              → LND aezeed entropy
//!
//! SECURITY: Never log mnemonic or seed material at any level.

use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

// ─── Constants ──────────────────────────────────────────────────────────

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const SEED_LEN: usize = 64;
const IDENTITY_INDEX_FILE: &str = "identity_index";
const ENCRYPTED_SEED_FILE: &str = "master_seed.enc";
const ENCRYPTED_LND_SEED_FILE: &str = "lnd_aezeed.enc";
const LND_SEED_ACK_FILE: &str = "lnd_aezeed.ack";

// HKDF info strings for domain-separated key derivation.
const NODE_ED25519_INFO: &[u8] = b"archipelago/node/ed25519/v1";
const NODE_NOSTR_INFO: &[u8] = b"archipelago/nostr-node/secp256k1/v1";
const FIPS_KEY_INFO: &[u8] = b"archipelago/fips/secp256k1/v1";
const LND_ENTROPY_INFO: &[u8] = b"archipelago/lnd/entropy/v1";
const RELEASE_ROOT_ED25519_INFO: &[u8] = b"archipelago/release/root/ed25519/v1";
const CASHU_ENTROPY_INFO: &[u8] = b"archipelago/cashu/bip39-entropy/v1";

// ─── MasterSeed ─────────────────────────────────────────────────────────

/// 64-byte master seed derived from a BIP-39 mnemonic.
/// Implements ZeroizeOnDrop to clear memory when dropped.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MasterSeed {
    bytes: [u8; SEED_LEN],
}

/// Probe whether the kernel CSPRNG is fully initialized, without blocking.
///
/// `getrandom(2)` already blocks until the entropy pool is initialized, so a
/// seed can never be drawn from an unseeded pool. This probe exists to make
/// that ordering auditable in the logs: first-boot ISO flows generate the seed
/// early, when a slow-to-seed pool would otherwise be invisible.
#[cfg(target_os = "linux")]
fn kernel_csprng_ready() -> Option<bool> {
    let mut byte = [0u8; 1];
    let ret = unsafe {
        libc::getrandom(
            byte.as_mut_ptr() as *mut libc::c_void,
            byte.len(),
            libc::GRND_NONBLOCK,
        )
    };
    if ret == 1 {
        Some(true)
    } else if std::io::Error::last_os_error().raw_os_error() == Some(libc::EAGAIN) {
        Some(false)
    } else {
        None
    }
}

#[cfg(not(target_os = "linux"))]
fn kernel_csprng_ready() -> Option<bool> {
    None
}

/// Generate a 24-word English BIP-39 mnemonic from an **explicitly supplied** CSPRNG.
///
/// The entropy source is an argument here, never a transitive-dependency default.
/// See `docs/security/ENTROPY-SEED-AUDIT-2026-07-31.md` finding F-02 / `[ARCHY-1]`:
/// a bare `bip39::Mnemonic::generate(24)` resolves through `Mnemonic::generate_in`
/// to `&mut rand::thread_rng()` *inside* the `bip39` crate, so the RNG backing every
/// Archipelago key — including the fleet release-root signing key — would be chosen
/// by a dependency's default rather than stated at this call site.
///
/// That is precisely the structural shape of the 2026-07-30 COLDCARD entropy defect
/// ("T1"), where a refactor silently rebound seed generation to a non-cryptographic
/// PRNG with no compile error and no test failure. Naming the source here means a
/// future `rand` or `bip39` bump cannot rebind it silently, and it creates the seam
/// that `mnemonic_generation_uses_injected_rng` needs to prove the passed RNG is the
/// one actually consumed.
///
/// **KEY-05 (2026-08-02) generalises that in two ways.**
///
/// The bound is no longer `rand::CryptoRng + rand::RngCore`. `CryptoRng` is a
/// marker with no compiler-checked content: any caller could implement it for any
/// type and satisfy this signature while supplying a counter. The bound is now
/// [`crate::entropy::KeyGenRng`], a **sealed** allowlist whose supertrait lives in a
/// private module of `entropy`, so the set of RNGs that can drive the master key
/// hierarchy is exactly the set written in that one file and the compiler enforces
/// it. Documentation became a constraint.
///
/// The entropy is also now **inspectable at this seam**: it is drawn into a local
/// buffer through [`crate::entropy::draw_key_bytes`], which refuses an all-zero,
/// all-identical or wrapping-counter draw before it can become a seed, and the
/// mnemonic is built with `from_entropy` instead of `generate_in_with`. Those two
/// are the same function for the same RNG output — `mnemonic_generation_uses_injected_rng`
/// below has asserted exactly that equivalence since F-02, and still does. The
/// buffer is zeroized before this function returns on every path.
fn generate_mnemonic_with<R: crate::entropy::KeyGenRng>(rng: &mut R) -> Result<bip39::Mnemonic> {
    let mut entropy = [0u8; 32];
    let result = crate::entropy::draw_key_bytes(rng, &mut entropy)
        .map_err(|e| {
            anyhow::anyhow!(
                "Refusing to build a mnemonic from degenerate entropy: {}",
                e
            )
        })
        .and_then(|()| {
            bip39::Mnemonic::from_entropy(&entropy)
                .map_err(|e| anyhow::anyhow!("Failed to generate mnemonic: {}", e))
        });
    entropy.zeroize();
    result
}

impl MasterSeed {
    /// Generate a new 24-word BIP-39 mnemonic and derive the master seed.
    pub fn generate() -> Result<(bip39::Mnemonic, Self)> {
        let ready = kernel_csprng_ready();
        match ready {
            Some(true) => tracing::info!("kernel CSPRNG initialized; generating master seed"),
            Some(false) => tracing::warn!(
                "kernel CSPRNG not yet initialized; getrandom() will block until the pool is seeded"
            ),
            None => {}
        }
        // Until KEY-05 layer (e) this verdict was computed, logged and thrown
        // away, so a node could never answer after the fact whether its keys were
        // born from a seeded pool (backlog R-09). Now it is durable. Best-effort:
        // `ceremony.rs` runs this offline where no data directory need exist, and
        // an audit record must never be able to fail key generation.
        crate::entropy::record_csprng_readiness(ready, "master-seed-generate");
        // OsRng is passed explicitly: a direct getrandom(2) wrapper with no
        // userspace state, chosen here rather than inherited. See
        // `generate_mnemonic_with` for why this is stated and not defaulted.
        let mnemonic = generate_mnemonic_with(&mut rand::rngs::OsRng)?;
        let seed = Self::from_mnemonic(&mnemonic);
        Ok((mnemonic, seed))
    }

    /// Derive master seed from an existing mnemonic (empty BIP-39 passphrase).
    pub fn from_mnemonic(mnemonic: &bip39::Mnemonic) -> Self {
        let seed_bytes = mnemonic.to_seed("");
        let mut bytes = [0u8; SEED_LEN];
        bytes.copy_from_slice(&seed_bytes);
        Self { bytes }
    }

    /// Parse a space-separated word string, validate checksum, and derive seed.
    pub fn from_mnemonic_words(words: &str) -> Result<(bip39::Mnemonic, Self)> {
        let mnemonic: bip39::Mnemonic = words
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid mnemonic: {}", e))?;
        let word_count = mnemonic.word_count();
        if word_count != 24 {
            anyhow::bail!("Expected 24 words, got {}", word_count);
        }
        let seed = Self::from_mnemonic(&mnemonic);
        Ok((mnemonic, seed))
    }

    /// Access raw seed bytes (for HKDF input).
    fn as_bytes(&self) -> &[u8; SEED_LEN] {
        &self.bytes
    }
}

// ─── Ed25519 Derivation (HKDF) ─────────────────────────────────────────

/// Derive the node's persistent Ed25519 signing key.
pub fn derive_node_ed25519(seed: &MasterSeed) -> Result<SigningKey> {
    let derived = hkdf_derive_32(seed.as_bytes(), NODE_ED25519_INFO)?;
    Ok(SigningKey::from_bytes(&derived))
}

/// Derive the fleet **release-root** Ed25519 signing key.
///
/// This is a *publisher-side* derivation: only the holder of the release master
/// seed runs it (e.g. in the signing ceremony). Fleet nodes never derive this —
/// they pin the corresponding PUBLIC key as a trust anchor (see
/// `crate::trust::anchor`) and use it to verify signed manifests/catalogs.
///
/// Keeping it seed-derived means the signing key is reproducible from a
/// backed-up mnemonic (disaster recovery) rather than a loose key file, and it
/// is domain-separated from every node/identity key by its HKDF info string.
pub fn derive_release_root_ed25519(seed: &MasterSeed) -> Result<SigningKey> {
    let derived = hkdf_derive_32(seed.as_bytes(), RELEASE_ROOT_ED25519_INFO)?;
    Ok(SigningKey::from_bytes(&derived))
}

/// Derive an identity's Ed25519 signing key by index.
pub fn derive_identity_ed25519(seed: &MasterSeed, index: u32) -> Result<SigningKey> {
    let info = format!("archipelago/identity/{}/ed25519/v1", index);
    let derived = hkdf_derive_32(seed.as_bytes(), info.as_bytes())?;
    Ok(SigningKey::from_bytes(&derived))
}

// ─── Secp256k1 / Nostr Derivation (BIP-32 + HKDF) ──────────────────────

/// Derive the node-level Nostr secp256k1 key (not per-identity).
pub fn derive_node_nostr_key(seed: &MasterSeed) -> Result<nostr_sdk::Keys> {
    let derived = hkdf_derive_32(seed.as_bytes(), NODE_NOSTR_INFO)?;
    let secret = nostr_sdk::SecretKey::from_slice(&derived)
        .map_err(|e| anyhow::anyhow!("Invalid secp256k1 key from HKDF: {}", e))?;
    Ok(nostr_sdk::Keys::new(secret))
}

/// Derive the FIPS mesh transport secp256k1 key.
/// Distinct from the Nostr-node key so compromise of one surface does not
/// impersonate on the other; still seed-recoverable.
pub fn derive_fips_key(seed: &MasterSeed) -> Result<nostr_sdk::Keys> {
    let derived = hkdf_derive_32(seed.as_bytes(), FIPS_KEY_INFO)?;
    let secret = nostr_sdk::SecretKey::from_slice(&derived)
        .map_err(|e| anyhow::anyhow!("Invalid secp256k1 key from HKDF: {}", e))?;
    Ok(nostr_sdk::Keys::new(secret))
}

/// Derive an identity's Nostr secp256k1 key via BIP-32.
/// Path: m/44'/1237'/0'/0/{index} (NIP-06 compliant).
pub fn derive_nostr_identity_key(seed: &MasterSeed, index: u32) -> Result<nostr_sdk::Keys> {
    use bitcoin::bip32::{ChildNumber, DerivationPath, Xpriv};
    use bitcoin::Network;

    let master = Xpriv::new_master(Network::Bitcoin, seed.as_bytes())
        .context("Failed to derive BIP-32 master key")?;

    let path = DerivationPath::from(vec![
        ChildNumber::from_hardened_idx(44).expect("valid"),
        ChildNumber::from_hardened_idx(1237).expect("valid"),
        ChildNumber::from_hardened_idx(0).expect("valid"),
        ChildNumber::from_normal_idx(0).expect("valid"),
        ChildNumber::from_normal_idx(index).expect("valid index"),
    ]);

    let secp = bitcoin::secp256k1::Secp256k1::new();
    let child = master
        .derive_priv(&secp, &path)
        .context("BIP-32 derivation failed")?;

    let secret_bytes = child.private_key.secret_bytes();
    let secret = nostr_sdk::SecretKey::from_slice(&secret_bytes)
        .map_err(|e| anyhow::anyhow!("Invalid Nostr key from BIP-32: {}", e))?;
    Ok(nostr_sdk::Keys::new(secret))
}

// ─── Bitcoin / LND Derivation ───────────────────────────────────────────

/// Derive the BIP-84 account-level extended private key.
/// Path: m/84'/0'/0' (native segwit, mainnet).
///
/// **Retained deliberately with no production caller (Phase 10, D-07c).** Its only
/// non-test caller was the Bitcoin Core wallet-init RPC handler, deleted under D-07b
/// because it imported this xprv into Bitcoin Core's `wallet.dat` (audit finding
/// F-13; see `docs/security/KEY-03-SIGNING-POSTURE.md`). This function is *not*
/// cruft: it is covered by existing tests below, and
/// it is the derivation D-07c's deferred BDK cold vault (ElectrumX-backed, daemon
/// side) will need. Do not delete it as dead code; if D-07c is abandoned, remove
/// the decision and the function together.
#[allow(dead_code)]
pub fn derive_bitcoin_xprv(seed: &MasterSeed) -> Result<bitcoin::bip32::Xpriv> {
    use bitcoin::bip32::{ChildNumber, DerivationPath, Xpriv};
    use bitcoin::Network;

    let master = Xpriv::new_master(Network::Bitcoin, seed.as_bytes())
        .context("Failed to derive BIP-32 master key")?;

    let path = DerivationPath::from(vec![
        ChildNumber::from_hardened_idx(84).expect("valid"),
        ChildNumber::from_hardened_idx(0).expect("valid"),
        ChildNumber::from_hardened_idx(0).expect("valid"),
    ]);

    let secp = bitcoin::secp256k1::Secp256k1::new();
    master
        .derive_priv(&secp, &path)
        .context("BIP-84 derivation failed")
}

/// Derive 16 bytes of entropy for LND aezeed wallet initialization.
pub fn derive_lnd_entropy(seed: &MasterSeed) -> Result<[u8; 16]> {
    let derived = hkdf_derive(seed.as_bytes(), LND_ENTROPY_INFO, 16)?;
    let mut entropy = [0u8; 16];
    entropy.copy_from_slice(&derived);
    Ok(entropy)
}

/// Derive the ecash (Cashu, NUT-13) wallet's own 24-word BIP-39 mnemonic.
///
/// The ecash wallet gets a **separate mnemonic** rather than being handed the
/// node's own 24 words, and both halves of that matter:
///
/// - it is still covered by the node's recovery phrase, because it is derived
///   from the master seed over a fixed domain-separated path — restore the
///   node from its words and the same ecash wallet comes back, with nothing
///   extra for the operator to write down;
/// - but it is *portable*. NUT-13 is a standard, so these words restore the
///   ecash in Minibits, Nutstash or `cdk-cli`. Showing the node seed here
///   would have made "back up my ecash" and "hand over the key to the entire
///   node" the same action.
///
/// One-way by construction: HKDF cannot be run backwards, so a leaked ecash
/// mnemonic does not expose the master seed or any other derived key.
pub fn derive_cashu_mnemonic(seed: &MasterSeed) -> Result<bip39::Mnemonic> {
    let mut entropy = hkdf_derive_32(seed.as_bytes(), CASHU_ENTROPY_INFO)?;
    let mnemonic = bip39::Mnemonic::from_entropy(&entropy)
        .map_err(|e| anyhow::anyhow!("Failed to derive the ecash mnemonic: {}", e));
    entropy.zeroize();
    mnemonic
}

// ─── Encrypted Seed Storage ─────────────────────────────────────────────

/// Encrypt `plaintext` with Argon2(passphrase) + ChaCha20-Poly1305.
/// Blob format: salt || nonce || ciphertext.
fn encrypt_blob(plaintext: &[u8], passphrase: &str) -> Result<Vec<u8>> {
    use argon2::Argon2;
    use chacha20poly1305::aead::{Aead, KeyInit};
    use rand::RngCore;

    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), &salt, &mut key)
        .map_err(|e| anyhow::anyhow!("Argon2 key derivation failed: {}", e))?;

    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("Cipher init: {}", e))?;
    let ciphertext = cipher
        .encrypt(
            chacha20poly1305::aead::generic_array::GenericArray::from_slice(&nonce),
            plaintext,
        )
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    key.zeroize();

    let mut blob = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(&salt);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

/// Decrypt a salt||nonce||ciphertext blob produced by `encrypt_blob`.
fn decrypt_blob(blob: &[u8], passphrase: &str) -> Result<Vec<u8>> {
    use argon2::Argon2;
    use chacha20poly1305::aead::{Aead, KeyInit};

    if blob.len() < SALT_LEN + NONCE_LEN {
        anyhow::bail!("Encrypted blob too short");
    }

    let salt = &blob[..SALT_LEN];
    let nonce = &blob[SALT_LEN..SALT_LEN + NONCE_LEN];
    let ciphertext = &blob[SALT_LEN + NONCE_LEN..];

    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow::anyhow!("Argon2 key derivation failed: {}", e))?;

    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("Cipher init: {}", e))?;

    key.zeroize();

    cipher
        .decrypt(
            chacha20poly1305::aead::generic_array::GenericArray::from_slice(nonce),
            ciphertext,
        )
        .map_err(|_| anyhow::anyhow!("Decryption failed — wrong passphrase"))
}

/// Write an encrypted blob under `identity/` with 0600 permissions.
async fn write_identity_blob(
    data_dir: &std::path::Path,
    file_name: &str,
    blob: &[u8],
) -> Result<()> {
    let identity_dir = data_dir.join("identity");
    tokio::fs::create_dir_all(&identity_dir)
        .await
        .context("Failed to create identity directory")?;

    let path = identity_dir.join(file_name);
    tokio::fs::write(&path, blob)
        .await
        .with_context(|| format!("Failed to write {file_name}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .await
            .with_context(|| format!("Failed to set {file_name} permissions"))?;
    }

    Ok(())
}

/// Encrypt and save the mnemonic words to disk (convenience backup).
/// Uses Argon2 key derivation + ChaCha20-Poly1305 AEAD.
pub async fn save_seed_encrypted(
    data_dir: &std::path::Path,
    mnemonic: &bip39::Mnemonic,
    passphrase: &str,
) -> Result<()> {
    let blob = encrypt_blob(mnemonic.to_string().as_bytes(), passphrase)?;
    write_identity_blob(data_dir, ENCRYPTED_SEED_FILE, &blob).await
}

/// Load and decrypt the mnemonic from disk.
pub async fn load_seed_encrypted(
    data_dir: &std::path::Path,
    passphrase: &str,
) -> Result<bip39::Mnemonic> {
    let path = data_dir.join("identity").join(ENCRYPTED_SEED_FILE);
    let blob = tokio::fs::read(&path)
        .await
        .context("Failed to read encrypted seed file")?;

    let plaintext = decrypt_blob(&blob, passphrase)?;
    let words = String::from_utf8(plaintext).context("Decrypted seed is not valid UTF-8")?;
    let mnemonic: bip39::Mnemonic = words
        .parse()
        .map_err(|e| anyhow::anyhow!("Decrypted data is not a valid mnemonic: {}", e))?;

    Ok(mnemonic)
}

/// Check if an encrypted seed file exists.
pub fn seed_exists(data_dir: &std::path::Path) -> bool {
    data_dir.join("identity").join(ENCRYPTED_SEED_FILE).exists()
}

// ─── Encrypted LND aezeed Backup ────────────────────────────────────────
//
// LND's wallet seed is a 24-word aezeed cipher-seed generated INSIDE LND —
// it is not the BIP-39 master mnemonic and cannot be re-derived from it
// after wallet creation, so it is captured once at wallet-init time and
// stored encrypted for later reveal. aezeed words share the BIP-39 English
// wordlist but use their own checksum, so they are stored as a plain
// space-separated string, not a `bip39::Mnemonic`.

/// Encrypt and save the LND aezeed words (space-separated) to disk.
pub async fn save_lnd_aezeed_encrypted(
    data_dir: &std::path::Path,
    words: &[String],
    passphrase: &str,
) -> Result<()> {
    if words.is_empty() {
        anyhow::bail!("Refusing to save an empty aezeed");
    }
    let mut plaintext = words.join(" ");
    let blob = encrypt_blob(plaintext.as_bytes(), passphrase)?;
    plaintext.zeroize();
    write_identity_blob(data_dir, ENCRYPTED_LND_SEED_FILE, &blob).await
}

/// Load and decrypt the LND aezeed words from disk.
pub async fn load_lnd_aezeed_encrypted(
    data_dir: &std::path::Path,
    passphrase: &str,
) -> Result<Vec<String>> {
    let path = data_dir.join("identity").join(ENCRYPTED_LND_SEED_FILE);
    let blob = tokio::fs::read(&path)
        .await
        .context("Failed to read encrypted aezeed file")?;

    let plaintext = decrypt_blob(&blob, passphrase)?;
    let mut text = String::from_utf8(plaintext).context("Decrypted aezeed is not valid UTF-8")?;
    let words: Vec<String> = text.split_whitespace().map(str::to_string).collect();
    text.zeroize();
    if words.is_empty() {
        anyhow::bail!("Decrypted aezeed is empty");
    }
    Ok(words)
}

/// Check if an encrypted LND aezeed backup exists.
pub fn lnd_aezeed_exists(data_dir: &std::path::Path) -> bool {
    data_dir
        .join("identity")
        .join(ENCRYPTED_LND_SEED_FILE)
        .exists()
}

/// Whether the user has confirmed writing down the LND aezeed.
pub fn lnd_aezeed_acknowledged(data_dir: &std::path::Path) -> bool {
    data_dir.join("identity").join(LND_SEED_ACK_FILE).exists()
}

/// Remove the acknowledgment marker (a new wallet means a new seed).
pub async fn clear_lnd_aezeed_acknowledged(data_dir: &std::path::Path) {
    let _ = tokio::fs::remove_file(data_dir.join("identity").join(LND_SEED_ACK_FILE)).await;
}

/// Record that the user confirmed backing up the LND aezeed.
pub async fn mark_lnd_aezeed_acknowledged(data_dir: &std::path::Path) -> Result<()> {
    let identity_dir = data_dir.join("identity");
    tokio::fs::create_dir_all(&identity_dir)
        .await
        .context("Failed to create identity directory")?;
    tokio::fs::write(identity_dir.join(LND_SEED_ACK_FILE), b"1")
        .await
        .context("Failed to write aezeed ack marker")
}

// ─── Identity Index Tracking ────────────────────────────────────────────

/// Save the next unused identity derivation index.
pub async fn save_identity_index(data_dir: &std::path::Path, next_index: u32) -> Result<()> {
    let path = data_dir.join("identity").join(IDENTITY_INDEX_FILE);
    tokio::fs::write(&path, next_index.to_string().as_bytes())
        .await
        .context("Failed to write identity index")
}

/// Load the next unused identity derivation index (0 if none saved).
pub async fn load_identity_index(data_dir: &std::path::Path) -> Result<u32> {
    let path = data_dir.join("identity").join(IDENTITY_INDEX_FILE);
    match tokio::fs::read_to_string(&path).await {
        Ok(s) => s.trim().parse::<u32>().context("Invalid identity index"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(e).context("Failed to read identity index"),
    }
}

// ─── Internal Helpers ───────────────────────────────────────────────────

/// HKDF-SHA256 derivation with no salt, returns `len` bytes.
fn hkdf_derive(ikm: &[u8], info: &[u8], len: usize) -> Result<Vec<u8>> {
    let hk = Hkdf::<Sha256>::new(None, ikm);
    let mut okm = vec![0u8; len];
    hk.expand(info, &mut okm)
        .map_err(|_| anyhow::anyhow!("HKDF expand failed"))?;
    Ok(okm)
}

/// HKDF-SHA256 derivation with no salt, returns exactly 32 bytes.
fn hkdf_derive_32(ikm: &[u8], info: &[u8]) -> Result<[u8; 32]> {
    let bytes = hkdf_derive(ikm, info, 32)?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";

    #[test]
    fn test_deterministic_node_key() {
        let (_, seed1) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let (_, seed2) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let key1 = derive_node_ed25519(&seed1).unwrap();
        let key2 = derive_node_ed25519(&seed2).unwrap();
        assert_eq!(
            key1.verifying_key().as_bytes(),
            key2.verifying_key().as_bytes(),
            "Same mnemonic must produce same node key"
        );
    }

    #[test]
    fn test_deterministic_identity_keys() {
        let (_, seed) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let key_a = derive_identity_ed25519(&seed, 0).unwrap();
        let key_b = derive_identity_ed25519(&seed, 1).unwrap();
        assert_ne!(
            key_a.verifying_key().as_bytes(),
            key_b.verifying_key().as_bytes(),
            "Different indices must produce different keys"
        );

        // Same index is deterministic.
        let key_a2 = derive_identity_ed25519(&seed, 0).unwrap();
        assert_eq!(
            key_a.verifying_key().as_bytes(),
            key_a2.verifying_key().as_bytes(),
        );
    }

    #[test]
    fn test_node_key_differs_from_identity() {
        let (_, seed) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let node = derive_node_ed25519(&seed).unwrap();
        let identity = derive_identity_ed25519(&seed, 0).unwrap();
        assert_ne!(
            node.verifying_key().as_bytes(),
            identity.verifying_key().as_bytes(),
            "Node key and identity key must differ"
        );
    }

    #[test]
    fn test_deterministic_nostr_keys() {
        let (_, seed) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let keys1 = derive_nostr_identity_key(&seed, 0).unwrap();
        let keys2 = derive_nostr_identity_key(&seed, 0).unwrap();
        assert_eq!(
            keys1.public_key().to_hex(),
            keys2.public_key().to_hex(),
            "Same mnemonic + index must produce same Nostr key"
        );

        let keys3 = derive_nostr_identity_key(&seed, 1).unwrap();
        assert_ne!(
            keys1.public_key().to_hex(),
            keys3.public_key().to_hex(),
            "Different indices must produce different Nostr keys"
        );
    }

    #[test]
    fn test_node_nostr_key() {
        let (_, seed) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let keys1 = derive_node_nostr_key(&seed).unwrap();
        let keys2 = derive_node_nostr_key(&seed).unwrap();
        assert_eq!(keys1.public_key().to_hex(), keys2.public_key().to_hex());
    }

    #[test]
    fn test_fips_key_deterministic_and_distinct() {
        let (_, seed) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let fips1 = derive_fips_key(&seed).unwrap();
        let fips2 = derive_fips_key(&seed).unwrap();
        assert_eq!(
            fips1.public_key().to_hex(),
            fips2.public_key().to_hex(),
            "FIPS key must be deterministic for a given seed"
        );

        let nostr = derive_node_nostr_key(&seed).unwrap();
        assert_ne!(
            fips1.public_key().to_hex(),
            nostr.public_key().to_hex(),
            "FIPS key must differ from the Nostr-node key"
        );
    }

    #[test]
    fn test_bitcoin_xprv_deterministic() {
        let (_, seed) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let xprv1 = derive_bitcoin_xprv(&seed).unwrap();
        let xprv2 = derive_bitcoin_xprv(&seed).unwrap();
        assert_eq!(xprv1, xprv2);
    }

    #[test]
    fn test_lnd_entropy_deterministic() {
        let (_, seed) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let e1 = derive_lnd_entropy(&seed).unwrap();
        let e2 = derive_lnd_entropy(&seed).unwrap();
        assert_eq!(e1, e2);
        assert_eq!(e1.len(), 16);
    }

    /// The ecash mnemonic must be reproducible from the node's words alone —
    /// that reproducibility is the entire backup story ("your 24 words already
    /// cover your ecash").
    #[test]
    fn cashu_mnemonic_is_reproducible_from_the_node_seed() {
        let (_, seed) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let a = derive_cashu_mnemonic(&seed).unwrap();
        let b = derive_cashu_mnemonic(&seed).unwrap();
        assert_eq!(a.to_string(), b.to_string());
        assert_eq!(a.word_count(), 24);

        // A different node seed must yield a different ecash wallet, or two
        // nodes would derive each other's coins.
        let (other_words, _) = MasterSeed::generate().unwrap();
        let (_, other_seed) = MasterSeed::from_mnemonic_words(&other_words.to_string()).unwrap();
        assert_ne!(a.to_string(), derive_cashu_mnemonic(&other_seed).unwrap().to_string());
    }

    /// It must NOT be the node's own phrase. Restoring ecash into a
    /// third-party wallet means handing these words over, and that must never
    /// be the same as handing over the node.
    #[test]
    fn cashu_mnemonic_is_not_the_node_mnemonic() {
        let (node_mnemonic, seed) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let cashu = derive_cashu_mnemonic(&seed).unwrap();
        assert_ne!(cashu.to_string(), node_mnemonic.to_string());

        // And knowing the ecash words must not re-derive the node seed: they
        // are a one-way HKDF descendant, so the seeds they expand to differ.
        let cashu_seed = MasterSeed::from_mnemonic(&cashu);
        assert_ne!(cashu_seed.as_bytes(), seed.as_bytes());
    }

    #[test]
    fn test_generate_produces_24_words() {
        let (mnemonic, _seed) = MasterSeed::generate().unwrap();
        assert_eq!(mnemonic.word_count(), 24);
    }

    /// The deterministic counter RNG that drives the seam below now lives in
    /// `crate::entropy::testing` (KEY-05). Its `fill_bytes` behaviour and
    /// therefore its emitted byte sequence are unchanged, so the known-answer
    /// mnemonic pinned below is unchanged.
    ///
    /// Its `impl rand::CryptoRng` did **not** move: that marker was a false
    /// promise — a counter is not a cryptographic source — and KEY-05 retires it
    /// rather than relocating it. Membership of the sealed `entropy::KeyGenRng`
    /// allowlist replaces it, and unlike a marker anyone can implement, that set
    /// is closed and compiler-enforced. The crate now contains zero
    /// `impl rand::CryptoRng` blocks.
    use crate::entropy::testing::CountingRng;

    #[test]
    fn mnemonic_generation_uses_injected_rng() {
        // Regression guard for [ARCHY-1] / F-02 in
        // docs/security/ENTROPY-SEED-AUDIT-2026-07-31.md. This test cannot exist
        // against a bare `bip39::Mnemonic::generate(24)` call: there is no seam
        // to inject through, so there is no way to observe *which* RNG was used.
        let mut rng = CountingRng(0);
        let mnemonic = generate_mnemonic_with(&mut rng).unwrap();

        assert_eq!(mnemonic.word_count(), 24, "must remain 256-bit / 24 words");

        // The injected RNG drew exactly 32 bytes and they became the mnemonic's
        // entropy verbatim — proof that the RNG passed at the call site, not
        // bip39's transitive `rand::thread_rng()` default, is the one consumed.
        let expected_entropy: Vec<u8> = (0u8..32).collect();
        let from_entropy = bip39::Mnemonic::from_entropy(&expected_entropy).unwrap();
        assert_eq!(
            mnemonic.to_string(),
            from_entropy.to_string(),
            "generated mnemonic must be exactly from_entropy(injected RNG output)"
        );

        // Known-answer pin, so a silent rebinding of the entropy source (a rand
        // or bip39 bump, a feature-flag change) fails loudly rather than quietly.
        //
        // These words are a public test vector derived from entropy 0x00..=0x1f —
        // a deliberately weak, published value. It is not, and must never be, a
        // real seed.
        assert_eq!(
            mnemonic.to_string(),
            "abandon amount liar amount expire adjust cage candy arch gather drum \
             bullet absurd math era live bid rhythm alien crouch range attend \
             journey unaware",
            "injected-RNG known-answer mnemonic"
        );

        // Same RNG state in, same mnemonic out.
        let mut rng2 = CountingRng(0);
        assert_eq!(
            generate_mnemonic_with(&mut rng2).unwrap().to_string(),
            mnemonic.to_string(),
        );
    }

    #[test]
    fn mnemonic_generation_is_256_bit() {
        // Production path: OsRng, explicitly passed. 24 words = 256 bits, and two
        // successive productions from the real entropy source must differ.
        let (a, _) = MasterSeed::generate().unwrap();
        let (b, _) = MasterSeed::generate().unwrap();
        assert_eq!(a.word_count(), 24);
        assert_eq!(b.word_count(), 24);
        assert_ne!(
            a.to_string(),
            b.to_string(),
            "real entropy must not repeat across calls"
        );
    }

    #[test]
    fn test_generate_nondeterministic() {
        // Regression guard against a fixed/seeded RNG ever being wired into
        // seed generation: with real entropy, collisions are impossible.
        let mnemonics: Vec<String> = (0..64)
            .map(|_| MasterSeed::generate().unwrap().0.to_string())
            .collect();
        let unique: std::collections::HashSet<&String> = mnemonics.iter().collect();
        assert_eq!(
            unique.len(),
            mnemonics.len(),
            "duplicate mnemonics generated"
        );

        // 64 × 24 = 1536 draws from the 2048-word list should hit ~1080
        // distinct words; a low-entropy source concentrates on far fewer.
        let distinct: std::collections::HashSet<&str> = mnemonics
            .iter()
            .flat_map(|m| m.split_whitespace())
            .collect();
        assert!(
            distinct.len() > 384,
            "only {} distinct words across 64 mnemonics",
            distinct.len()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_kernel_csprng_ready_on_test_host() {
        // By the time tests run, the pool has long been initialized.
        assert_eq!(kernel_csprng_ready(), Some(true));
    }

    #[test]
    fn test_invalid_mnemonic_rejected() {
        let result = MasterSeed::from_mnemonic_words("not a valid mnemonic");
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_word_count_rejected() {
        // 12 words (valid BIP-39 but we require 24)
        let result = MasterSeed::from_mnemonic_words(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
        );
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_encrypted_storage_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let (mnemonic, _seed) = MasterSeed::generate().unwrap();
        let words = mnemonic.to_string();

        save_seed_encrypted(dir.path(), &mnemonic, "test-passphrase")
            .await
            .unwrap();
        assert!(seed_exists(dir.path()));

        let restored = load_seed_encrypted(dir.path(), "test-passphrase")
            .await
            .unwrap();
        assert_eq!(restored.to_string(), words);
    }

    #[tokio::test]
    async fn test_lnd_aezeed_storage_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        // aezeed words aren't BIP-39-checksum-valid; any 24 words must survive.
        let words: Vec<String> = (0..24).map(|i| format!("word{i}")).collect();

        assert!(!lnd_aezeed_exists(dir.path()));
        save_lnd_aezeed_encrypted(dir.path(), &words, "node-secret")
            .await
            .unwrap();
        assert!(lnd_aezeed_exists(dir.path()));

        let restored = load_lnd_aezeed_encrypted(dir.path(), "node-secret")
            .await
            .unwrap();
        assert_eq!(restored, words);

        assert!(load_lnd_aezeed_encrypted(dir.path(), "wrong")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_lnd_aezeed_empty_rejected() {
        let dir = tempfile::tempdir().unwrap();
        assert!(save_lnd_aezeed_encrypted(dir.path(), &[], "x")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_lnd_aezeed_ack_marker() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!lnd_aezeed_acknowledged(dir.path()));
        mark_lnd_aezeed_acknowledged(dir.path()).await.unwrap();
        assert!(lnd_aezeed_acknowledged(dir.path()));
    }

    #[tokio::test]
    async fn test_encrypted_storage_wrong_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let (mnemonic, _seed) = MasterSeed::generate().unwrap();

        save_seed_encrypted(dir.path(), &mnemonic, "correct")
            .await
            .unwrap();
        let result = load_seed_encrypted(dir.path(), "wrong").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_identity_index_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        // Create identity subdirectory (required by the path).
        tokio::fs::create_dir_all(dir.path().join("identity"))
            .await
            .unwrap();

        assert_eq!(load_identity_index(dir.path()).await.unwrap(), 0);
        save_identity_index(dir.path(), 5).await.unwrap();
        assert_eq!(load_identity_index(dir.path()).await.unwrap(), 5);
    }

    #[test]
    fn test_full_derivation_from_known_mnemonic() {
        // Verify all derivation paths produce valid, distinct keys from a known mnemonic.
        let (_, seed) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();

        let node_ed = derive_node_ed25519(&seed).unwrap();
        let node_nostr = derive_node_nostr_key(&seed).unwrap();
        let fips = derive_fips_key(&seed).unwrap();
        let id0_ed = derive_identity_ed25519(&seed, 0).unwrap();
        let id0_nostr = derive_nostr_identity_key(&seed, 0).unwrap();
        let _btc = derive_bitcoin_xprv(&seed).unwrap();
        let lnd = derive_lnd_entropy(&seed).unwrap();

        // All keys should be distinct (comparing hex representations).
        let node_ed_hex = hex::encode(node_ed.verifying_key().as_bytes());
        let id0_ed_hex = hex::encode(id0_ed.verifying_key().as_bytes());
        let node_nostr_hex = node_nostr.public_key().to_hex();
        let fips_hex = fips.public_key().to_hex();
        let id0_nostr_hex = id0_nostr.public_key().to_hex();
        let lnd_hex = hex::encode(lnd);

        let all = [
            &node_ed_hex,
            &id0_ed_hex,
            &node_nostr_hex,
            &fips_hex,
            &id0_nostr_hex,
            &lnd_hex,
        ];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "Keys at positions {} and {} should differ", i, j);
                }
            }
        }
    }

    #[test]
    fn test_node_key_known_answer_vs_python_verifier() {
        // Cross-checks scripts/verify-seed-derivation.py: same mnemonic must
        // produce the same node_key bytes in Rust and in the Python verifier.
        let (_, seed) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let key = derive_node_ed25519(&seed).unwrap();
        assert_eq!(
            hex::encode(key.to_bytes()),
            "3b4f4a1450450260ae360adb9c33ea5eb86356fa14454ca0067dd4b51ea8be87"
        );
        let nostr = derive_node_nostr_key(&seed).unwrap();
        assert_eq!(
            hex::encode(nostr.secret_key().to_secret_bytes()),
            "3a94fb32efab2a5025401d53fd7d82b41323a5c06ad14ce528ebe3a813d88831"
        );
    }

    /// Known answers for the derivations the test above does NOT pin.
    ///
    /// `test_full_derivation_from_known_mnemonic` only asserts these are
    /// mutually distinct, which is satisfied by ANY change to an HKDF info
    /// string or BIP-32 path. That left the LND entropy — the seed behind a
    /// user's Lightning wallet — and the FIPS mesh transport key with no
    /// known-answer coverage at all: silently redefining either broke no test
    /// while invalidating every backup verification a user had already done.
    ///
    /// The expected values were produced independently by the Python verifier
    /// published in `docs/SEED-VERIFICATION.md`, whose primitives were in turn
    /// cross-checked against `bip_utils` and `cryptography`'s own HKDF. If one
    /// of these assertions fails, either a derivation changed (and every
    /// published backup-verification instruction is now wrong), or the doc and
    /// the code have drifted apart — both are release-blocking.
    #[test]
    fn test_all_derivations_known_answers_vs_python_verifier() {
        let (_, seed) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();

        // HKDF "archipelago/fips/secp256k1/v1"
        assert_eq!(
            derive_fips_key(&seed).unwrap().public_key().to_hex(),
            "31865360f8cfb8bc3d0d5343bf09f1bc5c17b7f0b509120e94cdbed47d203bca",
            "FIPS mesh transport key"
        );

        // HKDF "archipelago/identity/{i}/ed25519/v1"
        assert_eq!(
            hex::encode(
                derive_identity_ed25519(&seed, 0)
                    .unwrap()
                    .verifying_key()
                    .as_bytes()
            ),
            "c0415866117dd570e908e1b178bf1dd8f7b9cdb8eb662353165c6f4877fe222b",
            "identity[0] Ed25519"
        );
        assert_eq!(
            hex::encode(
                derive_identity_ed25519(&seed, 1)
                    .unwrap()
                    .verifying_key()
                    .as_bytes()
            ),
            "be7a0e1fe2711dedbbba976a70443e0a70e673bcf7b6e6a8fb20d1d2684e5eba",
            "identity[1] Ed25519 — pins the index into the info string"
        );

        // BIP-32 m/44'/1237'/0'/0/{i} (NIP-06)
        assert_eq!(
            derive_nostr_identity_key(&seed, 0)
                .unwrap()
                .public_key()
                .to_hex(),
            "1ca7e48a33d62063f25d79f18617cc892ef064628aaf80af485e41849343ca52",
            "identity[0] Nostr (NIP-06)"
        );

        // BIP-32 m/84'/0'/0'
        assert_eq!(
            hex::encode(
                derive_bitcoin_xprv(&seed)
                    .unwrap()
                    .private_key
                    .secret_bytes()
            ),
            "57558e8c90c2e72f0c121d0fb8844bbbe7a872f0065d21b218a990450b9f93be",
            "Bitcoin BIP-84 account key"
        );

        // HKDF "archipelago/lnd/entropy/v1" (16 bytes)
        assert_eq!(
            hex::encode(derive_lnd_entropy(&seed).unwrap()),
            "5c86f10629bd86cdd269b82a76cc51e4",
            "LND aezeed entropy"
        );
    }

    #[test]
    fn test_release_root_deterministic_and_domain_separated() {
        let (_, seed) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let a = derive_release_root_ed25519(&seed).unwrap();
        let b = derive_release_root_ed25519(&seed).unwrap();
        assert_eq!(
            a.verifying_key().as_bytes(),
            b.verifying_key().as_bytes(),
            "Same mnemonic must produce the same release-root key"
        );
        // Must NOT collide with the node key — different HKDF domain.
        let node = derive_node_ed25519(&seed).unwrap();
        assert_ne!(
            a.verifying_key().as_bytes(),
            node.verifying_key().as_bytes(),
            "Release-root key must be domain-separated from the node key"
        );
    }

    #[test]
    fn test_release_root_known_answer() {
        // KAT pins the derivation so the signing ceremony, the pinned anchor,
        // and any external verifier agree on the bytes for a given mnemonic.
        let (_, seed) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let key = derive_release_root_ed25519(&seed).unwrap();
        assert_eq!(
            hex::encode(key.to_bytes()),
            "613ab879e5fbd4fcded32bc7ffad662fff1ce0f744c69baa63e7416ffabe7b71",
            "release-root private key KAT"
        );
        assert_eq!(
            hex::encode(key.verifying_key().to_bytes()),
            "995eaf9188617f0ecbcff9cd44d57adb9aa7dd5f34db2733e97f3e317fb0aba2",
            "release-root public key KAT"
        );
    }

    /// The node's whole identity hangs off `Mnemonic::to_seed("")`, so this
    /// pins that derivation to the BIP-39 specification vectors rather than to
    /// whatever the `bip39` crate happens to do today.
    ///
    /// It exists because the crate is not version-pinned any more: the exact
    /// `=2.1.0` pin was relaxed to `"2.1"` in 2026-08 so the `cashu` crate
    /// could resolve (the pin transitively froze `unicode-normalization` at a
    /// version with no common solution). A bump that changed derivation would
    /// silently re-key every node on the fleet and orphan every existing
    /// backup, which no amount of code review reliably catches — this does.
    #[test]
    fn seed_derivation_matches_the_bip39_specification_vectors() {
        let words = "abandon abandon abandon abandon abandon abandon abandon \
                     abandon abandon abandon abandon about"
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let mnemonic: bip39::Mnemonic = words.parse().expect("valid test mnemonic");

        // Empty passphrase — exactly how MasterSeed::from_mnemonic derives.
        assert_eq!(
            hex::encode(mnemonic.to_seed("")),
            "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc1\
             9a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
                .replace(['\n', ' '], ""),
            "BIP-39 seed derivation changed — every node's keys would move"
        );

        // With a passphrase, where NFKD normalisation actually participates;
        // this is the arm a `unicode-normalization` change could disturb.
        assert_eq!(
            hex::encode(mnemonic.to_seed("TREZOR")),
            "c55257c360c07c72029aebc1b53c05ed0362ada38ead3e3e9efa3708e5349553\
             1f09a6987599d18264c1e1c92f2cf141630c7a3c4ab7c81b2f001698e7463b04"
                .replace(['\n', ' '], ""),
            "BIP-39 passphrase normalisation changed"
        );
    }
}
