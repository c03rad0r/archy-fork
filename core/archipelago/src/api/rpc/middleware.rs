use crate::session::SessionStore;
use std::net::IpAddr;

/// Methods that do not require a valid session cookie.
///
/// `pub(crate)` (not just `pub(super)`) so `crate::assistant`'s test suite
/// can assert directly against the live list that the assistant RPC prefix
/// is never added to it (Phase-10 hard constraint) — see the re-export in
/// `api/rpc/mod.rs`. Read-visibility only; the list's contents and every
/// other visibility in this module are unchanged.
pub(crate) const UNAUTHENTICATED_METHODS: &[&str] = &[
    "auth.login",
    "auth.login.totp",
    "auth.login.backup",
    "auth.isOnboardingComplete",
    "auth.isSetup",
    "auth.setup",
    "auth.onboardingComplete",
    "health",
    // Server readiness check (Login.vue polls this before showing form)
    "server.echo",
    // Onboarding flow (before user has a session — DID creation, signing, backup)
    "node.did",
    "node.signChallenge",
    "node.nostr-pubkey",
    "node.createBackup",
    "identity.create",
    "identity.verify",
    "identity.resolve-did",
    // Seed management (onboarding — before user has a session)
    "seed.generate",
    "seed.verify",
    "seed.restore",
    "seed.save-encrypted",
    // Onboarding restore (before user account exists)
    "backup.restore-identity",
    // Inter-node RPC: called by federated peers over Tor, no session cookies
    "federation.peer-joined",
    "federation.peer-address-changed",
    "federation.peer-did-changed",
    "federation.get-state",
    // Fleet telemetry ingest: called by remote nodes posting reports
    "telemetry.ingest",
    // Media-path counters (P1). Read-only, non-sensitive aggregate byte/stream counts —
    // the same class as "health". It is deliberately NOT "monitoring.current" (which
    // exposes host + container detail and stays authenticated): this route returns four
    // monotonically-increasing integers and nothing that identifies a peer, an object or a
    // user. Adding it is what makes P1's mandated read-back possible without a session.
    "monitoring.media",
];

/// Methods whose responses can be cached for a few seconds.
pub(super) const CACHEABLE_METHODS: &[&str] = &["system.stats", "federation.list-nodes"];

/// Sanitize error messages before returning to clients.
/// Keeps user-facing validation errors but strips internal system details.
pub(super) fn sanitize_error_message(msg: &str) -> String {
    // Allow known validation errors through (these are user-actionable)
    let user_facing_prefixes = [
        "Invalid",
        "Missing",
        "Not found",
        "Already exists",
        "Rate limit",
        "Unauthorized",
        "Forbidden",
        "Not supported",
        "Requires",
        "requires",
        "must be",
        "cannot",
        "Password",
        // auth.changePassword verifies the existing node password before it
        // writes either the web hash or the optional Linux/SSH password. This
        // is safe, actionable validation text; masking it as an internal
        // failure sent operators to the server logs for a simple typo.
        "Current password is incorrect",
        // OTA apply/download errors are all operator-actionable ("download it
        // again", "download first") — sanitizing them to "Operation failed"
        // left users stuck with no idea what to do, and hid the "already
        // running" text the update UI matches on to join an in-flight apply
        // instead of showing a false failure. Every such message starts "Update".
        "Update",
        // The federation escalation sentinel. "Password" above does NOT cover
        // it — starts_with is case-sensitive and the sentinel is ALL-CAPS —
        // so the frontend's isPasswordRequired() never saw it and the
        // password prompt could never open. Net effect: no node could mint a
        // Trusted invite or promote a peer from the trust dropdown, fleet-wide
        // (2026-08-09). The sentinel is machine-read by the UI; it must pass
        // through verbatim.
        "PASSWORD_REQUIRED",
        // "Tor address not available. Tor may not be running." — the invite
        // handler's precondition, entirely user-actionable, was likewise
        // collapsing into the generic message.
        "Tor address not available",
        "Session",
        "Failed to pull",
        "Failed to start",
        "Failed to open channel",
        "Failed to close channel",
        "Failed to connect to peer",
        // App-install dependency errors (package/dependencies.rs) — masking
        // these left users retrying installs blind ("LND install failed" on a
        // fresh node was really "Bitcoin Knots isn't running yet")
        "LND requires",
        "ElectrumX requires",
        "BTCPay Server requires",
        "Mempool requires",
        "Container",
        "Image",
        // Wallet-actionable errors: masking "Insufficient balance: need 80
        // sats, have 0 sats" behind "Operation failed. Check server logs."
        // sent the operator to journalctl for a message that was written for
        // them in the first place (ecash send, 2026-07-22).
        "Insufficient balance",
        "Insufficient funds",
        // On-chain send/sweep refusals from LND ("Failed to send: your
        // on-chain balance is too small or still unconfirmed to sweep…").
        // Masking sent the operator to journalctl again (a test node
        // sweep, 2026-08-06) — same lesson as the two above.
        "Failed to send",
        // A frontend newer than the daemon calls methods it doesn't have.
        // Masked, this reads as "the feature is broken" instead of "this
        // node needs its update" — hit live the moment the .126 LoRa panel
        // was deployed ahead of its binary (2026-08-06).
        "Unknown method",
        // RNode RF settings validation (mesh::rnode_settings::validate) —
        // every one names the offending field and its legal range, which is
        // the entire point of validating before touching the radio.
        "frequency ",
        "bandwidth ",
        "spreading factor ",
        "coding rate ",
        "tx power ",
        "airtime_limit_short",
        "airtime_limit_long",
        "port must be an absolute",
        "Invalid settings",
        "Missing 'settings'",
        // Mesh preconditions the operator can act on directly.
        "Mesh service not running",
        "No mesh device connected",
        "Mesh listener not running",
        "MeshCore radios have no remote reboot",
        "Radio state read-back",
        "The radio daemon did not answer",
        "The radio did not acknowledge",
        "RNode interface is disabled",
        // Lightning payment failures carry LND's reason ("invoice expired.
        // Valid until …", "no route", …) — the user can act on every one of
        // them, and masking sent the operator to journalctl (invoice-expired
        // send, 2026-07-23).
        "Payment failed",
        "Invalid payment request",
        "Missing 'payment_request'",
        "Your Lightning node is still finishing",
        "Bitcoin address",
        "No router",
        "No OpenWrt",
        "No space left",
        "Not enough flash",
        "Not enough space",
        "TollGate installation failed",
        "No pre-built TollGate",
        "opkg not found",
        "apk update failed",
        "No wireless interface",
        "No wireless radio",
        "WiFi radio enabled but",
        "Missing required field",
        // seed.reveal / auth flows — user-actionable, no internals to leak.
        // Without these the sanitizer collapsed every reveal failure into
        // "Operation failed. Check server logs." (which isn't even a crash).
        "Incorrect",
        "This node has no encrypted seed",
        "No Lightning seed backup",
        "Could not decrypt the saved Lightning seed",
        "A 2FA code is required",
        "2FA is enabled but",
        "Could not decrypt the saved seed",
        "Could not unlock 2FA",
        "No mnemonic available",
        "No pending seed generation",
        "Submitted words",
        "Already set up",
        // Ecash backup phrase — these two ARE the feature's safety rails, and
        // masking them made it dangerous rather than merely opaque. "That is
        // not a valid BIP-39 recovery phrase… check for typos" is the whole
        // help someone gets when a pasted phrase has a bad word; and "This
        // wallet already has a backup phrase… reveal and write down the
        // current phrase first, then confirm to replace it" is the warning
        // that stops an operator orphaning the words their balance was minted
        // under. Behind "check server logs" the first is unactionable and the
        // second is invisible.
        "That is not a valid BIP-39",
        "This wallet already has a backup phrase",
        "This wallet has no backup phrase yet",
        // Restore against a mint that never implemented NUT-09.
        "This mint does not support restoring",
    ];
    for prefix in &user_facing_prefixes {
        if msg.starts_with(prefix) {
            // Truncate long messages and strip file paths
            let sanitized = msg
                .replace("/var/lib/archipelago/", "[data]/")
                .replace("/usr/local/bin/", "[bin]/")
                .replace("/etc/", "[config]/");
            return if sanitized.len() > 200 {
                format!("{}...", &sanitized[..200])
            } else {
                sanitized
            };
        }
    }
    // For all other errors, return a generic message
    "Operation failed. Check server logs for details.".to_string()
}

#[cfg(test)]
mod sanitize_tests {
    use super::sanitize_error_message;

    /// The ecash import errors are the feature's safety rails. If the
    /// sanitizer eats them, a bad paste gives no hint and — worse — the
    /// warning about replacing an established phrase never reaches the person
    /// about to do it.
    #[test]
    fn ecash_backup_phrase_errors_reach_the_operator() {
        for msg in [
            "That is not a valid BIP-39 recovery phrase: invalid checksum. Check for typos",
            "This wallet already has a backup phrase. Importing a different one means coins \
             minted under the current phrase will no longer be restorable from words",
            "This wallet has no backup phrase yet, so there is nothing to restore from.",
            "This mint does not support restoring from a backup phrase (NUT-09).",
        ] {
            let out = sanitize_error_message(msg);
            assert_ne!(
                out, "Operation failed. Check server logs for details.",
                "swallowed: {msg}"
            );
        }
    }

    #[test]
    fn password_required_sentinel_passes_through_verbatim() {
        // The UI machine-reads this sentinel (isPasswordRequired checks
        // `includes('PASSWORD_REQUIRED')`) to know it should open the password
        // prompt. The "Password" prefix does not cover it — starts_with is
        // case-sensitive — and masking it made Trusted invites and trust
        // promotion impossible on EVERY node (2026-08-09): the prompt simply
        // never opened.
        let msg = "PASSWORD_REQUIRED: node password required to grant Trusted";
        assert_eq!(sanitize_error_message(msg), msg);
    }

    #[test]
    fn change_password_rejection_reaches_the_operator() {
        let msg = "Current password is incorrect";
        assert_eq!(sanitize_error_message(msg), msg);
    }

    #[test]
    fn tor_unavailable_precondition_passes_through() {
        let msg = "Tor address not available. Tor may not be running.";
        assert_eq!(sanitize_error_message(msg), msg);
    }

    #[test]
    fn seed_reveal_errors_pass_through() {
        // Every user-actionable seed.reveal failure must reach the user —
        // masking them as "Check server logs" sent a real user hunting a
        // crash that never happened.
        for msg in [
            "Incorrect password",
            "This node has no encrypted seed backup, so the recovery phrase cannot be shown. It was only displayed once during setup.",
            "A 2FA code is required to reveal the recovery phrase",
            "2FA is enabled but no TOTP data found",
            "Could not decrypt the saved seed. If you set a separate backup passphrase during setup, enter that passphrase.",
            "Could not unlock 2FA with this password",
            "No mnemonic available. Generate or restore a seed first.",
            "No Lightning seed backup exists on this node. It is captured automatically when the Lightning wallet is first created.",
            "Could not decrypt the saved Lightning seed backup",
            "Submitted words do not match generated seed",
            "Already set up. Use auth.changePassword to change.",
        ] {
            assert_ne!(
                sanitize_error_message(msg),
                "Operation failed. Check server logs for details.",
                "masked: {msg}"
            );
        }
    }

    #[test]
    fn lightning_payment_errors_pass_through() {
        // LND's payment-failure reasons are written for the payer — masking
        // "invoice expired" as "Check server logs" left a user retrying a
        // dead invoice (a test node, 2026-07-23).
        for msg in [
            "Payment failed: this invoice has expired (Valid until 2026-07-23 07:41:42 +0000 UTC). Ask the recipient for a fresh invoice and try again.",
            "Payment failed: unable to find a path to destination",
            "Invalid payment request: must be a Lightning invoice (lnbc...)",
            "Missing 'payment_request' parameter",
        ] {
            assert_ne!(
                sanitize_error_message(msg),
                "Operation failed. Check server logs for details.",
                "masked: {msg}"
            );
        }
    }

    #[test]
    fn internal_errors_stay_generic() {
        assert_eq!(
            sanitize_error_message("thread panicked at src/foo.rs:42"),
            "Operation failed. Check server logs for details."
        );
    }
}

/// Derive a CSRF token from the session token via HMAC.
/// Deterministic: same session token always produces the same CSRF token.
/// Survives backend restarts because it depends only on the session token
/// and the on-disk remember secret (not ephemeral state).
pub(crate) async fn derive_csrf_token(session_token: &str) -> std::io::Result<String> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let secret = SessionStore::load_or_create_remember_secret().await?;
    let mut mac = HmacSha256::new_from_slice(&secret).expect("HMAC key");
    mac.update(format!("csrf:{}", session_token).as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

/// Extract a named cookie value from headers.
pub(super) fn extract_cookie(headers: &hyper::HeaderMap, name: &str) -> Option<String> {
    let prefix = format!("{}=", name);
    for value in headers.get_all("cookie") {
        if let Ok(s) = value.to_str() {
            for part in s.split(';') {
                let part = part.trim();
                if let Some(val) = part.strip_prefix(&prefix) {
                    let val = val.trim();
                    if !val.is_empty() {
                        return Some(val.to_string());
                    }
                }
            }
        }
    }
    None
}

/// The TCP peer address of the connection a request arrived on, injected
/// into request extensions by the server accept loop.
#[derive(Debug, Clone, Copy)]
pub struct PeerAddr(pub std::net::SocketAddr);

/// Extract the client IP for rate limiting.
///
/// `X-Real-IP`/`X-Forwarded-For` are only honored when the connection
/// itself comes from loopback — i.e. from our local nginx, which sets
/// `X-Real-IP $remote_addr`. On a direct connection (the FIPS peer
/// listener, or anything that isn't the local proxy) the headers are
/// client-supplied, so trusting them let an attacker rotate per-request
/// "IPs" and defeat the login rate limiter; there we use the socket
/// address instead.
pub(super) fn extract_client_ip(parts: &hyper::http::request::Parts) -> IpAddr {
    let socket_ip = parts.extensions.get::<PeerAddr>().map(|p| p.0.ip());
    match socket_ip {
        Some(ip) if ip.is_loopback() => forwarded_client_ip(&parts.headers).unwrap_or(ip),
        Some(ip) => ip,
        // No socket info recorded (shouldn't happen in the server path);
        // fall back to the pre-extension behavior.
        None => {
            forwarded_client_ip(&parts.headers).unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
        }
    }
}

/// The proxy-reported client IP, if a forwarded header carries one.
fn forwarded_client_ip(headers: &hyper::HeaderMap) -> Option<IpAddr> {
    headers
        .get("x-real-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse::<IpAddr>().ok())
}

#[cfg(test)]
mod client_ip_tests {
    use super::*;
    use std::net::SocketAddr;

    fn parts_with(peer: Option<&str>, real_ip: Option<&str>) -> hyper::http::request::Parts {
        let mut builder = hyper::Request::builder().uri("/rpc/v1");
        if let Some(ip) = real_ip {
            builder = builder.header("x-real-ip", ip);
        }
        let (mut parts, _) = builder.body(()).unwrap().into_parts();
        if let Some(addr) = peer {
            parts
                .extensions
                .insert(PeerAddr(addr.parse::<SocketAddr>().unwrap()));
        }
        parts
    }

    #[test]
    fn loopback_connection_trusts_forwarded_header() {
        // nginx on loopback forwards the real client IP — use it.
        let parts = parts_with(Some("127.0.0.1:44412"), Some("192.0.2.50"));
        assert_eq!(
            extract_client_ip(&parts),
            "192.0.2.50".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn direct_connection_ignores_spoofed_header() {
        // A direct (non-proxy) client rotating X-Real-IP per request must
        // still bucket under its socket address.
        let parts = parts_with(Some("203.0.113.9:9999"), Some("10.0.0.1"));
        assert_eq!(
            extract_client_ip(&parts),
            "203.0.113.9".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn loopback_connection_without_header_uses_socket_ip() {
        let parts = parts_with(Some("127.0.0.1:5000"), None);
        assert_eq!(
            extract_client_ip(&parts),
            "127.0.0.1".parse::<IpAddr>().unwrap()
        );
    }
}
