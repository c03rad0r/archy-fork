# Phase 4+ — Paid swarm streaming & the IndeeHub "Archipelago" source

**Status:** PLAN / design (2026-06-17) · **Branch:** `agent-trust-wip` ·
**partly implemented — "not implemented" was stale.** The paid-serving half
landed on main: `core/archipelago/src/swarm/paid.rs` says in its own header that
it is "DHT distribution plan, Phase 4 step F", with `paid_alpn.rs` and
`payment.rs` alongside it, a `streaming::` module, and the
`streaming.list-services` / `configure-service` / `toggle-service` / `pay` /
`prepare-payment` RPCs. It is doubly default-off: the swarm needs the
`iroh-swarm` cargo feature plus `config.swarm_enabled`, and serving stays free
for everyone until the operator enables the `content-download` service. Check
those gates before assuming any step below is live or dead.
**Builds on:** `docs/dht-distribution-design.md` (Phases 0–3, swarm + Blossom), the
Phase 3 swarm work just landed (`swarm/`, `content_hash.rs`, `trust/`).

This plans three things the user asked for, in one coherent architecture:

1. **Pay sats (ecash) for transport** of streaming film data between nodes.
2. **Networking *through* nodes** — relaying/routing a stream via intermediate peers.
3. An **"Archipelago" content source in IndeeHub** that shows every film uploaded
   to *backstage*, on every node running the IndeeHub app.

> ## Headline finding
> **Most of the primitives already exist.** This is ~80% integration glue, not
> greenfield. A full Cashu/ecash wallet, a metered streaming payment gate, a
> 4-tier transport layer, the iroh-blobs swarm (just added), signed Nostr
> advertisements, and the Ed25519 trust module are all already in the tree. The
> genuinely new code is: (a) a paid-serving hook on the iroh side, (b) a relay
> protocol, and (c) the IndeeHub film catalog + Archipelago-local API.

---

## 0. Inventory — what we can build on (all already in `core/archipelago/src`)

| Capability | Where | State |
| --- | --- | --- |
| **Cashu ecash wallet** (mint/melt/send/receive, BDHKE) | `wallet/ecash.rs`, `wallet/cashu.rs`, `wallet/mint_client.rs`, `wallet/bdhke.rs` | ✅ implemented |
| **Local mint** (Fedimint) backing the wallet | `apps/fedimint` (`http://127.0.0.1:8175`) | ✅ deployed |
| **Lightning** (invoices, pay, channels) for mint/melt | `api/rpc/lnd/*`, `container/lnd.rs`, `apps/lnd` | ✅ implemented |
| **Streaming payment gate** (accepts `cashuA` tokens, opens metered session) | `streaming/gate.rs` | ✅ implemented |
| **Metering & pricing** (sats per byte / ms / request; e.g. content-download = 1 sat/MB) | `streaming/meter.rs`, `streaming/pricing.rs`, `streaming/session.rs` | ✅ implemented |
| **Revenue/profit accounting** (incl. `StreamingRevenue` tx type) | `wallet/profits.rs` | ✅ implemented |
| **Paid-service discovery** on Nostr (kind 10021, TollGate TIP-01 shape) | `streaming/advertisement.rs` | ✅ implemented |
| **Content server** that verifies+receives payment before serving | `content_server.rs` (`verify_and_receive_payment()`) | ✅ implemented |
| **iroh-blobs swarm** (fetch content-addressed blobs from peers, verify, seed) | `swarm/` (`iroh-swarm` feature) | ✅ just added |
| **Signed seed adverts** (NIP-33 kind 30081, blake3→endpoint) | `swarm/seed_advert.rs` | ✅ just added |
| **BLAKE3 content addressing** | `content_hash.rs` | ✅ implemented |
| **Ed25519 trust / `did:key` / detached signatures** | `trust/` | ✅ implemented (anchor ceremony pending) |
| **4-tier transport** (Mesh > LAN > FIPS > Tor) + `last_transport` | `transport/*`, `fips/dial.rs` | ✅ implemented |
| **Node discovery + federation trust** (Trusted/Observer) | `nostr_handshake.rs`, `federation/*` | ✅ implemented |

What is **NOT** present and must be built:

- **A paid-serving hook on the iroh-blobs provider.** Today the swarm seeds to
  anyone (`BlobsProtocol::new(&store, None)` — no authorization). To charge for
  swarm bandwidth we need a per-request gate that consults `streaming/gate.rs`.
- **A relay protocol.** No "peer A asks peer B to forward traffic to peer C".
  Transport is point-to-point; there is no multi-hop routing, TTL, or relay
  accounting.
- **IndeeHub Archipelago catalog.** The shipped IndeeHub points at the external
  `staging-api.indeehub.studio` + AWS S3/CloudFront. Nothing makes a film
  uploaded on node A visible on node B. No *backstage* code exists yet.

---

## 1. Pay sats (ecash) for transport of streaming films

### Goal
When node B streams a film blob (an HLS `.ts` segment) *from* node A's swarm,
A earns sats for the bytes it serves — using the ecash gate that already meters
`content-download`.

### What exists vs. what's new
- ✅ The economic machinery is done: `streaming/pricing.rs` already ships a
  `content-download` service priced per MB; `streaming/gate.rs` turns a `cashuA`
  token into a metered session; `meter.rs` deducts bytes; `profits.rs` records
  `StreamingRevenue`.
- ❌ The swarm serving path doesn't consult any of it. `IrohProvider::new`
  spins up `BlobsProtocol` that answers every blob request unconditionally.

### Design — "paid swarm" as a gated blob protocol
The clean seam is the iroh-blobs **accept** side. Two viable shapes:

**(A) In-band gate via a custom ALPN (preferred).** Keep iroh-blobs for the raw
byte transfer but front it with a tiny request/grant exchange on a second ALPN
(`archy/paid-blobs/1`):
1. B wants `blake3:H`. It dials A's endpoint and sends `{want: H, token?: cashuA}`.
2. A calls `streaming::gate::check_gate("content-download", peer=B, bytes≈len(H), token)`.
   - `PaymentRequired` → A replies with price + its accepted mints
     (`streaming.list-mints`) and the sat amount; B mints/sends a `cashuA` and retries.
   - `PaidAndAllowed` / `Allowed` (within existing session allotment) → A authorizes
     the blob hash for this connection and hands off to iroh-blobs to stream it.
3. A meters served bytes via `meter::record_and_check` and records revenue.

**(B) Pre-paid session, then open serving.** B opens a metered session up front
(buys N MB of `content-download` allotment with one token), and A's blob protocol
checks "does this peer have remaining allotment?" before each blob. Simpler, fewer
round-trips, slightly looser accounting. Good first cut.

Recommend **(B) for v1** (least new protocol surface — reuses sessions verbatim),
graduating to **(A)** when we want per-blob price discovery.

### Free vs. paid policy (important)
- **OTA + app-catalog blobs stay FREE.** Charging for security updates is hostile
  and breaks the "origin always wins" guarantee. Gating applies **only** to the
  IndeeHub film scope (a per-blob or per-advert "monetized" flag).
- Trusted federation peers (`TrustLevel::Trusted`) can be configured to serve each
  other free; payment is for untrusted/public swarm peers.

### Integration points
- **DONE (2026-06-17):** `swarm/paid.rs` — the accept-side gate. Builds the
  iroh-blobs `EventSender` (intercept connect + GET, hard-disable `push`) and
  authorizes each request through `streaming::gate::check_gate("content-download",
  peer_endpoint, blob_size, None)`. Free when the service is disabled (default);
  denies unpaid peers when enabled; fails OPEN on internal error. Wired into
  `IrohProvider::new`; unit-tested. The Settings toggle the user just got drives it.
- Reuse: `streaming/gate.rs`, `meter.rs`, `session.rs`, `wallet/ecash.rs`,
  `streaming/advertisement.rs` (advertise the node as a paid blob seeder).
- TODO (fetch side): `swarm::fetch_content_addressed` gains an optional
  "willing-to-pay budget + token source" so a downloading node can auto-pay from
  its ecash wallet up to a cap (opening a session via `streaming.pay`), then fall
  back to origin if too expensive. This is where **cross-mint settlement (§2a)**
  plugs in — the payer may need to swap into the seeder's accepted mint first.

---

## 2. Networking *through* nodes (relayed / routed streaming)

This is the largest genuinely-new piece. Two distinct meanings — both useful:

### 2a. iroh-native relays (cheap, already mostly free)
iroh 1.0 already hole-punches and falls back to **relay servers** for connectivity
when a direct QUIC path can't be established. So "streaming through a node that
can reach the seed when I can't" partly exists at the iroh layer. Action: run/seed
our **own** iroh relay(s) on the OVH/hub infrastructure and pin them in config, so
the swarm doesn't depend on n0's public relays. Low effort, high resilience.

### 2b. Application-level paid relay (the real gap)
"Node B pays node A to fetch a film from origin/swarm on B's behalf and forward it"
— useful when B is behind a censored/expensive link and A has good connectivity
(the beta-cellular-node scenario from memory). This needs a real protocol:

- **`relay.offer` advert** (Nostr kind 10021 with a `relay` tag + price/MB) — reuse
  `streaming/advertisement.rs`; add a `relay-bandwidth` service to `pricing.rs`.
- **`relay.fetch` request** over the existing transport (`PeerRequest` in
  `fips/dial.rs`): `{content: blake3:H | url, pay: cashuA}`. The relay runs the
  normal `swarm::fetch_content_addressed` (swarm-assist, origin fallback), meters
  the bytes through `streaming/gate`, and streams them back to the requester.
- **Accounting:** add a `RelayBytes` metric to `streaming/meter.rs` distinct from
  origin `content-download`, so "relay provided" is tracked separately in
  `profits.rs` (the doc already separates `routing_fees` from `streaming_revenue`).
- **Safety rails:** single-hop only for v1 (no A→B→C→D); TTL + loop guard before
  any multi-hop; cap per-session bytes; only relay the **public film scope**, never
  private user blobs or arbitrary URLs (prevent open-proxy abuse).

### Phasing for §2
1. Pin our own iroh relays (config only). — *days*
2. Single-hop paid `relay.fetch` for film blobs, gated by ecash. — *the core build*
3. Multi-hop routing + path discovery. — *deferred; only if single-hop proves out*

---

## 2a. Cross-mint ecash settlement — paying across *different* mints

**Problem (user, 2026-06-17):** payment must work when the payer and the seeder
use **different** mints — not only two nodes on the same Fedimint. A node holding
tokens on mint **A** must be able to pay a seeder that only accepts mint **B**,
automatically.

### Why this is mostly a generalization, not new crypto
The wallet already tracks proofs **per-mint**: `WalletData::balance_for_mint(url)`,
`select_proofs(url, amount)`, `add_proofs(url, proofs)` are all mint-scoped, and
`MintClient::new(url)` targets any mint. What's hardcoded is convenience: `mint_quote`
/ `melt_quote` / `mint_tokens` / `melt_tokens` always use the single home
`wallet.mint_url`. So the data model is multi-mint already; we add the *swap* and
parameterize the helpers by target mint.

### The swap primitive (Cashu/Fedimint settle over Lightning)
To move value **A → B**, both mints expose BOLT11 mint+melt quotes (already in
`mint_client.rs`), and Lightning bridges them:

1. `MintClient::new(B).mint_quote(amount)` → a BOLT11 invoice `inv_B` (pay it to get B tokens).
2. `MintClient::new(A).melt_quote(inv_B)` → cost in A tokens (`amount + fee_reserve`).
3. Select A proofs and `melt` them on A to pay `inv_B` over Lightning.
4. When `inv_B` settles, `MintClient::new(B).mint_tokens(quote_B)` → claim B tokens;
   `wallet.add_proofs(B, …)`.

Net: value lands on B minus (A melt fee + LN routing + B mint fee). The node's LND
isn't strictly required — the mints' own LN gateways settle — but a healthy local
node/route improves success. Implementation = three thin `*_at(mint_url, …)`
variants of the existing helpers + one composer:
`swap_between_mints(data_dir, from, to, amount, max_fee_sats) -> Result<u64>`.

### Where the swap happens — two models
- **Payer-side swap (recommended default).** Before paying seeder S (whose
  `accepted_mints` are advertised via `streaming.advertise` / the gate's
  `PaymentRequired.pricing.accepted_mints`), the payer picks the cheapest path:
  pay directly if it already holds a token on one of S's mints; otherwise
  `swap_between_mints(A → S_mint)` then send a token denominated in S's mint. **S
  never has to trust mint A** — it only ever receives its own mint's tokens. Clean.
- **Payee-side auto-consolidation (optional, more liberal).** S widens
  `accepted_mints` to any mint it's willing to melt-swap from, accepts an A token,
  then swaps A → home-mint in the background. Broader acceptance, but S briefly
  carries mint-A counterparty risk.

A node can do both: advertise a broad accept list *and* have payers prefer
direct/cheap mints.

### Guardrails (these are the real design decisions)
- **Mint trust list.** Mints can be insolvent or rug. Only swap *into* / accept
  mints on a configured allow-list (default: home mint + a small curated set, with
  the local Fedimint always trusted). Surface this in the Settings UI alongside the
  per-service pricing.
- **Fee/slippage cap.** Every swap costs sats. `max_fee_sats` (or a max %) refuses a
  swap that would cost more than the content is worth; the payer then declines and
  uses origin. Show the all-in cost (price + swap fee) before auto-paying.
- **Origin always wins.** If the LN swap fails (no route, mint offline, over
  budget), fall back to the HTTP origin with no payment. A mint problem must never
  block content.
- **Idempotency / crash-safety.** Persist in-flight swaps (`melt` quote id + `mint`
  quote id) so a crash between "paid `inv_B`" and "claimed B tokens" resumes the
  claim instead of double-paying. Reuse the wallet's tx log.
- **Liquidity.** Swaps need the mints to have inbound/outbound LN liquidity; cache
  recent swap success per mint-pair and prefer routes that have worked.

### Phasing for §2a
1. `*_at(mint_url, …)` helpers + `swap_between_mints` + mint trust list + fee cap. — *the core*
2. Payer-side auto-swap in the payment builder (pick cheapest accepted mint). — *wires §1/§2 to it*
3. Idempotent resume + per-pair liquidity cache. — *hardening*
4. (Optional) payee-side auto-consolidation.

This keeps the headline promise intact: **pay anyone, on any trusted mint,
automatically — or fall back to free origin.**

---

## 3. IndeeHub "Archipelago" content source

### Goal
A new source tab inside the IndeeHub app, **"Archipelago"**, listing every film
uploaded to *backstage*, streamable on any node — independent of the external
`indeehub.studio` API.

### Today (from the research)
- IndeeHub frontend (Next.js) is built against `NEXT_PUBLIC_API_URL =
  staging-api.indeehub.studio` and pulls media from AWS S3/CloudFront. It is
  **not Archipelago-aware**. nginx proxies it at `/app/indeedhub/` and injects a
  NIP-07 Nostr provider.
- A MinIO stack exists (`indeedhub-public` / `indeedhub-private` buckets); FFmpeg
  produces HLS there. **No backstage upload UI/code exists yet.**
- The design doc's Phase 4 already describes the target: backstage → FFmpeg → HLS
  → each `.ts` is a BLAKE3 blob → signed Nostr "Blossom" catalog event → any node
  resolves the content address and streams from the nearest holder; MinIO origin.

### Architecture — four pieces

**(i) Backstage upload + transcode (origin side).**
Minimal creator flow on a publisher node: upload master → FFmpeg → HLS
(`.m3u8` + `.ts`) into MinIO (reuse the existing `indeedhub-ffmpeg`/MinIO stack).
For each segment compute `blake3_hex` (`content_hash::blake3_hex`) and import it
into the iroh seed store (`IrohProvider::seed_and_advertise`, generalized beyond
releases). The playlist references segments by content hash.

**(ii) Signed film catalog on Nostr (the "Archipelago" source).**
Define a new addressable event — **kind 30082, `archy-film`** (sibling of the
30081 seed advert) — published by the publisher node, **signed via `trust/`**:
```jsonc
{
  "title": "...", "creator_did": "did:key:z...", "duration_s": 5400,
  "poster": "blake3:...",            // poster image blob
  "playlist": "blake3:...",          // the .m3u8 (itself a blob)
  "segments": ["blake3:...", ...],   // ordered .ts segment hashes
  "enc": { "scheme": "aes-128", "key_ref": "nip98" },  // see (iv)
  "monetized": { "service": "content-download", "sats_per_mb": 1 } // optional
}
```
The signature uses `trust::sign_detached`; consumers verify with
`trust::verify_detached`. **Publisher trust:** films show in the Archipelago tab
only from publishers on the node's trusted/federation set (or a pinned
"Archipelago film-root" key, mirroring the release-root anchor concept). This is
the key that stops the shared catalog from being a spam vector.

**(iii) Archipelago-local film API (makes it appear on every node).**
New RPC + HTTP endpoints in `api/`:
- `film.catalog` / `GET /api/film-catalog` — query Nostr relays for kind-30082
  events from trusted publishers, verify signatures, dedupe, return merged JSON.
  Cache like `app_catalog.rs` does (mtime/TTL, atomic write).
- `GET /api/film/:blake3` — serve a segment: `swarm::fetch_content_addressed`
  (swarm-assist → MinIO/OVH origin), BLAKE3-verified, with HTTP range support so
  the player can seek. This is where §1 (paid serving) and §2 (relay) plug in.
- The IndeeHub frontend gets an **"Archipelago" source** that points at
  `/api/film-catalog` instead of `indeehub.studio`. Cleanest: a small build/runtime
  flag or an injected config (same nginx `sub_filter` mechanism already used to
  inject the NIP-07 provider) that registers the Archipelago source alongside the
  existing studio source — additive, not a replacement.

**(iv) Encryption / access (private films).**
Public films: plaintext segments, freely cacheable, swarm-distributable. Private
films: keep AES-128 HLS; **untrusted seeds cache only ciphertext** (they never see
plaintext), and the decryption key is delivered per-viewer via NIP-98 auth (the
mechanism IndeeHub already uses) or NIP-44 DM. Payment (§1) gates *bytes*; the
*key* gates *plaintext* — two independent locks. This lets us pay strangers to
seed encrypted blobs without leaking content.

### "On every node" — propagation
Propagation is **pull**, not push: every node's `film.catalog` periodically queries
the same Nostr relays (already configured for discovery) for trusted-publisher film
events. A film uploaded on node A is therefore visible on node B as soon as B
refreshes its catalog — exactly how `app_catalog.rs` already distributes app
updates fleet-wide. No central server; the relays carry only signed metadata, the
blobs flow peer-to-peer with MinIO/OVH as origin.

---

## 4. Suggested end-to-end phasing

| Step | Deliverable | Risk | Reuses |
| --- | --- | --- | --- |
| **A** | Generalize `seed_and_advertise` beyond releases → arbitrary public blob scope (films) | low | swarm/ |
| **B** | `film.catalog` RPC + signed kind-30082 events + trusted-publisher gating | low–med | trust/, app_catalog.rs pattern |
| **C** | `GET /api/film/:blake3` range-streaming via swarm-assist + MinIO origin | med | swarm/, content_server.rs |
| **D** | IndeeHub "Archipelago" source wired to the local API (additive) | med (frontend, external repo) | nginx sub_filter |
| **E** | Backstage: upload → FFmpeg → HLS → blob import + catalog publish | med | MinIO/ffmpeg stack |
| **F** | **DONE** — paid swarm serving (`swarm/paid.rs` gates the blob protocol via `streaming/gate`); free by default | med | streaming/* |
| **F2** | Cross-mint settlement (§2a): `swap_between_mints` + payer-side auto-swap + mint trust list + fee cap | med–high | wallet/ecash, mint_client, lnd |
| **G** | Pin our own iroh relays (config) | low | iroh |
| **H** | Single-hop paid `relay.fetch` for film blobs | high | transport/, streaming/* |
| **I** | Multi-hop routing | high / deferred | — |

A→E delivers "films on every node" with free volunteer seeding (the design-doc
vision). F→H layer the sats economy on top. I is genuinely future work.

> **Shipping directive (user, 2026-06-17):** the IndeeHub "Archipelago" change
> ships — after testing — as a **decoupled app-catalog update**, NOT a binary
> OTA. Publish the new IndeeHub image + bump `releases/app-catalog.json` so every
> node gets the per-app "Update" badge (the mechanism in
> `container/app_catalog.rs` / `package.check-updates`). Node-side API changes
> (steps B/C) that need the binary go through the normal OTA; the *app* (step D,
> the IndeeHub frontend image) goes through the app catalog. See memories
> `project_decoupled_app_updates` + `reference_indeehub_canonical_source`.

---

## 5. Open questions / decisions needed

1. **iroh-blobs authorization granularity.** ✅ **RESOLVED (2026-06-17 spike).**
   iroh-blobs 0.103 exposes exactly the hook we need: `BlobsProtocol::new(&store,
   Some(EventSender))`. With an `EventMask` set to intercept, the provider asks our
   handler to authorize each request and we return `EventResult = Result<(),
   AbortReason>`:
   - `RequestMode::Intercept` / `InterceptLog` — per-blob-request allow/deny
     (`Err(AbortReason::Permission)` denies, `Err(AbortReason::RateLimited)` defers).
   - `ConnectMode::Intercept` — reject at the connection handshake (cheap pre-filter).
   - `ThrottleMode::Intercept` — per-request throttle/meter hook for byte accounting.
   - `RequestMode::Disabled` — hard-reject a whole request kind (e.g. disable `Push`
     so peers can never write into our store).
   → **§1 shape (A) is the recommended path** (native, no fork): the accept-side
   handler calls `streaming::gate::check_gate("content-download", peer_endpoint,
   bytes, token)` and maps `PaymentRequired`/`InsufficientPayment` →
   `Err(Permission)`, `Allowed`/`PaidAndAllowed` → `Ok(())`. Peer identity comes
   from the `Connection`'s remote endpoint id. (See `iroh_blobs::provider::events`.)
2. **Film-publisher trust anchor.** One global "Archipelago film-root" key (curated
   store, like release-root) vs. per-node trusted-publisher sets vs. both. Affects
   spam resistance and who can publish to *everyone's* Archipelago tab.
3. **MinIO as origin across the fleet** — single canonical MinIO on the hub vs.
   per-node MinIO with cross-seeding. The swarm makes per-node origin viable but
   the *first* upload needs a home.
4. **IndeeHub frontend is an external repo** (`~/Projects/indeehub-frontend`,
   built into `apps/indeedhub`). Adding an "Archipelago" source needs changes
   there; scope whether it's a build-time source registration or a runtime-injected
   config (preferred — keeps the node OS in control).
5. **Pricing defaults & free tier.** What's free (OTA, trusted peers, first N MB?)
   vs. paid, and the default sats/MB. `pricing.json` already supports this; needs a
   policy.
6. **Payment UX / auto-pay caps.** A downloading node auto-paying from its ecash
   wallet needs a user-set ceiling and a "prefer free origin if peer wants > X"
   rule, so streaming never silently drains the wallet.

---

## 6. Why this is tractable
The hard, slow-to-build substrate — an ecash wallet, a metered payment gate,
content addressing, a verifying swarm, signed discovery, a trust module, a
multi-transport stack — is **already in the tree and (for the swarm) just tested**.
The remaining work is wiring those together along the three axes above, with the
two new protocols (paid blob serving, single-hop relay) being the only substantial
net-new surface. Everything stays behind feature flags / opt-in config and obeys
the project's north star: **swarm-assist, origin always wins** — and now,
**free updates, optional paid films.**
