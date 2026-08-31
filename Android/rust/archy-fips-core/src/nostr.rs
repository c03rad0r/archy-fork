//! NIP-46 phone-side remote signer ("bunker") crypto core.
//!
//! Everything that must be constant-time correct for the companion to act as
//! a nostr remote signer: key handling (nsec/npub bech32), BIP340 schnorr
//! event signing, NIP-44 v2 payload encryption (the mandated NIP-46
//! transport), NIP-04 fallback decryption (deprecated, but real clients
//! still speak it), and `nostrconnect://` URI parsing. The protocol session
//! — relay WebSocket, JSON-RPC dispatch, approve/deny UX — lives in Kotlin;
//! this module is the crypto and nothing but.
//!
//! Verified against the official NIP-44 vectors and BIP-340 reference
//! vectors (see tests below).

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE as BASE64_URL};
use base64::Engine;
use bech32::{Bech32, Hrp};
use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20::ChaCha20;
use hmac::{Hmac, Mac};
use hkdf::Hkdf;
use secp256k1::ecdh;
use secp256k1::schnorr::Signature;
use secp256k1::{
    Keypair, Message, PublicKey, Secp256k1, SecretKey, XOnlyPublicKey,
};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

const NIP44_VERSION: u8 = 2;
const NIP44_SALT: &[u8] = b"nip44-v2";
const NIP44_MIN_PAYLOAD_LEN: usize = 99; // 1 ver + 32 nonce + 32 ct + 32 mac
const NIP44_MIN_B64_LEN: usize = 132;

// ── keys ──────────────────────────────────────────────────────────────────

/// Generate a fresh nostr secret key (hex) from the OS CSPRNG.
pub fn generate_secret() -> Result<String> {
    loop {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes).context("OS RNG")?;
        // Reject zero and >= curve order — the valid scalar range (mirrors
        // the mesh identity loop; rejection is astronomically unlikely).
        if bytes.iter().all(|&b| b == 0) {
            continue;
        }
        if SecretKey::from_slice(&bytes).is_ok() {
            return Ok(hex::encode(bytes));
        }
    }
}

/// Parse a secret key from hex or bech32 `nsec…` form into hex.
pub fn secret_from_any(s: &str) -> Result<String> {
    let s = s.trim();
    if s.starts_with("nsec") {
        return secret_from_nsec(s);
    }
    let bytes = hex::decode(s.trim()).context("secret key must be hex or nsec")?;
    let sk = SecretKey::from_slice(&bytes).context("invalid nostr secret key")?;
    Ok(hex::encode(sk.secret_bytes()))
}

pub fn secret_from_nsec(nsec: &str) -> Result<String> {
    let (hrp, data) = bech32::decode(nsec).context("bad nsec encoding")?;
    if hrp.as_str() != "nsec" {
        bail!("not an nsec");
    }
    let sk = SecretKey::from_slice(&data).context("invalid nostr secret key")?;
    Ok(hex::encode(sk.secret_bytes()))
}

pub fn nsec_from_secret(secret_hex: &str) -> Result<String> {
    let bytes = hex::decode(secret_hex.trim()).context("bad secret hex")?;
    let hrp = Hrp::parse("nsec").context("nsec hrp")?;
    bech32::encode::<Bech32>(hrp, &bytes).context("nsec encoding")
}

/// x-only public key (hex) for a secret key.
/// NOTE: `Keypair::public_key()` in secp256k1 0.29 is the full compressed
/// (33-byte) key — nostr uses x-only pubkeys, so serialize `.x_only_public_key().0`.
pub fn pubkey_hex(secret_hex: &str) -> Result<String> {
    let kp = keypair(secret_hex)?;
    Ok(hex::encode(kp.public_key().x_only_public_key().0.serialize()))
}

pub fn npub_from_pubkey(pub_hex: &str) -> Result<String> {
    let bytes = hex::decode(pub_hex.trim()).context("bad pubkey hex")?;
    let hrp = Hrp::parse("npub").context("npub hrp")?;
    bech32::encode::<Bech32>(hrp, &bytes).context("npub encoding")
}

/// Parse an x-only pubkey from hex or bech32 `npub…` form into hex.
pub fn pubkey_from_any(s: &str) -> Result<String> {
    let s = s.trim();
    let bytes = if s.starts_with("npub") {
        let (hrp, data) = bech32::decode(s).context("bad npub encoding")?;
        if hrp.as_str() != "npub" {
            bail!("not an npub");
        }
        data
    } else {
        hex::decode(s).context("pubkey must be hex or npub")?
    };
    XOnlyPublicKey::from_slice(&bytes).context("invalid x-only pubkey")?;
    Ok(hex::encode(bytes))
}

fn keypair(secret_hex: &str) -> Result<Keypair> {
    let bytes = hex::decode(secret_hex.trim()).context("bad secret hex")?;
    let sk = SecretKey::from_slice(&bytes).context("invalid nostr secret key")?;
    Ok(Keypair::from_secret_key(&Secp256k1::new(), &sk))
}

// ── nostrconnect:// URI ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ConnectUri {
    /// The client's pubkey, hex.
    pub client_pubkey: String,
    /// Relays the client is listening on (≥1 by spec; kept in URI order).
    pub relays: Vec<String>,
    /// One-time pairing secret the client expects to see echoed back.
    pub secret: String,
    /// Comma-separated permission grants the client requests (display hint
    /// only — approval always stays with the human).
    pub perms: Vec<String>,
    pub name: String,
    pub url: String,
    pub image: String,
}

impl ConnectUri {
    /// JSON shape for the JNI boundary (flat strings/arrays — easy to parse
    /// with org.json on the Kotlin side).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "clientPubkey": self.client_pubkey,
            "relays": self.relays,
            "secret": self.secret,
            "perms": self.perms,
            "name": self.name,
            "url": self.url,
            "image": self.image,
        })
    }
}

/// Parse `nostrconnect://<client-pubkey>?relay=…&secret=…&perms=…&name=…`.
///
/// Query values are percent-decoded; `relay` may repeat. The pubkey in the
/// host position may be hex or (non-spec but harmless) `npub…`.
pub fn parse_connect_uri(uri: &str) -> Result<ConnectUri> {
    let uri = uri.trim();
    let rest = uri
        .strip_prefix("nostrconnect://")
        .ok_or_else(|| anyhow::anyhow!("not a nostrconnect:// URI"))?;

    let (host, query) = match rest.split_once('?') {
        Some((h, q)) => (h, q),
        None => bail!("nostrconnect URI has no query parameters"),
    };
    let client_pubkey = pubkey_from_any(host).context("nostrconnect URI: bad client pubkey")?;

    let mut relays = Vec::new();
    let mut secret = String::new();
    let mut perms: Vec<String> = Vec::new();
    let mut name = String::new();
    let mut url = String::new();
    let mut image = String::new();

    for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
        let v = v.into_owned();
        match k.as_ref() {
            "relay" => {
                if v.starts_with("ws://") || v.starts_with("wss://") {
                    relays.push(v);
                }
            }
            "secret" => secret = v,
            "perms" => perms = v.split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
            "name" => name = v,
            "url" => url = v,
            "image" => image = v,
            _ => {} // forward-compat: ignore unknown params
        }
    }

    if relays.is_empty() {
        bail!("nostrconnect URI carries no relay");
    }
    if secret.is_empty() {
        bail!("nostrconnect URI carries no secret");
    }

    Ok(ConnectUri {
        client_pubkey,
        relays,
        secret,
        perms,
        name,
        url,
        image,
    })
}

// ── events (NIP-01 id + BIP340 signature) ─────────────────────────────────

/// Compute the NIP-01 event id: sha256 over the compact serialization
/// `[0, pubkey, created_at, kind, tags, content]`.
fn event_id(pubkey: &str, created_at: u64, kind: u64, tags: &serde_json::Value, content: &str) -> [u8; 32] {
    let serialized = serde_json::json!([
        0,
        pubkey,
        created_at,
        kind,
        tags,
        content,
    ]);
    let mut hasher = Sha256::new();
    hasher.update(serialized.to_string().as_bytes());
    hasher.finalize().into()
}

/// Sign an unsigned event `{kind, content, tags, created_at}` (pubkey filled
/// from the secret key; `pubkey` in the input ignored) and return the signed
/// event JSON. This is the `sign_event` NIP-46 method's core — the approve
/// happens before this call, never inside it.
pub fn sign_event(secret_hex: &str, event_json: &str) -> Result<String> {
    let ev: serde_json::Value = serde_json::from_str(event_json).context("event is not JSON")?;
    let kind = ev
        .get("kind")
        .and_then(|v| v.as_u64())
        .context("event has no kind")?;
    let created_at = ev
        .get("created_at")
        .and_then(|v| v.as_u64())
        .context("event has no created_at")?;
    let tags = ev
        .get("tags")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    let content = ev
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let kp = keypair(secret_hex)?;
    let pubkey = hex::encode(kp.public_key().x_only_public_key().0.serialize());
    let id = event_id(&pubkey, created_at, kind, &tags, &content);

    let mut aux = [0u8; 32];
    getrandom::getrandom(&mut aux).context("OS RNG")?;
    let sig = Secp256k1::new().sign_schnorr_with_aux_rand(
        &Message::from_digest(id),
        &kp,
        &aux,
    );

    Ok(serde_json::json!({
        "id": hex::encode(id),
        "pubkey": pubkey,
        "created_at": created_at,
        "kind": kind,
        "tags": tags,
        "content": content,
        "sig": hex::encode(sig.serialize()),
    })
    .to_string())
}

/// Verify a signed event's id and schnorr signature (tests + defensive use).
pub fn verify_event(event_json: &str) -> Result<()> {
    let ev: serde_json::Value = serde_json::from_str(event_json).context("event is not JSON")?;
    let pubkey = ev.get("pubkey").and_then(|v| v.as_str()).context("no pubkey")?;
    let id_hex = ev.get("id").and_then(|v| v.as_str()).context("no id")?;
    let sig_hex = ev.get("sig").and_then(|v| v.as_str()).context("no sig")?;
    let kind = ev.get("kind").and_then(|v| v.as_u64()).context("no kind")?;
    let created_at = ev.get("created_at").and_then(|v| v.as_u64()).context("no created_at")?;
    let tags = ev.get("tags").cloned().unwrap_or_else(|| serde_json::json!([]));
    let content = ev.get("content").and_then(|v| v.as_str()).unwrap_or("");

    let expected = event_id(pubkey, created_at, kind, &tags, content);
    if hex::encode(expected) != id_hex {
        bail!("event id mismatch");
    }

    let pk = XOnlyPublicKey::from_slice(&hex::decode(pubkey)?)
        .context("bad pubkey")?;
    let sig = Signature::from_slice(&hex::decode(sig_hex)?)
        .context("bad signature")?;
    Secp256k1::new()
        .verify_schnorr(&sig, &Message::from_digest(expected), &pk)
        .context("signature verification failed")?;
    Ok(())
}

// ── NIP-44 v2 ──────────────────────────────────────────────────────────────

/// ECDH shared x-coordinate (unhashed, 32 bytes) between our secret key and
/// the peer's x-only public key. Lifting the x-only key with even-y parity
/// is safe here: negating a point flips only y, so the shared x — the only
/// thing NIP-44/NIP-04 consume — is unchanged.
fn shared_x(secret_hex: &str, peer_pubkey_hex: &str) -> Result<[u8; 32]> {
    let sk_bytes = hex::decode(secret_hex.trim()).context("bad secret hex")?;
    let sk = SecretKey::from_slice(&sk_bytes).context("invalid secret key")?;
    let peer_hex = pubkey_from_any(peer_pubkey_hex)?;
    let peer = XOnlyPublicKey::from_slice(&hex::decode(&peer_hex)?)
        .context("invalid peer pubkey")?;
    // Lift x-only key to a full public key (even-y representative).
    let full = PublicKey::from_x_only_public_key(peer, secp256k1::Parity::Even);
    let point = ecdh::shared_secret_point(&full, &sk); // 64 bytes: x || y
    let mut x = [0u8; 32];
    x.copy_from_slice(&point[..32]);
    Ok(x)
}

/// NIP-44 v2 conversation key: HKDF-extract(IKM = ECDH x, salt = 'nip44-v2').
fn conversation_key(secret_hex: &str, peer_pubkey_hex: &str) -> Result<[u8; 32]> {
    let x = shared_x(secret_hex, peer_pubkey_hex)?;
    let mut hk = HkdfExtractSha256::new(Some(NIP44_SALT));
    hk.input_ikm(&x);
    let (prk, _) = hk.finalize();
    let mut ck = [0u8; 32];
    ck.copy_from_slice(prk.as_slice());
    Ok(ck)
}

/// HKDF-SHA256 extract step, exposing the raw PRK (Hkdf::expand hashes with
/// an info suffix even when info is empty, which is NOT the extract output;
/// finalize returns (PRK, ready-to-expand Hkdf)).
type HkdfExtractSha256 = hkdf::HkdfExtract<Sha256>;

/// Per-message keys: HKDF-expand(PRK = conversation key, info = nonce, L = 76)
/// sliced into chacha_key[32] chacha_nonce[12] hmac_key[32].
fn message_keys(ck: &[u8; 32], nonce: &[u8; 32]) -> ([u8; 32], [u8; 12], [u8; 32]) {
    let hk = Hkdf::<Sha256>::from_prk(ck).expect("conversation key is 32 bytes");
    let mut okm = [0u8; 76];
    hk.expand(nonce, &mut okm).expect("76 <= 255 * hash len");
    let mut chacha_key = [0u8; 32];
    let mut chacha_nonce = [0u8; 12];
    let mut hmac_key = [0u8; 32];
    chacha_key.copy_from_slice(&okm[..32]);
    chacha_nonce.copy_from_slice(&okm[32..44]);
    hmac_key.copy_from_slice(&okm[44..76]);
    (chacha_key, chacha_nonce, hmac_key)
}

/// NIP-44 padding: 2-byte big-endian plaintext length (6 bytes, `0x0000` +
/// u32, when ≥ 65536), zero-padded to the next power-of-two-ish chunk.
fn calc_padded_len(unpadded: usize) -> usize {
    let unpadded: u64 = unpadded as u64;
    if unpadded <= 32 {
        return 32;
    }
    let next_power = 1u64 << ((63 - (unpadded - 1).leading_zeros()) + 1);
    let chunk = if next_power <= 256 { 32 } else { next_power / 8 };
    (chunk * ((unpadded - 1) / chunk + 1)) as usize
}

fn pad(plaintext: &[u8]) -> Result<Vec<u8>> {
    if plaintext.is_empty() || plaintext.len() > u32::MAX as usize {
        bail!("invalid plaintext length");
    }
    let prefix: Vec<u8> = if plaintext.len() >= 65536 {
        let mut p = vec![0u8, 0u8];
        p.extend_from_slice(&(plaintext.len() as u32).to_be_bytes());
        p
    } else {
        (plaintext.len() as u16).to_be_bytes().to_vec()
    };
    let padded_len = calc_padded_len(plaintext.len());
    let mut out = Vec::with_capacity(prefix.len() + padded_len);
    out.extend_from_slice(&prefix);
    out.extend_from_slice(plaintext);
    out.resize(prefix.len() + padded_len, 0);
    Ok(out)
}

fn unpad(padded: &[u8]) -> Result<Vec<u8>> {
    if padded.len() < 2 {
        bail!("invalid padding");
    }
    let first_two = u16::from_be_bytes([padded[0], padded[1]]);
    let (unpadded_len, prefix_len) = if first_two == 0 {
        if padded.len() < 6 {
            bail!("invalid padding");
        }
        (u32::from_be_bytes([padded[2], padded[3], padded[4], padded[5]]) as usize, 6)
    } else {
        (first_two as usize, 2)
    };
    if unpadded_len == 0
        || padded.len() < prefix_len + unpadded_len
        || padded.len() != prefix_len + calc_padded_len(unpadded_len)
    {
        bail!("invalid padding");
    }
    Ok(padded[prefix_len..prefix_len + unpadded_len].to_vec())
}

/// Constant-time equality (length differs → false; content comparison never
/// short-circuits on a byte).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// NIP-44 v2 encrypt: returns `base64(0x02 || nonce || ciphertext || mac)`.
pub fn nip44_encrypt(secret_hex: &str, peer_pubkey_hex: &str, plaintext: &str) -> Result<String> {
    let ck = conversation_key(secret_hex, peer_pubkey_hex)?;
    let mut nonce = [0u8; 32];
    getrandom::getrandom(&mut nonce).context("OS RNG")?;
    let (chacha_key, chacha_nonce, hmac_key) = message_keys(&ck, &nonce);

    let mut padded = pad(plaintext.as_bytes())?;
    ChaCha20::new(&chacha_key.into(), &chacha_nonce.into()).apply_keystream(&mut padded);

    let mut mac = <HmacSha256 as Mac>::new_from_slice(&hmac_key).expect("hmac accepts any key len");
    mac.update(&nonce);
    mac.update(&padded);
    let tag = mac.finalize().into_bytes();

    let mut out = Vec::with_capacity(1 + 32 + padded.len() + 32);
    out.push(NIP44_VERSION);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&padded);
    out.extend_from_slice(&tag);
    Ok(BASE64.encode(&out))
}

/// NIP-44 v2 decrypt of a `base64(0x02 || …)` payload.
pub fn nip44_decrypt(secret_hex: &str, peer_pubkey_hex: &str, payload: &str) -> Result<String> {
    if payload.starts_with('#') {
        bail!("unknown NIP-44 version (non-base64 payload)");
    }
    let data = BASE64
        .decode(payload.trim())
        .context("payload is not base64")?;
    if payload.len() < NIP44_MIN_B64_LEN || data.len() < NIP44_MIN_PAYLOAD_LEN {
        bail!("invalid NIP-44 payload size");
    }
    if data[0] != NIP44_VERSION {
        bail!("unknown NIP-44 version {}", data[0]);
    }
    let nonce: [u8; 32] = data[1..33].try_into().expect("slice is 32");
    let ciphertext = &data[33..data.len() - 32];
    let mac_bytes = &data[data.len() - 32..];

    let ck = conversation_key(secret_hex, peer_pubkey_hex)?;
    let (chacha_key, chacha_nonce, hmac_key) = message_keys(&ck, &nonce);

    let mut mac = <HmacSha256 as Mac>::new_from_slice(&hmac_key).expect("hmac accepts any key len");
    mac.update(&nonce);
    mac.update(ciphertext);
    let expected = mac.finalize().into_bytes();
    if !ct_eq(&expected, mac_bytes) {
        bail!("invalid NIP-44 MAC");
    }

    let mut buf = ciphertext.to_vec();
    ChaCha20::new(&chacha_key.into(), &chacha_nonce.into()).apply_keystream(&mut buf);
    let plaintext = unpad(&buf)?;
    String::from_utf8(plaintext).context("decrypted payload is not UTF-8")
}

// ── NIP-04 (deprecated transport, still spoken by real clients) ────────────

/// NIP-04 encrypt: AES-256-CBC, key = raw ECDH x-coordinate (unhashed — the
/// spec's quirk), output `<base64 ct>?iv=<base64 iv>`.
pub fn nip04_encrypt(secret_hex: &str, peer_pubkey_hex: &str, plaintext: &str) -> Result<String> {
    use aes::cipher::{BlockEncryptMut, KeyIvInit};
    type Enc = cbc::Encryptor<aes::Aes256>;

    let key = shared_x(secret_hex, peer_pubkey_hex)?;
    let mut iv = [0u8; 16];
    getrandom::getrandom(&mut iv).context("OS RNG")?;
    let ct = Enc::new(&key.into(), &iv.into()).encrypt_padded_vec_mut::<aes::cipher::block_padding::Pkcs7>(plaintext.as_bytes());
    Ok(format!("{}?iv={}", BASE64.encode(&ct), BASE64.encode(iv)))
}

/// NIP-04 decrypt of `<base64 ct>?iv=<base64 iv>`.
pub fn nip04_decrypt(secret_hex: &str, peer_pubkey_hex: &str, payload: &str) -> Result<String> {
    use aes::cipher::{BlockDecryptMut, KeyIvInit};
    type Dec = cbc::Decryptor<aes::Aes256>;

    let (ct_b64, iv_b64) = payload
        .trim()
        .split_once("?iv=")
        .ok_or_else(|| anyhow::anyhow!("not a NIP-04 payload (no iv)"))?;
    let ct = BASE64.decode(ct_b64).context("bad NIP-04 ciphertext base64")?;
    let iv: [u8; 16] = BASE64
        .decode(iv_b64)
        .context("bad NIP-04 iv base64")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("NIP-04 iv must be 16 bytes"))?;
    let key = shared_x(secret_hex, peer_pubkey_hex)?;
    let pt = Dec::new(&key.into(), &iv.into())
        .decrypt_padded_vec_mut::<aes::cipher::block_padding::Pkcs7>(&ct)
        .map_err(|_| anyhow::anyhow!("NIP-04 decryption failed"))?;
    String::from_utf8(pt).context("decrypted payload is not UTF-8")
}

/// URL-safe base64 for keys that cross the JNI boundary — unused by the
/// protocol but handy for the Kotlin side; keep the engine in one place.
pub fn b64_url(data: &[u8]) -> String {
    BASE64_URL.encode(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── official NIP-44 vectors (paulmillr/nip44 nip44.vectors.json) ──────

    #[test]
    fn nip44_official_conversation_keys() {
        let vectors: &[(&str, &str, &str)] = &[
            ("315e59ff51cb9209768cf7da80791ddcaae56ac9775eb25b6dee1234bc5d2268", "c2f9d9948dc8c7c38321e4b85c8558872eafa0641cd269db76848a6073e69133", "3dfef0ce2a4d80a25e7a328accf73448ef67096f65f79588e358d9a0eb9013f1"),
            ("98a5902fd67518a0c900f0fb62158f278f94a21d6f9d33d30cd3091195500311", "aae65c15f98e5e677b5050de82e3aba47a6fe49b3dab7863cf35d9478ba9f7d1", "9c00b769d5f54d02bf175b7284a1cbd28b6911b06cda6666b2243561ac96bad7"),
            ("86ae5ac8034eb2542ce23ec2f84375655dab7f836836bbd3c54cefe9fdc9c19f", "59f90272378089d73f1339710c02e2be6db584e9cdbe86eed3578f0c67c23585", "19f934aafd3324e8415299b64df42049afaa051c71c98d0aa10e1081f2e3e2ba"),
            // sec1 == pub2 (ECDH with self)
            ("0000000000000000000000000000000000000000000000000000000000000001", "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798", "3b4610cb7189beb9cc29eb3716ecc6102f1247e8f3101a03a1787d8908aeb54e"),
        ];
        for (sec1, pub2, expected) in vectors {
            let ck = conversation_key(sec1, pub2).unwrap();
            assert_eq!(hex::encode(ck), *expected);
        }
    }

    #[test]
    fn nip44_official_message_keys() {
        let ck_bytes: [u8; 32] = hex::decode("a1a3d60f3470a8612633924e91febf96dc5366ce130f658b1f0fc652c20b3b54")
            .unwrap()
            .try_into()
            .unwrap();
        let vectors: &[(&str, &str, &str, &str)] = &[
            ("e1e6f880560d6d149ed83dcc7e5861ee62a5ee051f7fde9975fe5d25d2a02d72", "f145f3bed47cb70dbeaac07f3a3fe683e822b3715edb7c4fe310829014ce7d76", "c4ad129bb01180c0933a160c", "027c1db445f05e2eee864a0975b0ddef5b7110583c8c192de3732571ca5838c4"),
            ("ea6eb84cac23c5c1607c334e8bdf66f7977a7e374052327ec28c6906cbe25967", "ff68db24b34fa62c78ac5ffeeaf19533afaedf651fb6a08384e46787f6ce94be", "50bb859aa2dde938cc49ec7a", "06ff32e1f7b29753a727d7927b25c2dd175aca47751462d37a2039023ec6b5a6"),
        ];
        for (nonce_h, ck_exp, cn_exp, hk_exp) in vectors {
            let nonce: [u8; 32] = hex::decode(nonce_h).unwrap().try_into().unwrap();
            let (chacha_key, chacha_nonce, hmac_key) = message_keys(&ck_bytes, &nonce);
            assert_eq!(hex::encode(chacha_key), *ck_exp);
            assert_eq!(hex::encode(chacha_nonce), *cn_exp);
            assert_eq!(hex::encode(hmac_key), *hk_exp);
        }
    }

    #[test]
    fn nip44_offical_padded_len() {
        let vectors: &[(usize, usize)] = &[
            (16, 32), (32, 32), (33, 64), (37, 64), (45, 64), (49, 64), (64, 64),
            (65, 96), (100, 128), (111, 128), (200, 224), (250, 256), (320, 320),
            (383, 384), (384, 384), (400, 448), (500, 512), (512, 512), (515, 640),
            (700, 768), (800, 896), (900, 1024), (1020, 1024), (65536, 65536),
        ];
        for (unpadded, padded) in vectors {
            assert_eq!(calc_padded_len(*unpadded), *padded, "unpadded {unpadded}");
        }
    }

    #[test]
    fn nip44_official_encrypt_vectors() {
        // (sec1, sec2, nonce, plaintext, payload) — decrypt with the peer's
        // view (sec2, pub(sec1)) so this also proves key symmetry.
        let vectors: &[(&str, &str, &str, &str, &str)] = &[
            ("0000000000000000000000000000000000000000000000000000000000000001",
             "0000000000000000000000000000000000000000000000000000000000000002",
             "0000000000000000000000000000000000000000000000000000000000000001",
             "a",
             "AgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABee0G5VSK0/9YypIObAtDKfYEAjD35uVkHyB0F4DwrcNaCXlCWZKaArsGrY6M9wnuTMxWfp1RTN9Xga8no+kF5Vsb"),
            ("0000000000000000000000000000000000000000000000000000000000000002",
             "0000000000000000000000000000000000000000000000000000000000000001",
             "f00000000000000000000000000000f00000000000000000000000000000000f",
             "🍕🫃",
             "AvAAAAAAAAAAAAAAAAAAAPAAAAAAAAAAAAAAAAAAAAAPSKSK6is9ngkX2+cSq85Th16oRTISAOfhStnixqZziKMDvB0QQzgFZdjLTPicCJaV8nDITO+QfaQ61+KbWQIOO2Yj"),
            ("5c0c523f52a5b6fad39ed2403092df8cebc36318b39383bca6c00808626fab3a",
             "4b22aa260e4acb7021e32f38a6cdf4b673c6a277755bfce287e370c924dc936d",
             "b635236c42db20f021bb8d1cdff5ca75dd1a0cc72ea742ad750f33010b24f73b",
             "表ポあA鷗ŒéＢ逍Üßªąñ丂㐀𠀀",
             "ArY1I2xC2yDwIbuNHN/1ynXdGgzHLqdCrXUPMwELJPc7s7JqlCMJBAIIjfkpHReBPXeoMCyuClwgbT419jUWU1PwaNl4FEQYKCDKVJz+97Mp3K+Q2YGa77B6gpxB/lr1QgoqpDf7wDVrDmOqGoiPjWDqy8KzLueKDcm9BVP8xeTJIxs="),
            ("eba1687cab6a3101bfc68fd70f214aa4cc059e9ec1b79fdb9ad0a0a4e259829f",
             "dff20d262bef9dfd94666548f556393085e6ea421c8af86e9d333fa8747e94b3",
             "2180b52ae645fcf9f5080d81b1f0b5d6f2cd77ff3c986882bb549158462f3407",
             "( ͡° ͜ʖ ͡°)",
             "AiGAtSrmRfz59QgNgbHwtdbyzXf/PJhogrtUkVhGLzQHv4qhKQwnFQ54OjVMgqCea/Vj0YqBSdhqNR777TJ4zIUk7R0fnizp6l1zwgzWv7+ee6u+0/89KIjY5q1wu6inyuiv"),
            ("d5633530f5bcfebceb5584cfbbf718a30df0751b729dd9a789b9f30c0587d74e",
             "b74e6a341fb134127272b795a08b59250e5fa45a82a2eb4095e4ce9ed5f5e214",
             "a3e219242d85465e70adcd640b564b3feff57d2ef8745d5e7a0663b2dccceb54",
             "🙈 🙉 🙊 0️⃣ 1️⃣ 2️⃣ 3️⃣ 4️⃣ 5️⃣ 6️⃣ 7️⃣ 8️⃣ 9️⃣ 🔟 Powerلُلُصّبُلُلصّبُررً ॣ ॣh ॣ ॣ冗",
             "AqPiGSQthUZecK3NZAtWSz/v9X0u+HRdXnoGY7LczOtUf05aMF89q1FLwJvaFJYICZoMYgRJHFLwPiOHce7fuAc40kX0wXJvipyBJ9HzCOj7CgtnC1/cmPCHR3s5AIORmroBWglm1LiFMohv1FSPEbaBD51VXxJa4JyWpYhreSOEjn1wd0lMKC9b+osV2N2tpbs+rbpQem2tRen3sWflmCqjkG5VOVwRErCuXuPb5+hYwd8BoZbfCrsiAVLd7YT44dRtKNBx6rkabWfddKSLtreHLDysOhQUVOp/XkE7OzSkWl6sky0Hva6qJJ/V726hMlomvcLHjE41iKmW2CpcZfOedg=="),
        ];
        for (sec1, sec2, nonce_hex, plaintext, payload) in vectors {
            // Encrypt from A to B with the fixed nonce must reproduce the
            // official payload byte-for-byte.
            let pub1 = pubkey_hex(sec1).unwrap();
            let made = {
                let ck = conversation_key(sec1, &pubkey_hex(sec2).unwrap()).unwrap();
                let nonce: [u8; 32] = hex::decode(nonce_hex).unwrap().try_into().unwrap();
                let (chacha_key, chacha_nonce, hmac_key) = message_keys(&ck, &nonce);
                let mut padded = pad(plaintext.as_bytes()).unwrap();
                ChaCha20::new(&chacha_key.into(), &chacha_nonce.into()).apply_keystream(&mut padded);
                let mut mac = <HmacSha256 as Mac>::new_from_slice(&hmac_key).unwrap();
                mac.update(&nonce);
                mac.update(&padded);
                let tag = mac.finalize().into_bytes();
                let mut out = vec![NIP44_VERSION];
                out.extend_from_slice(&nonce);
                out.extend_from_slice(&padded);
                out.extend_from_slice(&tag);
                BASE64.encode(&out)
            };
            assert_eq!(&made, payload, "encrypt vector for {plaintext:?}");

            // Decrypt from B's view of A (key-role symmetry).
            let got = nip44_decrypt(sec2, &pub1, payload).unwrap();
            assert_eq!(got, *plaintext);
        }
    }

    #[test]
    fn nip44_round_trip_and_failures() {
        let sk_a = generate_secret().unwrap();
        let sk_b = generate_secret().unwrap();
        let pub_b = pubkey_hex(&sk_b).unwrap();
        let pub_a = pubkey_hex(&sk_a).unwrap();

        let msg = "hello, remote signer";
        let payload = nip44_encrypt(&sk_a, &pub_b, msg).unwrap();
        assert_eq!(nip44_decrypt(&sk_b, &pub_a, &payload).unwrap(), msg);

        // Round-trip long content across the 65536 prefix boundary.
        let long = "x".repeat(70_000);
        let payload = nip44_encrypt(&sk_a, &pub_b, &long).unwrap();
        assert_eq!(nip44_decrypt(&sk_b, &pub_a, &payload).unwrap(), long);

        // Wrong peer key must fail the MAC, not return garbage.
        let stranger = generate_secret().unwrap();
        assert!(nip44_decrypt(&sk_b, &pub_b, &payload).is_err());
        let _ = stranger;

        // Tampered payload fails.
        let payload = nip44_encrypt(&sk_a, &pub_b, msg).unwrap();
        let mut tampered = BASE64.decode(&payload).unwrap();
        let n = tampered.len();
        tampered[n - 1] ^= 0x01;
        assert!(nip44_decrypt(&sk_b, &pub_a, &BASE64.encode(&tampered)).is_err());

        // Truncated payload fails.
        assert!(nip44_decrypt(&sk_b, &pub_a, "AAAA").is_err());
    }

    // ── BIP-340 official vectors (github.com/bitcoin/bips test vectors) ────

    #[test]
    fn bip340_reference_sign_vectors() {
        // (seckey, pubkey, aux, msg, expected sig) — indices 0/1/2 of the
        // official BIP-340 `bip-0340/test-vectors.csv` "should sign" set,
        // transcribed from the file itself (x(3G) additionally verified
        // by independent scalar-math in the review notes for this commit).
        let vectors: &[(&str, &str, &str, &str, &str)] = &[
            ("0000000000000000000000000000000000000000000000000000000000000003",
             "F9308A019258C31049344F85F89D5229B531C845836F99B08601F113BCE036F9",
             "0000000000000000000000000000000000000000000000000000000000000000",
             "0000000000000000000000000000000000000000000000000000000000000000",
             "E907831F80848D1069A5371B402410364BDF1C5F8307B0084C55F1CE2DCA821525F66A4A85EA8B71E482A74F382D2CE5EBEEE8FDB2172F477DF4900D310536C0"),
            ("B7E151628AED2A6ABF7158809CF4F3C762E7160F38B4DA56A784D9045190CFEF",
             "DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659",
             "0000000000000000000000000000000000000000000000000000000000000001",
             "243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89",
             "6896BD60EEAE296DB48A229FF71DFE071BDE413E6D43F917DC8DCF8C78DE33418906D11AC976ABCCB20B091292BFF4EA897EFCB639EA871CFA95F6DE339E4B0A"),
            ("C90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74020BBEA63B14E5C9",
             "DD308AFEC5777E13121FA72B9CC1B7CC0139715309B086C960E18FD969774EB8",
             "C87AA53824B4D7AE2EB035A2B5BBBCCC080E76CDC6D1692C4B0B62D798E6D906",
             "7E2D58D8B3BCDF1ABADEC7829054F90DDA9805AAB56C77333024B9D0A508B75C",
             "5831AAEED7B44BB74E5EAB94BA9D4294C49BCF2A60728D8B4C200F50DD313C1BAB745879A5AD954A72C45A91C3A51D3C7ADEA98D82F8481E0E1E03674A6F3FB7"),
        ];
        for (sk_hex, pk_hex, aux_hex, msg_hex, sig_hex) in vectors {
            let sk_bytes = hex::decode(sk_hex).unwrap();
            let sk = SecretKey::from_slice(&sk_bytes).unwrap();
            let kp = Keypair::from_secret_key(&Secp256k1::new(), &sk);
            assert_eq!(hex::encode(kp.public_key().x_only_public_key().0.serialize()).to_uppercase(), *pk_hex);

            let msg: [u8; 32] = hex::decode(msg_hex).unwrap().try_into().unwrap();
            let aux: [u8; 32] = hex::decode(aux_hex).unwrap().try_into().unwrap();
            let sig = Secp256k1::new().sign_schnorr_with_aux_rand(
                &Message::from_digest(msg),
                &kp,
                &aux,
            );
            assert_eq!(hex::encode(sig.serialize()).to_uppercase(), *sig_hex);
        }
    }

    #[test]
    fn event_signing_round_trip() {
        let sk = generate_secret().unwrap();
        let unsigned = r#"{"kind":22242,"content":"{\"challenge\":\"abc123\"}","tags":[["relay","ws://127.0.0.1:7777"]],"created_at":1725100000}"#;
        let signed = sign_event(&sk, unsigned).unwrap();
        verify_event(&signed).unwrap();

        let ev: serde_json::Value = serde_json::from_str(&signed).unwrap();
        assert_eq!(ev["kind"], 22242);
        assert_eq!(ev["pubkey"], pubkey_hex(&sk).unwrap());
        // Tampering with content breaks the id, which breaks verification.
        let mut tampered = ev.clone();
        tampered["content"] = serde_json::Value::String("nope".into());
        assert!(verify_event(&tampered.to_string()).is_err());
    }

    #[test]
    fn connect_uri_parsing() {
        let uri = "nostrconnect://83f3b2ae6aa368e8275397b9c26cf550101d63ebaab900d19dd4a4429f5ad8f5?relay=wss%3A%2F%2Frelay1.example.com&perms=nip44_encrypt%2Csign_event%3A22242&name=My+Client&secret=0s8j2djs&relay=ws%3A%2F%2F192.168.1.20%3A7777";
        let info = parse_connect_uri(uri).unwrap();
        assert_eq!(info.client_pubkey, "83f3b2ae6aa368e8275397b9c26cf550101d63ebaab900d19dd4a4429f5ad8f5");
        assert_eq!(
            info.relays,
            vec!["wss://relay1.example.com", "ws://192.168.1.20:7777"]
        );
        assert_eq!(info.secret, "0s8j2djs");
        assert_eq!(info.perms, vec!["nip44_encrypt", "sign_event:22242"]);
        assert_eq!(info.name, "My Client");

        // npub client keys and unknown params tolerated — the npub is
        // generated through our own encoder so the test carries no
        // hand-transcribed bech32 string.
        let sk1 = "0000000000000000000000000000000000000000000000000000000000000001";
        let npub = npub_from_pubkey(&pubkey_hex(sk1).unwrap()).unwrap();
        let pubkey = pubkey_from_any(&npub).unwrap();
        let uri = format!("nostrconnect://{npub}?relay=wss://r&secret=s&future=1");
        let info = parse_connect_uri(&uri).unwrap();
        assert_eq!(info.client_pubkey, pubkey);
        assert_eq!(info.relays, vec!["wss://r"]);

        assert!(parse_connect_uri("bunker://abc?relay=wss://r&secret=s").is_err());
        assert!(parse_connect_uri("nostrconnect://zz?relay=wss://r&secret=s").is_err());
        assert!(parse_connect_uri("nostrconnect://83f3b2ae6aa368e8275397b9c26cf550101d63ebaab900d19dd4a4429f5ad8f5?name=x").is_err());
    }

    #[test]
    fn nip04_round_trip_and_cross_check() {
        let sk_a = generate_secret().unwrap();
        let sk_b = generate_secret().unwrap();
        let pub_b = pubkey_hex(&sk_b).unwrap();
        let pub_a = pubkey_hex(&sk_a).unwrap();

        let payload = nip04_encrypt(&sk_a, &pub_b, "old client hello").unwrap();
        assert!(payload.contains("?iv="));
        assert_eq!(nip04_decrypt(&sk_b, &pub_a, &payload).unwrap(), "old client hello");

        // Wrong key must fail (PKCS#7 padding check) rather than return garbage.
        assert!(nip04_decrypt(&sk_a, &pub_a, &payload).is_err());
        assert!(nip04_decrypt(&sk_b, &pub_b, &payload).is_err());
        assert!(nip04_decrypt(&sk_b, &pub_a, "not-a-payload").is_err());
    }

    #[test]
    fn key_encoding_round_trip() {
        let sk = generate_secret().unwrap();
        let nsec = nsec_from_secret(&sk).unwrap();
        assert!(nsec.starts_with("nsec1"));
        assert_eq!(secret_from_nsec(&nsec).unwrap(), sk);
        assert_eq!(secret_from_any(&nsec).unwrap(), sk);
        assert_eq!(secret_from_any(&sk).unwrap(), sk);

        let pk = pubkey_hex(&sk).unwrap();
        let npub = npub_from_pubkey(&pk).unwrap();
        assert!(npub.starts_with("npub1"));
        assert_eq!(pubkey_from_any(&npub).unwrap(), pk);
        assert_eq!(pubkey_from_any(&pk).unwrap(), pk);

        // The famous even-y lift edge case: pubkey of sk=1 is x(G) (y is odd);
        // shared_x with oneself is exactly x(G) — pins the unhashed-x ECDH and
        // the even-parity lift in one assertion (x is invariant under y-negation,
        // so the lift is safe for NIP-44/NIP-04 keys).
        let g_x = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        assert_eq!(
            pubkey_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap(),
            g_x
        );
        assert_eq!(
            hex::encode(
                shared_x("0000000000000000000000000000000000000000000000000000000000000001", g_x).unwrap()
            ),
            g_x
        );
        assert!(secret_from_nsec("npub1").is_err());
    }

    /// The mesh ULA is a PURE function of the node's public key:
    /// `fd ‖ sha256(x-only pubkey)[0..15]` (fips identity/node_addr.rs →
    /// identity/address.rs). That is what makes "address by npub" work —
    /// Termux's fipssh helper, and any future DNS-style resolver, just
    /// computes what the fips daemon's DNS answers.
    #[test]
    fn npub_derives_the_same_mesh_ula_as_the_fips_identity() {
        for seed in [0x42u8, 0x07, 0x31] {
            // 0xff… would exceed the curve order — secret keys must be valid scalars.
            let secret = [seed; 32];
            let id = fips::Identity::from_secret_bytes(&secret).unwrap();
            let npub = id.npub();
            let expected = id.address().to_ipv6().to_string();

            let pubkey_hex = pubkey_from_any(&npub).unwrap();
            let pk = hex::decode(&pubkey_hex).unwrap();
            let mut hasher = Sha256::new();
            hasher.update(&pk);
            let hash = hasher.finalize();
            let mut ula = [0u8; 16];
            ula[0] = 0xfd;
            ula[1..].copy_from_slice(&hash[..15]);
            assert_eq!(std::net::Ipv6Addr::from(ula).to_string(), expected, "npub {npub}");
        }
    }
}
