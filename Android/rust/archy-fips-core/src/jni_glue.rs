//! JNI surface for `com.archipelago.app.fips.FipsNative` and
//! `com.archipelago.app.NativeCore` — JSON over strings, no codegen (the
//! myco / nostr-vpn embedding pattern). Errors come back as
//! `{"error": "…"}` so Kotlin never sees a raw exception from native code.

use std::sync::Once;

use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jint, jstring};
use jni::JNIEnv;

use crate::mesh;

static LOG_INIT: Once = Once::new();

fn init_logging() {
    LOG_INIT.call_once(|| {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;
        let _ = tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new("info"))
            .with(paranoid_android::layer("archy-fips"))
            .try_init();
    });
}

fn jstr(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(|s| s.into()).unwrap_or_default()
}

fn out(env: &JNIEnv, s: String) -> jstring {
    env.new_string(s)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

fn err_json(e: impl std::fmt::Display) -> String {
    serde_json::json!({ "error": e.to_string() }).to_string()
}

fn identity_json(info: &mesh::IdentityInfo) -> String {
    serde_json::json!({
        "secret": info.secret_hex,
        "npub": info.npub,
        "address": info.address,
    })
    .to_string()
}

/// Kotlin: `external fun generateIdentity(): String`
#[no_mangle]
pub extern "system" fn Java_com_archipelago_app_fips_FipsNative_generateIdentity(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    init_logging();
    let json = match mesh::generate_identity() {
        Ok(info) => identity_json(&info),
        Err(e) => err_json(e),
    };
    out(&env, json)
}

/// Kotlin: `external fun deriveIdentity(secret: String): String`
#[no_mangle]
pub extern "system" fn Java_com_archipelago_app_fips_FipsNative_deriveIdentity(
    mut env: JNIEnv,
    _class: JClass,
    secret: JString,
) -> jstring {
    init_logging();
    let secret = jstr(&mut env, &secret);
    let json = match mesh::derive_identity(&secret) {
        Ok(info) => identity_json(&info),
        Err(e) => err_json(e),
    };
    out(&env, json)
}

/// Kotlin: `external fun start(secret: String, peersJson: String, tunFd: Int, listenPort: Int): String`
/// Returns `{"npub": "...", "address": "..."}` or `{"error": "..."}`.
/// `listenPort` 0 = outbound-only; non-zero = fixed UDP bind (party mode).
#[no_mangle]
pub extern "system" fn Java_com_archipelago_app_fips_FipsNative_start(
    mut env: JNIEnv,
    _class: JClass,
    secret: JString,
    peers_json: JString,
    tun_fd: jint,
    listen_port: jint,
) -> jstring {
    init_logging();
    let secret = jstr(&mut env, &secret);
    let peers = jstr(&mut env, &peers_json);
    let listen_port = u16::try_from(listen_port).unwrap_or(0);
    let json = match mesh::start(&secret, &peers, tun_fd, listen_port) {
        Ok((npub, address)) => {
            serde_json::json!({ "npub": npub, "address": address }).to_string()
        }
        Err(e) => err_json(e),
    };
    out(&env, json)
}

/// Kotlin: `external fun stop()`
#[no_mangle]
pub extern "system" fn Java_com_archipelago_app_fips_FipsNative_stop(
    _env: JNIEnv,
    _class: JClass,
) {
    mesh::stop();
}

/// Kotlin: `external fun isRunning(): Boolean`
#[no_mangle]
pub extern "system" fn Java_com_archipelago_app_fips_FipsNative_isRunning(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    mesh::is_running() as jboolean
}

/// Kotlin: `external fun statusJson(): String`
#[no_mangle]
pub extern "system" fn Java_com_archipelago_app_fips_FipsNative_statusJson(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    out(&env, mesh::status_json())
}

// ─────────────────────────────────────────────────────────────────────────────
// com.archipelago.app.NativeCore — companion backup (#128) and NIP-46 remote
// signer crypto (#139). Same library, JSON-over-strings contract.
// ─────────────────────────────────────────────────────────────────────────────

/// Kotlin: `external fun backupEncrypt(payload: String, passphrase: String): String`
/// Returns the ADR-005 envelope JSON or `{"error": …}`.
#[no_mangle]
pub extern "system" fn Java_com_archipelago_app_NativeCore_backupEncrypt(
    mut env: JNIEnv,
    _class: JClass,
    payload: JString,
    passphrase: JString,
) -> jstring {
    init_logging();
    let payload = jstr(&mut env, &payload);
    let passphrase = jstr(&mut env, &passphrase);
    let json = match crate::backup::encrypt(&payload, &passphrase) {
        Ok(envelope) => envelope,
        Err(e) => err_json(e),
    };
    out(&env, json)
}

/// Kotlin: `external fun backupDecrypt(envelope: String, passphrase: String): String`
/// Returns the decrypted payload JSON or `{"error": …}`.
#[no_mangle]
pub extern "system" fn Java_com_archipelago_app_NativeCore_backupDecrypt(
    mut env: JNIEnv,
    _class: JClass,
    envelope: JString,
    passphrase: JString,
) -> jstring {
    init_logging();
    let envelope = jstr(&mut env, &envelope);
    let passphrase = jstr(&mut env, &passphrase);
    let json = match crate::backup::decrypt(&envelope, &passphrase) {
        Ok(payload) => payload,
        Err(e) => err_json(e),
    };
    out(&env, json)
}

/// Kotlin: `external fun nostrGenerateSecret(): String`
/// Returns `{"secret": hex, "pubkey": hex, "npub": …, "nsec": …}` or `{"error": …}`.
#[no_mangle]
pub extern "system" fn Java_com_archipelago_app_NativeCore_nostrGenerateSecret(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    init_logging();
    let json = match crate::nostr::generate_secret() {
        Ok(secret) => nostr_key_info_json(&secret),
        Err(e) => err_json(e),
    };
    out(&env, json)
}

/// Kotlin: `external fun nostrSecretFromAny(secret: String): String`
/// Accepts hex or `nsec…`; returns key-info JSON or `{"error": …}`.
#[no_mangle]
pub extern "system" fn Java_com_archipelago_app_NativeCore_nostrSecretFromAny(
    mut env: JNIEnv,
    _class: JClass,
    secret: JString,
) -> jstring {
    init_logging();
    let secret = jstr(&mut env, &secret);
    let json = match crate::nostr::secret_from_any(&secret) {
        Ok(hex) => nostr_key_info_json(&hex),
        Err(e) => err_json(e),
    };
    out(&env, json)
}

fn nostr_key_info_json(secret_hex: &str) -> String {
    match (
        crate::nostr::pubkey_hex(secret_hex),
        crate::nostr::npub_from_pubkey(&crate::nostr::pubkey_hex(secret_hex).unwrap_or_default()),
        crate::nostr::nsec_from_secret(secret_hex),
    ) {
        (Ok(pubkey), Ok(npub), Ok(nsec)) => serde_json::json!({
            "secret": secret_hex,
            "pubkey": pubkey,
            "npub": npub,
            "nsec": nsec,
        })
        .to_string(),
        (e, _, _) => err_json(e.unwrap_err()),
    }
}

/// Kotlin: `external fun nostrParseConnectUri(uri: String): String`
/// Returns the parsed URI fields or `{"error": …}`.
#[no_mangle]
pub extern "system" fn Java_com_archipelago_app_NativeCore_nostrParseConnectUri(
    mut env: JNIEnv,
    _class: JClass,
    uri: JString,
) -> jstring {
    init_logging();
    let uri = jstr(&mut env, &uri);
    let json = match crate::nostr::parse_connect_uri(&uri) {
        Ok(info) => info.to_json().to_string(),
        Err(e) => err_json(e),
    };
    out(&env, json)
}

/// Kotlin: `external fun nostrSignEvent(secretHex: String, eventJson: String): String`
/// Returns the signed event JSON or `{"error": …}`. The approve/deny decision
/// is made in Kotlin BEFORE this is called — native code never signs unasked.
#[no_mangle]
pub extern "system" fn Java_com_archipelago_app_NativeCore_nostrSignEvent(
    mut env: JNIEnv,
    _class: JClass,
    secret_hex: JString,
    event_json: JString,
) -> jstring {
    init_logging();
    let secret = jstr(&mut env, &secret_hex);
    let event = jstr(&mut env, &event_json);
    let json = match crate::nostr::sign_event(&secret, &event) {
        Ok(signed) => signed,
        Err(e) => err_json(e),
    };
    out(&env, json)
}

macro_rules! nostr_cipher {
    ($name:ident, $doc:literal, $fn:path) => {
        #[doc = $doc]
        #[no_mangle]
        pub extern "system" fn $name(
            mut env: JNIEnv,
            _class: JClass,
            secret_hex: JString,
            peer_pub: JString,
            text: JString,
        ) -> jstring {
            init_logging();
            let secret = jstr(&mut env, &secret_hex);
            let peer = jstr(&mut env, &peer_pub);
            let text = jstr(&mut env, &text);
            let json = match $fn(&secret, &peer, &text) {
                Ok(out) => serde_json::json!({ "result": out }).to_string(),
                Err(e) => err_json(e),
            };
            out(&env, json)
        }
    };
}

nostr_cipher!(
    Java_com_archipelago_app_NativeCore_nostrNip44Encrypt,
    "Kotlin: `external fun nostrNip44Encrypt(secretHex: String, peerPub: String, plaintext: String): String` — returns `{\"result\": payload}` or `{\"error\": …}`.",
    crate::nostr::nip44_encrypt
);
nostr_cipher!(
    Java_com_archipelago_app_NativeCore_nostrNip44Decrypt,
    "Kotlin: `external fun nostrNip44Decrypt(secretHex: String, peerPub: String, payload: String): String`",
    crate::nostr::nip44_decrypt
);
nostr_cipher!(
    Java_com_archipelago_app_NativeCore_nostrNip04Encrypt,
    "Kotlin: `external fun nostrNip04Encrypt(secretHex: String, peerPub: String, plaintext: String): String`",
    crate::nostr::nip04_encrypt
);
nostr_cipher!(
    Java_com_archipelago_app_NativeCore_nostrNip04Decrypt,
    "Kotlin: `external fun nostrNip04Decrypt(secretHex: String, peerPub: String, payload: String): String`",
    crate::nostr::nip04_decrypt
);
