//! The app gate — authentication in front of every app port.
//!
//! # Why this exists
//!
//! Reproduced on a live node 2026-08-03: with no session cookie at all, over
//! the Tailscale address, six app ports answered `HTTP 200` with their real
//! UIs. `ss -tlnp` showed them bound `0.0.0.0`, so the same pages were served
//! on the LAN address, the FIPS mesh address, and through each app's onion.
//! This is the same bug class as the `/lnd-connect-info` and `/bitcoin-rpc/`
//! leaks closed in v1.7.120, but across every app rather than two endpoints.
//!
//! # Why one gate covers four transports
//!
//! LAN, Tailscale, Tor and the FIPS mesh all converge on
//! `127.0.0.1:<app_port>` — the container publishes there, the mesh relay
//! forwards there, and `HiddenServicePort` points there. Authorising at that
//! convergence point is one gate rather than four, which is the only reason
//! this is tractable at all.
//!
//! # Why not umbrel's sidecar proxy
//!
//! umbrelOS gives every app an `app_proxy` container that owns the published
//! port. That works, but it costs a container per app and a second service to
//! hold the shared secret. Here the daemon already terminates HTTP, already
//! owns the session store, and already runs a relay loop for the mesh, so the
//! gate is assembly rather than new infrastructure.
//!
//! # What it does NOT do
//!
//! It does not invent authentication policy. Password verification, TOTP
//! decryption and step replay protection, session lifetime, and rate limiting
//! are the same primitives the JSON-RPC login path uses. Only the transport
//! differs — an HTML form instead of JSON-RPC — because a browser being
//! redirected to an app cannot speak JSON-RPC.

pub mod identity;
pub mod listener;
pub mod tls;

use crate::auth::AuthManager;
use crate::rate_limit::LoginRateLimiter;
use crate::session::SessionStore;
use hyper::{header, Body, HeaderMap, Method, Request, Response, StatusCode};
use identity::{GatedPort, PortMap};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Paths the gate serves itself rather than proxying. Namespaced so an app
/// that happens to have its own `/login` is unaffected.
const GATE_PREFIX: &str = "/__archipelago-gate/";
#[derive(Clone, Copy)]
pub(crate) struct SecureTransport(pub bool);

/// Result of examining a request's credentials.
#[derive(Debug, PartialEq, Eq)]
pub enum Authorization {
    /// Proxy it through.
    Allow,
    /// Proxy it through, but the credential that allowed it was the gate's own
    /// `Authorization: Bearer <device token>` — strip that header before the
    /// app sees it, exactly as the session cookie is stripped.
    AllowGateToken,
    /// App-only cookie; never repair or issue a dashboard session for it.
    AllowGuest,
    /// Expiring external guest credential presented as an API bearer token.
    /// It still requires the current port's guest opt-in and is stripped.
    AllowGuestToken,
    /// Serve the login page.
    Challenge,
}

pub struct AppGate {
    sessions: SessionStore,
    auth: AuthManager,
    limiter: LoginRateLimiter,
    data_dir: PathBuf,
    port_map: Arc<RwLock<PortMap>>,
    /// TLS for gated ports. Shared by every accept loop so one reissue is
    /// picked up by all of them, and so the parse happens once rather than
    /// per port.
    pub(crate) tls: Arc<tls::GateTls>,
}

impl AppGate {
    pub fn new(
        sessions: SessionStore,
        auth: AuthManager,
        limiter: LoginRateLimiter,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            sessions,
            auth,
            limiter,
            data_dir,
            port_map: Arc::new(RwLock::new(identity::build_port_map())),
            tls: Arc::new(tls::GateTls::new()),
        }
    }

    /// Re-read the manifests. Called on catalog refresh so a newly installed
    /// app is gated without a daemon restart.
    pub async fn refresh(&self) {
        *self.port_map.write().await = identity::build_port_map();
    }

    pub async fn port_map(&self) -> PortMap {
        self.port_map.read().await.clone()
    }

    /// Does this request carry a credential good for `app_id`?
    ///
    /// Accepted credentials retain distinct scopes:
    ///
    /// * the node session cookie — and because a session still pending its
    ///   TOTP step fails `validate()`, **2FA is honoured here for free**. The
    ///   gate never sees a TOTP code on a proxied request and never needs to.
    /// * an app-scoped bearer token, for machine clients that speak HTTP but
    ///   cannot hold a cookie or complete an interactive login (Home
    ///   Assistant reaching an app's API is the motivating case).
    /// * a separately named app-only cookie, with live scope/expiry/revocation
    ///   checks and no ability to authenticate to dashboard RPC.
    pub async fn authorize(&self, headers: &HeaderMap, app_id: &str) -> Authorization {
        if let Some(token) = crate::session::extract_session_cookie(headers) {
            if self.sessions.validate(&token).await {
                return Authorization::Allow;
            }
        }

        let guest_enabled = self
            .port_map
            .read()
            .await
            .gated_ports()
            .any(|p| p.app_id == app_id && p.guest_access && p.auth_enabled);
        if let Some(token) = bearer_token(headers) {
            if let Some(credential) =
                crate::device_tokens::verified_app_token(&self.data_dir, &token, app_id).await
            {
                if !credential.name.starts_with("external:") || credential.apps.is_none() {
                    return Authorization::AllowGateToken;
                }
                if guest_enabled {
                    return Authorization::AllowGuestToken;
                }
            }
        }
        if guest_enabled {
            if let Some(token) = cookie_value(headers, &format!("archy_app_access_{app_id}")) {
                if crate::device_tokens::verify_guest(&self.data_dir, &token, app_id).await {
                    return Authorization::AllowGuest;
                }
            }
        }

        Authorization::Challenge
    }

    /// Handle one inbound request on a gated port.
    pub async fn handle(
        &self,
        req: Request<Body>,
        app: &GatedPort,
        client_ip: IpAddr,
    ) -> Response<Body> {
        // The accept loop captured its GatedPort at bind time; per-request
        // policy (the operator's gate on/off toggle, session_passthrough)
        // must come from the live map or a Settings change would only apply
        // to ports (re)bound after the next sweep. Falls back to the bound
        // snapshot when the port momentarily leaves the map mid-refresh.
        let live = self.port_map.read().await.gated(app.port).cloned();
        let app = live.as_ref().unwrap_or(app);
        if maintenance_active(&self.data_dir, &app.app_id) {
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .header(header::CACHE_CONTROL, "no-store")
                .header(header::RETRY_AFTER, "60")
                .body(Body::from(
                    "This app is temporarily unavailable while its update is recovered.",
                ))
                .unwrap();
        }
        // A managed public route must still refer to this guest-enabled app.
        // Refuse stale routes before login actions or public-resource exceptions.
        if let Some(expected) = req.headers().get("x-archipelago-app") {
            if expected.to_str().ok() != Some(app.app_id.as_str())
                || live.is_none()
                || !app.declared
                || !app.guest_access
                || !app.auth_enabled
                || app.session_passthrough
            {
                return Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("App route is no longer available"))
                    .unwrap();
            }
        }

        // Managed gateway routes are HTTPS-only. Mark cookies Secure even
        // though the final in-node FIPS hop uses HTTP. A forged header can only
        // strengthen this cookie attribute, never grant authorization.
        let mut req = req;
        if req.headers().contains_key("x-archipelago-app") {
            req.extensions_mut().insert(SecureTransport(true));
        }
        let path = req.uri().path().to_string();
        // A dashboard same-origin proxy strips `/app/<id>/` before this gate
        // sees the URI. Carry that trusted proxy mount into the challenge's
        // form/assets and its post-login redirect so the browser stays inside
        // the mounted app instead of posting to the dashboard root.
        let mount_prefix = forwarded_mount_prefix(req.headers());

        if let Some(action) = path.strip_prefix(GATE_PREFIX) {
            return self
                .handle_gate_action(req, app, action, client_ip, &mount_prefix)
                .await;
        }

        // A browser fetches a few subresources WITHOUT credentials by
        // specification, no matter how the user is authenticated: a PWA
        // manifest referenced by a plain `<link rel="manifest">` is the
        // canonical case. The cookie is never offered, so challenging these
        // returns 401 + a login page EVEN TO A FULLY AUTHENTICATED USER — and
        // the app's own service worker then serves a cached shell whose every
        // network call fails, which reads as "the app is broken" rather than
        // "the gate refused". IndeeHub worked all year and broke on the gate's
        // rollout for exactly this reason.
        //
        // These are passed through unauthenticated on purpose. It is a real
        // hole in the gate, so it is deliberately as small as the problem: an
        // exact-match allowlist of non-sensitive, static, well-known paths that
        // reveal nothing the gate's own login page does not already show (the
        // app's name and icon). No prefixes, no wildcards — a prefix here would
        // let `/manifest.json/../api/secrets` style paths ride through, and
        // anything user-specific must keep being challenged.
        if Self::is_credentialless_public_path(&path) {
            // Nothing on this list needs a credential, so any Authorization
            // riding along is unverified as far as the gate is concerned —
            // drop it rather than hand an unexamined token to the app.
            return proxy_to_app(req, app, true).await;
        }

        // Gate challenge disabled for this app (manifest `auth: open`, or
        // the operator's Settings toggle): forward everything to the app's
        // own authentication. The Authorization header passes through
        // untouched — git clients speak basic-auth to Gitea, API clients
        // carry the app's own tokens. Gate cookies are still stripped in
        // proxy_to_app (an ungated app must never see the node session),
        // and the frame fixes / app-down page still apply.
        if !app.auth_enabled {
            return proxy_to_app(req, app, false).await;
        }

        // Capture the platform session before the request is moved into the
        // upstream proxy. Older app-gate sessions (issued before the paired
        // CSRF-cookie fix) can then repair themselves on the very next app
        // response, before the app's provider creates its signer iframe.
        let session_for_csrf = crate::session::extract_session_cookie(req.headers());
        let needs_csrf_cookie = cookie_value(req.headers(), "csrf_token").is_none();

        match self.authorize(req.headers(), &app.app_id).await {
            // The credential was a cookie (or none was needed): the
            // Authorization header, if any, belongs to the app. Forward it.
            Authorization::Allow => {
                let mut response = proxy_to_app(req, app, false).await;
                if needs_csrf_cookie {
                    if let Some(token) = session_for_csrf {
                        set_csrf_cookie(&mut response, &token).await;
                    }
                }
                response
            }
            // The credential WAS the Authorization header, and it was ours.
            Authorization::AllowGateToken => proxy_to_app(req, app, true).await,
            Authorization::AllowGuest if app.guest_access && !app.session_passthrough => {
                proxy_to_app(req, app, false).await
            }
            Authorization::AllowGuestToken if app.guest_access && !app.session_passthrough => {
                proxy_to_app(req, app, true).await
            }
            // 401 rather than a redirect: a redirect to a login page is
            // indistinguishable from the app itself redirecting, and machine
            // clients would follow it and parse HTML as if it were their API
            // response. The status says "you are not authenticated" in a way
            // every client understands, and browsers still render the body.
            Authorization::Challenge
            | Authorization::AllowGuest
            | Authorization::AllowGuestToken => {
                login_page(app, None, StatusCode::UNAUTHORIZED, &mount_prefix)
            }
        }
    }

    /// Paths a browser fetches without credentials by specification.
    ///
    /// Exact matches only, and every entry must be static, non-user-specific,
    /// and no more revealing than the gate's own login page. Adding to this
    /// list widens an authentication bypass — justify each one here.
    ///
    /// - `/manifest.json`, `/site.webmanifest`: `<link rel="manifest">` is
    ///   fetched in no-credentials mode unless the tag opts in with
    ///   `crossorigin="use-credentials"`, which app authors cannot be required
    ///   to do. Contains the app's name, colours and icon paths.
    /// - `/favicon.ico`: same no-credentials treatment, and the gate's login
    ///   page already displays the app's icon.
    fn is_credentialless_public_path(path: &str) -> bool {
        matches!(
            path,
            "/manifest.json" | "/site.webmanifest" | "/favicon.ico"
        )
    }

    /// The gate's own endpoints: the login form target and the TOTP step.
    async fn handle_gate_action(
        &self,
        req: Request<Body>,
        app: &GatedPort,
        action: &str,
        client_ip: IpAddr,
        mount_prefix: &str,
    ) -> Response<Body> {
        // Assets are GET and pre-auth by nature: the login page cannot
        // render its own background or logo without them.
        if let Some(name) = action.strip_prefix("asset/") {
            return self.serve_asset(name);
        }
        if req.method() != Method::POST {
            return login_page(app, None, StatusCode::OK, mount_prefix);
        }

        // Captured before the body is consumed. The pending-2FA session
        // rides the cookie rather than a hidden form field so the token
        // never appears in the HTML, in a `view-source`, or in a screenshot
        // of the second-factor page.
        let pending = crate::session::extract_session_cookie(req.headers());
        let secure = req
            .extensions()
            .get::<SecureTransport>()
            .map(|s| s.0)
            .unwrap_or(false)
            || req
                .headers()
                .get("x-forwarded-proto")
                .and_then(|v| v.to_str().ok())
                == Some("https");

        // Same limiter instance as the JSON-RPC login path, so an attacker
        // cannot get a fresh budget of guesses simply by moving to an app
        // port.
        if !self.limiter.check(client_ip).await {
            return login_page(
                app,
                Some("Too many attempts. Wait a minute and try again."),
                StatusCode::TOO_MANY_REQUESTS,
                mount_prefix,
            );
        }

        let form = match read_form(req).await {
            Some(form) => form,
            None => {
                return login_page(
                    app,
                    Some("Malformed request."),
                    StatusCode::BAD_REQUEST,
                    mount_prefix,
                )
            }
        };

        match action {
            "guest" if app.guest_access && app.auth_enabled => {
                let token = field(&form, "access_token").unwrap_or_default();
                if !crate::device_tokens::verify_guest(&self.data_dir, &token, &app.app_id).await {
                    self.limiter.record_failure(client_ip).await;
                    return login_page(
                        app,
                        Some("App access token is invalid, expired or revoked."),
                        StatusCode::UNAUTHORIZED,
                        mount_prefix,
                    );
                }
                let mut response = redirect_to_app(mount_prefix);
                // Host-only and app-specific. A token is rechecked on EVERY
                // request, so revocation and its expiry apply immediately.
                let suffix = if secure { "; Secure" } else { "" };
                if let Ok(cookie) = header::HeaderValue::from_str(&format!("archy_app_access_{}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age=3600{suffix}", app.app_id)) {
                    response.headers_mut().append(header::SET_COOKIE, cookie);
                }
                response
            }
            "login" => self.do_login(app, &form, client_ip, mount_prefix).await,
            "totp" => {
                self.do_totp(app, &form, pending, client_ip, mount_prefix)
                    .await
            }
            _ => not_found(),
        }
    }

    /// Static assets the login page needs, served from the gate's own origin.
    ///
    /// The backgrounds are ~1 MB each, so inlining them as data URIs would
    /// bloat every challenge response. Serving them here keeps the page
    /// byte-identical to the dashboard's login while the CSP stays tight:
    /// `img-src 'self' data:` and nothing else.
    fn serve_asset(&self, name: &str) -> Response<Body> {
        let Some((bytes, mime)) = read_ui_asset(name) else {
            return not_found();
        };
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            // Immutable art; caching it costs nothing and keeps the login
            // instant on a repeat challenge.
            .header(header::CACHE_CONTROL, "public, max-age=86400")
            .body(Body::from(bytes))
            .expect("asset response builds")
    }

    async fn do_login(
        &self,
        app: &GatedPort,
        form: &Form,
        client_ip: IpAddr,
        mount_prefix: &str,
    ) -> Response<Body> {
        let password = field(form, "password").unwrap_or_default();

        match self.auth.verify_password(&password).await {
            Ok(true) => {}
            _ => {
                self.limiter.record_failure(client_ip).await;
                return login_page(
                    app,
                    Some("Incorrect password."),
                    StatusCode::UNAUTHORIZED,
                    mount_prefix,
                );
            }
        }

        // 2FA, if configured. The secret is encrypted with the password, so
        // this is the only moment it can be decrypted — exactly as in the
        // JSON-RPC path. A pending session cannot pass `authorize`, so a
        // half-finished login grants nothing.
        if self.auth.is_totp_enabled().await.unwrap_or(false) {
            if let Ok(Some(totp_data)) = self.auth.get_totp_data().await {
                if let Ok(secret) = crate::totp::decrypt_secret(&totp_data, &password) {
                    let pending = self.sessions.create_pending(secret).await;
                    let mut resp = totp_page(app, None, StatusCode::OK, mount_prefix);
                    set_session_cookie(&mut resp, &pending).await;
                    return resp;
                }
            }
            // TOTP is on but its data is unreadable. Refuse: falling through
            // to a full session would silently downgrade the node's second
            // factor to nothing.
            return login_page(
                app,
                Some("Two-factor data could not be read. Sign in from the dashboard."),
                StatusCode::INTERNAL_SERVER_ERROR,
                mount_prefix,
            );
        }

        let token = self.sessions.create().await;
        let mut resp = redirect_to_app(mount_prefix);
        set_session_cookie(&mut resp, &token).await;
        resp
    }

    async fn do_totp(
        &self,
        app: &GatedPort,
        form: &Form,
        pending: Option<String>,
        client_ip: IpAddr,
        mount_prefix: &str,
    ) -> Response<Body> {
        let code = field(form, "code").unwrap_or_default();
        let Some(pending) = pending.filter(|s| !s.is_empty()) else {
            return login_page(
                app,
                Some("Session expired."),
                StatusCode::UNAUTHORIZED,
                mount_prefix,
            );
        };

        let Some(secret) = self.sessions.get_pending_secret(&pending).await else {
            return login_page(
                app,
                Some("Session expired. Start again."),
                StatusCode::UNAUTHORIZED,
                mount_prefix,
            );
        };

        let totp_data = self.auth.get_totp_data().await.ok().flatten();
        let used_steps = totp_data
            .as_ref()
            .map(|d| d.used_steps.clone())
            .unwrap_or_default();

        match crate::totp::verify_code(&secret, &code, &used_steps) {
            Ok(Some(step)) => {
                // Record the step so the same code cannot be replayed — the
                // JSON-RPC path does this and skipping it here would make the
                // gate the weaker of the two doors.
                if let Some(mut data) = totp_data {
                    data.used_steps.push(step);
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let cutoff = (now / 30) - 10;
                    data.used_steps.retain(|s| *s > cutoff);
                    let _ = self.auth.update_totp(data).await;
                }
                match self.sessions.upgrade_to_full(&pending).await {
                    Some(full) => {
                        let mut resp = redirect_to_app(mount_prefix);
                        set_session_cookie(&mut resp, &full).await;
                        resp
                    }
                    None => login_page(
                        app,
                        Some("Session expired."),
                        StatusCode::UNAUTHORIZED,
                        mount_prefix,
                    ),
                }
            }
            _ => {
                self.limiter.record_failure(client_ip).await;
                let mut resp = totp_page(
                    app,
                    Some("Incorrect code."),
                    StatusCode::UNAUTHORIZED,
                    mount_prefix,
                );
                set_session_cookie(&mut resp, &pending).await;
                resp
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Request helpers
// ---------------------------------------------------------------------------

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))?;
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

type Form = std::collections::HashMap<String, String>;

/// Free function rather than a trait method: `HashMap` has an inherent `get`
/// that would win method resolution and silently return `Option<&String>`.
fn field(form: &Form, key: &str) -> Option<String> {
    form.get(key).cloned()
}

/// Read an `application/x-www-form-urlencoded` body.
///
/// Capped: an unauthenticated caller must not be able to make the daemon
/// buffer arbitrary bytes, and no legitimate login form approaches this.
const MAX_FORM_BYTES: usize = 8 * 1024;

async fn read_form(req: Request<Body>) -> Option<Form> {
    let bytes = hyper::body::to_bytes(req.into_body()).await.ok()?;
    if bytes.len() > MAX_FORM_BYTES {
        return None;
    }
    let text = std::str::from_utf8(&bytes).ok()?;
    let mut form = Form::new();
    for pair in text.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        form.insert(percent_decode(k), percent_decode(v));
    }
    Some(form)
}

fn percent_decode(input: &str) -> String {
    let bytes = input.replace('+', " ").into_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Forward an authorised request to the app on loopback.
async fn proxy_to_app(
    req: Request<Body>,
    app: &GatedPort,
    drop_authorization: bool,
) -> Response<Body> {
    let port = app.port;
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/")
        .to_string();
    let uri = match format!("http://127.0.0.1:{port}{path_and_query}").parse::<hyper::Uri>() {
        Ok(uri) => uri,
        Err(_) => return app_down_page(app),
    };

    let (mut parts, body) = req.into_parts();
    parts.uri = uri;
    strip_matching_cookies(&mut parts.headers, |name| {
        name.starts_with("archy_app_access_")
    });
    // Strip the gate's own credential before it reaches the app — the app
    // should never be in a position to log, echo, or forward the node
    // session. But ONLY the gate's cookies: apps run their own cookie logins
    // (vaultwarden, nextcloud, gitea…), and removing the whole header logged
    // every one of them out on each request. Companion UIs that proxy the
    // daemon's authenticated endpoints opt in to keeping the session via
    // `session_passthrough: true` on their gated port.
    if !app.session_passthrough {
        strip_gate_cookies(&mut parts.headers);
    }
    // Same principle, applied to the other credential the gate accepts, and
    // with the same precision. The gate's own header credential is exactly
    // one thing: `Authorization: Bearer <app-scoped device token>`, and the
    // caller has already told us whether THIS request was authorised by it.
    //
    // Removing the header unconditionally — as this did — deletes credentials
    // the gate never issued and never inspects. IndeeHub sends its NIP-98
    // `Authorization: Nostr <signed event>` to its own `/api/auth/nostr/session`;
    // the header arrived stripped, so its backend answered "Authorization
    // header is missing" and a Nostr login could not complete by ANY route:
    // a NIP-07 extension in a tab, the parent frame's NIP-07 bridge
    // (`nostr-provider.js`), or AIUI. The signing was always fine — the proof
    // of it was thrown away one hop before the app. The same blanket removal
    // breaks every app doing Basic auth or presenting a bearer token it
    // issued itself.
    if drop_authorization {
        parts.headers.remove(header::AUTHORIZATION);
    }

    // Websocket upgrades need splicing, not request forwarding. hyper::Client
    // alone completes the app's 101 handshake and then DROPS the upgraded
    // connection, so every ws-driven app (mempool's entire UI is one) loaded
    // fine through the gate and then died with close code 1006 on connect —
    // reported from a browser console, 2026-08-09, after two other layers of
    // the same symptom had already been fixed. The gate's server side already
    // accepts client upgrades (.with_upgrades() in listener.rs); this is the
    // missing upstream half: hand the handshake to the app and, on 101, bridge
    // the two upgraded connections byte-for-byte. Header stripping above still
    // applies — the app sees the same sanitized headers on a ws handshake as
    // on any other request.
    if parts.headers.contains_key(header::UPGRADE) {
        // The client side's OnUpgrade handle rides in the request extensions;
        // take it before the parts become the upstream request. It resolves
        // once serve_connection's with_upgrades hands us the raw socket after
        // we return the 101.
        let client_upgrade = parts.extensions.remove::<hyper::upgrade::OnUpgrade>();
        let upstream_req = Request::from_parts(parts, Body::empty());
        let client = hyper::Client::new();
        let mut upstream_resp = match client.request(upstream_req).await {
            Ok(resp) => resp,
            Err(_) => return app_down_page(app),
        };
        if upstream_resp.status() == StatusCode::SWITCHING_PROTOCOLS {
            if let Some(client_upgrade) = client_upgrade {
                let upstream_upgrade = hyper::upgrade::on(&mut upstream_resp);
                tokio::spawn(async move {
                    if let (Ok(mut client_io), Ok(mut app_io)) =
                        (client_upgrade.await, upstream_upgrade.await)
                    {
                        let _ = tokio::io::copy_bidirectional(&mut client_io, &mut app_io).await;
                    }
                });
            }
        }
        return upstream_resp;
    }

    let client = hyper::Client::new();
    match client.request(Request::from_parts(parts, body)).await {
        Ok(mut resp) => {
            neutralize_frame_blocking(resp.headers_mut());
            resp
        }
        Err(_) => app_down_page(app),
    }
}

/// Make gate-proxied app responses embeddable by the dashboard's My Apps
/// iframe. Apps that were never designed for framing ship
/// `X-Frame-Options: DENY` (Alby Hub) or a CSP `frame-ancestors` directive,
/// and either one makes the embedded app session a dead grey pane — the
/// historical workaround was a bespoke per-app nginx proxy (gitea), which is
/// exactly the per-app patching the manifest platform exists to delete.
///
/// Framing protection exists to stop a FOREIGN origin from framing an authed
/// page and clickjacking it. Behind the gate that threat model is already
/// handled the way the gate's own pages handle it: every proxied request is
/// authenticated by the gate first, and the gate's own responses declare
/// `frame-ancestors 'self' http://*:* https://*:*` (see `page()`) because the
/// dashboard is reached by LAN IP, mDNS name, and onion alike. Upstream
/// X-Frame-Options is dropped entirely; only the `frame-ancestors` directive
/// is removed from the app's CSP — the rest of the app's policy (script-src,
/// connect-src, …) is the app's business and passes through untouched.
fn neutralize_frame_blocking(headers: &mut hyper::HeaderMap) {
    headers.remove("x-frame-options");
    let Some(csp) = headers.get("content-security-policy") else {
        return;
    };
    let Ok(raw) = csp.to_str() else {
        return;
    };
    if !raw.to_ascii_lowercase().contains("frame-ancestors") {
        return;
    }
    let kept: Vec<&str> = raw
        .split(';')
        .map(str::trim)
        .filter(|d| !d.to_ascii_lowercase().starts_with("frame-ancestors") && !d.is_empty())
        .collect();
    if kept.is_empty() {
        headers.remove("content-security-policy");
        return;
    }
    match header::HeaderValue::from_str(&kept.join("; ")) {
        Ok(v) => {
            headers.insert("content-security-policy", v);
        }
        Err(_) => {
            // Unrepresentable after filtering — fail open for framing but
            // closed for the policy: better to drop a mangled CSP than to
            // serve one we rewrote incorrectly.
            headers.remove("content-security-policy");
        }
    }
}

/// Cookie names owned by the gate/daemon, never the app's to see.
const GATE_COOKIE_NAMES: &[&str] = &["session", "csrf_token", "remember"];

/// Remove the gate's own cookie pairs from the Cookie header, preserving the
/// app's cookies (its login/session/prefs) untouched. Drops the header
/// entirely when nothing remains.
fn strip_gate_cookies(headers: &mut hyper::HeaderMap) {
    strip_matching_cookies(headers, |name| {
        GATE_COOKIE_NAMES.contains(&name) || name.starts_with("archy_app_access_")
    });
}

fn strip_matching_cookies(headers: &mut hyper::HeaderMap, remove: fn(&str) -> bool) {
    let Some(cookie) = headers.get(header::COOKIE) else {
        return;
    };
    let Ok(raw) = cookie.to_str() else {
        // Not valid UTF-8 — can't safely filter pairs, so fail closed.
        headers.remove(header::COOKIE);
        return;
    };
    let kept: Vec<&str> = raw
        .split(';')
        .map(str::trim)
        .filter(|pair| {
            let name = pair.split('=').next().unwrap_or("").trim();
            !remove(name)
        })
        .filter(|pair| !pair.is_empty())
        .collect();
    if kept.is_empty() {
        headers.remove(header::COOKIE);
        return;
    }
    match header::HeaderValue::from_str(&kept.join("; ")) {
        Ok(v) => {
            headers.insert(header::COOKIE, v);
        }
        Err(_) => {
            headers.remove(header::COOKIE);
        }
    }
}

async fn set_session_cookie(resp: &mut Response<Body>, token: &str) {
    // The dashboard RPC layer requires a readable CSRF cookie as well as the
    // HttpOnly session cookie. An app-gate login is a complete node login, so
    // it must establish the same pair as auth.login; otherwise a fresh browser
    // can open the signer broker but every identity/signing RPC is rejected
    // with `has_session=true, has_header=false`.
    if !set_csrf_cookie(resp, token).await {
        return;
    }
    // No Domain attribute, so the cookie is host-only. Cookies ignore port,
    // which is what makes one sign-in cover the dashboard and every app port
    // on the same host — and equally why an app on a *different* host (its
    // own onion) is a separate sign-in.
    if let Ok(value) =
        header::HeaderValue::from_str(&format!("session={token}; HttpOnly; SameSite=Lax; Path=/"))
    {
        resp.headers_mut().append(header::SET_COOKIE, value);
    }
}

async fn set_csrf_cookie(resp: &mut Response<Body>, token: &str) -> bool {
    let csrf = match crate::api::rpc::derive_csrf_token(token).await {
        Ok(csrf) => csrf,
        Err(error) => {
            tracing::error!(%error, "App login could not load persistent session signing key");
            *resp = Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .header(header::CACHE_CONTROL, "no-store")
                .body(Body::from(
                    "Sign-in temporarily unavailable. Check server session storage.",
                ))
                .expect("static error response");
            return false;
        }
    };
    if let Ok(value) =
        header::HeaderValue::from_str(&format!("csrf_token={csrf}; SameSite=Lax; Path=/"))
    {
        resp.headers_mut().append(header::SET_COOKIE, value);
    }
    true
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .map(str::trim)
        .find_map(|pair| pair.strip_prefix(&prefix))
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Validate the mount supplied by the node's own nginx proxy.
///
/// Treat this as untrusted input even though our canonical proxy sets it: a
/// client can reach an app-gate port directly and forge request headers. Only
/// a short absolute path made from ordinary URL-path characters is accepted;
/// protocol-relative URLs, dot segments, escaping and query/fragment syntax
/// all fall back to the direct-port root.
fn forwarded_mount_prefix(headers: &HeaderMap) -> String {
    let Some(raw) = headers
        .get("x-forwarded-prefix")
        .and_then(|value| value.to_str().ok())
    else {
        return String::new();
    };
    let value = raw.trim_end_matches('/');
    if value.is_empty()
        || value.len() > 256
        || !value.starts_with('/')
        || value.starts_with("//")
        || value
            .bytes()
            .any(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.')))
        || value
            .split('/')
            .skip(1)
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return String::new();
    }
    value.to_owned()
}

fn gate_url(mount_prefix: &str, action: &str) -> String {
    format!("{mount_prefix}{GATE_PREFIX}{action}")
}

fn redirect_to_app(mount_prefix: &str) -> Response<Body> {
    let location = if mount_prefix.is_empty() {
        "/".to_owned()
    } else {
        format!("{mount_prefix}/")
    };
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, location)
        .body(Body::empty())
        .expect("static response builds")
}

/// Served when the app behind the gate does not answer on loopback.
///
/// A real page rather than the bare string `app is not responding`: the gate
/// answers on the app's own port, so this text IS the app as far as the
/// operator can tell, and the raw string read as the node itself being broken
/// (reported against Gitea on a fleet node, 2026-08-10 — the actual fault was
/// a ghost container crash-looping the app). Name the app, say the node is
/// fine, and retry on our own: an app that is restarting comes back without
/// the user knowing to reload. Status stays 502 so machine clients still see
/// an upstream failure rather than a success with HTML in it.
fn app_down_page(app: &GatedPort) -> Response<Body> {
    let body = format!(
        r#"{icon}
<h1>{name} is not responding</h1>
<p class="sub">The app is not answering right now — it may be stopped or still
starting. This page retries automatically. If it does not recover, open the
dashboard and check {name} under My Apps.</p>"#,
        icon = icon_markup(app),
        name = esc(&app.app_name),
    );
    let mut resp = page(
        "App not responding",
        app,
        &body,
        StatusCode::BAD_GATEWAY,
        "",
    );
    // Header-based refresh, not <meta> or script: page()'s CSP allows no
    // script, and the header keeps the retry out of the document entirely.
    resp.headers_mut()
        .insert("Refresh", header::HeaderValue::from_static("5"));
    resp
}

fn maintenance_active(data_dir: &std::path::Path, app_id: &str) -> bool {
    if app_id != "indeedhub" && !app_id.starts_with("indeedhub-") {
        return false;
    }
    match std::fs::symlink_metadata(data_dir.join("app-maintenance/indeedhub")) {
        Ok(_) => true,
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    }
}

fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .expect("static response builds")
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

/// Minimal HTML escape for values interpolated into the pages below.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// The app's icon as an `<img>`, or a lettermark when the manifest declares
/// none. Inlined as a data URI rather than linked: the gate is answering on
/// the app's own port, so any asset URL would either hit the unauthenticated
/// app behind it or a different origin the browser may not reach.
/// One stacked layer per background, each delayed so they cross-fade in turn.
fn background_layers(mount_prefix: &str) -> String {
    let step = LOGIN_BACKGROUNDS.len() as u32 * 9 / LOGIN_BACKGROUNDS.len() as u32;
    LOGIN_BACKGROUNDS
        .iter()
        .enumerate()
        .map(|(i, name)| {
            format!(
                r#"<div class="bg" style="background-image:url('{prefix}asset/{name}');animation-delay:{delay}s"></div>"#,
                prefix = gate_url(mount_prefix, ""),
                delay = i as u32 * step,
            )
        })
        .collect()
}

fn icon_markup(app: &GatedPort) -> String {
    let inner = app
        .icon
        .as_deref()
        .and_then(read_icon_data_uri)
        // A manifest that names no icon still gets one: the dashboard already
        // ships icons named after the app, so fall back to those before
        // giving up. Without this EVERY gated app showed a lettermark,
        // because no manifest declares metadata.icon (test node,
        // 2026-08-05).
        .or_else(|| {
            icon_candidates(&app.app_id)
                .iter()
                .find_map(|c| read_icon_data_uri(c))
        })
        .map(|data_uri| format!(r#"<img class="icon" src="{}" alt="">"#, esc(&data_uri)))
        .unwrap_or_else(|| {
            let letter = app
                .app_name
                .chars()
                .find(|c| c.is_alphanumeric())
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| "?".to_string());
            format!(r#"<div class="icon lettermark">{}</div>"#, esc(&letter))
        });
    format!(r#"<div class="tile">{inner}</div>"#)
}

/// The dashboard login's badge, reproduced square-for-square: the same 20
/// white rects AnimatedLogo.vue draws, with the same 100ms stagger, inside
/// the same gradient ring. Inline rather than an `<img>` because the shipped
/// `favico-black-v2.svg` bakes its own ring into the artwork — wrapping it in
/// the CSS ring drew a ring inside a ring, which is not what /login shows.
/// (x, y, width, height) as they appear in AnimatedLogo.vue.
const LOGO_RECTS: [(f32, f32, f32, f32); 20] = [
    (357.614, 318.0, 71.007, 70.936),
    (436.152, 318.0, 72.082, 70.936),
    (515.766, 318.0, 72.082, 70.936),
    (595.379, 318.0, 71.007, 70.936),
    (595.379, 396.46, 71.007, 72.011),
    (673.917, 396.46, 72.083, 72.011),
    (278.0, 475.994, 72.083, 72.012),
    (357.614, 475.994, 71.007, 72.012),
    (436.152, 475.994, 72.082, 72.012),
    (515.766, 475.994, 72.082, 72.012),
    (595.379, 475.994, 71.007, 72.012),
    (673.917, 475.994, 72.083, 72.012),
    (278.0, 555.529, 72.083, 70.936),
    (357.614, 555.529, 71.007, 70.936),
    (595.379, 555.529, 71.007, 70.936),
    (673.917, 555.529, 72.083, 70.936),
    (357.614, 633.989, 71.007, 72.011),
    (436.152, 633.989, 72.082, 72.011),
    (515.766, 633.989, 72.082, 72.011),
    (595.379, 633.989, 71.007, 72.011),
];

fn logo_markup() -> String {
    let rects: String = LOGO_RECTS
        .iter()
        .enumerate()
        .map(|(i, (x, y, w, h))| {
            format!(
                r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="white" class="sq" style="--d:{delay}ms"/>"#,
                delay = i * 100,
            )
        })
        .collect();
    format!(
        r##"<div class="logo"><svg viewBox="0 0 1024 1024" role="img" aria-label="Archipelago" xmlns="http://www.w3.org/2000/svg"><rect width="1024" height="1024" fill="#030202"/>{rects}</svg></div>"##
    )
}

/// The main login's in-button spinner, verbatim from Login.vue.
const SPINNER_SVG: &str = r#"<svg class="spin" viewBox="0 0 24 24" fill="none" aria-hidden="true"><circle style="opacity:.25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path style="opacity:.75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg>"#;

/// Submit feedback: flip the pressed button into its loading face and stop a
/// second press, exactly as /login does. This is the only script on the page,
/// and the CSP admits it by hash — not `'unsafe-inline'` — so an injected
/// `<script>` still cannot run. Everything works without it (the form is a
/// plain POST); losing JS costs only the spinner.
const SUBMIT_FEEDBACK_JS: &str = "document.addEventListener('submit',function(e){var b=e.target.querySelector('button');if(b){b.classList.add('loading');b.disabled=true;}});";

/// `'sha256-…'` CSP source expression for [`SUBMIT_FEEDBACK_JS`]. Computed
/// from the constant itself so the two can never drift apart.
fn submit_feedback_csp_hash() -> String {
    use sha2::{Digest, Sha256};
    format!(
        "'sha256-{}'",
        base64_encode(&Sha256::digest(SUBMIT_FEEDBACK_JS.as_bytes()))
    )
}

/// Icons live with the web UI. Only files under the icon directory are read,
/// and only known image extensions — the path comes from a manifest, which is
/// signed, but treating it as untrusted costs nothing.
/// Icon basenames to try for an app id, best first.
///
/// The shipped icon set is named for the *product*, while app ids carry
/// packaging detail — `filebrowser` vs `file-browser`, `morphos-server` vs
/// `morphos` — and the per-app screens (`lnd-ui`, `bitcoin-ui`, `electrs-ui`)
/// have no icon of their own but obviously belong to the app they front.
/// Resolving those here keeps the mapping in one readable place instead of
/// adding a `metadata.icon` line to every manifest, which would have to be
/// re-signed into the catalog to take effect.
fn icon_candidates(app_id: &str) -> Vec<String> {
    let mut out = vec![app_id.to_string()];
    let alias = match app_id {
        "filebrowser" => Some("file-browser"),
        "home-assistant" => Some("homeassistant"),
        "morphos-server" => Some("morphos"),
        "barkd" => Some("bark"),
        "archy-mempool-web" | "mempool-api" => Some("mempool"),
        "lnd-ui" | "lightning-stack" => Some("lnd"),
        "bitcoin-ui" => Some("bitcoin-core"),
        "electrs-ui" => Some("electrumx"),
        "fips-ui" | "aiui" | "did-wallet" => Some("archipelago-a"),
        "fedimint-gateway" | "fedimint-clientd" => Some("fedimint"),
        _ => None,
    };
    out.extend(alias.map(str::to_string));
    // `<app>-ui` / `-server` / `-web` front an app whose icon is the bare name.
    for suffix in ["-ui", "-server", "-web"] {
        if let Some(base) = app_id.strip_suffix(suffix) {
            out.push(base.to_string());
        }
    }
    out
}

/// Backgrounds the login cycles through, matching the dashboard's own
/// `/login` art. Cross-faded by CSS alone — the CSP forbids script, and a
/// rotation that needs JavaScript would not survive it.
const LOGIN_BACKGROUNDS: [&str; 4] = [
    "bg-intro.jpg",
    "bg-intro-4.webp",
    "bg-intro-6.webp",
    "bg-intro-3.jpg",
];

/// Assets the gate will serve, by exact name. An allowlist rather than a path
/// join: the name arrives in a URL, and the gate answers before any
/// authentication, so nothing here may be caller-controlled beyond this set.
fn read_ui_asset(name: &str) -> Option<(Vec<u8>, &'static str)> {
    // Only the rotating backgrounds: the logo badge is inline SVG now, so no
    // image asset backs it.
    let allowed = LOGIN_BACKGROUNDS.contains(&name);
    if !allowed {
        return None;
    }
    let mime = icon_mime(name.rsplit_once('.')?.1)?;
    for root in [
        "/opt/archipelago/web-ui/assets/img",
        "web/dist/neode-ui/assets/img",
        "neode-ui/public/assets/img",
        "/opt/archipelago/web-ui/assets/icon",
        "web/dist/neode-ui/assets/icon",
        "neode-ui/public/assets/icon",
    ] {
        if let Ok(bytes) = std::fs::read(std::path::Path::new(root).join(name)) {
            return Some((bytes, mime));
        }
    }
    None
}

const ICON_ROOTS: [&str; 2] = [
    "/opt/archipelago/web-ui/assets/img/app-icons",
    "web/dist/neode-ui/assets/img/app-icons",
];

fn icon_mime(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        _ => None,
    }
}

/// Read an app icon as a `data:` URI.
///
/// `icon_ref` may be a filename or path with an extension (a manifest's
/// `metadata.icon`), or a bare name such as an app id — in which case the
/// known extensions are tried in turn. Only the file name is used; the
/// directories searched are fixed, so a manifest cannot point the gate at an
/// arbitrary path.
fn read_icon_data_uri(icon_ref: &str) -> Option<String> {
    let name = std::path::Path::new(icon_ref).file_name()?.to_str()?;
    let candidates: Vec<(String, &str)> = match name.rsplit_once('.') {
        Some((_, ext)) => vec![(name.to_string(), icon_mime(ext)?)],
        None => ["svg", "png", "webp", "jpg"]
            .iter()
            .filter_map(|ext| Some((format!("{name}.{ext}"), icon_mime(ext)?)))
            .collect(),
    };
    for (file, mime) in candidates {
        for root in ICON_ROOTS {
            let candidate = std::path::Path::new(root).join(&file);
            if let Ok(bytes) = std::fs::read(&candidate) {
                if bytes.len() > 512 * 1024 {
                    continue;
                }
                return Some(format!("data:{mime};base64,{}", base64_encode(&bytes)));
            }
        }
    }
    None
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn page(
    title: &str,
    app: &GatedPort,
    body: &str,
    status: StatusCode,
    mount_prefix: &str,
) -> Response<Body> {
    let html = format!(
        r#"<!doctype html>
<html lang="en"><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>{title} — {app_name}</title>
<style>
/* The dashboard's own /login, rebuilt in static CSS: the same rotating
   intro art, .glass-card panel, .glass-button action and transparent
   white-bordered inputs from neode-ui/src/style.css. Written longhand
   rather than shared with the SPA because the gate answers before any
   bundle exists, and the CSP forbids external stylesheets and script. */
:root {{ color-scheme: dark; }}
* {{ box-sizing: border-box; }}
html {{ height:100%; }}
body {{ margin:0; color:#fff; background:#05070a; overflow:hidden;
  font:16px/1.5 system-ui,-apple-system,"Segoe UI",sans-serif;
  /* Fixed to the viewport rather than a tall scrolling page: an on-screen
     keyboard then overlays the card instead of scrolling it away, and the
     card stays optically centred. min-height:100vh scrolled with the
     keyboard on mobile and left the card off-centre (reported 2026-08-05). */
  position:fixed; inset:0;
  display:grid; place-items:center; padding:1rem;
  height:100vh; height:100svh; }}
/* Very short viewports (landscape phone, or a keyboard eating most of it):
   allow the card to scroll INSIDE the fixed frame rather than overflow. */
@media (max-height:640px) {{
  body {{ align-items:start; overflow-y:auto; padding-top:3rem; }}
}}
/* Rotating backgrounds: each layer holds its image and cross-fades on a
   shared cycle, so the art moves the way /login does with no script. */
.bg {{ position:fixed; inset:0; z-index:0; background-size:cover;
  background-position:center; opacity:0; animation:bg-cycle {cycle}s infinite; }}
.bg::after {{ content:''; position:absolute; inset:0;
  background:linear-gradient(180deg, rgba(0,0,0,.35), rgba(0,0,0,.72)); }}
@keyframes bg-cycle {{
  0% {{ opacity:0; }} 4% {{ opacity:1; }}
  {hold}% {{ opacity:1; }} {fade}% {{ opacity:0; }} 100% {{ opacity:0; }}
}}
main {{ position:relative; z-index:1; width:min(92vw,28rem); }}
.card {{ padding:2rem; padding-top:3.5rem; position:relative;
  background:rgba(0,0,0,.65); backdrop-filter:blur(18px);
  -webkit-backdrop-filter:blur(18px); border:1px solid rgba(255,255,255,.18);
  border-radius:1rem; box-shadow:0 8px 24px rgba(0,0,0,.45); text-align:center; }}
/* The Archipelago mark, half in and half out of the panel — same placement
   and gradient ring as Login.vue. */
.logo {{ position:absolute; top:-2.5rem; left:50%; transform:translateX(-50%);
  width:5rem; height:5rem; border-radius:9999px; padding:3px;
  background:linear-gradient(135deg, rgba(255,255,255,.6) 0%, rgba(0,0,0,.8) 100%);
  box-shadow:0 8px 24px rgba(0,0,0,.5); }}
.logo::after {{ content:''; position:absolute; inset:3px; border-radius:9999px;
  background:#000; z-index:0; }}
.logo svg {{ position:relative; z-index:1; width:100%; height:100%;
  border-radius:9999px; display:block; }}
/* AnimatedLogo.vue's reveal, timing intact: each square fades in on its own
   100ms-step delay over a 3s loop. */
.logo .sq {{ opacity:0; animation:logo-square-in 3s ease-out infinite;
  animation-delay:var(--d,0ms); animation-fill-mode:both; }}
@keyframes logo-square-in {{ 0% {{ opacity:0; }} 15% {{ opacity:1; }} 100% {{ opacity:1; }} }}
/* The app's own tile, in the My Apps shape: 18px-rounded square on dark
   glass with the same inner highlight and drop shadow. */
.tile {{ width:60px; height:60px; border-radius:18px; margin:0 auto .75rem;
  background:rgba(0,0,0,.72); box-shadow:0 8px 18px rgba(0,0,0,.38); }}
.tile .icon {{ width:100%; height:100%; border-radius:18px; display:block;
  object-fit:cover; border:1px solid rgba(255,255,255,.18);
  background:radial-gradient(circle at 35% 28%, rgba(255,255,255,.1), rgba(255,255,255,0) 42%),
    linear-gradient(145deg, rgba(22,22,24,.96), rgba(0,0,0,.96));
  box-shadow:inset 0 1px 0 rgba(255,255,255,.12), inset 0 -10px 24px rgba(0,0,0,.34); }}
.lettermark {{ display:grid; place-items:center; font-size:1.6rem; font-weight:600;
  color:rgba(255,255,255,.9); }}
h1 {{ font-size:1.5rem; font-weight:600; margin:0 0 .4rem;
  color:rgba(255,255,255,.96); text-shadow:0 2px 6px rgba(0,0,0,.4); }}
p.sub {{ margin:0 0 1.75rem; color:rgba(255,255,255,.6); font-size:.875rem; }}
input {{ width:100%; padding:.75rem 1rem; margin-bottom:1rem; border-radius:.5rem;
  border:1px solid rgba(255,255,255,.2); background:transparent; color:#fff;
  font-size:1rem; transition:border-color .2s ease; }}
input::placeholder {{ color:rgba(255,255,255,.4); }}
input:focus {{ outline:none; border-color:rgba(255,255,255,.4);
  box-shadow:0 0 0 1px rgba(255,255,255,.2); }}
/* .glass-button, longhand — the lift, the lightening and the rim glow on
   hover are what make the dashboard's buttons feel alive; the old flat
   darken-only hover here read as broken next to /login. */
button {{ position:relative; display:inline-flex; align-items:center;
  justify-content:center; width:100%; min-height:44px; padding:.75rem 1.25rem;
  border:none; border-radius:.75rem; background:rgba(0,0,0,.6);
  backdrop-filter:blur(24px); -webkit-backdrop-filter:blur(24px);
  box-shadow:0 8px 24px rgba(0,0,0,.45), inset 0 1px 0 rgba(255,255,255,.22);
  color:rgba(255,255,255,.9); font-size:1rem; font-weight:500; cursor:pointer;
  transition:transform .3s cubic-bezier(.4,0,.2,1), background-color .2s ease,
    box-shadow .3s ease; }}
button::before {{ content:''; position:absolute; inset:0; border-radius:inherit;
  padding:2px; background:linear-gradient(135deg, rgba(0,0,0,.8), transparent);
  -webkit-mask:linear-gradient(#fff 0 0) content-box, linear-gradient(#fff 0 0);
  -webkit-mask-composite:xor; mask-composite:exclude; pointer-events:none; }}
button:hover {{ transform:translateY(-2px); background:rgba(0,0,0,.35);
  box-shadow:0 12px 32px rgba(0,0,0,.6), inset 0 1px 0 rgba(255,255,255,.25); }}
button:hover::before {{ background:linear-gradient(135deg, rgba(255,255,255,.3), transparent); }}
button:active {{ transform:translateY(1px); }}
button:disabled {{ opacity:.5; cursor:not-allowed; transform:none; }}
/* Two faces per button; the submit-feedback script flips .loading on. */
button .busy {{ display:none; }}
button.loading .idle {{ display:none; }}
button.loading .busy {{ display:inline-flex; align-items:center; gap:.5rem; }}
.spin {{ width:1.25rem; height:1.25rem; animation:spin 1s linear infinite; }}
@keyframes spin {{ to {{ transform:rotate(360deg); }} }}
.err {{ background:rgba(239,68,68,.2); border:1px solid rgba(239,68,68,.4);
  color:#fecaca; padding:.75rem; border-radius:.5rem; margin-bottom:1rem;
  font-size:.875rem; text-align:left; }}
</style></head>
<body>{backgrounds}<main><div class="card">{body}</div></main>
<script>{submit_feedback}</script></body></html>"#,
        title = esc(title),
        app_name = esc(&app.app_name),
        body = body,
        submit_feedback = SUBMIT_FEEDBACK_JS,
        backgrounds = background_layers(mount_prefix),
        cycle = LOGIN_BACKGROUNDS.len() as u32 * 9,
        hold = 100 / LOGIN_BACKGROUNDS.len() as u32,
        fade = 100 / LOGIN_BACKGROUNDS.len() as u32 + 4,
    );
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        // The gate answers on the app's own port for an unauthenticated
        // caller; nothing here should be cached or framed.
        .header(header::CACHE_CONTROL, "no-store")
        // NOT X-Frame-Options: DENY. My Apps opens an app in an embedded
        // frame, so a blanket DENY made every gated app render as "app is
        // not responding" the moment the gate challenged it (reported on
        // a fleet node, 2026-08-05). frame-ancestors is the modern control
        // and can be precise: only pages from this same node may frame the
        // login, on any port or scheme, which is exactly the dashboard.
        // Anything else — another site embedding it to harvest the node
        // password — is still refused.
        // script-src admits exactly one script, by hash: the submit-feedback
        // snippet above. Injected markup (an app name, an error string) still
        // cannot execute — its hash would not match.
        .header(
            "Content-Security-Policy",
            format!(
                "default-src 'none'; img-src 'self' data:; style-src 'unsafe-inline'; \
                 script-src {hash}; form-action 'self'; frame-ancestors 'self' http://*:* https://*:*",
                hash = submit_feedback_csp_hash(),
            ),
        )
        .body(Body::from(html))
        .expect("static response builds")
}

/// The challenge. Names and pictures the app being opened, so the visitor can
/// confirm what they are authenticating to rather than being asked for a
/// password by an unexplained page.
fn login_page(
    app: &GatedPort,
    error: Option<&str>,
    status: StatusCode,
    mount_prefix: &str,
) -> Response<Body> {
    let body = format!(
        r#"{logo}
{icon}
<h1>Sign in to open {name}</h1>
<p class="sub">This app is protected by your node password.</p>
{err}
<form method="post" action="{prefix}login">
<input type="password" name="password" placeholder="Node password" autocomplete="current-password" autofocus required>
<button type="submit"><span class="idle">Sign in</span><span class="busy">{spinner}Signing in…</span></button>
</form>
{guest_form}"#,
        logo = logo_markup(),
        spinner = SPINNER_SVG,
        icon = icon_markup(app),
        name = esc(&app.app_name),
        err = error
            .map(|e| format!(r#"<div class="err">{}</div>"#, esc(e)))
            .unwrap_or_default(),
        prefix = gate_url(mount_prefix, ""),
        guest_form = if app.guest_access && app.auth_enabled {
            format!(
                r#"<details><summary>Have an app-only access token?</summary><p class="sub">This opens only this app, without a dashboard login. The app may also require its own account.</p><form method="post" action="{}"><input type="password" name="access_token" placeholder="App access token" autocomplete="off" required><button type="submit">Open this app</button></form></details>"#,
                gate_url(mount_prefix, "guest")
            )
        } else {
            String::new()
        },
    );
    page("Sign in", app, &body, status, mount_prefix)
}

/// Second factor. Reached only after the password verified, and the session
/// backing it cannot authorise anything until this completes.
fn totp_page(
    app: &GatedPort,
    error: Option<&str>,
    status: StatusCode,
    mount_prefix: &str,
) -> Response<Body> {
    let body = format!(
        r#"{icon}
<h1>Two-factor code</h1>
<p class="sub">Enter the 6-digit code to open {name}.</p>
{err}
<form method="post" action="{prefix}totp">
<input type="text" name="code" inputmode="numeric" pattern="[0-9]*" autocomplete="one-time-code" placeholder="000000" autofocus required>
<button type="submit"><span class="idle">Verify</span><span class="busy">{spinner}Verifying…</span></button>
</form>"#,
        spinner = SPINNER_SVG,
        icon = icon_markup(app),
        name = esc(&app.app_name),
        err = error
            .map(|e| format!(r#"<div class="err">{}</div>"#, esc(e)))
            .unwrap_or_default(),
        prefix = gate_url(mount_prefix, ""),
    );
    page("Two-factor", app, &body, status, mount_prefix)
}

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn managed_gateway_rejects_stale_identity_or_disabled_guest_policy_before_login() {
        let gate = test_gate().await;
        let mut app = app();
        app.guest_access = true;
        for (expected, enabled, declared) in [
            ("another-app", true, true),
            ("strfry", false, true),
            ("strfry", true, false),
        ] {
            app.auth_enabled = enabled;
            app.declared = declared;
            *gate.port_map.write().await = identity::test_port_map(app.clone());
            for path in ["/", "/manifest.json", "/__archipelago-gate/guest"] {
                let request = Request::get(path)
                    .header("x-archipelago-app", expected)
                    .body(Body::empty())
                    .unwrap();
                let response = gate
                    .handle(request, &app, "127.0.0.1".parse().unwrap())
                    .await;
                assert_eq!(response.status(), StatusCode::NOT_FOUND);
            }
        }
    }

    #[tokio::test]
    async fn guest_login_is_app_only_and_revocation_blocks_subsequent_requests() {
        let gate = test_gate().await;
        let mut app = app();
        app.guest_access = true;
        *gate.port_map.write().await = identity::test_port_map(app.clone());
        let token = crate::device_tokens::create_scoped_expiring(
            &gate.data_dir,
            "external:test:Guest",
            Some(vec![app.app_id.clone()]),
            Some(u64::MAX),
        )
        .await
        .unwrap();
        let mut request = Request::post(format!("{GATE_PREFIX}guest"))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(format!("access_token={token}")))
            .unwrap();
        request.extensions_mut().insert(SecureTransport(true));
        let response = gate
            .handle(request, &app, "127.0.0.1".parse().unwrap())
            .await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let cookies: Vec<_> = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|h| h.to_str().unwrap())
            .collect();
        assert_eq!(cookies.len(), 1);
        assert!(
            cookies[0].starts_with("archy_app_access_strfry=")
                && cookies[0].contains("; Secure")
                && cookies[0].contains("HttpOnly")
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            cookies[0].split(';').next().unwrap().parse().unwrap(),
        );
        assert_eq!(
            gate.authorize(&headers, &app.app_id).await,
            Authorization::AllowGuest
        );
        assert_eq!(
            gate.authorize(&headers, "lnd").await,
            Authorization::Challenge
        );
        assert!(!gate.sessions.validate(&token).await);
        assert!(crate::device_tokens::verify(&gate.data_dir, &token)
            .await
            .is_none());
        let mut bearer = HeaderMap::new();
        bearer.insert(
            header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
        assert_eq!(
            gate.authorize(&bearer, &app.app_id).await,
            Authorization::AllowGuestToken
        );
        app.guest_access = false;
        *gate.port_map.write().await = identity::test_port_map(app.clone());
        assert_eq!(
            gate.authorize(&bearer, &app.app_id).await,
            Authorization::Challenge
        );
        assert_eq!(
            gate.authorize(&headers, &app.app_id).await,
            Authorization::Challenge
        );
        app.guest_access = true;
        *gate.port_map.write().await = identity::test_port_map(app.clone());
        crate::device_tokens::remove(&gate.data_dir, "external:test:Guest")
            .await
            .unwrap();
        assert_eq!(
            gate.authorize(&headers, &app.app_id).await,
            Authorization::Challenge
        );
    }

    #[test]
    fn guest_and_remember_credentials_never_reach_the_app() {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, "session=owner; remember=master; csrf_token=csrf; archy_app_access_nextcloud=guest; own_app_session=keep".parse().unwrap());
        strip_gate_cookies(&mut headers);
        assert_eq!(headers[header::COOKIE], "own_app_session=keep");
    }

    #[test]
    fn credentialless_allowlist_covers_the_manifest_that_broke_apps() {
        // A <link rel="manifest"> fetch never carries the cookie, so these must
        // pass or a logged-in user still gets 401 + a login page.
        for p in ["/manifest.json", "/site.webmanifest", "/favicon.ico"] {
            assert!(
                AppGate::is_credentialless_public_path(p),
                "{p} still challenged"
            );
        }
    }

    #[test]
    fn credentialless_allowlist_is_exact_match_only() {
        // A prefix match here would be an authentication bypass. Anything that
        // merely CONTAINS or extends an allowed path must still be challenged.
        for p in [
            "/manifest.json/../api/secrets",
            "/manifest.json?x=1/../secrets",
            "/api/manifest.json",
            "/manifest.jsonx",
            "/MANIFEST.JSON",
            "/",
            "/api/auth/nostr/session",
            "/admin",
        ] {
            assert!(
                !AppGate::is_credentialless_public_path(p),
                "{p} wrongly bypassed the gate"
            );
        }
    }
    use super::*;

    fn app() -> GatedPort {
        GatedPort {
            guest_access: false,
            port: 8090,
            app_id: "strfry".to_string(),
            app_name: "Strfry Relay".to_string(),
            icon: None,
            declared: true,
            session_passthrough: false,
            auth_enabled: true,
        }
    }

    #[test]
    fn bearer_token_is_parsed_case_insensitively() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer abc123".parse().unwrap());
        assert_eq!(bearer_token(&headers), Some("abc123".to_string()));

        headers.insert(header::AUTHORIZATION, "bearer abc123".parse().unwrap());
        assert_eq!(bearer_token(&headers), Some("abc123".to_string()));
    }

    #[test]
    fn non_bearer_authorization_is_ignored() {
        let mut headers = HeaderMap::new();
        // An app's own Basic credential must never be mistaken for ours.
        headers.insert(header::AUTHORIZATION, "Basic dXNlcjpwYXNz".parse().unwrap());
        assert_eq!(bearer_token(&headers), None);
        headers.insert(header::AUTHORIZATION, "Bearer   ".parse().unwrap());
        assert_eq!(bearer_token(&headers), None);
    }

    #[test]
    fn forwarded_mount_prefix_accepts_only_a_safe_absolute_path() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-prefix",
            "/app/archipelago-source/".parse().unwrap(),
        );
        assert_eq!(forwarded_mount_prefix(&headers), "/app/archipelago-source");

        for unsafe_value in [
            "//other.example/app",
            "/app/../admin",
            "/app//source",
            "/app/source?next=//other.example",
            "https://other.example/app",
            "/app/%2e%2e/admin",
        ] {
            headers.insert("x-forwarded-prefix", unsafe_value.parse().unwrap());
            assert_eq!(
                forwarded_mount_prefix(&headers),
                "",
                "accepted {unsafe_value}"
            );
        }
    }

    #[tokio::test]
    async fn login_page_names_the_app() {
        let resp = login_page(&app(), None, StatusCode::UNAUTHORIZED, "");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(html.contains("Sign in to open Strfry Relay"));
        // A lettermark stands in when the manifest declares no icon.
        assert!(html.contains("lettermark"));
    }

    #[tokio::test]
    async fn page_escapes_app_names() {
        let mut app = app();
        app.app_name = r#"<script>alert(1)</script>"#.to_string();
        let resp = login_page(&app, None, StatusCode::UNAUTHORIZED, "");
        let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[tokio::test]
    async fn error_messages_are_escaped() {
        let resp = login_page(
            &app(),
            Some("<img src=x onerror=1>"),
            StatusCode::UNAUTHORIZED,
            "",
        );
        let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(!html.contains("<img src=x"));
    }

    /// The challenge must be framable by this node's own dashboard — My Apps
    /// opens apps in an embedded frame, and a blanket `X-Frame-Options: DENY`
    /// turned every gated app into "app is not responding" (observed on a fleet node,
    /// 2026-08-05). It must still be uncacheable, and still refuse to be
    /// framed by a foreign origin, which `frame-ancestors` expresses and
    /// `X-Frame-Options` cannot.
    /// Upstream frame-blocking must not survive the proxy: X-Frame-Options
    /// goes away entirely, CSP loses ONLY its frame-ancestors directive —
    /// the app's remaining policy must pass through byte-preserving in
    /// content (Alby Hub's DENY + strict CSP was the real-world case,
    /// archi-dev-box 2026-08-12).
    #[test]
    fn proxied_responses_lose_frame_blocking_but_keep_the_apps_csp() {
        let mut headers = hyper::HeaderMap::new();
        headers.insert("x-frame-options", "DENY".parse().unwrap());
        headers.insert(
            "content-security-policy",
            "default-src 'self'; frame-ancestors 'none'; img-src 'self' https://cdn.example"
                .parse()
                .unwrap(),
        );
        neutralize_frame_blocking(&mut headers);
        assert!(!headers.contains_key("x-frame-options"));
        let csp = headers["content-security-policy"].to_str().unwrap();
        assert!(!csp.contains("frame-ancestors"));
        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("img-src 'self' https://cdn.example"));

        // CSP that is ONLY a frame-ancestors directive disappears entirely.
        let mut only = hyper::HeaderMap::new();
        only.insert(
            "content-security-policy",
            "frame-ancestors 'self'".parse().unwrap(),
        );
        neutralize_frame_blocking(&mut only);
        assert!(!only.contains_key("content-security-policy"));

        // No frame directives at all → CSP untouched.
        let mut plain = hyper::HeaderMap::new();
        plain.insert(
            "content-security-policy",
            "default-src 'self'".parse().unwrap(),
        );
        neutralize_frame_blocking(&mut plain);
        assert_eq!(
            plain["content-security-policy"].to_str().unwrap(),
            "default-src 'self'"
        );
    }

    #[test]
    fn challenge_pages_are_uncacheable_and_framable_only_by_this_node() {
        let resp = login_page(&app(), None, StatusCode::UNAUTHORIZED, "");
        assert_eq!(resp.headers()[header::CACHE_CONTROL], "no-store");
        assert!(
            !resp.headers().contains_key("X-Frame-Options"),
            "X-Frame-Options cannot express 'my own node on another port' — it \
             blocked the dashboard's own frame"
        );
        let csp = resp.headers()["Content-Security-Policy"].to_str().unwrap();
        assert!(csp.contains("frame-ancestors 'self'"));
        assert!(csp.contains("form-action 'self'"));
    }

    /// A dead upstream must render as a page that names the app and retries,
    /// not the bare string "app is not responding" — that string standing
    /// alone on the app's own port read as the node being broken (Gitea on a
    /// fleet node, 2026-08-10). The 502 status must survive so machine
    /// clients still see an upstream failure.
    #[tokio::test]
    async fn a_dead_app_gets_a_named_retrying_page_not_a_bare_string() {
        let resp = app_down_page(&app());
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(resp.headers()["Refresh"], "5");
        assert_eq!(resp.headers()[header::CACHE_CONTROL], "no-store");
        let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(html.contains("Strfry Relay is not responding"));
        assert!(html.contains("<html"), "must be a page, not a bare string");
    }

    /// The login page must render entirely from the gate's own origin: the
    /// CSP allows no external host, so a background that 404s leaves a black
    /// page rather than the dashboard's art. The badge itself is inline SVG —
    /// the same 20 squares as the dashboard login's AnimatedLogo — so it can
    /// never 404 at all.
    #[tokio::test]
    async fn login_page_sources_its_art_from_the_gate() {
        let resp = login_page(&app(), None, StatusCode::UNAUTHORIZED, "");
        let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let html = String::from_utf8_lossy(&body).to_string();
        assert_eq!(
            html.matches(r#"class="sq""#).count(),
            LOGO_RECTS.len(),
            "the badge must draw every AnimatedLogo square inline"
        );
        for name in LOGIN_BACKGROUNDS {
            assert!(
                html.contains(&format!("{GATE_PREFIX}asset/{name}")),
                "background {name} is not referenced"
            );
        }
    }

    #[tokio::test]
    async fn mounted_login_keeps_forms_assets_and_redirect_inside_the_app() {
        let mount = "/app/archipelago-source";
        let resp = login_page(&app(), None, StatusCode::UNAUTHORIZED, mount);
        let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(html.contains(r#"action="/app/archipelago-source/__archipelago-gate/login""#));
        for name in LOGIN_BACKGROUNDS {
            assert!(html.contains(&format!("/app/archipelago-source{GATE_PREFIX}asset/{name}")));
        }

        let redirect = redirect_to_app(mount);
        assert_eq!(redirect.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            redirect.headers()[header::LOCATION],
            "/app/archipelago-source/"
        );
    }

    /// The only script the challenge pages may run is the submit-feedback
    /// snippet, admitted by hash. The page must carry exactly that script,
    /// and the CSP must name its hash — anything injected has a different
    /// hash and stays inert.
    #[tokio::test]
    async fn submit_feedback_script_is_present_and_hash_pinned() {
        let resp = login_page(&app(), None, StatusCode::UNAUTHORIZED, "");
        let csp = resp.headers()["Content-Security-Policy"]
            .to_str()
            .unwrap()
            .to_string();
        assert!(csp.contains(&format!("script-src {}", submit_feedback_csp_hash())));
        assert!(!csp.contains("script-src 'unsafe-inline'"));
        let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(html.contains(&format!("<script>{SUBMIT_FEEDBACK_JS}</script>")));
        // Both button faces render: idle label and the spinner face.
        assert!(html.contains(r#"<span class="idle">Sign in</span>"#));
        assert!(html.contains("Signing in…"));
        assert!(html.contains(r#"class="spin""#));
    }

    /// The allowlist is the whole security boundary for asset serving: the
    /// name arrives in a URL and is read before any authentication.
    #[test]
    fn asset_serving_refuses_anything_off_the_allowlist() {
        for name in [
            "../../../etc/passwd",
            "/etc/passwd",
            "db.sqlite3",
            "manifest.yml",
            "",
        ] {
            assert!(
                read_ui_asset(name).is_none(),
                "{name} must not be servable by the gate"
            );
        }
    }

    #[tokio::test]
    async fn form_parsing_decodes_percent_and_plus() {
        let req = Request::builder()
            .body(Body::from("password=a%40b+c&code=123456"))
            .unwrap();
        let form = read_form(req).await.unwrap();
        assert_eq!(field(&form, "password"), Some("a@b c".to_string()));
        assert_eq!(field(&form, "code"), Some("123456".to_string()));
    }

    #[tokio::test]
    async fn oversized_form_bodies_are_refused() {
        let req = Request::builder()
            .body(Body::from("x=".to_string() + &"a".repeat(MAX_FORM_BYTES)))
            .unwrap();
        assert!(read_form(req).await.is_none());
    }

    #[tokio::test]
    async fn no_credential_is_challenged() {
        let gate = test_gate().await;
        assert_eq!(
            gate.authorize(&HeaderMap::new(), "strfry").await,
            Authorization::Challenge
        );
    }

    #[tokio::test]
    async fn a_valid_session_cookie_is_allowed() {
        let gate = test_gate().await;
        let token = gate.sessions.create().await;
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, format!("session={token}").parse().unwrap());
        assert_eq!(
            gate.authorize(&headers, "strfry").await,
            Authorization::Allow
        );
    }

    /// The gate must remove ONLY its own cookie pairs: an app's login cookie
    /// riding the same header has to survive, or every gated app with its
    /// own auth (vaultwarden, nextcloud, gitea) is logged out on each
    /// request — the 2026-08-05 companion-UI/"app logged me out" regression.
    #[test]
    fn strip_gate_cookies_keeps_app_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "session=abc; vw_session=keepme; csrf_token=def; theme=dark"
                .parse()
                .unwrap(),
        );
        strip_gate_cookies(&mut headers);
        assert_eq!(
            headers.get(header::COOKIE).unwrap().to_str().unwrap(),
            "vw_session=keepme; theme=dark"
        );
    }

    #[test]
    fn strip_gate_cookies_drops_header_when_only_gate_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "session=abc; csrf_token=def".parse().unwrap(),
        );
        strip_gate_cookies(&mut headers);
        assert!(headers.get(header::COOKIE).is_none());
    }

    #[test]
    fn cookie_value_finds_only_a_nonempty_named_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "app_session=keep; csrf_token=csrf123; empty="
                .parse()
                .unwrap(),
        );
        assert_eq!(
            cookie_value(&headers, "csrf_token"),
            Some("csrf123".to_string())
        );
        assert_eq!(cookie_value(&headers, "session"), None);
        assert_eq!(cookie_value(&headers, "empty"), None);
    }

    /// An app-gate login must be equivalent to a dashboard login. The session
    /// cookie alone can load the broker route, but every identity/signing RPC
    /// also needs the matching readable CSRF cookie.
    #[tokio::test]
    async fn app_gate_login_establishes_the_dashboard_csrf_cookie() {
        let token = "app-gate-session-token";
        let mut resp = redirect_to_app("");

        set_session_cookie(&mut resp, token).await;

        let cookies: Vec<_> = resp
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect();
        let expected_csrf = crate::api::rpc::derive_csrf_token(token).await.unwrap();
        assert!(cookies
            .iter()
            .any(|cookie| cookie.starts_with(&format!("session={token};"))));
        assert!(cookies
            .iter()
            .any(|cookie| cookie.starts_with(&format!("csrf_token={expected_csrf};"))));
        assert!(cookies
            .iter()
            .any(|cookie| cookie.starts_with("session=") && cookie.contains("HttpOnly")));
        assert!(cookies
            .iter()
            .any(|cookie| cookie.starts_with("csrf_token=") && !cookie.contains("HttpOnly")));
    }

    /// The regression that killed every Nostr login on 2026-08-06.
    ///
    /// IndeeHub's NIP-98 credential rides in `Authorization: Nostr <event>`
    /// while the *gate's* credential is the session cookie. The gate must
    /// classify that as a plain `Allow` — not as its own token — because only
    /// `AllowGateToken` strips the header. Getting this wrong deleted the
    /// signed event one hop before the app, and the app answered
    /// "Authorization header is missing" no matter which signer produced it.
    #[tokio::test]
    async fn an_apps_own_authorization_is_not_mistaken_for_the_gates() {
        let gate = test_gate().await;
        let session = gate.sessions.create().await;
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("session={session}").parse().unwrap(),
        );
        for scheme in [
            "Nostr eyJraW5kIjogMjcyMzV9",
            "Basic dXNlcjpwYXNz",
            "Token app-issued-abc",
        ] {
            headers.insert(header::AUTHORIZATION, scheme.parse().unwrap());
            assert_eq!(
                gate.authorize(&headers, "indeehub").await,
                Authorization::Allow,
                "{scheme} was treated as the gate's own credential"
            );
        }
    }

    /// The end of that path: an authorised request must reach the app with the
    /// app's own `Authorization` intact. Asserted against a real proxy hop —
    /// the header survived every unit test of the classifier while being
    /// dropped in `proxy_to_app`, which is precisely how this shipped.
    #[tokio::test]
    async fn proxy_forwards_an_apps_own_authorization_and_withholds_the_gates() {
        // A stand-in app that reports the Authorization header it received.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let _ = hyper::server::conn::Http::new()
                    .serve_connection(
                        stream,
                        hyper::service::service_fn(|req: Request<Body>| async move {
                            let seen = req
                                .headers()
                                .get(header::AUTHORIZATION)
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("<none>")
                                .to_string();
                            Ok::<_, hyper::Error>(Response::new(Body::from(seen)))
                        }),
                    )
                    .await;
            }
        });

        let mut app = app();
        app.port = port;

        async fn seen_by_app(app: &GatedPort, header_value: &str, drop: bool) -> String {
            let req = Request::builder()
                .method(Method::POST)
                .uri("/api/auth/nostr/session")
                .header(header::AUTHORIZATION, header_value)
                .body(Body::empty())
                .unwrap();
            let resp = proxy_to_app(req, app, drop).await;
            let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
            String::from_utf8_lossy(&body).to_string()
        }

        // The app's own credential reaches the app.
        assert_eq!(
            seen_by_app(&app, "Nostr eyJraW5kIjogMjcyMzV9", false).await,
            "Nostr eyJraW5kIjogMjcyMzV9"
        );
        // The gate's own device token never does.
        assert_eq!(
            seen_by_app(&app, "Bearer gate-device-token", true).await,
            "<none>"
        );
    }

    #[test]
    fn strip_gate_cookies_no_header_is_a_noop() {
        let mut headers = HeaderMap::new();
        strip_gate_cookies(&mut headers);
        assert!(headers.get(header::COOKIE).is_none());
    }

    /// The load-bearing 2FA property: a session still awaiting its TOTP code
    /// fails `validate()`, so the gate rejects it without knowing anything
    /// about second factors.
    #[tokio::test]
    async fn a_pending_2fa_session_is_challenged() {
        let gate = test_gate().await;
        let pending = gate.sessions.create_pending(vec![1, 2, 3]).await;
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("session={pending}").parse().unwrap(),
        );
        assert_eq!(
            gate.authorize(&headers, "strfry").await,
            Authorization::Challenge
        );
    }

    #[tokio::test]
    async fn a_garbage_cookie_is_challenged() {
        let gate = test_gate().await;
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, "session=deadbeef".parse().unwrap());
        assert_eq!(
            gate.authorize(&headers, "strfry").await,
            Authorization::Challenge
        );
    }

    async fn test_gate() -> AppGate {
        let dir = std::env::temp_dir().join(format!("appgate-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        AppGate::new(
            SessionStore::new().await,
            AuthManager::new(dir.clone()),
            LoginRateLimiter::new(),
            dir,
        )
    }
}

#[cfg(test)]
mod maintenance_tests {
    #[test]
    fn held_indee_ingress_never_reaches_upstream_or_another_app() {
        let root = tempfile::tempdir().unwrap();
        assert!(!super::maintenance_active(root.path(), "indeedhub"));
        std::fs::create_dir(root.path().join("app-maintenance")).unwrap();
        std::fs::write(
            root.path().join("app-maintenance/indeedhub"),
            uuid::Uuid::new_v4().to_string(),
        )
        .unwrap();
        assert!(super::maintenance_active(root.path(), "indeedhub"));
        assert!(super::maintenance_active(root.path(), "indeedhub-api"));
        assert!(!super::maintenance_active(root.path(), "node-demo-v4v"));
        std::fs::remove_file(root.path().join("app-maintenance/indeedhub")).unwrap();
        assert!(!super::maintenance_active(root.path(), "indeedhub"));
    }
}
