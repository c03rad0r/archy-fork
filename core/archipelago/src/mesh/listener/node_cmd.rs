//! Mesh `!archy` command — answers node-status questions from this node's own
//! cached state, with no model in the loop.
//!
//! `!ai` sends the question to an LLM; `!archy` never does. Every answer here is
//! read straight from the same status caches the HTTP endpoints serve, so it is
//! deterministic, costs no tokens, and works with the assistant switched off.
//! Airtime is scarce, so replies are single-frame terse.

use super::assist::{is_sender_allowed, send_reply, AssistReply};
use super::MeshState;
use crate::{bitcoin_status, electrs_status};
use std::sync::Arc;
use tracing::{info, warn};

/// A parsed `!archy` sub-command.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum NodeCmd {
    Status,
    Bitcoin,
    Electrs,
    Version,
    Help,
}

impl NodeCmd {
    fn parse(rest: &str) -> Self {
        match rest.trim().to_ascii_lowercase().as_str() {
            "" | "status" => Self::Status,
            "btc" | "bitcoin" | "node" | "sync" => Self::Bitcoin,
            "electrs" | "electrum" => Self::Electrs,
            "version" | "ver" => Self::Version,
            _ => Self::Help,
        }
    }
}

/// Recognise a `!archy [subcommand]` prefix (case-insensitive) and return the
/// trimmed remainder, or `None` if the text isn't an archy command.
///
/// Accepts both a bare `!archy` and `!archy <sub>`, so the trailing space that
/// `strip_ai_trigger` requires is deliberately not required here.
pub(super) fn strip_archy_trigger(text: &str) -> Option<&str> {
    const P: &str = "!archy";
    let t = text.trim_start();
    if t.len() < P.len() || !t[..P.len()].eq_ignore_ascii_case(P) {
        return None;
    }
    let rest = &t[P.len()..];
    // `!archyfoo` is not the command; require end-of-text or a separator.
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(rest.trim())
}

/// Entry point: gate the asker, format the answer from local caches, reply.
///
/// Uses the same trust policy as the AI assistant (`is_sender_allowed`), so a
/// `trusted_only` node still only answers authenticated or allowlisted peers.
/// It does NOT require the assistant to be enabled — this path never calls a
/// model, so turning the LLM off shouldn't take node status with it.
pub(super) async fn run_node_cmd(
    rest: String,
    req_id: u64,
    asker_contact_id: u32,
    sender_name: String,
    authenticated: bool,
    reply: AssistReply,
    state: Arc<MeshState>,
) {
    let asker = asker_contact_id;

    if !is_sender_allowed(&state, asker, authenticated).await {
        warn!(
            from = asker,
            name = %sender_name,
            "!archy denied — sender not permitted by assistant policy"
        );
        // Silent on the wire, matching the assistant's denial behaviour.
        return;
    }

    let cmd = NodeCmd::parse(&rest);
    info!(from = asker, req_id, ?cmd, "Answering !archy over mesh");
    let answer = match cmd {
        NodeCmd::Status => status_line().await,
        NodeCmd::Bitcoin => bitcoin_line().await,
        NodeCmd::Electrs => electrs_line().await,
        NodeCmd::Version => version_line(),
        NodeCmd::Help => {
            "archy: !archy [status|btc|electrs|version]. Node status, no AI.".to_string()
        }
    };

    send_reply(&state, &reply, req_id, &answer).await;
}

fn version_line() -> String {
    format!("Archipelago OS v{}", env!("CARGO_PKG_VERSION"))
}

/// Pull `blocks`/`headers`/`connections` out of the cached Core RPC blobs.
async fn bitcoin_facts() -> Option<(u64, u64, u64, bool)> {
    let s = bitcoin_status::get_bitcoin_status().await;
    let chain = s.blockchain_info.as_ref()?;
    let blocks = chain.get("blocks")?.as_u64()?;
    let headers = chain.get("headers")?.as_u64()?;
    let ibd = chain
        .get("initialblockdownload")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let peers = s
        .network_info
        .as_ref()
        .and_then(|n| n.get("connections"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    Some((blocks, headers, peers, ibd))
}

async fn bitcoin_line() -> String {
    match bitcoin_facts().await {
        Some((blocks, headers, peers, ibd)) => {
            if !ibd && blocks == headers {
                format!("BTC: synced, block {blocks}, {peers} peers.")
            } else {
                format!("BTC: syncing {blocks}/{headers}, {peers} peers.")
            }
        }
        None => "BTC: status unavailable.".to_string(),
    }
}

async fn electrs_line() -> String {
    let e = electrs_status::get_electrs_sync_status().await;
    if let Some(err) = e.error.as_deref() {
        return format!("Electrum: error ({err}).");
    }
    if e.status == "ready" || e.status == "synced" {
        format!("Electrum: synced at {}.", e.indexed_height)
    } else {
        format!(
            "Electrum: {} {:.1}% ({}/{}).",
            e.status, e.progress_pct, e.indexed_height, e.bitcoin_height
        )
    }
}

/// One-frame overview: OS version + chain + electrum, kept under the plain-text
/// channel cap so a stock meshcore client sees the whole thing.
async fn status_line() -> String {
    let btc = match bitcoin_facts().await {
        Some((blocks, headers, peers, ibd)) if !ibd && blocks == headers => {
            format!("BTC synced {blocks} ({peers}p)")
        }
        Some((blocks, headers, peers, _)) => format!("BTC {blocks}/{headers} ({peers}p)"),
        None => "BTC n/a".to_string(),
    };
    let e = electrs_status::get_electrs_sync_status().await;
    let elec = if e.status == "ready" || e.status == "synced" {
        "electrum synced".to_string()
    } else {
        format!("electrum {:.0}%", e.progress_pct)
    };
    format!(
        "Archipelago OS v{}: {btc}, {elec}.",
        env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_matches_bare_and_sub() {
        assert_eq!(strip_archy_trigger("!archy"), Some(""));
        assert_eq!(strip_archy_trigger("  !ARCHY  "), Some(""));
        assert_eq!(strip_archy_trigger("!archy btc"), Some("btc"));
        assert_eq!(strip_archy_trigger("!Archy  electrs "), Some("electrs"));
    }

    #[test]
    fn trigger_rejects_non_commands() {
        assert_eq!(strip_archy_trigger("!archyfoo"), None);
        assert_eq!(strip_archy_trigger("!ai what is archy"), None);
        assert_eq!(strip_archy_trigger("archy"), None);
        assert_eq!(strip_archy_trigger("tell me !archy"), None);
    }

    #[test]
    fn subcommands_parse() {
        assert_eq!(NodeCmd::parse(""), NodeCmd::Status);
        assert_eq!(NodeCmd::parse("status"), NodeCmd::Status);
        assert_eq!(NodeCmd::parse("BTC"), NodeCmd::Bitcoin);
        assert_eq!(NodeCmd::parse("electrum"), NodeCmd::Electrs);
        assert_eq!(NodeCmd::parse("ver"), NodeCmd::Version);
        assert_eq!(NodeCmd::parse("wat"), NodeCmd::Help);
    }
}
