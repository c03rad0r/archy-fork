# Workstream B — Signed app-catalog: completion runbook

**Status: ✅ COMPLETE** (runbook retained for re-running the ceremony — key
rotation, a new publisher, or a fresh release root).

The ceremony described below has been performed. Verified 2026-08-08:

- The anchor is **pinned** — `trust::anchor::RELEASE_ROOT_PUBKEY_HEX` is a
  `Some(...)`, not `None`.
- `releases/app-catalog.json` carries a `signature` and a `signed_by` did:key.

Everything below therefore describes how to *do* the ceremony, not work that is
outstanding. The one-way-door warning in "Why this is gated on you" still
applies in full to any re-run: once a binary pins an anchor, a catalog signed by
a different key is hard-rejected fleet-wide.

---

**Original status (2026-06-28):** The registry-distributed manifest pipeline is live — nodes fetch
`releases/app-catalog.json` from the OTA mirror and embed manifests (origin-wins, disk
fallback). What remains for Workstream B is **authenticity**: pin the release-root anchor and
ship a *signed* catalog so nodes can cryptographically verify the publisher.

Today the catalog is **accepted unsigned** ("migration window") and the anchor is **unpinned**
(`core/archipelago/src/trust/anchor.rs` → `RELEASE_ROOT_PUBKEY_HEX = None`). Completing B is
a coordinated ceremony that **only the publisher can run** — it needs the offline
`RELEASE_MASTER_MNEMONIC`, which is not (and must not be) stored on any node or build host.

## Why this is gated on you (not automatable)

- The signing key is an **offline mnemonic** you hold (`archipelago ceremony gen` output, backed
  up offline / via `seed.reveal`). It is intentionally absent from the repo and all hosts.
- Order matters: once a binary **pins** the anchor, a catalog carrying a signature from the
  *wrong* key is **hard-rejected fleet-wide** (`trust/signed_doc.rs:79`). Unsigned and
  correctly-signed catalogs are both accepted; only a *mismatched* signature breaks nodes.
- So the pinned pubkey and the signature MUST come from the same key, shipped consistently.

## The ceremony (run from `core/`, with your mnemonic)

```bash
# 0. (only if you don't already have a release-root key) generate one and back the
#    mnemonic up OFFLINE. Prints the pubkey hex + signer did:key.
cargo run --release -p archipelago -- ceremony gen

# 1. Print the release-root pubkey hex for the anchor (idempotent; same mnemonic → same key)
RELEASE_MASTER_MNEMONIC="word1 word2 …" cargo run --release -p archipelago -- ceremony pubkey
#    → copy the 64-char hex.

# 2. Pin it in code:
#    core/archipelago/src/trust/anchor.rs:21
#    -  pub const RELEASE_ROOT_PUBKEY_HEX: Option<&str> = None;
#    +  pub const RELEASE_ROOT_PUBKEY_HEX: Option<&str> = Some("<64-char-hex-from-step-1>");

# 3. Sign the published catalog in place (inserts `signature` + `signed_by` over the
#    canonical JSON — re-run after ANY catalog regen, since signing covers the exact bytes):
RELEASE_MASTER_MNEMONIC="word1 word2 …" \
  cargo run --release -p archipelago -- ceremony sign releases/app-catalog.json

# 4. Verify locally before shipping (optional sanity): a node build with the pinned anchor
#    should log "app-catalog: release-root signature verified (<did>)" rather than
#    "self-consistent but anchor not pinned".
```

## Ship order (backward-compatible)

1. Commit the **signed** `releases/app-catalog.json` + the `anchor.rs` change together.
2. Push the signed catalog to the OTA mirror (gitea-vps2 `main`) — old binaries (no pinned
   anchor) still accept it (verified-but-unconfirmed); nothing breaks.
3. Build + OTA the binary with the pinned anchor. New nodes now **verify** the catalog against
   the anchor. (This is the normal release path — gate the tag per the ship-ritual.)
4. **Later / optional hardening:** once the whole fleet is on the pinned-anchor binary, flip
   the policy from "accept unsigned (migration window)" to "reject unsigned" in
   `container/app_catalog.rs` (the `SignatureStatus::Unsigned` arm). Do this LAST — while any
   node still runs an unsigned catalog it must keep being accepted.

## Env-override escape hatch (no rebuild)

For staging/canary you can pin the anchor without editing code via
`ARCHY_RELEASE_ROOT_PUBKEY=<hex>` (`trust/anchor.rs:23`) on a single node, then sign the catalog
and confirm that node verifies it before baking the constant in.

## What's already done (so this is the only remaining step)

- Catalog distribution + manifest embedding: live (this session's `169ff2e2` published the
  corrected catalog to the mirror).
- `ceremony gen|pubkey|sign` tooling: shipped (`core/archipelago/src/ceremony.rs`).
- Verify path: `trust::verify_detached` accepts unsigned, verifies signed against the anchor,
  hard-rejects mismatches (`trust/signed_doc.rs`).
- Detached-signature schema fields (`signature`/`signed_by`) already part of the signed
  preimage (`container/app_catalog.rs`).
