//! The Claude leg of the D-04 backend chain — Anthropic Messages API with
//! `tools`/`tool_use`/`tool_result`. Modeled on
//! `mesh/listener/assist.rs::call_claude`'s HTTP client construction and
//! `api/rpc/mesh/assistant.rs`'s key-path convention, but NOT extended
//! in place: this is a new, tool-calling-capable request/response shape,
//! and its constants are new (AI-SPEC §3 Pitfall 6 — the mesh constants are
//! airtime-tuned for LoRa and must not be reused here).

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Backend, BackendTurn};
use crate::assistant::egress::{self, EgressVerdict};
use crate::assistant::tools::{ChatMessage, Role, ToolCall, ToolDef};

const CLAUDE_URL: &str = "https://api.anthropic.com/v1/messages";
/// Kept in sync with `mesh/listener/assist.rs::CLAUDE_DEFAULT_MODEL` —
/// cheap and already proven fast enough; D-07 makes backend choice a
/// privacy/cost decision, not a capability-need one, so there is no reason
/// to default to a stronger model here.
const CLAUDE_MODEL: &str = "claude-haiku-4-5-20251001";
/// New, separate constant for the AIUI path's multi-turn tool loop (which
/// may include a network round trip) — deliberately NOT reusing the mesh
/// module's LoRa-airtime-tuned HTTP timeout constant (60s), which is sized
/// for a different transport entirely (AI-SPEC §3 Pitfall 6).
const ASSISTANT_HTTP_TIMEOUT: Duration = Duration::from_secs(180);
/// Raised from mesh's `512` — `tool_use` content blocks and multi-turn
/// reasoning need more headroom. Never left unbounded.
const ASSISTANT_MAX_TOKENS: u32 = 2048;

pub struct ClaudeBackend {
    data_dir: PathBuf,
}

impl ClaudeBackend {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl Backend for ClaudeBackend {
    async fn send(
        &self,
        system: &str,
        tools: &[ToolDef],
        history: &[ChatMessage],
    ) -> Result<BackendTurn> {
        // SAME key path `api/rpc/mesh/assistant.rs` probes — do not
        // introduce a second key location (D-01, one key ledger).
        let key = tokio::fs::read_to_string(self.data_dir.join("secrets/claude-api-key"))
            .await
            .map_err(|_| anyhow::anyhow!("Claude API key not configured on this node"))?;
        let key = key.trim();
        if key.is_empty() {
            anyhow::bail!("Claude API key is empty");
        }

        let messages: Vec<Value> = history.iter().filter_map(message_to_wire).collect();

        let claude_tools: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect();

        let mut body = json!({
            "model": CLAUDE_MODEL,
            "max_tokens": ASSISTANT_MAX_TOKENS,
            "system": system,
            "messages": messages,
            "stream": false,
        });
        if !claude_tools.is_empty() {
            body["tools"] = json!(claude_tools);
            // AI-SPEC §3 Pitfall 5: every tool_use.id from one assistant
            // turn needs a matching tool_result before the next request.
            // Disabling parallel tool use sidesteps that bookkeeping —
            // D-06's tools are one deliberate action at a time anyway.
            body["tool_choice"] = json!({"type": "auto", "disable_parallel_tool_use": true});
        }

        // G-B1/G-B2: screen the outbound body before it ever leaves this
        // node — the Claude leg is a cloud backend (unlike Ollama, which
        // never calls this at all — see egress.rs's module doc). Fails
        // closed: on a block, this call returns an Err and nothing was
        // sent.
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
                    "outbound request to Claude blocked before it left this node — it appeared \
                     to contain secret-shaped material (G-B1). Falling back to the local backend."
                );
            }
        }

        let client = reqwest::Client::builder()
            .timeout(ASSISTANT_HTTP_TIMEOUT)
            .build()?;
        let resp = client
            .post(CLAUDE_URL)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Claude API HTTP {}: {}",
                status,
                txt.chars().take(180).collect::<String>()
            );
        }

        let json: Value = resp.json().await?;
        let blocks = json
            .get("content")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        let mut tool_calls = Vec::new();
        let mut text = String::new();
        for block in &blocks {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("tool_use") => {
                    let id = block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let arguments = block.get("input").cloned().unwrap_or_else(|| json!({}));
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments,
                    });
                }
                Some("text") => {
                    if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                        text.push_str(t);
                    }
                }
                _ => {}
            }
        }

        if !tool_calls.is_empty() {
            Ok(BackendTurn::ToolCalls(tool_calls))
        } else {
            Ok(BackendTurn::Text(text))
        }
    }
}

/// Map one internal `ChatMessage` onto an Anthropic Messages API turn.
/// `Role::System` returns `None` — the system prompt is sent via the
/// top-level `system` field, not as a message in the array.
fn message_to_wire(msg: &ChatMessage) -> Option<Value> {
    match msg.role {
        Role::System => None,
        Role::User => Some(json!({
            "role": "user",
            "content": msg.text.clone().unwrap_or_default(),
        })),
        Role::Assistant => {
            if !msg.tool_calls.is_empty() {
                let blocks: Vec<Value> = msg
                    .tool_calls
                    .iter()
                    .map(|c| {
                        json!({
                            "type": "tool_use",
                            "id": c.id,
                            "name": c.name,
                            "input": c.arguments,
                        })
                    })
                    .collect();
                Some(json!({"role": "assistant", "content": blocks}))
            } else {
                Some(json!({
                    "role": "assistant",
                    "content": msg.text.clone().unwrap_or_default(),
                }))
            }
        }
        Role::Tool => {
            let blocks: Vec<Value> = msg
                .tool_results
                .iter()
                .map(|r| {
                    json!({
                        "type": "tool_result",
                        "tool_use_id": r.call_id,
                        "content": r.content,
                        "is_error": r.is_error,
                    })
                })
                .collect();
            // Anthropic's tool_result blocks travel back as a "user" turn.
            Some(json!({"role": "user", "content": blocks}))
        }
    }
}
