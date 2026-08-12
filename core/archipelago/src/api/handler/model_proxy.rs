//! Session-gated forwarder for `/aiui/api/claude/*` and `/aiui/api/ollama/*`.
//!
//! Replaces `claude-api-proxy.py` — a standalone Python process on port 3142
//! holding its **own** copy of the Anthropic API key, reachable with **no
//! session gate** — and retires the `/aiui/api/openrouter/` open relay
//! entirely (13-02-PLAN.md, T-13-08/T-13-09/T-13-10/T-13-11/T-13-12). Anyone
//! who could reach the node's web port could spend the owner's API budget.
//!
//! The daemon re-derives auth from the request's own session cookie — it
//! does not trust nginx to have gated the request already, the same
//! discipline `/lnd-connect-info`'s doc comment spells out for exactly this
//! reason (a second front door, or a misconfigured proxy, must not become a
//! silent bypass). It reads the node's single Claude key ledger
//! (`data_dir/secrets/claude-api-key`) fresh on every call rather than
//! caching it, and never forwards an inbound `x-api-key`, `authorization`
//! or `cookie` header upstream (T-13-14) — a caller must not be able to
//! bill a different account or leak the node's session to Anthropic.

use super::ApiHandler;
use crate::session::{self, SessionStore};
use anyhow::Result;
use hyper::{Body, HeaderMap, Method, Request, Response, StatusCode};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Anthropic Messages API base. The node's single key ledger
/// (`data_dir/secrets/claude-api-key`) authenticates every forwarded call.
const CLAUDE_UPSTREAM: &str = "https://api.anthropic.com/";
/// Local Ollama. No key — the session gate exists purely to stop anonymous
/// consumption of local GPU/CPU inference (T-13-11), not to protect a secret.
const OLLAMA_UPSTREAM: &str = "http://127.0.0.1:11434/";
/// Local SearXNG. Same gate rationale as Ollama: anonymous web search
/// through this endpoint attributes arbitrary queries to the node's IP at
/// external engines (S4 — the old nginx location proxied straight to :8888
/// with no session check at all).
const SEARXNG_UPSTREAM: &str = "http://127.0.0.1:8888/";
/// Generous enough for a multi-turn tool-call round trip; `mesh/listener/
/// assist.rs`'s OLLAMA_TIMEOUT (60s) is airtime-tuned for LoRa and not
/// reusable here — this path has no such constraint (13-AI-SPEC.md Pitfall 6).
const FORWARD_TIMEOUT_SECS: u64 = 180;

impl ApiHandler {
    /// Entry point wired into the `/aiui/api/claude/` and `/aiui/api/ollama/`
    /// arms in `mod.rs`. Kept as a thin method so it can read
    /// `self.session_store` / `self.config.data_dir`; the actual routing and
    /// forwarding logic lives in free functions below so it is unit-testable
    /// without constructing a full `ApiHandler` (RpcHandler + orchestrators +
    /// blob store) in every test.
    pub(super) async fn handle_model_proxy(
        &self,
        req: Request<Body>,
        path: &str,
    ) -> Result<Response<Body>> {
        route_model_proxy(&self.session_store, &self.config.data_dir, req, path).await
    }
}

/// Routing + auth gate, factored out of the `ApiHandler` method so tests can
/// exercise it with `SessionStore::new_for_tests` and a `tempfile` data_dir.
async fn route_model_proxy(
    session_store: &SessionStore,
    data_dir: &Path,
    req: Request<Body>,
    path: &str,
) -> Result<Response<Body>> {
    if !is_authenticated(session_store, req.headers()).await {
        tracing::warn!("401 model proxy {} — session invalid or missing", path);
        return Ok(unauthorized());
    }
    if let Some(rest) = path.strip_prefix("/aiui/api/claude/") {
        forward_claude(req, rest, data_dir).await
    } else if let Some(rest) = path.strip_prefix("/aiui/api/ollama/") {
        forward_ollama(req, rest).await
    } else if path.starts_with("/aiui/api/web-search") {
        forward_web_search(req, data_dir).await
    } else {
        // Unreachable given the caller's prefix match in mod.rs, but never
        // fall through to an unauthenticated 200 on an unrecognized path.
        Ok(unauthorized())
    }
}

/// Re-derive session auth from the request's own cookie. Deliberately not a
/// call back into `ApiHandler::is_authenticated` — keeping this small and
/// dependency-free is what makes the 401 behaviour unit-testable without
/// paying for a full `ApiHandler` in every test.
async fn is_authenticated(session_store: &SessionStore, headers: &HeaderMap) -> bool {
    match session::extract_session_cookie(headers) {
        Some(token) => session_store.validate(&token).await,
        None => false,
    }
}

fn unauthorized() -> Response<Body> {
    let body = serde_json::json!({ "error": "Unauthorized" });
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap_or_default()))
        .unwrap_or_else(|_| Response::new(Body::from("Unauthorized")))
}

/// A plain-language 503 naming the missing key — never a 500, and never the
/// key's filesystem path (that would hand an authenticated-but-untrusted
/// caller a hint about the node's on-disk layout for no benefit to them).
fn key_not_configured() -> Response<Body> {
    let body = serde_json::json!({
        "error": "Claude is not configured on this node yet — set an API key in Settings."
    });
    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap_or_default()))
        .unwrap_or_else(|_| Response::new(Body::from("Claude is not configured")))
}

/// S3: a forwarded body/query carried secret-shaped content (BIP39 words,
/// key/token shapes, or a literal value from this node's secrets dir). The
/// backends' egress screen never sees the forwarder path — the standalone
/// frontend posts FULL history and images straight here — so the forwarder
/// screens for itself. 400, plain-language, never naming what matched.
fn blocked_secret_shaped() -> Response<Body> {
    let body = serde_json::json!({
        "error": "Blocked: this request contained secret-shaped content (e.g. a seed phrase, key, or token). It was not sent anywhere."
    });
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap_or_default()))
        .unwrap_or_else(|_| Response::new(Body::from("Blocked: secret-shaped content")))
}

/// Screen a string about to leave the node through the forwarder against
/// the assistant's secret-shape rules (G-B1) with this node's own secrets
/// as the deny corpus. Returns Some(kind) — kind only, never the value —
/// when the content must not leave.
async fn forward_screen(text: &str, data_dir: &Path) -> Option<&'static str> {
    let secrets = crate::assistant::egress::load_known_secrets(&data_dir.join("secrets")).await;
    crate::assistant::egress::scan_secret_shapes(text, &secrets)
}

fn bad_gateway(msg: &str) -> Response<Body> {
    let body = serde_json::json!({ "error": msg });
    Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap_or_default()))
        .unwrap_or_else(|_| Response::new(Body::from(msg.to_string())))
}

/// Forward an already-authenticated request to Anthropic's Messages API.
/// `rest` is the path remainder after `/aiui/api/claude/` has been stripped
/// by the caller (e.g. `v1/messages`).
async fn forward_claude(req: Request<Body>, rest: &str, data_dir: &Path) -> Result<Response<Body>> {
    let key_path: PathBuf = data_dir.join("secrets/claude-api-key");
    let api_key = match tokio::fs::read_to_string(&key_path).await {
        Ok(k) if !k.trim().is_empty() => k.trim().to_string(),
        _ => {
            tracing::warn!("model proxy: claude key ledger missing, refusing forward");
            return Ok(key_not_configured());
        }
    };
    // S3: screen the outbound body before it leaves. The forwarder also
    // serves the STANDALONE frontend, whose requests carry full history and
    // base64 images with no assistant loop (and no egress screen) behind
    // them — an operator pasting a seed phrase into standalone chat would
    // otherwise send it straight to Anthropic.
    let (parts, body) = req.into_parts();
    let payload = hyper::body::to_bytes(body)
        .await
        .map_err(|e| anyhow::anyhow!("read request payload: {e}"))?;
    if let Some(kind) = forward_screen(&String::from_utf8_lossy(&payload), data_dir).await {
        tracing::error!(
            kind,
            "model proxy: blocked claude forward — secret-shaped content"
        );
        return Ok(blocked_secret_shaped());
    }
    let req = Request::from_parts(parts, Body::from(payload));
    forward(
        req,
        rest,
        CLAUDE_UPSTREAM,
        "api.anthropic.com",
        &[
            ("x-api-key", api_key),
            ("anthropic-version", "2023-06-01".to_string()),
        ],
    )
    .await
}

/// Forward an already-authenticated request to the node's local Ollama.
/// `rest` is the path remainder after `/aiui/api/ollama/` has been stripped.
async fn forward_ollama(req: Request<Body>, rest: &str) -> Result<Response<Body>> {
    forward(req, rest, OLLAMA_UPSTREAM, "127.0.0.1:11434", &[]).await
}

/// Build the upstream SearXNG path from the inbound query string, forcing
/// `format=json` — the AIUI client speaks only JSON, SearXNG answers HTML
/// unless asked, and the old nginx location passed the query through
/// untouched, so "web search" could 200 with a page that parsed as nothing.
/// A caller-supplied `format=` is stripped first so it cannot win.
fn web_search_upstream_path(query: &str) -> String {
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| !pair.is_empty() && !pair.starts_with("format="))
        .collect();
    if kept.is_empty() {
        "search?format=json".to_string()
    } else {
        format!("search?{}&format=json", kept.join("&"))
    }
}

/// Forward an already-authenticated GET to the node's local SearXNG. GET
/// only; 30s is plenty for a metasearch round trip and keeps a wedged
/// upstream from pinning a daemon task. The query is screened (S3): SearXNG
/// fans it out to upstream engines, so a pasted seed phrase would leave the
/// node here exactly as surely as in a Claude body.
async fn forward_web_search(req: Request<Body>, data_dir: &Path) -> Result<Response<Body>> {
    if req.method() != Method::GET {
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .header("Content-Type", "application/json")
            .body(Body::from("{\"error\":\"GET only\"}"))
            .unwrap_or_else(|_| Response::new(Body::from("GET only"))));
    }
    let query = req.uri().query().unwrap_or_default();
    // Minimal decode for the word-shape scan: percent-encoded spaces and
    // `+` are how a pasted phrase's words separate inside a query string.
    let decoded = query.replace('+', " ").replace("%20", " ");
    if let Some(kind) = forward_screen(&decoded, data_dir).await {
        tracing::error!(
            kind,
            "model proxy: blocked web-search query — secret-shaped content"
        );
        return Ok(blocked_secret_shaped());
    }
    let rest = web_search_upstream_path(query);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| anyhow::anyhow!("client build: {e}"))?;
    let url = format!("{}{}", SEARXNG_UPSTREAM, rest);
    match client.get(&url).send().await {
        Ok(resp) => stream_response(resp),
        Err(e) => {
            tracing::warn!("model proxy: searxng upstream failed: {}", e);
            Ok(bad_gateway("web search upstream unavailable"))
        }
    }
}

/// Shared forwarding core for both backends. Copies ONLY the inbound
/// `content-type`/`accept` request headers plus whatever `extra_headers`
/// the caller supplies (the Claude key + version pin) — the inbound
/// `x-api-key`, `authorization` and `cookie` headers are never read, let
/// alone forwarded (T-13-14). Streams the upstream response back rather
/// than buffering it, matching `proxy.rs`'s peer-content streaming shape,
/// so token-by-token replies still stream to the browser.
async fn forward(
    req: Request<Body>,
    rest: &str,
    upstream_base: &str,
    upstream_host_for_log: &str,
    extra_headers: &[(&str, String)],
) -> Result<Response<Body>> {
    let method = req.method().clone();
    let (parts, body) = req.into_parts();
    let content_type = parts
        .headers
        .get(hyper::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();
    let accept = parts
        .headers
        .get(hyper::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let payload = hyper::body::to_bytes(body)
        .await
        .map_err(|e| anyhow::anyhow!("read request payload: {e}"))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(FORWARD_TIMEOUT_SECS))
        .build()
        .map_err(|e| anyhow::anyhow!("client build: {e}"))?;

    let reqwest_method =
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::POST);
    let url = format!("{}{}", upstream_base, rest);
    let mut upstream_req = client
        .request(reqwest_method, &url)
        .header("content-type", content_type);
    for (name, value) in extra_headers {
        upstream_req = upstream_req.header(*name, value);
    }
    if let Some(accept) = accept {
        upstream_req = upstream_req.header("accept", accept);
    }
    // GET requests to Ollama carry no payload; avoid sending an empty body
    // on GET, which some servers treat differently from "no body at all".
    if method != Method::GET || !payload.is_empty() {
        upstream_req = upstream_req.body(payload.to_vec());
    }

    match upstream_req.send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            tracing::info!(
                "model proxy: forwarded to {}, status={}",
                upstream_host_for_log,
                status
            );
            stream_response(resp)
        }
        Err(e) => {
            tracing::warn!(
                "model proxy: upstream request to {} failed: {}",
                upstream_host_for_log,
                e
            );
            Ok(bad_gateway("upstream request failed"))
        }
    }
}

/// Stream the upstream response straight through instead of buffering it —
/// same shape as `proxy.rs`'s peer-content Range streamer — so a
/// token-by-token reply doesn't wait for the full response before the first
/// byte reaches the browser.
fn stream_response(resp: reqwest::Response) -> Result<Response<Body>> {
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let mut builder = Response::builder().status(status);
    for h in ["content-type", "content-length"] {
        if let Some(v) = headers.get(h).and_then(|v| v.to_str().ok()) {
            builder = builder.header(h, v);
        }
    }
    builder
        .body(Body::wrap_stream(resp.bytes_stream()))
        .map_err(|e| anyhow::anyhow!("response build: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    /// Unique suffix for a per-test temp file path (matches the pattern
    /// `session.rs`'s own tests already use — not key material, just a
    /// filename component, drawn unguarded).
    fn uniq() -> u64 {
        rand::RngCore::next_u64(&mut rand::rngs::OsRng)
    }

    async fn test_store() -> SessionStore {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("archy-model-proxy-test-sessions-{}.json", uniq()));
        SessionStore::new_for_tests(path)
    }

    fn req_with_cookie(method: &str, path: &str, cookie: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(c) = cookie {
            builder = builder.header("cookie", format!("session={c}"));
        }
        builder.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn claude_without_session_is_401() {
        let store = test_store().await;
        let data_dir = tempfile::tempdir().unwrap();
        let req = req_with_cookie("POST", "/aiui/api/claude/v1/messages", None);
        let resp = route_model_proxy(&store, data_dir.path(), req, "/aiui/api/claude/v1/messages")
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn ollama_without_session_is_401() {
        let store = test_store().await;
        let data_dir = tempfile::tempdir().unwrap();
        let req = req_with_cookie("GET", "/aiui/api/ollama/api/tags", None);
        let resp = route_model_proxy(&store, data_dir.path(), req, "/aiui/api/ollama/api/tags")
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn web_search_without_session_is_401() {
        let store = test_store().await;
        let data_dir = tempfile::tempdir().unwrap();
        let req = req_with_cookie("GET", "/aiui/api/web-search?q=bitcoin", None);
        let resp = route_model_proxy(&store, data_dir.path(), req, "/aiui/api/web-search")
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn web_search_query_forces_json_and_strips_format() {
        assert_eq!(
            web_search_upstream_path("q=bitcoin"),
            "search?q=bitcoin&format=json"
        );
        assert_eq!(
            web_search_upstream_path("q=bitcoin+halving&format=html"),
            "search?q=bitcoin+halving&format=json"
        );
        assert_eq!(web_search_upstream_path(""), "search?format=json");
    }

    #[tokio::test]
    async fn claude_with_invalid_session_is_401() {
        let store = test_store().await;
        let data_dir = tempfile::tempdir().unwrap();
        let req = req_with_cookie(
            "POST",
            "/aiui/api/claude/v1/messages",
            Some("not-a-real-token"),
        );
        let resp = route_model_proxy(&store, data_dir.path(), req, "/aiui/api/claude/v1/messages")
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn missing_key_is_503_not_500() {
        let store = test_store().await;
        let token = store.create().await;
        // Deliberately no data_dir/secrets/claude-api-key written.
        let data_dir = tempfile::tempdir().unwrap();
        let req = req_with_cookie("POST", "/aiui/api/claude/v1/messages", Some(&token));
        let resp = route_model_proxy(&store, data_dir.path(), req, "/aiui/api/claude/v1/messages")
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    fn req_with_cookie_and_body(
        method: &str,
        path: &str,
        cookie: Option<&str>,
        body: &'static str,
    ) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(c) = cookie {
            builder = builder.header("cookie", format!("session={c}"));
        }
        builder.body(Body::from(body)).unwrap()
    }

    async fn store_and_keyed_dir() -> (SessionStore, tempfile::TempDir) {
        let store = test_store().await;
        let data_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(data_dir.path().join("secrets")).unwrap();
        std::fs::write(
            data_dir.path().join("secrets/claude-api-key"),
            "sk-ant-test-KEYVALUE-should-never-leak",
        )
        .unwrap();
        (store, data_dir)
    }

    #[tokio::test]
    async fn claude_body_carrying_node_secret_is_blocked() {
        let (store, data_dir) = store_and_keyed_dir().await;
        let token = store.create().await;
        let req = req_with_cookie_and_body(
            "POST",
            "/aiui/api/claude/v1/messages",
            Some(&token),
            r#"{"messages":[{"role":"user","content":"remember this: sk-ant-test-KEYVALUE-should-never-leak"}]}"#,
        );
        let resp = route_model_proxy(&store, data_dir.path(), req, "/aiui/api/claude/v1/messages")
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn claude_body_carrying_bip39_is_blocked() {
        let (store, data_dir) = store_and_keyed_dir().await;
        let token = store.create().await;
        // The canonical checksum-valid test mnemonic.
        let req = req_with_cookie_and_body(
            "POST",
            "/aiui/api/claude/v1/messages",
            Some(&token),
            r#"{"messages":[{"role":"user","content":"abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"}]}"#,
        );
        let resp = route_model_proxy(&store, data_dir.path(), req, "/aiui/api/claude/v1/messages")
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn web_search_query_carrying_bip39_is_blocked() {
        let (store, data_dir) = store_and_keyed_dir().await;
        let token = store.create().await;
        let req = req_with_cookie(
            "GET",
            "/aiui/api/web-search?q=abandon+abandon+abandon+abandon+abandon+abandon+abandon+abandon+abandon+abandon+abandon+about",
            Some(&token),
        );
        let resp = route_model_proxy(&store, data_dir.path(), req, "/aiui/api/web-search")
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// Minimal local capture server (hyper 0.14, same crate `server.rs`
    /// already builds on) standing in for an upstream — records the headers
    /// of the one request it receives so the test can assert what actually
    /// left the node, without adding a mocking dependency.
    async fn spawn_capture_server() -> (String, Arc<TokioMutex<Option<HeaderMap>>>) {
        let captured: Arc<TokioMutex<Option<HeaderMap>>> = Arc::new(TokioMutex::new(None));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let captured_clone = captured.clone();
        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let captured = captured_clone.clone();
                let service = hyper::service::service_fn(move |req: Request<Body>| {
                    let captured = captured.clone();
                    async move {
                        *captured.lock().await = Some(req.headers().clone());
                        Ok::<_, std::convert::Infallible>(Response::new(Body::from("{}")))
                    }
                });
                let _ = hyper::server::conn::Http::new()
                    .serve_connection(stream, service)
                    .await;
            }
        });
        (format!("http://{addr}/"), captured)
    }

    #[tokio::test]
    async fn inbound_authorization_header_is_not_forwarded() {
        let (upstream, captured) = spawn_capture_server().await;
        let req = Request::builder()
            .method("POST")
            .uri("/v1/messages")
            .header("authorization", "Bearer caller-supplied-secret")
            .header("x-api-key", "attacker-supplied-key")
            .header("cookie", "session=some-session-token")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let resp = forward(req, "v1/messages", &upstream, "test-upstream", &[])
            .await
            .unwrap();
        assert!(resp.status().is_success());

        // Give the spawned capture task a moment to record the request.
        for _ in 0..20 {
            if captured.lock().await.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let headers = captured
            .lock()
            .await
            .clone()
            .expect("capture server did not receive a request");
        assert!(headers.get("authorization").is_none());
        assert!(headers.get("x-api-key").is_none());
        assert!(headers.get("cookie").is_none());
        // The one header we DO expect to survive the round trip.
        assert_eq!(
            headers.get("content-type").and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
    }
}
