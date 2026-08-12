//! JNI surface for `com.archipelago.app.fips.FipsNative` — JSON over strings,
//! no codegen (the myco / nostr-vpn embedding pattern). Errors come back as
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
