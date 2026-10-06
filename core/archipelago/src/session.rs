#[path = "session_secret.rs"]
mod secret_file;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{OnceCell, RwLock};
use zeroize::Zeroize;

/// Cached remember secret — loaded once, never regenerated within a process.
static REMEMBER_SECRET: OnceCell<Vec<u8>> = OnceCell::const_new();

type HmacSha256 = Hmac<Sha256>;

const FULL_SESSION_TTL: u64 = 86400; // 24 hours of inactivity
const PENDING_SESSION_TTL: u64 = 300; // 5 minutes
const MAX_TOTP_ATTEMPTS: u8 = 5;
const MAX_CONCURRENT_SESSIONS: usize = 50;
const SESSIONS_FILE: &str = "/var/lib/archipelago/sessions.json";
const REMEMBER_SECRET_FILE: &str = "/var/lib/archipelago/remember_secret";
pub const REMEMBER_TTL: u64 = 30 * 24 * 3600; // 30 days

#[derive(Clone)]
enum SessionType {
    Full,
    PendingTotp { totp_secret: Vec<u8>, attempts: u8 },
}

impl Drop for SessionType {
    fn drop(&mut self) {
        if let SessionType::PendingTotp { totp_secret, .. } = self {
            totp_secret.zeroize();
        }
    }
}

#[derive(Clone)]
struct Session {
    created_at: SystemTime,
    last_activity: SystemTime,
    session_type: SessionType,
    /// What kind of screen this login came from. A TV on the wall must not
    /// be signed out for sitting still — nobody is there to type a password
    /// back in — while a browser must be.
    device_class: crate::settings::session_policy::DeviceClass,
}

#[derive(Clone)]
pub struct SessionStore {
    sessions: Arc<RwLock<HashMap<[u8; 32], Session>>>,
    persist_path: PathBuf,
    /// Where the session policy lives. Held rather than looked up globally
    /// so tests can point at a temp dir.
    data_dir: PathBuf,
}

/// On-disk representation of a persisted session (only Full sessions, no TOTP secrets).
#[derive(serde::Serialize, serde::Deserialize)]
struct PersistedSession {
    hash_hex: String,
    created_at: u64,    // Unix timestamp
    last_activity: u64, // Unix timestamp
}

impl SessionStore {
    pub async fn new() -> Self {
        let persist_path = PathBuf::from(SESSIONS_FILE);
        let sessions = Self::load_from_disk(&persist_path).await;
        let count = sessions.len();
        if count > 0 {
            tracing::info!("Restored {} sessions from disk", count);
        }
        Self {
            sessions: Arc::new(RwLock::new(sessions)),
            persist_path,
            data_dir: PathBuf::from("/var/lib/archipelago"),
        }
    }

    /// Construct an empty SessionStore that persists to a caller-supplied
    /// path. Used by tests so they don't pick up sessions from the dev
    /// machine's real /var/lib/archipelago/sessions.json.
    #[cfg(test)]
    pub fn new_for_tests(persist_path: PathBuf) -> Self {
        // data_dir shares the temp path's parent so a test that writes a
        // policy file is honoured, and one that doesn't gets the defaults
        // rather than the dev machine's real configuration.
        let data_dir = persist_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            persist_path,
            data_dir,
        }
    }

    /// Load persisted sessions from disk (only Full sessions).
    async fn load_from_disk(path: &Path) -> HashMap<[u8; 32], Session> {
        let mut map = HashMap::new();
        let data = match tokio::fs::read_to_string(path).await {
            Ok(d) => d,
            Err(_) => return map,
        };
        let persisted: Vec<PersistedSession> = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Failed to parse sessions file: {}", e);
                return map;
            }
        };
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        for p in persisted {
            // Skip expired sessions
            if now_unix.saturating_sub(p.last_activity) >= FULL_SESSION_TTL {
                continue;
            }
            let hash = match hex::decode(&p.hash_hex) {
                Ok(h) if h.len() == 32 => {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&h);
                    arr
                }
                _ => continue,
            };
            let created_at = UNIX_EPOCH + std::time::Duration::from_secs(p.created_at);
            let last_activity = UNIX_EPOCH + std::time::Duration::from_secs(p.last_activity);
            map.insert(
                hash,
                Session {
                    created_at,
                    last_activity,
                    session_type: SessionType::Full,
                    device_class: crate::settings::session_policy::DeviceClass::Browser,
                },
            );
        }
        map
    }

    /// Save all Full sessions to disk. Called after mutations.
    async fn save_to_disk(sessions: &HashMap<[u8; 32], Session>, path: &Path) {
        let persisted: Vec<PersistedSession> = sessions
            .iter()
            .filter(|(_, s)| matches!(s.session_type, SessionType::Full))
            .map(|(hash, s)| PersistedSession {
                hash_hex: hex::encode(hash),
                created_at: s
                    .created_at
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                last_activity: s
                    .last_activity
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            })
            .collect();
        if let Ok(json) = serde_json::to_string(&persisted) {
            let _ = tokio::fs::write(path, json).await;
        }
    }

    /// Create a full (authenticated) session. Returns the plaintext token.
    /// Enforces max concurrent sessions by evicting the oldest if limit reached.
    pub async fn create(&self) -> String {
        let token = fresh_session_token();
        let hash = hash_token(&token);
        let now = SystemTime::now();
        let session = Session {
            created_at: now,
            last_activity: now,
            session_type: SessionType::Full,
            device_class: crate::settings::session_policy::DeviceClass::Browser,
        };

        let mut sessions = self.sessions.write().await;
        self.evict_if_over_limit(&mut sessions);
        sessions.insert(hash, session);
        // Sync save — must complete before returning the token to the client.
        // Async save risks losing the session if the process is killed (e.g., deploy restart).
        Self::save_to_disk(&sessions, &self.persist_path).await;
        token
    }

    /// Create a pending TOTP session (password verified, awaiting TOTP).
    /// Caches the decrypted TOTP secret in memory for verification.
    pub async fn create_pending(&self, totp_secret: Vec<u8>) -> String {
        let token = fresh_session_token();
        let hash = hash_token(&token);
        let now = SystemTime::now();
        let session = Session {
            created_at: now,
            last_activity: now,
            session_type: SessionType::PendingTotp {
                totp_secret,
                attempts: 0,
            },
            // A half-finished login is always treated as a browser: it lives
            // for PENDING_SESSION_TTL either way, and a kiosk exemption on a
            // session that has not passed 2FA yet would be the wrong default.
            device_class: crate::settings::session_policy::DeviceClass::Browser,
        };
        self.sessions.write().await.insert(hash, session);
        token
    }

    /// Validate a full session token. Returns true if the session exists and hasn't expired.
    /// Updates last_activity on successful validation (inactivity-based expiry).
    pub async fn validate(&self, token: &str) -> bool {
        let policy = self.policy().await;
        let hash = hash_token(token);
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&hash) {
            if !matches!(session.session_type, SessionType::Full) {
                return false;
            }
            let idle = session
                .last_activity
                .elapsed()
                .unwrap_or_default()
                .as_secs();
            let age = session.created_at.elapsed().unwrap_or_default().as_secs();
            // Both limits, not just idleness: the dashboard polls, so an
            // idle timeout alone would never fire on an open tab. The
            // absolute cap is what actually guarantees a login ends.
            if policy.is_expired(session.device_class, age, idle) {
                sessions.remove(&hash);
                return false;
            }
            session.last_activity = SystemTime::now();
            true
        } else {
            false
        }
    }

    /// The operator's session policy, re-read from disk rather than cached
    /// for the process lifetime so a change in Settings takes effect on the
    /// next request instead of the next restart.
    pub async fn policy(&self) -> crate::settings::session_policy::SessionPolicy {
        crate::settings::session_policy::load(&self.data_dir).await
    }

    /// Get the TOTP secret from a pending session. Returns None if not a valid pending session.
    /// Increments the attempt counter.
    pub async fn get_pending_secret(&self, token: &str) -> Option<Vec<u8>> {
        let hash = hash_token(token);
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&hash) {
            if session.created_at.elapsed().unwrap_or_default().as_secs() >= PENDING_SESSION_TTL {
                sessions.remove(&hash);
                return None;
            }
            if let SessionType::PendingTotp {
                ref totp_secret,
                ref mut attempts,
            } = session.session_type
            {
                *attempts += 1;
                if *attempts > MAX_TOTP_ATTEMPTS {
                    sessions.remove(&hash); // Too many attempts, force re-login
                    return None;
                }
                return Some(totp_secret.clone());
            }
        }
        None
    }

    /// Upgrade a pending session to a full session with token rotation.
    /// Deletes the old pending session and creates a new full session with a fresh token.
    /// Returns the new plaintext token so the caller can set it as the new cookie.
    pub async fn upgrade_to_full(&self, token: &str) -> Option<String> {
        let old_hash = hash_token(token);
        let mut sessions = self.sessions.write().await;
        // Only upgrade if the old session exists and is pending
        if sessions.remove(&old_hash).is_some() {
            let new_token = fresh_session_token();
            let new_hash = hash_token(&new_token);
            let now = SystemTime::now();
            self.evict_if_over_limit(&mut sessions);
            sessions.insert(
                new_hash,
                Session {
                    created_at: now,
                    last_activity: now,
                    session_type: SessionType::Full,
                    device_class: crate::settings::session_policy::DeviceClass::Browser,
                },
            );
            Self::save_to_disk(&sessions, &self.persist_path).await;
            Some(new_token)
        } else {
            None
        }
    }

    pub async fn remove(&self, token: &str) {
        let hash = hash_token(token);
        let mut sessions = self.sessions.write().await;
        sessions.remove(&hash);
        Self::save_to_disk(&sessions, &self.persist_path).await;
    }

    /// Invalidate all sessions except the one matching the given token.
    /// Used after sensitive operations like password change.
    pub async fn invalidate_all_except(&self, keep_token: &str) {
        let keep_hash = hash_token(keep_token);
        let mut sessions = self.sessions.write().await;
        sessions.retain(|hash, _| *hash == keep_hash);
        Self::save_to_disk(&sessions, &self.persist_path).await;
    }

    /// Rotate a session: invalidate the old token and create a new one.
    /// Returns the new plaintext token.
    pub async fn rotate(&self, old_token: &str) -> String {
        let old_hash = hash_token(old_token);
        let new_token = fresh_session_token();
        let new_hash = hash_token(&new_token);
        let now = SystemTime::now();

        let mut sessions = self.sessions.write().await;
        sessions.remove(&old_hash);
        sessions.insert(
            new_hash,
            Session {
                created_at: now,
                last_activity: now,
                session_type: SessionType::Full,
                device_class: crate::settings::session_policy::DeviceClass::Browser,
            },
        );
        Self::save_to_disk(&sessions, &self.persist_path).await;
        new_token
    }

    /// Remove all expired sessions (cleanup).
    #[cfg(test)]
    pub async fn cleanup_expired(&self) {
        let mut sessions = self.sessions.write().await;
        sessions.retain(|_, session| match &session.session_type {
            SessionType::Full => {
                session
                    .last_activity
                    .elapsed()
                    .unwrap_or_default()
                    .as_secs()
                    < FULL_SESSION_TTL
            }
            SessionType::PendingTotp { .. } => {
                session.created_at.elapsed().unwrap_or_default().as_secs() < PENDING_SESSION_TTL
            }
        });
    }

    /// Evict the oldest full session if at or over the concurrent limit.
    fn evict_if_over_limit(&self, sessions: &mut HashMap<[u8; 32], Session>) {
        let full_count = sessions
            .values()
            .filter(|s| matches!(s.session_type, SessionType::Full))
            .count();

        if full_count >= MAX_CONCURRENT_SESSIONS {
            // Find the oldest full session by last_activity
            if let Some(oldest_hash) = sessions
                .iter()
                .filter(|(_, s)| matches!(s.session_type, SessionType::Full))
                .min_by_key(|(_, s)| s.last_activity)
                .map(|(h, _)| *h)
            {
                sessions.remove(&oldest_hash);
            }
        }
    }

    /// Get the number of active full sessions.
    #[cfg(test)]
    pub async fn active_session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .filter(|s| {
                matches!(s.session_type, SessionType::Full)
                    && s.last_activity.elapsed().unwrap_or_default().as_secs() < FULL_SESSION_TTL
            })
            .count()
    }

    // ── Remember-me token ──────────────────────────────────────────────
    // HMAC-signed token that survives backend restarts. Secret is on disk.
    // Format: "timestamp_hex:hmac_hex"

    /// Create a remember-me token. Returns the cookie value.
    pub async fn create_remember_token(&self) -> std::io::Result<String> {
        let secret = Self::load_or_create_remember_secret().await?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ts_hex = hex::encode(now.to_be_bytes());
        let mut mac = HmacSha256::new_from_slice(&secret).expect("HMAC key");
        mac.update(format!("remember:{}", ts_hex).as_bytes());
        let sig = hex::encode(mac.finalize().into_bytes());
        Ok(format!("{}:{}", ts_hex, sig))
    }

    /// Validate a remember-me token. Returns true if valid and not expired.
    pub async fn validate_remember_token(token: &str) -> bool {
        let secret = match tokio::task::spawn_blocking(|| {
            secret_file::read_existing(Path::new(REMEMBER_SECRET_FILE))
        })
        .await
        {
            Ok(Ok(s)) => s,
            _ => return false,
        };
        let parts: Vec<&str> = token.splitn(2, ':').collect();
        if parts.len() != 2 {
            return false;
        }
        let ts_hex = parts[0];
        let sig_hex = parts[1];

        // Verify HMAC
        let mut mac = match HmacSha256::new_from_slice(&secret) {
            Ok(m) => m,
            Err(_) => return false,
        };
        mac.update(format!("remember:{}", ts_hex).as_bytes());
        let expected_sig = match hex::decode(sig_hex) {
            Ok(s) => s,
            Err(_) => return false,
        };
        if mac.verify_slice(&expected_sig).is_err() {
            return false;
        }

        // Check expiry
        let ts_bytes = match hex::decode(ts_hex) {
            Ok(b) if b.len() == 8 => {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&b);
                u64::from_be_bytes(arr)
            }
            _ => return false,
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(ts_bytes) < REMEMBER_TTL
    }

    pub async fn load_or_create_remember_secret() -> std::io::Result<Vec<u8>> {
        REMEMBER_SECRET
            .get_or_try_init(|| async {
                tokio::task::spawn_blocking(|| {
                    secret_file::load_or_create(Path::new(REMEMBER_SECRET_FILE))
                })
                .await
                .map_err(std::io::Error::other)?
            })
            .await
            .cloned()
    }
}

/// Mint a fresh 32-byte session token, hex-encoded.
///
/// KEY-05: the entropy source is named (`OsRng`) rather than inherited from a
/// dependency default, and the draw is inspected by the degenerate-entropy
/// predicate before it becomes a bearer credential.
///
/// **This aborts rather than returning on a degenerate draw, and that is
/// deliberate.** `create`, `create_pending` and `rotate` return a bare `String`,
/// and their callers live in `api/rpc/mod.rs` and `api/rpc/totp.rs` — files plan
/// 10-06 does not own; widening them to `Result` is an API change this plan is
/// explicitly not permitted to make. The only two behaviours available at this
/// seam are therefore "mint a predictable session token" and "refuse loudly",
/// and only the second is defensible: reaching this branch means the kernel
/// CSPRNG returned 32 bytes that are all-zero, all-identical or a ±1 counter,
/// i.e. the machine has no usable entropy and must not be issuing credentials at
/// all. It cannot be driven by attacker-supplied input — the predicate reads only
/// `OsRng` output — and the false-trip bound at 32 bytes is `3 · 2^−248`
/// (`docs/security/KEY-05-ENTROPY-ENFORCEMENT.md`).
fn fresh_session_token() -> String {
    let mut token_bytes = [0u8; 32];
    crate::entropy::draw_key_bytes(&mut rand::rngs::OsRng, &mut token_bytes).unwrap_or_else(|e| {
        panic!("refusing to mint a session token from degenerate entropy: {e} (KEY-05)")
    });
    hex::encode(token_bytes)
}

fn hash_token(token: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher.finalize().into()
}

/// Extract the session token from a Cookie header value.
pub fn extract_session_cookie(headers: &hyper::HeaderMap) -> Option<String> {
    headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|c| {
                let c = c.trim();
                c.strip_prefix("session=").map(|v| v.to_string())
            })
        })
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique suffix for a per-test temp file path.
    ///
    /// KEY-05 migrates test fixtures for the same reason it migrates production
    /// code: CI runs clippy with `--all-targets`, so a `rand::random()` left in a
    /// test is a build failure once the ban is live. This is not key material —
    /// it is a filename component — so it is drawn unguarded, and it names its
    /// source like everything else.
    fn uniq() -> u64 {
        rand::RngCore::next_u64(&mut rand::rngs::OsRng)
    }

    #[tokio::test]
    async fn test_session_create_and_validate() {
        let store = SessionStore::new_for_tests(
            std::env::temp_dir().join(format!("archipelago-sessions-test-{}.json", uniq())),
        );
        let token = store.create().await;

        assert!(store.validate(&token).await);
    }

    #[tokio::test]
    async fn test_session_invalid_token() {
        let store = SessionStore::new_for_tests(
            std::env::temp_dir().join(format!("archipelago-sessions-test-{}.json", uniq())),
        );
        assert!(!store.validate("nonexistent_token").await);
    }

    #[tokio::test]
    async fn test_session_remove() {
        let store = SessionStore::new_for_tests(
            std::env::temp_dir().join(format!("archipelago-sessions-test-{}.json", uniq())),
        );
        let token = store.create().await;

        assert!(store.validate(&token).await);
        store.remove(&token).await;
        assert!(!store.validate(&token).await);
    }

    #[tokio::test]
    async fn test_pending_session_upgrade() {
        let store = SessionStore::new_for_tests(
            std::env::temp_dir().join(format!("archipelago-sessions-test-{}.json", uniq())),
        );
        let secret = vec![1, 2, 3, 4];
        let token = store.create_pending(secret.clone()).await;

        // Pending session should not validate as full
        assert!(!store.validate(&token).await);

        // Can get the TOTP secret
        let got = store.get_pending_secret(&token).await;
        assert_eq!(got, Some(secret));

        // Upgrade to full — returns a new rotated token
        let new_token = store.upgrade_to_full(&token).await;
        assert!(new_token.is_some());
        let new_token = new_token.unwrap();

        // Old token should be invalid (rotated)
        assert!(!store.validate(&token).await);
        // New token should be valid
        assert!(store.validate(&new_token).await);
    }

    #[tokio::test]
    async fn test_pending_session_max_attempts() {
        let store = SessionStore::new_for_tests(
            std::env::temp_dir().join(format!("archipelago-sessions-test-{}.json", uniq())),
        );
        let secret = vec![1, 2, 3];
        let token = store.create_pending(secret).await;

        // Exhaust MAX_TOTP_ATTEMPTS (5) + 1 to trigger removal
        for _ in 0..MAX_TOTP_ATTEMPTS {
            assert!(store.get_pending_secret(&token).await.is_some());
        }
        // 6th attempt should fail (session removed)
        assert!(store.get_pending_secret(&token).await.is_none());
    }

    #[tokio::test]
    async fn test_extract_session_cookie() {
        let mut headers = hyper::HeaderMap::new();
        headers.insert("cookie", "session=abc123; other=xyz".parse().unwrap());

        assert_eq!(extract_session_cookie(&headers), Some("abc123".to_string()));
    }

    #[tokio::test]
    async fn test_extract_session_cookie_missing() {
        let headers = hyper::HeaderMap::new();
        assert_eq!(extract_session_cookie(&headers), None);
    }

    #[tokio::test]
    async fn test_session_activity_updates_on_validate() {
        let store = SessionStore::new_for_tests(
            std::env::temp_dir().join(format!("archipelago-sessions-test-{}.json", uniq())),
        );
        let token = store.create().await;

        // First validation should succeed and touch last_activity
        assert!(store.validate(&token).await);

        // Still valid after another validation
        assert!(store.validate(&token).await);
    }

    #[tokio::test]
    async fn test_invalidate_all_except() {
        let store = SessionStore::new_for_tests(
            std::env::temp_dir().join(format!("archipelago-sessions-test-{}.json", uniq())),
        );
        let token1 = store.create().await;
        let token2 = store.create().await;
        let token3 = store.create().await;

        // Invalidate all except token2
        store.invalidate_all_except(&token2).await;

        assert!(!store.validate(&token1).await);
        assert!(store.validate(&token2).await);
        assert!(!store.validate(&token3).await);
    }

    #[tokio::test]
    async fn test_session_rotate() {
        let store = SessionStore::new_for_tests(
            std::env::temp_dir().join(format!("archipelago-sessions-test-{}.json", uniq())),
        );
        let old_token = store.create().await;

        assert!(store.validate(&old_token).await);

        let new_token = store.rotate(&old_token).await;

        // Old token should be invalid
        assert!(!store.validate(&old_token).await);
        // New token should be valid
        assert!(store.validate(&new_token).await);
    }

    #[tokio::test]
    async fn test_max_concurrent_sessions() {
        let store = SessionStore::new_for_tests(
            std::env::temp_dir().join(format!("archipelago-sessions-test-{}.json", uniq())),
        );
        let mut tokens = Vec::new();

        // Create MAX_CONCURRENT_SESSIONS sessions
        for _ in 0..MAX_CONCURRENT_SESSIONS {
            tokens.push(store.create().await);
        }

        // All should be valid
        for token in &tokens {
            assert!(store.validate(token).await);
        }

        // Creating one more should evict the oldest
        let new_token = store.create().await;
        assert!(store.validate(&new_token).await);

        // The first token (oldest) should have been evicted
        assert!(!store.validate(&tokens[0]).await);

        // The rest should still be valid
        for token in &tokens[1..] {
            assert!(store.validate(token).await);
        }
    }

    #[tokio::test]
    async fn test_active_session_count() {
        let store = SessionStore::new_for_tests(
            std::env::temp_dir().join(format!("archipelago-sessions-test-{}.json", uniq())),
        );
        assert_eq!(store.active_session_count().await, 0);

        let token1 = store.create().await;
        assert_eq!(store.active_session_count().await, 1);

        let _token2 = store.create().await;
        assert_eq!(store.active_session_count().await, 2);

        store.remove(&token1).await;
        assert_eq!(store.active_session_count().await, 1);
    }

    #[tokio::test]
    async fn test_cleanup_expired_removes_stale() {
        let store = SessionStore::new_for_tests(
            std::env::temp_dir().join(format!("archipelago-sessions-test-{}.json", uniq())),
        );
        let token = store.create().await;

        assert!(store.validate(&token).await);
        assert_eq!(store.active_session_count().await, 1);

        // Cleanup shouldn't remove active sessions
        store.cleanup_expired().await;
        assert_eq!(store.active_session_count().await, 1);
    }

    #[tokio::test]
    async fn test_rotate_preserves_session_count() {
        let store = SessionStore::new_for_tests(
            std::env::temp_dir().join(format!("archipelago-sessions-test-{}.json", uniq())),
        );
        let token = store.create().await;
        assert_eq!(store.active_session_count().await, 1);

        let new_token = store.rotate(&token).await;
        assert_eq!(store.active_session_count().await, 1);
        assert!(store.validate(&new_token).await);
    }
}
