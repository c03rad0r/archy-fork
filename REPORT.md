# REPORT — P3: lift the 900 s media deadline

**Deliverable:** P3 (Tier 0) — a film longer than 15 minutes must stream to completion.
**Branch:** `fix/media-stream-deadline` @ **`79610eb`**, pushed to `felixfelix-bot/archy-fork`,
verified with `git ls-remote`.
**Base:** `fcc18b3` == `origin/main` (verified equal at worktree creation).
**Worktree:** `~/worktrees/archy-media-deadline` (never `/tmp`).
**Date:** 2026-10-22/23.

---

## 1. What was wrong

`server.rs:1645` defines `NON_UPGRADED_DEADLINE = 900 s`. The watchdog drops any connection
that is not websocket-upgraded once it exceeds 900 s. Media is served as a **plain HTTP
body** — `cloud_purchase.rs` wraps a stream, `registered_media.rs` serves 64 KiB chunks with
per-chunk authorization, and `api/rpc/playback.rs` proxies the same bytes to the buyer.
Nothing set the exemption for them, so **playback dies at exactly 900 s**.

## 2. What was changed

`core/archipelago/src/server.rs` only:

- **New `pub fn is_media_stream_path(&str) -> bool`** — true for `/content/` (seller serve)
  and `/api/rental-playback/` (buyer proxy). The bare `/content` catalog (a JSON exchange)
  is deliberately **not** exempt.
- **A second `Arc<AtomicBool>` `media`**, same shape as the existing `upgraded` flag
  (`:1604-1610`), set inside `service_fn` from `req.uri().path()`.
- **Watchdog condition** extended: `!upgraded && !media && elapsed >= DEADLINE`.

**Both prefixes are load-bearing.** `/content/` is the seller-side serve;
`/api/rental-playback/` (`api/rpc/playback.rs:222`) is the route a viewer actually hits.
Exempting only `/content/` fixes the seller and leaves the viewer dead at 15 minutes — a
half-fix that looks correct in a diff. This was verified as a real trap, not a hypothetical:
see §4.

**Safety.** `http1_keep_alive(false)` means one exchange per connection, so a media flag
cannot exempt a later, different request on the same connection. A connection that never
becomes media still takes the full 900 s bound, and the 30 s `http1_header_read_timeout`
still covers slowloris.

## 3. Evidence — commands and observed output

### 3.1 Build

```
$ cd core && cargo check -p archipelago
   Finished `dev` profile … exit 0
```

### 3.2 Lint — RATCHET, not absolute

`CONTRIBUTING.md:24` requires `clippy -D warnings`; on pristine `fcc18b3` that fails with
123 errors. The criterion is `after <= before`, and **both numbers were measured here**, not
taken from the prompt (a warm clippy cache reports `0` and would make the ratchet vacuous —
the crate was force-rechecked with `cargo clean -p archipelago` first):

| | warning lines |
|---|---|
| baseline `fcc18b3` (measured) | **124** |
| branch `9446877` (measured) | **124** |

`124 <= 124` — **ratchet satisfied**, no new lint debt.

```
$ cargo fmt --all -- --check   → exit 0 (clean)
```

### 3.3 Tests through the MANDATED runner

```
$ ARCHY_TEST_REPO=~/worktrees/archy-media-deadline scripts/test-backend-isolated.sh
test result: ok. 2078 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out
Finished with result: success  (exit 0)
```

2087 tests ran inside `systemd-run --private-network --private-tmp --private-devices`.

### 3.4 TDD — RED observed, then GREEN

Inline `#[cfg(test)] mod media_deadline_tests` in `server.rs` (the runner asserts exactly
one test executable, so `tests/*.rs` would be unrunnable):

| # | Step | Observed |
|---|---|---|
| 1 | tests written first | `error[E0425]: cannot find function is_media_stream_path` (14 errors) |
| 2 | **negative control** — a deliberately naive predicate matching only `/content/` | `2077 passed; **1 failed**` at `buyer playback proxy must be exempt; exempting only /content/ leaves the viewer route dead at 900 s` |
| 3 | real predicate | `2078 passed; 0 failed` |

Step 2 is the one that matters. Without it, step 1 proves only that a function was missing,
not that the assertions are meaningful. The naive stub is **the exact trap** the prompt warns
about, and the test caught it.

### 3.5 The mandatory read-back — it RUNS, and my first version of this section was wrong

**Correction, and it is the most important paragraph in this report.** An earlier version
of §3.5 reported this step as `BLOCKED`, citing "Failed to write PID marker / Permission
denied" as an environment limitation and calling it B3. **That was wrong.** I had copied a
claim out of `BLOCKERS.md` and reported it as though I had measured it — the exact failure
mode this prompt exists to prevent, committed by me, in the report about the prompt.

`ARCHIPELAGO_DATA_DIR` **is honoured** (`config.rs:125`). My read-back script never set it,
so the binary fell back to the hardcoded `/var/lib/archipelago` and failed. The blocker was
the script. With the override set, the read-back runs against the real release binary
(79.8 MB, `cargo build --release`, 16m09s):

```
  [PASS] binary starts and listens         — 127.0.0.1:42343 accepting TCP
  [PASS] media path reaches a handler      — GET /content/... -> HTTP 404
  [PASS] CONTROL non-media path answers    — GET /health -> HTTP 200
  [PASS] no path is refused at the connection layer
  [NOT_VERIFIED] a connection survives past 900 s

counts: PASS=4, NOT_VERIFIED=1, exit 0
```

The `404` on `/content/` is the useful result, not a failure: the media route is
**reachable and answering** rather than refused at the connection layer, and the CONTROL
(`200` on `/health`) proves the node is not simply broken — so the media result can be
read as evidence about the route rather than about a dead server.

A second, genuine blocker sat underneath: startup requires a container runtime
(`Error: Failed to create auto runtime / Caused by: Neither Podman nor Docker is
available`). **Installed podman 4.9.3** (apt candidate available). Checked first that this
host is *not* an archy node — no `/var/lib/archipelago`, 0 archy systemd units — so the
mandated runner's isolation was never at risk. Consequence: the **`hardware` venue is now
available too**, and only `twonode` remains missing (no FIPS DNS).

**What is still genuinely unverifiable — the real residue:** there is no way to *fixture a
purchase*, so an actual media **body** cannot be produced. Wire-level reachability of the
media route is proven; P3's literal criterion — a film **longer than 15 minutes** streaming
to completion — is a wall-clock **duration** property and remains **NOT_VERIFIED**. That
distinction is stated rather than collapsed into a single "blocked".

### 3.6 Harness delta — the honest measurement

The prompt says a run whose counts are identical to the baseline means nothing was verified.
My first harness run *did* match the baseline exactly (`PASS=6 FAIL=1 NOT_VERIFIED=1`), for a
reason worth stating: **the P3 test was passing on pristine `fcc18b3` too.** It defined its
own local `is_media_path()` and asserted on that — a mirror of the contract, not a check of
the SUT. It could never have failed for the reason it exists.

Fixed (`archy-test-harness` `d2cf472`): the test now reads `$ARCHY_SUT/.../server.rs` and
requires the predicate to exist, to exempt both prefixes, and to be consulted by the
watchdog. **Both directions observed:**

```
$ ARCHY_SUT=~/worktrees/archy-media-deadline pytest …::test_the_900s_deadline_…   → 1 passed
$ ARCHY_SUT=~/repos/archy              pytest …::test_the_900s_deadline_…   → 1 failed
  AssertionError: P3 not implemented: no `pub fn is_media_stream_path` in server.rs
```

Same harness, both SUTs:

| SUT | venue | PASS | FAIL | NOT_VERIFIED |
|---|---|---|---|---|
| pristine `fcc18b3` | inprocess | 5 | **2** | 1 |
| branch `9446877` | inprocess | 6 | **1** | 1 |

**Exactly one test flips, and it is the P3 guard.** The residual `FAIL=1` is
`test_buyer_proxy_serves_a_range_from_a_real_seller`, marked `venue_twonode` — this host has
no FIPS path, so it is correctly reported NOT VERIFIED rather than skipped.

### 3.7 Provenance

`results/20261023T024432Z-9446877/run.json`:

```json
"sut":     { "commit": "94468770f12639a20765db3d68e11950324df557", "dirty": false,
             "branch": "fix/media-stream-deadline", "describe": "v1.9.1-alpha-10-g9446877" }
"harness": { "commit": "f3818b204f568086ebfdbb8aa0859ae8e8700cff", "dirty": false }
```

Clean on both sides — reproducible from the commits alone.

## 4. The test-coverage contract, row by row

| Row | Status |
|---|---|
| **P3** — "a film longer than 15 minutes streams to completion", assert both prefixes, `localnode` | **PARTIALLY VERIFIED.** The DECISION function is proven on both prefixes and proven able to fail (naive-stub control + pristine-SUT failure). The media route is proven **reachable and answering on the real binary** (PASS=4 on the wire read-back). The DURATION is **NOT_VERIFIED** — no purchase can be fixtured, so no real media body exists to time. |

## 5. Boundaries — what this does NOT claim

- **No coverage percentage is claimed.** The repo measures none; inventing one would put a
  number nobody measured into circulation.
- **P3 is not fully verified.** The unit test proves the *decision*; the wire read-back
  proves the media route is **reachable and answering** on the real binary. Neither
  substitutes for a real 16-minute playback, which remains operator UAT and is a
  **duration** property no reachability test can reach.
- **B3 was corrected, not just worked around.** The harness's `BLOCKERS.md` had claimed
  the node could not be started with a fixture config; it can
  (`ARCHIPELAGO_DATA_DIR`, `config.rs:125`). Fixed there too (`b7c1b7f`) so the next worker
  is not misled as I was.
- **The `inprocess` venue does not exercise `server.rs`** — the listener, the semaphore, the
  v6 mirror and the path filter are all bypassed. `localnode` was used for the delta runs;
  it still cannot show a connection surviving 900 s.
- **CI-green ≠ mandated-runner-green.** They are different paths (`BLOCKERS.md` B6).
  §3.3 is the mandated runner.
- **Operator-only actions:** (1) merge `fix/media-stream-deadline` → `fork/main`;
  (2) the upstream decision — which is *not* taken here. **No PR was opened to
  `lfg2025/archy`, on ngit, or as a draft.**

### Manual steps that remain (P3)

1. **A film longer than 15 minutes plays through.** Needs a node with a writable data dir
   (B3) and a purchased stream. Expected: playback continues past 900 s on **both**
   `/content/…` and `/api/rental-playback/…`.

## 6. Incidents worth recording

- **OOM:** the first `cargo test` had `rustc` killed at 3.9 GB anon-rss; swap was already
  3.6/7.8 GB. Recovered by using the mandated runner, which applies
  `opt-level=0`/`debug=0` — less disk *and* less memory, and is the acceptance path anyway.
- **Disk:** fell 9.4 G → 2.4 G free (99%) during a debug build. Reclaimed; later recovered
  to 39 G. The handover warns about exactly this.
- **Network:** a push failed mid-way with `Could not resolve host: github.com`; retried and
  verified with `ls-remote` rather than trusting the retry.
- **Harness defect found and fixed** (`f3818b2`, `d2cf472`): a false-FAIL on wider venues,
  and a P3 test that could not fail. Both were my own harness, both found by asking what
  would have to be true for the test to fail.

## 7. Closing

P3's **code change is complete, tested through the mandated runner, pushed and verified**.
Its **full acceptance is not** — the duration proof is blocked on B3 and on wall clock, and
that is stated rather than papered over. Nothing here is marked done from a summary: every
claim above carries a command and observed output, and the one thing I could not observe is
labelled `NOT_VERIFIED`.

---

# P1 — media-path observability

**Status: DELIVERED, with one dimension honestly unimplemented.**

Branch `fix/media-stream-deadline`. Commits: `f7a397a` (counters + 5 sites), `25c28a1`
(monitoring.media RPC + race fix), `beee485` (allowlist + auth), `34baddb` (read-back
passes), `ac71da1` (per-object/per-peer/cache API), `5088f47` (cache producer + ratchet),
`f5a1b37` (wire assertion).

## What was built

`core/archipelago/src/media_metrics.rs` (new):

- Global `AtomicU64` counters following the existing idiom for this problem
  (`update.rs:15`, `pub static DOWNLOAD_BYTES`).
- `Counted` wraps a media chunk stream and counts bytes **inside `poll_next`**, not at
  construction. A body counted at construction is recorded as fully served even if the
  client disconnects after one chunk; counting at yield counts what was actually handed to
  the connection.
- `ActiveGuard` decrements on `Drop`, so the active-viewer gauge follows the body's real
  lifetime — a mid-stream disconnect drops the body and the gauge falls. The decrement is
  **saturating**: a plain `fetch_sub` on 0 wraps to `u64::MAX` and reads as "4 billion
  viewers".
- Generic over the chunk type, because the five handlers disagree: `rental_playback.rs`
  yields `bytes::Bytes`, the other four `Vec<u8>`. Both are `AsRef<[u8]>`, so one wrapper
  covers all five **without converting a chunk** — converting would allocate per chunk, the
  exact cost this module exists to avoid.
- Per chunk: one relaxed atomic add, ~64 KiB apart. No lock, no allocation, no store call.
  The plan's D3 warning (16,000 JSON rewrites per GB per viewer) is why.

**Wired at all five sites where media bytes actually flow:** `registered_media.rs:337`,
`rental_playback.rs:406`, `cloud_purchase.rs:140`, `prepared_media.rs:165`,
`media_stream.rs:91`. `media_stream.rs::file_response` is **not** a shared choke point — it
is called only by `proxy.rs:244`, which is why the other four needed their own wrapping.

**`MetricSnapshot.media`** added with `#[serde(default)]` so snapshots persisted before the
field existed still parse; `collector.rs` fills it.

**`monitoring.media`** RPC reads the statics **live**. `monitoring.current` returns the
latest *stored* snapshot, which lags by up to the collection interval — the wrong shape for
"did those bytes just move?".

## The mandated read-back — PASSES

    [PASS] binary starts and listens                     — 127.0.0.1:41549
    [PASS] session established (auth.setup + auth.login)
    [PASS] media body served over the real socket        — HTTP 200, 143360 bytes
    [PASS] bytes match the fixture exactly               — byte-for-byte identical
    [PASS] Range request served as 206                   — 77824 bytes
    [PASS] monitoring.media answers over the wire
    [PASS] COUNTER MOVED on the media path               — delta=143360 for a 143360-byte
                                                            serve (chunks_served 5 -> 8)
    [PASS] CONTROL: /health does NOT move the media counter
    [PASS] cache HIT recorded for a locally-served object — cache_hits +1, cache_misses +0
    [NOT_VERIFIED] per-object keys have no producer yet
    counts: PASS=11, NOT_VERIFIED=1, exit 0

The CONTROL matters as much as the delta: without it, a counter that merely measured
traffic would look identical. The cache assertion pins **both** sides (hits +1 AND misses
+0), because counting a locally-served object as a miss would leave a "working" counter
with a meaningless hit ratio.

## Provenance — what this does and does not prove

The object is a **fixture** written into the seller's `purchased-content` store. So this
proves **the counter observes the media path**, not that a purchase works. A real purchase
remains operator UAT.

## TDD

RED observed by **negative control**, not merely by a compile error: counting was disabled
on purpose and the test failed at exactly the right assertion — `bytes_served must equal
the sum of chunk lengths actually yielded`, left 0 vs right 131073. A test that only proves
a function was missing proves nothing about its assertions.

## The one dimension that is NOT implemented

**Per-object accounting has no producer.** The plan requires keys of
`(content_sha256, content_size)`. The seller's `OwnedItem` index
(`content_owned.rs:48-61`) carries onion, content_id, filename, mime, size, paid_sats,
backend, timestamp and download_complete — **no sha256**. So nothing at the serve site can
build the required key.

Keying on `content_id` instead is the easy move and the plan **explicitly forbids** it: it
fragments identical bytes across sellers and makes a hit ratio meaningless. The API is
present and unit-tested, with a targeted `#[allow(dead_code)]` and a doc comment naming the
missing input, rather than letting the lint delete a documented interface or quietly
substituting the forbidden key. The read-back reports this row as **NOT_VERIFIED**.

## Defects I introduced and fixed (each had produced a plausible wrong result)

1. **A test race.** The new RPC test's bytes leaked into the existing delta assertion
   (`147457` vs `131073`; difference exactly `4 x 4096`). Global counters + parallel tests.
   Fixed with a test-only lock, **not** `--test-threads=1`, which would have hidden it from CI.
2. **Wrong fixture directory.** Wrote to `content-owned/`; the constant is
   `purchased-content` (`content_owned.rs:42`). `open_owned()` returned `None`, the handler
   fell through to the peer path, and the read-back reported **"502 — requires FIPS"** — a
   FIPS message for a directory-name typo.
3. **Wrong cookie name.** Parsed for `csrf`; the server sets `csrf_token`. No cookie was
   ever captured, so every probe was silently unauthenticated.
4. **A self-contradictory status** in the script (`"NOT_VERIFIED" if False else "PASS"`)
   that would always have reported PASS.
5. **Broke the clippy ratchet** (124 -> 131) with seven "never used" warnings. Fixed by
   wiring the producer that genuinely exists (cache hit/miss at the local-vs-peer decision)
   and documenting the one that does not.

## Gates

| Gate | Result |
|---|---|
| `cargo check -p archipelago` | exit 0 |
| `cargo fmt --all -- --check` | clean |
| clippy, after vs baseline | **124 == 124** (both re-measured; a warm clippy cache reports 0) |
| mandated runner | **2083 passed / 1 failed** |
| pristine `fcc18b3` for comparison | 2073 passed / **1 failed** |

The single failure is `container::quadlet::tests::actual_quadlet_generator_stops_before_forced_removal`,
**identical on pristine `fcc18b3`** and environmental: the test self-skips when
`/usr/lib/systemd/system-generators/podman-system-generator` is absent, and installing
podman (needed for the `localnode`/`hardware` venues) activated it. Podman 4.9.3 emits
different text than the test expects. **Not edited, not weakened.**

## A security note worth carrying forward

`/api/peer-content/` is session-gated **on purpose** ("Same-origin, session-gated"). The
quick way to make the read-back pass was to exempt media paths from the auth check, which
would hand any unauthenticated caller the buyer's purchased bytes. The script authenticates
instead. Separately: `/var/lib/archipelago` on this host is a **bind-mount of `/tmp`**
(mode 1777, same inode). I created a `remember_secret` there before checking and **removed
it immediately**; a world-writable `/tmp` is no place for a session-signing key, and it
would be swept anyway. Recorded so the next worker does not repeat it.

## Still open, stated plainly

- Per-object keys: **no producer** (no sha256 in the owned index).
- Per-peer map: has a producer API but **no wire-level assertion**.
- A real purchase, and P3's >900 s duration, remain **operator UAT**.
