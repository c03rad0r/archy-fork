# Dual-ecash: Cashu + Fedimint, seamlessly

Status: **in progress** (2026-06-17). FE scaffolding + Fedimint HTTP bridge landed and
compile-checked; live federation round-trip and networking-sats routing are not yet validated.

## Why

Today the node's wallet (`core/archipelago/src/wallet/ecash.rs`, `mint_client.rs`, `cashu.rs`)
speaks **only** the Cashu NUT HTTP protocol (BDHKE, `cashuA…` tokens). There is **no** Fedimint
*client* — `apps/fedimint` is only the guardian server, and the "local Fedimint" default mint at
`127.0.0.1:8175` is just the guardian UI nginx, which does not expose the Cashu NUT API. So:

- The node can hold/spend generic Cashu tokens, but cannot hold Fedimint ecash or join federations.
- "Networking sats" (streaming/seeding revenue) is hardcoded to the Cashu wallet.

Goal: support **both** ecash protocols seamlessly — hold balances in either, join arbitrary
federations, and let networking-sats be paid/received over whichever protocol the peer accepts.

## Architecture decision

**Containerized `fedimint-clientd` + thin HTTP bridge** (chosen over linking the native
`fedimint-client` Rust SDK into the binary, and over a Lightning-only bridge).

```
archipelago binary
   ├─ CashuMintClient  ──HTTP (NUT /v1/*)──▶  cashu mint
   └─ FedimintClient   ──REST (/v2/*)─────▶  fedimint-clientd container ──▶ federation guardians
```

Rationale: keeps the heavy, fast-moving Fedimint SDK **out** of the main binary (no compile-time
coupling, no rebuild bloat, OTA-friendly), and fits the existing app/container architecture
(`apps/fedimint`, `apps/fedimint-gateway`). The Rust side is just a `reqwest` client, mirroring
`MintClient`.

### fedimint-clientd REST surface (v0.3.x)

- Auth: `Authorization: Bearer <password>`. Default port 8080 (we map it to host **8178** because
  8080 is LND REST). Base path `/v2/...`.
- `GET  /v2/admin/info` — per-federation balances (`totalAmountMsat`, denominations, meta).
- `POST /v2/admin/join` — `{ "inviteCode": "fed1…", "useManualSecret": false }` → joins / returns `federationId`.
- `POST /v2/mint/spend` — `{ federationId, amountMsat }` → serialized notes (ecash to send).
- `POST /v2/mint/reissue` — `{ federationId, notes }` → redeem received notes; returns reissued amount.
- `POST /v2/ln/invoice` / `POST /v2/ln/pay` — Lightning in/out (used for cross-protocol swaps).
- `GET  /health`.

Multi-federation: requests carry a `federationId`; clientd's `multimint` manages many clients.
**Exact JSON field names must be pinned to the clientd image tag we vendor** — code defensively.

## Components

### 1. Container app — `apps/fedimint-clientd/manifest.yml`
Sidecar running `fedimint-clientd`, mirroring `apps/fedimint-gateway`. Host port 8178 → container
8080. Password from node secret `fedimint-clientd-password`. State volume at
`/var/lib/archipelago/fedimint-clientd`. Added to `RESERVED_PORTS` (port_allocator.rs) and
`fallback_package_port()` (server.rs).

### 2. Rust bridge — `core/archipelago/src/wallet/fedimint_client.rs`
Thin `reqwest` client: `info()`, `join()`, `spend()`, `reissue()`, `ln_invoice()`, `ln_pay()`.
`from_node(data_dir)` resolves base URL + password (env `FEDIMINT_CLIENTD_URL` /
`FEDIMINT_CLIENTD_PASSWORD`, else defaults + secret file). Tor-proxy support via `with_client`,
mirroring `MintClient`.

### 3. RPCs — `core/archipelago/src/api/rpc/fedimint.rs`
- `wallet.fedimint-list` → joined federations + balances (`{federation_id, name, balance_sats}[]`).
- `wallet.fedimint-join` `{invite_code}` → joins via clientd, persists to
  `wallet/fedimint_federations.json`, returns `{federation_id}`.
- `wallet.fedimint-leave` `{federation_id}` → untracks locally.
- `wallet.fedimint-balance` → total sats across federations (from clientd `info`).

Local registry `wallet/fedimint_federations.json` = `{ federations: [{federation_id, name}] }` so
the list survives clientd being temporarily down; balances are live from clientd.

### 4. Frontend — `WalletSettingsModal.vue`
Tabbed: **Cashu Mints** (live: `streaming.list-mints` / `streaming.configure-mints`) and
**Fedimint Federations** (`wallet.fedimint-list` / `-join` / `-leave`). Gear icon on
`HomeWalletCard`. `fedimintBackendReady` flips to `true` once the RPCs ship; join degrades
gracefully with a clear error if the clientd app isn't installed.

### 4b. Default federation (zero-touch)
clientd auto-joins a default federation at boot via `FEDIMINT_CLIENTD_INVITE_CODE` (manifest), and
the Rust bridge `ensure_default_federation()` idempotently joins + tracks it (called from
`wallet.fedimint-list`) so already-running nodes pick it up too. Constant
`DEFAULT_FEDERATION_INVITE` in `fedimint_client.rs` is the single source on the Rust side; keep it
in sync with the manifest env. clientd is a **client, not the guardian** — it needs no local
`fedimintd`, so it bundles standalone.

### 4c. Bundling on every node
- **Bundled ISO:** add image to `scripts/image-versions.sh`, the ISO bundle `.tar` list, and a
  core-create block in `scripts/first-boot-containers.sh`; mark `tier: "core"` in
  `app-catalog/catalog.json`.
- **Unbundled ISO:** `first-boot-containers.sh` exits after FileBrowser only — add clientd to that
  early-exit block so unbundled nodes also get it out of the box.
- **CAVEAT:** confirm the *current* ISO assembler before editing the bundle list — the one found is
  under `image-recipe/_archived/` (likely stale); `first-boot-containers.sh`/`image-versions.sh`
  are current.
- Image: build from source (no official image; `flake.nix` only) → push to vps2
  `source.archipelago-foundation.org/lfg2025/fedimint-clientd:v0.4.0`.

### 5. Unified balance
`HomeWalletCard` ecash row = Cashu `wallet.ecash-balance` + Fedimint `wallet.fedimint-balance`.
(Home already calls `wallet.ecash-balance`; add fedimint and sum.)

## Networking sats — dual protocol (phase 6, NOT yet wired)

The economic layer (`streaming.rs`, `streaming/gate.rs`, sessions, pricing, metering) is already
protocol-agnostic — it just calls into the wallet. The injection points are:

1. **Protocol-tag accepted mints.** `accepted_mints: Vec<String>` → carry protocol, e.g.
   `cashu:https://mint…` / `fedimint:<federation_id>`. Migrate `wallet/accepted_mints.json` with a
   back-compat reader (bare URL ⇒ `cashu:`).
2. **`MintClient::new()` is the bottleneck** (~10 call sites). Introduce a `MintBackend` trait with
   `CashuBackend` (wraps current code) and `FedimintBackend` (calls `FedimintClient`).
3. **`Token` enum** `Cashu(CashuToken) | Fedimint(notes)`; serialize/verify by variant.
4. `build_payment_token()` picks a `(backend, id)` the peer accepts; `verify_and_receive_payment()`
   auto-detects the token variant and reissues/swaps on the right backend.
5. Cross-protocol settlement (Cashu↔Fedimint) bridges over Lightning (BOLT11) — both sides already
   have mint/melt (Cashu) and `ln/invoice`+`ln/pay` (Fedimint).
6. `Web5NetworkingProfitsSettings.vue`: per-service payout protocol/mint selector.

## Phases

- [x] **P0** FE tabbed Wallet Settings modal + gear (Cashu live, Fedimint tab structured).
- [x] **P1** `fedimint-clientd` container manifest + ports.
- [x] **P2** `FedimintClient` HTTP bridge + `wallet.fedimint-*` RPCs (compiles).
- [ ] **P3** Validate join / balance / spend / reissue against a live clientd + real federation on a scratch node.
- [ ] **P4** Unified ecash balance in the wallet card (Cashu + Fedimint).
- [ ] **P5** Flip FE fully live; surface "install Fedimint client app" when clientd unreachable.
- [ ] **P6** Networking-sats dual-protocol routing (the `MintBackend`/`Token` refactor above).

## Validation (per project testing discipline)

clientd image + a real federation are required; cannot be validated from the dev tree. Validate on
a scratch node: install Fedimint app + clientd, join a known test federation, confirm
`wallet.fedimint-balance`, then a spend→reissue round-trip between two nodes, then networking-sats
payment over Fedimint. Heavy/iterative work belongs in a worktree (see CLAUDE.md memory).
