# Talking to your node

Every way to ask an Archipelago node a question: over the LoRa mesh, by voice, and over HTTP.

Two rules worth internalising before the tables:

- **`!ai` asks a language model. `!archy` never does.** `!archy` reads the same status caches the HTTP endpoints serve, so its answers are deterministic, cost nothing, and keep working with the assistant switched off.
- **Mesh command prefixes are exact strings** (case-insensitive). **Voice phrases are not** — they are matched by Home Assistant's intent parser, so the examples below are representative, not literal.

---

## 1. Mesh commands

Sent as ordinary text over the mesh — either as plain channel/DM text from a stock meshcore or Meshtastic client, or typed into a 1:1 chat in the Archipelago UI.

### `!archy` — node status, no AI

| Command | Aliases | Answers with |
|---|---|---|
| `!archy` | `!archy status` | OS version, chain tip, peer count, electrum progress |
| `!archy btc` | `bitcoin`, `node`, `sync` | Sync state, block height, peer count |
| `!archy electrs` | `electrum` | Electrum index progress |
| `!archy version` | `ver` | Archipelago OS version |
| anything else | — | A one-line usage hint |

Examples of what comes back:

```
!archy            → Archipelago OS v1.7.99-alpha: BTC synced 957295 (12p), electrum 86%.
!archy btc        → BTC: synced, block 957295, 12 peers.
!archy electrs    → Electrum: syncing 86.2% (824785/957295).
!archy version    → Archipelago OS v1.7.99-alpha
!archy wat        → archy: !archy [status|btc|electrs|version]. Node status, no AI.
```

`!archyfoo` is not a command — the prefix must be followed by whitespace or end-of-message.

### `!ai` / `!ask` — language model

```
!ai what is the halving schedule
!ask how do I open a lightning channel
```

Requires the assistant to be **enabled** (`assistant_enabled` in `mesh-config.json`). Backend is `ollama` (default, `qwen2.5-coder`) or `claude` (`claude-haiku-4-5-20251001`), set by `assistant_backend`. The model is told to reply in at most two short sentences, because airtime is scarce.

### Who is allowed to ask

Both commands share one gate — `is_sender_allowed()` in `mesh/listener/assist.rs`. Evaluated in order:

1. **Blocked contact** → always denied.
2. **On `assistant_allowed_contacts`** → allowed, even without a signature. This is the deliberate opt-in for keyless phone clients.
3. **`assistant_trusted_only == false`** → anyone on the mesh may ask.
4. **`assistant_trusted_only == true`** → the asker must be *authenticated* **and** carry federation `Trusted` status.

"Authenticated" means the message carried an Ed25519 signature that verified against the sender's known identity key, **or** it arrived over the federation (Tor) transport, which verifies upstream. **Bare plain-text radio messages are never authenticated** — so on a `trusted_only` node, a stock meshcore client can only get an answer by being on the allowlist.

Denials are silent on the wire (no airtime is spent saying "no"); the asker is recorded so an operator can allow them from the UI.

The one asymmetry: **`!ai` additionally requires `assistant_enabled`; `!archy` does not**, because it never calls a model. Turning the LLM off should not take node status with it.

### Where the answer goes

| You asked from | Reply arrives as |
|---|---|
| Plain radio text, your pubkey is known | A private unicast DM (not the public channel) |
| Plain radio text, pubkey unresolvable | A broadcast on channel 0 |
| 1:1 chat in the Archipelago UI | A chat bubble in the same thread |
| The `AssistQuery` widget | Ordered, reassembled `AssistResponse` chunks |

Size caps: 480 characters per answer overall, 200 for plain-text channel/DM replies, and a hard 160-byte LoRa frame limit underneath both.

---

## 2. Voice

A [PineVoice](https://pine64.org) satellite speaker, wake word **"Hey Jarvis"** (or press the centre button). Speech-to-text is Whisper, text-to-speech is Piper — both run locally on the node. Nothing leaves the box.

Phrases are matched by intent, so wording is flexible. These are examples, not exact strings:

| Ask something like | You hear |
|---|---|
| "What is Archipelago OS?" · "What is this node running?" | A one-sentence description plus the running version |
| "What version am I running?" | Version and uptime |
| "Is my node synced?" · "How is my Bitcoin node doing?" · "Bitcoin node status" | Sync state, block height, peer count |
| "What's the current block height?" · "How many blocks do we have?" | The chain tip |
| "Is the electrum server synced?" · "Electrum status" | Index progress |

Optional and alternative words are part of the templates, so "how is *the* node doing" and "how is *my* Bitcoin node" both land on the same intent.

### How voice is wired

The speaker is a Wyoming satellite. Home Assistant runs it through an Assist pipeline: wake word on-device → audio streamed to Whisper → intent matched → Piper speaks the answer.

| Piece | Location (on the node) |
|---|---|
| Whisper + Piper services | `~/.config/containers/systemd/wyoming-{whisper,piper}.container` |
| Wyoming entries, Assist pipeline | `home-assistant/.storage/{core.config_entries,assist_pipeline.pipelines}` |
| Sensors + spoken answers | `home-assistant/configuration.yaml` (`rest:` and `intent_script:`) |
| Phrasings | `home-assistant/custom_sentences/en/archipelago.yaml` |

Home Assistant reaches the node's own HTTP API at `host.containers.internal` — **not** the node's LAN IP, which under rootless podman's pasta networking resolves back to the container itself.

Adding a phrase means editing `custom_sentences/en/archipelago.yaml`; adding an *answer* means adding an `intent_script` entry (and a `rest:` sensor if it needs new data).

---

## 3. HTTP

Served by nginx on port 80, proxying the backend on `127.0.0.1:5678`.

**No authentication required** (5-second cache):

| Endpoint | Returns |
|---|---|
| `GET /health` | Status, uptime, version, services |
| `GET /bitcoin-status` | `getblockchaininfo` + `getnetworkinfo` + `getindexinfo` |
| `GET /electrs-status` | Index height, progress, onion address |

```bash
curl -s http://<node>/bitcoin-status | jq '.blockchain_info.blocks'
```

**Session required** — everything else goes through JSON-RPC at `POST /rpc/v1`:

```bash
curl -s http://<node>/rpc/v1 -H 'Content-Type: application/json' \
  -d '{"method":"auth.login","params":{"password":"…"}}' -c jar.txt
curl -s http://<node>/rpc/v1 -b jar.txt -H 'Content-Type: application/json' \
  -d '{"method":"system.stats","params":{}}'
```

Login returns a `session` cookie. State-changing calls also need the `X-CSRF-Token` header. Exactly twelve read-only methods are CSRF-exempt, so for those the cookie alone is enough:

`node-messages-received` · `server.echo` · `server.get-state` · `system.stats` · `system.get-settings` · `system.get-node-key` · `system.get-metrics` · `system.get-version` · `tor.status` · `tor.onion-addresses` · `bitcoin.relay-status` · `federation.list-nodes`

Anything not on that list — including `bitcoin.getinfo` and `monitoring.current` — needs the CSRF header. If TOTP is enabled, follow the login with `auth.login.totp`.

---

## Adding a command

- **New `!archy` sub-command** — add a variant to `NodeCmd` and a match arm in `run_node_cmd`, both in `mesh/listener/node_cmd.rs`. Keep answers under 200 characters so a stock client sees the whole thing in one frame.
- **New mesh prefix** — add a `strip_*_trigger` alongside `strip_archy_trigger`, then hook it in `decode.rs` (plain radio text) and `dispatch.rs` (typed 1:1 chat). Both paths must be wired or the command only works from one of them.
- **New voice phrase or answer** — see the voice table above.
