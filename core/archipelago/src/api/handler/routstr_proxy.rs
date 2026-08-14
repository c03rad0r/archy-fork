//! Session-gated forwarder for `/aiui/api/routstr/*` — the explicit,
//! user-selected Routstr path (as opposed to `assistant/backends/routstr.rs`,
//! which is the D-04 fallback leg the operator never chooses directly).
//!
//! AIUI's model picker lists Routstr as a first-class provider; selecting one
//! of its models routes chat completions through here. Same discipline as
//! `model_proxy.rs`: auth is re-derived from the request's own session cookie
//! (never trusted to nginx), inbound `authorization`/`cookie` headers are
//! never forwarded, and every outbound body is egress-screened (S3) before it
//! leaves the node.
//!
//! Payment is Cashu, D-05-gated end to end: a request is refused unless the
//! operator has set a non-zero Routstr allowance (Settings → System), the
//! quoted price fits the remaining allowance, and `auto_pay_token` (the ONE
//! budget-capped payment primitive, T-13-89) agrees to build the token. The
//! provider's change (`X-Cashu` / `X-Cashu-Refund` response headers, per
//! docs.routstr.com) is redeemed back into the node wallet and only the net
//! is recorded against the allowance.
//!
//! Upstream is the public Routstr aggregator instance routstr.com itself
//! ships against (verified live 2026-08-14: `/v1/models` serves the full
//! catalog with `sats_pricing`; the canonical `api.routstr.com` host 404s).
//! Making the instance operator-configurable — or sourcing it from the Nostr
//! provider announcements once those carry real endpoint/pricing content —
//! is the planned follow-up, not this file's job.

use super::ApiHandler;
use crate::session::SessionStore;
use anyhow::Result;
use hyper::{Body, Method, Request, Response, StatusCode};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Mutex as StdMutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use super::model_proxy::{
    bad_gateway, blocked_secret_shaped, forward_screen, is_authenticated, unauthorized,
};

/// The live public Routstr aggregator (the same instance routstr.com's own
/// frontend queries for `/v1/providers` and `/v1/models`).
const ROUTSTR_INSTANCE: &str = "https://routstr.otrta.me";
/// Generation cap forced onto every forwarded completion — never unbounded
/// (T-13-88), and the completion half of the price quote is arithmetic over
/// exactly this figure.
const MAX_COMPLETION_TOKENS: u64 = 1024;
/// Same round-trip ceiling as `model_proxy.rs`'s Claude/Ollama forwarders.
const FORWARD_TIMEOUT_SECS: u64 = 180;
/// Models-catalog cache TTL — mirrors `backends/routstr.rs`'s provider
/// discovery TTL. The catalog prices every chat request, so it cannot be
/// fetched per-message without doubling latency.
const MODELS_CACHE_TTL: Duration = Duration::from_secs(300);

/// One model's sats-denominated pricing, parsed from the aggregator's
/// `/v1/models` entries (`sats_pricing`). Rates are sats PER TOKEN (fractional
/// floats); `request` is a flat per-request fee in sats. Untrusted input — a
/// missing/garbled field parses as 0.0 and simply prices low, which the
/// remaining-allowance ceiling still caps.
#[derive(Debug, Clone, Default, serde::Deserialize)]
struct SatsPricing {
    #[serde(default)]
    prompt: f64,
    #[serde(default)]
    completion: f64,
    #[serde(default)]
    request: f64,
}

/// Quote a price in whole sats for one completion call: flat request fee +
/// prompt rate × (payload chars / 4, the usual chars-per-token rule of thumb)
/// + completion rate × the forced `MAX_COMPLETION_TOKENS` cap, +20% margin,
/// rounded up, never below 1. Deliberately a pure function so the arithmetic
/// is unit-testable; deliberately conservative because the provider's change
/// comes back as a Cashu refund and is redeemed — overquoting costs nothing
/// but float, underquoting gets the request rejected upstream.
fn estimate_price_sats(pricing: &SatsPricing, prompt_chars: usize, completion_tokens: u64) -> u64 {
    let prompt_tokens = (prompt_chars as f64) / 4.0;
    let raw = pricing.request
        + pricing.prompt * prompt_tokens
        + pricing.completion * (completion_tokens as f64);
    let with_margin = raw * 1.2;
    (with_margin.ceil() as u64).max(1)
}

/// Process-lifetime cache of the aggregator's models catalog (same pattern as
/// `backends/routstr.rs`'s `PROVIDER_CACHE`).
static MODELS_CACHE: OnceLock<StdMutex<Option<(Instant, Value)>>> = OnceLock::new();

fn cached_models() -> Option<Value> {
    let cache = MODELS_CACHE.get_or_init(|| StdMutex::new(None));
    let guard = cache.lock().expect("routstr models cache poisoned");
    guard.as_ref().and_then(|(at, models)| {
        if at.elapsed() < MODELS_CACHE_TTL {
            Some(models.clone())
        } else {
            None
        }
    })
}

fn set_cached_models(models: Value) {
    let cache = MODELS_CACHE.get_or_init(|| StdMutex::new(None));
    *cache.lock().expect("routstr models cache poisoned") = Some((Instant::now(), models));
}

/// Fetch (or serve cached) the aggregator's `/v1/models` catalog.
async fn fetch_models() -> Result<Value> {
    if let Some(cached) = cached_models() {
        return Ok(cached);
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?;
    let url = format!("{ROUTSTR_INSTANCE}/v1/models");
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("routstr models upstream returned HTTP {}", resp.status());
    }
    let models: Value = resp.json().await?;
    set_cached_models(models.clone());
    Ok(models)
}

/// Find one model's `sats_pricing` in the catalog by exact id.
fn pricing_for_model(models: &Value, model_id: &str) -> Option<SatsPricing> {
    models
        .get("data")?
        .as_array()?
        .iter()
        .find(|m| m.get("id").and_then(|v| v.as_str()) == Some(model_id))
        .and_then(|m| m.get("sats_pricing"))
        .and_then(|sp| serde_json::from_value(sp.clone()).ok())
}

fn json_response(status: StatusCode, body: Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap_or_default()))
        .unwrap_or_else(|_| Response::new(Body::from("{}")))
}

/// Plain-language refusal naming the UI path that fixes it — never a bare
/// status (RULE: every action needs a UI path).
fn budget_refusal(msg: String) -> Response<Body> {
    json_response(
        StatusCode::SERVICE_UNAVAILABLE,
        json!({ "error": { "message": msg } }),
    )
}

impl ApiHandler {
    /// Entry point wired into the `/aiui/api/routstr/` arm in `mod.rs` —
    /// thin, like `handle_model_proxy`, so the routing/budget logic below is
    /// testable without a full `ApiHandler`.
    pub(super) async fn handle_routstr_proxy(
        &self,
        req: Request<Body>,
        path: &str,
    ) -> Result<Response<Body>> {
        route_routstr_proxy(&self.session_store, &self.config.data_dir, req, path).await
    }
}

async fn route_routstr_proxy(
    session_store: &SessionStore,
    data_dir: &Path,
    req: Request<Body>,
    path: &str,
) -> Result<Response<Body>> {
    if !is_authenticated(session_store, req.headers()).await {
        tracing::warn!("401 routstr proxy {} — session invalid or missing", path);
        return Ok(unauthorized());
    }
    match path.strip_prefix("/aiui/api/routstr/") {
        Some("models") if req.method() == Method::GET => forward_models().await,
        Some("chat/completions") if req.method() == Method::POST => {
            forward_chat(req, data_dir).await
        }
        _ => Ok(unauthorized()),
    }
}

/// GET /aiui/api/routstr/models — the full catalog, passed through so AIUI
/// can render ids/names and show sats pricing. Read-only and unpaid.
async fn forward_models() -> Result<Response<Body>> {
    match fetch_models().await {
        Ok(models) => Ok(json_response(StatusCode::OK, models)),
        Err(e) => {
            tracing::warn!("routstr proxy: models upstream failed: {}", e);
            Ok(bad_gateway("Routstr model catalog is unreachable"))
        }
    }
}

/// POST /aiui/api/routstr/chat/completions — one paid, non-streaming,
/// OpenAI-shaped completion. Order matters: screen (S3) → budget gate (D-05,
/// offline) → price quote → pay → forward → redeem change → record net.
async fn forward_chat(req: Request<Body>, data_dir: &Path) -> Result<Response<Body>> {
    let payload = hyper::body::to_bytes(req.into_body())
        .await
        .map_err(|e| anyhow::anyhow!("read request payload: {e}"))?;
    let payload_str = String::from_utf8_lossy(&payload).to_string();

    // S3: the standalone frontend posts full history straight here with no
    // assistant loop (and no egress screen) behind it.
    if let Some(kind) = forward_screen(&payload_str, data_dir).await {
        tracing::error!(
            kind,
            "routstr proxy: blocked chat forward — secret-shaped content"
        );
        return Ok(blocked_secret_shaped());
    }

    let mut body: Value = match serde_json::from_str(&payload_str) {
        Ok(v) => v,
        Err(_) => {
            return Ok(json_response(
                StatusCode::BAD_REQUEST,
                json!({ "error": { "message": "request body is not valid JSON" } }),
            ));
        }
    };
    let Some(model_id) = body.get("model").and_then(|v| v.as_str()).map(String::from) else {
        return Ok(json_response(
            StatusCode::BAD_REQUEST,
            json!({ "error": { "message": "request is missing a model id" } }),
        ));
    };

    // D-05 gate, checked before any network I/O: a zero allowance means
    // Routstr is refused outright, with the UI path that arms it.
    let mut budget = crate::assistant::AssistantBudget::load(data_dir).await;
    if budget.allowance_sats == 0 {
        return Ok(budget_refusal(
            "Routstr is disabled on this node — set a sats budget in Settings → System → \
             Routstr AI budget to enable it."
                .to_string(),
        ));
    }
    let remaining = budget.remaining_sats();
    if remaining == 0 {
        return Ok(budget_refusal(format!(
            "This period's Routstr budget is spent ({} of {} sats). Raise the allowance in \
             Settings → System → Routstr AI budget to continue.",
            budget.spent_sats, budget.allowance_sats
        )));
    }

    // Price the request from the catalog. An unknown model is a caller bug
    // (the dropdown only offers catalog models), not a reason to guess a
    // price.
    let models = match fetch_models().await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("routstr proxy: cannot price request, models fetch failed: {e}");
            return Ok(bad_gateway(
                "Routstr model catalog is unreachable — cannot price this request",
            ));
        }
    };
    let Some(pricing) = pricing_for_model(&models, &model_id) else {
        return Ok(json_response(
            StatusCode::BAD_REQUEST,
            json!({ "error": { "message": format!("unknown Routstr model: {model_id}") } }),
        ));
    };

    // Force the shape this forwarder actually supports: non-streaming, with
    // an explicit, capped generation limit (T-13-88).
    let max_tokens = body
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(MAX_COMPLETION_TOKENS)
        .min(MAX_COMPLETION_TOKENS);
    body["stream"] = json!(false);
    body["max_tokens"] = json!(max_tokens);

    let price_sats = estimate_price_sats(&pricing, payload_str.len(), max_tokens);
    if price_sats > remaining {
        return Ok(budget_refusal(format!(
            "This request quotes ~{price_sats} sats but only {remaining} sats remain in this \
             period's Routstr budget (Settings → System → Routstr AI budget)."
        )));
    }

    // Pay via the ONE budget-capped primitive (T-13-89) — same call, same
    // mint list as the fallback leg in backends/routstr.rs.
    let accepted_mints = crate::wallet::ecash::load_accepted_mints(data_dir)
        .await
        .map(|m| m.mints)
        .unwrap_or_default();
    let token = match crate::swarm::payment::auto_pay_token(
        data_dir,
        &budget.payment_policy(),
        &accepted_mints,
        price_sats,
    )
    .await?
    {
        Some(t) => t,
        None => {
            return Ok(budget_refusal(format!(
                "The node wallet could not fund this request (~{price_sats} sats) — check the \
                 ecash balance and accepted mints in Settings → Wallet."
            )));
        }
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(FORWARD_TIMEOUT_SECS))
        .build()?;
    let url = format!("{ROUTSTR_INSTANCE}/v1/chat/completions");
    let resp = match client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            // The token never reached the provider — reclaim it into our own
            // wallet so the sats aren't stranded, and record nothing.
            match crate::wallet::ecash::receive_token(data_dir, &token).await {
                Ok(_) => tracing::info!(
                    "routstr proxy: upstream send failed ({e}); unsent payment token reclaimed"
                ),
                Err(re) => tracing::warn!(
                    "routstr proxy: upstream send failed ({e}) AND reclaiming the unsent token \
                     failed ({re}) — {price_sats} sats may be stranded in the token"
                ),
            }
            return Ok(bad_gateway("Routstr provider is unreachable"));
        }
    };

    let status = resp.status();
    // Change comes back as a Cashu token header (docs.routstr.com names both
    // spellings across versions); redeem it so only the net leaves the
    // allowance.
    let refund_token = ["x-cashu-refund", "x-cashu"]
        .iter()
        .find_map(|h| resp.headers().get(*h))
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let resp_body = resp.bytes().await.unwrap_or_default();

    let mut reclaimed = 0u64;
    if let Some(refund) = refund_token {
        match crate::wallet::ecash::receive_token(data_dir, &refund).await {
            Ok(sats) => reclaimed = sats,
            Err(e) => tracing::warn!("routstr proxy: redeeming the change token failed: {e}"),
        }
    } else if !status.is_success() {
        // The provider refused the request (e.g. "mint unreachable") and
        // sent no change — if it never actually redeemed our token, the
        // proofs are still ours to take back. If it DID redeem and then
        // failed, this reclaim fails harmlessly and the spend stands.
        match crate::wallet::ecash::receive_token(data_dir, &token).await {
            Ok(sats) => {
                reclaimed = sats;
                tracing::info!(
                    "routstr proxy: upstream refused (HTTP {status}); unredeemed payment token \
                     reclaimed ({sats} sats)"
                );
            }
            Err(e) => tracing::warn!(
                "routstr proxy: upstream refused (HTTP {status}) and the payment token could \
                 not be reclaimed ({e}) — treating the {price_sats} sats as spent"
            ),
        }
    }
    let net_sats = price_sats.saturating_sub(reclaimed);
    if net_sats > 0 {
        if let Err(e) = budget.record_spend(data_dir, net_sats).await {
            tracing::warn!(
                error = %e,
                "routstr proxy: failed to persist the budget spend (the payment itself already happened)"
            );
        }
    }
    tracing::info!(
        model = %model_id,
        quoted = price_sats,
        reclaimed,
        net = net_sats,
        status = %status,
        "routstr proxy: forwarded paid chat completion"
    );

    Ok(Response::builder()
        .status(status.as_u16())
        .header("Content-Type", "application/json")
        .body(Body::from(resp_body))
        .unwrap_or_else(|_| Response::new(Body::from("{}"))))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_store() -> SessionStore {
        let path = std::env::temp_dir().join(format!(
            "archy-routstr-proxy-test-sessions-{}.json",
            rand::RngCore::next_u64(&mut rand::rngs::OsRng)
        ));
        SessionStore::new_for_tests(path)
    }

    fn req(method: &str, path: &str, cookie: Option<&str>, body: &'static str) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(c) = cookie {
            builder = builder.header("cookie", format!("session={c}"));
        }
        builder.body(Body::from(body)).unwrap()
    }

    #[tokio::test]
    async fn models_without_session_is_401() {
        let store = test_store().await;
        let data_dir = tempfile::tempdir().unwrap();
        let r = req("GET", "/aiui/api/routstr/models", None, "");
        let resp = route_routstr_proxy(&store, data_dir.path(), r, "/aiui/api/routstr/models")
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn chat_without_session_is_401() {
        let store = test_store().await;
        let data_dir = tempfile::tempdir().unwrap();
        let r = req("POST", "/aiui/api/routstr/chat/completions", None, "{}");
        let resp = route_routstr_proxy(
            &store,
            data_dir.path(),
            r,
            "/aiui/api/routstr/chat/completions",
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// D-05: a fresh node (no budget file → zero allowance) refuses the paid
    /// path BEFORE any pricing/network I/O — this test runs fully offline.
    #[tokio::test]
    async fn chat_with_zero_allowance_is_refused_offline() {
        let store = test_store().await;
        let token = store.create().await;
        let data_dir = tempfile::tempdir().unwrap();
        let r = req(
            "POST",
            "/aiui/api/routstr/chat/completions",
            Some(&token),
            r#"{"model":"some-model","messages":[{"role":"user","content":"hi"}]}"#,
        );
        let resp = route_routstr_proxy(
            &store,
            data_dir.path(),
            r,
            "/aiui/api/routstr/chat/completions",
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        let msg = v["error"]["message"].as_str().unwrap();
        assert!(msg.contains("Settings"), "refusal must name the UI path");
    }

    #[tokio::test]
    async fn chat_body_carrying_bip39_is_blocked() {
        let store = test_store().await;
        let token = store.create().await;
        let data_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(data_dir.path().join("secrets")).unwrap();
        let r = req(
            "POST",
            "/aiui/api/routstr/chat/completions",
            Some(&token),
            r#"{"model":"m","messages":[{"role":"user","content":"abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"}]}"#,
        );
        let resp = route_routstr_proxy(
            &store,
            data_dir.path(),
            r,
            "/aiui/api/routstr/chat/completions",
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn price_estimate_is_conservative_and_never_zero() {
        // A free/garbled pricing entry still quotes at least 1 sat.
        assert_eq!(estimate_price_sats(&SatsPricing::default(), 100, 1024), 1);

        // Live-catalog-shaped numbers (deepseek-v4-flash, 2026-08-14):
        // request 0.001, prompt ~0.000178/tok, completion ~0.000267/tok.
        let p = SatsPricing {
            prompt: 0.000178,
            completion: 0.000267,
            request: 0.001,
        };
        let quote = estimate_price_sats(&p, 4000, 1024);
        // ~0.18 + ~0.27 + flat, with margin → rounds up to 1 sat.
        assert_eq!(quote, 1);

        // A pricier model scales with the prompt.
        let expensive = SatsPricing {
            prompt: 0.05,
            completion: 0.1,
            request: 1.0,
        };
        let quote = estimate_price_sats(&expensive, 40_000, 1024);
        assert!(quote >= 600, "quote {quote} should reflect real rates");
    }

    #[test]
    fn pricing_lookup_finds_exact_model_id() {
        let models = json!({ "data": [
            { "id": "a-model", "sats_pricing": { "prompt": 0.1, "completion": 0.2, "request": 1.0 } },
            { "id": "other", "sats_pricing": { "prompt": 0.3 } }
        ]});
        let p = pricing_for_model(&models, "a-model").unwrap();
        assert_eq!(p.request, 1.0);
        assert!(pricing_for_model(&models, "missing").is_none());
    }
}
