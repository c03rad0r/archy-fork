use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Rate limiter for login attempts: max 5 failures per 60 seconds per IP.
#[derive(Clone)]
pub struct LoginRateLimiter {
    attempts: Arc<RwLock<HashMap<IpAddr, Vec<Instant>>>>,
}

const MAX_ATTEMPTS: usize = 5;
const WINDOW_SECS: u64 = 60;

impl LoginRateLimiter {
    pub fn new() -> Self {
        Self {
            attempts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn check(&self, ip: IpAddr) -> bool {
        let mut attempts = self.attempts.write().await;
        let now = Instant::now();
        let entry = attempts.entry(ip).or_default();
        entry.retain(|t| now.duration_since(*t).as_secs() < WINDOW_SECS);
        entry.len() < MAX_ATTEMPTS
    }

    pub async fn record_failure(&self, ip: IpAddr) {
        let mut attempts = self.attempts.write().await;
        let entry = attempts.entry(ip).or_default();
        entry.push(Instant::now());
    }

    /// Periodic cleanup of expired entries for IPs that are no longer active.
    pub async fn cleanup(&self) {
        let mut attempts = self.attempts.write().await;
        let now = Instant::now();
        attempts.retain(|_, timestamps| {
            timestamps.retain(|t| now.duration_since(*t).as_secs() < WINDOW_SECS);
            !timestamps.is_empty()
        });
    }
}

/// G-B3 / `assistant.chat`'s own soft-warn threshold and hard ceiling —
/// per AUTHENTICATED SESSION rather than client IP (13-AI-SPEC.md §6 G-B3
/// is explicit about this: an operator's own session can roam across IPs
/// within one LAN/Tailscale sitting, and "per authenticated session" is the
/// guardrail's own spec, not a stand-in for "per IP"). This is the
/// compensating control RESEARCH Open Question 2 names for the
/// same-origin-iframe residual risk 13-09's CSP does not fully close, and
/// the practical brake on an injection-driven loop that keeps re-invoking
/// `assistant.chat` itself (distinct from `loop_::MAX_TURNS`, which only
/// bounds tool calls WITHIN one already-running turn).
const ASSISTANT_CHAT_SOFT_THRESHOLD: usize = 30;
const ASSISTANT_CHAT_HARD_CEILING: usize = 60;
const ASSISTANT_CHAT_WINDOW_SECS: u64 = 300;

/// General-purpose rate limiter for sensitive endpoints.
/// Tracks request counts per (method, IP) with configurable limits and windows.
#[derive(Clone)]
pub struct EndpointRateLimiter {
    /// Map of (method, ip) -> list of request timestamps
    requests: Arc<RwLock<HashMap<(String, IpAddr), Vec<Instant>>>>, // Instant for monotonic rate limiting
    /// Per-method configuration: (max_requests, window_secs)
    limits: Arc<HashMap<String, (usize, u64)>>,
    /// `assistant.chat`'s own request log, keyed by authenticated SESSION
    /// id rather than client IP — same shape as `requests` above (a
    /// timestamp log per key, filtered by a trailing window), just a
    /// different identifier. Not a second limiter type: this struct still
    /// owns it, and every other entry in this module keeps using the
    /// IP-keyed `requests`/`limits` pair unchanged.
    session_requests: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
}

impl EndpointRateLimiter {
    pub fn new() -> Self {
        let mut limits = HashMap::new();
        // Financial operations: strict limits
        limits.insert("wallet.send".to_string(), (5usize, 300u64));
        limits.insert("wallet.ecash-send".to_string(), (10, 300));
        limits.insert("lnd.sendcoins".to_string(), (5, 300));
        limits.insert("lnd.bump-submit".to_string(), (5, 300));
        limits.insert("lnd.bump-quote".to_string(), (30, 60));
        limits.insert("lnd.payinvoice".to_string(), (10, 300));
        limits.insert("lnd.openchannel".to_string(), (3, 300));
        limits.insert("lnd.closechannel".to_string(), (3, 300));
        limits.insert("lnd.create-psbt".to_string(), (5, 300));
        limits.insert("lnd.finalize-psbt".to_string(), (5, 300));
        // Identity/credential operations
        limits.insert("identity.create".to_string(), (10, 300));
        limits.insert("identity.import-nostr".to_string(), (5, 300));
        limits.insert("identity.issue-credential".to_string(), (20, 300));
        // Backup operations (resource-intensive)
        limits.insert("backup.create".to_string(), (10, 600));
        limits.insert("backup.restore".to_string(), (5, 600));
        // Container operations
        limits.insert("container-install".to_string(), (5, 300));
        limits.insert("package.install".to_string(), (5, 300));
        // S3 backup operations (resource-intensive)
        limits.insert("backup.upload-s3".to_string(), (3, 600));
        limits.insert("backup.download-s3".to_string(), (3, 600));
        // System operations
        // Update apply is an authenticated local admin action. Keep a guard
        // against accidental button storms without locking operators out for
        // ten minutes during OTA troubleshooting.
        limits.insert("update.apply".to_string(), (10, 60));
        limits.insert("system.reboot".to_string(), (2, 300));
        limits.insert("system.shutdown".to_string(), (2, 300));
        // Password and TOTP changes
        limits.insert("auth.changePassword".to_string(), (3, 300));
        limits.insert("auth.totp.setup".to_string(), (3, 300));
        limits.insert("auth.totp.confirm".to_string(), (5, 300));
        // Federation join: prevent invite-code brute force
        limits.insert("federation.join".to_string(), (5, 60));
        limits.insert("federation.invite".to_string(), (10, 300));
        // Inter-node federation RPCs (unauthenticated, need stricter limits)
        limits.insert("federation.peer-joined".to_string(), (10, 60));
        limits.insert("federation.peer-address-changed".to_string(), (10, 60));
        limits.insert("federation.peer-did-changed".to_string(), (5, 60));
        limits.insert("federation.get-state".to_string(), (30, 60));
        // DID rotation: sensitive identity operation
        limits.insert("node.rotate-did".to_string(), (3, 600));

        // ── Unauthenticated onboarding mutators (F-01 / KEY-01) ─────────────
        //
        // These are in UNAUTHENTICATED_METHODS and can write node key
        // material, so they are rate-limited as defence in depth behind
        // `api::rpc::onboarding_gate`. The numbers are deliberately GENEROUS
        // rather than minimal, because a 429 here is a hard, user-visible
        // failure at the DID-creation screen — exactly the failure the
        // in-memory generate lock (`seed_rpc.rs:97-116`) was written to
        // prevent. A 429 comes back as `{"error":{"code":429,...}}` with
        // "Rate limit exceeded. Try again later." (`api/rpc/mod.rs:506-519`)
        // over HTTP 429, and neither the onboarding view's transient-error
        // regex (`OnboardingSeedGenerate.vue:243`) nor `rpc-client.ts`'s
        // retryable check (502/503 only) matches it — so a too-tight limit
        // surfaces to the user as "onboarding is broken".
        //
        // seed.generate — derivation: the view's 4s silent retry loop
        // (`OnboardingSeedGenerate.vue:265-268`) only fires on transient /
        // network errors, i.e. when the daemon is not answering at all, so the
        // limiter never sees those. What DOES reach the limiter is the
        // 30s-timeout aborts plus rpc-client's internal retries — roughly one
        // user-visible attempt per 30s, i.e. ~10 per 300s worst case. 20/300s
        // is ~6x the realistic budget and ~2x the pathological one.
        limits.insert("seed.generate".to_string(), (20, 300));
        // seed.restore — the audit suggests matching auth.changePassword at
        // 3/300s. REJECTED with cause: `rpc-client.ts:196-215` retries a single
        // call up to 3 times, so 3/300s would burn a user's entire budget on
        // one submit of a mistyped seed phrase and lock them out of the retry.
        limits.insert("seed.restore".to_string(), (10, 300));
        limits.insert("seed.save-encrypted".to_string(), (10, 300));
        limits.insert("backup.restore-identity".to_string(), (10, 300));

        Self {
            requests: Arc::new(RwLock::new(HashMap::new())),
            limits: Arc::new(limits),
            session_requests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// G-B3: `assistant.chat`, keyed by authenticated session id. `true`
    /// while this session is still under the hard ceiling — call BEFORE
    /// handling the turn, refuse the call if `false`. Pair with
    /// [`Self::record_session_request`], mirroring this module's existing
    /// `check`/`record` shape.
    pub async fn check_session(&self, session_id: &str) -> bool {
        let requests = self.session_requests.read().await;
        let now = Instant::now();
        let count = requests
            .get(session_id)
            .map(|v| {
                v.iter()
                    .filter(|t| now.duration_since(**t).as_secs() < ASSISTANT_CHAT_WINDOW_SECS)
                    .count()
            })
            .unwrap_or(0);
        count < ASSISTANT_CHAT_HARD_CEILING
    }

    /// Record one `assistant.chat` call for this session. Call AFTER
    /// `check_session` has allowed the call.
    pub async fn record_session_request(&self, session_id: &str) {
        let mut requests = self.session_requests.write().await;
        let now = Instant::now();
        let entry = requests.entry(session_id.to_string()).or_default();
        entry.retain(|t| now.duration_since(*t).as_secs() < ASSISTANT_CHAT_WINDOW_SECS);
        entry.push(now);
    }

    /// G-B3: `true` once this session's `assistant.chat` count within the
    /// window has reached the SOFT threshold — call AFTER
    /// `record_session_request` so the call that crossed it is included.
    /// Distinct from `check_session`'s hard refusal: this fires earlier,
    /// as an owner-visible warning rather than a block.
    pub async fn session_soft_threshold_reached(&self, session_id: &str) -> bool {
        let requests = self.session_requests.read().await;
        let now = Instant::now();
        let count = requests
            .get(session_id)
            .map(|v| {
                v.iter()
                    .filter(|t| now.duration_since(**t).as_secs() < ASSISTANT_CHAT_WINDOW_SECS)
                    .count()
            })
            .unwrap_or(0);
        count >= ASSISTANT_CHAT_SOFT_THRESHOLD
    }

    /// Periodic cleanup of expired session-keyed entries, mirroring
    /// `cleanup()` below for the IP-keyed map.
    pub async fn cleanup_sessions(&self) {
        let mut requests = self.session_requests.write().await;
        let now = Instant::now();
        requests.retain(|_, timestamps| {
            timestamps.retain(|t| now.duration_since(*t).as_secs() < ASSISTANT_CHAT_WINDOW_SECS);
            !timestamps.is_empty()
        });
    }

    /// Check if a request is allowed. Returns true if within limits.
    pub async fn check(&self, method: &str, ip: IpAddr) -> bool {
        let (max_req, window) = match self.limits.get(method) {
            Some(config) => *config,
            None => return true, // Not rate-limited
        };

        let key = (method.to_string(), ip);
        let mut requests = self.requests.write().await;
        let now = Instant::now();
        let entry = requests.entry(key).or_default();
        entry.retain(|t| now.duration_since(*t).as_secs() < window);
        entry.len() < max_req
    }

    /// Record a request for rate limiting purposes.
    pub async fn record(&self, method: &str, ip: IpAddr) {
        if !self.limits.contains_key(method) {
            return; // Not rate-limited, skip tracking
        }
        let key = (method.to_string(), ip);
        let mut requests = self.requests.write().await;
        let entry = requests.entry(key).or_default();
        entry.push(Instant::now());
    }

    /// Periodic cleanup of expired entries.
    pub async fn cleanup(&self) {
        let mut requests = self.requests.write().await;
        let now = Instant::now();
        requests.retain(|(method, _), timestamps| {
            let window = self.limits.get(method).map(|(_, w)| *w).unwrap_or(300);
            timestamps.retain(|t| now.duration_since(*t).as_secs() < window);
            !timestamps.is_empty()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_allows_under_limit() {
        let limiter = LoginRateLimiter::new();
        let ip: IpAddr = "127.0.0.1"
            .parse()
            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));

        for _ in 0..MAX_ATTEMPTS {
            assert!(limiter.check(ip).await);
            limiter.record_failure(ip).await;
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks_over_limit() {
        let limiter = LoginRateLimiter::new();
        let ip: IpAddr = "127.0.0.1"
            .parse()
            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));

        for _ in 0..MAX_ATTEMPTS {
            limiter.record_failure(ip).await;
        }

        assert!(!limiter.check(ip).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_different_ips() {
        let limiter = LoginRateLimiter::new();
        let ip1: IpAddr = "127.0.0.1"
            .parse()
            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        let ip2: IpAddr = "192.168.1.1"
            .parse()
            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));

        for _ in 0..MAX_ATTEMPTS {
            limiter.record_failure(ip1).await;
        }

        // ip1 should be blocked
        assert!(!limiter.check(ip1).await);
        // ip2 should still be allowed
        assert!(limiter.check(ip2).await);
    }

    fn test_ip() -> IpAddr {
        IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
    }

    /// seed.generate is rate-limited, but not below the real client retry
    /// budget: 20 attempts in the window are allowed, the 21st is refused.
    #[tokio::test]
    async fn seed_generate_allows_twenty_then_limits() {
        let limiter = EndpointRateLimiter::new();
        let ip = test_ip();

        for i in 0..20 {
            assert!(
                limiter.check("seed.generate", ip).await,
                "attempt {i} must be allowed — a 429 here is a hard failure at \
                 the DID-creation screen"
            );
            limiter.record("seed.generate", ip).await;
        }

        assert!(
            !limiter.check("seed.generate", ip).await,
            "the 21st attempt in the window must be refused"
        );
    }

    /// One user submit of a seed phrase costs up to 1 + 3 internal retries
    /// (`rpc-client.ts:196-215`). The limit must clear that comfortably, which
    /// is why the audit's suggested 3/300s was rejected.
    #[tokio::test]
    async fn seed_restore_allows_a_full_submit_with_its_retries() {
        let limiter = EndpointRateLimiter::new();
        let ip = test_ip();

        for i in 0..4 {
            assert!(
                limiter.check("seed.restore", ip).await,
                "call {i} of one user submit + its internal retries must be allowed"
            );
            limiter.record("seed.restore", ip).await;
        }
    }

    /// All four onboarding mutators are actually registered — a typo'd key
    /// silently means "not rate-limited at all" (`check` returns true for
    /// unknown methods).
    #[tokio::test]
    async fn onboarding_mutators_are_registered() {
        let limiter = EndpointRateLimiter::new();
        for method in [
            "seed.generate",
            "seed.restore",
            "seed.save-encrypted",
            "backup.restore-identity",
        ] {
            assert!(
                limiter.limits.contains_key(method),
                "{method} has no rate limit entry"
            );
        }
    }

    /// G-B3: `assistant.chat` is rate-limited per authenticated session —
    /// the soft threshold is reached first (an owner notice should fire at
    /// the call site), and the hard ceiling refuses the call outright.
    #[tokio::test]
    async fn assistant_chat_soft_threshold_then_hard_ceiling_refuses() {
        let limiter = EndpointRateLimiter::new();
        let session = "session-abc";

        for i in 0..ASSISTANT_CHAT_SOFT_THRESHOLD {
            assert!(
                limiter.check_session(session).await,
                "call {i} under the hard ceiling must be allowed"
            );
            limiter.record_session_request(session).await;
        }
        assert!(
            limiter.session_soft_threshold_reached(session).await,
            "the soft threshold must be reached at {ASSISTANT_CHAT_SOFT_THRESHOLD} calls"
        );
        // Still under the hard ceiling — allowed, just flagged.
        assert!(limiter.check_session(session).await);

        for i in ASSISTANT_CHAT_SOFT_THRESHOLD..ASSISTANT_CHAT_HARD_CEILING {
            assert!(
                limiter.check_session(session).await,
                "call {i} still under the hard ceiling must be allowed"
            );
            limiter.record_session_request(session).await;
        }
        assert!(
            !limiter.check_session(session).await,
            "the call beyond the hard ceiling must be refused"
        );
    }

    /// A different session's own count is unaffected by another session's
    /// activity — "per authenticated session", not a shared global bucket.
    #[tokio::test]
    async fn assistant_chat_sessions_are_independent() {
        let limiter = EndpointRateLimiter::new();
        for _ in 0..ASSISTANT_CHAT_HARD_CEILING {
            limiter.record_session_request("busy-session").await;
        }
        assert!(!limiter.check_session("busy-session").await);
        assert!(
            limiter.check_session("quiet-session").await,
            "an unrelated session must not be penalized by another session's activity"
        );
    }

    /// The new session-keyed `assistant.chat` limiter does not touch, and
    /// does not degrade, the existing IP-keyed methods this module already
    /// governs.
    #[tokio::test]
    async fn assistant_chat_limiter_does_not_affect_existing_ip_keyed_methods() {
        let limiter = EndpointRateLimiter::new();
        let ip = test_ip();
        for _ in 0..ASSISTANT_CHAT_HARD_CEILING + 10 {
            limiter.record_session_request("some-session").await;
        }
        // seed.generate's own IP-keyed limit is untouched by any amount of
        // assistant.chat session-keyed activity.
        for i in 0..20 {
            assert!(
                limiter.check("seed.generate", ip).await,
                "attempt {i} must still be allowed — unrelated to assistant.chat's session limiter"
            );
            limiter.record("seed.generate", ip).await;
        }
    }
}
