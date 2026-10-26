# CDN work — verified findings and the limits of each claim

This file records what was actually measured for the media-delivery work (P1–P4, D1, D3, D4a,
and the two halves of P2), what each measurement does NOT cover, and the traps that produced
wrong answers along the way. It is written for the next person who has to trust or extend this
work: every claim below is traceable to a command, and every gap is named instead of papered
over.

Nothing here is a self-report. Where a number appears, the tool that produced it is named.

---

## 1. Verification at head `9d50f739` (2026-10-26)

Prior read-back numbers were produced against *different builds*. The only release binary then
on this host (`~/worktrees/archy-media-deadline/core/target/release/archipelago`, mtime
2026-10-23 08:28) **predates** `036d28ac` (D3 cap enforcement, Oct 23 16:34) and `9d50f739`
(D1 snapshot index, Oct 24 04:23). So every earlier read-back was re-run against a fresh build
of the head under test, with the scripts' `ARCHY_SUT` pinned to `/home/c08r4d0r/repos/archy`.

- Build: `CARGO_BUILD_JOBS=1 cargo build --release` → `Finished release` in **28m 02s**,
  `CARGO_RC=0`. Binary 80,504,928 B,
  `sha256 70b9f8fa608a39bd72b3d1b3541ef027e6509329c6216a0f9a9430954f1afacd`.
- `scripts/p1-media-counter-readback.py` → **PASS=15, NOT_VERIFIED=1, FAIL=0**, exit 0.
  The counter observed `bytes_served 221184 → 364544` for a 143360-byte serve on a real socket,
  `/health` left it unchanged, cache HIT recorded, ETag/`If-Range`/304 all pass.
- `scripts/p2-admission-readback.py` → **PASS=6, NOT_VERIFIED=1, FAIL=0**, exit 0.
  48/48 media streams held, UI still 200 on `/health` and `/rpc/v1`, media still served.
- `scripts/p3-media-deadline-readback.py` → **PASS=4, NOT_VERIFIED=1, FAIL=0**, exit 0.
- `cargo fmt --all -- --check` → rc 0. `cargo check -p archipelago` → rc 0.
- `cargo clippy --all-targets --all-features` → rc 0, 0 errors, **122** warnings for the bin
  against a **124** ratchet.
- `scripts/test-backend-isolated.sh` → **2102 passed / 1 failed / 5 ignored**.

The single failure is
`container::quadlet::tests::actual_quadlet_generator_stops_before_forced_removal`:
`quadlet-generator` rejects `unsupported key 'StopTimeout' in group 'Container'`. It fails
identically on pristine `fcc18b3`, so it is environmental (this host's podman), not introduced
by this work. **Do not "fix" it by weakening the assertion.**

### The trap that invalidated the first attempt

A build was started as `cargo build --release 2>&1 | tail -40`, and the reported `rc=0` was
`tail`'s exit status, not cargo's. The run produced **no binary at all**. Any gate whose status
is captured through a pipe is not measured. Use a log file, `${PIPESTATUS[0]}`, or
`scripts/ci/run_gate.sh`.

---

## 2. P2, second half — per-peer connection cap (commit `ab2c4402`)

**The hole.** `PEER_CONNECTION_BUDGET` separated the peer budget from the UI budget, which fixed
"media starves the dashboard". It did **not** fix "one peer starves every other peer": the peer
budget is a shared pool of 1024, and a single source address could take all of it. A
per-listener number cannot express this; the limit has to be keyed by peer address.

**What was built.** `PeerConnectionSlots` in `server.rs`: a mutex-guarded
`HashMap<IpAddr, usize>` holding an in-flight count per peer, `PEER_PER_PEER_CONNECTION_CAP = 64`.

- Consulted on the accept path **before** the listener budget — a peer over its cap must not
  first consume a shared permit it will only hand back.
- Over-cap peers get **429 + `Retry-After: 5`**, deliberately distinct from the **503** that
  means budget exhaustion. Two failure modes, two codes, distinguishable in a log and by a client.
- The slot is moved into the connection task and released by `Drop`, so it counts **in-flight
  connections**, not accepts, and every exit path returns it. Entries are removed at zero, so the
  map cannot grow with uptime.
- Applied to the **peer listener only** (`peer_only`). Capping the UI listener would key on
  loopback and throttle the node's own dashboard.

**Unit evidence** (`server::peer_connection_cap_tests`): three behavioural tests against the real
limiter plus a wiring guard (constructed only under `peer_only`, consulted on accept, slot owned
by the task, refusal is 429). Negative control observed: mutating `>=` to `>` in the cap
comparison fails `one_peer_cannot_take_every_slot` (`2 passed; 1 failed`).

**Wire evidence** (`scripts/p2b-per-peer-cap-readback.py`, binary `4e2f961a…`):

| # | check | observed |
|---|---|---|
| 1 | cap readable from source | `PEER_PER_PEER_CONNECTION_CAP=64` |
| 2 | fips0 carries a ULA | `fd36:3c0f:d6f:8029:ea50:958b:672e:bb0` |
| 3 | binary serves both listeners | UI healthy, peer accepting |
| 4 | peer admits 64 concurrent | 64/64 held open |
| 5 | the connection past the cap | **HTTP 429** |
| 6 | refusal is 429, not 503 | observed 429 |
| 7 | UI unaffected at saturation | `/health` → 200 with 64 peer conns |
| 8 | released slot re-admitted | next connection → 200 |
| 9 | below the cap it is admitted | with 3 held, #4 → 200 |

`counts: PASS=9, NOT_VERIFIED=1, exit 0`.

Rows 8 and 9 are what make this evidence rather than coincidence: row 9 proves the refusal is
**count-keyed** (a latch or a machine-level limit would also refuse at depth 3), and row 8 proves
the slot is **returned**, not leaked.

**NOT VERIFIED:** isolation between two *different* peers. Every connection in that run
originates from this host, so they share one peer key. That half rests on the address-keyed
accounting proven by `one_peer_cannot_take_every_slot`. Closing it needs a second machine on the
mesh.

---

## 3. Bounded snapshot opens (P2, final gap)

**The hole.** Every delivery path called `tokio::task::spawn_blocking(open_matching)` directly,
unbounded. Tokio's blocking pool is capped (512 threads by default) and shared with every other
blocking call in the node — including the ones the UI needs. On a miss, `open_matching` may
`read_dir` over up to 10,000 snapshot directories, so a burst of purchases pins those threads
and starves unrelated blocking work while the async runtime still looks healthy. Connection
budgets do not bound the blocking work one connection can start.

**What was built.** `content_snapshot::open_matching_bounded` plus `OpenGate`
(`SNAPSHOT_OPEN_CONCURRENCY = 16`):

- **Backpressure, not rejection.** A waiter blocks until a slot frees. Refusing a delivery the
  buyer has already paid for is a worse outcome than waiting milliseconds for a directory scan.
- The permit is held across the blocking call and released on `Drop`, including on unwind.
- All four call sites route through it: `api/handler/cloud_purchase.rs`,
  `api/handler/onchain_purchase.rs`, `api/handler/lightning_purchase.rs`, `content_server.rs`.

**Unit evidence** (`content_snapshot::open_bounds_tests`):

- `opens_are_bounded_and_wait_rather_than_fail` — with both slots held, a third acquire does not
  complete; releasing one wakes it. Negative control observed: swapping `acquire_owned()` for
  `try_acquire_owned()` fails exactly this test (`3 passed; 1 failed`), because the third
  acquire then returns immediately instead of waiting.
- `slots_are_returned_and_do_not_drift` — 50 sequential acquisitions leave capacity at 4, and a
  task that panics inside the guarded section does not leak its permit.
- `no_call_site_bypasses_the_bounded_path` — reads the four modules' sources and fails on a bare
  `crate::content_snapshot::open_matching(` call, so a new handler that copies the old pattern
  cannot reintroduce the unbounded behaviour with every other test still green.
- `the_bound_is_meaningful` — the bound is non-zero and below Tokio's default pool size.

**NOT VERIFIED:** that a saturated open gate is observable on the wire as latency rather than
errors. That needs a fixture with many simultaneous purchases against a real snapshot directory,
which needs a real purchase (see §4).

---

## 4. What remains operator UAT

- **P3's "a film longer than 15 minutes streams to completion".** The *decision function* is
  proven on both prefixes (`/content/` seller, `/api/rental-playback/` buyer) and proven able to
  fail by a naive-stub control. The *duration* is NOT_VERIFIED: it needs >900 s of wall clock per
  case **and** a real purchased stream, which needs a live mint and buyer wallet.
- **A real purchase end to end.** No purchase can be fixtured here, so no test in this repo
  starts from real settled ecash and ends with bytes on a viewer's socket.

---

## 5. Corrected assumptions worth keeping

These each produced a confident wrong answer in this work:

1. **A DNS lookup is not a venue probe.** `getent hosts fips0` returns nothing on this host, which
   was read as "fips0 absent, so the peer listener cannot be tested". The interface exists with a
   ULA; the name simply does not resolve. Read `/proc/net/if_inet6` (as the binary itself does) or
   `ip -6 addr`. Under that wrong assumption, an entire class of claims was declared unverifiable
   that was in fact testable in minutes.
2. **A binary's mtime is not proof that earlier results were invalid** — and, symmetrically, a
   fresh build is the only way to attribute a number to a commit. Both errors appeared here; the
   fix is to rebuild and re-run, not to argue from timestamps in either direction.
3. **A pipe hides the status.** `cmd | tail` reports `tail`'s exit code. This produced a "successful"
   build that emitted no binary.
4. **Silent lookups look like absences.** Earlier in this work: cookie `csrf` vs `csrf_token`,
   fixture dir `content-owned` vs `purchased-content`, and case-sensitive header lookups against
   hyper's lowercase names. A probe that fails silently is indistinguishable from a negative
   result, so probes must fail loudly.
5. **A counter that only moves in a unit test proves nothing about the path.** Every read-back
   here drives a real listener on the real binary, and each carries a control that must NOT move.

---

## 6. Reproducing

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd ~/repos/archy/core
CARGO_BUILD_JOBS=1 cargo build --release      # ~15-28 min; jobs=2 OOM-kills rustc on 7 GB

export ARCHY_SUT=/home/c08r4d0r/repos/archy    # the scripts default to a worktree; pin it
python3 scripts/p1-media-counter-readback.py
python3 scripts/p2-admission-readback.py
python3 scripts/p3-media-deadline-readback.py
python3 scripts/p2b-per-peer-cap-readback.py   # needs a fips0 ULA; reports NOT_VERIFIED without

cd ~/repos/archy
ARCHY_TEST_REPO=~/repos/archy bash scripts/test-backend-isolated.sh   # the mandated runner
```

Expected, given this host's podman: **1 failure**, the quadlet `StopTimeout` test. Anything else
is a real regression.
