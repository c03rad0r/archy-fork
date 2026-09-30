# Repair and release execution — 2026-09-29

**Status: IN PROGRESS. Do not publish an OTA or ISO until the release gates pass.**

User requires all tasks completed and tested on the development box before the
next OTA and raw ISO. Passing unit tests alone does not establish live correctness.

## Confirmed evidence

- Dev-to-Shorty 100-sat Cashu file purchases failed twice. Both sellers' and
  buyers' accepted mints match. Shorty's mint swap returned HTTP 422; both
  attempted purchases were refunded 100 sats. The old message guessed a mint
  mismatch without evidence.
- Wallet import repaired truncated V2 keyset IDs, while paid-content redemption
  bypassed that repair. Central swap repair and protocol-level regression tests now pass.
- Core installation on dev reused existing chain data. At 17:42 UTC it was
  advancing through block replay with no Core container restarts. At 17:49 UTC
  it had connected to peers and started transaction-index synchronization.
- LND exited repeatedly with `bitcoind start timeout` while Core loaded. After
  Core became available LND stayed running and reported waiting for backend sync.
- Framework source fix 4237fb5e is already an ancestor of main. Existing live
  reboot/native balance evidence is in the incident document. Final display
  confirmation remains pending.

## Changes under validation

- Cashu V4/V2 ID expansion at every swap; fee-aware underpayment rejection;
  single-mint/sat-only/cryptographic paid tokens; no false mint-mismatch or
  unconditional refund claims. Missing content checked before redemption.
- mempool.space default; migrate old tx1138 default with fresh consent, retain
  local explorer priority and custom preferences.
- Core/Knots optional pruning on the version modal and app detail install path;
  persist choice across runtime restarts; use identical 50,000 MiB automatic
  pruning entrypoint behavior on large and small disks.
- Plain Bitcoin block-index startup message; defer LND wallet initialization or
  unlock until Bitcoin RPC is usable; authenticated dependency status and LND UI
  waiting states; no partial total displayed as a complete balance.

## Validation and release gates

- [x] Final backend regression suite passes (including mock mint HTTP and real
  curve signatures, v1/full-v2/truncated-v2, fees, errors, duplicate redemption).
- [x] Initial explorer and pruning modal tests pass: 15 tests.
- [x] Both actual manifest entrypoints tested with isolated fake bitcoind across
  6 disk/choice combinations each. No existing chain pruned for this test.
- [x] Initial LND UI install/start/sync/recovery and invalid-balance tests pass.
- [x] Frontend production build and relevant existing wallet tests pass (34
  focused tests, including 12 Home failure/recovery checks). Final UI suite: 1,120 passed; production build passed. Full release harness and final frontend follow-up passed.
- [x] Fault tests and final source review complete.
- [x] Candidate deployed with rollback to dev and Shorty; hashes verified.
- [x] Live paid-file purchase succeeds; failed purchase/refund behavior verified.
- [x] Live waiting/UI verified on dev; recovery covered by deterministic tests.
- [x] Framework operator acceptance and authorization to release recorded.
- [x] Release version/changelog, catalog/image implications, signing prepared.
- [ ] Signed OTA built, tested, published to git and ngit.
- [ ] Raw ISO built, boot-tested, signed and published; download command supplied.

Tests must not wipe/recreate wallets, prune the operator's existing full chain,
or claim that arbitrary failures can never happen. Record material gaps before
release. Signing keys remain with the user; prepare concrete artifacts first.

### Further startup findings

Live dev `/v1/state` returned `RPC_ACTIVE` while `/v1/getinfo` timed out during
Bitcoin initial sync. Candidate startup now recognizes the already-unlocked
state instead of repeating unlock attempts for ten minutes. The health watchdog
also now excludes Bitcoin initial sync, warmup, unavailable/stale status and
LND height progress from its restart criteria. A later observed `podman restart`
was externally initiated; its precise caller has not yet been established, so
the watchdog defect is a source finding rather than a confirmed attribution.

Framework SSH rejected the previously provided login on 2026-09-29. No password
was saved and no wallet changes were attempted. The human display-confirmation
question remains pending. Do not repeat a Framework reboot to reconfirm old work.

LND UI waiting-state, stale-balance, partial-failure/recovery and prompt-render
tests pass (4 Node tests). Waiting states avoid calls to LND endpoints that block
until sync, and prevent overlapping refreshes.

### Final source validation

The final backend suite passed: 1,548 passed, zero failed, four existing ignored
live/hardware tests. Includes saved pruning preference, rejecting an old catalog
that cannot honor explicit pruning, and all nine paid-Cashu protocol tests.
Unsigned candidate catalog passes strict drift and fleet registry trust checks.
The release gate caught a missing What's New entry; generated it from the curated
changelog and reran the frontend gate/build. No public release has been changed.

At 18:23 UTC dev Bitcoin exited with status 137 and restarted; current container
is not marked OOM-killed and no kernel/oomd record identified the cause. Bitcoin
is replaying blocks again (height 482071 at 18:31 UTC). Installed old LND continues
to time out while Bitcoin RPC warms up. Candidate is not deployed yet; verify its
readiness deferral live before declaring this fixed. Do not attribute the Bitcoin
exit to a specific actor without evidence.

### Doctor restart cause established and repaired

Full system journal identifies container-doctor at 18:23:21 UTC issuing raw
`podman restart bitcoin-core` for an allegedly missing 8333 listener. The same
script restarted LND at 17:57:48 and 18:23:35 UTC. The port was actually listening.
Reproduced the original `ss | awk | grep -q` pipeline returning `0 141 0`: grep
exits after its match, awk gets SIGPIPE, and pipefail falsely reports no listener.
The raw restart also enforces a short stop timeout and races Quadlet cleanup.

The repaired check consumes the entire socket snapshot, distinguishes inspection
failure from a missing port, and leaves containers running when inspection fails.
Necessary restarts use their managed systemd units and shutdown timeouts; unmanaged
Bitcoin/LND fallback receives 600/330-second grace respectively. Regression uses
20,000 socket rows plus mocked service/container commands and passes. Thirty
read-only checks of the actual Bitcoin listener pass. Script deployed to dev and
Shorty with root-only rollback copies. OTA runtime payload includes scripts/.
This evidence supersedes the earlier unknown-caller/unknown-exit attribution.

### Initial candidate live validation — 18:48 UTC

Source 0f85f588, optimized backend SHA256
84434c495c5f8472cf6bfcb6c65e762502c74718ad88271619373335c0054bb6,
deployed to dev and Shorty with matching hashes and rollback copies. Both
management services restarted; wallets/channels were not reset. Old embedded
runtime assets restored the old doctor on backend startup; updated the live
script AND embedded runtime copy on both nodes. Final OTA will contain the new
script directly.

Authenticated dev readiness transitioned from waiting_start to waiting_sync.
Real Chromium at 1440px and 390px showed Waiting for Bitcoin to sync, an unknown
balance, and no blocked native LND calls. Screenshot review also caught invented
zero capacity/channel counts during waiting: corrected them and the empty-channel
recommendation; five UI regression tests now pass.

Real Minibits Cashu purchase from dev to Shorty succeeded for one sat and returned
the expected 44 bytes. A rejected one-sat underpayment was refunded exactly, and
two cached downloads charged zero. Temporary seller files/catalog entries removed.
The first test runner expected data_base64 while the first-purchase API returns
data; cached responses use data_base64. Existing purchase clients only consume
data, so a follow-up normalizes both response variants to both fields.

The optional Files copy failed because FileBrowser owns host paths as mapped UID
100000. Follow-up uses its authenticated API with override=false and collision
suffixes. A live API probe succeeded, refused overwrite with HTTP409, preserved
original bytes, and cleaned up. New protocol tests cover folder creation, escaped
names, collisions, authentication failure, disk-full, and unavailable service.
Full backend suite for these follow-ups is running; do not package the earlier
backend as final.

### Follow-up validation and OTA delivery check

Paid-response and Files API regressions passed in the full backend run: 1,552
passed, zero failed, four existing ignored tests. Live browser waiting checks
passed again after removing invented zero capacity and channel counts.

OTA inspection found that companion image :local (created by old installers and
used on dev) bypassed both source-staleness detection and rebuilding. The earlier
assumption that build-context detection covered these nodes was incorrect.
Follow-up applies the existing source-mtime/stamp checks to both :local and
:latest, preserving the existing tag and rebuilding only stale source. Existing
image-ID comparison then restarts the UI companion onto the new image. This does
not restart LND itself. Regression covers every companion's two local tags; final
backend suite is running. Verify the resulting live rebuilt image before release.

### Test isolation finding — release remains blocked

The next full run passed 1,552 tests but one existing boot-loop timing test failed.
Its output and node logs exposed an independent test defect: MockRuntime tests
still invoked real Quadlet service operations and Podman socket recovery. These
caused further LND/companion restarts during unrestricted unit runs. They were not
a recurrence of the repaired doctor port check. Stopped unrestricted testing;
LND has remained running since 19:02:46 UTC during isolated test execution.

New isolated runner hides live wallets, service buses, container storage and host
process IDs, supplies a private network and temporary writable fixture paths,
and keeps host filesystems read-only. An independent boundary probe passed.
Test-only service helpers use a temporary Quadlet directory and simulated service
results; mocked runtimes skip real Podman socket/network provisioning. Host file
helpers require the isolated-runner marker and execute inside the namespace
instead of escaping through sudo/systemd-run. Release harness and AGENTS now
require this runner. Initial isolation trials correctly blocked host operations
and exposed fixture permission assumptions; final runner compiles and executes
the full suite with those fixture paths isolated. No final pass claimed yet.

Main dashboard candidate and AIUI build at b634f41a are now deployed on dev; served
index SHA matches the build. Live package.versions returns bitcoinPrune=false
for Core and Knots, preserving current automatic mode. Existing full chain stays
unpruned. Final backend (Files/cached response/legacy UI delivery follow-ups) is
not yet deployed; earlier 0f85f588 backend remains live on both nodes.

Final isolated backend run: **1,553 passed, zero failed, four existing ignored**
in 13 seconds after compilation. Boundary probe confirms no host service buses,
live wallet data, host process IDs, or external network. Bitcoin/LND start times
remained unchanged during isolated execution. Production helpers are unchanged;
the namespace-specific command behavior is compiled only into unit tests.
Release and ISO gates now use the isolated runner.

### Final backend deployment and App Store follow-up — 19:36 UTC

Full release harness passed: static/catalog checks, frontend type-check and
1,117 frontend tests, cargo-check, and isolated backend suite (1,553 passed,
four existing ignored). Final optimized backend built successfully; SHA256
16a173129672cbb40c250446ec52ba4a9bd1974cbb3a4988f90c6f3187b7a1f7.
Deployed to dev. Legacy :local LND companion automatically rebuilt at 19:35 UTC
and restarted onto image 702c0cd88fb5c8a561c76dabdb96c40648dd62d401c78f2e10d4318b06f02abe.
Served UI bytes match candidate source. Native Bitcoin/LND start times unchanged.

Actual desktop pruning screenshot exposed horizontal overflow; moved the
explanation below the app header. The App Store uses Marketplace.vue, a separate
install path from Discover.vue. Its first Install button bypassed the version
modal. The browser check therefore sent an unintended Knots install request at
19:28 UTC. Core remained running, no Knots container was created, and the full
chain was not pruned. Removed only the newly created Knots installed-app record
and newly created version config; preserved root-only rollback copies.

Marketplace now uses the shared version/pruning modal. Added integration tests
for both Core and Knots: no install request until confirmation, selected version
and pruning forwarded, cancellation sends no install request. Four Marketplace
tests pass (three new plus existing refresh check). Further browser checks block
package.install requests at their network boundary. Final frontend rebuild and
post-fix live checks remain pending. Final paid-file follow-up is still pending.

### Unsigned release candidate ready — 19:46 UTC

Final frontend source/build attribution: 3612458e. Production dashboard and AIUI
builds passed. Final frontend suite: 1,120 tests across 139 files passed.
Desktop 1280px and mobile 390px browser checks passed for the app detail pruning
choice and App Store version modal; no horizontal overflow and no installation
request. Screenshot review confirms readable controls and explanation. Browser
installation requests are blocked during these selection-only checks.

Final backend SHA above matches both dev and Shorty. A fresh one-sat purchase
passed on those exact binaries: correct file bytes, both response field aliases,
exact one-sat refund on underpayment, zero-charge cached repeat, and exact Files
copy. Temporary seller entries/files and Files test copy removed; transaction
audit and owned cache retained. Total net transfer during the two live purchase
rounds: two sats from dev to Shorty. Desktop/mobile LND waiting checks passed
again on the automatically rebuilt companion. Native Bitcoin and LND stayed up.

Prepared, unsigned files:
- releases/pending/v1.8.20-alpha/app-catalog.json
- releases/pending/v1.8.20-alpha/manifest.json

Staged OTA backend: 64,716,656 bytes, SHA256
16a173129672cbb40c250446ec52ba4a9bd1974cbb3a4988f90c6f3187b7a1f7.
Frontend archive: 97,152,297 bytes, SHA256
658b78fce0dfa20a627c987dd153b24cbac15adbde905cc6518744c637e12802.
Artifact sizes/hashes/release notes validate. Checked actual archive: flat
layout, readable root permissions, exact doctor/LND UI source bytes, and fresh
AIUI attribution. Catalog has zero metadata drift and passes fleet registry trust.

Remaining: user-local release-root signatures, Framework's final display
confirmation, signed publication to git/ngit, then raw ISO build/boot test/signing
and publication. No v1.8.20 public release or tag exists yet. Four pre-existing
hardware/live tests remain ignored. Bitcoin sync-to-ready recovery is covered
by deterministic tests; the live node remains in initial sync. Do not describe
these checks as proof against every possible network/payment failure.

### Signing and release authorization — 2026-09-30

Both catalog and OTA signatures verify against the pinned release root. Staged
artifact hash/size checks and catalog drift/trust checks pass. User accepted the
remaining Framework display check and explicitly authorized release. Publication
and ISO build may proceed; do not regenerate the signed manifest or artifacts.

### Published OTA; ISO withheld after live shutdown defect — 2026-09-30

Signed 1.8.20 OTA/catalog published to git and ngit, with public asset hashes
verified. Catalog rollout triggered a Bitcoin command update at 08:34 UTC.
Although the orchestrator allowed a long stop, Quadlet's generated Podman removal
still used its ten-second default and killed Bitcoin. Core replayed its block
index; LND later lost its connection to the previous Bitcoin container IP.

Stopped the ISO build and queued boot check; any partial 1.8.20 ISO is invalid
and must not be published. Preparing 1.8.21 to supersede the immutable signed OTA.
Installed explicit graceful-stop systemd overrides on dev and Shorty without
restarting native services. Candidate Quadlet fix adds per-app container, systemd,
and command-wait budgets, including existing containers and uninstall fallback.
Focused 43 tests pass, including actual Quadlet generator stop-before-remove order.
Full tests, disposable slow-stop verification, build and deployment remain pending.

Disposable live regression passed: started an Alpine container with its legacy
ten-second stop setting, rewrote and reloaded its Quadlet with explicit twenty-
second graceful stop, verified the same container ID and old internal timeout
remained running, then stopped it. Its twelve-second shutdown handler completed
in 12.6 seconds, emitted the completion marker, and exited without SIGKILL/137.
Fixture had no network or wallet mounts and was removed afterward.

Core finished index loading and resumed unpruned initial sync. LND automatically
unlocked at 08:47 UTC. The existing backend-address cascade then performed a
graceful LND restart at 08:57 UTC after Bitcoin reconciliation completed; LND
automatically unlocked again and reached chain-sync waiting. No manual wallet
unlock or restart was used for this recovery.

### False dependency restart exposed during monitoring — 09:08 UTC

The initial 1.8.21 candidate passed all 1,557 isolated backend tests and 1,120
frontend tests. Monitoring nevertheless found another managed LND restart at
09:08:32 while Bitcoin's container/start timestamp remained unchanged. Management
logs explicitly attribute it to the backend-address cascade. This also makes
the earlier 08:57 cascade suspect; it must not be described as a proven necessary
restart. These service restarts preceded the isolated test executable, whose
namespace boundaries remain intact.

The cascade trusted Started/Installed action reports. A failed runtime inspection
followed by successful systemctl start of an already active unit can produce
Started without changing Bitcoin. Dependency restarts now require observed
container-ID, running-state, or start-time changes. Failed observations remain
unknown, not absence; a known absent backend becoming running still qualifies.
Actual exec-drift restarts are recognized even when their outer report is NoOp.
Stopped/lifecycle-in-flight dependents remain excluded, and user stop markers
are re-read after the potentially slow pass. Added runtime-observation and
false-action/real-exec-drift regression cases; full isolated rerun pending.
Stopped the first optimized build and preparing new artifacts from this correction.

### Final 1.8.21 artifacts and live verification — 2026-09-30

Source and frontend/AIUI attribution: c993d9dd. Full isolated backend suite:
1,559 passed, zero failed, four existing hardware/live tests ignored. Frontend
suite: 1,120 passed; final production type-check/build passed after the last
release-note-only edit. Optimized backend built in 13m22s.

Staged unsigned 1.8.21 OTA manifest and artifacts:
- Backend: 64,748,176 bytes; SHA256
  ff602e85f340aff7e43d9d94f7f84f11f713735c964c0d8ba150e23b065c30eb.
- Frontend archive: 97,152,546 bytes; SHA256
  6c0842ec83a440269a353808a4cf154174f5232c9989b4a5448bc6486e1d0620.

Artifact validator passed. Actual archive has flat paths, readable root index,
and exact fresh AIUI, doctor and LND UI payload bytes. Exact files deployed to
dev at 09:32 UTC and Shorty at 09:35 UTC; rollback binaries and dashboards under
root-only /var/lib/archipelago/support/release-1821 on each node. Only management
services restarted. Existing Bitcoin/Core-or-Knots and native LND container IDs
and start times were preserved. Correct generated graceful-stop commands are
present before forced removal on both nodes; temporary grace overrides removed.
Dev systemd deadlines are 615 seconds for Bitcoin and 345 seconds for LND.

Desktop/mobile Lightning UI checks passed again: waiting for Bitcoin sync,
unknown balance, no unavailable native RPC requests. Served dashboard and AIUI
attribution bytes match the release. Native LND states: dev RPC_ACTIVE while
Bitcoin syncs; Shorty SERVER_ACTIVE. Dev completed full reconciliation passes
at 09:34:21 and 09:36:40 with Bitcoin/LND NoOp, and no dependency restart.
Shorty's first full pass completed 09:36:55 with Knots/LND NoOp.

Final paid-file check on these exact binaries passed: fresh one-sat dev-to-Shorty
purchase, exact one-sat refund on underpayment, identical response aliases,
correct Files copy, and zero-charge cached repeat. Removed temporary seller
entries/files and Files copy; retained purchase audit and owned cache. Total net
transfer across all three live payment rounds in this repair session: three sats.

Remaining: finish Shorty observation and remove temporary diagnostic logging;
user-local 1.8.21 OTA signature (existing catalog signature remains valid),
publish git/ngit, build/boot-test/sign and publish the raw 1.8.21 ISO.
No 1.8.21 release tag or public OTA yet. Do not publish the quarantined partial
1.8.20 ISO. The existing 1.8.20 git/ngit release notes now explain the withheld ISO
and pending hotfix; signed 1.8.20 assets remain immutable.

Shorty's second clean full pass completed at 09:38:06 UTC. Removed temporary
diagnostic logging on both nodes and restarted only management again; native
Bitcoin and LND IDs/start times remained unchanged, with generated stop settings
still verified. No temporary graceful-stop overrides remain. Catalog signature
verifies against the pinned release root; final 1.8.21 artifact validator passes.
The candidate is ready for the user's local OTA signing ceremony.

### 1.8.21 publication completed — 2026-09-30

The operator signed the OTA manifest and subsequently the ISO checksum JSON.
Both signatures verified against the pinned release root. The signed OTA was
published on git/ngit, and the operator confirmed that Framework could see the
update. Source main and the annotated `v1.8.21-alpha` tag were published.

Raw ISO:
`archipelago-installer-1.8.21-alpha-unbundled-x86_64_RC1.iso`

- Size: 2,682,419,200 bytes.
- SHA256: `8667b5522c476a40e29abba19df4180086191527a194a88765aa70ed527f9406`.
- Build and ISO smoke checks passed. The mounted backend matched the staged
  OTA backend hash, and the full dashboard/AIUI tree matched the fresh build.
- An isolated UEFI QEMU guest, with no network or host disks attached, booted to
  the installer prompt. The VM was stopped and the ISO unmounted afterward.
  This was an installer boot check, not a full installation onto hardware.
- Uploaded the raw ISO, plain SHA256 sidecar and signed checksum JSON to the
  [1.8.21 release](https://source.archipelago-foundation.org/lfg2025/archy/releases/tag/v1.8.21-alpha).
  The stored server file hashes matched, the public ISO headers and first/last
  byte samples matched, and both public checksum files matched byte-for-byte.
- The ngit downloader's full-ISO acquisition exceeded its fixed 30-minute
  deadline on the available connection. Published Nostr asset records using
  the already verified hashes, sizes and public URLs with the existing ngit
  signer; both repository relays acknowledged them. Ngit then accepted those
  records and final readback resolved all five release assets with the expected
  hashes and sizes. No new release-root signing was performed by the assistant.

Final publication evidence: `/tmp/archy-1821-finish-events.log`,
`/tmp/archy-ngit-1821-complete-view.json`, and
`/tmp/archy-1821-verified-asset-events.log` on the development box.

Subsequent review of PRs #161/#162 found additional delivery and concurrent
file-save edge cases. Their repaired, tested branches are recorded in
[the next-release review](pr-review-20260930.md); those changes are not in the
signed 1.8.21 artifacts. The X250 kiosk selector report is also tracked for the
next release. No claim of exhaustive hardware or network-failure coverage is
made for this release.
