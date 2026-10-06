# Container cleanup must respect runtime ownership

Status: source correction under test; not yet deployed or accepted.

## Confirmed live failure (2026-10-06)

A disposable V4V container used a separate rootless Podman graph root and run
root, leaving the node's app inventory and existing demo volumes untouched.
It started successfully and returned HTTP 200 from `/healthz`. The management
service then terminated it. Its journal explicitly identified that container
as a ghost because its ID was absent from the default `podman ps` inventory.
The same failure occurred when its supervisor ran under a separate user service.
This is not an application crash or an out-of-memory failure.

The former reaper enumerated every conmon process on the host and compared all
of them against one Podman inventory. Absence from that inventory does not mean
that a container in another storage root is orphaned.

## Candidate correction

- Resolve the current Podman graph root, with a bounded command timeout.
- Require the same effective user and an exact container-ID-bound conmon bundle
  path under that graph root. Unknown bundle layouts are skipped.
- Treat failed inventory/root inspection as insufficient evidence to reap.
- Recheck the inventory and supervisor identity immediately before cleanup.
- Count only cleanup attempts actually performed, excluding skipped candidates.

## Acceptance still required

The isolated ownership/parser tests must pass, followed by the backend suite.
After deployment, restart the isolated V4V fixture and verify it survives
multiple reconciliation passes without becoming a My Apps entry. Verify that
existing managed container IDs and start times remain unchanged. Retain valid
orphan cleanup in the normal storage root and distinguish this from a claim
that all lifecycle failures are solved. No live production orphan is created
merely to exercise a destructive cleanup test.

## Second cleanup path and deployment repair (2026-10-06)

The isolated fixture survived the scoped Rust reaper but was subsequently killed
by the independent shell doctor's global conmon scan. Its service journal names
the fixture supervisor at the termination time. Removed that shell cleanup;
only the backend's storage- and owner-scoped cleanup remains. The fixture then
survived a complete scheduled doctor run.

Candidate deployment exposed a separate packaging/startup problem: an older
runtime script remained inside the frontend payload, which startup promotes into
`/opt`. The embedded repair then failed with EROFS under `ProtectSystem=strict`.
The dev box's safe helper was restored; Yaya rollout is held until verification.
The repair now uses the established host command mechanism, checks executable
permissions, and runs synchronously after runtime promotion before reconciliation.
Regression tests cover stale content, missing execute permission, idempotence and
installation failure. Actual sandboxed service restart remains an acceptance gate;
unit tests alone do not prove escape from the production mount namespace.
