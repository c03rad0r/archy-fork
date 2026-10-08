//! Node identity: persistent Ed25519 key for private identification.
//! Enables future P2P features (file transfer, streaming, ecash/Lightning).
//! Supports did:key (W3C) for Web5/DID interoperability.

use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use std::path::{Path, PathBuf};
use tokio::fs;

const NODE_KEY_FILE: &str = "node_key";
const NODE_KEY_PUB_FILE: &str = "node_key.pub";
const FIPS_KEY_FILE: &str = "fips_key";
const FIPS_KEY_PUB_FILE: &str = "fips_key.pub";

/// Persistent node identity (Ed25519 keypair).
/// Survives reboots; used for signing, verification, and node address.
pub struct NodeIdentity {
    signing_key: SigningKey,
    _identity_dir: PathBuf,
}

impl NodeIdentity {
    /// Load existing identity or create and persist a new one.
    pub async fn load_or_create(identity_dir: &Path) -> Result<Self> {
        fs::create_dir_all(identity_dir)
            .await
            .context("Failed to create identity directory")?;

        let key_path = identity_dir.join(NODE_KEY_FILE);
        let pub_path = identity_dir.join(NODE_KEY_PUB_FILE);

        let signing_key = if key_path.exists() {
            let bytes = fs::read(&key_path)
                .await
                .context("Failed to read node key")?;
            let arr: [u8; 32] = bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid node key length"))?;
            let key = SigningKey::from_bytes(&arr);
            let pubkey_hex = hex::encode(key.verifying_key().as_bytes());
            tracing::info!(
                "Loaded existing node identity (pubkey: {}...)",
                &pubkey_hex[..16]
            );
            key
        } else {
            let signing_key = SigningKey::generate(&mut OsRng);
            fs::write(&key_path, signing_key.to_bytes())
                .await
                .context("Failed to write node key")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
                    .await
                    .context("Failed to set key permissions")?;
            }
            fs::write(&pub_path, signing_key.verifying_key().as_bytes())
                .await
                .context("Failed to write node public key")?;
            tracing::info!(
                "🔑 Generated new node identity at {}",
                identity_dir.display()
            );
            signing_key
        };

        Ok(Self {
            signing_key,
            _identity_dir: identity_dir.to_path_buf(),
        })
    }

    /// Load an existing identity for outbound requests. Never create or repair a
    /// key as a side effect of accessing another node.
    pub async fn load_existing(identity_dir: &Path) -> Result<Self> {
        let bytes = fs::read(identity_dir.join(NODE_KEY_FILE))
            .await
            .context("Existing node identity unavailable")?;
        let key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid node key length"))?;
        Ok(Self {
            signing_key: SigningKey::from_bytes(&key),
            _identity_dir: identity_dir.to_owned(),
        })
    }

    /// Create node identity from a BIP-39 master seed (deterministic derivation).
    /// Writes derived key to disk in the same format as load_or_create.
    /// Also derives and persists the FIPS mesh transport key so the
    /// FIPS system service can be unmasked after onboarding.
    pub async fn from_seed(identity_dir: &Path, seed: &crate::seed::MasterSeed) -> Result<Self> {
        fs::create_dir_all(identity_dir)
            .await
            .context("Failed to create identity directory")?;

        let signing_key = crate::seed::derive_node_ed25519(seed)?;
        let key_path = identity_dir.join(NODE_KEY_FILE);
        let pub_path = identity_dir.join(NODE_KEY_PUB_FILE);

        fs::write(&key_path, signing_key.to_bytes())
            .await
            .context("Failed to write node key")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
                .await
                .context("Failed to set key permissions")?;
        }
        fs::write(&pub_path, signing_key.verifying_key().as_bytes())
            .await
            .context("Failed to write node public key")?;

        let pubkey_hex = hex::encode(signing_key.verifying_key().as_bytes());
        tracing::info!(
            "Derived node identity from seed (pubkey: {}...)",
            &pubkey_hex[..16]
        );

        write_fips_key_from_seed(identity_dir, seed).await?;

        Ok(Self {
            signing_key,
            _identity_dir: identity_dir.to_path_buf(),
        })
    }

    /// Check if a node key already exists on disk.
    pub fn key_exists(identity_dir: &Path) -> bool {
        identity_dir.join(NODE_KEY_FILE).exists()
    }

    /// Access the signing key (for key derivation, e.g. mesh encryption).
    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }

    /// Public key as hex string (for ServerInfo, Nostr, etc.)
    pub fn pubkey_hex(&self) -> String {
        hex::encode(self.signing_key.verifying_key().as_bytes())
    }

    /// Stable node ID derived from pubkey (first 16 chars of hex).
    pub fn node_id(&self) -> String {
        self.pubkey_hex().chars().take(16).collect()
    }

    /// Sign data; returns hex-encoded signature.
    pub fn sign(&self, data: &[u8]) -> String {
        hex::encode(self.signing_key.sign(data).to_bytes())
    }

    /// Verify a signature from a peer (pubkey hex, data, signature hex).
    pub fn verify(pubkey_hex: &str, data: &[u8], sig_hex: &str) -> Result<bool> {
        let bytes = hex::decode(pubkey_hex).context("Invalid pubkey hex")?;
        let verifying_key = VerifyingKey::from_bytes(
            bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid pubkey length"))?,
        )?;
        let sig_bytes = hex::decode(sig_hex).context("Invalid signature hex")?;
        let sig = Signature::from_bytes(
            sig_bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid signature length"))?,
        );
        Ok(verifying_key.verify(data, &sig).is_ok())
    }

    /// Node address format for invites: archipelago://<onion>#<pubkey>
    pub fn node_address(&self, onion: &str) -> String {
        format!(
            "archipelago://{}#{}",
            onion.trim_end_matches('/'),
            self.pubkey_hex()
        )
    }

    /// DID in did:key format (W3C did:key method, Ed25519).
    /// Format: did:key:z&lt;base58btc(multicodec_ed25519_pub + 32-byte pubkey)&gt;
    pub fn did_key(&self) -> Result<String> {
        did_key_from_pubkey_hex(&self.pubkey_hex())
            .map_err(|e| anyhow::anyhow!("Invalid pubkey hex: {}", e))
    }

    /// Generate a W3C DID Document for this identity.
    #[allow(dead_code)]
    pub fn did_document(&self) -> Result<serde_json::Value> {
        did_document_from_pubkey_hex(&self.pubkey_hex())
    }
}

// ─── FIPS mesh transport key ────────────────────────────────────────────
//
// FIPS (Free Internetworking Peering System) uses a secp256k1 keypair as its
// native node identity — independent of the Nostr-node key so compromise of
// one surface cannot impersonate on the other. Both are seed-derived, so the
// FIPS npub is recoverable from the master mnemonic.
//
// Key material is written by `NodeIdentity::from_seed` only. Pre-onboarding
// the files do not exist and `archipelago-fips.service` stays masked.

use nostr_sdk::ToBech32;

async fn write_fips_key_from_seed(
    identity_dir: &Path,
    seed: &crate::seed::MasterSeed,
) -> Result<()> {
    let keys = crate::seed::derive_fips_key(seed)?;
    let key_path = identity_dir.join(FIPS_KEY_FILE);
    let pub_path = identity_dir.join(FIPS_KEY_PUB_FILE);

    // fips daemon reads the key with `fs::read_to_string` and expects a
    // bech32 nsec line — raw 32-byte secret bytes fail its UTF-8 check
    // ("failed to read config file /etc/fips/fips.key: stream did not
    // contain valid UTF-8"). Write the bech32 form with a trailing
    // newline so both archipelago and fips load it cleanly.
    let nsec = keys
        .secret_key()
        .to_bech32()
        .context("Failed to encode FIPS nsec")?;
    fs::write(&key_path, format!("{nsec}\n"))
        .await
        .context("Failed to write FIPS key")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
            .await
            .context("Failed to set FIPS key permissions")?;
    }
    // Upstream fips daemon expects 32 raw bytes in /etc/fips/fips.pub —
    // not a bech32 npub string. Writing the bech32 form here meant the
    // installed .pub file was a 63-char text file the daemon parsed as
    // 63 raw bytes of garbage, so it couldn't identify itself to peers
    // and the anchor never handshook. Write the raw public-key bytes
    // (PublicKey::to_bytes returns a [u8; 32]) so the daemon reads
    // them directly.
    let raw_pub: [u8; 32] = keys.public_key().to_bytes();
    fs::write(&pub_path, raw_pub)
        .await
        .context("Failed to write FIPS public key")?;

    let npub_for_log = keys.public_key().to_bech32().unwrap_or_default();
    tracing::info!(
        "Derived FIPS mesh key from seed (npub: {}...)",
        npub_for_log.chars().take(20).collect::<String>()
    );
    Ok(())
}

/// Check whether the FIPS keypair has been materialised on disk.
/// Returns true only after onboarding has written the seed-derived key.
#[allow(dead_code)]
pub fn fips_key_exists(identity_dir: &Path) -> bool {
    identity_dir.join(FIPS_KEY_FILE).exists()
}

/// Load the persisted FIPS keypair. Returns `Ok(None)` if onboarding has
/// not yet written the key (pre-onboarding node); errors only on I/O or
/// corruption of an existing file.
#[allow(dead_code)]
pub async fn load_fips_keys(identity_dir: &Path) -> Result<Option<nostr_sdk::Keys>> {
    let key_path = identity_dir.join(FIPS_KEY_FILE);
    // Read as raw bytes so we can detect and migrate both formats:
    //   - v1.6+: bech32 nsec text (what upstream fips expects)
    //   - <=v1.5: raw 32-byte secret (incompatible with upstream fips)
    // When we find the legacy format, rewrite the file in bech32 in place
    // so archipelago-fips.service stops crashlooping after an OTA update
    // from a release that shipped the old format.
    let bytes = match fs::read(&key_path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).context("Failed to read FIPS key"),
    };

    // Try bech32 first.
    if let Ok(text) = std::str::from_utf8(&bytes) {
        if let Ok(secret) = nostr_sdk::SecretKey::parse(text.trim()) {
            return Ok(Some(nostr_sdk::Keys::new(secret)));
        }
    }

    // Fall through: treat as legacy raw bytes and migrate.
    if bytes.len() == 32 {
        let secret = nostr_sdk::SecretKey::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("Corrupt FIPS key on disk: {}", e))?;
        let nsec = secret
            .to_bech32()
            .map_err(|e| anyhow::anyhow!("Failed to encode migrated nsec: {}", e))?;
        fs::write(&key_path, format!("{nsec}\n"))
            .await
            .context("Failed to rewrite FIPS key in bech32 format")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
                .await
                .context("Failed to re-set FIPS key permissions after migration")?;
        }
        tracing::info!("Migrated legacy raw-bytes FIPS key to bech32 nsec text");
        return Ok(Some(nostr_sdk::Keys::new(secret)));
    }

    anyhow::bail!(
        "Corrupt FIPS key on disk (not bech32 nsec and not 32 raw bytes, size={})",
        bytes.len()
    )
}

/// Return the FIPS npub (bech32) if the key has been materialised.
#[allow(dead_code)]
pub async fn fips_npub(identity_dir: &Path) -> Result<Option<String>> {
    Ok(load_fips_keys(identity_dir)
        .await?
        .and_then(|k| k.public_key().to_bech32().ok()))
}

/// Convert Ed25519 pubkey (hex) to did:key format.
/// Used by RPC when identity is loaded from state.
pub fn did_key_from_pubkey_hex(pubkey_hex: &str) -> Result<String> {
    let bytes = hex::decode(pubkey_hex).context("Invalid pubkey hex")?;
    if bytes.len() != 32 {
        return Err(anyhow::anyhow!("Invalid pubkey length"));
    }
    let mut multicodec_pubkey = [0u8; 34];
    multicodec_pubkey[0] = 0xed;
    multicodec_pubkey[1] = 0x01;
    multicodec_pubkey[2..34].copy_from_slice(&bytes);
    Ok(format!(
        "did:key:z{}",
        bs58::encode(multicodec_pubkey).into_string()
    ))
}

/// Generate a W3C DID Core v1.0 compliant DID Document from an Ed25519 public key.
/// Follows: https://www.w3.org/TR/did-core/
/// Includes: verificationMethod, authentication, assertionMethod, keyAgreement contexts.
pub fn did_document_from_pubkey_hex(pubkey_hex: &str) -> Result<serde_json::Value> {
    let did = did_key_from_pubkey_hex(pubkey_hex)?;
    let pubkey_bytes = hex::decode(pubkey_hex).context("Invalid pubkey hex")?;
    let pubkey_multibase = format!("z{}", bs58::encode(&pubkey_bytes).into_string());
    let key_id = format!("{}#key-1", did);

    // Build X25519 key agreement key from Ed25519 public key
    // Ed25519 -> X25519 conversion (Montgomery form)
    let ed_point = curve25519_dalek::edwards::CompressedEdwardsY(
        pubkey_bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid pubkey length"))?,
    );
    let x25519_key = if let Some(point) = ed_point.decompress() {
        let montgomery = point.to_montgomery();
        format!("z{}", bs58::encode(montgomery.as_bytes()).into_string())
    } else {
        // Fallback: use Ed25519 key if conversion fails
        pubkey_multibase.clone()
    };
    let x25519_key_id = format!("{}#key-x25519-1", did);

    Ok(serde_json::json!({
        "@context": [
            "https://www.w3.org/ns/did/v1",
            "https://w3id.org/security/suites/ed25519-2020/v1",
            "https://w3id.org/security/suites/x25519-2020/v1"
        ],
        "id": did,
        "verificationMethod": [
            {
                "id": key_id,
                "type": "Ed25519VerificationKey2020",
                "controller": did,
                "publicKeyMultibase": pubkey_multibase
            },
            {
                "id": x25519_key_id,
                "type": "X25519KeyAgreementKey2020",
                "controller": did,
                "publicKeyMultibase": x25519_key
            }
        ],
        "authentication": [key_id],
        "assertionMethod": [key_id],
        "capabilityInvocation": [key_id],
        "capabilityDelegation": [key_id],
        "keyAgreement": [x25519_key_id]
    }))
}

/// Generate a DID Document that includes both the Ed25519 key and a Nostr secp256k1 key.
/// The Nostr key is added as an additional verification method, formally pairing
/// the two identities so a user can use either protocol.
pub fn did_document_with_nostr(
    pubkey_hex: &str,
    nostr_pubkey_hex: &str,
) -> Result<serde_json::Value> {
    let mut doc = did_document_from_pubkey_hex(pubkey_hex)?;
    let did = did_key_from_pubkey_hex(pubkey_hex)?;
    let nostr_key_id = format!("{}#key-nostr-1", did);

    // Add EcdsaSecp256k1VerificationKey2019 context
    if let Some(contexts) = doc["@context"].as_array_mut() {
        contexts.push(serde_json::json!(
            "https://w3id.org/security/suites/secp256k1-2019/v1"
        ));
    }

    // Add Nostr secp256k1 key to verificationMethod array
    if let Some(vms) = doc["verificationMethod"].as_array_mut() {
        vms.push(serde_json::json!({
            "id": nostr_key_id,
            "type": "EcdsaSecp256k1VerificationKey2019",
            "controller": did,
            "publicKeyHex": nostr_pubkey_hex
        }));
    }

    // Add to authentication (Nostr key can also authenticate)
    if let Some(auth) = doc["authentication"].as_array_mut() {
        auth.push(serde_json::json!(nostr_key_id));
    }

    Ok(doc)
}

/// Extract the raw 32-byte Ed25519 public key from a did:key string.
pub fn pubkey_bytes_from_did_key(did: &str) -> Result<[u8; 32]> {
    let multibase_str = did
        .strip_prefix("did:key:z")
        .ok_or_else(|| anyhow::anyhow!("Invalid did:key format"))?;
    let decoded = bs58::decode(multibase_str)
        .into_vec()
        .context("Invalid base58 in did:key")?;
    if decoded.len() != 34 || decoded[0] != 0xed || decoded[1] != 0x01 {
        return Err(anyhow::anyhow!("Invalid Ed25519 multicodec prefix"));
    }
    let mut pubkey = [0u8; 32];
    pubkey.copy_from_slice(&decoded[2..34]);
    Ok(pubkey)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_or_create_generates_new_identity() {
        let dir = tempfile::tempdir().unwrap();
        let identity_dir = dir.path().join("identity");

        let identity = NodeIdentity::load_or_create(&identity_dir).await.unwrap();

        // pubkey_hex should be 64 hex chars (32 bytes)
        assert_eq!(identity.pubkey_hex().len(), 64);
        // node_id should be first 16 chars of pubkey_hex
        assert_eq!(identity.node_id(), &identity.pubkey_hex()[..16]);
    }

    #[tokio::test]
    async fn test_load_or_create_persists_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let identity_dir = dir.path().join("identity");

        let identity1 = NodeIdentity::load_or_create(&identity_dir).await.unwrap();
        let pubkey1 = identity1.pubkey_hex();

        let identity2 = NodeIdentity::load_or_create(&identity_dir).await.unwrap();
        let pubkey2 = identity2.pubkey_hex();

        assert_eq!(pubkey1, pubkey2);
    }

    #[tokio::test]
    async fn test_sign_and_verify() {
        let dir = tempfile::tempdir().unwrap();
        let identity = NodeIdentity::load_or_create(&dir.path().join("id"))
            .await
            .unwrap();

        let data = b"hello world";
        let sig = identity.sign(data);

        let valid = NodeIdentity::verify(&identity.pubkey_hex(), data, &sig).unwrap();
        assert!(valid);
    }

    #[tokio::test]
    async fn test_verify_wrong_data() {
        let dir = tempfile::tempdir().unwrap();
        let identity = NodeIdentity::load_or_create(&dir.path().join("id"))
            .await
            .unwrap();

        let sig = identity.sign(b"hello");
        let valid = NodeIdentity::verify(&identity.pubkey_hex(), b"wrong", &sig).unwrap();
        assert!(!valid);
    }

    #[tokio::test]
    async fn test_did_key_format() {
        let dir = tempfile::tempdir().unwrap();
        let identity = NodeIdentity::load_or_create(&dir.path().join("id"))
            .await
            .unwrap();

        let did = identity.did_key().unwrap();
        assert!(did.starts_with("did:key:z"));
    }

    #[test]
    fn test_did_key_from_pubkey_hex() {
        // 32-byte all-zeros pubkey in hex
        let hex = "0000000000000000000000000000000000000000000000000000000000000000";
        let did = did_key_from_pubkey_hex(hex).unwrap();
        assert!(did.starts_with("did:key:z"));
    }

    #[test]
    fn test_did_key_from_invalid_hex() {
        assert!(did_key_from_pubkey_hex("not_hex").is_err());
    }

    #[test]
    fn test_did_key_from_wrong_length() {
        assert!(did_key_from_pubkey_hex("0011").is_err());
    }

    #[tokio::test]
    async fn test_node_address_format() {
        let dir = tempfile::tempdir().unwrap();
        let identity = NodeIdentity::load_or_create(&dir.path().join("id"))
            .await
            .unwrap();

        let addr = identity.node_address("abc123.onion");
        assert!(addr.starts_with("archipelago://abc123.onion#"));
        assert!(addr.contains(&identity.pubkey_hex()));
    }

    #[tokio::test]
    async fn test_did_document_w3c_structure() {
        let dir = tempfile::tempdir().unwrap();
        let identity = NodeIdentity::load_or_create(&dir.path().join("id"))
            .await
            .unwrap();

        let doc = identity.did_document().unwrap();
        let did = identity.did_key().unwrap();

        // Verify @context
        let context = doc["@context"].as_array().unwrap();
        assert_eq!(context[0], "https://www.w3.org/ns/did/v1");

        // Verify id matches did:key
        assert_eq!(doc["id"], did);

        // Verify verificationMethod has Ed25519 and X25519 keys
        let vms = doc["verificationMethod"].as_array().unwrap();
        assert_eq!(vms.len(), 2);
        assert_eq!(vms[0]["type"], "Ed25519VerificationKey2020");
        assert_eq!(vms[1]["type"], "X25519KeyAgreementKey2020");
        assert_eq!(vms[0]["controller"], did);

        // Verify authentication references key-1
        let auth = doc["authentication"].as_array().unwrap();
        assert_eq!(auth[0], format!("{}#key-1", did));

        // Verify assertionMethod
        assert!(doc["assertionMethod"].as_array().is_some());

        // Verify keyAgreement references x25519 key
        let ka = doc["keyAgreement"].as_array().unwrap();
        assert_eq!(ka[0], format!("{}#key-x25519-1", did));
    }

    #[test]
    fn test_did_document_from_pubkey_hex() {
        let hex = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa3f4a18446b7e21e7e2c33";
        let doc = did_document_from_pubkey_hex(hex).unwrap();
        assert_eq!(doc["@context"].as_array().unwrap().len(), 3);
        assert!(doc["id"].as_str().unwrap().starts_with("did:key:z"));
    }

    #[test]
    fn test_pubkey_bytes_from_did_key_roundtrip() {
        let hex = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa3f4a18446b7e21e7e2c33";
        let did = did_key_from_pubkey_hex(hex).unwrap();
        let recovered = pubkey_bytes_from_did_key(&did).unwrap();
        assert_eq!(hex::encode(recovered), hex);
    }

    #[test]
    fn test_pubkey_bytes_from_invalid_did() {
        assert!(pubkey_bytes_from_did_key("did:web:example.com").is_err());
        assert!(pubkey_bytes_from_did_key("did:key:invalid").is_err());
    }

    #[tokio::test]
    async fn test_fips_key_absent_before_onboarding() {
        let dir = tempfile::tempdir().unwrap();
        let id_dir = dir.path().join("identity");
        fs::create_dir_all(&id_dir).await.unwrap();

        assert!(!fips_key_exists(&id_dir));
        assert!(load_fips_keys(&id_dir).await.unwrap().is_none());
        assert!(fips_npub(&id_dir).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_fips_key_written_from_seed_and_roundtrips() {
        use crate::seed::MasterSeed;
        const M: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        let dir = tempfile::tempdir().unwrap();
        let id_dir = dir.path().join("identity");
        let (_, seed) = MasterSeed::from_mnemonic_words(M).unwrap();

        let _ = NodeIdentity::from_seed(&id_dir, &seed).await.unwrap();

        assert!(fips_key_exists(&id_dir));
        let loaded = load_fips_keys(&id_dir).await.unwrap().unwrap();
        let expected = crate::seed::derive_fips_key(&seed).unwrap();
        assert_eq!(
            loaded.public_key().to_hex(),
            expected.public_key().to_hex(),
            "loaded FIPS key must match seed-derived key"
        );

        let npub = fips_npub(&id_dir).await.unwrap().unwrap();
        assert!(npub.starts_with("npub1"), "got: {}", npub);
    }

    #[tokio::test]
    async fn test_fips_private_key_is_chmod_600() {
        #[cfg(unix)]
        {
            use crate::seed::MasterSeed;
            use std::os::unix::fs::PermissionsExt;
            const M: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
            let dir = tempfile::tempdir().unwrap();
            let id_dir = dir.path().join("identity");
            let (_, seed) = MasterSeed::from_mnemonic_words(M).unwrap();

            NodeIdentity::from_seed(&id_dir, &seed).await.unwrap();

            let meta = fs::metadata(id_dir.join(FIPS_KEY_FILE)).await.unwrap();
            let mode = meta.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "FIPS private key must be 0600, got {:o}", mode);
        }
    }
}
