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
| 1 | Worktree created on `origin/main` (fcc18b3) | done | — |
| 2 | Baseline recorded (smoke 44 PASS / media 1 FAIL) | done | — |
| 3 | RED: failing test written for the media exemption | in progress | `core/archipelago/src/server.rs` (test module appended) |
| 4 | GREEN: implement `is_media_stream_path` + wire the watchdog | pending | `core/archipelago/src/server.rs` |
| 5 | Build binary → `localnode` venue becomes available | pending | — |
| 6 | READ-BACK on the media path (the mandatory criterion) | pending | — |
| 7 | fmt + clippy ratchet + isolated runner | pending | — |
| 8 | Commit + push to fork, verify with `ls-remote` | pending | — |

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
