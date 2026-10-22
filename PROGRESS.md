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
