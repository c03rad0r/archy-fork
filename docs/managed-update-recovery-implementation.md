# Managed update runtime recovery

Status: integrated candidate; combined Rust suite and real Podman/systemd
recovery primitives pass. Full application cutover qualification is pending. No live update, snapshot, stop, backup or rollback has been performed by
this work. Active deployed source is unchanged.

The managed path captures original source Quadlet bytes, mode, immutable image,
container identity, launch configuration and running intent. It requires an
original-hash-bound reviewed forward plan and verifies every planned image against
the already prepared catalog image. New manifest configuration/hooks are applied
on the forward path; rollback uses the captured original recipe and a private,
local-only writable-layer image. AutoRemove rollback recreates containers and does
not claim to restore their original IDs. Stopped supervised stacks currently fail
before mutation; the retained-container and separate stopped-stage paths cover
only their respective supported cases.

The legacy IndeeHub controller runs under the same inherited lifecycle lock and
operation-owned reconciliation holds. Original writable layers are captured
before any destructive stop. The controller fences ingress, drains supported
legacy work, takes coherent quiescent volume/database backups and retains the
fence through cutover or recovery. Its exact source hash must match the separately
installed script; a backend binary alone does not install the controller.

Completed updates and verified runtime restorations publish exact unit recipes
before releasing holds. Routine drift reconciliation validates those recipes;
it does not regenerate them from a newer catalog. Catalog-driven pre-start file
and mount mutations are skipped for these pinned installations. Missing saved
units/images are explicit recovery failures, never permission to reconstruct a
different runtime. Explicit uninstall removes the installed recipe, and completed
old journals cannot recreate it. A new reviewed transaction replaces the recipe.

Opted-in API registration environments must match an already provisioned pin
and the existing node identity in both manifest and exact Quadlet. Administrative
plan preparation must use the existing installer provisioning code. Execution
never invents a node identity or accepts a browser-supplied unit or hook.

Runtime restoration does not establish database compatibility. The controller's
restored-release verifier compares original table schemas and row commitments,
permitting only the exact reviewed additive migration prefix and empty new
application tables. Any other data/schema change keeps ingress closed. This is
not automatic database rollback or a promise that arbitrary migrations are
reversible.

Qualification required before integration/activation:

- Isolated backend compile and injectable lifecycle/fault tests, including lost
  replies, daemon interruption, foreign units/holds and preflight failures.
- Disposable real Podman/systemd and PostgreSQL execution of the controller and
  adapter. Sixteen pure controller tests currently pass; SQL/runtime behavior is
  not yet qualified.
- Final seven-member private plan with installer-resolved identity environment,
  verified local images and source-unit provenance; review required changes and
  retained operator configuration without exposing secret values.
- Disk-capacity and recovery-image retention checks, backup integrity and a
  documented recovery path for missing runtime artifacts.
- Actual-node controlled deployment and acceptance, preserving persistent data.

## Resumed qualification — 2026-10-07

Backup verification now requires the full database/four-volume artifact set and
rechecks SHA256, including same-size corruption. Nineteen pure controller tests
pass. The owned, network-none PostgreSQL fixture passes unchanged/additive
commitments, rejects four data/schema/history mutations, restores a real custom
dump with matching original commitments, and rejects a truncated dump. It mounts
no live volume and removes only its own container. This does not qualify actual
application writer drain or the complete supervised systemd cutover.

The updater compiled and its full isolated suite ran: 1,958 passed, one failed,
five ignored. The failure is the old snake_case rental receipt JSON fixture;
`c2c4d915` already corrects that exact test on the release candidate branch.
Do not duplicate or suppress it here. Integrate and rerun the complete candidate
suite before claiming a green backend gate. The earlier interrupted compile
and PostgreSQL timeout remain failed/incomplete attempts, not acceptance.

Evidence: `/tmp/archy-resumed-20261007-updater-full-backend.log`,
`/tmp/archy-resumed-20261007-indeehub-controller-tests-final.log`, and
`/tmp/archy-resumed-20261007-indeehub-postgres-restore.log`.


## Integrated candidate checkpoint — 2026-10-07

Local integration at `7fb7ee80` passes the complete isolated backend suite:
2,000 passed, zero failed, five ignored. The stale receipt fixture failure above
is resolved by the already-integrated correction. Disposable real Quadlet
AutoRemove recovery primitives pass with injected target failure, original
writable-layer/configuration restoration and unchanged persistent fixture bytes.
Repeatable fixture: `tests/lifecycle/supervised-runtime-primitives.py`.
This does not qualify the complete seven-member app drain/cutover adapter.

The fresh-backup restore barrier is being added after this checkpoint; its
qualification and new embedded-controller build remain separate from these
previously passing results. No live IndeeHub deployment has been changed.


Fresh database backup restoration now runs through the controller's production
method in a disposable network-none PostgreSQL with no external mounts or
published ports. The original local image is pinned, dump restore must exactly
match captured database commitments, and durable proof binds the operation,
image, dump hash and baseline. Ownership-checked cleanup survives retry and
refuses foreign fixtures. Verification rejects a missing/stale restore proof.

All21 pure controller tests pass. The real PostgreSQL fixture passes valid
restoration, rejects truncated and wrong-database dumps, checks cleanup and
retained admission on failure, and retains the four prior mutation rejection
checks. Evidence: `/tmp/archy-20261007-fresh-backup-restore.log`.
Final real restore-barrier checks pass on PostgreSQL15.17 and16.13 after waiting
for the final TCP server instead of the temporary Unix-socket bootstrap server.
The initial PG15 restore failure is retained as failed evidence; its private
command stderr was removed by fixture cleanup, so no exact cause is claimed.
The stale backend compile was interrupted after the readiness edit; a fresh
backend build/suite remains required for the final embedded controller.
Volume-archive restore and the full supervised app cutover remain open gates.

## Four-volume restore barrier — 2026-10-07

The controller now restores each fresh volume archive into separate owned storage
before target startup. It rejects unsafe paths/links and unsupported special files,
checks restored bytes, links, ownership and modes, and rearchives the restored tree
to compare ACLs and extended attributes explicitly (GNU tar compare alone does not
check xattrs). Durable proof binds all four archive hashes to the operation. Failure
retains ingress and lifecycle holds; retry removes only the operation-owned fixture.
No production volume is mounted or modified by restore verification.

All24 pure controller tests pass. The real rootless fixture passes four archives
containing hidden files, hardlinks, symlinks, mapped numeric ownership, ACLs and
xattrs; changed restored bytes/xattrs and unreadable archives are rejected. Evidence:
`/tmp/archy-20261007-volume-controller-tests.log` and
`/tmp/archy-20261007-volume-restore.log`. Repeatable fixture:
`tests/regression/test_indeehub_maintenance_volumes.py`.
Full seven-member application cutover and final backend build remain separate gates.
