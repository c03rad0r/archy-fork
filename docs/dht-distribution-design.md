# DHT / Peer-Distributed Content Design

**Status:** partially implemented — **not** "no code yet" as this line previously
read. `core/archipelago/src/swarm/` exists (`mod.rs`, `iroh_provider.rs`,
`paid.rs`, `paid_alpn.rs`, `payment.rs`) along with `content_hash.rs`, behind the
**default-off** `iroh-swarm` cargo feature (`Cargo.toml:21` — the iroh/iroh-blobs
deps are optional and only pulled in by that feature). `config.swarm_enabled`
gates it at runtime and also defaults off, so a stock build ships this inert.
Treat the phases below as design; check the feature flag before assuming a phase
is live. · **Date:** 2026-06-16 · **Author:** archipelago + Claude

## 1. Purpose

Make Archipelago's large-file movement **peer-distributed**: a node should be able to
fetch content (OTA updates, app/OCI images, IndeeHub films) from *any other node that
already has it*, falling back to the central origin only when no peer can serve it.

This document covers three use-cases that are **the same problem** —
"fetch content-addressed bytes from whatever node already has them, verify, fall back to
origin":

1. **OTA releases** — node binaries + frontend tarballs.
2. **App installs** — container/OCI images.
3. **IndeeHub streaming** — films created in "backstage" on one node, streamable from any
   node that has them stored or cached.

### Guiding principle (decided 2026-06-16)

> **Swarm-assist, origin always wins.** The peer swarm is an *optimization*. The central
> origin (OVH HTTP release assets / MinIO) remains the **guaranteed fallback** and the
> source of truth for reliability. We never bet correctness or availability on the P2P
> layer. This is what keeps the system bulletproof while the P2P stack matures.

## 2. Current state (verified 2026-06-16)

### OTA (`core/archipelago/src/update.rs`)
- Manifest at `DEFAULT_UPDATE_MANIFEST_URL` (`update.rs:67`) = vps2 OVH
  (`source.archipelago-foundation.org/lfg2025/archy/raw/branch/main/releases/manifest.json`).
- `check_for_updates()` (`:565`) walks an operator mirror list (`default_mirrors()` `:105`,
  `load_mirrors()` `:123`), origin-rewrites component URLs to the chosen mirror
  (`rewrite_manifest_origins()` `:227`).
- `download_component_resumable()` (`:821`) — resumable HTTP Range download, 6 retries,
  exponential backoff.
- **Integrity: SHA-256 only** (`:984`), compared against `ComponentUpdate.sha256`.
- **No authenticity:** manifests are *unsigned*. A compromised mirror can serve a malicious
  but hash-consistent binary. Post-apply health probe + auto-rollback exist
  (`verify_pending_update()` `:389`, `rollback_update()` `:1423`) but that is not a
  substitute for signature verification.
- Manifest schema: `{version, release_date, changelog[], components[{name, current_version,
  new_version, download_url, sha256, size_bytes}]}`.

### App installs (`core/archipelago/src/api/rpc/package/install.rs`)
- `handle_package_install()` (`:195`) → `do_pull_image()` (`:1062`) tries each registry from
  `container/registry.rs` in priority order (OVH primary), `rewrite_image()` rewrites the
  origin, `podman pull`. Same centralized-mirror shape as OTA.

### Transport & identity (already P2P-capable)
- `transport/mod.rs` — `NodeTransport` trait (`:74`), `TransportRouter` (`:336`), priority
  stack Mesh→LAN→FIPS→Tor. `PeerRegistry` (`:199`) tracks per-peer addresses
  (mesh id, LAN ip:port, `fips_npub`, onion).
- Seed-derived identity (`seed.rs`): node Ed25519 (`archipelago/node/ed25519/v1`), node
  Nostr secp256k1 (`archipelago/nostr-node/secp256k1/v1`), FIPS secp256k1
  (`archipelago/fips/secp256k1/v1`). DID + npub per node.
- **Already content-addressed:** `blobs.rs` stores `blobs/<cid>` keyed by **SHA-256** hex,
  with HMAC-SHA256 capability tokens (`BlobMeta`, 64 MiB cap). `transport/chunking.rs` does
  Reed-Solomon chunking for LoRa.

### Trust scaffolding — **NOT built yet**
- No `core/src/trust/`, no `ROOT_PUBKEY`, no `derive_release_root_*`, no
  `archipelago/release/root/*` HKDF strings, no JCS/canonical JSON, no signing ceremony
  scripts, no `manifest-v2.json`. The "Phase 0 signed manifest" design exists only as notes.

### IndeeHub (the streaming target)
- Original platform (not a fork). Working source: `~/Projects/Indeedhub Prototype/`
  (Vue 3 + NestJS). Submodule `source.archipelago-foundation.org/lfg2025/indeehub.git` (repointed off the retired host —
  needs a live remote). In `archy`: image-only, `apps/indeedhub/manifest.yml` pulls
  `source.archipelago-foundation.org/lfg2025/indeedhub:1.0.0` (+ `-api`, `-ffmpeg`, postgres, redis,
  minio, nostr-rs-relay).
- Streaming today: FFmpeg → **HLS (.m3u8 + AES-128 .ts segments)** in **MinIO**
  (`indeedhub-private`/`-public`), metadata in Postgres, transcode queue in Redis,
  auth via Nostr (NIP-98). Glue: `install.rs:68` `patch_indeedhub_nostr_provider()`
  injects the NIP-07 provider into the nginx-wrapped frontend.
- **No "backstage" code yet** — it's the creator/upload side we're introducing.

## 3. Protocol evaluation (verified maintenance status, 2026-06-16)

| Option | Verdict | Why |
| --- | --- | --- |
| **Web5 / TBD / DWN** | ❌ Reject | Block **wound TBD down**, handed components to DIF (`TBD54566975`→`decentralized-identity`). `web5-js` latest release **0.12.0, Oct 2024** (~20 mo stale). DWN spec still **Draft**. DWNs are DID-scoped *record stores*, not a blob-streaming swarm. Fails the "well-maintained + bulletproof" bar. |
| **iroh / iroh-blobs** | ✅ Swarm engine | **v1.0.0 shipped 2026-06-15.** Rust (matches core), **BLAKE3 verified streaming** over **QUIC + hole-punching + relays**, content-addressed, KB→TB, **native byte-range** support (ideal for HLS). n0 team, production relays. |
| **Nostr Blossom** | ✅ Index/catalog layer | SHA-256-addressed blobs over HTTP, modular BUD specs (BUD-01/02/04/05/06/08), actively developed, **already aligned** (Nostr identity everywhere; `blobs.rs` already SHA-256). Server-centric (not a peer swarm) → use as discovery + IndeeHub catalog + HTTP fallback, not the distribution engine. |
| **libp2p-kad (hand-rolled DHT)** | ⚠️ De-prioritize | Was the old "Phase 4 build a Kademlia" plan. iroh 1.0 supersedes the need to hand-roll discovery + swarm. Revisit only if iroh proves unworkable. |

**Note vs. prior plan:** the saved DHT design said "no iroh as a Phase 0–5 dep (revisit
post-Phase 3)." iroh hitting 1.0 removes the main reason for that deferral — **this design
reverses that non-choice** and adopts iroh as the swarm layer, collapsing the from-scratch
Kademlia work.

## 4. Recommended architecture — three layers, one engine

Build **one** peer-distribution layer; use it for all three use-cases.

```
                ┌─────────────────────────────────────────────┐
   Authenticity │  Signed Nostr events (per-node npub) +       │   "who published this,
   & Discovery  │  seed-derived RELEASE ROOT key for OTA +     │    who has it"
                │  Blossom BUD catalog for IndeeHub            │
                └─────────────────────────────────────────────┘
                ┌─────────────────────────────────────────────┐
   Integrity &  │  BLAKE3 content addressing (iroh-native,     │   "name bytes by hash,
   Addressing   │  range-verifiable). SHA-256 kept in manifest │    verify on arrival"
                │  during migration window.                    │
                └─────────────────────────────────────────────┘
                ┌─────────────────────────────────────────────┐
   Transport &  │  iroh-blobs swarm (peers that already have   │   "move the bytes"
   Swarm        │  it)  ─── fallback ───▶  OVH HTTP / MinIO    │
                │                          origin (ALWAYS wins) │
                └─────────────────────────────────────────────┘
```

- **Integrity/addressing — BLAKE3.** iroh-native, supports verified *range* streaming
  (essential for HLS + resumable). Keep SHA-256 in the manifest for back-compat through the
  migration window; add a `blake3` field alongside.
- **Discovery/authenticity — signed Nostr events + release root key.**
  - OTA: the **Phase 0 seed-derived release root key** signs the manifest (BLAKE3 root hash
    + version). Integrity ≠ authenticity — content addressing proves *bytes are intact*, the
    signature proves *we authorized them*. Both are required.
  - "Who has blob X" advertised via signed Nostr events `{content-hash, provider-npub, ts}`,
    so nodes find seeds without a central tracker.
  - IndeeHub: Blossom BUDs for the film catalog + provider/mirror lists.
- **Transport/swarm — iroh-blobs, origin fallback.** Node asks the swarm for a hash; peers
  that have it serve range-verified BLAKE3 streams; if the swarm yields nothing, fall back to
  the existing resumable HTTP path (`update.rs:821`) against OVH/MinIO. **A node that
  finishes a download automatically becomes a seed.**

### Bulletproof posture
The swarm sits *above* a proven HTTP path, never in place of it. Worst case (every peer
offline, iroh bug, NAT failure) the node downloads exactly as it does today. iroh 1.0 is new;
this containment is deliberate.

## 5. Use-case flows

### OTA / app installs
1. Node reads the **signed** manifest (via signed Nostr event or HTTP), gets BLAKE3 root hash
   + release-root signature; verify signature → reject on failure.
2. Query swarm (signed provider events) for peers holding that hash.
3. Download range-verified BLAKE3 stream from peers; verify full BLAKE3 (+ SHA-256 during
   migration).
4. No peers / failure → resumable HTTP from OVH (current path).
5. Apply + health-probe + auto-rollback (unchanged). Updated node **becomes a seed**.
6. OCI images: content-address image layers the same way; OVH registry stays the origin.

### IndeeHub streaming ("backstage → any node")
1. Creator publishes a film in **backstage** → FFmpeg → HLS; **each .ts segment is a
   content-addressed (BLAKE3) blob**, immutable and small → ideal swarm objects.
2. Publish a **signed Nostr event** advertising title + segment hashes (Blossom catalog).
3. Any node running IndeeHub resolves the content address and **streams from the nearest
   node(s) that have it stored/cached** via iroh range streaming; MinIO/OVH is origin.
4. AES-128 key delivery + NIP-98 auth unchanged (keys gate decryption; swarm only moves
   encrypted segments — so untrusted seeds can cache without seeing plaintext).

## 6. Phasing (folds into the existing Phase 0–6 plan)

0. **Signed manifests (required first, unbuilt).** `derive_release_root_ed25519` /
   `derive_release_root_nostr` in `seed.rs` (HKDF `archipelago/release/root/ed25519/v1`,
   `.../secp256k1/v1`); `core/src/trust/` (anchor/bundle/manifest/timestamp/nostr); JCS
   canonical JSON; ceremony scripts; `manifest-v2.json` with signature. Gives *authenticity*,
   which content-addressing does not.
1. **BLAKE3 alongside SHA-256** in the manifest + `blobs.rs`.
2. **iroh-blobs PoC** behind a feature flag: serve OTA blobs from the swarm with HTTP
   fallback; measure on a scratch/test node, then the fleet.
3. **Signed Nostr advertisement events** for releases (publisher identity + provider lists).
4. **IndeeHub on the same blob layer** (Blossom catalog + iroh swarm; MinIO origin).

This collapses the old "Phase 4: build S/Kademlia from scratch" into "adopt iroh," a large
de-risking.

## 7. Open decisions

- **BLAKE3 migration scope:** dual-hash window length; whether to re-hash historical
  releases or only BLAKE3 going forward.
- **iroh ↔ existing transports:** iroh brings its own QUIC + hole-punching + relays; decide
  how it coexists with FIPS/Tor (run iroh standalone first; integrate with `TransportRouter`
  later if useful).
- **Seed retention policy:** how long nodes keep blobs to seed others (disk pressure on small
  nodes); pinning rules for IndeeHub films vs. transient OTA blobs.
- **Privacy:** iroh dial-by-key vs. Tor's anonymity; default transport per content type.

## References
- iroh: https://github.com/n0-computer/iroh · iroh-blobs: https://github.com/n0-computer/iroh-blobs · docs: https://docs.iroh.computer/protocols/blobs
- Blossom: https://github.com/hzrd149/blossom · NIP-B7: https://nips.nostr.com/B7 · nostr-blossom (Rust): https://docs.rs/nostr-blossom
- Web5/DWN (rejected): https://github.com/decentralized-identity/web5-js · https://identity.foundation/decentralized-web-node/spec/ · https://block.xyz/inside/block-contributes-digital-identity-components-to-the-decentralized-identity-foundation
