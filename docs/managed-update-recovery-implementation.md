# Managed update runtime recovery

Status: isolated source implementation; Rust and real Podman/systemd qualification
pending. No live update, snapshot, stop, backup or rollback has been performed by
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
