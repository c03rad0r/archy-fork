# Cashu: move to the reference implementation (`cashu` / CDK)

Status: **planned** (2026-08-17). Scoped from the framework-pt keyset incident.

## Why

The node's Cashu support is hand-rolled: the NUT-00 token codec
(`wallet/cashu.rs`), the blind-DH crypto (`wallet/bdhke.rs`) and the mint HTTP
client (`wallet/mint_client.rs`) are ours. That was fine while the protocol
was small; it isn't any more.

The cost showed up on 2026-08-17: a Minibits token could not be redeemed
because it carried a **NUT-02 v2 keyset id truncated to 8 bytes**. The mint —
itself CDK-based — rejected the swap with `NUT02: ID length invalid`. The
reference implementation has carried the resolver for this exact case
(`Id::from_short_keyset_id`) since v0.11, and requires the mint's keyset list
to turn a token into proofs, so it *cannot* forward a truncated id. We shipped
an equivalent repair by hand (`2277fc46`); the general lesson is that we are
tracking a moving spec on our own, and we keep finding out where we lag from
production failures.

Other gaps we carry today:

- **No DLEQ verification.** V4 tokens' `d`/`w` fields are parsed and discarded,
  so we cannot prove a mint signed with the keyset it claims.
- **No deterministic secrets (NUT-13).** `bdhke::generate_secret` is pure
  randomness, which means **the ecash wallet cannot be restored from a seed** —
  losing `wallet/ecash.json` loses the coins.
- We emit only `cashuA` (V3); most wallets now default to `cashuB` (V4).

## What we adopt

**The `cashu` crate alone** (MIT, from cashubtc — the crate CDK itself is built
on), *not* the full `cdk` wallet with its `WalletDatabase`.

That gives us, from audited upstream code:

- `nut00::Token` — encode **and** decode both `cashuA` and `cashuB`, plus
  `to_v3_string()` for older wallets
- `nut02::{Id, KeySetVersion, KeySetInfo}` — correct v1 (8-byte) and v2
  (33-byte) ids, and `Id::from_short_keyset_id` for the truncated case
- `dhke::{hash_to_curve, blind_message, unblind_message}` — the crypto, with
  upstream's test vectors
- DLEQ verification
- NUT-13 deterministic secrets

It is a light dependency: ~7 transitive crates, no `reqwest`, no runtime
opinions. `bitcoin ^0.32.2` matches our pinned `=0.32.5`, so secp256k1 stays a
single copy in the tree.

### What we deliberately keep

- **`wallet/ecash.rs` and every on-disk file, unchanged.** `wallet/ecash.json`,
  `accepted_mints.json`, `pending_swaps.json`, `swap_liquidity.json` keep their
  exact schemas — including `StoredProof`'s flattened shape and the capital-`C`
  field. Balances, history and trusted mints survive the update untouched.
- **`EcashTransaction`** — Fedimint and Ark write the same struct; it is a
  cross-backend contract, not Cashu-private.
- **`MintClient`'s HTTP surface**, its Tor-capable `with_client` seam, and the
  NUT error-code → plain-English table (which is better UX than upstream's raw
  errors).
- **Our multi-mint logic** — `swap_between_mints`, `plan_payment`, the
  liquidity cache and the crash-safe swap journal have no upstream equivalent.

### What we do NOT adopt, and why

`cdk::wallet::Wallet` requires implementing `WalletDatabase` — ~50 methods.
Roughly 30 have no home in our format (keyset caches, NUT-13 counters, sagas,
uuid-keyed reservations, KV, P2PK), and the ~20 that do map are lossy in both
directions on exactly the records that hold real money (proof state, and a
transaction type shared with two other backends). A permanent compatibility
shim over live funds is the wrong trade. If we ever want the full wallet, it
should come with a one-way format migration, decided separately.

## Seed backup, derived from the node seed

Today the ecash wallet has no seed and cannot be recovered. With NUT-13 it can,
and it should not introduce a second thing for the operator to write down:

- Derive the Cashu wallet seed from the **existing node master seed** over a
  dedicated BIP32 path, alongside the node's other derived keys. The node's
  24 words then already back up the ecash wallet — nothing new to record, and a
  restored node re-derives the same secrets.
- Persist the per-keyset NUT-13 counter (upstream's `increment_keyset_counter`)
  in a new sidecar file. It is recovery metadata, not funds: a lost counter
  costs a restore scan, not coins.
- Surface it in the UI the way the node seed already is: a "back up / restore
  ecash" path that states plainly that the node's recovery phrase covers it.

**Open question for the operator:** whether to *also* allow an independent
wallet mnemonic, for someone who wants ecash separable from the node identity.
Default should be derived-from-node.

## Options this unlocks (worth considering, not committed)

- **Run our own mint.** Upstream ships `cdk-mintd`. Packaged as an app it would
  make a node its own Cashu mint — the same shape as the `fedimint-clientd`
  sidecar, and launchable from the dashboard if it has a UI. Independent of the
  wallet work here.
- **P2PK / locked tokens** (NUT-11) — send ecash only a specific pubkey can
  redeem, which fits the mesh/federation identity we already have.
- **Multi-unit** support beyond sats.

## Sequencing

1. ~~Repair truncated v2 keyset ids so the failing case works now~~ — done,
   `2277fc46`.
2. Swap `wallet/cashu.rs` + `wallet/bdhke.rs` internals for the `cashu` crate,
   keeping every public signature and the on-disk contract. Existing tests in
   both modules stay as the regression net; add upstream's DLEQ vectors.
3. Emit `cashuB` (V4) by default, keep `cashuA` for compatibility.
4. NUT-13 deterministic secrets + node-seed derivation + the restore path.
5. Then reconsider `cdk-mintd` and NUT-11 as separate features.

Verify each step against a real mint on a test node before the fleet, and keep
`wallet/ecash.json` from a pre-migration node to prove it still loads.
