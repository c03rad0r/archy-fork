//! `assistant.*` RPC surface (D-01/D-02) — the front door onto the shared
//! assistant service in `crate::assistant`. Every later `assistant.*`
//! method (13-05's `list-tools`/`grants-*`, 13-08's `confirm-tool`, 13-10's
//! `history`) is added inside this file; `dispatcher.rs` registers exactly
//! one guarded arm for the whole `assistant.` prefix (see
//! `grep -c 'starts_with("assistant.")' dispatcher.rs` == 1), never a new
//! per-method literal arm.

use super::RpcHandler;
use anyhow::Result;
use std::sync::Arc;

impl RpcHandler {
    /// Prefix sub-dispatcher for `assistant.*`. Reached only after the
    /// caller has already passed the session-cookie + CSRF +
    /// `role.can_access()` gate in `api/rpc/mod.rs:264-330` — no bespoke
    /// auth here (asserted by
    /// `assistant::loop_::tests::assistant_methods_require_session`, which
    /// confirms `assistant.*` is absent from `UNAUTHENTICATED_METHODS`).
    pub(in crate::api::rpc) async fn handle_assistant(
        self: &Arc<Self>,
        method: &str,
        params: Option<serde_json::Value>,
        session_token: &Option<String>,
    ) -> Result<serde_json::Value> {
        match method {
            "assistant.chat" => self.handle_assistant_chat(params, session_token).await,
            "assistant.list-tools" => self.handle_assistant_list_tools().await,
            "assistant.grants-get" => self.handle_assistant_grants_get().await,
            "assistant.grants-set" => self.handle_assistant_grants_set(params).await,
            "assistant.pending" => self.handle_assistant_pending().await,
            "assistant.confirm-tool" => self.handle_assistant_confirm_tool(params).await,
            "assistant.history" => self.handle_assistant_history(session_token).await,
            "assistant.clear-history" => self.handle_assistant_clear_history(session_token).await,
            "assistant.budget-get" => self.handle_assistant_budget_get().await,
            "assistant.budget-set" => self.handle_assistant_budget_set(params).await,
            other => anyhow::bail!("no such assistant method: {other}"),
        }
    }

    /// assistant.history — the calling session's own transcript (D-08/D-02):
    /// the running summary of everything compacted out of the verbatim
    /// window, plus the verbatim recent turns. Scoped to the caller's own
    /// `HistoryKey` — never any other caller's transcript (the same
    /// structural separation `operator_and_mesh_transcripts_are_separate`
    /// asserts in `history.rs`).
    async fn handle_assistant_history(
        self: &Arc<Self>,
        session_token: &Option<String>,
    ) -> Result<serde_json::Value> {
        let session_id = session_token.clone().unwrap_or_default();
        let caller = crate::assistant::CallerScope::LocalOperator { session_id };
        let key = crate::assistant::history::HistoryKey::from_caller(&caller);
        let hist = crate::assistant::history::History::load(self.data_dir(), &key).await;
        Ok(serde_json::json!({
            "summary": hist.summary,
            "turns": hist.recent(),
        }))
    }

    /// assistant.clear-history — delete the calling session's own
    /// transcript file. Never touches any other caller's file — each
    /// caller has a structurally distinct `HistoryKey`-derived path.
    async fn handle_assistant_clear_history(
        self: &Arc<Self>,
        session_token: &Option<String>,
    ) -> Result<serde_json::Value> {
        let session_id = session_token.clone().unwrap_or_default();
        let caller = crate::assistant::CallerScope::LocalOperator { session_id };
        let key = crate::assistant::history::HistoryKey::from_caller(&caller);
        crate::assistant::history::History::clear(self.data_dir(), &key).await?;
        Ok(serde_json::json!({ "cleared": true }))
    }

    /// assistant.pending — the current pending destructive-tool
    /// confirmation, if any: the node-authored description and the
    /// node-minted nonce. This is how the trusted chrome *fetches* the
    /// dialog text over the authenticated RPC session rather than
    /// receiving it from the iframe (D-11) — the iframe has no path into
    /// this text and no way to answer it.
    async fn handle_assistant_pending(self: &Arc<Self>) -> Result<serde_json::Value> {
        let gate = crate::assistant::confirm::global();
        Ok(match gate.peek() {
            Some(p) => serde_json::json!({
                "pending": {
                    "req_id": p.req_id,
                    "nonce": p.nonce,
                    "description": p.description,
                    "tool_name": p.tool_name,
                }
            }),
            None => serde_json::json!({ "pending": null }),
        })
    }

    /// assistant.confirm-tool — resolve a pending confirmation. Params:
    /// `{ "req_id": string, "nonce": string, "approved": bool }`. The
    /// nonce must be the node-minted one for exactly that pending action
    /// (S-02); a mismatched or replayed nonce is refused loudly — a
    /// mismatch can only mean a replay attempt or a trusted-chrome bug, so
    /// it is logged at error level and surfaced to the caller as an error,
    /// not swallowed as a toast.
    async fn handle_assistant_confirm_tool(
        self: &Arc<Self>,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let req_id = params
            .get("req_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing req_id"))?;
        let nonce = params
            .get("nonce")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing nonce"))?;
        let approved = params
            .get("approved")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("Missing approved"))?;

        let gate = crate::assistant::confirm::global();
        match gate.resolve(req_id, nonce, approved) {
            Ok(()) => Ok(serde_json::json!({ "resolved": true, "approved": approved })),
            Err(refusal) => {
                tracing::error!(%req_id, %refusal, "assistant.confirm-tool refused");
                anyhow::bail!("confirmation refused: {refusal}")
            }
        }
    }

    /// assistant.list-tools — the tools currently visible to the local
    /// operator (i.e. whose category is currently granted), each with its
    /// category and destructive flag, so neode-ui can render an honest
    /// capability list rather than guessing from the grants alone.
    async fn handle_assistant_list_tools(self: &Arc<Self>) -> Result<serde_json::Value> {
        let grants = crate::assistant::grants::Grants::load(self.data_dir()).await;
        let registry = crate::assistant::tools::registry();
        let visible = registry.visible_to(grants.categories());
        let tools: Vec<serde_json::Value> = visible
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "category": t.category,
                    "destructive": t.destructive,
                })
            })
            .collect();
        Ok(serde_json::json!({ "tools": tools }))
    }

    /// assistant.grants-get — the current open/closed state of all ten
    /// D-16 permission categories, for the browser's AI-permissions UI to
    /// render honestly (including the ones still closed).
    async fn handle_assistant_grants_get(self: &Arc<Self>) -> Result<serde_json::Value> {
        let grants = crate::assistant::grants::Grants::load(self.data_dir()).await;
        Ok(serde_json::json!({ "categories": grants_categories_json(&grants) }))
    }

    /// assistant.grants-set — open or close one permission category.
    /// Params: `{ "category": string, "granted": bool }`, where `category`
    /// is one of the ten kebab-case ids also used by
    /// `neode-ui/src/stores/aiPermissions.ts` (`"apps"`, `"ai-local"`,
    /// etc). Persists immediately via `Grants::save` — the next
    /// `assistant.chat` turn (not only the next session) sees the change,
    /// per `grant_revocation_takes_effect_next_turn`.
    async fn handle_assistant_grants_set(
        self: &Arc<Self>,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let category_str = params
            .get("category")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing category"))?;
        let granted = params
            .get("granted")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("Missing granted"))?;

        let category: crate::assistant::PermissionCategory =
            serde_json::from_value(serde_json::Value::String(category_str.to_string()))
                .map_err(|_| anyhow::anyhow!("Unknown permission category: {category_str}"))?;

        let mut grants = crate::assistant::grants::Grants::load(self.data_dir()).await;
        grants.set(category, granted);
        grants.save(self.data_dir()).await?;

        // Keep the legacy UI-store file in step — same downgrade-safety as
        // `ai.permissions.set` — so no reader of either file sees a stale
        // set, and a future `ai_grants_unified` migration (should the
        // grants file ever be removed) cannot resurrect an old state.
        let names: Vec<String> = grants
            .categories()
            .iter()
            .filter_map(|c| serde_json::to_value(c).ok()?.as_str().map(str::to_string))
            .collect();
        let _ = crate::settings::ai_permissions::save(
            self.data_dir(),
            crate::settings::ai_permissions::AiPermissions { granted: names },
        )
        .await;

        Ok(serde_json::json!({ "categories": grants_categories_json(&grants) }))
    }

    /// assistant.budget-get — the operator's current Routstr allowance
    /// (D-05): the total sats authorized this period, the amount spent so
    /// far, and the remainder. Never touched by anything model-influenced —
    /// this is purely a read of the persisted [`crate::assistant::AssistantBudget`].
    async fn handle_assistant_budget_get(self: &Arc<Self>) -> Result<serde_json::Value> {
        let budget = crate::assistant::AssistantBudget::load(self.data_dir()).await;
        Ok(serde_json::json!({
            "allowance_sats": budget.allowance_sats,
            "spent_sats": budget.spent_sats,
            "remaining_sats": budget.remaining_sats(),
        }))
    }

    /// assistant.budget-set — the operator sets (or raises) the prepaid
    /// Routstr allowance. Params: `{ "allowance_sats": u64 }`. Only the
    /// allowance changes here; `spent_sats` is left untouched, so raising
    /// the allowance after an exhaustion stop (D-05's "offering to top up")
    /// simply widens the remainder rather than resetting the period's
    /// spend history. This is the ONLY place `AssistantBudget.allowance_sats`
    /// is ever written — never from a tool call, never from model output
    /// (D-05: the ceiling is operator-set, full stop).
    async fn handle_assistant_budget_set(
        self: &Arc<Self>,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let allowance_sats = params
            .get("allowance_sats")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing allowance_sats"))?;

        let mut budget = crate::assistant::AssistantBudget::load(self.data_dir()).await;
        budget.allowance_sats = allowance_sats;
        budget.save(self.data_dir()).await?;

        Ok(serde_json::json!({
            "allowance_sats": budget.allowance_sats,
            "spent_sats": budget.spent_sats,
            "remaining_sats": budget.remaining_sats(),
        }))
    }

    /// assistant.chat — a single chat turn from the authenticated local
    /// operator. Params: `{ "text": string }`. Returns `{ "text": string }`.
    async fn handle_assistant_chat(
        self: &Arc<Self>,
        params: Option<serde_json::Value>,
        session_token: &Option<String>,
    ) -> Result<serde_json::Value> {
        let text = params
            .as_ref()
            .and_then(|p| p.get("text"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("text is required"))?;

        // The caller's authenticated session identifies this LocalOperator —
        // authority is resolved node-side from CallerScope, never from
        // anything the browser or the model asserts about itself.
        let session_id = session_token.clone().unwrap_or_default();

        // G-B3 / 13-12 Task 3: assistant.chat is rate-limited per
        // authenticated session — the practical brake on an
        // injection-driven loop that keeps re-invoking chat itself, and
        // the compensating control RESEARCH Open Question 2 names for the
        // same-origin iframe residual risk 13-09's CSP does not fully
        // close.
        if !self.endpoint_rate_limiter.check_session(&session_id).await {
            anyhow::bail!(
                "assistant.chat rate limit exceeded for this session — please wait before \
                 sending more messages"
            );
        }
        self.endpoint_rate_limiter
            .record_session_request(&session_id)
            .await;
        if self
            .endpoint_rate_limiter
            .session_soft_threshold_reached(&session_id)
            .await
        {
            crate::assistant::global_counters().owner_notice(
                crate::assistant::OwnerNoticeKind::Ux,
                "This session has sent a high number of assistant messages recently.",
            );
        }

        let caller = crate::assistant::CallerScope::LocalOperator { session_id };

        let outcome = crate::assistant::chat_with_surfaces(Arc::clone(self), caller, text).await?;
        // `surfaces` carries the structured results of any content tools
        // this turn ran, so the browser can render them as a grid instead
        // of leaving the surface empty beside a correct prose answer.
        // Always present (possibly empty) so the caller never has to
        // distinguish "no content" from "old node".
        Ok(serde_json::json!({
            "text": outcome.text,
            "surfaces": outcome.surfaces,
            "refused_categories": outcome.refused_categories,
        }))
    }

    /// Internal-only bridge: executes a curated assistant tool against the
    /// SAME `RpcHandler` method every authenticated RPC caller dispatches
    /// through (never an AI-only backdoor). NOT itself an RPC method — only
    /// `assistant::tools::dispatch` calls this, and only for the RPC method
    /// names its own hand-written match arms name explicitly (D-06 — this
    /// list is exactly as curated as the tool registry itself; it is not a
    /// general-purpose relay onto `api::rpc`'s method table).
    pub(crate) async fn assistant_dispatch_tool(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        match method {
            "system.disk-status" => self.handle_system_disk_status().await,
            "system.stats" => self.handle_system_stats().await,
            "container-list" => self.handle_container_list().await,
            "container-logs" => self.handle_container_logs(params).await,
            "container-start" => self.handle_container_start(params).await,
            "container-stop" => self.handle_container_stop(params).await,
            "container-restart" => self.handle_container_restart(params).await,
            "bitcoin.getinfo" => self.handle_bitcoin_getinfo().await,
            "network.get-visibility" => self.handle_network_get_visibility().await,
            "network.diagnostics" => self.handle_network_diagnostics().await,
            "network.set-visibility" => self.handle_network_set_visibility(params).await,
            "network.set-wifi-radio" => self.handle_network_set_wifi_radio(params).await,
            "mesh.status" => self.handle_mesh_status().await,
            "content.list-mine" => self.handle_content_list_mine().await,
            // The other three `content_list` scopes (peers/purchased/films).
            // Their absence here — while `tools.rs`'s scope match named them
            // — made every non-"own" scope fail with "no such handler", which
            // reads downstream as "the peers have no content" when in truth
            // the tool never ran. Curated one-by-one, same as every arm above.
            "content.browse-all-peers" => self.handle_content_browse_all_peers().await,
            "content.owned-list" => self.handle_content_owned_list().await,
            "content.indeehub-projects" => self.handle_content_indeehub_projects().await,
            "system.settings.get" => self.handle_system_settings_get(params).await,
            "system.settings.set" => self.handle_system_settings_set(params).await,
            "system.kiosk-display.get" => self.handle_system_kiosk_display_get().await,
            "system.kiosk-display.set" => self.handle_system_kiosk_display_set(params).await,
            "bitcoin.relay-update-settings" => {
                self.handle_bitcoin_relay_update_settings(params).await
            }
            other => anyhow::bail!("assistant_dispatch_tool: no such handler for {other}"),
        }
    }

    /// Spawn-capable sibling of `assistant_dispatch_tool` for the async
    /// lifecycle methods — install/uninstall take minutes and return a
    /// started-handle immediately. Same curation discipline (D-06): exactly
    /// the two methods named below, reached only through
    /// `assistant::tools::dispatch`'s `app_install`/`app_uninstall` arms,
    /// behind the 13-08 confirm gate.
    pub(crate) async fn assistant_dispatch_tool_spawn(
        self: Arc<Self>,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        match method {
            "package.install" => self.spawn_package_install(params).await,
            "package.uninstall" => self.spawn_package_uninstall(params).await,
            other => anyhow::bail!("assistant_dispatch_tool_spawn: no such handler for {other}"),
        }
    }

    /// Read-only `data_dir` accessor for `crate::assistant`, which lives
    /// outside `api::rpc`'s module tree and so cannot read the private
    /// `config` field directly. Minimal, `pub(crate)`, no behavior change.
    pub(crate) fn data_dir(&self) -> &std::path::Path {
        &self.config.data_dir
    }

    /// Read-only accessor for the node's configured Nostr Tor-proxy address
    /// (`ARCHIPELAGO_NOSTR_TOR_PROXY`) — used by `assistant::backends::select_backend`
    /// to decide whether the Routstr leg should prefer an onion endpoint.
    /// Owned `Option<String>` (not a borrow) so the caller can carry it
    /// into a `RoutstrBackend` that outlives this borrow of `self`.
    pub(crate) fn nostr_tor_proxy(&self) -> Option<String> {
        self.config.nostr_tor_proxy.clone()
    }
}

/// Shared shape for `assistant.grants-get` and `assistant.grants-set`'s
/// response: all ten categories, each with its current `granted` state —
/// never only the open ones, so the browser can render an honest
/// still-closed list too.
fn grants_categories_json(grants: &crate::assistant::grants::Grants) -> serde_json::Value {
    let categories: Vec<serde_json::Value> = crate::assistant::PermissionCategory::ALL
        .iter()
        .map(|c| serde_json::json!({ "category": c, "granted": grants.allows(*c) }))
        .collect();
    serde_json::json!(categories)
}
