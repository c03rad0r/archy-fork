# D1 — request collapsing: design, decisions, and what is deliberately not built

**Branch:** `fix/media-stream-deadline` (consolidated onto `fork/main`)
**Status:** index implemented and negative-control-proven; collapsing + demotion NOT started
**Date:** 2026-10-23

This document records the reasoning behind D1 — including the parts that are decisions
rather than code, because most of D1's risk is in the decisions.

---

## 1. What D1 is actually for

The plan names two things. They are not equally valuable, and conflating them is how this
deliverable gets mis-reported:

1. **An `open_matching` index.** Today `content_snapshot::open_matching` walks
   `content-snapshots/` **on every request**: `read_dir` over up to 10,000 entries, then
   `open_at` + `read_manifest` (a JSON parse) per entry until a match. For one object that is
   I/O proportional to the **whole catalogue**, repeated for every viewer and every resume.
2. **Request collapsing.** N concurrent viewers of one object share one origin reader, with a
   ring buffer (W ≤ 5 s) and per-subscriber cursors.

**(1) is the win. (2) is resource control and must be described as such.**

### The thing I am NOT claiming

Coalescing does **not** increase fan-out. `min(P, N·b)` is invariant: you cannot send more
than the origin can produce, however cleverly you share it. Anyone claiming D1 "fixes
fan-out" has measured CPU, not network. D1 buys:

- fewer origin reads (`reads ≈ 1×S` instead of `N×S` where the page cache does not already
  collapse them — M3 predicts it largely does, in which case the win is CPU and descriptor
  count, not disk),
- fewer open descriptors,
- bounded per-object work under load.

That is the honest claim and the commit message says it.

---

## 2. What landed: the index

`snapshot_index.rs` maps `(content_id, sha256, size)` → snapshot directory name.

### Decision: the index carries no authority

The plan forbids trusting a cached answer for authorization, and forbids keying on
`content_id` alone. The index resolves **which directory**, never **may this reader have
these bytes**.

A hit supplies a directory name; `open_verified` then runs the *same* manifest, `SourceStamp`
and mode-`0o400` checks through the same confined-descriptor path as before. A stale or
poisoned entry therefore causes a **miss or a failed verify → fall back to the scan**. It
cannot make an unverified file readable.

Implementation detail that matters: the verification lives in one function (`open_verified`)
used by **both** the indexed and the scanning path. Duplicating the checks for the cached
path is exactly how a cache silently becomes an authorization bypass.

### Decision: key is `(content_id, sha256, size)`, and that is not a contradiction

The settled decision is that cache keys use `(content_sha256, content_size)`, never
`content_id`. This key includes `content_id` — because `open_matching` filters on it and two
sellers may publish identical bytes under different ids. It is part of the **lookup**, not
the **cache identity**: a different sha256 or size is a miss even for the same `content_id`,
and the test `a_different_hash_or_size_is_a_miss_not_a_hit` pins that.

### Decision: bounded, evicting arbitrarily

`content_id` comes from a seller, so an unbounded map is a memory-exhaustion vector. The
index caps at 10,000 entries.

Eviction drops an **arbitrary** entry rather than implementing LRU. A `HashMap` has no order,
and pretending to be LRU by inserting a parallel recency structure buys nothing here: a miss
costs exactly the old behaviour (one scan). Scoring a miss is not worth a second data
structure that can itself drift.

### Decision: no directory watcher

An inotify watcher would be a second source of truth and another thing to keep correct. Entries
are validated **on use** (`open_verified`) and dropped via `invalidate` when a caller observes
them stale. A miss is cheap and correct; a watcher is a new failure mode.

### Evidence (observed, not asserted)

```
test snapshot_index::tests::one_object_is_indexed_once_and_served_from_memory_afterwards ... ok
test snapshot_index::tests::a_different_hash_or_size_is_a_miss_not_a_hit ... ok
test snapshot_index::tests::invalidate_forces_a_miss ... ok
test snapshot_index::tests::the_index_is_bounded ... ok
test content_snapshot::tests::repeated_lookups_of_one_object_hit_the_index_instead_of_rescanning ... ok
```

**Negative control.** Disabling the index lookup (restoring always-scan) makes the
`open_matching`-level test fail:

```
panicked at content_snapshot.rs:427:
assertion `left == right` failed: the first lookup must be a miss (it has to find the snapshot somehow)
  left: 0
 right: 1
test result: FAILED. 0 passed; 1 failed
```

That failure is the point: the test measures the index, not a function's existence. Control
restored and re-verified green afterwards.

---

## 3. Not built: collapsing and the demotion rule

### The problem, stated plainly

With a shared ring buffer, **one slow viewer fills it**. Fast readers overtake, the buffer
wraps, and serving the slow viewer means either stalling everyone or dropping bytes it has not
read. Without a rule, one slow client degrades **every** viewer of that object — strictly
worse than today, where separate TCP connections isolate them.

### The decision (operator-confirmed 2026-10-23)

**(a) Demote, with a cap** — when subscriber X falls more than W behind, close its cursor into
the shared buffer and give it its own independent reader. Cap concurrent demoted readers per
object (~4 suggested); past the cap, **(b) disconnect**.

Rejected: **(c) stall the group** — that is the current bug with extra machinery.

Why (a): a paying viewer's stream is the thing being protected. An extra origin read is cheap
next to a truncated film. The cap stops demotion from degenerating into N independent readers,
which is the exact state D1 exists to avoid — uncapped demotion *is* no coalescing, with extra
bookkeeping.

### Why this is not built yet

It is 8–12 days, and it changes behaviour on the live media path for paying viewers. The
plan's own rule applies: a deliverable whose semantics are undecided must not be half-built.
The index — the measurable win — is landed and proven; the collapsing layer is a separate,
larger change with its own review.

### Non-negotiable when it is built

Authorization must stay **per subscriber, per chunk** (`registered_media.rs:287-293`,
`rental_playback.rs:330-370`). Checking once at group-join is a security regression, and it is
the obvious "optimization" a collapsing implementation invites. The named guard
`per_subscriber_rental_window_is_still_enforced_mid_stream` must exist and fail without the
per-chunk check.

---

## 4. Boundaries — what this does NOT show

- **No wire measurement.** The index is proven with counters at the `open_matching` level, not
  over HTTP. A `localnode` venue run is needed to show the scan is actually avoided in a live
  node, and that is **NOT_VERIFIED**.
- **M3 is unrun.** If the page cache already collapses concurrent reads (`reads ≈ 1×S`), the
  index's disk-side benefit is smaller than the I/O count suggests; its real win is then CPU
  and descriptor count. I have not measured this, so I do not claim either way.
- **No coverage percentage.** This repo measures no coverage. The substitute is: every new
  public function has a test that fails without it.
- **The merge is operator-only.** This branch is not on `fork/main` until the operator merges.
- **No upstream PR**, not even a draft, per the standing directive.
