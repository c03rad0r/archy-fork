//! Nostr node discovery: publish node identity to relays for peer discovery.
//! Uses NIP-33 replaceable events (kind 30078) with d-tag "archipelago-node".
//!
//! Security: Publishing is opt-in (ARCHIPELAGO_NOSTR_DISCOVERY_ENABLED + relays).
//! All Nostr traffic routes through Tor when ARCHIPELAGO_NOSTR_TOR_PROXY is set.
//! Legacy revocation overwrites any previously published data on old public relays.

use anyhow::{Context, Result};
use nostr_sdk::prelude::*;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;
use tokio::fs;

/// Parse "host:port" to SocketAddr. Returns None if invalid.
fn parse_proxy_addr(s: &str) -> Option<SocketAddr> {
    s.trim().parse().ok()
}

const NOSTR_SECRET_FILE: &str = "nostr_secret";
const NOSTR_PUB_FILE: &str = "nostr_pub";
const NOSTR_REVOKED_FILE: &str = "nostr_revoked";
const ARCHIPELAGO_KIND: u64 = 30078;
const D_TAG: &str = "archipelago-node";

/// Relays we previously published to (for one-time revocation overwrite only)
const LEGACY_RELAYS: &[&str] = &["wss://relay.damus.io", "wss://relay.nostr.info"];

/// Does this node already have a Nostr identity on disk?
///
/// `load_or_create_nostr_keys` mints one when it finds none. That is right at
/// bootstrap and wrong for anything that AUTHENTICATES as this node: signing
/// with a key generated on the spot proves nothing, presents the node as a
/// stranger to whatever it is authenticating to, and writes a new secret to
/// disk as a side effect of what the caller thought was a read. Callers that
/// must prove an EXISTING identity gate on this first and fail loudly.
pub(crate) async fn nostr_identity_exists(identity_dir: &Path) -> bool {
    fs::try_exists(identity_dir.join(NOSTR_SECRET_FILE))
        .await
        .unwrap_or(false)
}

/// Load or create Nostr keys (secp256k1) for node discovery.
pub(crate) async fn load_or_create_nostr_keys(identity_dir: &Path) -> Result<Keys> {
    let secret_path = identity_dir.join(NOSTR_SECRET_FILE);
    let pub_path = identity_dir.join(NOSTR_PUB_FILE);

    let keys = if secret_path.exists() {
        let hex_secret = fs::read_to_string(&secret_path)
            .await
            .context("Failed to read Nostr secret")?;
        Keys::parse(hex_secret.trim()).context("Invalid Nostr secret")?
    } else {
        let keys = Keys::generate();
        fs::create_dir_all(identity_dir)
            .await
            .context("Failed to create identity dir")?;
        let hex = keys.secret_key().to_secret_hex();
        fs::write(&secret_path, hex)
            .await
            .context("Failed to write Nostr secret")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&secret_path, std::fs::Permissions::from_mode(0o600))
                .await
                .context("Failed to set Nostr key permissions")?;
        }
        fs::write(&pub_path, keys.public_key().to_hex())
            .await
            .context("Failed to write Nostr pubkey")?;
        tracing::info!("🔑 Generated Nostr discovery key");
        keys
    };

    Ok(keys)
}

/// Load Nostr keys only if they exist (does not create). Used for revocation.
async fn load_nostr_keys_if_exists(identity_dir: &Path) -> Result<Option<Keys>> {
    let secret_path = identity_dir.join(NOSTR_SECRET_FILE);
    if !secret_path.exists() {
        return Ok(None);
    }
    let hex_secret = fs::read_to_string(&secret_path)
        .await
        .context("Failed to read Nostr secret")?;
    let keys = Keys::parse(hex_secret.trim()).context("Invalid Nostr secret")?;
    Ok(Some(keys))
}

/// Publish a replaceable event with empty content to overwrite/revoke previously published data.
/// Uses NIP-33: same kind + d-tag + author = latest replaces. Sends to LEGACY_RELAYS only.
/// Requires tor_proxy to avoid leaking IP to relay operators.
pub(crate) fn build_nostr_client(keys: Keys, tor_proxy: Option<&str>) -> Result<Client> {
    let client = if let Some(proxy_str) = tor_proxy {
        let addr = parse_proxy_addr(proxy_str)
            .ok_or_else(|| anyhow::anyhow!("Invalid Nostr Tor proxy: {}", proxy_str))?;
        let connection = Connection::new().proxy(addr).target(ConnectionTarget::All);
        let opts = ClientOptions::new().connection(connection);
        Client::builder().signer(keys).opts(opts).build()
    } else {
        Client::new(keys)
    };
    Ok(client)
}

/// Publish a replaceable event with empty content to overwrite/revoke previously published data.
/// Uses NIP-33: same kind + d-tag + author = latest replaces. Sends to LEGACY_RELAYS only.
/// Only call when tor_proxy is set (avoids IP leak).
pub async fn publish_node_revocation(identity_dir: &Path, tor_proxy: Option<&str>) -> Result<()> {
    let Some(keys) = load_nostr_keys_if_exists(identity_dir).await? else {
        return Ok(()); // No keys = never published, nothing to revoke
    };

    let client = build_nostr_client(keys, tor_proxy)?;
    for url in LEGACY_RELAYS {
        let _ = client.add_relay(*url).await;
    }
    if tokio::time::timeout(Duration::from_secs(10), client.connect())
        .await
        .is_err()
    {
        tracing::warn!("Nostr relay connection timed out after 10s, continuing anyway");
    }

    // NIP-33 replaceable: empty content overwrites previous event
    let builder =
        EventBuilder::new(Kind::Custom(ARCHIPELAGO_KIND as u16), "{}").tag(Tag::identifier(D_TAG));
    let _ = client.send_event_builder(builder).await;
    client.disconnect().await;

    Ok(())
}

/// If we have Nostr keys but haven't revoked yet, publish revocation to overwrite legacy data.
/// Uses tor_proxy if set; otherwise tries 127.0.0.1:9050 (archy-tor default). Creates nostr_revoked sentinel.
pub async fn revoke_if_needed(identity_dir: &Path, tor_proxy: Option<&str>) -> Result<()> {
    let revoked_path = identity_dir.join(NOSTR_REVOKED_FILE);
    if revoked_path.exists() {
        return Ok(());
    }
    if load_nostr_keys_if_exists(identity_dir).await?.is_none() {
        return Ok(());
    }
    // Use configured proxy or Tor default (archy-tor exposes 127.0.0.1:9050)
    let proxy = tor_proxy.or(Some("127.0.0.1:9050"));

    if let Err(e) = publish_node_revocation(identity_dir, proxy).await {
        tracing::warn!("Nostr revocation (non-fatal): {}", e);
        return Ok(());
    }

    fs::create_dir_all(identity_dir).await?;
    fs::write(&revoked_path, "").await?;
    tracing::info!("🔒 Nostr discovery data revoked (overwritten on legacy relays)");
    Ok(())
}

/// Get Nostr public key for this node (hex).
pub async fn get_nostr_pubkey(identity_dir: &Path) -> Result<String> {
    let keys = load_or_create_nostr_keys(identity_dir).await?;
    Ok(keys.public_key().to_hex())
}

/// Sign a 32-byte hash with the node's Nostr Schnorr key.
pub async fn nostr_sign_hash(identity_dir: &Path, hash_hex: &str) -> Result<String> {
    let keys = load_or_create_nostr_keys(identity_dir).await?;
    let hash_bytes = hex::decode(hash_hex).context("Invalid hash hex")?;
    if hash_bytes.len() != 32 {
        anyhow::bail!("Hash must be 32 bytes");
    }
    let message = nostr_sdk::secp256k1::Message::from_digest(
        hash_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid hash length"))?,
    );
    let sig = keys.sign_schnorr(&message);
    Ok(sig.to_string())
}

/// Verify that our node's Nostr discovery data was revoked on the legacy relays.
/// Queries relays for our pubkey's kind 30078 events; if latest has empty content, revocation succeeded.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RevocationStatus {
    pub revoked: bool,
    pub nostr_pubkey: String,
    pub latest_content: Option<String>,
    pub error: Option<String>,
}

pub async fn verify_revocation(
    identity_dir: &Path,
    tor_proxy: Option<&str>,
) -> Result<RevocationStatus> {
    let keys = match load_nostr_keys_if_exists(identity_dir).await? {
        Some(k) => k,
        None => {
            return Ok(RevocationStatus {
                revoked: true,
                nostr_pubkey: String::new(),
                latest_content: None,
                error: Some("No Nostr keys - never published".to_string()),
            });
        }
    };
    let pubkey_hex = keys.public_key().to_hex();

    let anon_keys = Keys::generate();
    let client = build_nostr_client(anon_keys, tor_proxy)?;
    for url in LEGACY_RELAYS {
        let _ = client.add_relay(*url).await;
    }
    if tokio::time::timeout(Duration::from_secs(10), client.connect())
        .await
        .is_err()
    {
        tracing::warn!("Nostr relay connection timed out after 10s, continuing anyway");
    }

    let filter = Filter::new()
        .kind(Kind::Custom(ARCHIPELAGO_KIND as u16))
        .identifier(D_TAG)
        .author(keys.public_key())
        .limit(10);
    let events = client
        .fetch_events(filter, std::time::Duration::from_secs(15))
        .await
        .map(|e| e.to_vec())
        .unwrap_or_default();
    client.disconnect().await;

    // NIP-33: latest event wins. fetch_events returns sorted by timestamp desc.
    let mut events: Vec<_> = events.into_iter().collect();
    events.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let latest = events.into_iter().next();
    let (revoked, latest_content) = match latest {
        None => (true, None),
        Some(ev) => {
            let content = ev.content;
            let is_revoked =
                content == "{}" || content.is_empty() || !content.contains("node_address");
            (is_revoked, Some(content))
        }
    };

    Ok(RevocationStatus {
        revoked,
        nostr_pubkey: pubkey_hex,
        latest_content,
        error: None,
    })
}

/// Discovered Archipelago node from Nostr.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredNode {
    pub did: String,
    pub node_address: String,
    pub onion: String,
    pub pubkey: String,
    pub version: String,
}

/// Query Nostr relays for other Archipelago nodes.
/// Returns empty if relays is empty (opt-in discovery).
/// When tor_proxy is set, routes through Tor to prevent IP exposure.
pub async fn discover_archipelago_nodes(
    identity_dir: &Path,
    relays: &[String],
    tor_proxy: Option<&str>,
) -> Result<Vec<DiscoveredNode>> {
    if relays.is_empty() {
        return Ok(Vec::new());
    }

    let _keys = load_or_create_nostr_keys(identity_dir).await?;
    let anon_keys = Keys::generate();
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

    let filter = Filter::new()
        .kind(Kind::Custom(ARCHIPELAGO_KIND as u16))
        .identifier(D_TAG)
        .limit(50);
    let events = client
        .fetch_events(filter, std::time::Duration::from_secs(15))
        .await
        .map(|e| e.to_vec())
        .unwrap_or_default();
    client.disconnect().await;

    let mut nodes = Vec::new();
    for event in events {
        if let Ok(content) = serde_json::from_str::<serde_json::Value>(&event.content) {
            // Skip revoked/empty events
            let node_address = content
                .get("node_address")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if node_address.is_empty() {
                continue;
            }
            let did = content
                .get("did")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let version = content
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("0.1")
                .to_string();
            // Parse archipelago://xxx.onion#pubkey
            let (onion, pubkey) = if node_address.starts_with("archipelago://") {
                let rest = node_address.trim_start_matches("archipelago://");
                if let Some((o, p)) = rest.split_once('#') {
                    (o.to_string(), p.to_string())
                } else {
                    (rest.to_string(), "".to_string())
                }
            } else {
                ("".to_string(), "".to_string())
            };
            if !onion.is_empty() {
                nodes.push(DiscoveredNode {
                    did,
                    node_address,
                    onion: onion.trim_end_matches('/').to_string(),
                    pubkey,
                    version,
                });
            }
        }
    }
    Ok(nodes)
}
