//! The `Backend` trait — the wire-format-agnostic seam every model backend
//! (Ollama, Claude, Routstr) implements once. The loop and every tool are
//! written against this trait only; wire-format differences live entirely
//! inside each adapter.

use anyhow::Result;
use async_trait::async_trait;

use super::tools::{ChatMessage, ToolCall, ToolDef};
use crate::api::rpc::RpcHandler;

pub mod claude;
pub mod ollama;
pub mod routstr;
#[cfg(test)]
pub mod scripted;

pub enum BackendTurn {
    Text(String),
    ToolCalls(Vec<ToolCall>),
}

#[async_trait]
pub trait Backend: Send + Sync {
    async fn send(
        &self,
        system: &str,
        tools: &[ToolDef],
        history: &[ChatMessage],
    ) -> Result<BackendTurn>;
}

/// D-04's identified backends — used for tracing which backend answered a
/// given turn. Deliberately NOT carried into `ChatMessage`/`history.rs`
/// (outside this plan's file scope; per-turn backend attribution in the
/// persisted transcript is a natural follow-up, not required by any of
/// 13-10's behaviors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendId {
    Ollama,
    Claude,
    /// 13-13: the third D-04 leg. Not currently returned as the "primary"
    /// id by `select_backend` (mirroring the existing convention that the
    /// returned id names the primary attempt, not necessarily which leg of
    /// a `FallbackChain` actually answers) — kept as its own variant for
    /// future tracing/observability parity with `Ollama`/`Claude`.
    Routstr,
}

impl std::fmt::Display for BackendId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendId::Ollama => write!(f, "ollama"),
            BackendId::Claude => write!(f, "claude"),
            BackendId::Routstr => write!(f, "routstr"),
        }
    }
}

/// D-04's per-call fallback: try `primary`'s `send()`, and on a transport
/// error fall through to `secondary` for that SAME call rather than
/// failing the whole turn — a local model that answers earlier turns and
/// then drops off mid-loop (Ollama restarted, OOM-killed, network blip)
/// still completes the turn via Claude instead of surfacing an error to
/// the user. Generic over both legs so tests can exercise the fallthrough
/// with lightweight stub backends instead of a live network leg.
struct FallbackChain {
    primary: Box<dyn Backend>,
    primary_id: BackendId,
    secondary: Box<dyn Backend>,
}

#[async_trait]
impl Backend for FallbackChain {
    async fn send(
        &self,
        system: &str,
        tools: &[ToolDef],
        history: &[ChatMessage],
    ) -> Result<BackendTurn> {
        match self.primary.send(system, tools, history).await {
            Ok(turn) => Ok(turn),
            Err(e) => {
                tracing::warn!(
                    backend = %self.primary_id,
                    error = %e,
                    "D-04: backend transport error mid-turn — falling through to the next backend rather than failing this turn"
                );
                self.secondary.send(system, tools, history).await
            }
        }
    }
}

/// Pure decision logic for D-04's leading leg — whether Ollama should be
/// selected given the two facts `select_backend` gathers from live probes
/// (`detect_ollama()`/`ollama::model_supports_tools()`). Extracted so the
/// decision table itself is directly testable without faking a network
/// seam for both probes.
fn ollama_is_selectable(detected: bool, tool_capable: bool) -> bool {
    detected && tool_capable
}

/// D-04's complete backend chain: local Ollama first (node data never
/// leaves the node when a local model is available and tool-capable),
/// Claude second, and Routstr — a Nostr-discovered, Cashu-paid provider —
/// third, reached only when the first two are unavailable AND the operator
/// has actually authorized spending (D-05: a ZERO allowance means Routstr
/// is never even selected, not selected-and-then-declined).
///
/// Reuses `detect_ollama()` — the SAME probe `mesh.assistant-status`
/// reports — rather than writing a second one. Ollama being unreachable OR
/// its configured model being reachable-but-not-tool-capable both fall
/// through to Claude, each with a logged reason — never a silent,
/// tools-free degrade.
pub async fn select_backend(handler: &RpcHandler) -> (Box<dyn Backend>, BackendId) {
    let data_dir = handler.data_dir();
    let (detected, _models) = crate::api::rpc::mesh::assistant::detect_ollama().await;
    let model = ollama::OLLAMA_DEFAULT_MODEL;
    let tool_capable = if detected {
        ollama::model_supports_tools(ollama::OLLAMA_BASE_URL, model).await
    } else {
        false
    };

    if ollama_is_selectable(detected, tool_capable) {
        tracing::info!(
            model,
            "D-04: local Ollama selected — node data stays on-node this turn"
        );
        let primary =
            ollama::OllamaBackend::new(ollama::OLLAMA_BASE_URL.to_string(), model.to_string());
        let secondary = claude::ClaudeBackend::new(data_dir.to_path_buf());
        let chain = FallbackChain {
            primary: Box::new(primary),
            primary_id: BackendId::Ollama,
            secondary: Box::new(secondary),
        };
        return (Box::new(chain), BackendId::Ollama);
    }

    if !detected {
        tracing::info!("D-04: Ollama not detected — falling through to Claude");
    } else {
        tracing::info!(
            model,
            "D-04: Ollama detected but the configured model is not tool-capable — falling through to Claude"
        );
        // G-B3/T-13-83: Ollama IS up (reachable) — this is exactly the
        // "cloud used even though local is up" case the owner must see,
        // even though the reason this time is capability, not health.
        crate::assistant::global_counters().note_cloud_escalation_while_local_up(&format!(
            "Ollama is reachable but its configured model ({model}) is not tool-capable"
        ));
    }

    // D-04's third leg: Routstr, reached only when Ollama isn't selectable
    // — and only when the operator has actually authorized spending. The
    // budget is read fresh HERE, once, at the start of the turn — the
    // policy `RoutstrBackend` pays against for the WHOLE turn is fixed at
    // this point, before any model output exists yet (D-05's "not a
    // function of model output" property).
    let budget = crate::assistant::AssistantBudget::load(data_dir).await;
    if budget.allowance_sats == 0 {
        tracing::info!("D-04: Routstr allowance is zero — Claude alone, Routstr not selected");
        return (
            Box::new(claude::ClaudeBackend::new(data_dir.to_path_buf())),
            BackendId::Claude,
        );
    }

    let policy = budget.payment_policy();
    let accepted_mints = crate::wallet::ecash::load_accepted_mints(data_dir)
        .await
        .map(|m| m.mints)
        .unwrap_or_default();
    let tor_proxy = handler.nostr_tor_proxy();
    let routstr_backend =
        routstr::RoutstrBackend::new(data_dir.to_path_buf(), policy, accepted_mints, tor_proxy);
    let chain = FallbackChain {
        primary: Box::new(claude::ClaudeBackend::new(data_dir.to_path_buf())),
        primary_id: BackendId::Claude,
        secondary: Box::new(routstr_backend),
    };
    (Box::new(chain), BackendId::Claude)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::tools::{ChatMessage, ToolDef};

    struct ErroringBackend;
    #[async_trait]
    impl Backend for ErroringBackend {
        async fn send(
            &self,
            _system: &str,
            _tools: &[ToolDef],
            _history: &[ChatMessage],
        ) -> Result<BackendTurn> {
            anyhow::bail!("stub transport error")
        }
    }

    struct OkBackend(&'static str);
    #[async_trait]
    impl Backend for OkBackend {
        async fn send(
            &self,
            _system: &str,
            _tools: &[ToolDef],
            _history: &[ChatMessage],
        ) -> Result<BackendTurn> {
            Ok(BackendTurn::Text(self.0.to_string()))
        }
    }

    /// Behavior: an Ollama transport error falls through to the next
    /// backend rather than failing the turn.
    #[tokio::test]
    async fn ollama_transport_error_falls_through_to_next_backend() {
        let chain = FallbackChain {
            primary: Box::new(ErroringBackend),
            primary_id: BackendId::Ollama,
            secondary: Box::new(OkBackend("answered by claude")),
        };
        let result = chain
            .send("sys", &[], &[])
            .await
            .expect("falls through, does not fail the turn");
        assert!(matches!(result, BackendTurn::Text(t) if t == "answered by claude"));
    }

    #[tokio::test]
    async fn healthy_primary_never_reaches_secondary() {
        let chain = FallbackChain {
            primary: Box::new(OkBackend("answered by ollama")),
            primary_id: BackendId::Ollama,
            secondary: Box::new(ErroringBackend),
        };
        let result = chain.send("sys", &[], &[]).await.expect("primary answers");
        assert!(matches!(result, BackendTurn::Text(t) if t == "answered by ollama"));
    }

    /// Behavior: `select_backend` returns Ollama when reachable and
    /// tool-capable; falls through to Claude when unreachable, and also
    /// when reachable but not tool-capable.
    #[test]
    fn ollama_is_selectable_truth_table() {
        assert!(
            ollama_is_selectable(true, true),
            "reachable + tool-capable => selectable"
        );
        assert!(
            !ollama_is_selectable(false, true),
            "unreachable => falls through to Claude"
        );
        assert!(
            !ollama_is_selectable(true, false),
            "reachable but not tool-capable => falls through to Claude"
        );
        assert!(
            !ollama_is_selectable(false, false),
            "neither => falls through to Claude"
        );
    }
}
