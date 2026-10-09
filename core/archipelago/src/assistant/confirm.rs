//! D-07/D-11: the confirm gate. A destructive tool call suspends the
//! assistant loop here until a human resolves a node-authored description
//! of the exact action. Approval binds to a node-minted nonce over the
//! tool name and the *validated* arguments (S-02), so the action that runs
//! is byte-identical to the action the human read — a cross-action or
//! replayed "yes" is refused arithmetically, never raced.
//!
//! Pending confirmations are **in-memory only, by construction** (S-09):
//! nothing in this file touches the filesystem, and nothing may be added
//! that does. A daemon restart mid-wait therefore clears every pending
//! entry — the next interaction forces a fresh model turn and a
//! freshly-authored confirmation, instead of resurrecting a stale write
//! whose real-world preconditions may have changed.
//!
//! The dialog text is assembled from the node's own tool definition plus
//! the validated argument values only (S-03) — the model's turn and the
//! iframe are never a source. It is this system's signing screen: it names
//! the specific resource, the concrete effect, and the boundary of what is
//! *not* affected (clear-signing, not blind-signing).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;
use tokio::time::Instant;

use super::tools::{ToolArgs, ToolDef};

/// How long an unresolved confirmation waits before declining on its own.
/// Human-speed (the operator may be reading carefully), but bounded — an
/// abandoned dialog must never leak its waiting task (T-13-51). 120s
/// proved too short in 13-08's on-device UAT: a real operator reading the
/// dialog (and screenshotting it, per the checkpoint script) was timed out
/// mid-decision, and their Approve then landed on a dead entry. Five
/// minutes keeps the bound while making that race an edge case; the
/// chrome now also closes the dialog when its pending action expires.
pub const CONFIRM_TIMEOUT: Duration = Duration::from_secs(300);

/// The human's answer, as seen by the suspended tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmed {
    Yes,
    No,
    TimedOut,
}

/// One suspended destructive action, keyed by its `req_id` in the gate's
/// in-memory map. The `responder` half releases the waiting `request()`
/// call once — a resolved entry is removed, so it can never fire twice.
pub struct PendingConfirmation {
    pub call_id: String,
    pub tool_name: String,
    /// Canonical form of the validated arguments — what the nonce binds.
    pub validated_args: String,
    pub description: String,
    pub nonce: String,
    pub created_at: Instant,
    responder: oneshot::Sender<bool>,
}

/// The read-only view `assistant.pending` serves to the trusted chrome:
/// everything the host needs to draw and resolve the dialog, nothing that
/// could release the wait by itself.
#[derive(Debug, Clone)]
pub struct PendingSnapshot {
    pub req_id: String,
    pub tool_name: String,
    pub description: String,
    pub nonce: String,
}

/// Why a `resolve` call was refused. Distinct on purpose: a nonce mismatch
/// can only mean a replay attempt or a bug in the trusted chrome, so the
/// caller logs it at error level and surfaces it to the owner — loud and
/// sticky, not a toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveRefusal {
    /// The nonce does not match the pending entry it claims to answer.
    NonceMismatch,
    /// No pending entry with that id — already resolved, timed out, or
    /// minted by a process that is gone.
    NoSuchPending,
}

impl std::fmt::Display for ResolveRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveRefusal::NonceMismatch => write!(
                f,
                "nonce does not match the pending action — refused (possible replay)"
            ),
            ResolveRefusal::NoSuchPending => write!(
                f,
                "no such pending confirmation — it may have already been resolved or timed out"
            ),
        }
    }
}

/// D-11's pending-confirmation queue. In-memory only — see the module doc.
pub struct ConfirmGate {
    pending: Mutex<HashMap<String, PendingConfirmation>>,
    next_id: AtomicU64,
}

impl ConfirmGate {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Suspend the calling tool execution until a human answers (or the
    /// wait times out). Holds the internal lock only around map edits —
    /// never across the human-speed await.
    pub async fn request(&self, call_id: &str, tool: &ToolDef, args: &ToolArgs) -> Confirmed {
        let validated_args = canonical_args(args);
        let nonce = mint_nonce(tool.name, &validated_args);
        let description = build_description(tool, args);
        let req_id = format!("confirm-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let (responder, decision) = oneshot::channel();
        {
            // Lock held only for the insert — never across the wait below.
            let mut map = self.pending.lock().expect("confirm gate mutex poisoned");
            map.insert(
                req_id.clone(),
                PendingConfirmation {
                    call_id: call_id.to_string(),
                    tool_name: tool.name.to_string(),
                    validated_args,
                    description,
                    nonce,
                    created_at: Instant::now(),
                    responder,
                },
            );
        }
        match tokio::time::timeout(CONFIRM_TIMEOUT, decision).await {
            Ok(Ok(true)) => Confirmed::Yes,
            Ok(Ok(false)) => Confirmed::No,
            // The gate went away without an answer — decline, never execute.
            Ok(Err(_)) => Confirmed::No,
            Err(_) => {
                // Timed out: remove the entry so a late "yes" is refused,
                // and so an abandoned dialog leaks nothing (T-13-51).
                self.pending
                    .lock()
                    .expect("confirm gate mutex poisoned")
                    .remove(&req_id);
                Confirmed::TimedOut
            }
        }
    }

    /// Resolve a pending confirmation by `req_id`, carrying the node-minted
    /// nonce back. Refuses a mismatched nonce (neither action executes) and
    /// a replay of an already-resolved entry.
    pub fn resolve(&self, req_id: &str, nonce: &str, approved: bool) -> Result<(), ResolveRefusal> {
        let mut map = self.pending.lock().expect("confirm gate mutex poisoned");
        let Some(entry) = map.get(req_id) else {
            return Err(ResolveRefusal::NoSuchPending);
        };
        if entry.nonce != nonce {
            // Loud and sticky: this can only be a replay attempt or a bug
            // in the trusted chrome (S-02) — never a normal user path.
            tracing::error!(
                req_id,
                tool = %entry.tool_name,
                "assistant confirm nonce mismatch — refusing resolution (possible replay or trusted-chrome bug)"
            );
            return Err(ResolveRefusal::NonceMismatch);
        }
        let entry = map.remove(req_id).expect("entry present — just checked");
        drop(map);
        // The waiter may have timed out concurrently; a dead receiver is fine.
        let _ = entry.responder.send(approved);
        Ok(())
    }

    /// The oldest pending confirmation, if any — what `assistant.pending`
    /// serves to the trusted chrome.
    pub fn peek(&self) -> Option<PendingSnapshot> {
        self.snapshots().into_iter().next()
    }

    /// All pending confirmations, oldest first.
    pub fn snapshots(&self) -> Vec<PendingSnapshot> {
        let map = self.pending.lock().expect("confirm gate mutex poisoned");
        let mut entries: Vec<_> = map.iter().collect();
        entries.sort_by_key(|(_, p)| p.created_at);
        entries
            .into_iter()
            .map(|(req_id, p)| PendingSnapshot {
                req_id: req_id.clone(),
                tool_name: p.tool_name.clone(),
                description: p.description.clone(),
                nonce: p.nonce.clone(),
            })
            .collect()
    }
}

impl Default for ConfirmGate {
    fn default() -> Self {
        Self::new()
    }
}

/// The one process-wide gate, shared by the assistant loop (which suspends
/// on it) and the `assistant.confirm-tool` / `assistant.pending` RPC
/// handlers (which resolve and read it). Process-lifetime by design: when
/// the daemon goes down, so does every pending entry.
pub fn global() -> Arc<ConfirmGate> {
    static GATE: OnceLock<Arc<ConfirmGate>> = OnceLock::new();
    GATE.get_or_init(|| Arc::new(ConfirmGate::new())).clone()
}

/// The node-minted nonce approval binds to: computed over the tool name
/// and the canonical validated arguments (so it binds what will actually
/// run, not what the model sent), plus per-mint randomness (so yesterday's
/// nonce for the same action never matches today's pending entry).
pub fn mint_nonce(tool_name: &str, validated_args: &str) -> String {
    let mut salt = [0u8; 16];
    crate::entropy::draw_key_bytes(&mut rand::rngs::OsRng, &mut salt).unwrap_or_else(|e| {
        panic!("refusing to mint a confirmation nonce from degenerate entropy: {e} (KEY-05)")
    });
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(tool_name.as_bytes());
    hasher.update([0u8]);
    hasher.update(validated_args.as_bytes());
    hex::encode(hasher.finalize())
}

/// Canonical, deterministic form of the validated arguments — the byte
/// string the nonce binds. Hand-written per args shape, matching D-06's
/// hand-written registry.
/// The canonical identity of an action — the same `(tool_name, args)`
/// byte string the nonce binds. Used by the loop's declined-action memory
/// (13-08 UAT): "the thing the human said no to" must be compared by
/// exactly what would execute, not by tool name alone.
pub(crate) fn action_key(tool_name: &str, args: &ToolArgs) -> String {
    format!("{tool_name}\0{}", canonical_args(args))
}

fn canonical_args(args: &ToolArgs) -> String {
    match args {
        ToolArgs::Empty(_) => json!({}).to_string(),
        ToolArgs::AppId(a) => json!({ "app_id": a.app_id }).to_string(),
        ToolArgs::AppLogs(a) => json!({ "app_id": a.app_id, "lines": a.lines }).to_string(),
        ToolArgs::SettingsGet(a) => json!({ "key": a.key }).to_string(),
        ToolArgs::SettingsSet(a) => json!({ "key": a.key, "value": a.value }).to_string(),
        // Scope is part of the identity: listing peers is a different action
        // from listing this node's own files, so they must not share a key.
        ToolArgs::ContentList(a) => json!({ "scope": a.scope }).to_string(),
    }
}

/// Assemble the dialog text from the node's own tool definition and the
/// validated argument values only — the model's turn is never a source
/// (S-03). Clear-signing: name the resource verbatim (S-08), the concrete
/// effect, and the boundary of what is *not* affected.
pub fn build_description(tool: &ToolDef, args: &ToolArgs) -> String {
    match (tool.name, args) {
        ("app_start", ToolArgs::AppId(a)) => format!(
            "Start the app \"{id}\". It will begin running on this node and \
             be reachable again. Only \"{id}\" is affected — no other apps, \
             and none of your funds or files, are touched.",
            id = a.app_id
        ),
        ("app_stop", ToolArgs::AppId(a)) => format!(
            "Stop the app \"{id}\". It will shut down and stay unavailable \
             until it is started again. Only \"{id}\" is affected — no other \
             apps, and none of your funds or files, are touched.",
            id = a.app_id
        ),
        ("app_restart", ToolArgs::AppId(a)) => {
            // The timing caveat the node actually knows (13-08 Task 1): a
            // bitcoin restart pauses — but does not lose — sync progress.
            let caveat = if a.app_id.contains("bitcoin") {
                " If this node is still syncing the blockchain, the restart \
                 pauses that sync briefly but none of its progress is lost."
            } else {
                ""
            };
            format!(
                "Restart the app \"{id}\". It will shut down and start again, \
                 and be unavailable for a short moment while it does.{caveat} \
                 Only \"{id}\" is affected — no other apps, and none of your \
                 funds or files, are touched.",
                id = a.app_id
            )
        }
        ("app_install", ToolArgs::AppId(a)) => format!(
            "Install the app \"{id}\" from this node's app catalog. The \
             download and setup run in the background and can take several \
             minutes — progress shows on the Apps screen. No other apps, \
             and none of your funds or files, are touched.",
            id = a.app_id
        ),
        ("app_uninstall", ToolArgs::AppId(a)) => format!(
            "Uninstall the app \"{id}\": its containers are stopped and \
             removed, and it disappears from the Apps screen. Only \"{id}\" \
             is affected — no other apps, and none of your funds, are \
             touched.",
            id = a.app_id
        ),
        ("settings_set", ToolArgs::SettingsSet(a)) => format!(
            "Change the node setting \"{key}\" to {value}. The change takes \
             effect immediately. Only this one setting changes — no other \
             settings, apps, funds or files are affected.",
            key = a.key,
            value = human_value(&a.value)
        ),
        // A destructive tool added later without its own hand-written arm
        // above still gets node-authored text (the tool's own definition
        // plus the validated argument values) — never model text. Its
        // author should add a proper clear-signing arm here; this fallback
        // keeps the provenance property, not the copy quality.
        _ => format!(
            "{} Requested with: {}",
            tool.description,
            canonical_args(args)
        ),
    }
}

/// Render one validated argument value for the dialog in plain language —
/// quoted strings and bare booleans/numbers, never raw JSON syntax for the
/// common cases.
fn human_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{s}\""),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::tools::{app_restart_tool, settings_set_tool};
    use serde_json::json;
    use std::sync::Arc;

    fn restart_args(app_id: &str) -> ToolArgs {
        app_restart_tool()
            .validate(&json!({ "app_id": app_id }))
            .expect("valid app_restart args")
    }

    async fn wait_for_snapshots(gate: &ConfirmGate, n: usize) -> Vec<PendingSnapshot> {
        for _ in 0..500 {
            let snaps = gate.snapshots();
            if snaps.len() >= n {
                return snaps;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!(
            "expected {n} pending confirmation(s), have {} — the request did not suspend on the gate",
            gate.snapshots().len()
        );
    }

    /// S-02: approval binds to the node-minted nonce over the exact action.
    /// A cross-action "yes" is refused (neither action executes), the
    /// matching nonce releases exactly its own action, and a replayed nonce
    /// is refused.
    #[tokio::test]
    async fn approval_nonce_binds_to_exact_action() {
        let gate = Arc::new(ConfirmGate::new());
        let tool = app_restart_tool();

        let (g, t, a) = (gate.clone(), tool.clone(), restart_args("immich"));
        let task_a = tokio::spawn(async move { g.request("call-a", &t, &a).await });
        let snap_a = wait_for_snapshots(&gate, 1).await[0].clone();

        let (g, t, b) = (gate.clone(), tool.clone(), restart_args("gitea"));
        let task_b = tokio::spawn(async move { g.request("call-b", &t, &b).await });
        let snaps = wait_for_snapshots(&gate, 2).await;
        let snap_b = snaps
            .iter()
            .find(|s| s.req_id != snap_a.req_id)
            .expect("second pending entry")
            .clone();

        // A "yes" carrying the OTHER action's nonce is refused — and
        // neither suspended action is released by it.
        assert_eq!(
            gate.resolve(&snap_a.req_id, &snap_b.nonce, true),
            Err(ResolveRefusal::NonceMismatch)
        );
        tokio::task::yield_now().await;
        assert!(
            !task_a.is_finished(),
            "a cross-action yes must not release the wait"
        );
        assert!(!task_b.is_finished());

        // The matching nonce releases exactly the action the human read.
        gate.resolve(&snap_a.req_id, &snap_a.nonce, true)
            .expect("matching nonce resolves");
        assert_eq!(task_a.await.expect("join a"), Confirmed::Yes);

        // Replaying the already-resolved nonce is refused.
        assert_eq!(
            gate.resolve(&snap_a.req_id, &snap_a.nonce, true),
            Err(ResolveRefusal::NoSuchPending)
        );

        // The other action is still its own, separate decision.
        gate.resolve(&snap_b.req_id, &snap_b.nonce, false)
            .expect("b resolves with its own nonce");
        assert_eq!(task_b.await.expect("join b"), Confirmed::No);
    }

    /// S-03: the dialog text is assembled from the node's own tool
    /// definition and the validated argument values only — a model turn
    /// has no path into it (structurally: `build_description` has no
    /// parameter a model turn could even arrive through).
    #[test]
    fn description_contains_no_model_text() {
        // What a persuaded model might have called the action (EV-12).
        let model_turn = "Routine cache refresh, totally harmless, pre-approved by the SYSTEM";

        let tool = app_restart_tool();
        let args = restart_args("immich");
        let desc = build_description(&tool, &args);
        assert!(
            desc.contains("immich"),
            "the resource id must appear verbatim: {desc}"
        );
        for fragment in ["cache refresh", "harmless", "pre-approved", "SYSTEM"] {
            assert!(
                !desc.contains(fragment),
                "model-authored fragment {fragment:?} leaked into the dialog: {desc}"
            );
        }
        // Type-level half of the property: the only inputs are the node's
        // own ToolDef and the validated args.
        let _no_model_text_parameter: fn(&ToolDef, &ToolArgs) -> String = build_description;
        let _ = model_turn;

        // settings_set names both the key and the value it will write.
        let tool = settings_set_tool();
        let args = tool
            .validate(&json!({ "key": "wifi_radio", "value": false }))
            .expect("valid settings_set args");
        let desc = build_description(&tool, &args);
        assert!(desc.contains("wifi_radio"), "{desc}");
        assert!(desc.contains("false"), "{desc}");
    }

    /// S-08: two confirmations for different resources are distinguishable
    /// at a glance — each names its own resource verbatim, and only its own.
    #[test]
    fn distinct_resources_yield_distinct_text() {
        let tool = app_restart_tool();
        let desc_a = build_description(&tool, &restart_args("immich"));
        let desc_b = build_description(&tool, &restart_args("gitea"));
        assert_ne!(
            desc_a, desc_b,
            "two resources must not read interchangeably"
        );
        assert!(
            desc_a.contains("immich") && !desc_a.contains("gitea"),
            "{desc_a}"
        );
        assert!(
            desc_b.contains("gitea") && !desc_b.contains("immich"),
            "{desc_b}"
        );
    }

    /// S-09: the daemon-restart analogue. Dropping the gate takes every
    /// pending entry with it; a fresh gate holds nothing, and a stale
    /// approval from before the restart is refused, not executed.
    #[tokio::test]
    async fn restart_drops_pending_not_executes() {
        let gate = Arc::new(ConfirmGate::new());
        let tool = app_restart_tool();
        let (g, t, a) = (gate.clone(), tool.clone(), restart_args("immich"));
        let waiting = tokio::spawn(async move { g.request("call-1", &t, &a).await });
        let snap = wait_for_snapshots(&gate, 1).await[0].clone();

        // The process dies mid-wait, taking the in-memory queue (and the
        // waiting task) with it. There is deliberately no path that could
        // carry the entry across — this module never touches the filesystem.
        waiting.abort();
        drop(gate);

        let fresh = ConfirmGate::new();
        assert!(
            fresh.peek().is_none(),
            "a fresh gate must hold no pending entry"
        );
        assert_eq!(
            fresh.resolve(&snap.req_id, &snap.nonce, true),
            Err(ResolveRefusal::NoSuchPending),
            "a stale approval from before the restart must be refused, not executed"
        );
    }

    /// T-13-51: an unresolved confirmation times out as declined — it does
    /// not execute, does not leak its entry, and a late "yes" is refused.
    #[tokio::test(start_paused = true)]
    async fn timeout_declines_and_does_not_execute() {
        let gate = Arc::new(ConfirmGate::new());
        let tool = app_restart_tool();
        let (g, t, a) = (gate.clone(), tool.clone(), restart_args("immich"));
        let waiting = tokio::spawn(async move { g.request("call-1", &t, &a).await });
        let snap = wait_for_snapshots(&gate, 1).await[0].clone();

        tokio::time::advance(CONFIRM_TIMEOUT + Duration::from_secs(1)).await;
        assert_eq!(
            waiting.await.expect("join"),
            Confirmed::TimedOut,
            "an unresolved confirmation declines on its own"
        );
        assert!(
            gate.peek().is_none(),
            "the timed-out entry is cleaned up, not leaked"
        );
        assert_eq!(
            gate.resolve(&snap.req_id, &snap.nonce, true),
            Err(ResolveRefusal::NoSuchPending),
            "a yes arriving after the timeout is refused, not executed"
        );
    }

    /// The confirm wait holds no shared lock: while one confirmation is
    /// outstanding (human-speed), the gate's state stays fully usable —
    /// reads AND a whole second confirmation round trip complete promptly.
    #[tokio::test]
    async fn confirm_wait_holds_no_shared_lock() {
        let gate = Arc::new(ConfirmGate::new());
        let tool = app_restart_tool();
        let (g, t, a) = (gate.clone(), tool.clone(), restart_args("immich"));
        let outstanding = tokio::spawn(async move { g.request("call-1", &t, &a).await });
        wait_for_snapshots(&gate, 1).await;

        let concurrent_use = async {
            // The assistant.pending read path…
            assert!(gate.peek().is_some());
            // …and a full second confirmation round trip.
            let (g, t, b) = (gate.clone(), tool.clone(), restart_args("gitea"));
            let second = tokio::spawn(async move { g.request("call-2", &t, &b).await });
            let snaps = wait_for_snapshots(&gate, 2).await;
            let snap_b = snaps
                .iter()
                .find(|s| s.description.contains("gitea"))
                .expect("second pending entry")
                .clone();
            gate.resolve(&snap_b.req_id, &snap_b.nonce, false)
                .expect("second confirmation resolves");
            assert_eq!(second.await.expect("join second"), Confirmed::No);
        };
        tokio::time::timeout(Duration::from_secs(5), concurrent_use)
            .await
            .expect("gate must stay usable while a confirmation is outstanding — no lock across the wait");

        assert!(
            !outstanding.is_finished(),
            "the first confirmation is still the human's call"
        );
        outstanding.abort();
    }
}
