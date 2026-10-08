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

## Manager startup guard found by real VM rehearsal — 2026-10-07

The isolated VM imported all seven public baseline images and started a fresh
synthetic stack with generated credentials, internal-only networking and retained
unit recipes. Offline original-recipe preparation succeeded. Starting the real
manager then exposed a destructive ordering bug before the first update RPC:
ordinary environment-drift reconciliation stopped/removed original members before
`install_fresh` reached its existing managed-recipe refusal. The journal records
this sequence for worker, API, PostgreSQL and MinIO. This was not a demonstrated
OOM: guest kernel records showed no OOM and its disk-backed swap was available.

The new guard precedes staged stops, secrets, hooks, dynamic configuration, drift
repair and generic recreate paths. It preserves explicit stop/uninstall markers,
validates saved unit bytes/mode, and observes an already-running managed runtime.
Missing or stopped managed runtimes refuse automatic repair; explicit owner
Start/Stop/Restart uses the exact saved systemd unit after validation. Restart
preflights all member units/images before reverse dependency stop and dependency
start order. Managed members skip legacy network, catalog-port repair and raw
Podman fallback. Held update records refuse these owner lifecycle mutations. The
outer ownership sweep also excludes saved/held members, and generic staged cleanup
refuses a saved member before disabling/removing its unit. Regression scenarios
verify unchanged runtime inventory and observation-only calls for running,
stopped, missing and modified-unit cases, plus durable stop/uninstall choices.
The health monitor also refuses automatic restart for saved, held or damaged
recovery evidence and reports the unhealthy app without mutating it. Updating
packages are excluded from health recovery. Combined isolated validation is
pending: the superseded compile was deliberately stopped before any tests ran
so these final lifecycle/health changes can share one full suite.

The VM is now off. SSH shutdown attempts timed out and ACPI powerdown did not
complete; QMP quit terminated only this transaction-idle synthetic fixture to
release its RAM during compilation. The next dedicated 4 GB boot must mask the
old manager in GRUB, check guest filesystem recovery, install the corrected binary
and recapture baseline before acceptance. Failed-run identities are retained in the VM's
`indeehub-fixture/startup-drift-failure-evidence`; all seven exact saved units were
verified and a new synthetic baseline explicitly recaptured. This recapture is
not rollback acceptance. The first RPC script stopped at its seven-running
precondition, so no missing-plan, rollback or successful update RPC acceptance
has yet completed. No live Yaya stack, catalog, wallet or payment was changed.

### Hardened-controller context correction (2026-10-07)

The final lifecycle/payment isolated suite passed 2,006 tests (zero failures,
five existing ignores), with all 532 captured inputs unchanged. Receipt:
`/tmp/archy-paid-indee-lifecycle-backend-20261007.log`. This result predates the
controller-launch change described below; it is not a current-source full pass.

An isolated VM probe reproduced another actual integration defect before rebuilding
its executable. Plain system-service `podman exec /bin/true` passed, but matching
the manager's `ProtectSystem=strict`, `Delegate=yes` and writable-path policy made
API and PostgreSQL exec fail with exit126. Both commands passed in a user systemd
scope. The scope retained the same inherited flock open description: inode check
and nonblocking exclusive re-lock both passed while the parent held the lock.
All seven container IDs remained unchanged by these probes. Private guest receipt:
`/home/archipelago/indeehub-fixture/context-probe.receipt.json`; bounded probe source:
`/tmp/indeehub-vm-context-probe.py` on the development host.

The controller now launches through `systemd-run --user --scope --quiet --collect`
before the pinned Python helper. This preserves synchronous pipes and inherited
lifecycle locking while executing rootless Podman from the user unit context.
Rust syntax and diff checks pass. Corrected executable and actual full transaction
qualification are still required; no live IndeeHub activation is claimed.

The fixture's earlier forced shutdown required root journal recovery and rebuilding
seven stale synthetic Podman runtime records from their byte-verified saved units;
their private pre-recovery inspection was retained. These recreated IDs must be
recaptured as a new baseline, never described as preserved across that shutdown.
A direct-kernel initramfs-only recovery boot installed an absent-marker manager
startup condition before normal boot, and `ConditionResult=no` verified the old
manager never started. The subsequent diagnostic boot shut down gracefully.

### Actual RPC preflight and nginx namespace evidence (2026-10-08 UTC)

Corrected VM executable built successfully from frozen inputs (normal binary
SHA256 `3cbe5a5c3a74e6463ab0874c74e23dfca68f0bfc3fe54883f0edf69abb37ac0d`;
stripped transfer SHA256 `753f8ba6ac342c8f4b9a19c8079a51dfd1da4dcb517d4ea4d0e54035c22f5788`).
Receipt: `/tmp/archy-indeehub-corrected-executable-20261007.json`. It predates the
later rental guard and nginx helper correction; not a release artifact.

Actual manager startup completed a62-app reconcile pass with all seven IndeeHub
members `NoOp`, every baseline container identity preserved, and API/manager
HTTP200. Subsequent real update attempts initially met the legitimate background
lifecycle lock. A temporary behavior-preserving fixture tracer confirmed later
admission succeeded. One early diagnostic teardown interrupted asynchronous
preflight and is invalid as source-defect evidence. The corrected diagnostic
waited for its terminal refusal before restoring the deliberately withheld plan
and removing the tracer; all seven original identities remained, no supervised
journal existed. No speculative lifecycle-lock patch was made.

The admitted reviewed-plan RPC then reached `Prepared → Editing → Restoring`.
All seven recovery images exist; target startup never began. The controller
remained `Prepared` because `sudo nginx -T` attempted to open `/run/nginx.pid`
inside the manager's read-only mount namespace. Holds and the unresolved journal
were preserved; this is not a successful rollback or migration receipt.

A manager-equivalent hardened VM probe passed with the fixed command
`sudo -n /usr/bin/systemd-run --quiet --wait --pipe --collect -- /usr/sbin/nginx -T`.
It validated the required ingress guards without printing effective configuration
or widening manager write access. The controller uses that fixed command now;
25 pure controller tests pass, including refusal before fence creation on dump
failure. A new helper hash requires a matching backend rebuild. Candidate-helper
qualification remains separate from actual backend transaction acceptance.

### Legacy worker termination qualification — 2026-10-07

The held disposable-VM operation `4320fe90-8ab5-4496-a4e7-cf11ca0fd376`
exposed the original worker's Node-as-PID1 SIGTERM behavior. A no-network,
no-volume probe from its exact recovery image exited 137 without init and 143
with init. This is not graceful shutdown or proof of completed jobs.

The controller now requires an exact six-field nonnegative integer queue
observation; absent `active` can no longer imply idle. Its narrowly scoped legacy
worker path records **forced idle termination**, with `graceful=false` and
`completed_work_claim=false`, only after paused all-zero observations before and
after, closed ingress/frontend, exact operation/original/recovery image and known
command, unchanged saved unit, original stop intent and died event, and proof the
process is dead. Other writers' 137 exits remain refused. All 31 pure controller
regressions passed, including missing/nonzero counts, reopened queue, changed
identity, command, unit and process state. Actual worker-only classification passed
under the hardened service → user scope with the lifecycle lock held.

The same fixture's API has npm as PID1 and one direct `node dist/main` child.
After fresh empty-business-state proof and durable stop intent, an exact-command,
parent-validated SIGTERM to that child stopped the original container; npm emitted
exit 1. The existing clean-exit gate correctly retained the hold. API termination
classification is still under review; no generic exit-1 allowance was added.
Frontend/worker are confirmed stopped; API is stopped but unconfirmed; storage
members remain running. Backup/fresh-restore, full RPC rollback/success and live
activation are **not passed**. Installed pinned helper remains unchanged; this is
private candidate-helper qualification only. Live Yaya remains unchanged.

Future worker image source now includes idempotent SIGTERM/SIGINT shutdown in app
commit `29627fc` with four passing Jest tests. Its image has not been built; the
previous frontend/API-only candidate catalog cannot cover that new worker image.

### API child-signal compatibility — 2026-10-07

The original API wraps `node dist/main` in npm PID1. Directly signalling that exact
child produces npm exit 1, not a clean exit. The candidate controller now records
this only as **non-graceful empty-business termination**, never as completed work.
It requires the exact original/recovery image and saved unit, automatic restart
policy `no`, fresh complete zero business-table/transaction counts, a paused empty
queue, and durable operation/container/parent/child PID+starttime+command signal
intent followed by syscall acknowledgement. Missing acknowledgement retains the
hold; retries cannot infer one. Extra direct children, reused process identity,
partial proof, OOM, forced exit 137, or replacement API writers are refused.

Post-stop queue verification now uses atomic, read-only Redis Lua, authenticated
through stdin rather than secret command arguments. The observed original API
queue endpoint must bind to the exact original Redis ID/image and shared network
alias. All six counts, prioritized and waiting-children must remain zero, with
admission paused. No default or absent field can establish empty state.

All **38 pure controller tests passed**, including actual Node execution of the
process selector. The actual hardened VM Redis observer passed. A uniquely named
recovery-image API process probe, with no persistent mounts or published ports,
passed exact child-starttime/signal-acknowledgement checks and exited 1 without
OOM; bound Redis observations before/after were paused and entirely zero. The
probe was removed. Its receipt is `api-process-probe.receipt.json` in the private
VM fixture directory. This qualifies the primitive, not a full backend update.
The earlier held API diagnostic lacks the new durable proof and remains
unaccepted; it must be restored by the matching rebuilt manager before a fresh
full update/rollback rehearsal. No live Yaya mutation or activation occurred.

### Restart policy and recovery qualification checkpoint

The actual retained Yaya units use `Restart=always`, `StopTimeout=30` and
`TimeoutStopSec=45`. The earlier synthetic `Restart=no` baseline was not
sufficient acceptance. The controller now holds automatic API restart through
an operation-owned runtime drop-in, without rewriting its retained unit body.
It records creation intent, requires exact owned bytes/mode and safe ancestry,
recreates a missing runtime override after reboot, verifies effective policy,
and removes only its own file at terminal release. The reviewed target must
retain the original Restart policy; a differing terminal policy stays held.

All **45 pure controller tests pass**. The actual hardened VM primitive passed
`always -> no -> always`, lost-runtime-file recreation, unchanged unit body and
no API startup (`restart-policy-probe.receipt.json`). This is primitive evidence,
not full transaction acceptance.

The rebuilt 9964b5c1 fixture executable reached native restoration but refused
with `Original launch configuration did not recover`. Its raw fingerprint
includes runtime-generated environment values/order. The manager was stopped;
all recovery images and the held transaction remain preserved. No old journal
hash was rewritten and no recovery was declared successful. Stable, versioned
fingerprints for fresh transactions are being qualified separately; legacy
records must retain strict comparison. The pre-target writer-preservation fix
in 07c7eb0f also awaits combined compilation/tests. No Yaya migration or catalog
activation has occurred.

### Actual seven-service rehearsal, 2026-10-08

The matching VM executable for 32317236 passed the full isolated suite:
**2,021 passed, zero failed, five existing ignores**, 532 inputs unchanged.
Its fresh child VM uses the real `Restart=always`, 30-second Podman stop and
45-second systemd stop settings. Manager startup preserved all seven registered
container IDs. The actual authenticated missing-plan update refusal retained
all seven IDs and cleared Updating. The subsequent controller recorded the
frontend exit 0, narrowly qualified idle-worker exit 137, and empty-business
API wrapper exit 1, including its operation-owned runtime restart override.

The database barrier then correctly refused an invalid table assumption:
IndeeHub's actual history is `public.typeorm_migrations`, not `public.migrations`.
Both compiled configuration files in exact original API image
`364a8d5dd4114349b9c09ac0b16b00aa066195294ae41399d72a85122db7b5a9`
and a read-only database existence query confirmed this. The helper now uses
that configured table, requires it in both compatibility snapshots and gives
no row-change exemption to an unrelated table called `migrations`.
**48 pure controller tests pass.** No history table or database row was edited.

Recovery also exposed that the native no-external-overrides guard rejects the
controller's own runtime restart fence. API-only exact ownership recognition
is checkpointed in 884ea492, with regression cases; its combined Rust validation
and executable remain pending. It does not admit arbitrary drop-ins.

A separate candidate-helper rehearsal, with the manager stopped and real
lifecycle flock retained, passed actual database commitments/dump and clean
MinIO/Redis stops. It then refused relay exit 137. The original relay image
`061516573b143b44e331f960036a6a3dc43c9b256ef8ca71afedbeb2cf797a4b`
uses a shell PID 1 around `nostr-rs-relay`. A disposable, networkless child-SIGINT
probe exited 130 without OOM; this is **not accepted as graceful**. Relay shutdown
remains under investigation. The held operation is
`3b3c564b-cce6-4727-8be8-369a95c479e4` in child fixture
`indeehub-v2-20261007T232717`. PostgreSQL remains intact; all original recovery
images and failure evidence are retained. The manager is stopped and startup
barred. The candidate helper was separate from the installed pinned helper.
Neither full target rollback nor successful update has passed. Yaya is unchanged.
