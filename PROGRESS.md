# PROGRESS — P3: lift the 900 s media deadline

**Worktree:** `~/worktrees/archy-media-deadline` · **Branch:** `fix/media-stream-deadline`
**Base:** `fcc18b3` == `origin/main` (verified equal at worktree creation)
**Deliverable:** P3 — media streams must not be truncated at 15 minutes.

---

## STEP 0 — gates (all observed, 2026-10-22)

| Gate | Result |
|---|---|
| 0. toolchain | `cargo 1.99.1`, `rustc 1.99.1` (needed `export PATH="$HOME/.cargo/bin:$PATH"`) |
| 1. quota gate | `cheap-gate: ALLOW via fallback-probe`, exit 0 |
| 2. freeze marker | absent — proceeding |
| 3. ground truth | `scripts/cdn-ground-truth.sh` → **every file:line claim holds — no upstream drift** |
| disk | 9.4G free (96%). Reclaimed 582M go-build first; a build adds >1G |

## BASELINE — recorded before any change (per the prompt's requirement)

| Profile | Exit | Counts |
|---|---|---|
| `smoke` | **0** | PASS 44 |
| `media` | **1** | PASS 6, FAIL 1, NOT_VERIFIED 1 |

The `media` FAIL=1 is the `twonode` venue being unavailable (no FIPS path) → the
buyer-proxy test marked `required` reports FAILED — NOT VERIFIED **by design**.
**Not a defect. Not to be "fixed". Not to have its `required` marker removed.**

Criterion for this job is **delta, not absolute green**.

## Venue availability (doctor)

- `inprocess` — available
- `localnode` — unavailable: no built binary yet (build in progress)
- `twonode` — unavailable: `fips0` present but FIPS DNS 127.0.0.1:5354 not answering
- `hardware` — unavailable: no podman

**Consequence for P3's proof:** P3 lives in `server.rs`, and an `inprocess` run bypasses the
listener, the semaphore, the v6 mirror and the path filter. So P3 needs `localnode` or
higher. The binary build is therefore part of the proof, not optional.

---

## Milestones

| # | Milestone | Status | Files touched |
|---|---|---|---|
| 1 | Worktree created on `origin/main` (fcc18b3) | **done** | — |
| 2 | Baseline recorded (smoke 44 PASS / media 1 FAIL) | **done** | — |
| 3 | RED: failing test written for the media exemption | **done** | `server.rs` (test module appended) |
| 4 | RED-NEGATIVE-CONTROL: naive stub (`/content/` only) observed failing | **done** | `server.rs` |
| 5 | GREEN: real `is_media_stream_path` + flag wiring + watchdog exemption | **done** | `server.rs` |
| 6 | Sanctioned suite green (2078 passed, 0 failed, exit 0) | **done** | — |
| 7 | fmt clean; commit + push to fork verified by `ls-remote` | **done** | commit `d046d02` |
| 8 | Build binary → `localnode` venue available | in progress | — |
| 9 | READ-BACK on the media path (the mandatory criterion) | pending | — |
| 10 | clippy ratchet (after vs baseline 124/123) | pending | — |

## Results so far

**RED observed, twice, and the second one is the important one.**

1. Test written first → `error[E0425]: cannot find function is_media_stream_path` (14 errors).
   That only proves the function was missing, not that the assertions are meaningful.
2. So a deliberately NAIVE implementation was inserted — matching only `/content/`, which is
   the exact trap the prompt warns about — and the suite was run. Result:
   `2077 passed; 1 failed` at precisely
   `buyer playback proxy must be exempt; exempting only /content/ leaves the viewer route dead at 900 s`.
   **The test catches the half-fix.** Without this step the test would have passed for a
   reason unrelated to the bug.
3. Real predicate landed → `test result: ok. 2078 passed; 0 failed; 5 ignored`, exit 0.

**Resource incidents (both real, both recovered — the handover warned about the disk one):**

- OOM: `rustc` killed at anon-rss 3.9 GB on the first `cargo test`; swap was already 3.6/7.8 GB.
- Disk: fell 9.4G → 2.4G free (99%) during a debug build.
- Fix: use the SANCTIONED runner (`scripts/test-backend-isolated.sh`), which applies
  `profile.test.package.archipelago.opt-level=0` and `debug=0` — less disk and less memory
  than plain `cargo test`, and it is the mandated acceptance path anyway. It also runs the
  suite inside `systemd-run --private-network`, which is a stronger check than a bare run.
- Prerequisites for the systemd backend were all present: `systemd-run`, `unshare`,
  `setpriv`, and working `sudo -n`.

## The change (design, decided before coding)

`server.rs:1604-1610` already carries `Arc<AtomicBool> upgraded`, set inside `service_fn`
from the request headers; the watchdog at `:1645-1662` checks `!upgraded.load(...)`.
So the hook exists — P3 is a **second flag of the same shape**, keyed off the request URI:

```rust
if is_media_stream_path(req.uri().path()) {
    media_flag.store(true, Ordering::Relaxed);
}
```

and the watchdog becomes:

```rust
if !upgraded.load(...) && !media.load(...) && started.elapsed() >= NON_UPGRADED_DEADLINE
```

**Both prefixes required:** `/content/` (seller serve) and `/api/rental-playback/`
(buyer proxy — the route a viewer actually hits). Exempting only `/content/` leaves the
viewer route dead at 900 s.

**Safety:** `http1_keep_alive(false)` means one exchange per connection, so a media flag set
by request 1 cannot exempt a later non-media request on the same connection. A connection
that never becomes media still takes the full 900 s bound, and the 30 s
`http1_header_read_timeout` still applies to slowloris.

## Pitfalls carried in from the handover (do not re-discover)

- cargo is NOT on a non-login shell's PATH.
- workspace root is `core/`, not the repo root.
- tests go inline as `#[cfg(test)] mod tests` — the isolated runner asserts exactly ONE
  test executable, so a new `tests/*.rs` file would be unrunnable.
- Do NOT run unrestricted `cargo test` on a node with installed apps. (This box is not an
  archy node, so `cargo test` is used here for the RED/GREEN loop; the **sanctioned
  acceptance run** is `scripts/test-backend-isolated.sh`.)

---

# P1 HAND-OFF — media-path observability (recon complete, NOT started)

**Status: recon done, no code written.** Recorded here so the next worker skips the
discovery I already paid for. Do not re-derive it.

## What P1 must add

Per-object bytes served · active-viewer count · per-peer accounting · cache hit/miss.
Today `MetricSnapshot` (`monitoring/types.rs:14-35`) is host + container only.

## The counter idiom ALREADY EXISTS — copy it, do not invent one

    core/archipelago/src/update.rs:15
    pub static DOWNLOAD_BYTES: AtomicU64 = AtomicU64::new(0);
    pub static DOWNLOAD_TOTAL: AtomicU64 = AtomicU64::new(0);

That is the established pattern in this codebase for exactly this problem (progress
counting for a byte stream). Also present: `OnceLock<Mutex<..>>` in
`api/handler/routstr_proxy.rs:88` and `api/rpc/lnd/macaroons.rs:169`.

## The 5 sites where media bytes actually flow (verified, file:line)

| Site | Stream built at | `wrap_stream` at | Notes |
|---|---|---|---|
| `api/handler/registered_media.rs` | `:281` (`rental_response`) | `:337` | seller-side registered rental |
| `api/handler/rental_playback.rs` | `:320` | `:406` | **buyer proxy — the route a viewer hits** |
| `api/handler/cloud_purchase.rs` | `:109` | `:140` | purchase-gated serve |
| `prepared_media.rs` | `:133` | `:165` | prepared/transcoded media |
| `media_stream.rs` | `:61` | `:91` | shared `file_response`; **only** called by `api/handler/proxy.rs:244` |

**`media_stream.rs` is NOT the single choke point** — it is used only by `proxy.rs`. The
other four build their own `try_unfold` streams. So P1 needs the counter at 5 sites, or a
shared wrapper introduced at those 5 sites. Do not assume one insertion point; I checked.

## Design that fits the existing shape

1. New counters as `pub static ... AtomicU64` (per the `update.rs` idiom), in a new
   `media_metrics.rs` or under `monitoring/`.
2. Wrap each of the 5 chunk streams so every emitted chunk adds `read` to a byte counter —
   keep the per-chunk work to a single relaxed atomic add. **Never** do a JSON write or a
   lock per chunk; the D3 section of the prompt already warns that
   `meter::record_and_check` per chunk is ~16,000 JSON rewrites per GB per viewer.
3. Add fields to `MetricSnapshot` and fill them in `monitoring/collector.rs:37`
   (`collect_snapshot`), which is where `rpc_latency_ms`/`ws_connections` are already
   "filled in by MetricsStore::push" — follow that comment's precedent.
4. Active viewers / cache hit-miss need a decision: a gauge (inc on stream start, dec on
   `Drop`) is the honest shape. Copy the `Reservation`+`Drop` idiom noted in the handover
   for D4b rather than inventing a second lifetime mechanism.

## The read-back P1 must satisfy (from the coverage contract)

Serve one object over `/content/...`, read the counter back, **show the delta**. A counter
that only increments in a unit test proves nothing.

**This is blocked on the same thing P3's wire proof is: B3.** The binary hardcodes
`/var/lib/archipelago` and dies with `Failed to write PID marker / Permission denied`, so
no node can be started to serve the object. **B3 gates the wire-level proof of P1, P2, P3,
P4, D3 and D5.** It is the highest-value unblock on the board, and it is a one-line
question for the operator: is a writable data dir acceptable, or should the binary take a
config/env override?

## Suggested test names (from the prompt's list, D1/D3 overlap)

- P1's own: a counter delta observed over the media path.
- The prompt names `per_subscriber_rental_window_is_still_enforced_mid_stream` for D1 and
  `range_requests_cannot_reset_the_cap` for D3 — both touch the same code, so land P1
  first and reuse its seam.

## Estimated effort

The prompt says 1-2 d. My recon agrees: 5 insertion sites, one new module, one snapshot
field group. The real cost is B3, not the counter.

---

# P1 IMPLEMENTED — and one environmental consequence I caused

## P1 status: code complete, counters wired at all 5 sites, pushed (`f7a397a`)

`core/archipelago/src/media_metrics.rs` (new) + wiring at
`registered_media.rs:337`, `rental_playback.rs:406`, `cloud_purchase.rs:140`,
`prepared_media.rs:165`, `media_stream.rs:91`. `MetricSnapshot.media` added with
`#[serde(default)]`; `collector.rs` fills it.

**RED observed by negative control**, not by a compile error: counting was disabled on
purpose and the test failed at exactly the right assertion —
`bytes_served must equal the sum of chunk lengths actually yielded`, left 0 vs right
131073. A test that only proves a function was missing proves nothing about its assertions.

## THE MANDATED RUNNER NOW HAS 1 FAILURE ON THIS HOST — and it is not this change

| SUT | passed | failed |
|---|---|---|
| pristine `fcc18b3` | 2073 | **1** |
| branch `f7a397a` | 2079 | **1** |

Same single failure on both: `container::quadlet::tests::actual_quadlet_generator_stops_before_forced_removal`.

**Root cause, and it is a consequence of my own podman install.** That test opens with a
self-skip guard:

    let generator = Path::new("/usr/lib/systemd/system-generators/podman-system-generator");
    if !generator.exists() { eprintln!("… run this regression on the Linux release host"); return; }

On a host without podman it **silently passes**. Installing podman (which I did to unblock
the `localnode`/`hardware` venues) put that path on disk, so the test stopped skipping and
started asserting against the real generator — and podman **4.9.3** emits different text
than the test expects:

    observed (4.9.3):  ExecStop=/usr/bin/podman rm -v -f -i --cidfile=%t/%N.cid
                       TimeoutStopSec=615
                       (no "podman stop --ignore --time=600", no "--stop-timeout 600")
    expected by test:  ExecStop=/usr/bin/podman stop --ignore --time=600
                       before ExecStop=/usr/bin/podman rm …
                       --stop-timeout 600, TimeoutStopSec=615

The test is version-sensitive and was written against a newer podman — presumably the one
on the project's actual release host. Ubuntu 24.04 ships 4.9.3.

**What I did NOT do:** I did not edit the test, delete it, or weaken its assertions. The
prompt forbids refactoring unrelated source, the test may well be correct for the podman
the project targets, and "make the suite green by editing the failing test" is exactly the
move the prompt exists to prevent. It is reported instead.

**Trade-off, stated plainly:** podman is required for the `localnode` and `hardware`
venues (and therefore for every wire-level read-back in this job). Installing it activated
a dormant, version-sensitive test. Both facts are true; neither cancels the other.

## Delta, honestly

+6 tests on this branch versus pristine (4 from P3 + 2 from P1), **all passing**. The
single failure is identical on both trees and is environmental, not behavioural.

---

# P2 HAND-OFF — admission control (recon + design, NOT implemented)

**Status: recon done, no code written.** Banked so the next worker skips the discovery.

## The problem, verified against the tree

`server.rs:1136` creates **ONE** `Semaphore::new(1024)` in `serve_with_shutdown` and clones it
into every listener (`:1142`, `:1168`, `:1221`). So the main UI listener and the peer
listener **share one budget**. `accept_loop` (`:1563`) takes a permit per connection with
`try_acquire_owned()` (`:1601`) and, on exhaustion, writes a bare 503 and closes
(`:1602-1619`).

Consequence: N concurrent media streams consume the same 1024 permits the local UI needs.
At exhaustion **the UI 503s too** — the node's own management surface becomes unreachable
because a video is streaming. That is the failure P2 exists to remove.

`spawn_blocking(open_matching)` is unbounded (`cloud_purchase.rs:97`).

## Why a naive wrapper would be a REGRESSION (read this before coding)

The tempting implementation is to wrap each media body so it holds a permit for its
lifetime. **Do not do that.** `DRAIN_TIMEOUT`/graceful shutdown waits for permits to return
(`:1232` `while active_connections.available_permits() < 1024`). A permit held for the whole
duration of a 2-hour film would make shutdown wait 2 hours, turning a clean restart into a
hang. That is worse than the bug.

Also note the existing comment at `:1594-1600`: `acquire_owned().await` in the accept loop
was **deliberately removed** because it froze `accept()` entirely when permits drained
(the `.228` session-flapping / CLOSE-WAIT signature). Any P2 design must not reintroduce a
blocking acquire in the accept path.

## The design that fits

1. **Separate the budgets.** Media must not be able to starve the UI, and the UI must not be
   able to starve media. Give the peer/media listener its **own** semaphore rather than
   sharing the UI's. This alone fixes the headline failure and is the smallest correct change.
2. **Bound the unbounded.** Cap concurrent media serves with a dedicated, smaller semaphore
   (the plan's "admission control"), and cap `spawn_blocking(open_matching)` fan-out. On
   exhaustion, media admission must **fail cleanly** (a bounded, honest error) rather than
   evicting or blocking.
3. **Do NOT hold a connection permit for a stream's lifetime.** If a long-lived media permit
   is needed at all, it must be released at `DRAIN_TIMEOUT`, not at stream end — otherwise
   graceful shutdown hangs. Prefer bounding *concurrent media serves* via a separate counter
   that shutdown does not wait on.
4. **Per-peer rate cap** on the media path: keyed per peer, bounded map (copy the bounded-map
   idiom from `media_metrics::detail`, `MAX_PEERS`), and it must not be bypassable by
   opening more connections.

## Regression guard the plan names

`quota_is_enforced_and_admission_fails_cleanly_rather_than_evicting_a_settled_purchase`
— i.e. when admission is full, the answer is a clean bounded failure, **never** eviction of a
settled purchase (`content_purchase.rs:133-135`, `content_snapshot.rs:261` are the liability
this protects).

## The read-back P2 must satisfy

N concurrent media streams, then **the node's own UI still answers**. Before P2 the shared
semaphore 503s everything including the UI. Venue: `localnode` (available — podman installed).
The read-back harness pattern already exists: `scripts/p1-media-counter-readback.py` drives
the real release binary with `ARCHIPELAGO_DATA_DIR`, a fixture in `purchased-content/`, and an
authenticated session (`auth.setup` + `auth.login`). **Reuse that script's shape** rather than
writing a second one — the auth and fixture setup cost real time to get right, and the two
gotchas are documented in its comments (cookie is `csrf_token`, not `csrf`; fixture dir is
`purchased-content`, not `content-owned`).

## P2 IMPLEMENTED + VERIFIED (2026-10-23)

`server.rs`: `UI_CONNECTION_BUDGET` / `PEER_CONNECTION_BUDGET` as named constants; the peer
listener gets its own semaphore; the drain references the UI budget by name. NOT a smaller
peer budget — shrinking it would swap a UI-starvation bug for a peer-starvation bug.

The trap avoided: wrapping a media body to hold a permit for its lifetime is WORSE than the
bug — graceful shutdown waits for permits to return (`:1232`), so a 2-hour film makes a
restart wait 2 hours.

Evidence — live read-back (`scripts/p2-admission-readback.py`), real binary:

    [PASS] BASELINE UI answers, no media load — /health 200, /rpc/v1 200
    [PASS] 48 media streams established — 48/48 HTTP 200 held open
    [PASS] UI STILL ANSWERS under concurrent media load — /health 200, /rpc/v1 200
    [PASS] CONTROL: media still served (UI budget did not starve it)
    [NOT_VERIFIED] scale caveat — 48 streams, not 1024
    counts: PASS=6, NOT_VERIFIED=1, exit 0

Source-level guard, negative-control proven: re-merging the budgets fails it at
`peer_late_bind_loop must receive peer_connections`.

Two of my own defects: the guard first failed against ITSELF (`include_str!` includes the
test module, so the needle appeared in the assertion searching for it — needle now built at
runtime); and the drain used a bare `1024`, now the named constant.

Gates: check 0 · fmt clean · clippy 124 == 124 · runner 2085 passed / 1 failed (the
environmental quadlet test, identical on pristine fcc18b3).

STILL NOT DONE for P2: the **per-peer rate cap** (the plan's second half), and
`spawn_blocking(open_matching)` remains unbounded (`cloud_purchase.rs:97`). The guard is
source-level, not behavioural — a unit test cannot observe two semaphores.

## P4 IMPLEMENTED + VERIFIED (2026-10-23)

`media_stream.rs` — the shared local-cache serve (reached via `/api/peer-content/` →
`proxy.rs:244`). Before this there was **no ETag anywhere in the tree**.

- `etag_for(metadata)` — STRONG validator (no `W/`), from size + mtime.
- `range_is_honoured(headers, etag)` — RFC 7233 §3.2: a mismatched `If-Range` makes the
  server IGNORE `Range` and send the whole body (200).
- `If-None-Match` → 304; `ETag` on the 200/206/304 paths.

The property that matters: a **stale `If-Range` must disable the range**, or new bytes get
spliced onto an old prefix and the client receives a corrupt file that looks like a
successful resume.

Two deliberate design points, documented in the code:
1. **Not a content hash.** The plan's `(content_sha256, content_size)` is a CACHE KEY for
   D1/D5. An HTTP validator answers a different question; size+mtime is the standard answer
   (nginx, Apache). Content-deriving it means hashing every byte before the first chunk — the
   cost resume exists to avoid. Known limit stated: identical size AND mtime would pass.
2. **Strong, not weak** — RFC 7232 §2.3.2 forbids a weak validator with `If-Range`.

Evidence — wire read-back on the real binary:

    [PASS] first response carries an ETag    — ETag='"143360-1792736964166697662"'
    [PASS] matching If-Range resumes 206     — Content-Range=bytes 65536-143359/143360
    [PASS] STALE If-Range returns whole body — HTTP 200, 143360 bytes
    [PASS] If-None-Match returns 304         — HTTP 304
    counts: PASS=15, NOT_VERIFIED=1, exit 0

Negative control: making `range_is_honoured` ignore `If-Range` (the pre-P4 behaviour) fails
the test at the stale-validator assertion.

Gates: check 0 · fmt clean · clippy 124 == 124 · runner 2087 passed / 1 failed (the
environmental quadlet test).

**NOT covered:** the peer/seller handlers build their own responses and still emit no ETag,
so resume works on the local-cache route but **not yet** on `registered_media.rs`,
`rental_playback.rs`, `cloud_purchase.rs` or `prepared_media.rs`.

**A recurring failure mode worth naming** — three times now a slug/case mismatch produced a
confident wrong diagnosis: cookie `csrf` vs `csrf_token`; fixture dir `content-owned` vs
`purchased-content`; and now case-sensitive header lookups against hyper's lowercase names
(the 206 row had been printing `Content-Range=None` since P1). A lookup that fails silently
is indistinguishable from absence. Probes must fail loudly.

## D3 IMPLEMENTED (2026-10-23) — accounting half; cap enforcement NOT yet wired

**Why coarse:** `meter::record_and_check` (`streaming/meter.rs:26`) calls
`session::load_sessions` and **saves the whole session store** on every call. Per-chunk
metering of a 5 Mbps stream is therefore ~16,000 store writes per GB per viewer. Bytes
accumulate in memory (`media_meter.rs`) and flush at end-of-stream or on a coarse window
(`DEFAULT_FLUSH_BYTES` = 8 MiB, ~13 s at 5 Mbps).

**New `media_meter.rs`:**
- `MediaUsageAccumulator` keyed by (peer, service); `add` **saturates** (a plain `+` at
  u64::MAX wraps small → effectively unlimited allowance — same class as the `fetch_sub`
  wrap already fixed on the active-stream gauge).
- `add_and_should_flush` / `take` / `drain_all`. `take` drains what it charged, or the next
  flush double-charges and every viewer's allowance halves.
- `flush_usage()` — the only place bytes become billable; per-pair errors are logged and
  skipped so one corrupt session cannot stop everyone else being billed.

**Wired into the body:** `media_metrics::counted_for(stream, peer, service)` accumulates in
`poll_next`, so the count is the bytes **actually handed out** (a body counted at
construction reads as fully served even on client disconnect). `counted()` keeps
`usage: None` for callers with no billable peer.

**Tests (6 new), 2093 passed / 1 failed** (environmental quadlet), fmt clean:

| Test | Proves |
|---|---|
| `range_requests_cannot_reset_the_cap` | named guard: a resume ADDS, never resets |
| `usage_is_accounted_per_peer_and_service` | two viewers don't share a counter |
| `flush_threshold_fires_on_accumulation_and_resets_only_that_pair` | flush at window, pair-local reset |
| `enormous_usage_saturates_instead_of_wrapping` | no u64 wrap → no free unlimited stream |
| `drain_all_empties_and_totals` | no double-count |
| `counted_for_accumulates_body_bytes_against_the_peer` | body drives it; CONTROL: `counted` charges nobody |

**Two defects of my own:** `Counted` is a `Stream`, not an `HttpBody`, so `data()` does not
exist on it (drive with `StreamExt::next`); `stream::iter` needs an explicit error type
(E0282).

**NOT done, stated plainly — the remaining D3 work:**
1. `flush_usage` **is now called** (60 s sweep in `main.rs`) — accumulated bytes are persisted.
2. The buyer proxy now charges **real per-viewer bytes** (`counted_for` with the rental
   contract id), so the cap has a subject.
3. **Still missing: nothing REFUSES a stream on exhaustion.** `check_gate` is not consulted
   on the media path, so bytes are charged and flushed but the cap does not yet stop anyone.
4. Therefore the wire read-back (`serve over /content/..., show the counter delta`) is
   **NOT_VERIFIED** — it needs the localnode venue. `check_gate`'s three callers remain off
   the video path.

So: bytes are accounted, attributed to a real buyer, and flushed durably — but D3 is **not
done** until exhaustion refuses a stream.

### The gate check that lied (keep this)

`cargo clippy ... | grep -c '^warning:'` returned **0** twice while the crate **did not
compile**. A failed build emits no `warning:` lines, so "0 warnings" read as a perfect pass
against a 124 baseline. The check now greps `^error` and `Finished` FIRST, then counts.
Same silent-failure class as the header-case bug: a lookup that fails indistinguishably from
success.

## D3 COMPLETE (in-process) — accounting + attribution + flush + REFUSAL

Branch tip `036d28a`. All four halves now exist:

| Half | Where | Evidence |
|---|---|---|
| accounting | `media_meter::MediaUsageAccumulator` | 6 tests |
| attribution | `counted_for` charges the rental contract id | body-level test + CONTROL |
| flush | 60 s sweep in `main.rs` | `flush_usage` wired |
| **refusal** | `counted_within_allowance` ends the body at `Ok(None)` | `stream_ends_cleanly_when_the_allowance_is_spent` |

Refusal specifics: `check_within_allowance` counts **unflushed** bytes against the durable
remaining (charging late is fine, *admitting* late is not); `remaining` is an
`Arc<AtomicU64>` so the allowance is re-read per poll rather than frozen at construction;
the check runs BEFORE polling so no byte past the allowance escapes; breach returns
`Poll::Ready(None)` — a clean end of body, never an error, per the plan.

Safety branch: `NoSession`/`NotMetered` ⇒ **no cap**. D3 must not refuse sessions that were
never metered — inventing refusals truncates paying viewers, worse than under-charging.

**Still NOT_VERIFIED:** the wire read-back over `/content/...` needs the localnode venue.
Proven at the body level, not on the wire.

## D4a DECISION — (a) Report it [operator, 2026-10-23]

Operator chose **(a) Report it**: D4a warns loudly when a declared `disk_limit` cannot be
enforced, and enforces where the filesystem allows. Already built; no code change.

Superseded options, for the record: (b) refuse at deploy, (c) move enforcement to the
node-level media budget (`snapshot_budget.rs`). (c) remains a live option if per-app disk
governance is ever wanted on ext4 — it works on every filesystem, unlike `--storage-opt`.

## D4a IMPLEMENTED (2026-10-23) — enforced on XFS, REPORTED on ext4

`disk_limit` was declared and read nowhere; now wired, but **not** as a copy of the memory
path, because per-container size limits need overlay-on-XFS with project quotas and this
platform is ext4 (see the recon section below for the measured error).

- `disk_limit_support(backing_fs)` — `Enforceable` on xfs, `NotEnforceable` elsewhere with a
  reason naming the real filesystem and the fix.
- `disk_limit_storage_opt(limit, backing_fs)` — the storage_opt when honourable, a reason
  when not. Reuses `parse_memory_limit` rather than a second parser that could drift.
- `storage_backing_fs()` — statfs on the container storage root; detection failure returns
  empty, which **fails safe** to NotEnforceable. Claiming enforceability we cannot back up is
  the silent failure; an extra warning costs nothing.
- `create_container` passes `storage_opt` when possible and `tracing::warn!`s when not — so a
  limit the host cannot enforce is **visible** instead of silent.

Tests (3 new): refused-with-reason on non-XFS and wired on XFS; unparseable warns rather than
sending `size: 0` (which podman rejects at start); unknown backing FS fails safe.

Observed: **86 passed / 0 failed** in `archipelago-container`, doctests 0 failed. Note the
field lives in the **`archipelago-container`** crate, not `archipelago` — filtering on the
wrong crate reports "0 tests" and looks like a pass.

Also fixed a doctest I broke: the `$ podman run` example was indented 4 spaces, so rustdoc
compiled it as Rust and failed with `expected item, found $`. Marked ```text```.

Gates: fmt clean · clippy 124 == 124 · runner **2087 passed / 1 failed** (environmental
quadlet, identical on pristine).

**Not claimed:** disk_limit is *effective* only on XFS hosts. On ext4 — including this one —
it is now *reported*, not enforced. Whether the field should instead be expressed through the
working node-level media budget (`snapshot_budget.rs`) is an operator product decision.

## D4a RECON — `disk_limit` is declared but unread, AND the obvious wiring does not work

**Status: recon complete; implementation now landed — see the section above.**

### Correction 1: the line number was stale, the field is real

The prompt cites `manifest.rs:481-488`. The field is actually at
**`container/src/manifest.rs:488`** inside `ResourceLimits`:

    pub struct ResourceLimits {
        pub cpu_limit: Option<u32>,       // :484 — wired
        pub memory_limit: Option<String>, // :486 — wired
        pub disk_limit: Option<String>,   // :488 — DECLARED BUT READ NOWHERE
    }

Verified by grepping the whole tree outside `target/`: `disk_limit` appears on **exactly one
line** — its own declaration. So D4a's target is real and my initial "it does not exist"
was my own error (I grepped from the wrong directory first; the wrong diagnosis, not a
missing field).

### Correction 2: the wiring the prompt implies CANNOT WORK on this platform

The natural implementation is `--storage-opt size=<bytes>`, the way `memory_limit` maps to
`memory.limit`. Measured, not assumed:

    $ podman run --rm --storage-opt size=100M docker.io/library/alpine:latest true
    Error: storage option overlay.size and overlay.inodes only supported for backingFS XFS.
    Found extfs

    $ df -T ~/.local/share/containers/storage
    /dev/mapper/ubuntu--vg-ubuntu--lv ext4

The overlay driver supports per-container size limits **only on XFS with project quotas**.
This host is ext4 — and so is a stock **Debian 13** install, which is the platform the
project targets. So a naive `--storage-opt size=` wiring would either error at container
start or be silently ignored, depending on the call path. Writing it and reporting "D4a
done" would be the exact silent-failure class this job exists to remove.

`storage_opt` also appears **nowhere** in `container/src/` today, so there is no existing
plumbing to copy.

### What D4a can honestly be

Three options, in order of how much they actually enforce:

1. **Reject-or-warn at manifest validation** — if `disk_limit` is set and the storage
   backing filesystem cannot enforce it, say so loudly (at deploy time, not at container
   start). This turns a silently-ignored limit into a visible one. Small, honest, and the
   only option that is correct on every filesystem.
2. **Wire it where it CAN be enforced** — pass `--storage-opt size=` only when the backing
   FS is XFS with prjquota, detected at runtime, and report the skip otherwise. Enforces
   where possible without lying where it is not.
3. **Enforce at the node level instead** — the media budget in `snapshot_budget.rs:15-16`
   (64 GiB / 512 MiB floor, hard-fail `:153-159`) is the project's existing disk-governance
   mechanism, and it *does* work. A per-app limit that cannot be enforced may be better
   expressed there than as a manifest field that does nothing.

**Recommendation: (1) + (2) together** — validate loudly, enforce where the filesystem
allows. Option (3) is a product decision (does the manifest field mean anything if it can
only be honoured on XFS?) and belongs to the operator, not to this loop.

### Why I stopped here rather than picking one

The plan lists D4a as "1-2 d, imitate `parse_memory_limit`". The imitation is 20 lines; the
*decision* is which of the three above is the intended semantics, and that changes what
"done" means. That is a DECIDE-vs-ESCALATE item with a product answer, so it is surfaced
rather than guessed — and the recon is banked so the answer costs one message, not a day.

## Environment notes that will save time

- Release build: **`CARGO_BUILD_JOBS=1`**. `jobs=2` OOM-killed rustc twice at ~2.2-3.9 GB.
  ~12-13 min per release build.
- Test builds: `RUSTFLAGS="-C debuginfo=0" CARGO_PROFILE_TEST_DEBUG=0
  CARGO_PROFILE_TEST_OPT_LEVEL=0 CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0` — plain
  `cargo test` OOMs.
- `/var/lib/archipelago` on this host is a **bind-mount of `/tmp`** (mode 1777). Never put a
  session secret there.
- `/api/peer-content/` is session-gated **on purpose** — do not exempt media paths from auth
  to make a test pass; that hands purchased bytes to unauthenticated callers.

---

# P2 IMPLEMENTED + VERIFIED (2026-10-23)

## What landed

`server.rs`: `UI_CONNECTION_BUDGET` and `PEER_CONNECTION_BUDGET` as named constants; the peer
listener receives its own `peer_connections` semaphore; the drain condition references the UI
budget **by name** (a bare `1024` there could drift).

Deliberately NOT a smaller peer budget — shrinking it would convert a UI-starvation bug into a
peer-starvation bug. The fix is separation, not a smaller number.

## The trap this design avoids

The obvious implementation — wrap each media body so it holds a permit for its lifetime —
is **worse than the bug**. Graceful shutdown waits for permits to return (`server.rs:1232`),
so a permit held for a 2-hour film makes a clean restart wait 2 hours. Keeping peer permits
out of the drain path is what makes that deadlock unreachable. The file also records at
`:1594-1600` that a blocking acquire in the accept path was removed after it froze `accept()`
entirely (the `.228` CLOSE-WAIT signature); this change does not reintroduce it.

## Evidence

Live read-back on the real release binary (`scripts/p2-admission-readback.py`):

    [PASS] BASELINE: UI answers with no media load — /health 200, /rpc/v1 200
    [PASS] 48 media streams established — 48/48 HTTP 200 while held open
    [PASS] UI STILL ANSWERS under concurrent media load — /health 200, /rpc/v1 200
    [PASS] CONTROL: media is still served (UI budget did not starve it)
    [NOT_VERIFIED] scale caveat — 48 streams, not 1024
    counts: PASS=6, NOT_VERIFIED=1, exit 0

Source-level guard in `server.rs`, **negative-control proven**: re-merging the budgets on
purpose — restoring the original bug — fails it at exactly
`peer_late_bind_loop must receive peer_connections`.

Two of my own defects found getting here:
  * The guard first failed against **itself**: `include_str!("server.rs")` includes the test
    module, so the literal needle appeared in the assertion searching for it. Needle is now
    assembled at runtime.
  * The drain used a bare `1024`; now the named constant, with an assertion it does not return.

Gates: check 0 · fmt clean · clippy **124 == 124** · mandated runner **2085 passed / 1 failed**
(the environmental quadlet test, identical on pristine `fcc18b3`).

## Still NOT done for P2

- **`spawn_blocking(open_matching)` remains unbounded** (`cloud_purchase.rs:97`).
- **The guard is source-level, not behavioural**: a unit test cannot observe two semaphores.
  The live read-back covers the observable claim; the guard covers the structural one. Neither
  alone is sufficient.

### Per-peer cap — IMPLEMENTED (2026-10-26), retiring the first item above

The budget separation left one hole open: `PEER_CONNECTION_BUDGET` is a *shared* 1024, so a
single peer could take every permit and starve all other peers. A per-listener number cannot
express that — the limit has to be keyed by the peer's address.

`PeerConnectionSlots` (server.rs) now does exactly that: a mutex-guarded `HashMap<IpAddr, usize>`
with `PEER_PER_PEER_CONNECTION_CAP = 64` in-flight connections per source address, consulted on
the accept path **before** the listener budget (a peer over its cap must not first consume a
shared permit it will only hand back). Over-cap peers get **429 + Retry-After**, deliberately
distinct from the **503** that means budget exhaustion, so the two failure modes are
distinguishable in a log and to a client. The slot is moved into the connection task and released
by `Drop`, so it measures in-flight connections rather than accepts and every exit path returns
it. Entries are removed at zero, so the map cannot grow with uptime.

Applied to the **peer listener only** (`peer_only`): the UI listener is a local management
surface, and keying a cap there would cap loopback — the node's own dashboard.

TDD: three behavioural tests on the real limiter plus one wiring guard. Negative control
observed — changing `>=` to `>` in the cap comparison fails
`one_peer_cannot_take_every_slot` at the third-acquire assertion (`2 passed; 1 failed`),
so the test bites rather than merely passing.

Gates at this commit: fmt rc 0 · clippy **0 errors, 122 warnings** (124 ratchet) · mandated
runner **2106 passed / 1 failed / 5 ignored** (the same environmental quadlet test; the four new
tests are among the passes).

**Not claimed:** the cap's live behaviour on a mesh listener is **not** wire-verified — that
needs `localnode`/`twonode`, which this host does not have (no `fips0`, FIPS DNS `5354` silent).
The limiter is proven behaviourally and the wiring is proven on the source; the third leg, an
actual 65th connection from one peer being refused in front of a real listener, is not done.

### WIRE VERIFICATION — done (2026-10-26), superseding the paragraph above

**The paragraph above was wrong about the venue.** `getent hosts fips0` returned nothing and I
read that as "no fips0", but the interface exists (`fips0`, ULA
`fd36:3c0f:d6f:8029:ea50:958b:672e:bb0`, read from `/proc/net/if_inet6`). It only means the
name does not resolve. The peer listener therefore IS wire-testable on this host, and a DNS
lookup is not a venue probe — read `/proc/net/if_inet6` or `ip -6 addr`, the way the binary
itself does.

`scripts/p2b-per-peer-cap-readback.py` drives the real release binary's peer listener on
`[ULA]:5679` and counts what the kernel hands back:

    [PASS] cap is readable from source                 — PEER_PER_PEER_CONNECTION_CAP=64
    [PASS] fips0 carries a ULA                         — fd36:3c0f:…:bb0
    [PASS] binary starts and serves both listeners     — UI healthy, peer accepting
    [PASS] peer admits 64 concurrent connections       — 64/64 held open
    [PASS] the connection past the cap is refused      — connection 65 → HTTP 429
    [PASS] refusal is 429, not the 503 for budget exhaustion
    [PASS] UI listener still answers while the peer cap is saturated — /health → 200 at 64
    [PASS] closing a peer connection returns its slot  — next connection → 200
    [PASS] below the cap the next connection is admitted — with 3 held, #4 → 200
    [NOT_VERIFIED] two DIFFERENT peers are isolated    — all connections share one source key
    counts: PASS=9, NOT_VERIFIED=1, exit 0

Binary under test: `4e2f961ac3954966dc0243c02ea276e1b46b17c03b9983c73d10f392d7841466`
(80,067,328 B, built from `ab2c4402`, `Finished release` in 15m 32s, `CARGO_RC=0`).

The last two rows are what make this a control rather than a coincidence: the shallow probe
proves the refusal is **count-keyed** (a latch or machine-level limit would also refuse at
depth 3), and the release probe proves the slot is **returned** rather than leaked.

**Still not claimed:** isolation between two DIFFERENT peers. Every connection originates from
this host, so they share one peer key; that half rests on the address-keyed accounting proven by
`one_peer_cannot_take_every_slot`, not on this wire run. A second machine on the mesh is the only
way to close it.




# VERIFICATION AT HEAD `9d50f739` (2026-10-26) — fresh build, all three read-backs re-run

Prior read-back numbers were produced against separate builds, and the only release binary left
on this host (`~/worktrees/archy-media-deadline/core/target/release/archipelago`, mtime
2026-10-23 08:28) predates `036d28ac` (D3 enforcement, Oct 23 16:34) and `9d50f739` (D1, Oct 24
04:23). So the numbers below were produced from a **fresh build of this head**, and the scripts'
`ARCHY_SUT` was pinned to `/home/c08r4d0r/repos/archy` rather than their default worktree.

Binary: `core/target/release/archipelago`, 80,504,928 B,
`sha256 70b9f8fa608a39bd72b3d1b3541ef027e6509329c6216a0f9a9430954f1afacd`.
Build: `CARGO_BUILD_JOBS=1 cargo build --release` → `Finished \`release\` profile` in **28m 02s**,
`CARGO_RC=0`. (A first attempt was run with its output piped through `tail`, so the observed
`rc=0` was `tail`'s and the run ended with no binary at all — the pipe hid the real status.
`PIPESTATUS` / a log file, never `| tail`.)

| Read-back | Result | Exit |
|---|---|---|
| `p1-media-counter-readback.py` | **PASS=15, NOT_VERIFIED=1, FAIL=0** | 0 |
| `p2-admission-readback.py` | **PASS=6, NOT_VERIFIED=1, FAIL=0** | 0 |
| `p3-media-deadline-readback.py` | **PASS=4, NOT_VERIFIED=1, FAIL=0** | 0 |

The three NOT_VERIFIED rows are the known fixture limits, unchanged and not upgraded:

- **P1 per-object keys** — the seller's `OwnedItem` index carries no sha256, so nothing at the
  serve site can build the plan's `(sha256, size)` key. API present, unit-tested, no producer.
- **P2 scale caveat** — 48 streams, not 1024. Representative in kind (shared-budget bug), not a
  load test.
- **P3 >900 s duration** — requires >900 s of wall clock per case and a real purchased stream
  (needs the `twonode` venue; FIPS DNS `5354` is not answering on this host).

Other gates at this head:

- `cargo fmt --all -- --check` → **rc 0**
- `cargo check -p archipelago` → **rc 0** (49 warnings, all pre-existing dead-code)
- `cargo clippy --all-targets --all-features` → **rc 0, 0 errors, 122 warnings** for the bin —
  below the 124 baseline, so the ratchet holds
- mandated runner `scripts/test-backend-isolated.sh` → **2102 passed, 1 failed, 5 ignored**.
  The single failure is
  `container::quadlet::tests::actual_quadlet_generator_stops_before_forced_removal` — podman's
  `quadlet-generator` rejects `StopTimeout` in group `Container` on this host; the same test
  fails identically on pristine `fcc18b3`, so it is environmental, not introduced here.

Nothing in this section changes the "STILL NOT DONE" list that precedes it: the per-peer rate
cap is not implemented, `spawn_blocking(open_matching)` is still unbounded, and a real purchase
remains operator UAT.
