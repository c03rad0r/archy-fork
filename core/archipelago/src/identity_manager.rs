//! Multi-identity manager: multiple Ed25519 identities with DID support.
//! Each identity has a keypair, display name, purpose tag, and DID:key.
//! Identities are stored as JSON files encrypted with the node's master key.

use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;

use crate::identity::did_key_from_pubkey_hex;
use nostr_sdk::ToBech32;

const IDENTITIES_DIR: &str = "identities";
const DEFAULT_MARKER: &str = ".default";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IdentityPurpose {
    Personal,
    Business,
    Anonymous,
}

impl std::fmt::Display for IdentityPurpose {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentityPurpose::Personal => write!(f, "personal"),
            IdentityPurpose::Business => write!(f, "business"),
            IdentityPurpose::Anonymous => write!(f, "anonymous"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityRecord {
    pub id: String,
    pub name: String,
    pub purpose: IdentityPurpose,
    pub pubkey_hex: String,
    pub did: String,
    /// did:dht identifier (published to Mainline DHT for discoverability)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dht_did: Option<String>,
    pub created_at: String,
    /// Nostr secp256k1 public key in hex format
    pub nostr_pubkey: Option<String>,
    /// Nostr public key in bech32 npub format (NIP-19)
    pub nostr_npub: Option<String>,
    /// Nostr profile metadata (NIP-01 kind 0)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<IdentityProfile>,
}

/// Nostr profile metadata fields (NIP-01 kind 0 + NIP-24 extra fields).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdentityProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nip05: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lud16: Option<String>,
}

/// On-disk format for identity storage (includes secret key bytes).
#[derive(Serialize, Deserialize)]
struct IdentityFile {
    id: String,
    name: String,
    purpose: IdentityPurpose,
    secret_key: Vec<u8>,
    pubkey_hex: String,
    did: String,
    created_at: String,
    #[serde(default)]
    nostr_secret_hex: Option<String>,
    #[serde(default)]
    nostr_pubkey_hex: Option<String>,
    /// Nostr profile metadata
    #[serde(default)]
    profile: Option<IdentityProfile>,
    /// BIP-39 seed derivation index (if created from seed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    derivation_index: Option<u32>,
}

pub struct IdentityManager {
    identities_dir: PathBuf,
}

/// Result of a multi-relay profile broadcast.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProfilePublishOutcome {
    pub event_id: String,
    pub accepted: Vec<String>,
    pub rejected: Vec<(String, String)>,
}

/// Relay URL equality that tolerates minor normalization differences
/// (trailing slash, case). nostr-sdk canonicalises URLs internally and
/// we compare on the surface strings, so be liberal about what matches.
fn relay_url_matches(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim_end_matches('/').trim().to_ascii_lowercase();
    norm(a) == norm(b)
}

/// True when `record` is the node's own identity: the one whose ed25519 key
/// is the node key (`server_info.pubkey`). `identity.list` reports this as
/// `is_node`, and clients must never offer it as an app signer.
pub fn is_node_identity(record: &IdentityRecord, node_pubkey_hex: &str) -> bool {
    !node_pubkey_hex.is_empty() && record.pubkey_hex == node_pubkey_hex
}

/// True when the app identity picker hides `record`, mirroring
/// `NostrIdentityPicker.vue`'s filter exactly: the node identity
/// (`is_node`), any `node-*` id and any identity named "Node".
pub(crate) fn is_hidden_from_app_signer(record: &IdentityRecord, node_pubkey_hex: &str) -> bool {
    // Match ECMAScript trim exactly: Rust includes U+0085 and excludes U+FEFF.
    let trim_js = |value: &str| value.trim_matches(is_js_whitespace).to_lowercase();
    is_node_identity(record, node_pubkey_hex)
        || trim_js(&record.id).starts_with("node-")
        || trim_js(&record.name) == "node"
}

fn is_js_whitespace(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000D}' | '\u{0020}' | '\u{00A0}' | '\u{1680}'
        | '\u{2000}'..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}'
        | '\u{205F}' | '\u{3000}' | '\u{FEFF}')
}

impl IdentityManager {
    pub async fn new(data_dir: &Path) -> Result<Self> {
        let identities_dir = data_dir.join(IDENTITIES_DIR);
        fs::create_dir_all(&identities_dir)
            .await
            .context("Failed to create identities directory")?;
        Ok(Self { identities_dir })
    }

    /// List all identities (without secret keys).
    pub async fn list(&self) -> Result<(Vec<IdentityRecord>, Option<String>)> {
        let default_id = self.get_default_id().await;
        let mut identities = Vec::new();

        let mut entries = fs::read_dir(&self.identities_dir)
            .await
            .context("Failed to read identities directory")?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match self.load_record(&path).await {
                Ok(record) => identities.push(record),
                Err(e) => {
                    tracing::warn!("Skipping corrupt identity file {:?}: {}", path, e);
                }
            }
        }

        identities.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok((identities, default_id))
    }

    /// Nostr public keys of the identities an app may sign with through the
    /// NIP-07 bridge, as sorted, de-duplicated, comma-joined lowercase hex.
    ///
    /// Leaves out what the identity picker hides (`is_hidden_from_app_signer`)
    /// and identities without a Nostr key (they cannot sign). Empty when no
    /// identity qualifies.
    pub async fn app_signable_nostr_pubkeys(&self, node_pubkey_hex: &str) -> Result<String> {
        let (identities, _) = self.list().await?;
        let mut pubkeys: Vec<String> = identities
            .iter()
            .filter(|r| !is_hidden_from_app_signer(r, node_pubkey_hex))
            .filter_map(|r| r.nostr_pubkey.as_deref())
            .map(|key| {
                anyhow::ensure!(key.len() == 64, "invalid app owner Nostr public key");
                nostr_sdk::PublicKey::from_hex(key)
                    .map(|key| key.to_hex())
                    .context("invalid app owner Nostr public key")
            })
            .collect::<Result<Vec<_>>>()?;
        pubkeys.sort();
        pubkeys.dedup();
        Ok(pubkeys.join(","))
    }

    /// Import a business key without modifying any existing identity or default.
    pub async fn import_nostr(
        &self,
        name: String,
        nsec: &str,
        expected_npub: &str,
    ) -> Result<IdentityRecord> {
        anyhow::ensure!(!name.trim().is_empty() && name.len() <= 100, "Invalid identity name");
        anyhow::ensure!(nsec.starts_with("nsec1") && nsec.len() == 63, "Enter a plain nsec owner key");
        let secret = nostr_sdk::SecretKey::parse(nsec)
            .map_err(|_| anyhow::anyhow!("Invalid owner key"))?;
        let keys = nostr_sdk::Keys::new(secret);
        let nostr_pubkey = keys.public_key().to_hex();
        anyhow::ensure!(keys.public_key().to_bech32()? == expected_npub,
            "Owner key does not match this website");

        // Serializes imports only; mature creation/signing paths are untouched.
        static IMPORT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        let _guard = IMPORT_LOCK.lock().await;
        let (existing, _) = self.list().await?;
        if let Some(record) = existing.into_iter().find(|record| {
            record.purpose == IdentityPurpose::Business
                && record.nostr_pubkey.as_deref() == Some(nostr_pubkey.as_str())
        }) {
            return Ok(record);
        }

        let signing_key = SigningKey::generate(&mut OsRng);
        let pubkey_hex = hex::encode(signing_key.verifying_key().as_bytes());
        let id = uuid::Uuid::new_v4().to_string();
        let record = IdentityFile {
            id: id.clone(),
            name,
            purpose: IdentityPurpose::Business,
            secret_key: signing_key.to_bytes().to_vec(),
            did: did_key_from_pubkey_hex(&pubkey_hex)?,
            profile: Some(IdentityProfile {
                picture: Some(crate::avatar::identicon(&pubkey_hex)),
                ..Default::default()
            }),
            pubkey_hex,
            created_at: chrono::Utc::now().to_rfc3339(),
            nostr_secret_hex: Some(keys.secret_key().display_secret().to_string()),
            nostr_pubkey_hex: Some(nostr_pubkey),
            derivation_index: None,
        };
        let encoded = serde_json::to_vec_pretty(&record)?;
        // Hidden staging file is never visible as an incomplete identity to readers.
        let staging = self.identities_dir.join(format!(".import-{id}.tmp"));
        let destination = self.identities_dir.join(format!("{id}.json"));
        let write_result: Result<()> = async {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            let mut file = options.open(&staging).await?;
            tokio::io::AsyncWriteExt::write_all(&mut file, &encoded).await?;
            tokio::io::AsyncWriteExt::flush(&mut file).await?;
            file.sync_all().await?;
            // Atomic publication, and unlike rename this cannot replace a file.
            fs::hard_link(&staging, &destination).await?;
            Ok(())
        }.await;
        let _ = fs::remove_file(&staging).await;
        write_result.context("Could not save imported identity")?;
        self.get(&id).await
    }

    /// Create a new identity.
    pub async fn create(&self, name: String, purpose: IdentityPurpose) -> Result<IdentityRecord> {
        let signing_key = SigningKey::generate(&mut OsRng);
        let pubkey_hex = hex::encode(signing_key.verifying_key().as_bytes());
        let did = did_key_from_pubkey_hex(&pubkey_hex)?;
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();

        // Every new identity gets a deterministic default avatar derived from
        // its pubkey. Non-seed identities aren't the master node, so they use
        // the 5×5 identicon (never the hexagonal node silhouette).
        let default_profile = IdentityProfile {
            picture: Some(crate::avatar::identicon(&pubkey_hex)),
            ..Default::default()
        };

        let identity_file = IdentityFile {
            id: id.clone(),
            name: name.clone(),
            purpose: purpose.clone(),
            secret_key: signing_key.to_bytes().to_vec(),
            pubkey_hex: pubkey_hex.clone(),
            did: did.clone(),
            created_at: created_at.clone(),
            nostr_secret_hex: None,
            nostr_pubkey_hex: None,
            profile: Some(default_profile),
            derivation_index: None,
        };

        let file_path = self.identities_dir.join(format!("{}.json", id));
        let json =
            serde_json::to_string_pretty(&identity_file).context("Failed to serialize identity")?;
        fs::write(&file_path, json.as_bytes())
            .await
            .context("Failed to write identity file")?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o600))
                .await
                .context("Failed to set identity file permissions")?;
        }

        // If this is the first identity, make it the default
        let (existing, _) = self.list().await?;
        if existing.len() <= 1 {
            self.set_default(&id).await?;
        }

        // Auto-generate Nostr keypair so every identity has both key types (legacy path)
        let _ = self.create_nostr_key(&id).await;

        // Re-read to pick up the Nostr keys
        let record = self.get(&id).await?;

        tracing::info!("Created identity '{}' ({})", name, purpose);

        Ok(record)
    }

    /// Mirror an existing Ed25519 signing key as a manager-level identity.
    ///
    /// Used at boot to expose the node's own seed-derived key (the one that
    /// backs `server_info.pubkey` and peer-to-peer connections) as an
    /// entry in the Identities page, so all three surfaces — DID Status,
    /// "Node" entry on Identities, and peer-connect DID — resolve to the
    /// same DID. The id is deterministic (`node-<pubkey16>`), so repeated
    /// calls on the same key are idempotent: if the file already exists
    /// we return the existing record untouched.
    pub async fn create_from_signing_key(
        &self,
        name: String,
        purpose: IdentityPurpose,
        signing_key: SigningKey,
    ) -> Result<IdentityRecord> {
        let pubkey_hex = hex::encode(signing_key.verifying_key().as_bytes());
        let did = did_key_from_pubkey_hex(&pubkey_hex)?;
        let id = format!("node-{}", &pubkey_hex[..16]);

        // Idempotent: if we already mirrored this key, just return it.
        let file_path = self.identities_dir.join(format!("{}.json", id));
        if file_path.exists() {
            return self.get(&id).await;
        }

        let created_at = chrono::Utc::now().to_rfc3339();
        // Mark as the node (master) identity so it gets the hex SVG.
        let default_profile = IdentityProfile {
            picture: Some(crate::avatar::default_picture(&pubkey_hex, true)),
            ..Default::default()
        };

        let identity_file = IdentityFile {
            id: id.clone(),
            name: name.clone(),
            purpose: purpose.clone(),
            secret_key: signing_key.to_bytes().to_vec(),
            pubkey_hex: pubkey_hex.clone(),
            did: did.clone(),
            created_at,
            nostr_secret_hex: None,
            nostr_pubkey_hex: None,
            profile: Some(default_profile),
            derivation_index: Some(0),
        };

        let json =
            serde_json::to_string_pretty(&identity_file).context("Failed to serialize identity")?;
        fs::write(&file_path, json.as_bytes())
            .await
            .context("Failed to write identity file")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o600))
                .await
                .context("Failed to set identity file permissions")?;
        }

        // First identity becomes the default.
        let (existing, _) = self.list().await?;
        if existing.len() <= 1 {
            self.set_default(&id).await?;
        }

        tracing::info!(
            "Mirrored node signing key as Node identity '{}' ({})",
            name,
            purpose
        );

        self.get(&id).await
    }

    /// Create a new identity with keys derived from a BIP-39 master seed.
    /// The derivation index is auto-incremented and persisted.
    pub async fn create_from_seed(
        &self,
        name: String,
        purpose: IdentityPurpose,
        seed: &crate::seed::MasterSeed,
        data_dir: &std::path::Path,
    ) -> Result<IdentityRecord> {
        let index = crate::seed::load_identity_index(data_dir).await?;

        let signing_key = crate::seed::derive_identity_ed25519(seed, index)?;
        let pubkey_hex = hex::encode(signing_key.verifying_key().as_bytes());
        let did = did_key_from_pubkey_hex(&pubkey_hex)?;
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();

        // Derive Nostr key from the same seed via BIP-32.
        let nostr_keys = crate::seed::derive_nostr_identity_key(seed, index)?;
        let nostr_secret_hex = nostr_keys.secret_key().display_secret().to_string();
        let nostr_pubkey_hex = nostr_keys.public_key().to_hex();

        // Derivation index 0 is the primary seed-derived identity — the
        // "master" node identity — and gets the distinctive hexagonal SVG.
        // Later indices get the standard identicon.
        let default_profile = IdentityProfile {
            picture: Some(crate::avatar::default_picture(&pubkey_hex, index == 0)),
            ..Default::default()
        };

        let identity_file = IdentityFile {
            id: id.clone(),
            name: name.clone(),
            purpose: purpose.clone(),
            secret_key: signing_key.to_bytes().to_vec(),
            pubkey_hex: pubkey_hex.clone(),
            did: did.clone(),
            created_at: created_at.clone(),
            nostr_secret_hex: Some(nostr_secret_hex),
            nostr_pubkey_hex: Some(nostr_pubkey_hex),
            profile: Some(default_profile),
            derivation_index: Some(index),
        };

        let file_path = self.identities_dir.join(format!("{}.json", id));
        let json =
            serde_json::to_string_pretty(&identity_file).context("Failed to serialize identity")?;
        fs::write(&file_path, json.as_bytes())
            .await
            .context("Failed to write identity file")?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o600))
                .await
                .context("Failed to set identity file permissions")?;
        }

        // Increment the derivation index for next identity.
        crate::seed::save_identity_index(data_dir, index + 1).await?;

        // If first identity, make it the default.
        let (existing, _) = self.list().await?;
        if existing.len() <= 1 {
            self.set_default(&id).await?;
        }

        let record = self.get(&id).await?;
        tracing::info!(
            "Created seed-derived identity '{}' ({}) at index {}",
            name,
            purpose,
            index
        );

        Ok(record)
    }

    /// Get a single identity by ID (without secret key).
    pub async fn get(&self, id: &str) -> Result<IdentityRecord> {
        let file_path = self.identities_dir.join(format!("{}.json", id));
        if !file_path.exists() {
            return Err(anyhow::anyhow!("Identity not found: {}", id));
        }
        self.load_record(&file_path).await
    }

    /// Delete an identity.
    pub async fn delete(&self, id: &str) -> Result<()> {
        let file_path = self.identities_dir.join(format!("{}.json", id));
        if !file_path.exists() {
            return Err(anyhow::anyhow!("Identity not found: {}", id));
        }

        fs::remove_file(&file_path)
            .await
            .context("Failed to delete identity file")?;

        // If this was the default, clear the marker
        if let Some(default_id) = self.get_default_id().await {
            if default_id == id {
                let marker = self.identities_dir.join(DEFAULT_MARKER);
                let _ = fs::remove_file(marker).await;

                // Set a new default if other identities exist
                let (remaining, _) = self.list().await?;
                if let Some(first) = remaining.first() {
                    self.set_default(&first.id).await?;
                }
            }
        }

        tracing::info!("Deleted identity {}", id);
        Ok(())
    }

    /// Set the default identity.
    pub async fn set_default(&self, id: &str) -> Result<()> {
        // Verify it exists
        let file_path = self.identities_dir.join(format!("{}.json", id));
        if !file_path.exists() {
            return Err(anyhow::anyhow!("Identity not found: {}", id));
        }

        let marker = self.identities_dir.join(DEFAULT_MARKER);
        fs::write(&marker, id.as_bytes())
            .await
            .context("Failed to write default identity marker")?;
        Ok(())
    }

    /// Get the Ed25519 signing key for an identity (for DHT publication).
    pub async fn get_signing_key(&self, id: &str) -> Result<SigningKey> {
        self.load_signing_key(id).await
    }

    /// Sign data with a specific identity.
    pub async fn sign(&self, id: &str, data: &[u8]) -> Result<String> {
        let signing_key = self.load_signing_key(id).await?;
        Ok(hex::encode(signing_key.sign(data).to_bytes()))
    }

    /// Verify a signature against a DID's public key.
    /// Works for any valid did:key (not just local identities).
    pub async fn verify(&self, did: &str, data: &[u8], sig_hex: &str) -> Result<bool> {
        // Extract pubkey from did:key directly — no local lookup needed
        let pubkey_bytes = pubkey_bytes_from_did_key(did)?;
        let verifying_key = VerifyingKey::from_bytes(
            pubkey_bytes
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

    /// Create a Nostr keypair for an identity.
    pub async fn create_nostr_key(&self, id: &str) -> Result<String> {
        let file_path = self.identities_dir.join(format!("{}.json", id));
        if !file_path.exists() {
            return Err(anyhow::anyhow!("Identity not found: {}", id));
        }
        let data = fs::read(&file_path)
            .await
            .context("Failed to read identity file")?;
        let mut file: IdentityFile =
            serde_json::from_slice(&data).context("Failed to parse identity file")?;

        if file.nostr_secret_hex.is_some() {
            return Err(anyhow::anyhow!(
                "Nostr key already exists for this identity"
            ));
        }

        let keys = nostr_sdk::Keys::generate();
        let secret_hex = keys.secret_key().display_secret().to_string();
        let pubkey_hex = keys.public_key().to_hex();
        let npub = keys.public_key().to_bech32().unwrap_or_default();

        file.nostr_secret_hex = Some(secret_hex);
        file.nostr_pubkey_hex = Some(pubkey_hex.clone());

        let json = serde_json::to_string_pretty(&file).context("Failed to serialize identity")?;
        fs::write(&file_path, json.as_bytes())
            .await
            .context("Failed to write identity file")?;

        tracing::info!(
            "Created Nostr key for identity {} (npub: {})",
            id,
            &npub[..20.min(npub.len())]
        );
        Ok(pubkey_hex)
    }

    /// Sign a Nostr event (NIP-01) with an identity's Nostr key.
    /// Returns the signature hex string.
    pub async fn nostr_sign(&self, id: &str, event_hash_hex: &str) -> Result<String> {
        let file_path = self.identities_dir.join(format!("{}.json", id));
        if !file_path.exists() {
            return Err(anyhow::anyhow!("Identity not found: {}", id));
        }
        let data = fs::read(&file_path)
            .await
            .context("Failed to read identity file")?;
        let file: IdentityFile =
            serde_json::from_slice(&data).context("Failed to parse identity file")?;

        let secret_hex = file
            .nostr_secret_hex
            .ok_or_else(|| anyhow::anyhow!("No Nostr key for this identity"))?;
        let keys = nostr_sdk::Keys::parse(&secret_hex).context("Invalid Nostr secret key")?;

        let hash_bytes = hex::decode(event_hash_hex).context("Invalid event hash hex")?;
        if hash_bytes.len() != 32 {
            return Err(anyhow::anyhow!("Event hash must be 32 bytes"));
        }

        let message = nostr_sdk::secp256k1::Message::from_digest(
            hash_bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid hash length"))?,
        );
        let sig = keys.sign_schnorr(&message);
        Ok(sig.to_string())
    }

    /// Load the Nostr secret key for an identity, returning the parsed Keys.
    async fn load_nostr_keys(&self, id: &str) -> Result<nostr_sdk::Keys> {
        let file_path = self.identities_dir.join(format!("{}.json", id));
        if !file_path.exists() {
            return Err(anyhow::anyhow!("Identity not found: {}", id));
        }
        let data = fs::read(&file_path)
            .await
            .context("Failed to read identity file")?;
        let file: IdentityFile =
            serde_json::from_slice(&data).context("Failed to parse identity file")?;
        let secret_hex = file
            .nostr_secret_hex
            .ok_or_else(|| anyhow::anyhow!("No Nostr key for this identity"))?;
        nostr_sdk::Keys::parse(&secret_hex).context("Invalid Nostr secret key")
    }

    /// NIP-04 encrypt plaintext for a peer pubkey.
    pub async fn nostr_encrypt_nip04(
        &self,
        id: &str,
        peer_pubkey_hex: &str,
        plaintext: &str,
    ) -> Result<String> {
        let keys = self.load_nostr_keys(id).await?;
        let peer_pk =
            nostr_sdk::PublicKey::from_hex(peer_pubkey_hex).context("Invalid peer pubkey hex")?;
        nostr_sdk::nips::nip04::encrypt(keys.secret_key(), &peer_pk, plaintext)
            .context("NIP-04 encryption failed")
    }

    /// NIP-04 decrypt ciphertext from a peer pubkey.
    pub async fn nostr_decrypt_nip04(
        &self,
        id: &str,
        peer_pubkey_hex: &str,
        ciphertext: &str,
    ) -> Result<String> {
        let keys = self.load_nostr_keys(id).await?;
        let peer_pk =
            nostr_sdk::PublicKey::from_hex(peer_pubkey_hex).context("Invalid peer pubkey hex")?;
        nostr_sdk::nips::nip04::decrypt(keys.secret_key(), &peer_pk, ciphertext)
            .context("NIP-04 decryption failed")
    }

    /// NIP-44 encrypt plaintext for a peer pubkey.
    pub async fn nostr_encrypt_nip44(
        &self,
        id: &str,
        peer_pubkey_hex: &str,
        plaintext: &str,
    ) -> Result<String> {
        let keys = self.load_nostr_keys(id).await?;
        let peer_pk =
            nostr_sdk::PublicKey::from_hex(peer_pubkey_hex).context("Invalid peer pubkey hex")?;
        nostr_sdk::nips::nip44::encrypt(
            keys.secret_key(),
            &peer_pk,
            plaintext,
            nostr_sdk::nips::nip44::Version::V2,
        )
        .context("NIP-44 encryption failed")
    }

    /// NIP-44 decrypt ciphertext from a peer pubkey.
    pub async fn nostr_decrypt_nip44(
        &self,
        id: &str,
        peer_pubkey_hex: &str,
        ciphertext: &str,
    ) -> Result<String> {
        let keys = self.load_nostr_keys(id).await?;
        let peer_pk =
            nostr_sdk::PublicKey::from_hex(peer_pubkey_hex).context("Invalid peer pubkey hex")?;
        nostr_sdk::nips::nip44::decrypt(keys.secret_key(), &peer_pk, ciphertext)
            .context("NIP-44 decryption failed")
    }

    /// Update the profile metadata for an identity.
    pub async fn update_profile(&self, id: &str, profile: IdentityProfile) -> Result<()> {
        let file_path = self.identities_dir.join(format!("{}.json", id));
        if !file_path.exists() {
            return Err(anyhow::anyhow!("Identity not found: {}", id));
        }
        let data = fs::read(&file_path)
            .await
            .context("Failed to read identity file")?;
        let mut file: IdentityFile =
            serde_json::from_slice(&data).context("Failed to parse identity file")?;
        file.profile = Some(profile);
        let json = serde_json::to_string_pretty(&file).context("Failed to serialize identity")?;
        fs::write(&file_path, json.as_bytes())
            .await
            .context("Failed to write identity file")?;
        Ok(())
    }

    /// Publish kind 0 (metadata) event to one or more Nostr relays.
    ///
    /// Connects all relays in parallel, broadcasts the signed event to
    /// every one of them, and reports back the event id plus per-relay
    /// acceptance status. At least one successful relay is required —
    /// if every relay rejects the event, this returns an error so the
    /// UI can surface "publish failed" instead of silently succeeding.
    pub async fn publish_profile(
        &self,
        id: &str,
        relay_urls: &[String],
    ) -> Result<ProfilePublishOutcome> {
        let record = self.get(id).await?;
        let keys = self.load_nostr_keys(id).await?;
        let profile = record.profile.unwrap_or_default();

        if relay_urls.is_empty() {
            anyhow::bail!("No relays configured — add a relay under Manage Relays first");
        }

        // Build kind 0 content JSON (NIP-01 + NIP-24)
        let mut content = serde_json::Map::new();
        content.insert("name".to_string(), serde_json::json!(record.name));
        if let Some(v) = &profile.display_name {
            content.insert("display_name".to_string(), serde_json::json!(v));
        }
        if let Some(v) = &profile.about {
            content.insert("about".to_string(), serde_json::json!(v));
        }
        if let Some(v) = &profile.picture {
            content.insert("picture".to_string(), serde_json::json!(v));
        }
        if let Some(v) = &profile.banner {
            content.insert("banner".to_string(), serde_json::json!(v));
        }
        if let Some(v) = &profile.website {
            content.insert("website".to_string(), serde_json::json!(v));
        }
        if let Some(v) = &profile.nip05 {
            content.insert("nip05".to_string(), serde_json::json!(v));
        }
        if let Some(v) = &profile.lud16 {
            content.insert("lud16".to_string(), serde_json::json!(v));
        }

        let content_str =
            serde_json::to_string(&content).context("Failed to serialize profile content")?;

        let client = nostr_sdk::Client::new(keys);
        for url in relay_urls {
            if let Err(e) = client.add_relay(url).await {
                tracing::warn!(relay = %url, error = %e, "Failed to add relay; continuing");
            }
        }
        // 15s gives each relay a reasonable chance to hand-shake before we
        // fire the publish. nostr-sdk's send_event_builder to "all relays"
        // will only reach relays that have connected by then — some slow
        // relays can miss the first publish but subsequent publishes hit
        // them once the connection has settled.
        if tokio::time::timeout(Duration::from_secs(15), client.connect())
            .await
            .is_err()
        {
            tracing::warn!("Nostr relay connection timed out after 15s, continuing anyway");
        }

        let builder =
            nostr_sdk::prelude::EventBuilder::new(nostr_sdk::prelude::Kind::Metadata, &content_str);
        let output = match client.send_event_builder(builder).await {
            Ok(o) => o,
            Err(e) => {
                client.disconnect().await;
                return Err(anyhow::anyhow!("Publish failed on every relay: {}", e));
            }
        };

        let event_id = output.id().to_hex();
        // `Output` has `success: HashSet<RelayUrl>` + `failed: HashMap<RelayUrl, String>`.
        // Normalise to string comparisons (RelayUrl trims trailing slashes etc.).
        let success_strs: std::collections::HashSet<String> =
            output.success.iter().map(|u| u.to_string()).collect();
        let failed_strs: std::collections::HashMap<String, String> = output
            .failed
            .iter()
            .map(|(u, msg)| (u.to_string(), msg.clone()))
            .collect();
        let mut accepted: Vec<String> = Vec::new();
        let mut rejected: Vec<(String, String)> = Vec::new();
        for url in relay_urls {
            let match_url = success_strs.iter().any(|s| relay_url_matches(s, url));
            if match_url {
                accepted.push(url.clone());
            } else if let Some((_, reason)) =
                failed_strs.iter().find(|(s, _)| relay_url_matches(s, url))
            {
                rejected.push((url.clone(), reason.clone()));
            } else {
                rejected.push((url.clone(), "(no ack from relay)".to_string()));
            }
        }
        client.disconnect().await;

        if accepted.is_empty() {
            anyhow::bail!(
                "Profile published on 0 relays — {} attempted. Failures: {:?}",
                relay_urls.len(),
                rejected
            );
        }

        Ok(ProfilePublishOutcome {
            event_id,
            accepted,
            rejected,
        })
    }

    /// Export all keys for an identity (SENSITIVE — only call after password verification).
    pub async fn export_keys(&self, id: &str) -> Result<serde_json::Value> {
        let file_path = self.identities_dir.join(format!("{}.json", id));
        if !file_path.exists() {
            return Err(anyhow::anyhow!("Identity not found: {}", id));
        }
        let data = fs::read(&file_path)
            .await
            .context("Failed to read identity file")?;
        let file: IdentityFile =
            serde_json::from_slice(&data).context("Failed to parse identity file")?;

        let ed25519_secret_hex = hex::encode(&file.secret_key);

        let nostr_nsec = file.nostr_secret_hex.as_ref().and_then(|h| {
            nostr_sdk::SecretKey::from_hex(h)
                .ok()
                .and_then(|sk| sk.to_bech32().ok())
        });

        Ok(serde_json::json!({
            "ed25519_secret_hex": ed25519_secret_hex,
            "nostr_secret_hex": file.nostr_secret_hex,
            "nostr_nsec": nostr_nsec,
        }))
    }

    // --- internal helpers ---
}

/// Extract Ed25519 pubkey bytes from a did:key string.
/// Format: did:key:z<base58btc(0xed01 + 32-byte-pubkey)>
fn pubkey_bytes_from_did_key(did: &str) -> Result<Vec<u8>> {
    let z_part = did
        .strip_prefix("did:key:z")
        .ok_or_else(|| anyhow::anyhow!("Invalid did:key format: {}", did))?;
    let decoded = bs58::decode(z_part)
        .into_vec()
        .context("Invalid base58 in did:key")?;
    if decoded.len() != 34 || decoded[0] != 0xed || decoded[1] != 0x01 {
        return Err(anyhow::anyhow!("Invalid Ed25519 did:key multicodec prefix"));
    }
    Ok(decoded[2..].to_vec())
}

impl IdentityManager {
    async fn get_default_id(&self) -> Option<String> {
        let marker = self.identities_dir.join(DEFAULT_MARKER);
        fs::read_to_string(&marker)
            .await
            .ok()
            .map(|s| s.trim().to_string())
    }

    async fn load_record(&self, path: &Path) -> Result<IdentityRecord> {
        let data = fs::read(path)
            .await
            .context("Failed to read identity file")?;
        let file: IdentityFile =
            serde_json::from_slice(&data).context("Failed to parse identity file")?;

        // Derive npub (bech32) from hex pubkey if available
        let nostr_npub = file.nostr_pubkey_hex.as_ref().and_then(|hex| {
            nostr_sdk::PublicKey::from_hex(hex)
                .ok()
                .and_then(|pk| pk.to_bech32().ok())
        });

        // Backfill a default avatar for identities created before the
        // default-avatar feature shipped. The synthetic profile lives only
        // in the returned record — we don't rewrite the file on disk,
        // since a later explicit save will persist whatever the user
        // actually chose. Master identities (seed index 0) get the hex
        // node SVG; all other pre-existing identities get the identicon.
        let profile = file.profile.or_else(|| {
            let is_master = file.derivation_index == Some(0);
            Some(IdentityProfile {
                picture: Some(crate::avatar::default_picture(&file.pubkey_hex, is_master)),
                ..Default::default()
            })
        });

        Ok(IdentityRecord {
            id: file.id,
            name: file.name,
            purpose: file.purpose,
            pubkey_hex: file.pubkey_hex,
            did: file.did,
            dht_did: None,
            created_at: file.created_at,
            nostr_pubkey: file.nostr_pubkey_hex,
            nostr_npub,
            profile,
        })
    }

    async fn load_signing_key(&self, id: &str) -> Result<SigningKey> {
        let file_path = self.identities_dir.join(format!("{}.json", id));
        if !file_path.exists() {
            return Err(anyhow::anyhow!("Identity not found: {}", id));
        }
        let data = fs::read(&file_path)
            .await
            .context("Failed to read identity file")?;
        let file: IdentityFile =
            serde_json::from_slice(&data).context("Failed to parse identity file")?;
        let arr: [u8; 32] = file
            .secret_key
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid secret key length"))?;
        Ok(SigningKey::from_bytes(&arr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn import_nostr_preserves_identities_and_rejects_mismatches() {
        let dir = tempdir().unwrap();
        let manager = IdentityManager::new(dir.path()).await.unwrap();
        let original = manager.create("Personal".into(), IdentityPurpose::Personal).await.unwrap();
        let keys = nostr_sdk::Keys::generate();
        let nsec = keys.secret_key().to_bech32().unwrap();
        let npub = keys.public_key().to_bech32().unwrap();
        assert!(manager.import_nostr("Wrong".into(), &nsec, "npub1wrong").await.is_err());
        assert_eq!(manager.list().await.unwrap().0.len(), 1);
        let imported = manager.import_nostr("Website".into(), &nsec, &npub).await.unwrap();
        assert_eq!(imported.nostr_npub.as_deref(), Some(npub.as_str()));
        assert_eq!(manager.import_nostr("Again".into(), &nsec, &npub).await.unwrap().id, imported.id);
        let (records, default) = manager.list().await.unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(default.as_deref(), Some(original.id.as_str()));
        assert_eq!(manager.get(&original.id).await.unwrap().nostr_pubkey, original.nostr_pubkey);
        assert_eq!(manager.export_keys(&imported.id).await.unwrap()["nostr_nsec"], nsec);
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("identities").join(format!("{}.json", imported.id))).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[tokio::test]
    async fn concurrent_nostr_imports_reuse_one_identity_and_sign_correctly() {
        let dir = tempdir().unwrap();
        let manager = IdentityManager::new(dir.path()).await.unwrap();
        let keys = nostr_sdk::Keys::generate();
        let nsec = keys.secret_key().to_bech32().unwrap();
        let npub = keys.public_key().to_bech32().unwrap();
        let (first, second) = tokio::join!(
            manager.import_nostr("Website".into(), &nsec, &npub),
            manager.import_nostr("Website again".into(), &nsec, &npub),
        );
        let first = first.unwrap();
        assert_eq!(first.id, second.unwrap().id);
        let (records, default) = manager.list().await.unwrap();
        assert_eq!(records.len(), 1);
        assert!(default.is_none());
        let hash = [7u8; 32];
        let signature = manager.nostr_sign(&first.id, &hex::encode(hash)).await.unwrap();
        let signature: nostr_sdk::secp256k1::schnorr::Signature = signature.parse().unwrap();
        let pubkey: nostr_sdk::secp256k1::XOnlyPublicKey = keys.public_key().to_hex().parse().unwrap();
        nostr_sdk::secp256k1::Secp256k1::verification_only().verify_schnorr(
            &signature, &nostr_sdk::secp256k1::Message::from_digest(hash), &pubkey,
        ).unwrap();
        let entries = std::fs::read_dir(dir.path().join("identities")).unwrap();
        assert!(entries.map(|entry| entry.unwrap().file_name()).all(|name| !name.to_string_lossy().ends_with(".tmp")));
    }

    #[tokio::test]
    async fn test_create_identity_did_key_format() {
        let dir = tempdir().unwrap();
        let mgr = IdentityManager::new(dir.path()).await.unwrap();
        let record = mgr
            .create("Test".to_string(), IdentityPurpose::Personal)
            .await
            .unwrap();
        assert!(
            record.did.starts_with("did:key:z6Mk"),
            "DID should be did:key:z6Mk..., got {}",
            record.did
        );
        assert!(!record.id.is_empty());
        assert_eq!(record.name, "Test");
    }

    #[tokio::test]
    async fn test_create_nostr_key_npub_format() {
        let dir = tempdir().unwrap();
        let mgr = IdentityManager::new(dir.path()).await.unwrap();
        // `create()` auto-provisions a Nostr key for every identity, so the
        // returned record should already have a valid bech32 npub.
        let record = mgr
            .create("Personal".to_string(), IdentityPurpose::Personal)
            .await
            .unwrap();
        let npub = record.nostr_npub.expect("nostr npub should be populated");
        assert!(
            npub.starts_with("npub1"),
            "npub should start with npub1, got {}",
            npub
        );
    }

    #[tokio::test]
    async fn test_sign_and_verify() {
        let dir = tempdir().unwrap();
        let mgr = IdentityManager::new(dir.path()).await.unwrap();
        let record = mgr
            .create("Signer".to_string(), IdentityPurpose::Personal)
            .await
            .unwrap();
        let data = b"hello archipelago";
        let sig = mgr.sign(&record.id, data).await.unwrap();
        let valid = mgr.verify(&record.did, data, &sig).await.unwrap();
        assert!(valid, "Signature should verify");
    }

    #[tokio::test]
    async fn test_sign_verify_wrong_data() {
        let dir = tempdir().unwrap();
        let mgr = IdentityManager::new(dir.path()).await.unwrap();
        let record = mgr
            .create("Signer".to_string(), IdentityPurpose::Personal)
            .await
            .unwrap();
        let sig = mgr.sign(&record.id, b"correct").await.unwrap();
        let valid = mgr.verify(&record.did, b"wrong", &sig).await.unwrap();
        assert!(!valid, "Signature should not verify with wrong data");
    }

    #[tokio::test]
    async fn test_list_identities() {
        let dir = tempdir().unwrap();
        let mgr = IdentityManager::new(dir.path()).await.unwrap();
        mgr.create("One".to_string(), IdentityPurpose::Personal)
            .await
            .unwrap();
        mgr.create("Two".to_string(), IdentityPurpose::Business)
            .await
            .unwrap();
        let (list, _) = mgr.list().await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_identity() {
        let dir = tempdir().unwrap();
        let mgr = IdentityManager::new(dir.path()).await.unwrap();
        let r1 = mgr
            .create("First".to_string(), IdentityPurpose::Personal)
            .await
            .unwrap();
        let r2 = mgr
            .create("Second".to_string(), IdentityPurpose::Business)
            .await
            .unwrap();
        mgr.delete(&r1.id).await.unwrap();
        let (list, _) = mgr.list().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, r2.id);
    }

    #[tokio::test]
    async fn test_set_and_get_default() {
        let dir = tempdir().unwrap();
        let mgr = IdentityManager::new(dir.path()).await.unwrap();
        let r1 = mgr
            .create("First".to_string(), IdentityPurpose::Personal)
            .await
            .unwrap();
        let r2 = mgr
            .create("Second".to_string(), IdentityPurpose::Business)
            .await
            .unwrap();
        mgr.set_default(&r2.id).await.unwrap();
        let (_, default_id) = mgr.list().await.unwrap();
        assert_eq!(default_id, Some(r2.id.clone()));
        assert_ne!(default_id, Some(r1.id));
    }

    fn record(id: &str, name: &str, pubkey_hex: &str) -> IdentityRecord {
        IdentityRecord {
            id: id.to_string(),
            name: name.to_string(),
            purpose: IdentityPurpose::Personal,
            pubkey_hex: pubkey_hex.to_string(),
            did: String::new(),
            dht_did: None,
            created_at: String::new(),
            nostr_pubkey: None,
            nostr_npub: None,
            profile: None,
        }
    }

    #[test]
    fn is_node_identity_matches_only_the_node_pubkey() {
        let node = "ab".repeat(32);
        let other = "cd".repeat(32);
        assert!(is_node_identity(&record("uuid-1", "Laptop", &node), &node));
        assert!(!is_node_identity(
            &record("uuid-1", "Laptop", &other),
            &node
        ));
        // An unknown node key matches nothing, not the records without a key.
        assert!(!is_node_identity(&record("uuid-1", "Laptop", ""), ""));
        // The id and name rules belong to the picker filter, not to `is_node`.
        assert!(!is_node_identity(
            &record("node-abc", "Node", &other),
            &node
        ));
    }

    #[test]
    fn is_hidden_from_app_signer_mirrors_the_picker_rules() {
        let node = "ab".repeat(32);
        let other = "cd".repeat(32);
        let hidden = |id: &str, name: &str, pk: &str| {
            is_hidden_from_app_signer(&record(id, name, pk), &node)
        };
        // is_node: matched by key alone, whatever the id and name.
        assert!(hidden("uuid-1", "Laptop", &node));
        // node-* id, any case, surrounding whitespace ignored.
        assert!(hidden("node-0123456789abcdef", "Laptop", &other));
        assert!(hidden("  NODE-x ", "Laptop", &other));
        assert!(hidden("Node-x", "Laptop", &other));
        // The name "Node", any case, surrounding whitespace ignored.
        assert!(hidden("uuid-1", "Node", &other));
        assert!(hidden("uuid-1", " nODe\t", &other));
        assert!(hidden("\u{FEFF}NODE-x\u{FEFF}", "Laptop", &other));
        assert!(hidden("uuid-1", "\u{FEFF}Node\u{FEFF}", &other));
        // ECMAScript keeps U+0085; do not add owners that the picker hides,
        // or hide identities that it offers.
        assert!(!hidden("uuid-1", "\u{0085}Node\u{0085}", &other));
        // Near misses stay visible.
        assert!(!hidden("uuid-1", "Laptop", &other));
        assert!(!hidden("my-node-1", "Node 2", &other));
        assert!(!hidden("nodes", "Nodes", &other));
    }

    #[tokio::test]
    async fn app_signable_nostr_pubkeys_mirror_the_identity_picker() {
        let dir = tempdir().unwrap();
        let mgr = IdentityManager::new(dir.path()).await.unwrap();
        let personal = mgr
            .create("Personal".to_string(), IdentityPurpose::Personal)
            .await
            .unwrap();
        let business = mgr
            .create("Business".to_string(), IdentityPurpose::Business)
            .await
            .unwrap();
        // The node identity as mirrored at startup: a `node-` id named
        // "Node", given a Nostr key so only the id and name rules hide it.
        let mirrored_key = SigningKey::generate(&mut OsRng);
        let mirrored = mgr
            .create_from_signing_key("Node".to_string(), IdentityPurpose::Personal, mirrored_key)
            .await
            .unwrap();
        assert!(mirrored.id.starts_with("node-"));
        mgr.create_nostr_key(&mirrored.id).await.unwrap();
        // A user-created identity named "Node" is hidden by the picker too.
        let named_node = mgr
            .create(" node ".to_string(), IdentityPurpose::Anonymous)
            .await
            .unwrap();
        // The node key belongs to an identity with a uuid id and an ordinary
        // name, so only the `is_node` match can hide it.
        let laptop = mgr
            .create("Laptop".to_string(), IdentityPurpose::Personal)
            .await
            .unwrap();
        assert!(!laptop.id.starts_with("node-"));

        let (all, _) = mgr.list().await.unwrap();
        assert!(all
            .iter()
            .any(|r| r.id == mirrored.id && r.nostr_pubkey.is_some()));
        assert!(all.iter().any(|r| r.id == named_node.id));
        assert!(all
            .iter()
            .any(|r| r.id == laptop.id && r.nostr_pubkey.is_some()));

        let mut expected = vec![
            personal.nostr_pubkey.unwrap().to_ascii_lowercase(),
            business.nostr_pubkey.unwrap().to_ascii_lowercase(),
        ];
        expected.sort();
        assert_eq!(
            mgr.app_signable_nostr_pubkeys(&laptop.pubkey_hex)
                .await
                .unwrap(),
            expected.join(",")
        );
        // Without the node key, the same identity is offered like any other.
        let mut with_laptop = expected.clone();
        with_laptop.push(laptop.nostr_pubkey.unwrap().to_ascii_lowercase());
        with_laptop.sort();
        assert_eq!(
            mgr.app_signable_nostr_pubkeys("").await.unwrap(),
            with_laptop.join(",")
        );
    }

    #[tokio::test]
    async fn app_owner_pubkeys_reject_malformed_key_material() {
        let dir = tempdir().unwrap();
        let mgr = IdentityManager::new(dir.path()).await.unwrap();
        let identity = mgr
            .create("Owner".into(), IdentityPurpose::Personal)
            .await
            .unwrap();
        let path = mgr.identities_dir.join(format!("{}.json", identity.id));
        let original = fs::read(&path).await.unwrap();
        for invalid in ["", "not-a-key", "owner,another-owner", "\nINJECTED=true"] {
            let mut data: serde_json::Value = serde_json::from_slice(&original).unwrap();
            data["nostr_pubkey_hex"] = serde_json::json!(invalid);
            fs::write(&path, serde_json::to_vec(&data).unwrap())
                .await
                .unwrap();
            assert!(mgr.app_signable_nostr_pubkeys("").await.is_err());
        }
    }

    #[tokio::test]
    async fn app_signable_nostr_pubkeys_is_empty_with_only_the_node_identity() {
        let dir = tempdir().unwrap();
        let mgr = IdentityManager::new(dir.path()).await.unwrap();
        let node_key = SigningKey::generate(&mut OsRng);
        let node_pubkey_hex = hex::encode(node_key.verifying_key().as_bytes());
        mgr.create_from_signing_key("Node".to_string(), IdentityPurpose::Personal, node_key)
            .await
            .unwrap();
        assert_eq!(
            mgr.app_signable_nostr_pubkeys(&node_pubkey_hex)
                .await
                .unwrap(),
            ""
        );
    }

    #[tokio::test]
    async fn test_delete_default_shifts() {
        let dir = tempdir().unwrap();
        let mgr = IdentityManager::new(dir.path()).await.unwrap();
        let r1 = mgr
            .create("First".to_string(), IdentityPurpose::Personal)
            .await
            .unwrap();
        mgr.create("Second".to_string(), IdentityPurpose::Business)
            .await
            .unwrap();
        mgr.set_default(&r1.id).await.unwrap();
        mgr.delete(&r1.id).await.unwrap();
        let (list, _) = mgr.list().await.unwrap();
        assert_eq!(list.len(), 1);
    }
}
