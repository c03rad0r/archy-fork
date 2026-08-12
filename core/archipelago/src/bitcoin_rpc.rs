//! Bitcoin RPC credential management.
//!
//! Uses `rpcauth` in bitcoin.conf (salted hash — no plaintext in config or CLI).
//! The actual password is stored in `/var/lib/archipelago/secrets/bitcoin-rpc-password`
//! and stays stable across reboots, restarts, and deploys.

use tokio::sync::OnceCell;
use tracing::debug;

const SECRETS_PATH: &str = "/var/lib/archipelago/secrets/bitcoin-rpc-password";
const RPC_USER: &str = "archipelago";

static CACHED_PASSWORD: OnceCell<String> = OnceCell::const_new();

/// Read the Bitcoin RPC password from the secrets file.
/// Falls back to env var (dev), then generates and persists a random password.
async fn read_password() -> String {
    // 1. Secrets file (production)
    if let Ok(pass) = tokio::fs::read_to_string(SECRETS_PATH).await {
        let pass = pass.trim().to_string();
        if !pass.is_empty() {
            debug!("Bitcoin RPC password loaded from secrets file");
            return pass;
        }
    }

    // 2. Environment variable (dev)
    if let Ok(pass) = std::env::var("BITCOIN_RPC_PASSWORD") {
        if !pass.is_empty() {
            debug!("Bitcoin RPC password loaded from env var");
            return pass;
        }
    }

    // 3. Generate and persist (first boot)
    let random_pass = generate_random_password();
    if let Some(parent) = std::path::Path::new(SECRETS_PATH).parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    match tokio::fs::write(SECRETS_PATH, &random_pass).await {
        Ok(_) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = tokio::fs::set_permissions(
                    SECRETS_PATH,
                    std::fs::Permissions::from_mode(0o600),
                )
                .await;
            }
            debug!("Bitcoin RPC password generated and saved");
        }
        Err(e) => {
            tracing::warn!("Failed to save Bitcoin RPC password: {}", e);
        }
    }
    random_pass
}

/// Generate a cryptographically random password (32 hex chars).
///
/// KEY-05: this is the node's Bitcoin RPC credential, so the source is named and
/// the 16-byte draw is guarded. It returns a bare `String` and its caller
/// (`read_password`) is a `OnceCell` initialiser that also returns a bare
/// `String`, so a degenerate draw aborts rather than propagating — the condition
/// means the kernel CSPRNG is broken, and a predictable Bitcoin RPC password on
/// a node that also serves LAN traffic is worse than a loud stop.
fn generate_random_password() -> String {
    let mut bytes = [0u8; 16];
    crate::entropy::draw_key_bytes(&mut rand::rngs::OsRng, &mut bytes).unwrap_or_else(|e| {
        panic!("refusing to generate a Bitcoin RPC password from degenerate entropy: {e} (KEY-05)")
    });
    hex::encode(bytes)
}

/// Get Bitcoin RPC credentials (user, password). Cached after first call.
pub async fn bitcoin_rpc_credentials() -> (String, String) {
    let pass = CACHED_PASSWORD
        .get_or_init(|| async { read_password().await })
        .await;
    (RPC_USER.to_string(), pass.clone())
}
