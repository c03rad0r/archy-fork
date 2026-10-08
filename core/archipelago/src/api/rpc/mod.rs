mod analytics;
mod appgate;
mod ark;
mod assistant_chat;
mod auth;
mod backup_rpc;
mod bitcoin;
pub(crate) mod bitcoin_relay;
mod container;
mod content;
mod credentials;
mod dispatcher;
mod dwn;
mod federation;
mod fedimint;
mod fips;
mod handshake;
mod identity;
mod interfaces;
pub(crate) mod lnd;
mod marketplace;
// pub(crate): 13-10's `assistant::backends::select_backend` reuses
// `mesh::assistant::detect_ollama()` (D-04) rather than re-probing —
// matches the existing `pub(crate) mod bitcoin_relay;`/`pub(crate) mod
// lnd;` convention already in this file for cross-module reuse.
pub(crate) mod mesh;
mod middleware;
mod monitoring;
mod music;
mod names;
mod network;
mod publishing;
mod node;
mod nostr;
mod onboarding_gate;
mod openwrt;
mod package;
pub(crate) use package::patch_indeedhub_nostr_provider;
pub(crate) use package::wyoming_satellite_keeper;
mod peers;
mod pine_status;
mod response;
mod router;
mod security;
mod seed_rpc;
mod streaming;
mod system;
pub(crate) mod tor;
mod totp;
mod transitional;
mod transport;
mod update;
mod vpn;
mod wallet;
mod webhooks;

use crate::auth::AuthManager;
use crate::config::Config;
use crate::container::{ContainerOrchestrator, DevContainerOrchestrator};
use crate::monitoring::MetricsStore;
use crate::port_allocator::PortAllocator;
use crate::rate_limit::{EndpointRateLimiter, LoginRateLimiter};
use crate::session::{self, SessionStore, REMEMBER_TTL};
use crate::state::StateManager;
use anyhow::{Context, Result};
use hyper::{Request, Response, StatusCode};
use std::sync::Arc;
use tracing::{debug, error};

pub use middleware::PeerAddr;
// Re-exported `pub(crate)` (not just imported) so `crate::assistant`'s test
// suite can assert directly against the live list that `assistant.*` is
// never added to it — the Phase-10 hard constraint this crate must hold.
// The list's *contents* are unchanged; only its read-visibility widens from
// "this module" to "this crate".
pub(crate) use middleware::{derive_csrf_token, UNAUTHENTICATED_METHODS};
use middleware::{extract_client_ip, extract_cookie, sanitize_error_message, CACHEABLE_METHODS};
use response::{cookie_header, json_response, ResponseCache, RpcError, RpcRequest, RpcResponse};

/// Browser apps run on dedicated high ports and can share the authenticated
/// node cookie. Nostr signing must therefore be callable by the dashboard
/// bridge (ports 80/443), not directly by an iframe that could bypass its
/// consent dialog. Requests without Origin remain available to authenticated
/// local CLI/integration clients. Development permits loopback origins.
fn nostr_signing_origin_allowed(headers: &hyper::HeaderMap, dev_mode: bool) -> bool {
    let Some(origin) = headers.get("origin").and_then(|value| value.to_str().ok()) else {
        return true;
    };
    let Ok(url) = reqwest::Url::parse(origin) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return false;
    }
    if dev_mode && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1")) {
        return true;
    }
    matches!(url.port_or_known_default(), Some(80 | 443))
}

/// Read-only authenticated methods may skip CSRF, but they must still exist in
/// the dispatcher. The tab signer uses `system.get-hostname` as its lightweight
/// session probe, so keeping the policy in one testable function protects that
/// cross-origin app-gate bootstrap contract.
fn csrf_exempt_method(method: &str) -> bool {
    matches!(
        method,
        "node-messages-received"
            | "server.echo"
            | "server.get-state"
            | "system.stats"
            | "tor.status"
            | "tor.onion-addresses"
            | "bitcoin.relay-status"
            | "federation.list-nodes"
            | "system.get-settings"
            | "system.get-node-key"
            | "system.get-metrics"
            | "system.get-hostname"
    )
}

/// Default dev password when no user is set up (matches mock-backend).
/// Dev builds only — the pre-setup login bypass that reads this is
/// cfg-gated out of release binaries.
#[cfg(debug_assertions)]
pub(crate) const DEV_DEFAULT_PASSWORD: &str = "password123";

pub struct RpcHandler {
    config: Config,
    auth_manager: AuthManager,
    /// Shared lifecycle orchestrator (Dev or Prod). Always `Some` in a normal
    /// build — the only reason it is `Option` is so tests that don't exercise
    /// container RPCs can skip constructing one.
    orchestrator: Option<Arc<dyn ContainerOrchestrator>>,
    /// Concrete handle to the dev orchestrator, when we're in dev mode. Used by
    /// `container-install { manifest_path }` which takes an ad-hoc manifest
    /// path and is not part of the shared trait.
    dev_orchestrator: Option<Arc<DevContainerOrchestrator>>,
    state_manager: Arc<StateManager>,
    pub(crate) metrics_store: Arc<MetricsStore>,
    port_allocator: Arc<tokio::sync::Mutex<PortAllocator>>,
    pub session_store: SessionStore,
    login_rate_limiter: LoginRateLimiter,
    /// Authentication in front of every app port. Built here rather than in
    /// `server.rs` so it shares this handler's session store and login rate
    /// limiter — an attacker must not get a fresh budget of password guesses
    /// by moving from the dashboard to an app port.
    pub(crate) app_gate: Arc<crate::appgate::AppGate>,
    endpoint_rate_limiter: EndpointRateLimiter,
    response_cache: ResponseCache,
    mesh_service: Arc<tokio::sync::RwLock<Option<crate::mesh::MeshService>>>,
    /// LoRa radio firmware-flash job state, sibling to `mesh_service` — one
    /// job at a time, since flashing needs exclusive access to the port.
    flash_job: crate::mesh::flash::FlashJobHandle,
    transport_router: Arc<tokio::sync::RwLock<Option<Arc<crate::transport::TransportRouter>>>>,
    /// Shared content-addressed blob store. Set by ApiHandler after construction
    /// so mesh.send-content / mesh.fetch-content RPCs can reach it without a
    /// second instance and duplicated cap_key.
    pub(crate) blob_store: Arc<tokio::sync::RwLock<Option<Arc<crate::blobs::BlobStore>>>>,
    /// Our own Ed25519 pubkey hex — needed by ContentRef senders for cap scoping
    /// and by ContentRef receivers to request caps scoped to themselves.
    pub(crate) self_pubkey_hex: Arc<tokio::sync::RwLock<Option<String>>>,
    /// Kick the package scanner to run immediately (bypassing the 60s interval).
    /// Used by install/update success paths so the fresh manifest (with populated
    /// `interfaces.main.ui`) lands before we flip state to Running — closes the
    /// "Launch button is missing for up to 60s after install" UX gap.
    pub(crate) scan_kick: Arc<tokio::sync::Notify>,
    /// Monotonic counter incremented by the scan loop after each completed scan.
    /// Install/update success paths subscribe to this to know when a kicked scan
    /// has actually finished before flipping to the terminal state.
    pub(crate) scan_tick: Arc<tokio::sync::watch::Sender<u64>>,
}

impl RpcHandler {
    pub async fn new(
        config: Config,
        state_manager: Arc<StateManager>,
        metrics_store: Arc<MetricsStore>,
        session_store: SessionStore,
        orchestrator: Option<Arc<dyn ContainerOrchestrator>>,
        dev_orchestrator: Option<Arc<DevContainerOrchestrator>>,
    ) -> Result<Self> {
        let auth_manager = AuthManager::new(config.data_dir.clone());
        let port_allocator = Arc::new(tokio::sync::Mutex::new(
            PortAllocator::new(&config.data_dir).await?,
        ));

        let login_rate_limiter = LoginRateLimiter::new();
        let endpoint_rate_limiter = EndpointRateLimiter::new();

        // Spawn periodic rate limiter cleanup (every 5 minutes)
        {
            let limiter = endpoint_rate_limiter.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
                loop {
                    interval.tick().await;
                    limiter.cleanup().await;
                    limiter.cleanup_sessions().await;
                }
            });
        }
        {
            let limiter = login_rate_limiter.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
                loop {
                    interval.tick().await;
                    limiter.cleanup().await;
                }
            });
        }

        let app_gate = Arc::new(crate::appgate::AppGate::new(
            session_store.clone(),
            auth_manager.clone(),
            login_rate_limiter.clone(),
            config.data_dir.clone(),
        ));

        Ok(Self {
            config,
            auth_manager,
            orchestrator,
            dev_orchestrator,
            state_manager,
            metrics_store,
            port_allocator,
            session_store,
            login_rate_limiter,
            app_gate,
            endpoint_rate_limiter,
            response_cache: ResponseCache::new(5),
            mesh_service: Arc::new(tokio::sync::RwLock::new(None)),
            flash_job: crate::mesh::flash::new_job_handle(),
            transport_router: Arc::new(tokio::sync::RwLock::new(None)),
            blob_store: Arc::new(tokio::sync::RwLock::new(None)),
            self_pubkey_hex: Arc::new(tokio::sync::RwLock::new(None)),
            scan_kick: Arc::new(tokio::sync::Notify::new()),
            scan_tick: Arc::new(tokio::sync::watch::channel(0u64).0),
        })
    }

    /// Set the mesh service (called after identity is loaded).
    pub async fn set_mesh_service(self: &Arc<Self>, service: crate::mesh::MeshService) {
        // If the blob store is already initialised, propagate it into the
        // freshly-started mesh state so the listener can persist inline
        // attachments. Mirrors `set_blob_store`'s forward-propagation.
        if let Some(store) = self.blob_store.read().await.as_ref().cloned() {
            *service.shared_state().blob_store.write().await = Some(store);
        }
        // Wire the mesh `!ai` path into the assistant's shared tool loop: a
        // trusted mesh peer's question runs the same tool registry with the
        // operator's persisted grants, and writes still suspend on this
        // node's confirm gate. Same forward-propagation pattern as above.
        *service.shared_state().assistant_handler.write().await = Some(Arc::clone(self));
        *self.mesh_service.write().await = Some(service);
    }

    /// Set the transport router (called after all transports are initialized).
    pub async fn set_transport_router(&self, router: Arc<crate::transport::TransportRouter>) {
        *self.transport_router.write().await = Some(router);
    }

    /// Share the blob store + our pubkey so mesh.send-content / fetch-content
    /// can reach them. Called once from ApiHandler::new.
    pub async fn set_blob_store(
        &self,
        store: Arc<crate::blobs::BlobStore>,
        self_pubkey_hex: String,
    ) {
        *self.blob_store.write().await = Some(store.clone());
        *self.self_pubkey_hex.write().await = Some(self_pubkey_hex);
        // Propagate into a running mesh service if one is already up — keeps
        // `set_blob_store` and `set_mesh_service` order-independent.
        if let Some(svc) = self.mesh_service.read().await.as_ref() {
            *svc.shared_state().blob_store.write().await = Some(store);
        }
    }

    /// Get reference to the mesh service Arc (for MeshTransport wrapper).
    pub fn mesh_service_arc(&self) -> Arc<tokio::sync::RwLock<Option<crate::mesh::MeshService>>> {
        Arc::clone(&self.mesh_service)
    }

    /// Shared Notify handle the package-scanner loop waits on (in addition to
    /// its periodic tick). Install/update success paths call `notify_one()` to
    /// trigger an immediate scan so the fresh manifest lands before we flip to
    /// the terminal Running state.
    pub fn scan_kick(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.scan_kick)
    }

    /// Sender half of the scan-completion watch channel. The scanner bumps this
    /// counter after every finished scan; install/update wait for an advance
    /// after kicking so they know the fresh manifest has landed.
    pub fn scan_tick(&self) -> Arc<tokio::sync::watch::Sender<u64>> {
        Arc::clone(&self.scan_tick)
    }

    fn cookie_suffix_for_request(&self, headers: &hyper::header::HeaderMap) -> &'static str {
        // Only set Secure flag when the original request was over HTTPS.
        // Nginx sends X-Forwarded-Proto: https for HTTPS connections.
        // On LAN HTTP, Secure flag prevents browsers from sending cookies back.
        if self.config.dev_mode {
            return "";
        }
        if let Some(proto) = headers.get("x-forwarded-proto") {
            if proto.as_bytes() == b"https" {
                tracing::debug!("[onboarding] cookie: Secure (X-Forwarded-Proto: https)");
                return "; Secure";
            }
        }
        tracing::debug!("[onboarding] cookie: no Secure flag (HTTP or no X-Forwarded-Proto)");
        ""
    }

    pub async fn handle(
        self: Arc<Self>,
        req: Request<hyper::Body>,
    ) -> Result<Response<hyper::Body>> {
        // Extract session cookie before consuming the request
        let (parts, body) = req.into_parts();
        let session_token = session::extract_session_cookie(&parts.headers);
        let secure_suffix = self.cookie_suffix_for_request(&parts.headers);

        let body_bytes = hyper::body::to_bytes(body)
            .await
            .context("Failed to read body")?;

        let rpc_req: RpcRequest =
            serde_json::from_slice(&body_bytes).context("Invalid RPC request")?;

        debug!("RPC method: {}", rpc_req.method);

        if matches!(
            rpc_req.method.as_str(),
            "node.nostr-sign" | "identity.nostr-sign"
        ) && !nostr_signing_origin_allowed(&parts.headers, self.config.dev_mode)
        {
            return Ok(self.error_response(
                403,
                "Nostr signing from app origins requires the dashboard consent bridge",
                StatusCode::FORBIDDEN,
            ));
        }

        // Enforce authentication for non-allowlisted methods
        let is_unauthenticated = UNAUTHENTICATED_METHODS.contains(&rpc_req.method.as_str());
        let mut new_session_cookies: Option<(String, String)> = None;
        if !is_unauthenticated {
            let mut authenticated = match &session_token {
                Some(token) => self.session_store.validate(token).await,
                None => false,
            };

            // If session invalid, try remember-me token to auto-restore session
            if !authenticated {
                if let Some(remember) = extract_cookie(&parts.headers, "remember") {
                    if crate::session::SessionStore::validate_remember_token(&remember).await {
                        let new_token = self.session_store.create().await;
                        let new_csrf = derive_csrf_token(&new_token).await;
                        tracing::info!("Auto-restored session from remember-me token");
                        new_session_cookies = Some((new_token, new_csrf));
                        authenticated = true;
                    }
                }
            }

            if !authenticated {
                let reason = if session_token.is_none() {
                    "no session cookie"
                } else {
                    "invalid/expired token"
                };
                tracing::warn!(method = %rpc_req.method, reason, "401 Unauthorized — rejecting RPC call");
                return Ok(self.error_response(401, "Unauthorized", StatusCode::UNAUTHORIZED));
            }
        }

        // RBAC: check if the user's role allows this method
        if !is_unauthenticated {
            if let Ok(Some(user)) = self.auth_manager.get_user().await {
                if !user.role.can_access(&rpc_req.method) {
                    return Ok(self.error_response(
                        403,
                        "Forbidden: insufficient permissions",
                        StatusCode::FORBIDDEN,
                    ));
                }
            }
        }

        // CSRF protection: validate X-CSRF-Token header via HMAC derivation from session token.
        // Skip CSRF for read-only methods (polling, status) — CSRF prevents state-changing forgery.
        // Skip when session was just auto-restored from remember-me (browser has stale CSRF cookie).
        let csrf_exempt = csrf_exempt_method(&rpc_req.method);
        if !is_unauthenticated && new_session_cookies.is_none() && !csrf_exempt {
            let csrf_header = parts
                .headers
                .get("x-csrf-token")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let csrf_valid = match (&session_token, &csrf_header) {
                (Some(token), Some(header)) => {
                    use hmac::{Hmac, Mac};
                    use sha2::Sha256;
                    type HmacSha256 = Hmac<Sha256>;
                    let secret = SessionStore::load_or_create_remember_secret().await;
                    let mut mac = match HmacSha256::new_from_slice(&secret) {
                        Ok(m) => m,
                        Err(_) => {
                            return Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, b"{}"));
                        }
                    };
                    mac.update(format!("csrf:{}", token).as_bytes());
                    match hex::decode(header) {
                        Ok(header_bytes) => mac.verify_slice(&header_bytes).is_ok(),
                        Err(_) => false,
                    }
                }
                _ => false,
            };

            if !csrf_valid {
                // Debug: log expected vs received for diagnosis
                if let (Some(token), Some(header)) = (&session_token, &csrf_header) {
                    let expected = derive_csrf_token(token).await;
                    tracing::warn!(
                        method = %rpc_req.method,
                        session_prefix = %&token[..8.min(token.len())],
                        csrf_prefix = %&header[..8.min(header.len())],
                        expected_prefix = %&expected[..8.min(expected.len())],
                        "403 CSRF mismatch — session/csrf/expected prefixes shown"
                    );
                } else {
                    tracing::warn!(
                        method = %rpc_req.method,
                        has_session = session_token.is_some(),
                        has_header = csrf_header.is_some(),
                        "403 CSRF validation failed — rejecting RPC call"
                    );
                }
                return Ok(self.error_response(
                    403,
                    "CSRF token missing or invalid",
                    StatusCode::FORBIDDEN,
                ));
            }
        }

        // Rate limit login attempts
        if rpc_req.method == "auth.login" {
            let client_ip = extract_client_ip(&parts);
            if !self.login_rate_limiter.check(client_ip).await {
                return Ok(self.rate_limit_response());
            }
        }

        // Rate limit sensitive endpoints
        {
            let client_ip = extract_client_ip(&parts);
            if !self
                .endpoint_rate_limiter
                .check(&rpc_req.method, client_ip)
                .await
            {
                return Ok(self.rate_limit_response());
            }
            self.endpoint_rate_limiter
                .record(&rpc_req.method, client_ip)
                .await;
        }

        // Extract params; clone for post-routing use (login 2FA check needs password)
        let params = rpc_req.params;
        let login_params: Option<serde_json::Value> = if rpc_req.method == "auth.login" {
            params.clone()
        } else {
            None
        };

        // Check cache for cacheable methods
        let is_cacheable = CACHEABLE_METHODS.contains(&rpc_req.method.as_str());
        if is_cacheable {
            if let Some(cached) = self.response_cache.get(&rpc_req.method).await {
                let rpc_resp = RpcResponse {
                    result: Some(cached),
                    error: None,
                };
                let body = serde_json::to_vec(&rpc_resp)?;
                return Ok(json_response(StatusCode::OK, &body));
            }
        }

        // Route to handler (track latency for metrics)
        let rpc_start = std::time::Instant::now();
        let result = Self::dispatch(&self, &rpc_req.method, params, &session_token).await;

        // Record RPC latency for monitoring
        let elapsed_ms = rpc_start.elapsed().as_secs_f64() * 1000.0;
        self.metrics_store.record_rpc_latency(elapsed_ms).await;

        // Build response (cache successful results for cacheable methods)
        let mut rpc_resp = match result {
            Ok(data) => {
                if is_cacheable {
                    self.response_cache
                        .set(rpc_req.method.clone(), data.clone())
                        .await;
                }
                RpcResponse {
                    result: Some(data),
                    error: None,
                }
            }
            Err(e) => {
                // `{:#}` renders the whole anyhow context chain. Logging only the
                // outermost context threw away the actual cause: a peer-files
                // failure logged just "Failed to connect to peer", with the real
                // error (Tor SOCKS failure, FIPS resolve, timeout) discarded — so
                // the logs couldn't distinguish a dead peer from a slow circuit.
                // The client-facing message below stays `{}` so internals aren't leaked.
                error!("RPC error on {}: {:#}", rpc_req.method, e);
                let user_message = sanitize_error_message(&e.to_string());
                RpcResponse {
                    result: None,
                    error: Some(RpcError {
                        code: -1,
                        message: user_message,
                        data: None,
                    }),
                }
            }
        };

        let resp_body = serde_json::to_vec(&rpc_resp).context("Failed to serialize response")?;

        let mut response = json_response(StatusCode::OK, &resp_body);

        // Post-dispatch: set cookies for auth-related methods
        let client_ip = extract_client_ip(&parts);
        self.apply_auth_cookies(
            &rpc_req.method,
            &mut rpc_resp,
            &mut response,
            &session_token,
            &login_params,
            &new_session_cookies,
            client_ip,
            secure_suffix,
        )
        .await;

        Ok(response)
    }

    /// Build a JSON error response with the given RPC error code and HTTP status.
    fn error_response(
        &self,
        code: i32,
        message: &str,
        status: StatusCode,
    ) -> Response<hyper::Body> {
        let rpc_resp = RpcResponse {
            result: None,
            error: Some(RpcError {
                code,
                message: message.to_string(),
                data: None,
            }),
        };
        let resp_body = serde_json::to_vec(&rpc_resp).unwrap_or_default();
        json_response(status, &resp_body)
    }

    /// Build a 429 Too Many Requests response.
    fn rate_limit_response(&self) -> Response<hyper::Body> {
        let rpc_resp = RpcResponse {
            result: None,
            error: Some(RpcError {
                code: 429,
                message: "Rate limit exceeded. Try again later.".to_string(),
                data: None,
            }),
        };
        let resp_body = serde_json::to_vec(&rpc_resp).unwrap_or_default();
        let mut resp = json_response(StatusCode::TOO_MANY_REQUESTS, &resp_body);
        resp.headers_mut()
            .insert("Retry-After", cookie_header("60"));
        resp
    }

    /// Apply session/CSRF/remember-me cookies after dispatch for auth-related methods.
    async fn apply_auth_cookies(
        &self,
        method: &str,
        rpc_resp: &mut RpcResponse,
        response: &mut Response<hyper::Body>,
        session_token: &Option<String>,
        login_params: &Option<serde_json::Value>,
        new_session_cookies: &Option<(String, String)>,
        client_ip: std::net::IpAddr,
        secure_suffix: &str,
    ) {
        // Track failed login attempts for rate limiting
        if method == "auth.login" && rpc_resp.error.is_some() {
            self.login_rate_limiter.record_failure(client_ip).await;
        }

        // On successful login, check if 2FA is required. Device-token logins
        // (companion pairing QR) skip the TOTP challenge like remember-me does:
        // the token was minted from an already-authenticated session, and there
        // is no password with which to decrypt the TOTP secret anyway.
        if method == "auth.login" && rpc_resp.error.is_none() {
            let password = login_params
                .as_ref()
                .and_then(|p| p.get("password"))
                .and_then(|v| v.as_str());
            let is_token_login = login_params
                .as_ref()
                .and_then(|p| p.get("token"))
                .and_then(|v| v.as_str())
                .is_some()
                || match password {
                    // Companion device tokens also arrive through the password
                    // field (see handle_auth_login) — those logins get a full
                    // session too; there's no password to decrypt TOTP with.
                    Some(pw) => crate::device_tokens::verify(&self.config.data_dir, pw)
                        .await
                        .is_some(),
                    None => false,
                };
            let totp_enabled =
                !is_token_login && self.auth_manager.is_totp_enabled().await.unwrap_or(false);
            if totp_enabled {
                let password = login_params
                    .as_ref()
                    .and_then(|p| p.get("password"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if let Ok(Some(totp_data)) = self.auth_manager.get_totp_data().await {
                    if let Ok(secret) = crate::totp::decrypt_secret(&totp_data, password) {
                        let token = self.session_store.create_pending(secret).await;
                        let csrf_token = derive_csrf_token(&token).await;
                        self.set_session_cookie(response, &token, secure_suffix);
                        self.set_csrf_cookie(response, &csrf_token, secure_suffix);
                        let totp_body = serde_json::json!({
                            "result": { "requires_totp": true },
                            "error": null
                        });
                        *response.body_mut() =
                            hyper::Body::from(serde_json::to_vec(&totp_body).unwrap_or_default());
                    }
                }
            } else {
                let token = self.session_store.create().await;
                let csrf_token = derive_csrf_token(&token).await;
                let remember_token = self.session_store.create_remember_token().await;
                self.set_session_cookie(response, &token, secure_suffix);
                self.set_csrf_cookie(response, &csrf_token, secure_suffix);
                self.set_remember_cookie(response, &remember_token, secure_suffix);
            }
        }

        // On successful TOTP verification, set the rotated session cookie
        if (method == "auth.login.totp" || method == "auth.login.backup")
            && rpc_resp.error.is_none()
        {
            let new_token_opt = rpc_resp
                .result
                .as_ref()
                .and_then(|r| r.get("new_session_token"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if let Some(new_token) = new_token_opt {
                let csrf_token = derive_csrf_token(&new_token).await;
                let remember_token = self.session_store.create_remember_token().await;
                self.set_session_cookie(response, &new_token, secure_suffix);
                self.set_csrf_cookie(response, &csrf_token, secure_suffix);
                self.set_remember_cookie(response, &remember_token, secure_suffix);
                // Strip the token from the response body
                if let Some(result) = rpc_resp.result.as_mut() {
                    if let Some(obj) = result.as_object_mut() {
                        obj.remove("new_session_token");
                    }
                }
                let body_bytes = serde_json::to_vec(&rpc_resp).unwrap_or_default();
                *response.body_mut() = hyper::Body::from(body_bytes);
            }
        }

        // On password change, rotate the session token for the caller
        if method == "auth.changePassword" && rpc_resp.error.is_none() {
            if let Some(token) = session_token {
                let new_token = self.session_store.rotate(token).await;
                let csrf_token = derive_csrf_token(&new_token).await;
                self.set_session_cookie(response, &new_token, secure_suffix);
                self.set_csrf_cookie(response, &csrf_token, secure_suffix);
            }
        }

        // On logout, invalidate session and expire cookies
        if method == "auth.logout" {
            if let Some(token) = session_token {
                self.session_store.remove(token).await;
            }
            response.headers_mut().append(
                "Set-Cookie",
                cookie_header(&format!(
                    "session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{}",
                    secure_suffix
                )),
            );
            response.headers_mut().append(
                "Set-Cookie",
                cookie_header(&format!(
                    "csrf_token=; SameSite=Lax; Path=/; Max-Age=0{}",
                    secure_suffix
                )),
            );
        }

        // If session was auto-restored from remember-me, set new cookies
        if let Some((new_session, new_csrf)) = new_session_cookies {
            self.set_session_cookie(response, new_session, secure_suffix);
            self.set_csrf_cookie(response, new_csrf, secure_suffix);
        }
    }

    fn set_session_cookie(
        &self,
        response: &mut Response<hyper::Body>,
        token: &str,
        secure_suffix: &str,
    ) {
        response.headers_mut().append(
            "Set-Cookie",
            cookie_header(&format!(
                "session={}; HttpOnly; SameSite=Lax; Path=/{}",
                token, secure_suffix
            )),
        );
    }

    fn set_csrf_cookie(
        &self,
        response: &mut Response<hyper::Body>,
        csrf_token: &str,
        secure_suffix: &str,
    ) {
        response.headers_mut().append(
            "Set-Cookie",
            cookie_header(&format!(
                "csrf_token={}; SameSite=Lax; Path=/{}",
                csrf_token, secure_suffix
            )),
        );
    }

    fn set_remember_cookie(
        &self,
        response: &mut Response<hyper::Body>,
        remember_token: &str,
        secure_suffix: &str,
    ) {
        response.headers_mut().append(
            "Set-Cookie",
            cookie_header(&format!(
                "remember={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}{}",
                remember_token, REMEMBER_TTL, secure_suffix
            )),
        );
    }
}

#[cfg(test)]
mod nostr_signing_origin_tests {
    use super::*;
    use hyper::header::{HeaderMap, HeaderValue, ORIGIN};

    fn headers(origin: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(origin) = origin {
            headers.insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
        }
        headers
    }

    #[test]
    fn signing_accepts_dashboard_and_authenticated_non_browser_clients() {
        assert!(nostr_signing_origin_allowed(&headers(None), false));
        assert!(nostr_signing_origin_allowed(
            &headers(Some("https://node.local")),
            false
        ));
        assert!(nostr_signing_origin_allowed(
            &headers(Some("http://192.0.2.10")),
            false
        ));
    }

    #[test]
    fn signing_rejects_app_ports_but_allows_loopback_dev_server() {
        assert!(!nostr_signing_origin_allowed(
            &headers(Some("https://node.local:8337")),
            false
        ));
        assert!(!nostr_signing_origin_allowed(
            &headers(Some("https://node.local:7778")),
            false
        ));
        assert!(nostr_signing_origin_allowed(
            &headers(Some("http://localhost:5173")),
            true
        ));
    }
}

#[cfg(test)]
mod session_probe_contract_tests {
    use super::*;

    #[test]
    fn signer_session_probe_is_implemented_authenticated_and_read_only() {
        const PROBE: &str = "system.get-hostname";
        const DISPATCHER: &str = include_str!("dispatcher.rs");

        assert!(csrf_exempt_method(PROBE));
        assert!(!UNAUTHENTICATED_METHODS.contains(&PROBE));
        assert!(DISPATCHER.contains("\"system.get-hostname\" =>"));
        assert!(!DISPATCHER.contains("\"system.get-version\" =>"));
    }
}
