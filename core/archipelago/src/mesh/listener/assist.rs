//! Mesh-AI assistant (issue #50) — answers `AssistQuery` messages with this
//! node's local LLM and sends the reply back over the mesh.
//!
//! This is the Rust-native lift of Meshroller's "LLM bridge": a trusted peer
//! asks a question over meshcore, an internet/compute-bearing node runs it
//! through a local model (Ollama) and streams the answer back in capped,
//! ordered chunks. Airtime is scarce, so the reply is length-capped and each
//! asker is limited to one in-flight query.

use super::super::message_types::{self, AssistResponsePayload, MeshMessageType};
use super::super::types::MeshEvent;
use super::bitcoin::send_to_peer;
use super::{MeshCommand, MeshState};
use crate::federation::TrustLevel;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

/// Local Ollama generate endpoint (same host the Ollama app binds).
const OLLAMA_URL: &str = "http://localhost:11434/api/generate";
/// Default model when the node hasn't configured one (matches Meshroller).
const DEFAULT_MODEL: &str = "qwen2.5-coder";
/// Anthropic Messages API (called with the shared proxy token).
const CLAUDE_URL: &str = "https://api.anthropic.com/v1/messages";
/// Default Claude model — Haiku 4.5: fast + cheap, ideal for short mesh answers.
const CLAUDE_DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";
/// Max time to wait on the model before giving up.
const OLLAMA_TIMEOUT: Duration = Duration::from_secs(60);
/// Hard cap on answer length sent over the radio — keeps airtime sane.
const MAX_REPLY_CHARS: usize = 480;
/// Characters of answer text per `AssistResponse` chunk.
const CHUNK_CHARS: usize = 160;
/// Tighter cap for plain-text channel replies (bare `!ai` clients) — these
/// aren't reassembled by an archipelago UI, so keep them to a couple frames.
const CHANNEL_REPLY_CHARS: usize = 200;

/// Where an answer should go.
pub(super) enum AssistReply {
    /// Typed `AssistResponse` chunks addressed to one peer — the archipelago
    /// UI path (rich, reassembled, correlated by `req_id`).
    Typed { contact_id: u32 },
    /// Plain-text broadcast on a mesh channel — the bare `!ai` path, so any
    /// client (including non-archipelago meshcore/Meshtastic nodes) sees it.
    ChannelText { channel: u8 },
    /// Normal `Text` chat bubble sent back into the 1:1 thread — the
    /// archipelago `!ai`-in-chat path. The asker typed `!ai …` as a regular
    /// direct message, so the answer lands inline in that same conversation
    /// (encrypted, peer-addressed) rather than as a separate widget.
    ChatText { contact_id: u32 },
    /// Plain-text NATIVE direct message back to the asker's radio contact —
    /// the bare `!ai` path for a stock meshcore client (e.g. a phone). The
    /// answer goes as a real unicast DM (not a public-channel broadcast), so
    /// only the asker sees it and a stock client can read it.
    RadioDm { dest_prefix: [u8; 6] },
}

/// Entry point: gate the query, run the model, send the answer back via the
/// requested reply path. Spawned off the radio loop so it never blocks.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_assist(
    prompt: String,
    model_override: Option<String>,
    req_id: u64,
    asker_contact_id: u32,
    sender_name: String,
    // Whether the asker's message was cryptographically authenticated (a
    // verified signature, or arrival over the federation transport). Required
    // for any identity-based allow under `trusted_only`/the allowlist.
    authenticated: bool,
    reply: AssistReply,
    state: Arc<MeshState>,
) {
    let asker = asker_contact_id;

    // Trust + block gate.
    if !is_sender_allowed(&state, asker, authenticated).await {
        warn!(
            from = asker,
            name = %sender_name,
            "AssistQuery denied — sender not permitted by assistant policy"
        );
        // Record who was turned away so the operator can find + allow them from
        // the UI (the silent-on-wire denial otherwise only shows in the journal).
        record_denied(&state, asker, &sender_name).await;
        // Silent on the wire (no airtime spent on denials); surface to the UI.
        let _ = state
            .event_tx
            .send(super::super::types::MeshEvent::AssistResponseReady {
                req_id,
                to_contact_id: asker,
                error: Some("denied".to_string()),
            });
        return;
    }

    // One in-flight query per asker.
    {
        let mut inflight = state.assist_inflight.write().await;
        if !inflight.insert(asker) {
            warn!(
                from = asker,
                "AssistQuery dropped — asker already has one in flight"
            );
            return;
        }
    }

    let _ = state
        .event_tx
        .send(super::super::types::MeshEvent::AssistQueryReceived {
            from_contact_id: asker,
            prompt: prompt.clone(),
        });

    let (backend, configured_model) = {
        let a = state.assistant.read().await;
        (a.backend.clone(), a.model.clone())
    };
    let is_claude = backend == "claude";
    let default_model = if is_claude {
        CLAUDE_DEFAULT_MODEL
    } else {
        DEFAULT_MODEL
    };
    let model = model_override
        .or(configured_model)
        .unwrap_or_else(|| default_model.to_string());

    info!(from = asker, req_id, backend = %backend, model = %model, "Answering AI query over mesh");

    // Same tool surface as the embedded assistant (task: mesh `!ai` must
    // ACTION, not just chat): when the server has wired the shared loop in,
    // the asker's prompt runs through `assistant::chat` with a Mesh caller
    // scope — the operator's persisted grants gate what the model may touch,
    // and any write suspends on the node's confirm gate for the operator to
    // approve. The asker passed `is_sender_allowed` above, so `authorized`
    // is true here. Without the handler (early boot), the legacy bare-LLM
    // path still answers.
    let shared = state.assistant_handler.read().await.clone();
    let result = match shared {
        Some(handler) => {
            let caller = crate::assistant::CallerScope::Mesh {
                peer_id: asker.to_string(),
                authorized: true,
            };
            crate::assistant::chat(handler, caller, prompt.clone()).await
        }
        None => {
            if is_claude {
                call_claude(&state.data_dir, &model, &prompt).await
            } else {
                call_ollama(&model, &prompt).await
            }
        }
    };

    match result {
        Ok(answer) => {
            send_reply(&state, &reply, req_id, &answer).await;
            let _ = state
                .event_tx
                .send(super::super::types::MeshEvent::AssistResponseReady {
                    req_id,
                    to_contact_id: asker,
                    error: None,
                });
        }
        Err(e) => {
            warn!(req_id, "AI query failed: {}", e);
            send_failure(&state, &reply, req_id, "AI unavailable").await;
            let _ = state
                .event_tx
                .send(super::super::types::MeshEvent::AssistResponseReady {
                    req_id,
                    to_contact_id: asker,
                    error: Some(e.to_string()),
                });
        }
    }

    state.assist_inflight.write().await.remove(&asker);
}

/// Whether `sender_contact_id` may invoke the assistant under the node's policy.
///
/// Always denies user-blocked contacts. Identity-based allows (the per-contact
/// allowlist and the federation-Trusted match) require `authenticated == true` —
/// i.e. the asker's message carried a signature that verified against its known
/// key (or it arrived over the federation transport, which verifies upstream).
/// A bare radio packet can CLAIM any key or DID, so without that proof the
/// allowlist and trust list are spoofable; only the explicit "anyone on the
/// mesh" policy (`trusted_only == false`) admits an unauthenticated asker.
pub(super) async fn is_sender_allowed(
    state: &Arc<MeshState>,
    sender_contact_id: u32,
    authenticated: bool,
) -> bool {
    let (pubkey_hex, did) = {
        let peers = state.peers.read().await;
        match peers.get(&sender_contact_id) {
            // Match identity on the bound archipelago key (stable, advert/
            // federation-verified), not the firmware routing key.
            Some(p) => (
                p.identity_pubkey_hex().map(|s| s.to_string()),
                p.did.clone(),
            ),
            None => (None, None),
        }
    };

    // Never answer a user-blocked contact, regardless of policy.
    if let Some(ref pk) = pubkey_hex {
        if state
            .contacts
            .read()
            .await
            .get(pk)
            .map(|c| c.blocked)
            .unwrap_or(false)
        {
            return false;
        }
    }

    // Explicit per-contact allowlist: the operator deliberately ticked THIS
    // contact, so honour it even for an unauthenticated radio asker. A stock
    // meshcore client (e.g. a phone) can't sign our typed envelopes, so it can
    // never be `authenticated` — gating the allowlist on authentication made
    // ticking such a contact have no effect. We match the asker's resolved
    // identity key: the bound archipelago key if we know it, else the firmware
    // routing key (`pubkey_hex`), which is how meshcore addresses the contact
    // and what the UI adds to the allowlist for a keyless radio peer. This is a
    // narrow, explicit opt-in for a specific key — the spoofable federation-
    // trust-list match below still requires authentication.
    if let Some(ref pk) = pubkey_hex {
        let allowed = state.assistant.read().await.allowed_contacts.clone();
        if allowed.iter().any(|a| a.eq_ignore_ascii_case(pk)) {
            return true;
        }
    }

    if !state.assistant.read().await.trusted_only {
        return true;
    }

    // Trusted-only from here: an unauthenticated asker can never match the trust
    // list (it could otherwise just claim a trusted node's public key/DID).
    if !authenticated {
        return false;
    }

    // Match against the federation trust list by the asker's verified archipelago
    // pubkey or DID (a radio peer gets these from its signed identity advert).
    let nodes = crate::federation::load_nodes(&state.data_dir)
        .await
        .unwrap_or_default();
    nodes.iter().any(|n| {
        n.trust_level == TrustLevel::Trusted
            && (Some(&n.pubkey) == pubkey_hex.as_ref() || Some(&n.did) == did.as_ref())
    })
}

/// Newest-first cap on the denied-asker buffer — enough to surface the people
/// who recently tried, without unbounded growth from a spammer.
const MAX_DENIED_ASKERS: usize = 25;

/// Record a turned-away `!ai` asker so the UI can offer a one-click "Allow".
/// Dedupes by contact id (moves an existing entry to the front and refreshes its
/// timestamp/name) so repeated denials from one device don't flood the list.
async fn record_denied(state: &Arc<MeshState>, asker_contact_id: u32, sender_name: &str) {
    // Capture the bound archipelago identity key (NOT the firmware routing key):
    // one-click "Allow" adds this to the allowlist, which the gate matches on the
    // archipelago key. A peer with no advert has no arch key → None → the UI shows
    // "no key" (only the "anyone on the mesh" policy can admit it).
    let pubkey_hex = {
        let peers = state.peers.read().await;
        peers
            .get(&asker_contact_id)
            .and_then(|p| p.arch_pubkey_hex.clone())
    };
    let entry = super::DeniedAsker {
        contact_id: asker_contact_id,
        name: sender_name.to_string(),
        pubkey_hex,
        at: chrono::Utc::now().to_rfc3339(),
    };
    let mut denied = state.assist_denied.write().await;
    denied.retain(|d| d.contact_id != asker_contact_id);
    denied.push_front(entry);
    denied.truncate(MAX_DENIED_ASKERS);
}

/// Cap the answer to `MAX_REPLY_CHARS`, appending a marker when truncated.
/// Returns (text_to_send, was_truncated).
fn cap_reply(answer: &str) -> (String, bool) {
    let trimmed = answer.trim();
    if trimmed.chars().count() <= MAX_REPLY_CHARS {
        return (trimmed.to_string(), false);
    }
    let capped: String = trimmed.chars().take(MAX_REPLY_CHARS).collect();
    (format!("{capped}…(truncated)"), true)
}

/// Send a successful answer via the requested reply path.
pub(super) async fn send_reply(
    state: &Arc<MeshState>,
    reply: &AssistReply,
    req_id: u64,
    answer: &str,
) {
    match reply {
        AssistReply::Typed { contact_id } => {
            let (text, _) = cap_reply(answer);
            send_typed_chunks(state, *contact_id, req_id, &text).await;
        }
        AssistReply::ChannelText { channel } => {
            let text = cap_channel(answer);
            send_channel_text(state, *channel, &text).await;
        }
        AssistReply::ChatText { contact_id } => {
            let (text, _) = cap_reply(answer);
            send_chat_text(state, *contact_id, &text).await;
        }
        AssistReply::RadioDm { dest_prefix } => {
            let text = cap_channel(answer);
            let _ = state
                .send_cmd(MeshCommand::SendNativeText {
                    dest_pubkey_prefix: *dest_prefix,
                    payload: text.into_bytes(),
                })
                .await;
        }
    }
}

/// Send a failure notice via the requested reply path.
async fn send_failure(state: &Arc<MeshState>, reply: &AssistReply, req_id: u64, msg: &str) {
    match reply {
        AssistReply::Typed { contact_id } => {
            let payload = AssistResponsePayload {
                req_id,
                text: String::new(),
                seq: 0,
                done: true,
                error: Some(msg.to_string()),
            };
            send_typed_response(state, *contact_id, &payload).await;
        }
        AssistReply::ChannelText { channel } => {
            send_channel_text(state, *channel, &format!("AI: {msg}")).await;
        }
        AssistReply::ChatText { contact_id } => {
            send_chat_text(state, *contact_id, &format!("AI: {msg}")).await;
        }
        AssistReply::RadioDm { dest_prefix } => {
            let _ = state
                .send_cmd(MeshCommand::SendNativeText {
                    dest_pubkey_prefix: *dest_prefix,
                    payload: format!("AI: {msg}").into_bytes(),
                })
                .await;
        }
    }
}

/// Split the answer into ordered `AssistResponse` chunks and send each back to
/// the asker on the encrypted, peer-addressed path (archipelago UI path).
async fn send_typed_chunks(state: &Arc<MeshState>, dest_contact_id: u32, req_id: u64, text: &str) {
    let chars: Vec<char> = text.chars().collect();
    let chunks: Vec<String> = if chars.is_empty() {
        vec![String::new()]
    } else {
        chars
            .chunks(CHUNK_CHARS)
            .map(|c| c.iter().collect())
            .collect()
    };
    let last = chunks.len().saturating_sub(1);
    for (i, chunk) in chunks.into_iter().enumerate() {
        let payload = AssistResponsePayload {
            req_id,
            text: chunk,
            seq: i as u16,
            done: i == last,
            error: None,
        };
        send_typed_response(state, dest_contact_id, &payload).await;
    }
}

/// Encode an `AssistResponse` payload and send it to a peer.
async fn send_typed_response(
    state: &Arc<MeshState>,
    dest_contact_id: u32,
    payload: &AssistResponsePayload,
) {
    let bytes = match message_types::encode_payload(payload) {
        Ok(b) => b,
        Err(e) => {
            warn!("Failed to encode AssistResponse: {}", e);
            return;
        }
    };
    let envelope = message_types::TypedEnvelope::new(MeshMessageType::AssistResponse, bytes);
    match envelope.to_wire() {
        Ok(wire) => send_to_peer(state, dest_contact_id, wire).await,
        Err(e) => warn!("Failed to encode AssistResponse envelope: {}", e),
    }
}

/// Send the answer back into the 1:1 chat thread as a normal chat bubble.
/// Used for the `!ai`-in-chat path. We emit an `AssistChatReply` event rather
/// than sending here, because the reply must be routed transport-aware:
/// `!ai` can arrive over LoRa OR over federation (Tor), and only
/// `MeshService::send_message` (which owns the signing key + Tor client) knows
/// to POST over the peer's onion for a federation-synthetic contact_id. The
/// radio-only path used to drop the reply for federation askers — the answer
/// showed on the answering node but never reached the asker. A server-layer
/// consumer fulfils this event via `send_message`, which also records the
/// Sent bubble and allocates the seq.
async fn send_chat_text(state: &Arc<MeshState>, contact_id: u32, text: &str) {
    let _ = state.event_tx.send(MeshEvent::AssistChatReply {
        contact_id,
        text: text.to_string(),
    });
}

/// Broadcast a plain-text answer on a channel for bare `!ai` clients.
async fn send_channel_text(state: &Arc<MeshState>, channel: u8, text: &str) {
    let _ = state
        .send_cmd(MeshCommand::BroadcastChannel {
            channel,
            payload: text.as_bytes().to_vec(),
        })
        .await;
}

/// Cap a plain-text channel reply to a couple of frames.
fn cap_channel(answer: &str) -> String {
    let trimmed = answer.trim();
    if trimmed.chars().count() <= CHANNEL_REPLY_CHARS {
        return format!("AI: {trimmed}");
    }
    let capped: String = trimmed.chars().take(CHANNEL_REPLY_CHARS).collect();
    format!("AI: {capped}…")
}

/// Call the local Ollama model and return the generated text.
async fn call_ollama(model: &str, prompt: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder().timeout(OLLAMA_TIMEOUT).build()?;
    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
    });
    let resp = client.post(OLLAMA_URL).json(&body).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("Ollama returned HTTP {}", resp.status());
    }
    let json: serde_json::Value = resp.json().await?;
    let text = json
        .get("response")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();
    if text.trim().is_empty() {
        anyhow::bail!("Ollama returned an empty response");
    }
    Ok(text)
}

/// Call Claude via the Anthropic Messages API using the node's shared proxy
/// token at `secrets/claude-api-key`. Keeps answers short for radio airtime.
async fn call_claude(data_dir: &Path, model: &str, prompt: &str) -> anyhow::Result<String> {
    let key = tokio::fs::read_to_string(data_dir.join("secrets/claude-api-key"))
        .await
        .map_err(|_| anyhow::anyhow!("Claude API key not configured on this node"))?;
    let key = key.trim();
    if key.is_empty() {
        anyhow::bail!("Claude API key is empty");
    }
    let client = reqwest::Client::builder().timeout(OLLAMA_TIMEOUT).build()?;
    let body = serde_json::json!({
        "model": model,
        "max_tokens": 512,
        "system": "You answer questions over a low-bandwidth radio mesh. Reply in at most two short sentences. No markdown, no preamble.",
        "messages": [{ "role": "user", "content": prompt }],
    });
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
    let json: serde_json::Value = resp.json().await?;
    // `content` is an array of blocks; take the first text block.
    let text = json
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| {
            arr.iter()
                .find_map(|b| b.get("text").and_then(|t| t.as_str()))
        })
        .unwrap_or("")
        .to_string();
    if text.trim().is_empty() {
        anyhow::bail!("Claude returned an empty response");
    }
    Ok(text)
}
