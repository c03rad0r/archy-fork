mod blob;
mod cdp;
mod cloud_purchase;
mod content;
mod dwn;
pub(crate) mod lightning_purchase;
mod model_proxy;
mod node_message;
pub(crate) mod onchain_purchase;
mod proxy;
mod purchase;
mod registered_media;
mod remote_input;
mod remote_relay;
mod rental_playback;
mod routstr_proxy;
mod terminal;
mod websocket;

use crate::api::rpc::RpcHandler;
use crate::blobs::BlobStore;
use crate::config::Config;
use crate::container::{ContainerOrchestrator, DevContainerOrchestrator};
use crate::monitoring::MetricsStore;
use crate::session::{self, SessionStore};
use crate::state::StateManager;
use anyhow::Result;
use hyper::{Method, Request, Response, StatusCode};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::debug;

/// Build an HTTP response without unwrap. Falls back to a plain 500 if builder fails.
// Used by handler submodules after unwrap elimination
#[allow(dead_code)]
pub(super) fn build_response(
    status: StatusCode,
    content_type: &str,
    body: hyper::Body,
) -> Response<hyper::Body> {
    Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .body(body)
        .unwrap_or_else(|_| Response::new(hyper::Body::from("Internal error")))
}

pub struct ApiHandler {
    config: Config,
    rpc_handler: Arc<RpcHandler>,
    state_manager: Arc<StateManager>,
    metrics_store: Arc<MetricsStore>,
    session_store: SessionStore,
    /// Broadcast channel for relaying companion app input to remote browsers.
    input_relay_tx: broadcast::Sender<String>,
    /// Reverse broadcast channel: the kiosk browser publishes "open this URL
    /// externally" requests here, and the companion (phone) socket forwards them
    /// to the phone's default browser. Lets "open in external browser" apps —
    /// which the kiosk can't usefully open itself — launch on the controller.
    external_open_tx: broadcast::Sender<String>,
    /// Bridge that dispatches companion input into the local kiosk Chromium
    /// as trusted CDP events (reaches inside cross-origin app iframes).
    /// Inert (never connects) on nodes without a kiosk.
    cdp_bridge: cdp::CdpBridge,
    /// Content-addressed blob store for attachments shared over mesh/federation.
    blob_store: Arc<BlobStore>,
    /// Our own node pubkey (hex) — used to self-sign debug/test capabilities.
    self_pubkey_hex: String,
}

impl ApiHandler {
    pub async fn new(
        config: Config,
        state_manager: Arc<StateManager>,
        metrics_store: Arc<MetricsStore>,
        orchestrator: Option<Arc<dyn ContainerOrchestrator>>,
        dev_orchestrator: Option<Arc<DevContainerOrchestrator>>,
    ) -> Result<Self> {
        let session_store = SessionStore::new().await;
        let rpc_handler = Arc::new(
            RpcHandler::new(
                config.clone(),
                state_manager.clone(),
                metrics_store.clone(),
                session_store.clone(),
                orchestrator,
                dev_orchestrator,
            )
            .await?,
        );
        let (input_relay_tx, _) = broadcast::channel(64);
        let (external_open_tx, _) = broadcast::channel(16);
        let cdp_bridge = cdp::CdpBridge::spawn();

        // Derive a blob-store capability key from the node's Ed25519 signing
        // key. SHA-256 domain-separated so rotating the identity rotates
        // every outstanding capability token (intentional — prevents a
        // replaced node from honouring old caps).
        let identity_dir = config.data_dir.join("identity");
        let identity = crate::identity::NodeIdentity::load_or_create(&identity_dir).await?;
        let mut hasher = Sha256::new();
        hasher.update(identity.signing_key().to_bytes());
        hasher.update(b"|archipelago-blob-cap-v1");
        let mut cap_key = [0u8; 32];
        cap_key.copy_from_slice(&hasher.finalize());
        let blob_store = Arc::new(BlobStore::open(&config.data_dir, cap_key).await?);
        let self_pubkey_hex = hex::encode(identity.signing_key().verifying_key().as_bytes());

        // Share blob store with the RPC layer so mesh.send-content /
        // mesh.fetch-content can reach the same instance (single cap_key,
        // single on-disk root) without re-opening it.
        rpc_handler
            .set_blob_store(blob_store.clone(), self_pubkey_hex.clone())
            .await;

        Ok(Self {
            config,
            rpc_handler,
            state_manager,
            metrics_store,
            session_store,
            input_relay_tx,
            external_open_tx,
            cdp_bridge,
            blob_store,
            self_pubkey_hex,
        })
    }

    /// Access the RPC handler (for service initialization after construction).
    pub fn rpc_handler(&self) -> &Arc<RpcHandler> {
        &self.rpc_handler
    }

    /// Check if the request has a valid session cookie.
    async fn is_authenticated(&self, headers: &hyper::HeaderMap) -> bool {
        match session::extract_session_cookie(headers) {
            Some(token) => self.session_store.validate(&token).await,
            None => false,
        }
    }

    /// Server-side fetch of the upstream app catalog so the browser can
    /// load it without fighting CORS (upstream Gitea emits no ACAO) or
    /// CSP (the fallback IP-port URL isn't in `connect-src`). The upstream
    /// list is derived from the operator's configured container registries
    /// so switching mirrors in Settings changes the App Store source too —
    /// each active registry contributes one Gitea `raw/branch/main/catalog.json`
    /// URL (http or https per `tls_verify`), tried in priority order.
    /// If registry config can't be loaded, falls back to the hardcoded OVH
    /// URL so the App Store still renders on nodes that haven't persisted
    /// a registry config yet. 15s total timeout.
    async fn handle_app_catalog_proxy(&self) -> Result<Response<hyper::Body>> {
        // The daemon already refreshes and verifies releases/app-catalog.json.
        // Serve that release-root-anchored cache first so a newly published app
        // appears immediately, without a frontend release. The old external UI
        // catalog below is emergency compatibility only; it must never override
        // a healthy signed catalog (Cuprate was invisible for exactly that reason).
        if let Ok(body) =
            crate::container::app_catalog::verified_catalog_body(&self.config.data_dir).await
        {
            return Ok(Response::builder()
                .status(hyper::StatusCode::OK)
                .header("Content-Type", "application/json")
                .header("Cache-Control", "no-cache")
                .body(hyper::Body::from(body))?);
        }

        let mut upstreams: Vec<String> = Vec::new();
        if let Ok(config) = crate::container::registry::load_registries(&self.config.data_dir).await
        {
            for reg in config.active_registries() {
                let scheme = if reg.tls_verify { "https" } else { "http" };
                // Gitea raw URL: <scheme>://<host>/<namespace>/app-catalog/raw/branch/main/catalog.json.
                // reg.url already includes the namespace (e.g. "host/lfg2025"),
                // so we just tack on the repo + raw path.
                upstreams.push(format!(
                    "{}://{}/app-catalog/raw/branch/main/catalog.json",
                    scheme, reg.url
                ));
            }
        }
        if upstreams.is_empty() {
            upstreams.push(
                "https://source.archipelago-foundation.org/lfg2025/app-catalog/raw/branch/main/catalog.json"
                    .to_string(),
            );
        }

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                return Ok(build_response(
                    hyper::StatusCode::INTERNAL_SERVER_ERROR,
                    "text/plain",
                    hyper::Body::from(format!("client build failed: {}", e)),
                ));
            }
        };
        for url in &upstreams {
            match client.get(url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(bytes) = resp.bytes().await {
                        return Ok(Response::builder()
                            .status(hyper::StatusCode::OK)
                            .header("Content-Type", "application/json")
                            .header("Cache-Control", "public, max-age=3600")
                            .body(hyper::Body::from(bytes))
                            .unwrap_or_else(|_| {
                                Response::new(hyper::Body::from("proxy response build failed"))
                            }));
                    }
                }
                _ => continue,
            }
        }
        Ok(build_response(
            hyper::StatusCode::BAD_GATEWAY,
            "text/plain",
            hyper::Body::from("all upstream catalog URLs failed"),
        ))
    }

    /// Serve an encrypted backup archive (`<data_dir>/backups/<id>.bak`) as a
    /// browser download. The archive is passphrase-encrypted at rest; the
    /// session gate at the route controls who can fetch it.
    async fn handle_backup_download(&self, path: &str) -> Result<Response<hyper::Body>> {
        let id = path.strip_prefix("/api/blob/backup/").unwrap_or("");
        // Backup ids are UUIDs — reject anything that could traverse paths.
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return Ok(build_response(
                StatusCode::BAD_REQUEST,
                "application/json",
                hyper::Body::from(r#"{"error":"invalid backup id"}"#),
            ));
        }
        let file = self
            .config
            .data_dir
            .join("backups")
            .join(format!("{id}.bak"));
        match tokio::fs::read(&file).await {
            Ok(bytes) => Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/octet-stream")
                .header(
                    "Content-Disposition",
                    format!("attachment; filename=\"archipelago-backup-{id}.bak\""),
                )
                .header("Content-Length", bytes.len())
                .body(hyper::Body::from(bytes))
                .unwrap_or_else(|_| Response::new(hyper::Body::from("Internal error")))),
            Err(_) => Ok(build_response(
                StatusCode::NOT_FOUND,
                "application/json",
                hyper::Body::from(r#"{"error":"backup not found"}"#),
            )),
        }
    }

    /// Build a 401 Unauthorized JSON response.
    fn unauthorized() -> Response<hyper::Body> {
        let body = serde_json::json!({ "error": "Unauthorized" });
        let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
        Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header("Content-Type", "application/json")
            .body(hyper::Body::from(body_bytes))
            .unwrap()
    }

    /// A 401 that still carries CORS headers, for endpoints fetched
    /// cross-origin by same-node app UIs (e.g. the LND wallet UI on its own
    /// port). Without the ACAO header the browser surfaces an opaque CORS
    /// error instead of the 401, so the app can't tell it just needs auth.
    /// `origin` is the already-validated reflect value from `app_cors_origin`
    /// (empty string when the origin isn't allowed → no CORS header added).
    fn unauthorized_cors(origin: &str) -> Response<hyper::Body> {
        let body = serde_json::json!({ "error": "Unauthorized" });
        let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
        let mut builder = Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header("Content-Type", "application/json")
            .header("Vary", "Origin");
        if !origin.is_empty() {
            builder = builder
                .header("Access-Control-Allow-Origin", origin)
                .header("Access-Control-Allow-Credentials", "true");
        }
        builder.body(hyper::Body::from(body_bytes)).unwrap()
    }

    /// Allowed CORS origins derived from the config host IP.
    fn allowed_origins(&self) -> Vec<String> {
        let mut origins = vec![
            format!("http://{}", self.config.host_ip),
            format!("https://{}", self.config.host_ip),
        ];
        if self.config.dev_mode {
            origins.push("http://localhost:8100".to_string()); // Vite dev server
        }
        origins
    }

    /// Validate the Origin header against allowed origins.
    /// Returns the matched origin if valid, None if cross-origin is not allowed.
    fn validate_origin(&self, headers: &hyper::HeaderMap) -> Option<String> {
        let origin = headers.get("origin").and_then(|v| v.to_str().ok())?;
        let allowed = self.allowed_origins();
        if allowed.iter().any(|a| a == origin) {
            Some(origin.to_string())
        } else {
            None
        }
    }

    // A node app on another port shares host-only owner cookies. Never grant
    // that app access to the owner shell, even though other app APIs accept
    // same-host cross-port origins. Browser terminal requests must originate
    // from the exact dashboard authority (Host includes its port).
    fn has_terminal_origin(headers: &hyper::HeaderMap) -> bool {
        let Some(origin) = headers
            .get(hyper::header::ORIGIN)
            .and_then(|v| v.to_str().ok())
        else {
            return false;
        };
        let Some(host) = headers
            .get(hyper::header::HOST)
            .and_then(|v| v.to_str().ok())
        else {
            return false;
        };
        origin == format!("http://{host}") || origin == format!("https://{host}")
    }

    // Browsers do not send Origin on a same-origin GET. Fetch Metadata is a
    // browser-controlled header and distinguishes the dashboard from an app on
    // another port, which is only same-site. Mutating requests and WebSockets
    // still require the exact Origin above.
    fn has_terminal_read_origin(headers: &hyper::HeaderMap) -> bool {
        if headers.contains_key(hyper::header::ORIGIN) {
            return Self::has_terminal_origin(headers);
        }
        headers
            .get("sec-fetch-site")
            .and_then(|value| value.to_str().ok())
            == Some("same-origin")
    }

    /// Permissive origin check for the share-to-mesh iframe intent: any scheme
    /// http(s):// followed by the configured host_ip, optionally `:port`. Apps
    /// proxied under other ports (APP_PORTS) call this from within the same
    /// node, so they share host_ip but not port. The session cookie still has
    /// to be valid — this is a sanity check, not the primary auth.
    fn validate_app_origin(&self, headers: &hyper::HeaderMap) -> Option<String> {
        let origin = headers.get("origin").and_then(|v| v.to_str().ok())?;
        // Allow localhost dev server too so the Vite frontend can exercise it.
        if self.config.dev_mode && origin == "http://localhost:8100" {
            return Some(origin.to_string());
        }
        let host_ip = &self.config.host_ip;
        let matches = |scheme: &str| -> bool {
            let prefix = format!("{}{}", scheme, host_ip);
            if origin == prefix {
                return true;
            }
            let with_port = format!("{}:", prefix);
            origin.starts_with(&with_port)
                && origin[with_port.len()..]
                    .bytes()
                    .all(|b| b.is_ascii_digit())
        };
        if matches("http://") || matches("https://") {
            Some(origin.to_string())
        } else {
            None
        }
    }

    /// CORS origin to echo for same-node app → backend calls (e.g. the LND
    /// wallet UI, served on its own APP_PORTS port). Such apps share the node's
    /// host but use a different port, so the strict allowlist (`host_ip`, no
    /// port) rejects them and the browser gets no `Access-Control-Allow-Origin`
    /// header ("blocked by CORS policy"). Reflect the Origin when its host
    /// matches the request's own `Host` header — i.e. the app lives on the same
    /// address the node is being reached by, which transparently covers the LAN
    /// IP, the Tailscale IP, localhost, and the `.onion` address without needing
    /// to enumerate them. Auth is still enforced by the session cookie; this
    /// only authorizes the browser to *read* the reply. Returns "" (no echoed
    /// origin) when there is no match.
    fn app_cors_origin(&self, headers: &hyper::HeaderMap) -> String {
        if let Some(origin) = self.validate_origin(headers) {
            return origin;
        }
        let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) else {
            return String::new();
        };
        // host portion (no scheme, no port) of an `scheme://host[:port]` value
        let host_of = |s: &str| -> Option<String> {
            let after_scheme = s.split_once("://").map(|(_, r)| r).unwrap_or(s);
            let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
            let host = host_port
                .rsplit_once(':')
                .map(|(h, _)| h)
                .unwrap_or(host_port);
            (!host.is_empty()).then(|| host.to_string())
        };
        let origin_host = host_of(origin);
        let req_host = headers
            .get(hyper::header::HOST)
            .and_then(|v| v.to_str().ok())
            .and_then(host_of);
        match (origin_host, req_host) {
            (Some(o), Some(r)) if o == r => origin.to_string(),
            _ => String::new(),
        }
    }

    pub async fn handle_request(&self, req: Request<hyper::Body>) -> Result<Response<hyper::Body>> {
        let path = req.uri().path().to_string();
        let method = req.method().clone();

        if path.starts_with("/api/rental-playback/") {
            return self
                .handle_local_rental_request(&method, &path, req.headers())
                .await;
        }

        // Handle CORS preflight for all routes
        if method == Method::OPTIONS {
            let mut builder = Response::builder()
                .status(StatusCode::NO_CONTENT)
                .header("Vary", "Origin");
            let preflight_origin = self.app_cors_origin(req.headers());
            if !preflight_origin.is_empty() {
                builder = builder
                    .header("Access-Control-Allow-Origin", &preflight_origin)
                    .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
                    .header("Access-Control-Allow-Headers", "Content-Type, X-CSRF-Token")
                    .header("Access-Control-Allow-Credentials", "true");
            }
            return Ok(builder.body(hyper::Body::empty()).unwrap());
        }

        // WebSocket upgrade — validate session before upgrading
        if method == Method::GET && path == "/ws/db" {
            if !self.is_authenticated(req.headers()).await {
                tracing::warn!("401 WebSocket /ws/db — session invalid or missing");
                return Ok(Self::unauthorized());
            }
            return Self::handle_websocket(
                req,
                self.state_manager.clone(),
                self.metrics_store.clone(),
            )
            .await;
        }

        // Owner terminal attachment — the browser socket is disposable; the
        // authenticated tmux session survives reconnects and browser closes.
        if method == Method::GET && path == "/ws/terminal" {
            if !self.is_authenticated(req.headers()).await {
                tracing::warn!("401 WebSocket /ws/terminal — session invalid or missing");
                return Ok(Self::unauthorized());
            }
            if !Self::has_terminal_origin(req.headers()) {
                return Ok(build_response(
                    StatusCode::FORBIDDEN,
                    "application/json",
                    hyper::Body::from(r#"{"error":"Terminal origin denied"}"#),
                ));
            }
            return Self::handle_terminal_websocket(req).await;
        }

        // Remote input WebSocket — companion app sends keyboard/mouse events
        if method == Method::GET && path == "/ws/remote-input" {
            if !self.is_authenticated(req.headers()).await {
                tracing::warn!("401 WebSocket /ws/remote-input — session invalid or missing");
                return Ok(Self::unauthorized());
            }
            return Self::handle_remote_input(
                req,
                self.input_relay_tx.clone(),
                self.external_open_tx.subscribe(),
                self.cdp_bridge.clone(),
            )
            .await;
        }

        // Remote relay WebSocket — browser receives companion input events
        if method == Method::GET && path == "/ws/remote-relay" {
            if !self.is_authenticated(req.headers()).await {
                tracing::warn!("401 WebSocket /ws/remote-relay — session invalid or missing");
                return Ok(Self::unauthorized());
            }
            return Self::handle_remote_relay(
                req,
                self.input_relay_tx.subscribe(),
                self.external_open_tx.clone(),
                self.cdp_bridge.clone(),
            )
            .await;
        }

        if method == Method::POST && path == lightning_purchase::ROUTE {
            return self.handle_lightning_purchase(req).await;
        }
        // Purchase routes bound the original body before the generic buffer.
        if method == Method::POST
            && matches!(
                path.as_str(),
                crate::content_purchase_protocol::PREPARE_OFFER_ROUTE
                    | crate::content_purchase_protocol::OFFER_ROUTE
                    | crate::content_purchase_protocol::ACCEPT_ROUTE
                    | crate::content_purchase_protocol::SETTLE_ROUTE
                    | crate::content_purchase_protocol::STATUS_ROUTE
                    | crate::content_purchase_protocol::CANCEL_ROUTE
            )
        {
            return self.handle_purchase_request(req).await;
        }

        if method == Method::POST
            && path.starts_with("/content/registered_")
            && path.contains("/rental/")
            && (path.ends_with("/prepare") || path.ends_with("/start"))
        {
            return self.handle_rental_control(req).await;
        }

        // Convert body to bytes for non-WS routes
        let headers = req.headers().clone();
        let query_string = req.uri().query().map(|s| s.to_string()).unwrap_or_default();
        let (parts, body) = req.into_parts();
        let body_bytes = hyper::body::to_bytes(body)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read body: {}", e))?;
        let req_with_bytes = Request::from_parts(parts, hyper::Body::from(body_bytes.clone()));

        debug!("{} {}", method, path);

        match (method, path.as_str()) {
            // RPC — auth is handled inside rpc handler per-method
            (Method::POST, "/rpc/v1") => {
                let origin = self.app_cors_origin(&headers);
                let mut response = self.rpc_handler.clone().handle(req_with_bytes).await?;
                if !origin.is_empty() {
                    response.headers_mut().insert(
                        "access-control-allow-origin",
                        hyper::header::HeaderValue::from_str(&origin)?,
                    );
                    response.headers_mut().insert(
                        "access-control-allow-credentials",
                        hyper::header::HeaderValue::from_static("true"),
                    );
                    response.headers_mut().insert(
                        hyper::header::VARY,
                        hyper::header::HeaderValue::from_static("Origin"),
                    );
                }
                Ok(response)
            }

            // AIUI model proxy — session-gated forwarder to Claude/Ollama,
            // replacing the unauthenticated claude-api-proxy.py sidecar and
            // the /aiui/api/openrouter/ open relay (13-02-PLAN.md,
            // T-13-08/T-13-09/T-13-10/T-13-11). The daemon re-derives auth
            // from the cookie inside handle_model_proxy — it does not trust
            // nginx to have gated the request already, the same "don't trust
            // the front door" discipline as /lnd-connect-info below.
            (_, p)
                if p.starts_with("/aiui/api/claude/")
                    || p.starts_with("/aiui/api/ollama/")
                    || p.starts_with("/aiui/api/web-search") =>
            {
                self.handle_model_proxy(req_with_bytes, p).await
            }

            // AIUI Routstr proxy — the explicit, user-selected Routstr
            // provider (model catalog + Cashu-paid completions), same
            // session-gate discipline as the model proxy above. D-05: paid
            // requests are refused unless the operator has armed a budget.
            (_, p) if p.starts_with("/aiui/api/routstr/") => {
                self.handle_routstr_proxy(req_with_bytes, p).await
            }

            // Health — unauthenticated, returns JSON with service status
            (Method::GET, "/health") => {
                let recovery_complete = crate::crash_recovery::is_recovery_complete();
                let uptime = crate::crash_recovery::uptime_seconds();
                let health_status = if recovery_complete { "ok" } else { "degraded" };
                let status = serde_json::json!({
                    "status": health_status,
                    "crash_recovery_complete": recovery_complete,
                    "uptime_seconds": uptime,
                    "version": env!("CARGO_PKG_VERSION"),
                    "services": {
                        "rpc": true,
                        "sessions": true,
                    }
                });
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .body(hyper::Body::from(
                        serde_json::to_vec(&status).unwrap_or_default(),
                    ))
                    .unwrap())
            }

            (Method::GET, "/api/terminal/sessions") => {
                if !self.is_authenticated(&headers).await {
                    return Ok(Self::unauthorized());
                }
                if !Self::has_terminal_read_origin(&headers) {
                    return Ok(build_response(
                        StatusCode::FORBIDDEN,
                        "application/json",
                        hyper::Body::from(r#"{"error":"Terminal origin denied"}"#),
                    ));
                }
                terminal::list_response().await
            }
            (Method::POST, "/api/terminal/sessions") => {
                if !self.is_authenticated(&headers).await {
                    return Ok(Self::unauthorized());
                }
                if !Self::has_terminal_origin(&headers) {
                    return Ok(build_response(
                        StatusCode::FORBIDDEN,
                        "application/json",
                        hyper::Body::from(r#"{"error":"Terminal origin denied"}"#),
                    ));
                }
                terminal::create(&body_bytes).await
            }

            // Node message — P2P endpoint (authenticated by source validation, not cookie)
            (Method::POST, "/archipelago/node-message") => {
                Self::handle_node_message(body_bytes).await
            }

            // Mesh typed envelope relay over federation — peers POST
            // pre-encoded TypedEnvelope wire bytes here when the envelope is
            // too large for a single LoRa frame (primarily ContentRef). No
            // session auth: the body carries a pubkey + ed25519 signature
            // over the wire bytes which we verify before dispatching.
            (Method::POST, "/archipelago/mesh-typed") => {
                Self::handle_mesh_typed_relay(self.rpc_handler.clone(), body_bytes).await
            }

            // Backup archive download — session-gated. Lives under /api/blob/
            // so the existing nginx `location /api/blob` prefix proxies it on
            // every fleet node without a config change.
            (Method::GET, p) if p.starts_with("/api/blob/backup/") => {
                if !self.is_authenticated(&headers).await {
                    return Ok(Self::unauthorized());
                }
                self.handle_backup_download(p).await
            }

            // Blob upload — local/session use only. Session-authenticated so
            // only the node owner can push attachments into the blob store.
            (Method::POST, "/api/blob") => {
                if !self.is_authenticated(&headers).await {
                    return Ok(Self::unauthorized());
                }
                Self::handle_blob_upload(
                    &self.blob_store,
                    &self.self_pubkey_hex,
                    &self.config.data_dir,
                    &headers,
                    body_bytes,
                )
                .await
            }

            // Share-to-mesh intent — marketplace app iframes POST a file here
            // to stage it as a mesh attachment. Same body format as /api/blob
            // (raw bytes + X-Blob-Mime/X-Blob-Filename headers). The app is
            // expected to postMessage `{type:'share-to-mesh', cid, ...}` to
            // its parent window afterwards so the Mesh view can pick it up.
            // Authenticated by session cookie + a relaxed Origin check (any
            // port on the archipelago host is allowed, so proxied apps on
            // their own ports can reach it with credentials:'include').
            (Method::POST, "/api/share-to-mesh") => {
                if !self.is_authenticated(&headers).await {
                    return Ok(Self::unauthorized());
                }
                let origin = match self.validate_app_origin(&headers) {
                    Some(o) => o,
                    None => {
                        return Ok(build_response(
                            StatusCode::FORBIDDEN,
                            "text/plain",
                            hyper::Body::from("origin not allowed"),
                        ))
                    }
                };
                Self::handle_share_to_mesh(
                    &self.blob_store,
                    &self.self_pubkey_hex,
                    &headers,
                    body_bytes,
                    &origin,
                )
                .await
            }

            // Blob download — peer-facing. No session required; authenticated
            // by HMAC capability token signed when the blob ref was shared.
            (Method::GET, p) if p.starts_with("/blob/") => {
                Self::handle_blob_download(&self.blob_store, p, &query_string).await
            }

            // Immutable registered rentals use durable seller receipts and their
            // first-open window, never legacy mutable filename shares.
            (Method::GET, p) if p.starts_with("/content/") && p.contains("/purchase/") => {
                self.handle_cloud_purchase(p, &headers).await
            }
            (Method::GET, p) if p.starts_with("/content/registered_") && p.contains("/rental/") => {
                self.handle_registered_rental(p, &headers).await
            }

            // Content preview — degraded previews for paid content (no auth, no payment)
            (Method::GET, p) if p.starts_with("/content/") && p.ends_with("/preview") => {
                Self::handle_content_preview(p, &self.config).await
            }

            // Lightning-invoice peer-file sale (#46): mint invoice / poll settlement
            (Method::GET, p) if p.starts_with("/content/") && p.ends_with("/invoice") => {
                self.handle_content_invoice(p, &headers).await
            }
            (Method::GET, p) if p.starts_with("/content/") && p.contains("/invoice-status/") => {
                self.handle_content_invoice_status(p).await
            }

            // On-chain peer-file sale (#46): issue address / poll for payment
            (Method::GET, p) if p.starts_with("/content/") && p.contains("/onchain-status/") => {
                self.handle_content_onchain_status(p).await
            }
            (Method::GET, p) if p.starts_with("/content/") && p.ends_with("/onchain") => {
                self.handle_content_onchain(p, &headers).await
            }

            // Content serving — peers access shared content over Tor (no session auth);
            // a valid operator session cookie unlocks the owner path inside.
            (Method::GET, p) if p.starts_with("/content/") => {
                self.handle_content_request(p, &headers, &self.config).await
            }

            // Content catalog — list available content (no session auth, for peers)
            (Method::GET, "/content") => self.handle_content_catalog(&headers).await,

            // Electrs status — unauthenticated (read-only sync status)
            (Method::GET, "/electrs-status") => Self::handle_electrs_status().await,
            (Method::GET, "/bitcoin-status") => Self::handle_bitcoin_status().await,

            // App-catalog proxy — fetches catalog.json from the configured
            // upstream URLs server-side so the browser doesn't hit CORS
            // (upstream Gitea has no ACAO header) or CSP (IP-port upstream
            // falls outside `connect-src`). Session-authenticated so only
            // the logged-in node owner can spin up fetches.
            (Method::GET, "/api/node-app-catalog") => {
                if !self.is_authenticated(&headers).await {
                    return Ok(Self::unauthorized());
                }
                let data_dir = self.config.data_dir.clone();
                let result = tokio::task::spawn_blocking(move || {
                    crate::container::node_catalog::verified_body(&data_dir)
                })
                .await
                .unwrap_or_else(|error| Err(anyhow::anyhow!(error)));
                let (status, body) = match result {
                    Ok(Some(body)) => (StatusCode::OK, body),
                    Ok(None) => (StatusCode::NOT_FOUND, "{}".to_owned()),
                    Err(error) => {
                        tracing::warn!("Node demo catalog rejected: {error}");
                        (
                            StatusCode::CONFLICT,
                            "{\"error\":\"Node demo catalog is unavailable\"}".to_owned(),
                        )
                    }
                };
                Ok(Response::builder()
                    .status(status)
                    .header("Content-Type", "application/json")
                    .header("Cache-Control", "private, no-store")
                    .body(hyper::Body::from(body))?)
            }

            (Method::GET, "/api/app-catalog") => {
                if !self.is_authenticated(&headers).await {
                    return Ok(Self::unauthorized());
                }
                self.handle_app_catalog_proxy().await
            }

            // Pine node status — public tier (version/uptime/height/sync/peer
            // counts) is unauthenticated like /bitcoin-status; Lightning
            // balances + latest mesh message additionally require the bearer
            // token the pine/HA seeder minted (or a valid session).
            (Method::GET, "/api/pine/status") => {
                let bearer = headers
                    .get(hyper::header::AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.strip_prefix("Bearer "))
                    .unwrap_or("");
                let authorized = self.rpc_handler.pine_status_token_ok(bearer).await
                    || self.is_authenticated(&headers).await;
                let body = self.rpc_handler.pine_status_json(authorized).await;
                Ok(build_response(
                    StatusCode::OK,
                    "application/json",
                    hyper::Body::from(serde_json::to_vec(&body).unwrap_or_default()),
                ))
            }

            // Session probe for app-container nginx `auth_request` gates.
            //
            // App UIs run their own nginx and proxy selected paths into this
            // backend. Some of those paths inject credentials the caller never
            // supplied (bitcoin-ui's /bitcoin-rpc/ adds Bitcoin Core's Basic
            // auth), which makes the proxy itself the authorization boundary —
            // and nginx has no way to validate a session cookie on its own. This
            // endpoint gives it one: 204 when the request carries a valid
            // session, 401 otherwise. Body is deliberately empty; `auth_request`
            // discards it and it must never become an oracle.
            (Method::GET, "/auth/session-check") => {
                if !self.is_authenticated(&headers).await {
                    return Ok(Self::unauthorized());
                }
                Ok(Response::builder()
                    .status(StatusCode::NO_CONTENT)
                    .header("Cache-Control", "no-store")
                    .body(hyper::Body::empty())
                    .unwrap())
            }

            // LND connect info — REQUIRES A SESSION. This response is a complete
            // remote-control package for the node's Lightning wallet: the admin
            // macaroon, the TLS cert, the gRPC/REST ports and the onion address.
            // Anyone who receives it can drain the wallet from anywhere, and the
            // onion means they keep that ability after losing network access.
            //
            // It used to carry no backend check, on two premises that were both
            // false in production:
            //
            //   "nginx validates the session cookie" — the MAIN nginx does. But
            //   the lnd-ui app container runs its OWN nginx on :18083 that
            //   proxies /lnd-connect-info straight here, forwarding whatever
            //   cookies arrived, including none. That second front door never
            //   performed the presence check the premise depended on.
            //
            //   "the backend is bound to 127.0.0.1 so only nginx can reach it" —
            //   true of the backend socket, but irrelevant: :18083 is a reachable
            //   proxy INTO it, it binds 0.0.0.0, and it is explicitly on the
            //   fips0 mesh allowlist (fips/app_ports.rs). So an unauthenticated
            //   GET from any mesh peer, LAN host or Tailscale peer returned the
            //   admin macaroon. Verified live on a test node 2026-08-02.
            //
            // The lesson generalises: an auth check performed by one reverse
            // proxy is not an auth check, because it only holds for traffic that
            // arrived through that proxy. Authorisation belongs at the resource.
            // Do not remove this in favour of a front-door check again.
            //
            // 401s carry CORS headers for the same reason /proxy/lnd/ does: the
            // wallet UI fetches this cross-origin, so a bare 401 without them
            // surfaces in the browser as an unreadable CORS failure.
            (Method::GET, "/lnd-connect-info") => {
                let origin = self.app_cors_origin(&headers);
                if !self.is_authenticated(&headers).await {
                    return Ok(Self::unauthorized_cors(&origin));
                }
                Self::handle_lnd_connect_info(self.rpc_handler.clone(), &origin).await
            }

            // Container logs — requires session
            (Method::GET, path) if path.starts_with("/api/container/logs") => {
                if !self.is_authenticated(&headers).await {
                    return Ok(Self::unauthorized());
                }
                let origin = self.validate_origin(&headers).unwrap_or_default();
                Self::handle_container_logs_http(self.rpc_handler.clone(), path, &origin).await
            }

            // Peer content streaming proxy — Range-streams a peer's media file
            // so <video>/<audio> can seek/play (B3). Same-origin, session-gated.
            (Method::GET, p) if p.starts_with("/api/peer-content/") => {
                if !self.is_authenticated(&headers).await {
                    return Ok(Self::unauthorized());
                }
                self.handle_peer_content_stream(p, &headers).await
            }

            // LND proxy — requires session. The LND wallet UI calls this
            // cross-origin from its own app port, so even the 401 must carry
            // CORS headers; otherwise the browser reports a bare CORS failure
            // ("No 'Access-Control-Allow-Origin' header") instead of a
            // readable 401 the UI can act on.
            (Method::GET, path) if path.starts_with("/proxy/lnd/") => {
                let origin = self.app_cors_origin(&headers);
                if !self.is_authenticated(&headers).await {
                    return Ok(Self::unauthorized_cors(&origin));
                }
                Self::handle_lnd_proxy(self.rpc_handler.clone(), path, &origin).await
            }

            // DWN health — unauthenticated
            (Method::GET, "/dwn/health") => Self::handle_dwn_health(&self.config).await,

            // DWN message processing — peers access over Tor for sync (no session auth)
            (Method::POST, "/dwn") => Self::handle_dwn_message(body_bytes, &self.config).await,

            _ => Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(hyper::Body::from("Not Found"))
                .unwrap()),
        }
    }
}

/// Validate that an app ID matches the safe pattern: lowercase alphanumeric + hyphens.
fn is_valid_app_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && id.as_bytes()[0] != b'-'
}

/// Validate that a pubkey is a 64-char hex string.
fn is_valid_pubkey_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Strip newlines and ANSI escape sequences from strings before logging.
fn sanitize_log_string(s: &str) -> String {
    s.replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\x1b', "")
}

/// Strip HTML-sensitive characters to prevent XSS when stored/rendered.
fn sanitize_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[cfg(test)]
mod terminal_origin_tests {
    use super::ApiHandler;
    use hyper::header::{HeaderMap, HeaderValue, HOST, ORIGIN};

    fn allowed(host: &str, origin: Option<&str>) -> bool {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_str(host).unwrap());
        if let Some(origin) = origin {
            headers.insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
        }
        ApiHandler::has_terminal_origin(&headers)
    }

    fn read_allowed(host: &str, origin: Option<&str>, fetch_site: Option<&str>) -> bool {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_str(host).unwrap());
        if let Some(origin) = origin {
            headers.insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
        }
        if let Some(fetch_site) = fetch_site {
            headers.insert("sec-fetch-site", HeaderValue::from_str(fetch_site).unwrap());
        }
        ApiHandler::has_terminal_read_origin(&headers)
    }

    #[test]
    fn terminal_accepts_only_the_exact_dashboard_authority() {
        assert!(allowed("node.local", Some("https://node.local")));
        assert!(allowed("192.168.1.2:7778", Some("http://192.168.1.2:7778")));
        assert!(!allowed("node.local", Some("https://node.local:7778")));
        assert!(!allowed("node.local:7778", Some("https://node.local")));
        assert!(!allowed("node.local", Some("https://evil.example")));
        assert!(!allowed("node.local", Some("null")));
        assert!(!allowed("node.local", None));
    }

    #[test]
    fn terminal_read_accepts_same_origin_fetch_but_not_same_site_apps() {
        assert!(read_allowed("node.local", None, Some("same-origin")));
        assert!(!read_allowed("node.local", None, Some("same-site")));
        assert!(!read_allowed("node.local", None, Some("cross-site")));
        assert!(!read_allowed("node.local", None, None));
        assert!(!read_allowed(
            "node.local",
            Some("https://evil.example"),
            Some("same-site")
        ));
        assert!(!read_allowed(
            "node.local",
            Some("https://evil.example"),
            Some("same-origin")
        ));
    }
}
