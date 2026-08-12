//! Decentralized app marketplace: discover, verify, and publish app manifests
//! via Nostr relays. Uses NIP-78 (kind 30078) with d-tag "archipelago-app:<id>".
//!
//! See docs/marketplace-protocol.md for the full protocol specification.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::fs;
use tracing::{debug, info, warn};

const MARKETPLACE_DIR: &str = "marketplace";
const CACHE_FILE: &str = "manifests.json";
const PUBLISHED_DIR: &str = "published";
const ARCHIPELAGO_KIND: u64 = 30078;
const D_TAG_PREFIX: &str = "archipelago-app:";
const MARKETPLACE_TAG: &str = "archipelago-marketplace";

/// Categories for marketplace apps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AppCategory {
    Money,
    Commerce,
    Data,
    Networking,
    Home,
    Community,
    Other,
}

impl std::fmt::Display for AppCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Money => write!(f, "money"),
            Self::Commerce => write!(f, "commerce"),
            Self::Data => write!(f, "data"),
            Self::Networking => write!(f, "networking"),
            Self::Home => write!(f, "home"),
            Self::Community => write!(f, "community"),
            Self::Other => write!(f, "other"),
        }
    }
}

/// Author information in a marketplace manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestAuthor {
    pub name: String,
    pub did: String,
    #[serde(default)]
    pub nostr_pubkey: String,
}

/// Container configuration in a marketplace manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestContainer {
    pub image: String,
    #[serde(default)]
    pub ports: Vec<PortMapping>,
    #[serde(default)]
    pub volumes: Vec<VolumeMapping>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_true")]
    pub readonly_root: bool,
    #[serde(default = "default_true")]
    pub no_new_privileges: bool,
    #[serde(default = "default_uid")]
    pub run_as_user: u32,
}

fn default_true() -> bool {
    true
}

fn default_uid() -> u32 {
    1000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub container: u16,
    pub host: u16,
    #[serde(default = "default_tcp")]
    pub protocol: String,
}

fn default_tcp() -> String {
    "tcp".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMapping {
    pub name: String,
    pub path: String,
}

/// App manifest signatures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestSignatures {
    pub manifest_hash: String,
    pub did_signature: String,
}

/// A marketplace app manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppManifest {
    pub app_id: String,
    pub name: String,
    pub version: String,
    pub description: ManifestDescription,
    pub author: ManifestAuthor,
    pub container: ManifestContainer,
    pub category: AppCategory,
    #[serde(default)]
    pub icon_url: String,
    #[serde(default)]
    pub repo_url: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub min_archipelago_version: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub signatures: Option<ManifestSignatures>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ManifestDescription {
    Simple(String),
    Detailed { short: String, long: String },
}

impl ManifestDescription {
    /// Return the short description regardless of variant.
    #[allow(dead_code)]
    pub fn short(&self) -> &str {
        match self {
            Self::Simple(s) => s,
            Self::Detailed { short, .. } => short,
        }
    }
}

/// A discovered marketplace app with trust scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredApp {
    pub manifest: AppManifest,
    pub trust_score: u32,
    pub trust_tier: String,
    pub relay_count: u32,
    pub first_seen: String,
    pub nostr_pubkey: String,
    /// Outcome of checking the manifest's DID signature. `#[serde(default)]`
    /// so a cache written before this field existed still loads (as `missing`,
    /// which is the honest answer for an entry we never verified).
    #[serde(default)]
    pub signature: SignatureStatus,
}

// ─── DID signature layer ────────────────────────────────────────────────
//
// A marketplace manifest travels inside a Nostr event, so it already carries a
// NIP-01 Schnorr signature proving *the publishing relay key* sent it. That
// says nothing about the `author.did` the manifest claims. This layer closes
// that gap: the author signs a digest of their own manifest with the Ed25519
// key their did:key encodes, and every consumer re-derives the digest and
// checks it.
//
// Until this existed, `signatures.manifest_hash` / `signatures.did_signature`
// were struct fields nothing read, and the trust score awarded 30 points for
// `did.starts_with("did:")` — i.e. for typing a string.

/// Outcome of checking a manifest's `signatures` block.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum SignatureStatus {
    /// `manifest_hash` matches the content and `did_signature` verifies against
    /// the key in `author.did`.
    Valid,
    /// No `signatures` block. Not an attack — an unsigned publisher — but it
    /// earns none of the identity-derived trust.
    #[default]
    Missing,
    /// A `signatures` block is present and wrong: corrupt, tampered with, or
    /// signed by a key other than the one `author.did` names.
    Invalid { reason: String },
}

impl SignatureStatus {
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid)
    }
}

/// Recursively rebuild a JSON value with every object's keys in lexicographic
/// order, so the signed preimage is byte-stable.
///
/// This is not belt-and-braces. `ManifestContainer::env` is a `HashMap`, whose
/// iteration order is randomised per process; and `serde_json::Map` is only a
/// sorted `BTreeMap` while the `preserve_order` feature is off — a feature any
/// crate anywhere in the dependency graph can turn on for everyone through
/// Cargo feature unification. Either way the digest would start changing
/// between runs and every signature would break. Sorting here makes the
/// preimage independent of both.
fn canonicalize(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut pairs: Vec<(String, serde_json::Value)> = map.into_iter().collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            let mut out = serde_json::Map::new();
            for (k, v) in pairs {
                out.insert(k, canonicalize(v));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(canonicalize).collect())
        }
        other => other,
    }
}

/// The exact bytes a manifest signature covers: the manifest as canonical JSON
/// (sorted keys, no whitespace) with the `signatures` block itself omitted —
/// a signature cannot cover the field that holds it.
pub fn canonical_signing_bytes(manifest: &AppManifest) -> Result<Vec<u8>> {
    let mut unsigned = manifest.clone();
    unsigned.signatures = None;
    let value = serde_json::to_value(&unsigned).context("Serializing manifest for signing")?;
    serde_json::to_vec(&canonicalize(value)).context("Encoding canonical manifest JSON")
}

/// SHA-256 over [`canonical_signing_bytes`]. This digest is what gets signed,
/// and what `signatures.manifest_hash` records as `sha256:<hex>`.
pub fn manifest_digest(manifest: &AppManifest) -> Result<[u8; 32]> {
    use sha2::{Digest, Sha256};
    Ok(Sha256::digest(canonical_signing_bytes(manifest)?).into())
}

/// Sign `manifest` in place with an Ed25519 key, filling in `signatures`.
///
/// The caller must ensure `author.did` is the did:key for `signing_key` —
/// [`publish`] enforces that. Signing with a mismatched key produces a manifest
/// that every verifier rejects.
pub fn sign_manifest(
    manifest: &mut AppManifest,
    signing_key: &ed25519_dalek::SigningKey,
) -> Result<()> {
    use ed25519_dalek::Signer;
    // Clear first so a re-sign never covers a previous signature.
    manifest.signatures = None;
    let digest = manifest_digest(manifest)?;
    let signature = signing_key.sign(&digest);
    manifest.signatures = Some(ManifestSignatures {
        manifest_hash: format!("sha256:{}", hex::encode(digest)),
        did_signature: base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            signature.to_bytes(),
        ),
    });
    Ok(())
}

/// Verify a manifest's `signatures` block against its own content and the
/// Ed25519 key encoded in `author.did`.
///
/// Never returns an error: a manifest arriving off a public relay is untrusted
/// input, and every way it can be wrong is a verdict rather than an exception.
pub fn verify_manifest_signature(manifest: &AppManifest) -> SignatureStatus {
    use ed25519_dalek::Verifier;

    let invalid = |reason: &str| SignatureStatus::Invalid {
        reason: reason.to_string(),
    };

    let sigs = match &manifest.signatures {
        Some(s) => s,
        None => return SignatureStatus::Missing,
    };

    let digest = match manifest_digest(manifest) {
        Ok(d) => d,
        Err(_) => return invalid("manifest could not be canonicalized"),
    };

    // 1. Content integrity: does the recorded hash describe this manifest?
    let claimed_hex = match sigs.manifest_hash.strip_prefix("sha256:") {
        Some(h) => h,
        None => return invalid("manifest_hash is not in sha256:<hex> form"),
    };
    match hex::decode(claimed_hex) {
        Ok(claimed) if claimed == digest => {}
        Ok(_) => return invalid("manifest_hash does not match the manifest content"),
        Err(_) => return invalid("manifest_hash is not valid hex"),
    }

    // 2. Identity: resolve the DID to a key.
    let pubkey_bytes = match crate::identity::pubkey_bytes_from_did_key(&manifest.author.did) {
        Ok(p) => p,
        Err(e) => {
            return SignatureStatus::Invalid {
                reason: format!("author.did is not a resolvable Ed25519 did:key: {e}"),
            }
        }
    };
    let verifying_key = match ed25519_dalek::VerifyingKey::from_bytes(&pubkey_bytes) {
        Ok(k) => k,
        Err(_) => return invalid("author.did does not encode a valid Ed25519 key"),
    };

    // 3. Authenticity: did that key sign this digest?
    let sig_bytes = match base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &sigs.did_signature,
    ) {
        Ok(b) => b,
        Err(_) => return invalid("did_signature is not valid base64"),
    };
    let sig_array: [u8; 64] = match sig_bytes.as_slice().try_into() {
        Ok(a) => a,
        Err(_) => return invalid("did_signature is not a 64-byte Ed25519 signature"),
    };

    match verifying_key.verify(&digest, &ed25519_dalek::Signature::from_bytes(&sig_array)) {
        Ok(()) => SignatureStatus::Valid,
        Err(_) => invalid("did_signature does not verify against author.did"),
    }
}

/// Cache of discovered marketplace apps.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarketplaceCache {
    pub apps: Vec<DiscoveredApp>,
    pub last_updated: String,
}

/// Ensure marketplace directories exist.
async fn ensure_dirs(data_dir: &Path) -> Result<()> {
    let market_dir = data_dir.join(MARKETPLACE_DIR);
    fs::create_dir_all(market_dir.join("cache"))
        .await
        .context("Creating marketplace cache dir")?;
    fs::create_dir_all(market_dir.join(PUBLISHED_DIR))
        .await
        .context("Creating marketplace published dir")?;
    Ok(())
}

/// Load cached marketplace data.
pub async fn load_cache(data_dir: &Path) -> Result<MarketplaceCache> {
    let path = data_dir
        .join(MARKETPLACE_DIR)
        .join("cache")
        .join(CACHE_FILE);
    if !path.exists() {
        return Ok(MarketplaceCache::default());
    }
    let data = fs::read_to_string(&path)
        .await
        .context("Reading marketplace cache")?;
    serde_json::from_str(&data).context("Parsing marketplace cache")
}

/// Save marketplace cache.
pub async fn save_cache(data_dir: &Path, cache: &MarketplaceCache) -> Result<()> {
    ensure_dirs(data_dir).await?;
    let path = data_dir
        .join(MARKETPLACE_DIR)
        .join("cache")
        .join(CACHE_FILE);
    let data = serde_json::to_string_pretty(cache)?;
    fs::write(&path, data)
        .await
        .context("Writing marketplace cache")?;
    Ok(())
}

/// Validate a manifest meets security requirements.
pub fn validate_manifest(manifest: &AppManifest) -> Vec<String> {
    let mut issues = Vec::new();

    // Required fields
    if manifest.app_id.is_empty() {
        issues.push("Missing app_id".into());
    }
    if manifest.name.is_empty() {
        issues.push("Missing name".into());
    }
    if manifest.version.is_empty() {
        issues.push("Missing version".into());
    }
    if manifest.container.image.is_empty() {
        issues.push("Missing container image".into());
    }

    // Security checks
    if manifest.container.image.ends_with(":latest") {
        issues.push("Container image uses :latest tag (must pin specific version)".into());
    }
    if !manifest.container.readonly_root {
        issues.push("readonly_root is false (should be true)".into());
    }
    if !manifest.container.no_new_privileges {
        issues.push("no_new_privileges is false (should be true)".into());
    }
    if manifest.container.run_as_user < 1000 {
        issues.push(format!(
            "run_as_user is {} (must be >= 1000)",
            manifest.container.run_as_user
        ));
    }

    // app_id format
    if !manifest
        .app_id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit())
    {
        issues.push("app_id must be lowercase kebab-case".into());
    }

    issues
}

/// Calculate trust score for a discovered app manifest.
/// `signature` gates both identity-derived factors. Pass the result of
/// [`verify_manifest_signature`].
pub fn calculate_trust_score(
    manifest: &AppManifest,
    relay_count: u32,
    federated_dids: &[String],
    signature: &SignatureStatus,
) -> (u32, String) {
    let mut score: u32 = 0;

    // Identity (30 points) — the author proved control of the key their
    // did:key names. This used to be `did.starts_with("did:")`, i.e. a string
    // test any publisher could pass by typing one, which made the whole
    // "Verified" tier meaningless.
    let identity_proven = signature.is_valid();
    if identity_proven {
        score += 30;
    }

    // Relay consensus (20 points) — found on multiple relays
    score += match relay_count {
        0..=1 => 5,
        2..=3 => 12,
        _ => 20,
    };

    // Federation trust (20 points) — developer DID in federation.
    //
    // Also gated on the signature: an unverified `author.did` is just a string
    // the publisher chose, so without this an attacker could copy a DID the
    // user federates with and collect 20 points for impersonating them —
    // exactly the peer they trust most.
    if identity_proven && federated_dids.contains(&manifest.author.did) {
        score += 20;
    }

    // Version history (15 points) — has a proper semver version
    if manifest.version.split('.').count() == 3 {
        score += 10;
    }
    if !manifest.repo_url.is_empty() {
        score += 5;
    }

    // Security compliance (15 points)
    let issues = validate_manifest(manifest);
    if issues.is_empty() {
        score += 15;
    } else if issues.len() <= 2 {
        score += 5;
    }

    let tier = match score {
        80..=100 => "verified",
        50..=79 => "community",
        20..=49 => "unverified",
        _ => "untrusted",
    };

    (score, tier.to_string())
}

/// Discover app manifests from Nostr relays.
///
/// Queries configured relays for kind 30078 events with the marketplace tag,
/// parses manifests, validates, scores, and returns sorted by trust score.
pub async fn discover(
    data_dir: &Path,
    relays: &[String],
    tor_proxy: Option<&str>,
    federated_dids: &[String],
) -> Result<Vec<DiscoveredApp>> {
    if relays.is_empty() {
        return Ok(Vec::new());
    }

    info!(
        relay_count = relays.len(),
        "Discovering marketplace apps from Nostr relays"
    );

    let anon_keys = nostr_sdk::prelude::Keys::generate();
    let client = build_nostr_client(anon_keys, tor_proxy)?;
    for url in relays {
        let _ = client.add_relay(url).await;
    }
    if tokio::time::timeout(Duration::from_secs(10), client.connect())
        .await
        .is_err()
    {
        tracing::warn!("Nostr relay connection timed out after 10s, continuing anyway");
    }

    let filter = nostr_sdk::prelude::Filter::new()
        .kind(nostr_sdk::prelude::Kind::Custom(ARCHIPELAGO_KIND as u16))
        .hashtag(MARKETPLACE_TAG)
        .limit(200);

    let events = client
        .fetch_events(filter, std::time::Duration::from_secs(20))
        .await
        .map(|e| e.to_vec())
        .unwrap_or_default();
    client.disconnect().await;

    debug!(
        event_count = events.len(),
        "Fetched marketplace events from relays"
    );

    // Deduplicate by app_id, keeping the latest version
    let mut app_map: HashMap<String, (DiscoveredApp, u32)> = HashMap::new();

    for event in events {
        // Parse manifest from event content
        let manifest: AppManifest = match serde_json::from_str(&event.content) {
            Ok(m) => m,
            Err(e) => {
                debug!(err = %e, "Skipping invalid marketplace manifest");
                continue;
            }
        };

        // Validate
        let issues = validate_manifest(&manifest);
        if issues
            .iter()
            .any(|i| i.contains("Missing app_id") || i.contains("Missing container image"))
        {
            debug!(issues = ?issues, "Skipping manifest with critical issues");
            continue;
        }

        // Check the DID signature before the manifest is allowed anywhere near
        // the cache. A *wrong* signature is not a low-trust manifest, it is a
        // corrupt or forged one — drop it rather than listing it at reduced
        // score, so it can never be installed. A *missing* signature is
        // different: an unsigned publisher is legitimate, just unproven, and
        // scores zero on the identity factors.
        let signature = verify_manifest_signature(&manifest);
        if let SignatureStatus::Invalid { reason } = &signature {
            warn!(
                app_id = %manifest.app_id,
                author_did = %manifest.author.did,
                nostr_pubkey = %event.pubkey.to_hex(),
                reason = %reason,
                "Rejecting marketplace manifest with a bad DID signature"
            );
            continue;
        }

        let app_id = manifest.app_id.clone();
        let entry = app_map.entry(app_id).or_insert_with(|| {
            let (trust_score, trust_tier) =
                calculate_trust_score(&manifest, 1, federated_dids, &signature);
            (
                DiscoveredApp {
                    manifest,
                    trust_score,
                    trust_tier,
                    relay_count: 0,
                    first_seen: event.created_at.to_human_datetime(),
                    nostr_pubkey: event.pubkey.to_hex(),
                    signature,
                },
                0,
            )
        });
        entry.1 += 1;
    }

    // Update relay counts and recalculate scores
    let mut apps: Vec<DiscoveredApp> = app_map
        .into_values()
        .map(|(mut app, relay_count)| {
            app.relay_count = relay_count;
            let (score, tier) =
                calculate_trust_score(&app.manifest, relay_count, federated_dids, &app.signature);
            app.trust_score = score;
            app.trust_tier = tier;
            app
        })
        .collect();

    // Sort by trust score descending
    apps.sort_by(|a, b| b.trust_score.cmp(&a.trust_score));

    // Cache results
    let cache = MarketplaceCache {
        apps: apps.clone(),
        last_updated: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(e) = save_cache(data_dir, &cache).await {
        warn!(err = %e, "Failed to save marketplace cache");
    }

    info!(app_count = apps.len(), "Marketplace discovery complete");
    Ok(apps)
}

/// Publish an app manifest to Nostr relays.
pub async fn publish(
    data_dir: &Path,
    manifest: &AppManifest,
    relays: &[String],
    tor_proxy: Option<&str>,
) -> Result<String> {
    if relays.is_empty() {
        anyhow::bail!("No relays configured for publishing");
    }

    let issues = validate_manifest(manifest);
    if !issues.is_empty() {
        anyhow::bail!("Manifest validation failed: {}", issues.join(", "));
    }

    let identity_dir = data_dir.join("identity");

    // Sign with the node's Ed25519 identity key — the same key its did:key
    // encodes — so consumers can verify authorship independently of whichever
    // Nostr key happens to relay the event.
    let identity = crate::identity::NodeIdentity::load_or_create(&identity_dir)
        .await
        .context("Loading node identity to sign the manifest")?;
    let our_did = identity.did_key().context("Deriving this node's did:key")?;

    let mut manifest = manifest.clone();
    if manifest.author.did.is_empty() {
        manifest.author.did = our_did.clone();
    } else if manifest.author.did != our_did {
        // We can only sign as ourselves. Publishing under someone else's DID
        // would produce a manifest every verifier rejects, so fail loudly here
        // instead of broadcasting garbage to every relay.
        anyhow::bail!(
            "Cannot publish as author.did {} — this node can only sign as {}",
            manifest.author.did,
            our_did
        );
    }
    sign_manifest(&mut manifest, identity.signing_key()).context("Signing manifest")?;
    debug_assert!(verify_manifest_signature(&manifest).is_valid());
    let manifest = &manifest;

    let keys = load_or_create_keys(&identity_dir).await?;
    let client = build_nostr_client(keys, tor_proxy)?;

    let content = serde_json::to_string(manifest).context("Serializing manifest")?;
    let d_tag = format!("{}{}", D_TAG_PREFIX, manifest.app_id);

    for url in relays {
        let _ = client.add_relay(url).await;
    }
    if tokio::time::timeout(Duration::from_secs(10), client.connect())
        .await
        .is_err()
    {
        tracing::warn!("Nostr relay connection timed out after 10s, continuing anyway");
    }

    let builder = nostr_sdk::prelude::EventBuilder::new(
        nostr_sdk::prelude::Kind::Custom(ARCHIPELAGO_KIND as u16),
        &content,
    )
    .tag(nostr_sdk::prelude::Tag::identifier(&d_tag))
    .tag(nostr_sdk::prelude::Tag::hashtag(MARKETPLACE_TAG))
    .tag(nostr_sdk::prelude::Tag::hashtag(format!(
        "category:{}",
        manifest.category
    )))
    .tag(nostr_sdk::prelude::Tag::custom(
        nostr_sdk::prelude::TagKind::custom("version"),
        [&manifest.version],
    ))
    .tag(nostr_sdk::prelude::Tag::custom(
        nostr_sdk::prelude::TagKind::custom("image"),
        [&manifest.container.image],
    ));

    let output = client.send_event_builder(builder).await?;
    client.disconnect().await;

    // Save to published directory
    ensure_dirs(data_dir).await?;
    let pub_path = data_dir
        .join(MARKETPLACE_DIR)
        .join(PUBLISHED_DIR)
        .join(format!("{}.json", manifest.app_id));
    let pub_data = serde_json::to_string_pretty(manifest)?;
    fs::write(&pub_path, pub_data)
        .await
        .context("Saving published manifest")?;

    info!(app_id = %manifest.app_id, "Published app manifest to {} relays", relays.len());
    Ok(output.id().to_hex())
}

/// List manifests published by this node.
pub async fn list_published(data_dir: &Path) -> Result<Vec<AppManifest>> {
    let pub_dir = data_dir.join(MARKETPLACE_DIR).join(PUBLISHED_DIR);
    if !pub_dir.exists() {
        return Ok(Vec::new());
    }

    let mut manifests = Vec::new();
    let mut entries = fs::read_dir(&pub_dir)
        .await
        .context("Reading published dir")?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            let data = fs::read_to_string(&path).await?;
            if let Ok(manifest) = serde_json::from_str::<AppManifest>(&data) {
                manifests.push(manifest);
            }
        }
    }

    Ok(manifests)
}

// Re-use nostr client builder pattern from nostr_discovery
fn build_nostr_client(
    keys: nostr_sdk::prelude::Keys,
    tor_proxy: Option<&str>,
) -> Result<nostr_sdk::prelude::Client> {
    use nostr_sdk::prelude::*;
    let client = if let Some(proxy_str) = tor_proxy {
        let addr: std::net::SocketAddr = proxy_str
            .trim()
            .parse()
            .ok()
            .ok_or_else(|| anyhow::anyhow!("Invalid Tor proxy: {}", proxy_str))?;
        let connection = Connection::new().proxy(addr).target(ConnectionTarget::All);
        let opts = ClientOptions::new().connection(connection);
        Client::builder().signer(keys).opts(opts).build()
    } else {
        Client::new(keys)
    };
    Ok(client)
}

/// Load or create Nostr keys for marketplace publishing.
async fn load_or_create_keys(identity_dir: &Path) -> Result<nostr_sdk::prelude::Keys> {
    use nostr_sdk::prelude::Keys;

    let secret_path = identity_dir.join("nostr_secret");
    if secret_path.exists() {
        let hex_secret = fs::read_to_string(&secret_path)
            .await
            .context("Reading Nostr secret")?;
        Keys::parse(hex_secret.trim()).context("Invalid Nostr secret")
    } else {
        let keys = Keys::generate();
        fs::create_dir_all(identity_dir).await?;
        fs::write(&secret_path, keys.secret_key().to_secret_hex()).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&secret_path, std::fs::Permissions::from_mode(0o600))
                .await?;
        }
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> AppManifest {
        AppManifest {
            app_id: "test-app".into(),
            name: "Test App".into(),
            version: "1.0.0".into(),
            description: ManifestDescription::Detailed {
                short: "A test app".into(),
                long: "A longer description of the test app".into(),
            },
            author: ManifestAuthor {
                name: "Test Dev".into(),
                did: "did:key:z6MkTest123".into(),
                nostr_pubkey: String::new(),
            },
            container: ManifestContainer {
                image: "docker.io/test/app:1.0.0".into(),
                ports: vec![PortMapping {
                    container: 8080,
                    host: 8180,
                    protocol: "tcp".into(),
                }],
                volumes: vec![],
                env: HashMap::new(),
                capabilities: vec![],
                readonly_root: true,
                no_new_privileges: true,
                run_as_user: 1000,
            },
            category: AppCategory::Other,
            icon_url: String::new(),
            repo_url: "https://github.com/test/app".into(),
            license: "MIT".into(),
            min_archipelago_version: "0.1.0".into(),
            dependencies: vec![],
            signatures: None,
        }
    }

    #[test]
    fn test_validate_valid_manifest() {
        let manifest = sample_manifest();
        let issues = validate_manifest(&manifest);
        assert!(issues.is_empty(), "Expected no issues, got: {:?}", issues);
    }

    #[test]
    fn test_validate_latest_tag() {
        let mut manifest = sample_manifest();
        manifest.container.image = "docker.io/test/app:latest".into();
        let issues = validate_manifest(&manifest);
        assert!(issues.iter().any(|i| i.contains("latest")));
    }

    #[test]
    fn test_validate_root_user() {
        let mut manifest = sample_manifest();
        manifest.container.run_as_user = 0;
        let issues = validate_manifest(&manifest);
        assert!(issues.iter().any(|i| i.contains("run_as_user")));
    }

    #[test]
    fn test_validate_missing_fields() {
        let mut manifest = sample_manifest();
        manifest.app_id = String::new();
        manifest.container.image = String::new();
        let issues = validate_manifest(&manifest);
        assert!(issues.len() >= 2);
    }

    /// A real Ed25519 keypair, its did:key, and a manifest signed by it.
    fn signed_manifest() -> (ed25519_dalek::SigningKey, String, AppManifest) {
        let key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let did =
            crate::identity::did_key_from_pubkey_hex(&hex::encode(key.verifying_key().as_bytes()))
                .unwrap();
        let mut manifest = sample_manifest();
        manifest.author.did = did.clone();
        sign_manifest(&mut manifest, &key).unwrap();
        (key, did, manifest)
    }

    #[test]
    fn verify_accepts_a_properly_signed_manifest() {
        let (_key, _did, manifest) = signed_manifest();
        assert_eq!(verify_manifest_signature(&manifest), SignatureStatus::Valid);
    }

    #[test]
    fn verify_reports_missing_when_there_is_no_signature_block() {
        let manifest = sample_manifest();
        assert_eq!(
            verify_manifest_signature(&manifest),
            SignatureStatus::Missing
        );
    }

    /// Tampering with any covered field must break the hash check.
    #[test]
    fn verify_rejects_a_tampered_field() {
        let (_key, _did, mut manifest) = signed_manifest();
        manifest.container.image = "docker.io/evil/backdoor:1.0.0".into();
        match verify_manifest_signature(&manifest) {
            SignatureStatus::Invalid { reason } => {
                assert!(reason.contains("does not match"), "reason: {reason}")
            }
            other => panic!("tampered manifest accepted: {other:?}"),
        }
    }

    /// The interesting attack: tamper with the content AND recompute
    /// `manifest_hash` so the integrity check passes. Without the key the
    /// signature can't be regenerated, so this must still fail.
    #[test]
    fn verify_rejects_tampering_that_also_rewrites_the_hash() {
        let (_key, _did, mut manifest) = signed_manifest();
        let stolen_signature = manifest.signatures.clone().unwrap().did_signature;

        manifest.container.image = "docker.io/evil/backdoor:1.0.0".into();
        let new_digest = manifest_digest(&manifest).unwrap();
        manifest.signatures = Some(ManifestSignatures {
            manifest_hash: format!("sha256:{}", hex::encode(new_digest)),
            did_signature: stolen_signature,
        });

        match verify_manifest_signature(&manifest) {
            SignatureStatus::Invalid { reason } => {
                assert!(reason.contains("does not verify"), "reason: {reason}")
            }
            other => panic!("forged manifest accepted: {other:?}"),
        }
    }

    /// Signing with one key while claiming another's DID must fail — this is
    /// the impersonation case the whole layer exists to stop.
    #[test]
    fn verify_rejects_a_signature_from_a_key_other_than_the_claimed_did() {
        let (_key_a, _did_a, mut manifest) = signed_manifest();
        let (_key_b, did_b, _) = signed_manifest();
        manifest.author.did = did_b;
        assert!(
            !verify_manifest_signature(&manifest).is_valid(),
            "a manifest signed by A must not verify as B"
        );
    }

    #[test]
    fn verify_rejects_unusable_author_dids_and_malformed_signatures() {
        let (key, _did, base) = signed_manifest();

        // Not a did:key at all.
        let mut m = base.clone();
        m.author.did = "did:web:example.com".into();
        assert!(!verify_manifest_signature(&m).is_valid());

        // did:key shaped but not decodable — the old code scored this 30/30.
        let mut m = base.clone();
        m.author.did = "did:key:z6MkTest123".into();
        assert!(!verify_manifest_signature(&m).is_valid());

        // Signature that isn't base64.
        let mut m = base.clone();
        m.signatures.as_mut().unwrap().did_signature = "not!base64!".into();
        assert!(!verify_manifest_signature(&m).is_valid());

        // Base64 of the wrong length.
        let mut m = base.clone();
        m.signatures.as_mut().unwrap().did_signature =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [0u8; 16]);
        assert!(!verify_manifest_signature(&m).is_valid());

        // Hash in the wrong form.
        let mut m = base.clone();
        m.signatures.as_mut().unwrap().manifest_hash = "deadbeef".into();
        assert!(!verify_manifest_signature(&m).is_valid());

        // Sanity: the untouched manifest still verifies, so the cases above
        // failed for their own reasons and not because the fixture is broken.
        let mut ok = base;
        sign_manifest(&mut ok, &key).unwrap();
        assert!(verify_manifest_signature(&ok).is_valid());
    }

    /// `env` is a HashMap, whose iteration order is randomised per process. If
    /// the preimage were not canonicalised, the same manifest would hash
    /// differently between runs and signatures would fail at random.
    #[test]
    fn digest_is_stable_regardless_of_map_insertion_order() {
        let mut a = sample_manifest();
        a.container.env.insert("ZEBRA".into(), "1".into());
        a.container.env.insert("ALPHA".into(), "2".into());
        a.container.env.insert("MIDDLE".into(), "3".into());

        let mut b = sample_manifest();
        b.container.env.insert("MIDDLE".into(), "3".into());
        b.container.env.insert("ALPHA".into(), "2".into());
        b.container.env.insert("ZEBRA".into(), "1".into());

        assert_eq!(manifest_digest(&a).unwrap(), manifest_digest(&b).unwrap());
        assert_eq!(
            canonical_signing_bytes(&a).unwrap(),
            canonical_signing_bytes(&b).unwrap()
        );
    }

    /// The signature must not cover the field that holds it, or re-signing an
    /// already-signed manifest would produce a different digest each time.
    #[test]
    fn digest_ignores_the_signatures_block() {
        let (key, _did, signed) = signed_manifest();
        let mut unsigned = signed.clone();
        unsigned.signatures = None;
        assert_eq!(
            manifest_digest(&signed).unwrap(),
            manifest_digest(&unsigned).unwrap()
        );

        // Re-signing is stable (Ed25519 is deterministic).
        let mut resigned = signed.clone();
        sign_manifest(&mut resigned, &key).unwrap();
        assert_eq!(
            resigned.signatures.unwrap().did_signature,
            signed.signatures.unwrap().did_signature
        );
    }

    #[test]
    fn test_trust_score_full() {
        let (_key, did, manifest) = signed_manifest();
        let signature = verify_manifest_signature(&manifest);
        let (score, tier) = calculate_trust_score(&manifest, 3, &[did], &signature);
        // identity (30) + relays 2-3 (12) + federation (20) + semver (10) + repo (5) + security clean (15) = 92
        assert!(score >= 80, "Expected verified, got score={score}");
        assert_eq!(tier, "verified");
    }

    #[test]
    fn test_trust_score_no_federation() {
        let (_key, _did, manifest) = signed_manifest();
        let signature = verify_manifest_signature(&manifest);
        let (score, tier) = calculate_trust_score(&manifest, 1, &[], &signature);
        // identity (30) + 1 relay (5) + semver (10) + repo (5) + security (15) = 65
        assert_eq!(tier, "community");
        assert!((50..80).contains(&score));
    }

    #[test]
    fn test_trust_score_untrusted() {
        let mut manifest = sample_manifest();
        manifest.author.did = String::new();
        manifest.repo_url = String::new();
        manifest.version = "1".into();
        manifest.container.readonly_root = false;
        let (score, _tier) = calculate_trust_score(&manifest, 1, &[], &SignatureStatus::Missing);
        assert!(score < 50, "Expected low score, got {score}");
    }

    /// The regression that made "Verified" meaningless: an unsigned manifest
    /// with a plausible-looking DID string used to score 30/30 on identity and
    /// land at 65 — "Community" — on nothing but a `starts_with("did:")`.
    #[test]
    fn an_unsigned_manifest_earns_no_identity_points() {
        let manifest = sample_manifest(); // author.did is "did:key:z6MkTest123"
        let signature = verify_manifest_signature(&manifest);
        assert_eq!(signature, SignatureStatus::Missing);

        let (score, tier) = calculate_trust_score(&manifest, 1, &[], &signature);
        // 1 relay (5) + semver (10) + repo (5) + security (15) = 35, no identity 30.
        assert_eq!(score, 35);
        assert_eq!(tier, "unverified");
    }

    /// Impersonation via the federation factor: claiming a DID the user
    /// federates with must earn nothing unless the claim is proven.
    #[test]
    fn claiming_a_federated_did_without_proving_it_earns_no_federation_points() {
        let (_key, victim_did, _) = signed_manifest();

        let mut impostor = sample_manifest();
        impostor.author.did = victim_did.clone();
        assert_eq!(
            verify_manifest_signature(&impostor),
            SignatureStatus::Missing
        );

        let federated = [victim_did];
        let (unproven, _) =
            calculate_trust_score(&impostor, 1, &federated, &SignatureStatus::Missing);
        let (proven, _) = calculate_trust_score(&impostor, 1, &federated, &SignatureStatus::Valid);

        assert_eq!(
            proven - unproven,
            50,
            "identity (30) + federation (20) must both hang off proof"
        );
    }

    #[test]
    fn test_manifest_serialization() {
        let manifest = sample_manifest();
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: AppManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.app_id, "test-app");
        assert_eq!(parsed.description.short(), "A test app");
    }

    #[test]
    fn test_category_display() {
        assert_eq!(AppCategory::Money.to_string(), "money");
        assert_eq!(AppCategory::Networking.to_string(), "networking");
    }

    #[tokio::test]
    async fn test_cache_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MarketplaceCache {
            apps: vec![DiscoveredApp {
                manifest: sample_manifest(),
                trust_score: 75,
                trust_tier: "community".into(),
                relay_count: 2,
                first_seen: "2026-03-10T00:00:00Z".into(),
                nostr_pubkey: "abc123".into(),
                signature: SignatureStatus::Valid,
            }],
            last_updated: "2026-03-10T00:00:00Z".into(),
        };
        save_cache(dir.path(), &cache).await.unwrap();
        let loaded = load_cache(dir.path()).await.unwrap();
        assert_eq!(loaded.apps.len(), 1);
        assert_eq!(loaded.apps[0].manifest.app_id, "test-app");
        assert_eq!(loaded.apps[0].signature, SignatureStatus::Valid);
    }

    /// A cache written before the signature field existed must still load, and
    /// must come back as unverified rather than silently defaulting to trusted.
    #[tokio::test]
    async fn a_legacy_cache_without_the_signature_field_loads_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        ensure_dirs(dir.path()).await.unwrap();
        let legacy = serde_json::json!({
            "apps": [{
                "manifest": sample_manifest(),
                "trust_score": 75,
                "trust_tier": "community",
                "relay_count": 2,
                "first_seen": "2026-03-10T00:00:00Z",
                "nostr_pubkey": "abc123"
            }],
            "last_updated": "2026-03-10T00:00:00Z"
        });
        let path = dir
            .path()
            .join(MARKETPLACE_DIR)
            .join("cache")
            .join(CACHE_FILE);
        fs::write(&path, serde_json::to_vec(&legacy).unwrap())
            .await
            .unwrap();

        let loaded = load_cache(dir.path()).await.unwrap();
        assert_eq!(loaded.apps.len(), 1);
        assert_eq!(loaded.apps[0].signature, SignatureStatus::Missing);
    }

    /// `SignatureStatus` crosses the RPC boundary to the UI, so its wire shape
    /// is a contract worth pinning.
    #[test]
    fn signature_status_serialises_to_a_tagged_object() {
        assert_eq!(
            serde_json::to_value(SignatureStatus::Valid).unwrap(),
            serde_json::json!({ "status": "valid" })
        );
        assert_eq!(
            serde_json::to_value(SignatureStatus::Missing).unwrap(),
            serde_json::json!({ "status": "missing" })
        );
        assert_eq!(
            serde_json::to_value(SignatureStatus::Invalid {
                reason: "nope".into()
            })
            .unwrap(),
            serde_json::json!({ "status": "invalid", "reason": "nope" })
        );
    }
}
