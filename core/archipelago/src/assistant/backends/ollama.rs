//! The Ollama leg of the D-04 backend chain — `POST /api/chat` with a
//! `messages` array and a `tools` array, and a `message.tool_calls`
//! response. This is a DIFFERENT endpoint and a DIFFERENT request/response
//! shape from `mesh/listener/assist.rs::call_ollama`'s single-shot prompt
//! endpoint, which has no tool-calling support at all (13-AI-SPEC.md §3
//! Pitfall 1) — `call_ollama` is not extended in place.
//!
//! Two cross-provider gotchas live entirely at this adapter's edge, never
//! in the shared loop (`loop_.rs`) or the shared tool model (`tools.rs`):
//! Ollama's `function.arguments` arrives as an already-parsed JSON object,
//! never a JSON-encoded string that would need a second parse pass the way
//! OpenAI-shape providers' arguments do (Pitfall 2), and Ollama's tool
//! calls carry no `id` field at all, so this file synthesizes one
//! (Pitfall 3) or the loop's result-matching would break silently.

use std::collections::HashMap;
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Backend, BackendTurn};
use crate::assistant::tools::{ChatMessage, Role, ToolCall, ToolDef};

/// The real local Ollama server. `OllamaBackend::new` takes a `base_url`
/// explicitly rather than hardcoding this everywhere, so this module's own
/// tests can point the SAME type at a local HTTP stub instead — see the
/// `tests` module below.
pub const OLLAMA_BASE_URL: &str = "http://localhost:11434";
/// Ollama's tool-calling chat endpoint path. Tool-calling needs THIS
/// endpoint's `messages`/`tools` request shape and `message.tool_calls`
/// response shape — the older single-shot prompt endpoint
/// `assist.rs::call_ollama` posts to has no tool-calling support at all.
pub const OLLAMA_CHAT_URL: &str = "/api/chat";
/// Ollama's model-info endpoint — queried by `model_supports_tools` to
/// turn 13-AI-SPEC.md §4's `[ASSUMED]` note about `qwen2.5-coder` into a
/// runtime fact instead of a guess.
const OLLAMA_SHOW_URL: &str = "/api/show";
/// Default model when the node hasn't configured one — mirrors
/// `assist.rs::DEFAULT_MODEL`. Never trusted blindly: `model_supports_tools`
/// checks THIS specific model's capability before it is ever handed a
/// tool; a non-tool-capable result falls through to Claude (see
/// `backends::select_backend`) rather than silently degrading to a
/// tools-free assistant.
pub const OLLAMA_DEFAULT_MODEL: &str = "qwen2.5-coder";
/// Explicit generation-length cap (`options.num_predict`), sent on every
/// request — never left unbounded (AI-SPEC §4b.3). Kept below the Claude
/// leg's 2048-token cap: local models generate slower per token than
/// Claude, so a smaller cap keeps a local turn's worst-case latency sane
/// on a modest node.
const OLLAMA_NUM_PREDICT: u32 = 1024;
/// New, separate HTTP timeout for the AIUI Ollama path's multi-turn tool
/// loop — deliberately NOT the mesh module's own 60-second, LoRa-airtime-
/// tuned constant of the same shape (AI-SPEC §3 Pitfall 6). Reusing that
/// one here would under-time-out a legitimate multi-turn local-model round
/// trip that has no radio-airtime constraint at all.
const OLLAMA_HTTP_TIMEOUT: Duration = Duration::from_secs(120);

/// The Ollama leg of the D-04 backend chain. `base_url` is explicit (never
/// a hardcoded constant read directly inside `send()`) so production
/// (`OLLAMA_BASE_URL`) and this module's own tests (a local HTTP stub)
/// construct the identical type against different servers.
pub struct OllamaBackend {
    base_url: String,
    model: String,
}

impl OllamaBackend {
    pub fn new(base_url: String, model: String) -> Self {
        Self { base_url, model }
    }
}

#[async_trait]
impl Backend for OllamaBackend {
    async fn send(
        &self,
        system: &str,
        tools: &[ToolDef],
        history: &[ChatMessage],
    ) -> Result<BackendTurn> {
        let mut messages: Vec<Value> = vec![json!({ "role": "system", "content": system })];
        messages.extend(history.iter().flat_map(message_to_wire));

        let ollama_tools: Vec<Value> = tools
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
            "model": self.model,
            "messages": messages,
            // Requested non-streaming for every turn while the loop is
            // still deciding whether a tool is being called — partial
            // JSON tool arguments cannot be structurally validated
            // mid-stream (AI-SPEC §4b.2). The final, tool-free answer turn
            // would be a legitimate streaming candidate, but this adapter
            // always buffers: the shared loop needs the complete text back
            // either way.
            "stream": false,
            // Generation cap, always explicit — never unbounded.
            "options": { "num_predict": OLLAMA_NUM_PREDICT },
        });
        if !ollama_tools.is_empty() {
            body["tools"] = json!(ollama_tools);
        }

        let client = reqwest::Client::builder()
            .timeout(OLLAMA_HTTP_TIMEOUT)
            .build()?;
        let url = format!("{}{}", self.base_url, OLLAMA_CHAT_URL);
        let resp = client.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Ollama chat HTTP {}: {}",
                status,
                txt.chars().take(180).collect::<String>()
            );
        }

        let json: Value = resp.json().await?;
        let message = json.get("message").cloned().unwrap_or_default();
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

        let tool_calls: Vec<ToolCall> = raw_calls
            .iter()
            .enumerate()
            .map(|(idx, raw)| {
                let name = raw
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                // Ollama's function.arguments arrives as an already-parsed
                // JSON object. Assigning it straight through is the
                // CORRECT normalization here — re-parsing it as a string
                // (right for OpenAI-shape providers, per AI-SPEC §3
                // Pitfall 2) would be a type error against this wire
                // shape, and is never done anywhere in this file.
                let arguments = raw
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                ToolCall {
                    // Ollama gives tool calls no id at all — synthesize a
                    // stable, non-empty, unique-within-this-turn id.
                    // Leaving this empty would make the loop's
                    // result-matching break silently, which is worse than
                    // failing loudly.
                    id: synthesize_call_id(idx),
                    name,
                    arguments,
                }
            })
            .collect();
        Ok(BackendTurn::ToolCalls(tool_calls))
    }
}

/// A stable, non-empty, unique-within-the-turn id for a tool call whose
/// wire response carried none. `idx` is the call's position within THIS
/// turn's `tool_calls` array — sufficient for uniqueness because a fresh
/// set of ids is synthesized for every `send()` call (every model turn),
/// never reused or carried across turns.
fn synthesize_call_id(idx: usize) -> String {
    format!("ollama-tool-{idx}")
}

/// Map one internal `ChatMessage` onto zero or more Ollama chat wire
/// messages. `Role::Tool` can carry more than one `ToolResult` in a single
/// `ChatMessage` (a batch of tool calls from one turn) — Ollama's wire
/// format has no analog to Claude's single "user" turn wrapping an array
/// of `tool_result` blocks, so each result becomes its own `role: "tool"`
/// message instead.
fn message_to_wire(msg: &ChatMessage) -> Vec<Value> {
    match msg.role {
        // The system prompt is sent as the FIRST message in `send()`
        // itself, sourced from the `system` parameter, never from
        // history — this arm exists for completeness (mirrors
        // `claude.rs`'s handling) but no `ChatMessage` of role System is
        // constructed anywhere in this codebase today.
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
                            "function": { "name": c.name, "arguments": c.arguments },
                        })
                    })
                    .collect();
                vec![json!({ "role": "assistant", "content": "", "tool_calls": calls })]
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
            .map(|r| json!({ "role": "tool", "content": r.content }))
            .collect(),
    }
}

/// Process-lifetime cache for `model_supports_tools`, keyed by
/// `"{base_url}::{model}"` so two different stub servers in the same test
/// binary — or a stub and the real node — never collide on the same
/// model name.
static TOOL_CAPABILITY_CACHE: OnceLock<StdMutex<HashMap<String, bool>>> = OnceLock::new();

/// Query Ollama's own model-info endpoint (`/api/show`) for whether
/// `model` is tagged tool-capable, and cache the answer for the process
/// lifetime. This is what turns 13-AI-SPEC.md §4's `[ASSUMED]` note about
/// `qwen2.5-coder` into a runtime fact: a model that cannot call tools is
/// never silently used as the assistant's primary. Fails CLOSED — an
/// unreachable node, a malformed response, or a `capabilities` list that
/// doesn't mention `"tools"` all report `false`, so `select_backend` falls
/// through to Claude rather than handing tools to a model that cannot use
/// them.
pub async fn model_supports_tools(base_url: &str, model: &str) -> bool {
    let cache_key = format!("{base_url}::{model}");
    let cache = TOOL_CAPABILITY_CACHE.get_or_init(|| StdMutex::new(HashMap::new()));
    if let Some(&cached) = cache
        .lock()
        .expect("tool capability cache poisoned")
        .get(&cache_key)
    {
        return cached;
    }
    let supports = probe_model_supports_tools(base_url, model).await;
    cache
        .lock()
        .expect("tool capability cache poisoned")
        .insert(cache_key, supports);
    supports
}

async fn probe_model_supports_tools(base_url: &str, model: &str) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(OLLAMA_HTTP_TIMEOUT)
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let url = format!("{base_url}{OLLAMA_SHOW_URL}");
    let resp = match client
        .post(&url)
        .json(&json!({ "model": model }))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        _ => return false,
    };
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => return false,
    };
    body.get("capabilities")
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().any(|v| v.as_str() == Some("tools")))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use tokio::sync::Mutex as AsyncMutex;

    /// One HTTP request captured by the stub server: the path hit and the
    /// parsed JSON body sent.
    #[derive(Debug, Clone)]
    struct CapturedRequest {
        path: String,
        body: Value,
    }

    /// A minimal local HTTP stub standing in for Ollama's `/api/chat` and
    /// `/api/show` endpoints. No mock-HTTP crate exists in this workspace
    /// (verified against `Cargo.toml`) — built directly on `hyper`
    /// (already in-tree, `full` feature), matching `server.rs`'s own
    /// `Http::new().serve_connection` pattern.
    struct StubOllama {
        base_url: String,
        captured: Arc<AsyncMutex<Vec<CapturedRequest>>>,
        response: Arc<AsyncMutex<Value>>,
    }

    impl StubOllama {
        /// Start the stub; `response` is returned verbatim (as JSON) for
        /// every request regardless of path — tests that care which path
        /// was hit read `captured()` afterwards.
        async fn start(response: Value) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind stub");
            let addr = listener.local_addr().expect("local_addr");
            let captured = Arc::new(AsyncMutex::new(Vec::new()));
            let response = Arc::new(AsyncMutex::new(response));
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
                                    let body_bytes = hyper::body::to_bytes(req.into_body())
                                        .await
                                        .unwrap_or_default();
                                    let body: Value =
                                        serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
                                    captured.lock().await.push(CapturedRequest { path, body });
                                    let resp_body = response.lock().await.clone();
                                    let resp = hyper::Response::builder()
                                        .status(200)
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

        async fn set_response(&self, response: Value) {
            *self.response.lock().await = response;
        }
    }

    fn chat_response_text(text: &str) -> Value {
        json!({
            "model": "test-model",
            "message": { "role": "assistant", "content": text },
            "done": true,
        })
    }

    fn chat_response_with_tool_calls(calls: Vec<(&str, Value)>) -> Value {
        json!({
            "model": "test-model",
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": calls
                    .into_iter()
                    .map(|(name, args)| json!({ "function": { "name": name, "arguments": args } }))
                    .collect::<Vec<_>>(),
            },
            "done": true,
        })
    }

    fn show_response(capabilities: &[&str]) -> Value {
        json!({ "capabilities": capabilities })
    }

    /// Behavior: a turn with tools produces a request to the chat endpoint
    /// — never the older single-shot prompt endpoint.
    #[tokio::test]
    async fn ollama_uses_chat_endpoint_not_generate() {
        let stub = StubOllama::start(chat_response_text("hello")).await;
        let backend = OllamaBackend::new(stub.base_url.clone(), "test-model".to_string());
        let result = backend.send("system", &[], &[]).await.expect("send");
        assert!(matches!(result, BackendTurn::Text(t) if t == "hello"));
        let reqs = stub.captured().await;
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].path, OLLAMA_CHAT_URL);
    }

    /// Behavior: a response containing tool calls maps to
    /// `BackendTurn::ToolCalls`, with each call assigned a non-empty,
    /// unique-within-the-turn id even though the wire response carries
    /// none.
    #[tokio::test]
    async fn tool_calls_get_synthesized_ids() {
        let stub = StubOllama::start(chat_response_with_tool_calls(vec![
            ("app_restart", json!({ "app_id": "immich" })),
            ("app_restart", json!({ "app_id": "gitea" })),
        ]))
        .await;
        let backend = OllamaBackend::new(stub.base_url.clone(), "test-model".to_string());
        let result = backend.send("system", &[], &[]).await.expect("send");
        let BackendTurn::ToolCalls(calls) = result else {
            panic!("expected tool calls")
        };
        assert_eq!(calls.len(), 2);
        assert!(!calls[0].id.is_empty(), "id must not be empty");
        assert!(!calls[1].id.is_empty(), "id must not be empty");
        assert_ne!(
            calls[0].id, calls[1].id,
            "ids must be unique within the turn"
        );
    }

    /// Behavior: tool-call arguments arrive as an already-parsed object
    /// and are passed through without a second string-parse.
    #[tokio::test]
    async fn arguments_object_is_not_string_parsed() {
        let args = json!({ "app_id": "immich", "nested": { "a": 1, "b": [1, 2, 3] } });
        let stub = StubOllama::start(chat_response_with_tool_calls(vec![(
            "app_restart",
            args.clone(),
        )]))
        .await;
        let backend = OllamaBackend::new(stub.base_url.clone(), "test-model".to_string());
        let result = backend.send("system", &[], &[]).await.expect("send");
        let BackendTurn::ToolCalls(calls) = result else {
            panic!("expected tool calls")
        };
        assert_eq!(
            calls[0].arguments, args,
            "arguments must pass through as the same parsed object, not a re-parsed string"
        );
    }

    /// Behavior: a response with only text maps to `BackendTurn::Text`.
    #[tokio::test]
    async fn text_only_response_maps_to_backend_turn_text() {
        let stub = StubOllama::start(chat_response_text("disk space report generated")).await;
        let backend = OllamaBackend::new(stub.base_url.clone(), "test-model".to_string());
        let result = backend.send("system", &[], &[]).await.expect("send");
        assert!(matches!(result, BackendTurn::Text(t) if t == "disk space report generated"));
    }

    /// Behavior: every turn is requested non-streaming, and the
    /// generation-length cap is always set explicitly.
    #[tokio::test]
    async fn request_is_non_streaming_with_explicit_generation_cap() {
        let stub = StubOllama::start(chat_response_text("ok")).await;
        let backend = OllamaBackend::new(stub.base_url.clone(), "test-model".to_string());
        backend.send("system", &[], &[]).await.expect("send");
        let reqs = stub.captured().await;
        assert_eq!(
            reqs[0].body.get("stream").and_then(|v| v.as_bool()),
            Some(false),
            "every turn must be requested non-streaming"
        );
        assert_eq!(
            reqs[0]
                .body
                .get("options")
                .and_then(|o| o.get("num_predict"))
                .and_then(|v| v.as_u64()),
            Some(OLLAMA_NUM_PREDICT as u64),
            "the generation cap must be set explicitly on every request"
        );
    }

    /// The request carries a `messages` array (system + history) and a
    /// `tools` array — never a bare prompt string.
    #[tokio::test]
    async fn request_carries_messages_and_tools_arrays() {
        let stub = StubOllama::start(chat_response_text("ok")).await;
        let backend = OllamaBackend::new(stub.base_url.clone(), "test-model".to_string());
        let tool = crate::assistant::tools::system_disk_status_tool();
        let history = vec![ChatMessage {
            role: Role::User,
            text: Some("hi".to_string()),
            tool_calls: vec![],
            tool_results: vec![],
        }];
        backend
            .send("sys prompt", &[tool], &history)
            .await
            .expect("send");
        let reqs = stub.captured().await;
        let body = &reqs[0].body;
        let messages = body
            .get("messages")
            .and_then(|m| m.as_array())
            .expect("messages array present");
        assert!(
            messages.len() >= 2,
            "expected at least the system message and the user message: {messages:?}"
        );
        let tools = body
            .get("tools")
            .and_then(|t| t.as_array())
            .expect("tools array present");
        assert!(!tools.is_empty());
    }

    /// `model_supports_tools` reads the `capabilities` array from
    /// `/api/show` — a tool-capable model reports true.
    #[tokio::test]
    async fn model_supports_tools_reads_capabilities_from_api_show() {
        let stub = StubOllama::start(show_response(&["completion", "tools"])).await;
        let supports = model_supports_tools(&stub.base_url, "unique-model-a").await;
        assert!(supports);
        let reqs = stub.captured().await;
        assert_eq!(reqs[0].path, OLLAMA_SHOW_URL);
    }

    /// Behavior: `select_backend` falls through to Claude when the
    /// configured model is reachable but not tool-capable — the mechanism
    /// this proves is that `model_supports_tools` reports `false` for such
    /// a model, which is exactly the signal `backends::select_backend`
    /// acts on.
    #[tokio::test]
    async fn non_tool_capable_model_falls_through_to_claude() {
        let stub = StubOllama::start(show_response(&["completion"])).await;
        let supports = model_supports_tools(&stub.base_url, "unique-model-b").await;
        assert!(
            !supports,
            "a model without the tools capability must report false so select_backend falls through to Claude"
        );
    }

    /// `model_supports_tools` caches its answer for the process lifetime —
    /// a second call for the same (base_url, model) does not re-probe.
    #[tokio::test]
    async fn model_supports_tools_caches_for_process_lifetime() {
        let stub = StubOllama::start(show_response(&["tools"])).await;
        let first = model_supports_tools(&stub.base_url, "unique-model-c").await;
        assert!(first);
        // Reconfigure the stub to report no tools capability — a cached
        // call must NOT re-probe and see this change.
        stub.set_response(show_response(&["completion"])).await;
        let second = model_supports_tools(&stub.base_url, "unique-model-c").await;
        assert!(
            second,
            "the process-lifetime cache must not re-probe once an answer is cached"
        );
    }

    /// An unreachable Ollama returns a transport error from `send()`
    /// rather than panicking — `backends::select_backend`'s `FallbackChain`
    /// is what turns this into a fall-through, but this adapter's own
    /// contract is simply: propagate the error, never crash.
    #[tokio::test]
    async fn unreachable_ollama_returns_transport_error_not_panic() {
        let backend = OllamaBackend::new("http://127.0.0.1:1".to_string(), "m".to_string());
        let result = backend.send("sys", &[], &[]).await;
        assert!(result.is_err());
    }
}
