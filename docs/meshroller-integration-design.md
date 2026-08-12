# Meshroller → Rust-native mesh assistant (issue #50)

**Decision (2026-06-17): seam (a) — lift Meshroller's *behaviors* into our Rust
mesh stack as typed message kinds.** We do NOT package the Python/Meshtastic
daemon. Meshroller rides Meshtastic-serial + a local Ollama; our radio is
**meshcore** (Heltec V3) and the `meshtastic` Python module cannot drive it. So
we reimplement its four behaviors natively against `core/archipelago/src/mesh/`,
drop the Python + Meshtastic dependency, and reuse our existing event/transport
seams.

Meshroller's behaviors (from the Phase-0 review of `meshroller.py`):
1. **LLM bridge** — relay an inbound mesh message to a local LLM, send the reply
   back on the mesh.
2. **Trusted-node auth** — only trusted senders may invoke commands.
3. **Scheduled / queued messaging** — send messages at a future time; queue for
   peers that are currently offline.
4. **On-channel command parser** — recognise commands in channel traffic.

---

## Where this plugs in (verified seam map)

| Concern | File / type | Anchor |
|---|---|---|
| Wire message kinds | `mesh/message_types.rs` `MeshMessageType` (`#[repr(u8)]`) | 28–73 |
| Envelope (CBOR, `0x02` marker, `seq`, `sig`) | `mesh/message_types.rs` `TypedEnvelope` | 183–197 |
| Inbound dispatch match | `mesh/listener/dispatch.rs` `handle_typed_envelope_direct()` | 80–691 |
| Outbound send | `mesh/mod.rs` `send_typed_wire()` / `send_channel_typed_wire()` | 848 / 1152 |
| Radio I/O command channel | `mesh/listener/mod.rs` `MeshCommand` (`SendText`/`BroadcastChannel`) | 55–73 |
| Frame chunking (≤160 B/frame, transparent) | `mesh/listener/session.rs` `send_dm_via_channel()` | — |
| UI push | `mesh/types.rs` `MeshEvent` (broadcast on `state.event_tx`, cap 64) | 125–164 |
| Trust gate | `federation/types.rs` `TrustLevel::Trusted` on `FederatedNode`; `federation::load_nodes()` | 5–52 |
| Block on user-blocklist | `mesh/listener/mod.rs` `ContactEntry.blocked` (`state.contacts`) | 110 |
| Local model | Ollama container, port **11434** (`port_allocator.rs:11`); call via `reqwest` (already a dep) | — |

No in-Rust LLM exists yet; we call the **local Ollama HTTP API** (the same model
Meshroller used) so nothing new is baked into the binary.

---

## Phase 1 — the assistant on the wire

### 1.1 New typed message kinds (`message_types.rs`)
Add two variants (next free tag = 24):

```rust
AssistQuery    = 24,   // "ask the node's AI" — prompt + optional model
AssistResponse = 25,   // reply — request_id + text + done flag
```
Wire the four spots the enum requires (`from_u8` 76–104, `from_label` 109–137,
`label()` 139–166, plus the variant) — mirror the `Invoice` variant exactly.

Payloads (CBOR via `encode_payload`/`decode_payload`):
```rust
pub struct AssistQueryPayload  { pub req_id: u64, pub prompt: String, pub model: Option<String> }
pub struct AssistResponsePayload { pub req_id: u64, pub text: String, pub seq: u16, pub done: bool }
```
`seq`/`done` let a long reply span multiple `AssistResponse` messages without
relying solely on frame reassembly (radio airtime is scarce — see §1.4 cap).

### 1.2 Inbound handler (`listener/dispatch.rs`)
Add a match arm for `AssistQuery`, mirroring the **`TxRelay`** arm (169–207):
validate → **gate** → spawn background work (never block the radio loop).

```rust
Some(MeshMessageType::AssistQuery) => {
    let payload = decode_payload::<AssistQueryPayload>(&envelope.v)?;
    if !assistant_enabled(state) { return; }                 // kill switch (config)
    if !sender_is_allowed(state, sender_contact_id).await { warn!(..); return; }
    if !rate_limit_ok(state, sender_contact_id).await { return; } // 1 in-flight / sender
    let _ = state.event_tx.send(MeshEvent::AssistQueryReceived { from_contact_id, prompt });
    let st = Arc::clone(state);
    tokio::spawn(async move { run_assist(&st, sender_contact_id, payload).await; });
}
```

`run_assist`: POST `http://localhost:11434/api/generate`
(`{model, prompt, stream:false}`), cap + chunk the response (§1.4), and emit each
chunk back to the sender via `send_typed_wire(contact_id, …, "assist_response", …)`.
Also store via the existing `store_typed_message` path so it lands in history,
and emit `MeshEvent::AssistResponseReady`.

### 1.3 Trust gate (`sender_is_allowed`)
Reuse the federation trust list — no new store:
```rust
let nodes = federation::load_nodes(&data_dir).await.unwrap_or_default();
let peer  = state.peers.read().await.get(&sender_contact_id).cloned();
let trusted = peer.and_then(|p| nodes.iter().find(|n|
    Some(&n.pubkey) == p.pubkey_hex.as_ref() || Some(&n.did) == p.did.as_ref())
    .map(|n| n.trust_level == TrustLevel::Trusted)).unwrap_or(false);
```
Plus honour `ContactEntry.blocked`. Config picks the policy:
**trusted-only** (default) | **specific contacts** | **anyone on channel** (opt-in).

### 1.4 Airtime discipline (meshcore reality)
Frames are ≤160 B and reassembly is automatic, but bandwidth is tiny. So:
- **Cap** the reply (default ~480 chars / ≤3 `AssistResponse` chunks); append
  `…(truncated — reply '!more')` and keep the tail server-side for a `!more`.
- **Rate-limit**: one in-flight query per sender; drop/deny extras.
- **Timeout** the Ollama call (e.g. 60 s) and reply with a short error on failure
  (`MeshEvent::AssistResponseReady { error }`).

### 1.5 Channel command parser
The killer entry point is a plain channel message, not a typed one. In the
inbound **`Text`** path, when a channel-0/1 message starts with the trigger
(default `!ai ` / `!ask `), synthesise an `AssistQuery` from the remainder and
run the same gated `run_assist`. This means **any meshcore client** (even a bare
Meshtastic-style sender) can ask, while typed `AssistQuery` is the rich path our
own UI uses. Trigger + enable are config.

A sibling command, **`!archy`**, answers node-status questions from the local
status caches with no model in the loop (`mesh/listener/node_cmd.rs`). It reuses
this design's trust gate (`is_sender_allowed`) and reply routing verbatim, but
deliberately does *not* require `assistant_enabled` — turning the LLM off should
not take node status with it. See [COMMANDS.md](COMMANDS.md) for the full
user-facing command surface.

### 1.6 UI events (`types.rs`)
```rust
AssistQueryReceived  { from_contact_id: u32, prompt: String },
AssistResponseReady  { req_id: u64, to_contact_id: u32, error: Option<String> },
ScheduledMessageFired { message_id: u64 },   // for Phase 1.7
```
Subscribers already flow through the single `event_tx` broadcast — no extra
wiring.

### 1.7 Scheduled / queued messaging
A small `AssistScheduler` owned by `MeshService` (sits beside `relay_tracker` /
`dead_man_switch` in `mod.rs`):
- Persisted queue `{ id, contact_id|channel, wire, fire_at, attempts }` under
  `data_dir/mesh/scheduled.json`.
- A tokio task wakes at the earliest `fire_at`, sends via the normal
  `send_typed_wire` / `MeshCommand::SendText` path, emits `ScheduledMessageFired`.
- **Offline queue**: on send failure (peer unreachable) keep the item and retry
  when a `PeerDiscovered` / `PeerUpdated` event names that peer.
- RPC: `mesh.schedule-message { contact_id|channel, body, fire_at }`,
  `mesh.list-scheduled`, `mesh.cancel-scheduled`.

---

## Phase 2 — killer Mesh-tab UX (ties into `project_mesh_telegram_plan`)

**Onboarding (one screen, three steps):**
1. *Model* — detect Ollama on :11434. If absent, a single "Install AI (Ollama)"
   button deep-links to the App Store entry; if present, pick the model
   (default the one already pulled).
2. *Who can ask* — Trusted nodes only (default) · Pick contacts · Anyone on the
   mesh channel (with a clear "uses your node's compute / airtime" warning).
3. *Trigger word* — default `!ai`; toggle the whole feature on.

**Usage (Mesh tab):**
- An **Assistant** card: on/off, model, policy, trigger; live feed driven by
  `AssistQueryReceived` / `AssistResponseReady`.
- Composer gains two actions: **Ask the mesh AI** (sends a typed `AssistQuery`)
  and **Send later** (date/time → `mesh.schedule-message`), with a "Scheduled"
  list (`mesh.list-scheduled`, cancel).

The 1–2 killer actions: *ask the island's AI from any radio*, and *queue a
message that sends itself when a peer comes back in range.*

---

## Verification
Needs **2 radios** (the .116 meshcore + a second) + Ollama running on the
answering node:
1. From radio B send `!ai what's the block height?` → node A (trusted) answers on
   the channel; untrusted B is silently denied.
2. Typed `AssistQuery` from our UI → chunked `AssistResponse` renders in the feed.
3. Long reply → truncation + `!more` continues.
4. Schedule a message to an out-of-range peer → it fires when the peer reappears.

## Effort & order
Multi-day. Land in this order so each step is testable alone:
1.1 enum + payloads → 1.2/1.3/1.4 gated bridge → 1.5 channel trigger →
1.6 events → 1.7 scheduler → Phase 2 UI. Phases 1.1–1.4 are the minimum
demoable slice (ask over the mesh, get an answer).
