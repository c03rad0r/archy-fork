//! Companion app backup — the ADR-005 encrypted-backup envelope.
//!
//! Reuses the node's backup format exactly (ADR-005:
//! `core/archipelago/src/backup/identity.rs`): Argon2id key derivation with
//! default params, ChaCha20-Poly1305 AEAD, and the same blob layout
//! `base64(salt[16] || nonce[12] || ciphertext)`. A companion backup and a
//! node backup share one crypto story — the payload differs (the companion
//! serializes its servers, FIPS identity and signer key instead of a node
//! key), the envelope does not.
//!
//! The envelope is JSON with `version`, `kind`, `encrypted`, `blob` and
//! `timestamp`; [`decrypt`] ignores any extra fields, so node envelopes
//! (which carry `did`/`pubkey`/`kid`) decrypt here too.

use anyhow::{bail, Context, Result};
use argon2::Argon2;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use serde_json::json;

/// Envelope version. Bump only when the blob layout itself changes — and
/// then only with a reader for the old layout (same policy as the node).
const BACKUP_VERSION: u32 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// Encrypt a JSON payload into an ADR-005 envelope.
///
/// The passphrase never leaves this call; the envelope carries only the
/// salt (Argon2id parameter), the AEAD nonce, and the ciphertext.
pub fn encrypt(payload: &str, passphrase: &str) -> Result<String> {
    if payload.is_empty() {
        bail!("backup payload is empty");
    }
    if passphrase.is_empty() {
        bail!("backup passphrase must not be empty");
    }

    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    // Same CSPRNG discipline as identity generation (getrandom, see mesh.rs):
    // OS RNG, never thread-local or derived-from-content randomness for key
    // material or nonces.
    getrandom::getrandom(&mut salt).context("OS RNG")?;
    getrandom::getrandom(&mut nonce).context("OS RNG")?;

    let key = derive_key(passphrase, &salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), payload.as_bytes())
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;

    let mut blob = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(&salt);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);

    Ok(json!({
        "version": BACKUP_VERSION,
        "kind": "companion",
        "encrypted": true,
        "blob": BASE64.encode(&blob),
        "timestamp": chrono_like_now(),
    })
    .to_string())
}

/// Decrypt an ADR-005 envelope back into its JSON payload.
///
/// Accepts `version: 1` envelopes regardless of `kind` or extra fields —
/// the node's identity backups use the same blob, and being able to decrypt
/// one here is free interop (the caller decides what to do with it).
pub fn decrypt(envelope: &str, passphrase: &str) -> Result<String> {
    let obj: serde_json::Value =
        serde_json::from_str(envelope).context("not a JSON backup envelope")?;

    if obj.get("version").and_then(|v| v.as_u64()) != Some(BACKUP_VERSION as u64) {
        bail!("unsupported backup version (expected {BACKUP_VERSION})");
    }

    let blob_b64 = obj
        .get("blob")
        .and_then(|v| v.as_str())
        .context("missing 'blob' in backup envelope")?;
    let blob = BASE64
        .decode(blob_b64)
        .context("invalid base64 in backup blob")?;
    if blob.len() < SALT_LEN + NONCE_LEN {
        bail!("backup blob too short");
    }

    let salt = &blob[..SALT_LEN];
    let nonce = &blob[SALT_LEN..SALT_LEN + NONCE_LEN];
    let ciphertext = &blob[SALT_LEN + NONCE_LEN..];

    let key = derive_key(passphrase, salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| anyhow::anyhow!("decryption failed — wrong passphrase or corrupted backup"))?;

    String::from_utf8(plaintext).context("decrypted payload is not valid UTF-8")
}

fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; KEY_LEN]> {
    let mut key = [0u8; KEY_LEN];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow::anyhow!("Argon2 key derivation failed: {e}"))?;
    Ok(key)
}

/// RFC 3339 UTC timestamp without pulling chrono into the .so — the node's
/// envelope field is informational (display), not part of the authenticated
/// or derived material.
fn chrono_like_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil-from-days (Howard Hinnant's algorithm), valid for 1970-2100+.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD: &str = r#"{"app":"archipelago-companion","servers":["192.168.1.10|false|1301||Lab Node|fd00::1|npub1abc"]}"#;

    #[test]
    fn round_trip() {
        let envelope = encrypt(PAYLOAD, "correct horse battery staple").unwrap();
        let decrypted = decrypt(&envelope, "correct horse battery staple").unwrap();
        assert_eq!(decrypted, PAYLOAD);
    }

    #[test]
    fn wrong_passphrase_fails() {
        let envelope = encrypt(PAYLOAD, "right").unwrap();
        let err = decrypt(&envelope, "wrong").unwrap_err();
        assert!(
            err.to_string().contains("wrong passphrase"),
            "error should name the likely cause: {err}"
        );
    }

    #[test]
    fn envelope_shape_matches_node_format() {
        let envelope = encrypt(PAYLOAD, "pw").unwrap();
        let obj: serde_json::Value = serde_json::from_str(&envelope).unwrap();

        assert_eq!(obj["version"], 1);
        assert_eq!(obj["encrypted"], true);
        assert!(obj["kind"].as_str().is_some());
        assert!(obj["timestamp"].as_str().is_some());

        // Blob layout is exactly the node's: base64(salt||nonce||ct) with the
        // AEAD tag inside the ciphertext — at least 16+12+16+1 bytes.
        let blob = BASE64
            .decode(obj["blob"].as_str().unwrap())
            .expect("blob is base64");
        assert!(blob.len() >= SALT_LEN + NONCE_LEN + 16 + PAYLOAD.len());
    }

    #[test]
    fn fresh_salt_and_nonce_every_time() {
        let a = encrypt(PAYLOAD, "pw").unwrap();
        let b = encrypt(PAYLOAD, "pw").unwrap();
        let (oa, ob): (serde_json::Value, serde_json::Value) = (
            serde_json::from_str(&a).unwrap(),
            serde_json::from_str(&b).unwrap(),
        );
        assert_ne!(oa["blob"], ob["blob"], "salt/nonce must never repeat");
    }

    #[test]
    fn tampered_blob_fails_to_decrypt() {
        let envelope = encrypt(PAYLOAD, "pw").unwrap();
        let mut obj: serde_json::Value = serde_json::from_str(&envelope).unwrap();
        let blob = BASE64.decode(obj["blob"].as_str().unwrap()).unwrap();
        let mut tampered = blob.clone();
        // Flip a bit inside the ciphertext (past salt+nonce).
        tampered[SALT_LEN + NONCE_LEN] ^= 0x01;
        obj["blob"] = serde_json::Value::String(BASE64.encode(&tampered));
        assert!(decrypt(&obj.to_string(), "pw").is_err());
    }

    /// Node identity backups use the same blob layout but carry their own
    /// envelope fields (did/pubkey/kid). Decrypt must ignore those extras —
    /// one envelope reader, two producers.
    #[test]
    fn node_style_envelope_with_extra_fields_decrypts() {
        let envelope = encrypt(PAYLOAD, "pw").unwrap();
        let mut obj: serde_json::Value = serde_json::from_str(&envelope).unwrap();
        obj["kind"] = serde_json::Value::String("node-identity".into());
        obj["did"] = serde_json::Value::String("did:key:z6Mktest".into());
        obj["pubkey"] = serde_json::Value::String("aabbcc".into());
        obj["kid"] = serde_json::Value::String("did:key:z6Mktest#key-1".into());
        let decrypted = decrypt(&obj.to_string(), "pw").unwrap();
        assert_eq!(decrypted, PAYLOAD);
    }

    #[test]
    fn rejects_unknown_version_and_garbage() {
        let err = decrypt("{\"version\":99,\"blob\":\"AAAA\"}", "pw").unwrap_err();
        assert!(err.to_string().contains("version"));
        assert!(decrypt("not json", "pw").is_err());
        assert!(decrypt("{\"version\":1}", "pw").is_err());
    }

    #[test]
    fn rejects_empty_passphrase_and_payload() {
        assert!(encrypt(PAYLOAD, "").is_err());
        assert!(encrypt("", "pw").is_err());
    }

    #[test]
    fn timestamp_is_rfc3339_utc() {
        let envelope = encrypt(PAYLOAD, "pw").unwrap();
        let obj: serde_json::Value = serde_json::from_str(&envelope).unwrap();
        let ts = obj["timestamp"].as_str().unwrap();
        // 2026-08-31T12:34:56Z — 20 chars, RFC 3339 UTC.
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
        assert!(ts.starts_with("20"));
    }
}
