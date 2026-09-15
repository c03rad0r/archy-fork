# Incident — 2026-09-15: Minibits Cashu claim stuck retrying an already-redeemed token (archy-x250-pa3)

## Report

User: "The cashu server is unable to get it's tokens from nostr on
archipelago@archy-x250-pa3" — clarified as the Cashu **client wallet**
(Minibits `@minibits.cash` Lightning-address receive flow), not a mint
server. UI showed: *"a payment arrived but couldn't be redeemed yet (1)"*.

## Root cause

`wallet::minibits::claim_and_redeem` (`core/archipelago/src/wallet/minibits.rs`)
polls Nostr relays for NIP-04-encrypted Cashu tokens sent to the node's
`@minibits.cash` address, decrypts them, and redeems them at the mint. A
token that fails to redeem is kept in `MinibitsState.pending_claims` and
retried on the next poll — by design, so a *transient* failure (mint briefly
down, decrypt hiccup) never drops real money.

But one queued claim had already been redeemed (mint error **11001 "Token
Already Spent"** — most likely double-delivered by the relay, or redeemed
by an earlier run before a crash lost track of it). That's a *terminal*
condition, not a transient one: the code didn't distinguish the two, so it
retried the same dead claim every ~6 seconds forever:

```
WARN archipelago::wallet::ecash: Failed to swap proofs from mint https://mint.minibits.cash/Bitcoin:
     This ecash has already been redeemed — it can't be claimed twice.: {"code":11001,"detail":"Token Already Spent"}
WARN archipelago::wallet::minibits: Minibits claim decrypted but failed to redeem (...); will retry next poll
```

Confirmed via `sudo journalctl -u archipelago.service` on archy-x250-pa3, and
via `/var/lib/archipelago/wallet/minibits.json`, which had exactly one
`pending_claims` entry. Each poll also unconditionally queried all three
`CLAIM_RELAY_URLS` (`relay.minibits.cash`, `relay.damus.io`, `nos.lol`)
instead of the primary relay only, adding needless churn and leaking the
wallet's Nostr pubkey to two relays it didn't need to touch — `relay.damus.io`
was additionally failing NIP-42 auth / 503ing on every poll.

**No funds were at risk** — an already-redeemed token has zero remaining
value. The only symptom was a permanently stuck "couldn't be redeemed yet"
banner and wasted relay connections.

### Why this had already been "fixed" once and came back

This exact bug (terminal-11001 handling + relay-query reduction) was fixed
on 2026-09-09 on branch `feat/minibits-lnurl-receive` (commits `4e410d7`,
`489995c`) and pushed to `origin`. **That branch was never merged into
`main`.** `main` carries its own, independently-diverged rewrite of
`minibits.rs` that never got those two hardening fixes. archy-x250-pa3 OTA'd
to `1.8.16-alpha` (built from `main`) earlier on 2026-09-15, so the bug
resurfaced on the first replayed/double-delivered claim after that update.

## Fix

Two parts:

### 1. Immediate unstick (archy-x250-pa3, operational, no code change)

- Backed up `/var/lib/archipelago/wallet/minibits.json`.
- Stopped `archipelago.service`, emptied `pending_claims` (`[]`) in the
  state file, restarted the service.
- Verified via `journalctl` that polling resumed cleanly with no further
  "already been redeemed" warnings.

### 2. Code fix, ported into `main`

- **`core/archipelago/src/wallet/mint_client.rs`**: exposed the existing
  NUT error-code-11001 translation as a public constant,
  `ALREADY_REDEEMED_MSG`, so other modules can recognize it without
  duplicating the string.
- **`core/archipelago/src/wallet/minibits.rs`**:
  - Added `is_already_redeemed(&anyhow::Error) -> bool`, checking the error
    chain for `ALREADY_REDEEMED_MSG`.
  - In the claim redeem loop, a redeem failure matching
    `is_already_redeemed` is now dropped (logged at `info!`, not retried)
    instead of being pushed back onto `pending_claims`. Every other failure
    still retries next poll, unchanged.
  - `fetch_relay_dms` now connects to `RELAY_URL` (the Minibits relay)
    alone first via `try_connect_relay`, and only adds the two public
    fallback relays (`relay.damus.io`, `nos.lol`) if that primary relay is
    unreachable. Also paginates the DM fetch (200/page, capped at 5 pages)
    so a backlog larger than one page can't silently strand older DMs
    behind an un-advanced watermark.

Deliberately **not** ported from the unmerged branch: its `STATE_LOCK`
skip-if-busy guard and per-claim attempt-count backstop. `main`'s existing
`MINIBITS_STATE_LOCK` already fully serializes claim polls (blocks rather
than skips — a different but equally valid way to close the same race), and
an attempt-count backstop would have required reshaping the `PendingClaim`
enum for marginal extra protection beyond what the 11001 fix already covers.

## Verification

- `cargo build -p archipelago` — clean, no new warnings.
- `cargo test -p archipelago --bin archipelago wallet::minibits` — existing
  suite still green (see PR/commit for the run).
- Live on archy-x250-pa3: claim poll loop confirmed quiet post-unstick
  (only `relay.minibits.cash` connects logged, no redeem-failure warnings).

## Lesson (recorded in memory)

A fix that lives only on an unmerged feature branch is not a fix that's
actually deployed. Before trusting a memory or changelog claim that
something "shipped," check which branch the running/released build was
built from (`git log <branch>..main` / `main..<branch>`) rather than
assuming a pushed branch was merged.
