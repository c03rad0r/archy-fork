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
fence through cutover or recovery. Its exact source hash must match the installed script. Current binary startup
promotes its embedded controller before recovery/reconciliation, including over
an older runtime payload; the updater still verifies the exact on-disk hash.

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

## Explicit legacy preparation commands

The candidate adds two offline administration commands; neither starts the daemon,
changes a catalog, enables registration/publication, or starts/stops an app:

- `archipelago prepare-indeehub-registration DATA_DIR MANIFEST_JSON PRIVATE_OUTPUT_JSON`
  validates a private opted-in API manifest with an immutable image and both feature
  flags disabled, loads the existing node identity, invokes the existing installer
  pin provisioner, and writes a private resolved manifest. Retry preserves the same
  audience and identity; missing identity is an error, never identity generation.
- `archipelago prepare-indeehub-update DATA_DIR` validates the exact seven-member
  installed reviewed plan, original unit hashes, local target images and registration
  pins under the lifecycle lock. It preserves observed original unit recipes before
  a newer catalog can drift-reconcile them. This records original installation
  evidence, not a fabricated completed update. Explicit uninstall removes those
  recipes through the existing path. Prepare the complete reviewed plan first and
  retain the current catalog until this command succeeds for all seven members.

Binary startup now promotes its exact embedded maintenance controller before
recovery/reconciliation, including when an older dashboard payload is installed.
The updater still checks the on-disk helper hash against the binary. These source
changes passed full isolated backend qualification below; they have not been
applied to Yaya. The full adapter fixture is prepared in a separate outbound-isolated
QEMU copy-on-write VM, using public base images, the verified tracked baseline
catalog and newly generated fixture credentials; no live app metadata/data is copied.

Preparation qualification: the final combined isolated backend suite passes
**2,003 tests, zero failures, five ignored**. Installer preparation preserves the
existing identity/audience on retry and refuses absent identity or enabled feature
flags. Original-recipe tests cover all-member preflight, retry, foreign edit
preservation and explicit uninstall. An initial run had2,002 passes and one failure
in the new fixture's assertion that the journal directory was absent; the shared
readiness check creates an empty directory. The corrected test requires no journal
files, which verifies the intended absence of a fabricated completed transaction.
Evidence: `/tmp/archy-20261007-indeehub-final-backend-recheck.log`; initial failed
fixture evidence remains in `/tmp/archy-20261007-indeehub-final-backend.log`.
Optimized artifact build and full real VM adapter acceptance remain pending.

## Full RPC dispatch correction — 2026-10-07

Tracing the actual `package.update` path found that IndeeHub was marked as a stack
but missing from the pinned stack-image mapping. It therefore resolved only its
frontend before the seven-member runtime membership check. The mapping now covers
PostgreSQL, Redis, MinIO, relay, API, worker and frontend in dependency order.

Managed preflight now resolves the signed catalog references locally against the
reviewed immutable plan and checks original unit hashes/registration pins before
image preparation. Only verified local digest references reach preparation, so
unchanged mutable dependency tags cannot trigger a registry pull before the private
plan is checked. Unmanaged updates retain registry preparation. Inventory and plan
refusals clear progress and the inner Updating state; the asynchronous RPC wrapper
also retains its existing failure/scanner cleanup.

The final source passes **2,004 isolated backend tests, zero failures, five ignored**:
`/tmp/archy-20261007-indeehub-dispatch-final-backend.log`. A prior 2,004-test receipt
predated the last preparation-state cleanup and is not the final-byte result.
The optimized build was deliberately interrupted after these integration gaps were
found. A test-profile application executable is being built solely for the isolated
full RPC rehearsal; final release optimization/deployment remains pending that
rehearsal. No live IndeeHub stack or catalog has been changed.
