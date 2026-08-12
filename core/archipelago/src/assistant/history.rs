//! D-08: node-side chat persistence under `data_dir`, scoped by caller
//! identity + permission scope (D-02) so an operator's AIUI transcript and
//! a mesh peer's transcript never see each other's history. Follows
//! `streaming/session.rs`'s `data_dir`-scoped persisted-state conventions
//! and `music/index.rs::save_atomic`'s temp-file-then-`rename` write
//! discipline — a crash mid-append leaves the previous transcript intact,
//! never a partial file.
//!
//! **Pending confirmations (`confirm.rs`) are never reachable from this
//! file.** `project`'s only inputs are a completed turn's `ChatMessage`s
//! and a resolved tool-name -> category map — there is no parameter type
//! here through which a `confirm::PendingConfirmation` could ever arrive
//! (see `project_has_no_path_to_pending_confirmation_state`). S-09's
//! in-memory-only property is not weakened by this module; it simply has
//! no path into it to weaken.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::tools::{ChatMessage, Role};
use super::{CallerScope, PermissionCategory};

const HISTORY_DIR: &str = "assistant/history";

/// A tool result longer than this is truncated before entering history,
/// with a visible marker (AI-SPEC §4b.4). A NEW, assistant-scoped
/// constant — deliberately not the mesh module's own 480-character,
/// LoRa-airtime-tuned reply cap of the same shape (AI-SPEC §3 Pitfall 6).
/// Sized for a local model's context-window budget, not radio bandwidth.
pub const MAX_TOOL_RESULT_CHARS: usize = 4000;

/// Turns kept verbatim in the recent window before compaction folds older
/// ones into the running summary (AI-SPEC §4b.4's "keep the last K turns
/// verbatim").
pub const KEEP_VERBATIM_TURNS: usize = 10;

/// Tool categories whose call arguments are never persisted, regardless of
/// what the actual tool call carried — AI-SPEC §7b's field policy applied
/// to on-disk persistence, not only to tracing. No `wallet`/`files`
/// category tool exists in the D-06 registry today, but this list is what
/// keeps that true of the TRANSCRIPT even once one is added later,
/// mirroring `registry_never_exposes_excluded_authority`'s
/// scan-the-whole-set-not-a-review discipline.
const REDACTED_CATEGORIES: [PermissionCategory; 2] =
    [PermissionCategory::Wallet, PermissionCategory::Files];

/// D-02's per-caller history key: a mesh peer's transcript and the local
/// operator's transcript are structurally distinct files, derived from
/// `CallerScope` itself — not two rows an implementer could forget to
/// filter on.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HistoryKey(String);

impl HistoryKey {
    pub fn from_caller(caller: &CallerScope) -> Self {
        match caller {
            CallerScope::LocalOperator { session_id } => {
                HistoryKey(format!("operator-{}", sanitize(session_id)))
            }
            CallerScope::Mesh { peer_id, .. } => HistoryKey(format!("mesh-{}", sanitize(peer_id))),
        }
    }

    fn filename(&self) -> String {
        format!("{}.json", self.0)
    }
}

/// Filesystem-safe form of a caller identifier. Session ids and mesh peer
/// ids are not guaranteed to be path-safe, so anything outside a
/// conservative allowlist is replaced — a narrow allowlist (alnum, `-`,
/// `_`) rather than a broad denylist, since two different raw ids that
/// collide after sanitization would incorrectly share a transcript (the
/// exact property `operator_and_mesh_transcripts_are_separate` guards).
fn sanitize(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PersistedRole {
    System,
    User,
    Assistant,
    Tool,
}

impl From<Role> for PersistedRole {
    fn from(r: Role) -> Self {
        match r {
            Role::System => PersistedRole::System,
            Role::User => PersistedRole::User,
            Role::Assistant => PersistedRole::Assistant,
            Role::Tool => PersistedRole::Tool,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedToolCall {
    pub name: String,
    /// `None` for a `wallet`/`files`-category tool — the argument VALUE
    /// never reaches disk for those categories, regardless of what the
    /// tool call actually carried. `Some` for every other category.
    pub arguments: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedToolResult {
    pub content: String,
    pub is_error: bool,
    /// Whether `content` was truncated from a longer result.
    #[serde(default)]
    pub truncated: bool,
}

/// One persisted turn — a redacted, size-bounded projection of a
/// `ChatMessage`, never the live in-memory type itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedMessage {
    pub role: PersistedRole,
    pub text: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<PersistedToolCall>,
    #[serde(default)]
    pub tool_results: Vec<PersistedToolResult>,
}

/// A persisted transcript: the running summary of everything folded out of
/// the verbatim window, plus the verbatim window itself.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct History {
    /// Extended incrementally as turns age out of the verbatim window —
    /// never regenerated from the full transcript (AI-SPEC §4b.4's
    /// bounded-summarization-cost requirement).
    #[serde(default)]
    pub summary: String,
    /// The most recent turns, kept verbatim, oldest first. Bounded to
    /// `KEEP_VERBATIM_TURNS` by `compact()`.
    #[serde(default)]
    pub turns: Vec<PersistedMessage>,
}

impl History {
    pub fn empty() -> Self {
        Self::default()
    }

    fn path(data_dir: &Path, key: &HistoryKey) -> PathBuf {
        data_dir.join(HISTORY_DIR).join(key.filename())
    }

    /// Load the persisted transcript for `key`. A missing or unparseable
    /// file is an empty history, never an error — a fresh caller (or a
    /// caller whose file predates a schema change) starts clean rather
    /// than blocking the turn.
    pub async fn load(data_dir: &Path, key: &HistoryKey) -> History {
        let path = Self::path(data_dir, key);
        let Ok(content) = tokio::fs::read_to_string(&path).await else {
            return History::empty();
        };
        serde_json::from_str(&content).unwrap_or_else(|_| History::empty())
    }

    /// Delete this caller's transcript file and nothing else — a caller
    /// with no file yet (never chatted, or already cleared) is a no-op,
    /// never an error.
    pub async fn clear(data_dir: &Path, key: &HistoryKey) -> Result<()> {
        let path = Self::path(data_dir, key);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).context("Failed to remove history file"),
        }
    }

    /// The verbatim recent window, oldest first.
    pub fn recent(&self) -> &[PersistedMessage] {
        &self.turns
    }

    /// Append one completed turn's worth of messages, redacting
    /// wallet/files tool-call arguments and truncating oversized tool
    /// results, then persist atomically and compact if the verbatim
    /// window has grown past `KEEP_VERBATIM_TURNS`.
    ///
    /// `tool_categories` maps a tool name to the category the CALLER
    /// resolved it to (from the same `tools::registry()` `execute_tool`
    /// uses) — this module never re-derives categories itself, so there is
    /// no second, hand-maintained category list that could drift from
    /// D-06's real one.
    pub async fn append(
        &mut self,
        data_dir: &Path,
        key: &HistoryKey,
        messages: &[ChatMessage],
        tool_categories: &HashMap<String, PermissionCategory>,
    ) -> Result<()> {
        for msg in messages {
            self.turns.push(project(msg, tool_categories));
        }
        self.compact();
        self.save(data_dir, key).await
    }

    /// Fold turns older than `KEEP_VERBATIM_TURNS` into `self.summary`,
    /// extending it with only the turns that just aged out — never
    /// re-summarizing the whole transcript, so the cost of compaction
    /// itself stays bounded as the transcript grows (AI-SPEC §4b.4).
    pub fn compact(&mut self) {
        if self.turns.len() <= KEEP_VERBATIM_TURNS {
            return;
        }
        let overflow = self.turns.len() - KEEP_VERBATIM_TURNS;
        let aged_out: Vec<PersistedMessage> = self.turns.drain(0..overflow).collect();
        let extension = summarize_turns(&aged_out);
        if extension.is_empty() {
            return;
        }
        if self.summary.is_empty() {
            self.summary = extension;
        } else {
            self.summary.push('\n');
            self.summary.push_str(&extension);
        }
    }

    /// Write atomically: serialize to a sibling temp file in the same
    /// directory, then `rename` over the target — matches
    /// `music/index.rs::save_atomic`'s discipline (this codebase's own
    /// precedent for a `data_dir`-scoped JSON index). A reader never sees
    /// a partial file, and a crash mid-write leaves the previous
    /// transcript intact.
    async fn save(&self, data_dir: &Path, key: &HistoryKey) -> Result<()> {
        let path = Self::path(data_dir, key);
        let dir = path
            .parent()
            .expect("history path always has a parent directory")
            .to_path_buf();
        tokio::fs::create_dir_all(&dir)
            .await
            .context("Failed to create assistant/history dir")?;

        let tmp = dir.join(format!(".{}.tmp.{}", key.filename(), std::process::id()));
        let content = serde_json::to_string_pretty(self).context("Failed to serialize history")?;
        let write_result: Result<()> = async {
            tokio::fs::write(&tmp, &content)
                .await
                .context("Failed to write history temp file")?;
            tokio::fs::rename(&tmp, &path)
                .await
                .context("Failed to rename history into place")?;
            Ok(())
        }
        .await;
        if write_result.is_err() {
            let _ = tokio::fs::remove_file(&tmp).await;
        }
        write_result?;

        // 0600: following `grants.rs`'s convention (itself following
        // `streaming/session.rs`'s data_dir-scoped persisted-state
        // pattern) — a transcript is a sensitive-data location by
        // definition (D-08), never world-readable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).ok();
        }
        Ok(())
    }
}

/// Whether a tool call's category means its arguments must never reach
/// disk.
fn should_redact(category: Option<PermissionCategory>) -> bool {
    matches!(category, Some(c) if REDACTED_CATEGORIES.contains(&c))
}

/// Truncate a tool result before it enters history, with a visible marker.
fn truncate_result(content: &str) -> (String, bool) {
    if content.chars().count() <= MAX_TOOL_RESULT_CHARS {
        return (content.to_string(), false);
    }
    let truncated: String = content.chars().take(MAX_TOOL_RESULT_CHARS).collect();
    (
        format!("{truncated}\n…[truncated — result exceeded {MAX_TOOL_RESULT_CHARS} characters]"),
        true,
    )
}

/// Project a live `ChatMessage` into its persisted, redacted,
/// size-bounded form. The ONLY inputs are a completed turn's `ChatMessage`
/// and a resolved category map — no parameter here has a path to
/// `confirm::PendingConfirmation` (see the module doc and
/// `project_has_no_path_to_pending_confirmation_state`).
fn project(
    msg: &ChatMessage,
    tool_categories: &HashMap<String, PermissionCategory>,
) -> PersistedMessage {
    PersistedMessage {
        role: msg.role.into(),
        text: msg.text.clone(),
        tool_calls: msg
            .tool_calls
            .iter()
            .map(|c| {
                let category = tool_categories.get(&c.name).copied();
                PersistedToolCall {
                    name: c.name.clone(),
                    arguments: if should_redact(category) {
                        None
                    } else {
                        Some(c.arguments.clone())
                    },
                }
            })
            .collect(),
        tool_results: msg
            .tool_results
            .iter()
            .map(|r| {
                let (content, truncated) = truncate_result(&r.content);
                PersistedToolResult {
                    content,
                    is_error: r.is_error,
                    truncated,
                }
            })
            .collect(),
    }
}

/// A cheap, local, non-model textual digest of the turns aging out of the
/// verbatim window. AI-SPEC §4b.4 names a model-backed summarizer
/// (preferring the already-selected local backend) as the eventual
/// implementation; this plan's scope is the compaction MECHANISM
/// (fold-not-truncate, extend-incrementally-not-regenerate-from-scratch),
/// Replay a persisted transcript as the model-facing prefix for a new
/// turn: the running summary (if any) as a System note, then the verbatim
/// recent turns. Text-only — tool calls and their results are deliberately
/// NOT replayed: a stale tool result is a claim about the node's state at
/// some earlier moment, and re-presenting it as if it were this turn's
/// evidence is how an assistant ends up asserting that a container is
/// running because it was running ten minutes ago.
///
/// Without this, D-08's persistence is write-only: 13-10 stored every turn
/// and never showed the model any of it, so the assistant answered "I
/// don't have access to any previous conversation history" while its own
/// transcript sat on disk (found on-device 2026-08-06).
pub fn replay(hist: &History) -> Vec<ChatMessage> {
    let mut out = Vec::with_capacity(hist.turns.len() + 1);
    if !hist.summary.trim().is_empty() {
        out.push(ChatMessage {
            role: Role::System,
            text: Some(format!(
                "Earlier in this conversation: {}",
                hist.summary.trim()
            )),
            tool_calls: vec![],
            tool_results: vec![],
        });
    }
    for turn in &hist.turns {
        let role = match turn.role {
            PersistedRole::User => Role::User,
            PersistedRole::Assistant => Role::Assistant,
            // Tool traffic and prior system notes are not replayed.
            PersistedRole::Tool | PersistedRole::System => continue,
        };
        let Some(text) = turn.text.as_ref().filter(|t| !t.trim().is_empty()) else {
            continue;
        };
        out.push(ChatMessage {
            role,
            text: Some(text.clone()),
            tool_calls: vec![],
            tool_results: vec![],
        });
    }
    out
}

/// proven here with a plain digest rather than a model call — see the
/// plan's SUMMARY for why a model-backed summarizer is left as a named
/// follow-up rather than implemented in this pass.
fn summarize_turns(turns: &[PersistedMessage]) -> String {
    let mut lines = Vec::with_capacity(turns.len());
    for turn in turns {
        match turn.role {
            PersistedRole::User => {
                if let Some(t) = &turn.text {
                    lines.push(format!("User asked: {t}"));
                }
            }
            PersistedRole::Assistant => {
                if let Some(t) = &turn.text {
                    lines.push(format!("Assistant answered: {t}"));
                } else if !turn.tool_calls.is_empty() {
                    let names: Vec<&str> =
                        turn.tool_calls.iter().map(|c| c.name.as_str()).collect();
                    lines.push(format!("Assistant called: {}", names.join(", ")));
                }
            }
            PersistedRole::Tool | PersistedRole::System => {}
        }
    }
    lines.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::tools::{ToolCall, ToolResult};
    use serde_json::json;

    fn user_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: Role::User,
            text: Some(text.to_string()),
            tool_calls: vec![],
            tool_results: vec![],
        }
    }

    /// D-08 regression (on-device, 2026-08-06): the transcript must be
    /// REPLAYED to the model, not merely stored. Persistence was
    /// write-only — every turn was saved and none was ever shown, so the
    /// assistant answered "I don't have access to any previous
    /// conversation history" with its own transcript sitting on disk.
    #[tokio::test]
    async fn replay_feeds_prior_turns_back_to_the_model() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let key = HistoryKey::from_caller(&CallerScope::LocalOperator {
            session_id: "op-1".to_string(),
        });
        let categories = HashMap::new();
        let mut hist = History::load(tmp.path(), &key).await;

        hist.append(
            tmp.path(),
            &key,
            &[
                user_msg("is filebrowser running?"),
                ChatMessage {
                    role: Role::Assistant,
                    text: Some("Yes, filebrowser is running.".to_string()),
                    tool_calls: vec![],
                    tool_results: vec![],
                },
            ],
            &categories,
        )
        .await
        .expect("append");

        let replayed = replay(&History::load(tmp.path(), &key).await);
        let texts: Vec<&str> = replayed.iter().filter_map(|m| m.text.as_deref()).collect();
        assert!(
            texts.iter().any(|t| t.contains("is filebrowser running?")),
            "the user's own prior turn must be replayed: {texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t.contains("Yes, filebrowser is running.")),
            "the assistant's prior answer must be replayed: {texts:?}"
        );
        // Tool traffic is never replayed: a stale tool result is a claim
        // about the node's state at an earlier moment.
        assert!(
            replayed
                .iter()
                .all(|m| m.tool_calls.is_empty() && m.tool_results.is_empty()),
            "replay must be text-only"
        );
    }

    /// Behavior: a completed turn is appended to a transcript stored under
    /// `data_dir` and survives a daemon restart (simulated by dropping the
    /// in-memory `History` and reloading from disk).
    #[tokio::test]
    async fn append_persists_and_survives_reload() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let key = HistoryKey::from_caller(&CallerScope::LocalOperator {
            session_id: "op-1".to_string(),
        });
        let categories = HashMap::new();

        let mut hist = History::load(tmp.path(), &key).await;
        assert!(
            hist.recent().is_empty(),
            "a fresh caller starts with no transcript"
        );
        let msg = user_msg("how much disk space is left?");
        hist.append(tmp.path(), &key, std::slice::from_ref(&msg), &categories)
            .await
            .expect("append");

        // Simulate a daemon restart: nothing but disk survives.
        let reloaded = History::load(tmp.path(), &key).await;
        assert_eq!(reloaded.recent().len(), 1);
        assert_eq!(
            reloaded.recent()[0].text.as_deref(),
            Some("how much disk space is left?")
        );
    }

    /// Behavior: an operator's AIUI transcript and a mesh peer's
    /// transcript are separate — reading one never returns a turn from the
    /// other.
    #[tokio::test]
    async fn operator_and_mesh_transcripts_are_separate() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let operator_key = HistoryKey::from_caller(&CallerScope::LocalOperator {
            session_id: "op-1".to_string(),
        });
        let mesh_key = HistoryKey::from_caller(&CallerScope::Mesh {
            peer_id: "peer-1".to_string(),
            authorized: true,
        });
        let categories = HashMap::new();

        let mut operator_hist = History::load(tmp.path(), &operator_key).await;
        let operator_msg = user_msg("operator secret question");
        operator_hist
            .append(
                tmp.path(),
                &operator_key,
                std::slice::from_ref(&operator_msg),
                &categories,
            )
            .await
            .expect("append operator");

        let mut mesh_hist = History::load(tmp.path(), &mesh_key).await;
        let mesh_msg = user_msg("mesh peer question");
        mesh_hist
            .append(
                tmp.path(),
                &mesh_key,
                std::slice::from_ref(&mesh_msg),
                &categories,
            )
            .await
            .expect("append mesh");

        let reloaded_operator = History::load(tmp.path(), &operator_key).await;
        let reloaded_mesh = History::load(tmp.path(), &mesh_key).await;

        assert_eq!(reloaded_operator.recent().len(), 1);
        assert_eq!(
            reloaded_operator.recent()[0].text.as_deref(),
            Some("operator secret question")
        );
        assert_eq!(reloaded_mesh.recent().len(), 1);
        assert_eq!(
            reloaded_mesh.recent()[0].text.as_deref(),
            Some("mesh peer question")
        );

        assert!(
            !reloaded_operator
                .recent()
                .iter()
                .any(|t| t.text.as_deref() == Some("mesh peer question")),
            "the operator's transcript must never contain the mesh peer's turn"
        );
        assert!(
            !reloaded_mesh
                .recent()
                .iter()
                .any(|t| t.text.as_deref() == Some("operator secret question")),
            "the mesh transcript must never contain the operator's turn"
        );
    }

    /// Behavior: a tool result longer than the cap is truncated before it
    /// enters history, with a visible marker; a short result is untouched.
    #[tokio::test]
    async fn long_tool_result_is_truncated_with_marker() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let key = HistoryKey::from_caller(&CallerScope::LocalOperator {
            session_id: "s".to_string(),
        });
        let categories = HashMap::new();

        let long_content = "x".repeat(MAX_TOOL_RESULT_CHARS + 500);
        let long_msg = ChatMessage {
            role: Role::Tool,
            text: None,
            tool_calls: vec![],
            tool_results: vec![ToolResult {
                call_id: "1".to_string(),
                content: long_content.clone(),
                is_error: false,
            }],
        };
        let mut hist = History::load(tmp.path(), &key).await;
        hist.append(
            tmp.path(),
            &key,
            std::slice::from_ref(&long_msg),
            &categories,
        )
        .await
        .expect("append");

        let reloaded = History::load(tmp.path(), &key).await;
        let persisted = &reloaded.recent()[0].tool_results[0];
        assert!(
            persisted.truncated,
            "an oversized result must be marked truncated"
        );
        assert!(persisted.content.len() < long_content.len());
        assert!(
            persisted.content.to_lowercase().contains("truncated"),
            "the marker must be visible in the persisted content: {}",
            persisted.content
        );

        let short_msg = ChatMessage {
            role: Role::Tool,
            text: None,
            tool_calls: vec![],
            tool_results: vec![ToolResult {
                call_id: "2".to_string(),
                content: "short".to_string(),
                is_error: false,
            }],
        };
        let mut hist2 = History::load(tmp.path(), &key).await;
        hist2
            .append(
                tmp.path(),
                &key,
                std::slice::from_ref(&short_msg),
                &categories,
            )
            .await
            .expect("append short");
        let reloaded2 = History::load(tmp.path(), &key).await;
        let short_persisted = &reloaded2.recent()[1].tool_results[0];
        assert!(
            !short_persisted.truncated,
            "a short result must not be marked truncated"
        );
        assert_eq!(short_persisted.content, "short");
    }

    /// Behavior: once the transcript exceeds the verbatim window, older
    /// turns fold into a running summary and the recent window stays
    /// verbatim; the summary is extended incrementally rather than
    /// regenerated from scratch.
    #[tokio::test]
    async fn compaction_folds_older_turns_into_incremental_summary() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let key = HistoryKey::from_caller(&CallerScope::LocalOperator {
            session_id: "s".to_string(),
        });
        let categories = HashMap::new();
        let mut hist = History::load(tmp.path(), &key).await;

        for i in 0..(KEEP_VERBATIM_TURNS + 3) {
            let msg = user_msg(&format!("turn {i}"));
            hist.append(tmp.path(), &key, std::slice::from_ref(&msg), &categories)
                .await
                .expect("append");
        }

        assert_eq!(
            hist.recent().len(),
            KEEP_VERBATIM_TURNS,
            "the verbatim window must stay bounded at KEEP_VERBATIM_TURNS"
        );
        assert!(
            !hist.summary.is_empty(),
            "turns that aged out of the window must be folded into the summary, not dropped"
        );
        assert!(hist.summary.contains("turn 0"));
        assert!(hist.summary.contains("turn 1"));
        assert!(hist.summary.contains("turn 2"));
        assert_eq!(
            hist.recent().last().unwrap().text.as_deref(),
            Some(format!("turn {}", KEEP_VERBATIM_TURNS + 2).as_str()),
            "the verbatim window must keep the MOST RECENT turns"
        );

        let summary_before_further_growth = hist.summary.clone();

        for i in (KEEP_VERBATIM_TURNS + 3)..(KEEP_VERBATIM_TURNS + 6) {
            let msg = user_msg(&format!("turn {i}"));
            hist.append(tmp.path(), &key, std::slice::from_ref(&msg), &categories)
                .await
                .expect("append");
        }
        assert!(
            hist.summary.contains(&summary_before_further_growth),
            "the summary must be EXTENDED incrementally — the earlier summary text must survive \
             verbatim as a substring, never be regenerated from the full transcript"
        );
        assert_eq!(hist.recent().len(), KEEP_VERBATIM_TURNS);
    }

    /// Behavior: `assistant.clear-history` removes the calling session's
    /// transcript and nothing else.
    #[tokio::test]
    async fn clear_removes_only_this_callers_transcript() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let key_a = HistoryKey::from_caller(&CallerScope::LocalOperator {
            session_id: "a".to_string(),
        });
        let key_b = HistoryKey::from_caller(&CallerScope::LocalOperator {
            session_id: "b".to_string(),
        });
        let categories = HashMap::new();
        let msg = user_msg("hi");

        let mut hist_a = History::load(tmp.path(), &key_a).await;
        hist_a
            .append(tmp.path(), &key_a, std::slice::from_ref(&msg), &categories)
            .await
            .expect("append a");
        let mut hist_b = History::load(tmp.path(), &key_b).await;
        hist_b
            .append(tmp.path(), &key_b, std::slice::from_ref(&msg), &categories)
            .await
            .expect("append b");

        History::clear(tmp.path(), &key_a).await.expect("clear a");

        let reloaded_a = History::load(tmp.path(), &key_a).await;
        let reloaded_b = History::load(tmp.path(), &key_b).await;
        assert!(
            reloaded_a.recent().is_empty(),
            "the cleared transcript must load empty"
        );
        assert_eq!(
            reloaded_b.recent().len(),
            1,
            "clearing one caller's transcript must not touch another caller's"
        );

        History::clear(tmp.path(), &key_a)
            .await
            .expect("clearing an already-absent transcript is a no-op, not an error");
    }

    /// Behavior: no tool argument value from a `wallet`- or
    /// `files`-category tool is written to the transcript file — asserted
    /// both at the deserialized-struct level and against the raw on-disk
    /// bytes, so this proves the value never touches disk, not merely that
    /// a struct field reads `None`.
    #[tokio::test]
    async fn wallet_tool_arguments_never_reach_the_transcript() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let key = HistoryKey::from_caller(&CallerScope::LocalOperator {
            session_id: "s".to_string(),
        });
        let mut categories = HashMap::new();
        categories.insert("wallet_send".to_string(), PermissionCategory::Wallet);
        categories.insert("files_read".to_string(), PermissionCategory::Files);
        categories.insert("app_restart".to_string(), PermissionCategory::Apps);

        let msg = ChatMessage {
            role: Role::Assistant,
            text: None,
            tool_calls: vec![
                ToolCall {
                    id: "1".to_string(),
                    name: "wallet_send".to_string(),
                    arguments: json!({ "amount_sats": 5000, "address": "bc1qexampleexampleexample" }),
                },
                ToolCall {
                    id: "2".to_string(),
                    name: "files_read".to_string(),
                    arguments: json!({ "path": "/home/user/very-secret-plan.txt" }),
                },
                ToolCall {
                    id: "3".to_string(),
                    name: "app_restart".to_string(),
                    arguments: json!({ "app_id": "immich" }),
                },
            ],
            tool_results: vec![],
        };

        let mut hist = History::load(tmp.path(), &key).await;
        hist.append(tmp.path(), &key, std::slice::from_ref(&msg), &categories)
            .await
            .expect("append");

        let reloaded = History::load(tmp.path(), &key).await;
        let persisted = &reloaded.recent()[0];
        assert_eq!(
            persisted.tool_calls[0].arguments, None,
            "wallet-category tool arguments must never reach the transcript"
        );
        assert_eq!(
            persisted.tool_calls[1].arguments, None,
            "files-category tool arguments must never reach the transcript"
        );
        assert_eq!(
            persisted.tool_calls[2].arguments,
            Some(json!({ "app_id": "immich" })),
            "a non-redacted category's arguments ARE persisted"
        );

        // The stronger property: the raw file bytes never contain the
        // sensitive values at all.
        let raw = tokio::fs::read_to_string(History::path(tmp.path(), &key))
            .await
            .expect("read raw file");
        assert!(!raw.contains("5000"), "wallet amount must never touch disk");
        assert!(
            !raw.contains("bc1qexampleexampleexample"),
            "wallet address must never touch disk"
        );
        assert!(
            !raw.contains("very-secret-plan.txt"),
            "files path must never touch disk"
        );
    }

    /// Type-level assertion: `project`'s only inputs are `&ChatMessage` and
    /// a resolved category map — there is no parameter type here through
    /// which a `confirm::PendingConfirmation` could ever reach this
    /// function, so it is structurally impossible for this module to
    /// persist pending-confirmation state.
    #[test]
    fn project_has_no_path_to_pending_confirmation_state() {
        let _shape: fn(&ChatMessage, &HashMap<String, PermissionCategory>) -> PersistedMessage =
            project;
    }

    /// Behavior: the transcript file is created 0600.
    #[tokio::test]
    async fn history_file_is_created_0600() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let key = HistoryKey::from_caller(&CallerScope::LocalOperator {
            session_id: "s".to_string(),
        });
        let categories = HashMap::new();
        let msg = user_msg("hi");
        let mut hist = History::load(tmp.path(), &key).await;
        hist.append(tmp.path(), &key, std::slice::from_ref(&msg), &categories)
            .await
            .expect("append");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(History::path(tmp.path(), &key)).expect("metadata");
            assert_eq!(
                meta.permissions().mode() & 0o777,
                0o600,
                "the transcript file must be 0600"
            );
        }
    }
}
