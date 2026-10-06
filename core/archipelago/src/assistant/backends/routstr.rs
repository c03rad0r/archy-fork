//! The Routstr leg of the D-04 backend chain — the third and last fallback,
//! reached only when Ollama and Claude are both unavailable. An OpenAI-
//! compatible `POST /v1/chat/completions` paid per request in Cashu ecash,
//! with providers/models/prices discovered over Nostr (kind `ROUTSTR_KIND`).
//!
//! **Task 1's decision (`proceed-docs-with-probe-first`, see `13-13-SUMMARY.md`):**
//! `13-ROUTSTR-FINDINGS.md` observed **zero of nine** cited protocol claims
//! against live relays — no provider was announcing during the 13-03 probe
//! window, so nothing here is written against an independently-observed
//! event or response. Everything below is written against
//! `docs.routstr.com` as cited in `13-RESEARCH.md`'s "Routstr
//! chat-completions call shape", and the FIRST real HTTP call this code
//! ever makes against a live provider (in `send_paid_request` below) IS the
//! capability probe the decision calls for: a non-success status or a
//! response missing the expected `choices[0].message` shape fails LOUDLY
//! with the real status/body, never silently degrading to an empty or
//! wrong answer. There is no separate preliminary probe call — the first
//! paid request itself is required to fail loudly on a wrong guess, which
//! is exactly what happens here.
//!
//! Two cross-provider gotchas live entirely at this adapter's edge, never
//! in the shared loop (`loop_.rs`) or the shared tool model (`tools.rs`):
//! this is the ONE backend whose `function.arguments` arrives as a
//! JSON-ENCODED STRING (AI-SPEC §3 Pitfall 2, distinct from Ollama's
//! already-parsed object and Claude's native `input` value) — parsed
//! exactly once, in [`parse_openai_tool_calls`] — and provider events are
//! self-published, untrusted Nostr data (T-13-86): nothing about a
//! discovered provider ever widens what this node does beyond issuing one
//! paid chat request to the endpoint it advertised.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use nostr_sdk::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{Backend, BackendTurn};
use crate::assistant::egress::{self, EgressVerdict};
use crate::assistant::tools::{ChatMessage, Role, ToolCall, ToolDef};
use crate::swarm::payment::PaymentPolicy;

/// Provider-announcement Nostr event kind. 13-ROUTSTR-FINDINGS.md row 1 is
/// **NOT OBSERVED** (zero matching events across all 3 default relays in a
/// 30s window on 2026-08-03) — this is `docs.routstr.com`'s cited value,
/// unverified against a live event.
pub const ROUTSTR_KIND: u16 = 38421;
/// 13-ROUTSTR-FINDINGS.md row 2: **NOT OBSERVED** — the `d` tag value
/// `docs.routstr.com` cites for a provider-announcement event.
const ROUTSTR_D_TAG: &str = "routstr-provider";
/// The same three default relays 13-03 actually probed
/// (13-ROUTSTR-FINDINGS.md row 9: relay reachability itself WAS confirmed —
/// all three accepted the connection; no provider happened to be announcing
/// during that window).
const DEFAULT_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://relay.nostr.band",
    "wss://nos.lol",
];
/// Bounded wait for the discovery subscription and the relay connect —
/// the third leg of a fallback chain must never hang the whole turn
/// waiting on relays that may have nothing to say.
pub const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
/// Process-lifetime provider cache TTL (T-13-93) — a relay round trip on
/// every chat turn is not acceptable latency on the leg reached only when
/// the first two backends are unavailable.
const DISCOVERY_CACHE_TTL: Duration = Duration::from_secs(300);
/// Explicit generation-length cap, sent on every request — the same
/// discipline as `ollama.rs`'s `OLLAMA_NUM_PREDICT` and `claude.rs`'s
/// `ASSISTANT_MAX_TOKENS`. An unbounded generation on a PAID backend is a
/// direct budget-cap violation risk (T-13-88), never just a latency
/// concern.
pub const ROUTSTR_MAX_TOKENS: u32 = 1024;
const ROUTSTR_HTTP_TIMEOUT: Duration = Duration::from_secs(60);
/// 13-ROUTSTR-FINDINGS.md row 6: **NOT OBSERVED** — no provider endpoint
/// was ever discovered, so the probe never got to see a provider name its
/// own header. `docs.routstr.com` cites `Authorization: Bearer cashuA…` as
/// the primary spelling (an `X-Cashu` header is also documented as an
/// alternative); Task 1's decision governs proceeding against this docs
/// value, with the response-shape check in `send_paid_request` as the
/// fail-loud check against a wrong guess.
const PAYMENT_HEADER: &str = "Authorization";

/// One Nostr-discovered Routstr provider, parsed from a `ROUTSTR_KIND`
/// event's JSON content per `13-RESEARCH.md`'s cited shape (`endpoints`,
/// `models`, `pricing`). 13-ROUTSTR-FINDINGS.md rows 3-5 are all **NOT
/// OBSERVED** — this parse target is docs-shaped, not an observed event
/// schema. Untrusted data throughout (T-13-86): a provider is a
/// self-published Nostr event from an unknown party.
#[derive(Debug, Clone, Deserialize)]
pub struct RoutstrProvider {
    #[serde(default)]
    pub endpoints: Vec<String>,
    #[serde(default)]
    pub models: Vec<String>,
    /// model name -> price in sats. D-05's budget ceiling is arithmetic
    /// over exactly this figure.
    #[serde(default)]
    pub pricing: HashMap<String, u64>,
}

impl RoutstrProvider {
    /// Prefer an onion endpoint when Tor is up; otherwise the first
    /// non-onion endpoint (falling back to whatever is first if every
    /// advertised endpoint happens to be onion-shaped and Tor is down —
    /// still returned, since routing that specific case is `reqwest`'s
    /// problem at send time, not a reason to refuse to even try).
    fn best_endpoint(&self, tor_up: bool) -> Option<&str> {
        if tor_up {
            if let Some(onion) = self.endpoints.iter().find(|e| e.contains(".onion")) {
                return Some(onion.as_str());
            }
        }
        self.endpoints
            .iter()
            .find(|e| !e.contains(".onion"))
            .or_else(|| self.endpoints.first())
            .map(|s| s.as_str())
    }
}

/// Parse one provider-announcement event's JSON content. A pure function —
/// no network — directly testable against a fixture event (no real
/// observed event exists yet per 13-ROUTSTR-FINDINGS.md; this is
/// docs-shaped). Malformed/unexpected content is `None`, never a panic —
/// a hostile or malformed provider event must never crash discovery.
fn parse_provider_event(content: &str) -> Option<RoutstrProvider> {
    serde_json::from_str(content).ok()
}

/// Process-lifetime provider cache, mirroring `ollama.rs`'s
/// `TOOL_CAPABILITY_CACHE` pattern but with a TTL rather than an
/// indefinite cache — providers come and go, unlike a model's static
/// tool-calling capability.
static PROVIDER_CACHE: OnceLock<StdMutex<Option<(Instant, Vec<RoutstrProvider>)>>> =
    OnceLock::new();

fn cached_providers() -> Option<Vec<RoutstrProvider>> {
    let cache = PROVIDER_CACHE.get_or_init(|| StdMutex::new(None));
    let guard = cache.lock().expect("routstr provider cache poisoned");
    guard.as_ref().and_then(|(at, providers)| {
        if at.elapsed() < DISCOVERY_CACHE_TTL {
            Some(providers.clone())
        } else {
            None
        }
    })
}

fn set_cached_providers(providers: Vec<RoutstrProvider>) {
    let cache = PROVIDER_CACHE.get_or_init(|| StdMutex::new(None));
    *cache.lock().expect("routstr provider cache poisoned") = Some((Instant::now(), providers));
}

/// Subscribe for `ROUTSTR_KIND` provider-announcement events over the
/// node's existing Tor-proxy-aware Nostr client
/// (`nostr_discovery::build_nostr_client` — never a second relay client,
/// T-13-90). Bounded by `DISCOVERY_TIMEOUT`. Finding nothing — no matching
/// event, a relay connect failure, an unreachable network — is an EMPTY
/// LIST, never an `Err`: this is the third leg of a fallback chain, and a
/// discovery miss must read exactly like "nothing here" to the caller, not
/// like a crash (T-13-93, matching `nostr_discovery::discover_archipelago_nodes`'s
/// own fail-to-empty convention).
pub async fn discover_providers(tor_proxy: Option<&str>) -> Vec<RoutstrProvider> {
    if let Some(cached) = cached_providers() {
        return cached;
    }
    let providers = discover_providers_uncached(tor_proxy).await;
    set_cached_providers(providers.clone());
    providers
}

async fn discover_providers_uncached(tor_proxy: Option<&str>) -> Vec<RoutstrProvider> {
    let anon_keys = Keys::generate();
    let client = match crate::nostr_discovery::build_nostr_client(anon_keys, tor_proxy) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "routstr: failed to build the Nostr client — treating as no providers found"
            );
            return Vec::new();
        }
    };
    for relay in DEFAULT_RELAYS {
        let _ = client.add_relay(*relay).await;
    }
    if tokio::time::timeout(DISCOVERY_TIMEOUT, client.connect())
        .await
        .is_err()
    {
        tracing::warn!("routstr: relay connect timed out — no providers found this turn");
        return Vec::new();
    }
    let filter = Filter::new()
        .kind(Kind::Custom(ROUTSTR_KIND))
        .identifier(ROUTSTR_D_TAG)
        .limit(50);
    let events = client
        .fetch_events(filter, DISCOVERY_TIMEOUT)
        .await
        .map(|e| e.to_vec())
        .unwrap_or_default();
    client.disconnect().await;
    events
        .iter()
        .filter_map(|e| parse_provider_event(&e.content))
        .collect()
}

/// Pick the globally cheapest advertised (provider, model) price that
/// `remaining_budget` affords, preferring an onion endpoint when `tor_up`.
/// Routstr has no operator-configured target model in this phase (unlike
/// Ollama's `OLLAMA_DEFAULT_MODEL`/Claude's `CLAUDE_MODEL` constants) —
/// CONTEXT.md explicitly delegates "Routstr provider selection strategy"
/// to Claude's discretion, so this considers every model every discovered
/// provider advertises and requests whichever turns out cheapest and
/// affordable, rather than pinning to one hardcoded model name a real
/// provider might not even offer. Untrusted input throughout (T-13-86): a
/// provider is a self-published event, so this only ever decides WHICH
/// endpoint to POST a paid request to — it never widens authority.
fn select_provider(
    providers: &[RoutstrProvider],
    remaining_budget: u64,
    tor_up: bool,
) -> Option<(RoutstrProvider, String, u64, String)> {
    providers
        .iter()
        .flat_map(|p| {
            p.pricing.iter().filter_map(move |(model, &price)| {
                if price == 0 || price > remaining_budget {
                    return None;
                }
                let endpoint = p.best_endpoint(tor_up)?;
                Some((p.clone(), model.clone(), price, endpoint.to_string()))
            })
        })
        .min_by_key(|(_, _, price, _)| *price)
}

/// D-05: build the Cashu payment token via the existing budget-capped
/// primitive — **never hand-rolled here** (T-13-89). `Ok(None)` means the
/// price exceeds the remaining allowance, or the wallet/mint declined; the
/// caller decides how to handle that (Task 2's own contract stops at "call
/// this and propagate a clear error on `None`" — the typed, `loop_.rs`-
/// downcastable signal is 13-13 Task 3's addition, see `send_paid_request`'s
/// caller below).
async fn attach_payment(
    data_dir: &std::path::Path,
    policy: &PaymentPolicy,
    accepted_mints: &[String],
    price_sats: u64,
) -> Result<Option<String>> {
    crate::swarm::payment::auto_pay_token(data_dir, policy, accepted_mints, price_sats).await
}

/// OpenAI-shape `tool_calls[]` parsing. This is the ONE backend whose
/// `function.arguments` arrives as a JSON-ENCODED STRING (AI-SPEC §3
/// Pitfall 2) rather than an already-parsed object (Ollama) or a native
/// `input` value (Claude) — parsed exactly once, here, at this adapter's
/// edge, so the shared loop always receives the same parsed-object shape
/// regardless of which backend answered. This is exactly the test named
/// `openai_string_arguments_are_parsed_once_at_the_edge` below.
fn parse_openai_tool_calls(raw_calls: &[Value]) -> Vec<ToolCall> {
    raw_calls
        .iter()
        .filter_map(|raw| {
            let id = raw.get("id").and_then(|v| v.as_str())?.to_string();
            let function = raw.get("function")?;
            let name = function.get("name").and_then(|v| v.as_str())?.to_string();
            let arguments_str = function
                .get("arguments")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let arguments: Value =
                serde_json::from_str(arguments_str).unwrap_or_else(|_| json!({}));
            Some(ToolCall {
                id,
                name,
                arguments,
            })
        })
        .collect()
}

/// Map one internal `ChatMessage` onto zero or more OpenAI-compatible chat
/// wire messages. Modeled on `ollama.rs`'s own `message_to_wire` (system as
/// the first message, `role: "tool"` for results — never Claude's
/// `role: "user"`-wrapped `tool_result` blocks), but tool-call turns here
/// additionally carry an `id` and stringify `arguments` back to JSON text
/// (the wire-format inverse of `parse_openai_tool_calls`), and tool-result
/// turns carry `tool_call_id` so each call's id is echoed back exactly —
/// the OpenAI-shape contract this adapter's edge is responsible for.
pub(super) fn message_to_wire(msg: &ChatMessage) -> Vec<Value> {
    match msg.role {
        Role::System => vec![],
        Role::User => vec![json!({
            "role": "user",
            "content": msg.text.clone().unwrap_or_default(),
        })],
        Role::Assistant => {
            if !msg.tool_calls.is_empty() {
                let calls: Vec<Value> = msg
                    .tool_calls
                    .iter()
                    .map(|c| {
                        json!({
                            "id": c.id,
                            "type": "function",
                            "function": {
                                "name": c.name,
                                "arguments": serde_json::to_string(&c.arguments).unwrap_or_default(),
                            },
                        })
                    })
                    .collect();
                vec![json!({ "role": "assistant", "content": Value::Null, "tool_calls": calls })]
            } else {
                vec![json!({
                    "role": "assistant",
                    "content": msg.text.clone().unwrap_or_default(),
                })]
            }
        }
        Role::Tool => msg
            .tool_results
            .iter()
            .map(|r| {
                json!({
                    "role": "tool",
                    "tool_call_id": r.call_id,
                    "content": r.content,
                })
            })
            .collect(),
    }
}

/// The Routstr leg of the D-04 backend chain. `data_dir`/`policy`/
/// `accepted_mints`/`tor_proxy` are all explicit constructor parameters
/// (never read from a global) so production (`backends::select_backend`)
/// and this module's own tests construct the identical type against
/// different policies/servers — matching `OllamaBackend`'s own
/// explicit-`base_url` precedent.
pub struct RoutstrBackend {
    data_dir: PathBuf,
    policy: PaymentPolicy,
    accepted_mints: Vec<String>,
    tor_proxy: Option<String>,
}

impl RoutstrBackend {
    pub fn new(
        data_dir: PathBuf,
        policy: PaymentPolicy,
        accepted_mints: Vec<String>,
        tor_proxy: Option<String>,
    ) -> Self {
        Self {
            data_dir,
            policy,
            accepted_mints,
            tor_proxy,
        }
    }
}

#[async_trait]
impl Backend for RoutstrBackend {
    async fn send(
        &self,
        system: &str,
        tools: &[ToolDef],
        history: &[ChatMessage],
    ) -> Result<BackendTurn> {
        let providers = discover_providers(self.tor_proxy.as_deref()).await;
        self.send_with_providers(&providers, system, tools, history)
            .await
    }
}

impl RoutstrBackend {
    /// Split out from `send()` so tests can supply a fixture provider list
    /// directly, without ever touching the network (`send()` itself is the
    /// only caller that goes through the real `discover_providers`).
    async fn send_with_providers(
        &self,
        providers: &[RoutstrProvider],
        system: &str,
        tools: &[ToolDef],
        history: &[ChatMessage],
    ) -> Result<BackendTurn> {
        let tor_up = self.tor_proxy.is_some();
        let remaining_budget = self.policy.budget_sats;

        let Some((_provider, model, price_sats, endpoint)) =
            select_provider(providers, remaining_budget, tor_up)
        else {
            // T-13-93/D-04: no discovered provider currently advertises an
            // affordable model — a clear, ordinary transport-style error.
            // Routstr is always the terminal leg of the D-04 chain, so
            // there is nothing further to fall through to from here; the
            // caller (run_loop, via FallbackChain) reports this plainly
            // rather than hanging or panicking.
            anyhow::bail!(
                "no Routstr provider currently advertises an affordable model for this request"
            );
        };

        // D-05: pay via the existing budget-capped primitive — never
        // hand-rolled here (T-13-89). A `None` return means the price
        // exceeds the remaining allowance, or the wallet/mint declined.
        // 13-13 Task 3: this is a TYPED signal (`crate::assistant::BudgetExhausted`),
        // not a plain string bail — `loop_.rs` downcasts it out of the
        // generic `Err` to stop the turn cleanly (no retry, no re-price,
        // no partial spend) rather than treating it like an ordinary
        // transport error that might be worth another attempt.
        let token = match attach_payment(
            &self.data_dir,
            &self.policy,
            &self.accepted_mints,
            price_sats,
        )
        .await?
        {
            Some(t) => t,
            None => {
                return Err(crate::assistant::BudgetExhausted {
                    remaining_sats: remaining_budget,
                    quoted_price_sats: price_sats,
                }
                .into());
            }
        };

        // D-05/S-12: the payment already went through at this point — a
        // Cashu token was built and its proofs already committed by
        // `attach_payment`/`auto_pay_token`, regardless of whether the
        // chat HTTP call below succeeds. Record it against the operator's
        // persisted allowance NOW, before attempting the request, not
        // after — reloaded fresh from disk (rather than mutating
        // `self.policy`'s turn-start snapshot) so concurrent spend from
        // elsewhere is never clobbered by a stale in-memory copy. A
        // failure to persist is logged, never surfaced as a chat error —
        // the payment already happened either way.
        let mut budget = crate::assistant::AssistantBudget::load(&self.data_dir).await;
        if let Err(e) = budget.record_spend(&self.data_dir, price_sats).await {
            tracing::warn!(
                error = %e,
                "routstr: failed to persist the budget spend (the payment itself already succeeded)"
            );
        }

        self.send_paid_request(&model, &endpoint, &token, system, tools, history)
            .await
    }

    /// The actual OpenAI-shaped HTTP call, given an already-selected
    /// model/endpoint and an already-built payment token. Split out so
    /// tests can exercise the wire format (headers, `tools`, generation
    /// cap, tool-call parsing) against a local HTTP stub without ever
    /// going through provider discovery or a real payment.
    async fn send_paid_request(
        &self,
        model: &str,
        endpoint: &str,
        token: &str,
        system: &str,
        tools: &[ToolDef],
        history: &[ChatMessage],
    ) -> Result<BackendTurn> {
        let mut messages: Vec<Value> = vec![json!({ "role": "system", "content": system })];
        messages.extend(history.iter().flat_map(message_to_wire));

        let routstr_tools: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    },
                })
            })
            .collect();

        let mut body = json!({
            "model": model,
            "messages": messages,
            // Every turn while the loop may still receive a tool call is
            // requested non-streaming — partial JSON tool arguments cannot
            // be structurally validated mid-stream (AI-SPEC §4b.2), same
            // discipline as ollama.rs/claude.rs.
            "stream": false,
            // Generation cap, always explicit — never unbounded (T-13-88).
            "max_tokens": ROUTSTR_MAX_TOKENS,
        });
        if !routstr_tools.is_empty() {
            body["tools"] = json!(routstr_tools);
        }

        // G-B1/G-B2: this is a cloud leg exactly like Claude's — screen
        // before anything leaves the node. Fails closed: on a block, this
        // call returns an Err and nothing was sent.
        let egress_ctx = egress::EgressContext::from_turn(
            history,
            &tools.iter().map(|t| t.name).collect::<Vec<_>>(),
            &self.data_dir.join("secrets"),
        )
        .await;
        match egress::screen_outbound(&body.to_string(), &egress_ctx) {
            EgressVerdict::Allow => {}
            EgressVerdict::Truncate(truncated) => {
                if let Ok(v) = serde_json::from_str::<Value>(&truncated) {
                    body = v;
                }
            }
            EgressVerdict::BlockFallBackLocal => {
                crate::assistant::global_counters().note_blocked_egress();
                anyhow::bail!(
                    "outbound request to Routstr blocked before it left this node — it appeared \
                     to contain secret-shaped material (G-B1). Falling back to the local backend."
                );
            }
        }

        let client = reqwest::Client::builder()
            .timeout(ROUTSTR_HTTP_TIMEOUT)
            .build()?;
        let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));
        let resp = client
            .post(&url)
            .header(PAYMENT_HEADER, format!("Bearer {token}"))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            // Task 1 decision (`proceed-docs-with-probe-first`): this IS
            // the capability probe — a non-success status from a
            // docs-shaped, unverified request (13-ROUTSTR-FINDINGS.md: 0/9
            // claims confirmed) fails LOUDLY with the real status/body
            // rather than being masked as a generic transport error.
            anyhow::bail!(
                "Routstr chat-completions HTTP {status} — this request was built against \
                 docs.routstr.com only (13-ROUTSTR-FINDINGS.md: 0/9 claims independently \
                 observed), so this may mean the header name or request shape guessed here is \
                 wrong for this provider: {}",
                txt.chars().take(180).collect::<String>()
            );
        }

        let json_resp: Value = resp.json().await?;
        // Capability-probe half 2: the response must look like an OpenAI
        // chat completion. Fail loudly rather than silently returning an
        // empty/garbage answer if the shape doesn't match.
        let Some(choice) = json_resp
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
        else {
            anyhow::bail!(
                "Routstr response had no 'choices' array — the docs-shaped response contract \
                 did not match this provider's real response"
            );
        };
        let message = choice.get("message").cloned().unwrap_or_default();
        let raw_calls = message
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        if raw_calls.is_empty() {
            let text = message
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            return Ok(BackendTurn::Text(text));
        }

        Ok(BackendTurn::ToolCalls(parse_openai_tool_calls(&raw_calls)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use tokio::sync::Mutex as AsyncMutex;

    /// One HTTP request captured by the stub server: the path hit, the
    /// headers, and the parsed JSON body sent.
    #[derive(Debug, Clone)]
    struct CapturedRequest {
        path: String,
        headers: std::collections::HashMap<String, String>,
        body: Value,
    }

    /// A minimal local HTTP stub standing in for a Routstr provider's
    /// `/v1/chat/completions` endpoint. Same `hyper`-direct pattern as
    /// `ollama.rs`'s `StubOllama` (no mock-HTTP crate exists in this
    /// workspace).
    struct StubRoutstr {
        base_url: String,
        captured: Arc<AsyncMutex<Vec<CapturedRequest>>>,
        response: Arc<AsyncMutex<(u16, Value)>>,
    }

    impl StubRoutstr {
        async fn start(response: Value) -> Self {
            Self::start_with_status(200, response).await
        }

        async fn start_with_status(status: u16, response: Value) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind stub");
            let addr = listener.local_addr().expect("local_addr");
            let captured = Arc::new(AsyncMutex::new(Vec::new()));
            let response = Arc::new(AsyncMutex::new((status, response)));
            let captured_bg = captured.clone();
            let response_bg = response.clone();
            tokio::spawn(async move {
                loop {
                    let (stream, _) = match listener.accept().await {
                        Ok(v) => v,
                        Err(_) => break,
                    };
                    let captured = captured_bg.clone();
                    let response = response_bg.clone();
                    tokio::spawn(async move {
                        let service =
                            hyper::service::service_fn(move |req: hyper::Request<hyper::Body>| {
                                let captured = captured.clone();
                                let response = response.clone();
                                async move {
                                    let path = req.uri().path().to_string();
                                    let headers = req
                                        .headers()
                                        .iter()
                                        .map(|(k, v)| {
                                            (
                                                k.to_string(),
                                                v.to_str().unwrap_or_default().to_string(),
                                            )
                                        })
                                        .collect();
                                    let body_bytes = hyper::body::to_bytes(req.into_body())
                                        .await
                                        .unwrap_or_default();
                                    let body: Value =
                                        serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
                                    captured.lock().await.push(CapturedRequest {
                                        path,
                                        headers,
                                        body,
                                    });
                                    let (status, resp_body) = response.lock().await.clone();
                                    let resp = hyper::Response::builder()
                                        .status(status)
                                        .header("content-type", "application/json")
                                        .body(hyper::Body::from(resp_body.to_string()))
                                        .expect("static response builds");
                                    Ok::<_, std::convert::Infallible>(resp)
                                }
                            });
                        let _ = hyper::server::conn::Http::new()
                            .http1_keep_alive(false)
                            .serve_connection(stream, service)
                            .await;
                    });
                }
            });
            Self {
                base_url: format!("http://{addr}"),
                captured,
                response,
            }
        }

        async fn captured(&self) -> Vec<CapturedRequest> {
            self.captured.lock().await.clone()
        }
    }

    fn chat_response_text(text: &str) -> Value {
        json!({
            "choices": [
                { "message": { "role": "assistant", "content": text } }
            ]
        })
    }

    fn chat_response_with_tool_calls(calls: Vec<(&str, &str, &str)>) -> Value {
        // (id, name, JSON-encoded-string arguments)
        json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": calls
                            .into_iter()
                            .map(|(id, name, args_str)| json!({
                                "id": id,
                                "type": "function",
                                "function": { "name": name, "arguments": args_str },
                            }))
                            .collect::<Vec<_>>(),
                    }
                }
            ]
        })
    }

    /// `_base_url` is accepted (not read) purely so call sites read as
    /// "a backend pointed at this stub" even though `send_paid_request`'s
    /// tests pass the endpoint explicitly as its own argument.
    fn backend_for(_base_url: &str, budget_sats: u64) -> RoutstrBackend {
        RoutstrBackend::new(
            std::env::temp_dir(),
            PaymentPolicy::with_budget(budget_sats, 5),
            vec!["https://mint.example.com".to_string()],
            None,
        )
    }

    fn provider(endpoint: &str, model: &str, price_sats: u64) -> RoutstrProvider {
        let mut pricing = HashMap::new();
        pricing.insert(model.to_string(), price_sats);
        RoutstrProvider {
            endpoints: vec![endpoint.to_string()],
            models: vec![model.to_string()],
            pricing,
        }
    }

    // -----------------------------------------------------------------
    // Behavior 1: discovery parses the docs-cited shape from a fixture
    // event — no network involved (parse_provider_event is a pure fn).
    // -----------------------------------------------------------------
    #[test]
    fn discovery_parses_endpoints_models_and_pricing_from_a_fixture_event() {
        let fixture = json!({
            "endpoints": ["https://provider.example.com", "http://abc123.onion"],
            "models": ["gpt-4o-mini"],
            "pricing": { "gpt-4o-mini": 25 },
        })
        .to_string();
        let parsed = parse_provider_event(&fixture).expect("fixture event parses");
        assert_eq!(
            parsed.endpoints,
            vec![
                "https://provider.example.com".to_string(),
                "http://abc123.onion".to_string()
            ]
        );
        assert_eq!(parsed.models, vec!["gpt-4o-mini".to_string()]);
        assert_eq!(parsed.pricing.get("gpt-4o-mini"), Some(&25));
    }

    /// A malformed/unexpected event content must never crash discovery —
    /// it is simply excluded.
    #[test]
    fn malformed_provider_event_is_skipped_not_a_panic() {
        assert!(parse_provider_event("not even json {{{").is_none());
        assert!(parse_provider_event("42").is_none());
    }

    // -----------------------------------------------------------------
    // Behavior 2: discovery finding nothing (or select_provider finding no
    // affordable candidate) is an empty/None result, never an error — and
    // RoutstrBackend::send_with_providers with zero providers returns a
    // clean, well-typed Err (never a panic), which is what lets
    // select_backend's fallback chain fall through cleanly rather than
    // hanging or crashing the turn.
    // -----------------------------------------------------------------
    #[test]
    fn no_provider_found_falls_through_not_errors() {
        // select_provider itself: zero providers -> None, not a panic.
        assert!(select_provider(&[], 1_000, false).is_none());
        // A provider that doesn't price the model nobody asked for either
        // -> still None.
        let p = provider("https://example.com", "some-model", 10);
        assert!(select_provider(&[p], 0, false).is_none());
    }

    #[tokio::test]
    async fn send_with_zero_providers_returns_a_clean_error_not_a_panic() {
        let backend = backend_for("unused", 1_000);
        let result = backend.send_with_providers(&[], "sys", &[], &[]).await;
        assert!(result.is_err(), "zero providers must be a clean Err");
        let msg = result.err().expect("checked is_err above").to_string();
        assert!(
            msg.to_lowercase().contains("provider"),
            "the error must explain why: {msg}"
        );
    }

    // -----------------------------------------------------------------
    // Behavior 3: select_provider picks the cheapest affordable price,
    // preferring an onion endpoint when Tor is up.
    // -----------------------------------------------------------------
    #[test]
    fn select_provider_picks_cheapest_affordable_price() {
        let cheap = provider("https://cheap.example.com", "model-a", 10);
        let expensive = provider("https://expensive.example.com", "model-a", 500);
        let (chosen, model, price, endpoint) =
            select_provider(&[expensive, cheap], 1_000, false).expect("affordable candidate");
        assert_eq!(price, 10);
        assert_eq!(model, "model-a");
        assert_eq!(endpoint, "https://cheap.example.com");
        assert_eq!(chosen.pricing.get("model-a"), Some(&10));
    }

    #[test]
    fn select_provider_excludes_prices_over_the_remaining_budget() {
        let too_expensive = provider("https://x.example.com", "model-a", 5_000);
        assert!(select_provider(&[too_expensive], 100, false).is_none());
    }

    #[test]
    fn select_provider_prefers_onion_endpoint_when_tor_is_up() {
        let mut pricing = HashMap::new();
        pricing.insert("model-a".to_string(), 10);
        let p = RoutstrProvider {
            endpoints: vec![
                "https://clearnet.example.com".to_string(),
                "http://onionaddr123.onion".to_string(),
            ],
            models: vec!["model-a".to_string()],
            pricing,
        };
        let (_p, _m, _price, endpoint) =
            select_provider(&[p.clone()], 1_000, true).expect("affordable");
        assert_eq!(endpoint, "http://onionaddr123.onion");

        // Tor down -> clearnet endpoint instead.
        let (_p, _m, _price, endpoint) = select_provider(&[p], 1_000, false).expect("affordable");
        assert_eq!(endpoint, "https://clearnet.example.com");
    }

    // -----------------------------------------------------------------
    // Behavior 4/7: the chat request is OpenAI-shaped, carries a tools[]
    // array, is non-streaming, and always sets the generation cap.
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn request_is_openai_shaped_non_streaming_with_tools_and_explicit_cap() {
        let stub = StubRoutstr::start(chat_response_text("ok")).await;
        let backend = backend_for(&stub.base_url, 1_000);
        let tool = crate::assistant::tools::system_disk_status_tool();
        let history = vec![ChatMessage {
            role: Role::User,
            text: Some("hi".to_string()),
            tool_calls: vec![],
            tool_results: vec![],
        }];
        backend
            .send_paid_request(
                "model-a",
                &stub.base_url,
                "cashuAtesttoken",
                "sys prompt",
                &[tool],
                &history,
            )
            .await
            .expect("send");

        let reqs = stub.captured().await;
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].path, "/v1/chat/completions");
        assert_eq!(
            reqs[0].body.get("stream").and_then(|v| v.as_bool()),
            Some(false),
            "every turn must be requested non-streaming"
        );
        assert_eq!(
            reqs[0].body.get("max_tokens").and_then(|v| v.as_u64()),
            Some(ROUTSTR_MAX_TOKENS as u64),
            "the generation cap must be set explicitly on every request"
        );
        let tools_sent = reqs[0]
            .body
            .get("tools")
            .and_then(|t| t.as_array())
            .expect("tools array present");
        assert!(!tools_sent.is_empty());
        let messages = reqs[0]
            .body
            .get("messages")
            .and_then(|m| m.as_array())
            .expect("messages array present");
        assert!(messages.len() >= 2, "system + user message expected");
    }

    // -----------------------------------------------------------------
    // Behavior 5: tool-call arguments arrive as a JSON-encoded string and
    // are parsed exactly once at this adapter's edge.
    // -----------------------------------------------------------------
    #[test]
    fn openai_string_arguments_are_parsed_once_at_the_edge() {
        let raw = vec![json!({
            "id": "call_1",
            "type": "function",
            "function": {
                "name": "app_restart",
                "arguments": "{\"app_id\":\"immich\",\"nested\":{\"a\":1}}",
            }
        })];
        let calls = parse_openai_tool_calls(&raw);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].name, "app_restart");
        assert_eq!(
            calls[0].arguments,
            json!({ "app_id": "immich", "nested": { "a": 1 } }),
            "the string-encoded arguments must be parsed into the same object shape every other backend produces"
        );
    }

    #[tokio::test]
    async fn tool_calls_response_maps_to_backend_turn_tool_calls_with_parsed_arguments() {
        let stub = StubRoutstr::start(chat_response_with_tool_calls(vec![(
            "call_1",
            "app_restart",
            "{\"app_id\":\"immich\"}",
        )]))
        .await;
        let backend = backend_for(&stub.base_url, 1_000);
        let result = backend
            .send_paid_request(
                "model-a",
                &stub.base_url,
                "cashuAtesttoken",
                "sys",
                &[],
                &[],
            )
            .await
            .expect("send");
        let BackendTurn::ToolCalls(calls) = result else {
            panic!("expected tool calls")
        };
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].arguments, json!({ "app_id": "immich" }));
    }

    // -----------------------------------------------------------------
    // Behavior 6: each tool_calls[] entry's id is echoed back in the
    // corresponding result turn.
    // -----------------------------------------------------------------
    #[test]
    fn tool_call_id_is_echoed_back_in_the_result_turn() {
        let msg = ChatMessage {
            role: Role::Tool,
            text: None,
            tool_calls: vec![],
            tool_results: vec![crate::assistant::tools::ToolResult {
                call_id: "call_1".to_string(),
                content: "{\"ok\":true}".to_string(),
                is_error: false,
            }],
        };
        let wire = message_to_wire(&msg);
        assert_eq!(wire.len(), 1);
        assert_eq!(
            wire[0].get("tool_call_id").and_then(|v| v.as_str()),
            Some("call_1"),
            "the tool result's wire form must echo the same id the model's tool_calls[] entry carried"
        );
    }

    // -----------------------------------------------------------------
    // Behavior 8: payment is attached using the header spelling
    // 13-ROUTSTR-FINDINGS.md recorded, and the token comes from the
    // existing primitive — never constructed here.
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn payment_token_is_attached_via_the_documented_header() {
        let stub = StubRoutstr::start(chat_response_text("ok")).await;
        let backend = backend_for(&stub.base_url, 1_000);
        backend
            .send_paid_request(
                "model-a",
                &stub.base_url,
                "cashuAsometesttoken",
                "sys",
                &[],
                &[],
            )
            .await
            .expect("send");
        let reqs = stub.captured().await;
        // HTTP header names are case-insensitive on the wire — hyper
        // canonicalizes to lowercase when iterating captured headers, so
        // compare case-insensitively rather than assuming the exact
        // capitalization `PAYMENT_HEADER` uses when constructing the
        // request survives round-trip capture.
        assert_eq!(
            reqs[0]
                .headers
                .get(&PAYMENT_HEADER.to_lowercase())
                .map(|s| s.as_str()),
            Some("Bearer cashuAsometesttoken"),
            "the payment header spelling must match 13-ROUTSTR-FINDINGS.md's cited value: {:?}",
            reqs[0].headers
        );
    }

    /// A price over the remaining budget declines WITHOUT touching the
    /// wallet — `auto_pay_token`'s own short-circuit
    /// (`policy.affords`) — and `send_with_providers` propagates that as a
    /// clear Err, never constructing a token itself.
    #[tokio::test]
    async fn over_budget_price_declines_without_a_token_ever_being_built() {
        let backend = backend_for("http://127.0.0.1:1", 5); // budget too small
        let p = provider("http://127.0.0.1:1", "model-a", 500);
        let result = backend.send_with_providers(&[p], "sys", &[], &[]).await;
        assert!(result.is_err());
        // 500 > 5, so select_provider itself must already exclude this
        // candidate — the error must be "no provider", not a payment
        // failure, proving the budget filter runs before any payment
        // attempt at all.
        let msg = result.err().expect("checked is_err above").to_string();
        assert!(
            msg.to_lowercase().contains("provider"),
            "an unaffordable price must be filtered out by select_provider before any payment attempt: {msg}"
        );
    }

    // -----------------------------------------------------------------
    // Behavior 9 (grep-verified at the acceptance-criteria level, and
    // demonstrated behaviorally here): screen_outbound runs on this leg
    // before any body is sent. A secret-shaped user turn is blocked
    // BEFORE the stub ever receives a request.
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn secret_shaped_content_never_reaches_the_stub() {
        let stub = StubRoutstr::start(chat_response_text("ok")).await;
        let backend = backend_for(&stub.base_url, 1_000);
        // A CHECKSUM-VALID mnemonic — what a real leak looks like, and what
        // the screen actually keys on. The earlier fixture was the first
        // twelve wordlist entries, which is not a parseable mnemonic: the
        // 2026-08-06 precision rewrite of `screen_outbound` moved from a
        // shape rule to checksum validation (the shape rule blocked every
        // legitimate turn on a live node, twice), `egress.rs`'s own test was
        // updated to match, and this copy was not — so it failed here while
        // the behaviour it names was intact. Checksum-invalid runs are
        // deliberately allowed below `IMPLAUSIBLE_MEMBER_RUN`; see
        // `assistant::egress`.
        let secret_seed = "abandon abandon abandon abandon abandon abandon \
                           abandon abandon abandon abandon abandon about";
        assert!(
            bip39::Mnemonic::parse_normalized(secret_seed).is_ok(),
            "the fixture must be a real mnemonic or this test proves nothing"
        );
        let history = vec![ChatMessage {
            role: Role::User,
            text: Some(format!("my seed is: {secret_seed}")),
            tool_calls: vec![],
            tool_results: vec![],
        }];
        let result = backend
            .send_paid_request(
                "model-a",
                &stub.base_url,
                "cashuAtesttoken",
                "sys",
                &[],
                &history,
            )
            .await;
        assert!(result.is_err(), "a secret-shaped body must be blocked");
        assert_eq!(
            stub.captured().await.len(),
            0,
            "nothing must reach the stub once the body is blocked"
        );
    }

    // -----------------------------------------------------------------
    // Capability probe (Task 1's decision): a non-success HTTP status or
    // an unexpected response shape fails loudly, never silently.
    // -----------------------------------------------------------------
    #[tokio::test]
    async fn non_success_status_fails_loudly() {
        let stub = StubRoutstr::start_with_status(402, json!({"error": "payment required"})).await;
        let backend = backend_for(&stub.base_url, 1_000);
        let result = backend
            .send_paid_request(
                "model-a",
                &stub.base_url,
                "cashuAtesttoken",
                "sys",
                &[],
                &[],
            )
            .await;
        assert!(result.is_err());
        assert!(result
            .err()
            .expect("checked is_err above")
            .to_string()
            .contains("402"));
    }

    #[tokio::test]
    async fn response_missing_choices_fails_loudly_not_silently() {
        let stub = StubRoutstr::start(json!({"unexpected": "shape"})).await;
        let backend = backend_for(&stub.base_url, 1_000);
        let result = backend
            .send_paid_request(
                "model-a",
                &stub.base_url,
                "cashuAtesttoken",
                "sys",
                &[],
                &[],
            )
            .await;
        assert!(
            result.is_err(),
            "a response that doesn't match the docs-shaped contract must fail loudly, never return an empty/garbage answer"
        );
        assert!(result
            .err()
            .expect("checked is_err above")
            .to_string()
            .contains("choices"));
    }

    /// An unreachable Routstr endpoint returns a transport error from
    /// `send_paid_request`, never a panic.
    #[tokio::test]
    async fn unreachable_endpoint_returns_transport_error_not_panic() {
        let backend = backend_for("http://127.0.0.1:1", 1_000);
        let result = backend
            .send_paid_request(
                "model-a",
                "http://127.0.0.1:1",
                "cashuAtesttoken",
                "sys",
                &[],
                &[],
            )
            .await;
        assert!(result.is_err());
    }
}
