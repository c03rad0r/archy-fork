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
