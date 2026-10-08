# Legacy IndeeHub maintenance controller

Status: isolated source implementation. Fourteen pure Python fake-runtime regressions
pass; no live invocation or production qualification. The controller is not part
of the already signed private app candidate and needs no new app image/API.

The supervised updater owns the lifecycle flock, seven durable holds, original
Quadlets and private writable-layer recovery images. It records its destructive
obligation before invoking the fixed controller with bounded JSON over stdin:

```
python3 /opt/archipelago/scripts/indeehub-maintenance-controller.py acquire
{ "operation_id": "<uuid>", "original_members": [
  { "name": "indeedhub", "container_id": "<64hex>", "image_id": "<64hex>",
    "unit_sha256": "<64hex>", "config_sha256": "<64hex>", "running": true }
  // All seven exact members; JSON does not include this illustrative comment.
], "recovery": false }
```

Other actions are `verify` with operation_id, and `release` with operation_id and
outcome committed/restored/aborted. Replies are <=4KiB and report drained, held,
released, or recovering for explicit recovery acquire. Inherited
ARCHY_UPDATE_LOCK_FD stays open and is passed to child commands; the script never
unlocks it. Journal: data/update-transactions/indeehub-maintenance/<uuid>/journal.json.

## Forward sequence

- Validate exact original IDs/source-unit hashes and known port exposure. Only
  frontend127.0.0.1:7778 is supported; direct backend/S3 ports refuse before stop.
- Require deployed native AppGate and legacy nginx maintenance guards. Inspect
  every known legacy sublocation and any direct7778 proxy; unknown routes refuse.
  Save an operation-owned readable sentinel, then verify local ingress returns503.
- Record prior BullMQ transcode pause state, globally pause future job admission,
  retain queued/delayed/failed jobs. Gracefully stop frontend ingress; bounded
  polling waits for active transcodes to finish before stopping worker and API.
- Require successful systemd shutdown plus an exact original Podman died event
  with exit0. Forced exits and missing event evidence retain the hold and are
  never labelled completed writes.
- While PostgreSQL remains running, capture a fresh custom dump. Cleanly stop
  MinIO/Redis/relay/Postgres, then archive all four complete quiescent volumes
  (including SQLite WAL and Redis persistence) with metadata. No volume deletion
  or migration rollback. Archive hashes/size and per-step obligations are durable.
- Keep admission closed while the updater renders, starts and verifies targets.

## Interrupted recovery

The node first records phase Restoring with boolean target_startup_began, then
calls acquire with recovery:true. That path preserves the original failure and
fence; it does not retry a killed original into a fictitious successful drain or
claim missing backups exist. The node restores exact saved old runtime under the
same hold. Release before any target startup can state only that original runtime
was restored. If target startup/migration began, recorded data-compatibility
verification is required before restored release; an old image alone does not
prove compatibility with newly changed data. No automatic DB/media restore exists.

## Qualification and remaining integration

`python3 tests/regression/test_indeehub_maintenance_controller.py` passes fourteen
fake-runtime cases in temporary directories, without services/network/containers.
Source nginx template guard coverage also passes its parser check. Production
adapter compilation, actual Podman event format/systemd clean-exit behavior,
application writer shutdown, interrupted backup and supervised restart still need
isolated lifecycle fixtures and then coordinated node acceptance. A long-lived
WebSocket or active upload can exceed graceful-stop deadlines; the current code
refuses completion and preserves recovery obligations rather than silently
calling interrupted work finished.

The deployment must install the exact qualified controller script and record its
hash alongside the backend artifact. Binary-only deployment does not install it.
The backend must refuse missing/mismatched prerequisites before snapshots/stops.
Native AppGate + nginx guards are separate node source changes owned by the
supervised updater agent. The signed app catalog/private image receipts remain
unchanged. Existing live stop/uninstall intent must not be rewritten as maintenance.

A pre-acquire snapshot/preflight failure may leave no controller journal. An
Aborted node journal with target_startup_began=false then permits idempotent
no-op acknowledgement, without touching any other operation’s admission fence.
A matching fence without its controller journal requires recovery investigation.

Read-only source evidence from actual old API/ffmpeg shows neither has SIGTERM
shutdown hooks. The controller permits worker143 only after a paused queue has
zero active jobs. Legacy API143 additionally requires closed/stopped frontend,
stopped worker, and a fresh empty projects/contents/payments/shareholders/
subscriptions/library_items store with no other active DB transaction. This is
a narrow first-upgrade compatibility path, not evidence populated work completed.
Populated or ambiguous legacy state remains a refused forward cutover.

### Operation-bound rollback data verification

Restored release after any target startup now performs its own PostgreSQL
compatibility check; it does not accept a manually asserted verification boolean.
Before the coherent backup, the controller captures a read-only, repeatable-read
transaction containing every original public table's columns, constraints,
indexes, triggers, row-security policies, row count and canonical row SHA-256,
plus the exact migration history. The private operation journal binds this
baseline to the original operation UUID.

After original runtime recovery, while ingress and worker admission remain
closed, the controller captures the same observations again. It requires original
tables and definitions unchanged, original non-migration rows unchanged and the
original migration-history prefix intact. Additional migration records must be
the exact ordered three migrations qualified for API commit `3b09b81`, and only
their five named new tables may appear, all empty. Any unexpected data or schema
change keeps ingress closed. Successful proof records before/after commitment
hashes and the operation UUID before release. No down migration or automatic
volume restoration is performed.

Sixteen pure Python regressions pass, including altered rows/schema/history,
foreign operation, nonempty added tables and durable proof before fence release.
The SQL transaction and Podman lifecycle still require isolated integration
qualification. These are table-level compatibility checks, not a claim that
arbitrary database extensions/functions, other writers, or changed application
code are safe. The candidate images, migration scope and admission barrier must
also match the reviewed operation.
