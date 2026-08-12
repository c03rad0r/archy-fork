//! Node-to-node messaging over Tor.
//! Sends messages to peer .onion addresses via SOCKS5 proxy.
//! Messages are persisted to disk and survive restarts.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const MAX_STORED: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    pub from_pubkey: String,
    #[serde(default)]
    pub from_onion: Option<String>,
    /// Sender's node name (for display in group chat).
    #[serde(default)]
    pub from_name: Option<String>,
    /// Sender-assigned unique id for the message. Used to dedup reliably even
    /// when a slow-Tor retry/redelivery arrives outside the time window (#31).
    #[serde(default)]
    pub msg_id: Option<String>,
    pub message: String,
    pub timestamp: String,
    /// "sent" or "received"
    #[serde(default = "default_received")]
    pub direction: String,
}

fn default_received() -> String {
    "received".to_string()
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MessageStore {
    messages: Vec<IncomingMessage>,
}

fn store() -> &'static Mutex<MessageStore> {
    static STORE: OnceLock<Mutex<MessageStore>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(MessageStore::default()))
}

fn data_path() -> &'static Mutex<Option<PathBuf>> {
    static PATH: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    PATH.get_or_init(|| Mutex::new(None))
}

/// At-rest encryption key for messages.json, derived from the node identity in
/// `init()`. `None` only if the node key is unreadable (pre-onboarding) — in
/// which case we persist plaintext rather than lose messages.
fn enc_key() -> &'static Mutex<Option<[u8; 32]>> {
    static KEY: OnceLock<Mutex<Option<[u8; 32]>>> = OnceLock::new();
    KEY.get_or_init(|| Mutex::new(None))
}

/// Initialize message store — load from disk. Call once at startup.
pub async fn init(data_dir: &Path) {
    let path = data_dir.join("messages.json");
    *data_path().lock().unwrap_or_else(|e| e.into_inner()) = Some(path.clone());

    // Derive + cache the at-rest encryption key (bound to this node's identity).
    match crate::storage_crypto::derive_key(data_dir, crate::storage_crypto::DOMAIN_MESSAGES).await
    {
        Ok(k) => *enc_key().lock().unwrap_or_else(|e| e.into_inner()) = Some(k),
        Err(e) => tracing::warn!(
            "message store: encryption key unavailable ({e}); will persist plaintext"
        ),
    }

    let Ok(raw) = tokio::fs::read(&path).await else {
        return; // no file yet (new node)
    };
    // Decrypt the on-disk blob, transparently migrating a legacy plaintext file.
    let mut was_plaintext = false;
    let bytes = if crate::storage_crypto::is_plaintext_json(&raw) {
        was_plaintext = true;
        Some(raw)
    } else {
        let key = *enc_key().lock().unwrap_or_else(|e| e.into_inner());
        match key {
            Some(k) => match crate::storage_crypto::open(&raw, &k) {
                Ok(p) => Some(p),
                Err(e) => {
                    tracing::error!(
                        "message store: decrypt failed ({e}); NOT overwriting on-disk data"
                    );
                    None
                }
            },
            None => None,
        }
    };
    if let Some(bytes) = bytes {
        if let Ok(loaded) = serde_json::from_slice::<MessageStore>(&bytes) {
            let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
            *guard = loaded;
            tracing::info!("Loaded {} messages from disk", guard.messages.len());
        }
    }
    // Eagerly re-write a legacy plaintext file as encrypted on first boot.
    if was_plaintext
        && enc_key()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    {
        persist();
        tracing::info!("message store: migrated plaintext messages.json to encrypted at rest");
    }
}

/// Persist current messages to disk.
/// Serializes under the lock, then writes asynchronously via spawn_blocking
/// to avoid blocking the tokio runtime.
fn persist() {
    let guard = store().lock().unwrap_or_else(|e| e.into_inner());
    let path_guard = data_path().lock().unwrap_or_else(|e| e.into_inner());
    let key = *enc_key().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref path) = *path_guard {
        if let Ok(content) = serde_json::to_vec(&*guard) {
            let path = path.clone();
            drop(path_guard);
            drop(guard);
            tokio::task::spawn(async move {
                // Encrypt at rest when the node key is available; fall back to
                // plaintext rather than drop the write if it somehow isn't.
                let bytes = match key {
                    Some(k) => crate::storage_crypto::seal(&content, &k).unwrap_or(content),
                    None => content,
                };
                // Atomic write: stage to a temp file then rename, so a crash or
                // reboot mid-write can never truncate/corrupt the real history
                // (rename is atomic on the same filesystem).
                let tmp = path.with_extension("json.tmp");
                if tokio::fs::write(&tmp, &bytes).await.is_ok() {
                    let _ = tokio::fs::rename(&tmp, &path).await;
                } else {
                    let _ = tokio::fs::remove_file(&tmp).await;
                }
            });
        }
    }
}

/// Store a received message (called from HTTP handler).
pub fn store_received_sync(
    from_pubkey: &str,
    message: &str,
    from_name: Option<&str>,
    msg_id: Option<&str>,
) {
    let ts = chrono::Utc::now().to_rfc3339();
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());

    // Deduplication. When the sender supplied a unique id, dedup on
    // (from_pubkey, msg_id) across all retained history — this is robust even
    // when a slow-Tor redelivery arrives well outside any time window (#31).
    // Older senders send no id; fall back to the legacy same-pubkey+message
    // within-30s heuristic.
    let duplicate = if let Some(id) = msg_id {
        guard
            .messages
            .iter()
            .any(|m| m.from_pubkey == from_pubkey && m.msg_id.as_deref() == Some(id))
    } else {
        guard.messages.iter().rev().take(20).any(|m| {
            m.from_pubkey == from_pubkey
                && m.message == message
                && m.direction == "received"
                && within_seconds(&m.timestamp, &ts, 30)
        })
    };
    if duplicate {
        return;
    }

    guard.messages.push(IncomingMessage {
        from_pubkey: from_pubkey.to_string(),
        from_onion: None,
        from_name: from_name.map(|s| s.to_string()),
        msg_id: msg_id.map(|s| s.to_string()),
        message: message.to_string(),
        timestamp: ts,
        direction: "received".to_string(),
    });
    trim_messages(&mut guard);
    drop(guard);
    persist();
}

pub async fn store_received(
    from_pubkey: &str,
    message: &str,
    from_name: Option<&str>,
    msg_id: Option<&str>,
) {
    store_received_sync(from_pubkey, message, from_name, msg_id);
}

/// Store a sent message (for display in Archipelago channel).
pub fn store_sent(message: &str) {
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    guard.messages.push(IncomingMessage {
        from_pubkey: "me".to_string(),
        from_onion: None,
        from_name: None,
        msg_id: None,
        message: message.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        direction: "sent".to_string(),
    });
    trim_messages(&mut guard);
    drop(guard);
    persist();
}

/// Get all messages (sent + received) for UI display.
pub fn get_received() -> Vec<IncomingMessage> {
    store()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .messages
        .clone()
}

fn trim_messages(store: &mut MessageStore) {
    if store.messages.len() > MAX_STORED {
        let drain = store.messages.len() - MAX_STORED;
        store.messages.drain(0..drain);
    }
}

/// Check if two ISO8601 timestamps are within N seconds of each other.
fn within_seconds(ts1: &str, ts2: &str, secs: i64) -> bool {
    use chrono::DateTime;
    let a = DateTime::parse_from_rfc3339(ts1).ok();
    let b = DateTime::parse_from_rfc3339(ts2).ok();
    match (a, b) {
        (Some(a), Some(b)) => (a - b).num_seconds().unsigned_abs() < secs as u64,
        _ => false,
    }
}

// ─── E2E Encryption ─────────────────────────────────────────────

use crate::mesh::crypto;
use base64::Engine;

/// Encrypt a message for a recipient using X25519 ECDH + ChaCha20-Poly1305.
/// Returns base64-encoded ciphertext (nonce + encrypted data).
fn encrypt_for_peer(
    our_signing_key: &ed25519_dalek::SigningKey,
    their_pubkey_hex: &str,
    plaintext: &str,
) -> Result<String> {
    let their_pubkey_bytes: [u8; 32] = hex::decode(their_pubkey_hex)
        .context("Invalid peer pubkey hex")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid peer pubkey length"))?;

    let their_x25519 = crypto::ed25519_pubkey_to_x25519(&their_pubkey_bytes)?;
    let our_x25519 = crypto::ed25519_secret_to_x25519(our_signing_key);
    let shared = crypto::x25519_shared_secret(&our_x25519, &their_x25519);

    // HKDF to derive message key (domain separation for Tor messages)
    let msg_key_bytes = crypto::hkdf_sha256(
        b"archipelago-tor-msg-v1",
        &shared,
        b"message-encryption",
        32,
    )?;
    let msg_key: [u8; 32] = msg_key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("HKDF key length mismatch"))?;

    let encrypted = crypto::encrypt(&msg_key, plaintext.as_bytes())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&encrypted))
}

/// Decrypt a message from a sender using X25519 ECDH + ChaCha20-Poly1305.
pub fn decrypt_from_peer(
    our_signing_key: &ed25519_dalek::SigningKey,
    sender_pubkey_hex: &str,
    encrypted_b64: &str,
) -> Result<String> {
    let sender_pubkey_bytes: [u8; 32] = hex::decode(sender_pubkey_hex)
        .context("Invalid sender pubkey hex")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid sender pubkey length"))?;

    let sender_x25519 = crypto::ed25519_pubkey_to_x25519(&sender_pubkey_bytes)?;
    let our_x25519 = crypto::ed25519_secret_to_x25519(our_signing_key);
    let shared = crypto::x25519_shared_secret(&our_x25519, &sender_x25519);

    let msg_key_bytes = crypto::hkdf_sha256(
        b"archipelago-tor-msg-v1",
        &shared,
        b"message-encryption",
        32,
    )?;
    let msg_key: [u8; 32] = msg_key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("HKDF key length mismatch"))?;

    let encrypted = base64::engine::general_purpose::STANDARD
        .decode(encrypted_b64)
        .context("Invalid base64 ciphertext")?;
    let plaintext_bytes = crypto::decrypt(&msg_key, &encrypted)?;
    String::from_utf8(plaintext_bytes).context("Decrypted message is not valid UTF-8")
}

// ─── Tor Messaging ──────────────────────────────────────────────

/// Tor v3 onion hostname is 56 base32 chars (a-z, 2-7). Reject invalid formats.
fn validate_onion(onion: &str) -> Result<()> {
    let host = onion.trim_end_matches(".onion");
    if host.len() != 56 {
        anyhow::bail!(
            "Invalid onion address (expected 56 chars, got {}). The peer may have wrong data - try removing and re-adding via Discover.",
            host.len()
        );
    }
    let valid = host
        .chars()
        .all(|c| c.is_ascii_lowercase() || ('2'..='7').contains(&c));
    if !valid {
        anyhow::bail!("Invalid onion address: must be 56 base32 chars (a-z, 2-7)");
    }
    Ok(())
}

/// Send an encrypted message to a peer over Tor.
/// The message is encrypted with ChaCha20-Poly1305 using an X25519 shared secret
/// derived from both nodes' ed25519 keys.
pub async fn send_to_peer(
    onion: &str,
    fips_npub: Option<&str>,
    from_pubkey: &str,
    message: &str,
    signing_key: Option<&ed25519_dalek::SigningKey>,
    recipient_pubkey: Option<&str>,
    from_name: Option<&str>,
    // Federation data dir for last-transport recording; None skips recording
    // (callers without a data dir in scope).
    record_data_dir: Option<&std::path::Path>,
) -> Result<()> {
    validate_onion(onion)?;

    // Encrypt message if we have both keys
    let (payload_message, encrypted) = match (signing_key, recipient_pubkey) {
        (Some(sk), Some(rpk)) => match encrypt_for_peer(sk, rpk, message) {
            Ok(enc) => (enc, true),
            Err(e) => {
                tracing::warn!("Encryption failed, sending plaintext: {}", e);
                (message.to_string(), false)
            }
        },
        _ => (message.to_string(), false),
    };

    let mut body = serde_json::json!({
        "from_pubkey": from_pubkey,
        "message": payload_message,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "encrypted": encrypted,
        // Unique per-message id so receivers can dedup reliably even across
        // slow-Tor retries/redeliveries (#31). Old receivers ignore it.
        "msg_id": uuid::Uuid::new_v4().to_string(),
    });
    if let Some(name) = from_name {
        body["from_name"] = serde_json::Value::String(name.to_string());
    }

    let mut req =
        crate::fips::dial::PeerRequest::new(fips_npub, onion, "/archipelago/node-message")
            .service(crate::settings::transport::PeerService::Messaging)
            .timeout(std::time::Duration::from_secs(60))
            .fips_timeout(std::time::Duration::from_secs(8));
    if let Some(dir) = record_data_dir {
        req = req.record_transport(dir);
    }
    let (resp, transport) = req.send_json(&body).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("connection refused") || msg.contains("Connection refused") {
            anyhow::anyhow!("Peer unreachable. Check Tor (127.0.0.1:9050) and FIPS daemon status.")
        } else if msg.contains("timeout") || msg.contains("timed out") {
            anyhow::anyhow!("Connection timed out. The peer may be offline.")
        } else {
            anyhow::anyhow!("Failed to send: {}", msg)
        }
    })?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "Peer returned {} {} (via {}). The peer may need /archipelago/ in its nginx config.",
            resp.status().as_u16(),
            resp.status().canonical_reason().unwrap_or(""),
            transport,
        );
    }
    Ok(())
}

/// Check if a peer is reachable (ping). FIPS is preferred when an npub
/// is known, Tor is the fallback.
pub async fn check_peer_reachable(onion: &str, fips_npub: Option<&str>) -> Result<bool> {
    validate_onion(onion)?;
    match crate::fips::dial::PeerRequest::new(fips_npub, onion, "/health")
        .service(crate::settings::transport::PeerService::Messaging)
        // 12s, not 30s: this is a liveness dot, not a data transfer. A Tor
        // circuit that hasn't answered /health in 12s is "offline" for UI
        // purposes; the old 30s made the Connected Nodes probes crawl.
        .timeout(std::time::Duration::from_secs(12))
        .fips_timeout(std::time::Duration::from_secs(4))
        .send_get()
        .await
    {
        Ok((resp, _)) => Ok(resp.status().is_success()),
        Err(_) => Ok(false),
    }
}
