# Archipelago release work handover — 2026-10-07

This document is the resumable state of play for the post-1.9.0 work. The
candidate worktree is `/home/archipelago/Projects/archy-session-key` on branch
`work/post-190-session-key`. Do not use credentials from chat or print private
node, wallet, session or registry data.

Read **Current live checkpoint** first. Older receipts below apply only to their named source/artifacts; later entries retain detailed evidence and corrections.

## Current live checkpoint — supersedes historical entries below

- Work continues in `archy-session-key`, branch `work/post-190-session-key`.
  Latest backend changes: rental guard `4dde14bf`, Fleet collector provenance
  `9268930c`; both await the next combined isolated backend run. The previous
  2,006-pass suite predates those changes and the maintenance user-scope fix.
- Live dev/Yaya dashboard index is
  `92050cbda69954f57f4b0bdd73d89623f6aae9fc66e4c074ee4f46bdcb9c3b2a`.
  Guarded preservation and browser checks passed. **Phone artwork still failed**;
  the operator explicitly parked it until all other tasks finish (task23).
  APK0.5.37 background playback/notification presence remain accepted.
- Original1.9.0 ISO software release is now published on ngit with all three
  original assets; see the publication receipt below. No next OTA/ISO/catalog
  release is accepted or published.
- IndeeHub's corrected integration executable (scope/lifecycle fixes) built with
  stable input hashes; SHA3cbe5a5c3a74e6463ab0874c74e23dfca68f0bfc3fe54883f0edf69abb37ac0d.
  Its seven-service VM manager-start acceptance passed. It predates the rental
  and Fleet changes and is not an optimized release artifact.
- VM update/rollback acceptance is still open. Properly awaited missing-plan
  refusal passed; temporary tracing was removed. Reviewed-plan execution reached
  Prepared/Editing/Restoring, but stopped before target startup because nginx -T
  inherited the manager's read-only mount context. Proven helper fix b03d3c89
  invokes that fixed inspection through a root system service;25 focused Python
  tests pass. Candidate-helper standalone drain/backup qualification is underway
  against the synthetic held operation before another embedded-hash backend build.
  Standalone helper then passed fence closure, queue pause and frontend stop,
  but refused the legacy worker exit137. Original worker lacks signal shutdown
  handlers; safe drain compatibility is under investigation. No database backup
  or target startup was reached. Do not classify forced exit as graceful.
  No controller hash bypass; original recovery images/journals/holds retained.
  Earlier lock refusals were periodic reconciliation contention, not a proven
  persistent source defect. Do not claim full rollback/update acceptance yet.
- The exact selected demo is Yaya Cloud `/Photos/web54321-balanced-2.mp4`,
  203331877 bytes, SHA256766f5f6b9d2a6fd90cb7541447fbba666773635bd90def145bcd479a1b8b0591.
  Read-only hash and Chromium metadata (1920x1080,249.766667s) pass. Preserve source.
  Backstage publication, cross-node paid viewing and bounded payment authorization
  remain separate open gates; do not spend or select another personal file.
- Fleet alert-error/provenance/history UI is integrated f5a1806a+fd98b383,
  not live:29 focused tests and app typecheck pass;390/1440 browser checks cover
  alert/provenance, while history has a real component regression. Frozen combined
  UI suite passed1459tests/177files with all771input hashes stable. A subsequent
  alert-row/cache provenance delta is integrated as ff522086:31 focused tests
  and exact app typecheck pass. Cached nodes and alerts retain content but lose
  identity claims until a fresh server response. Final artifact build is next;
  do not claim the full suite covered that later delta. Deploy Fleet UI with its
  corrected backend, never treat an older server's report claims as proof.
- Task21 public UI groundwork `6bcdd106` (37 tests + app typecheck) and task22
  Mesh picker groundwork `3360354a` (10 tests) remain isolated/unintegrated. They
  do not enable live menus or complete backend/global search/paid delivery/folders.
  Their end-of-backlog delivery order remains in force.
- Paid-file agent owns the next full isolated backend run, after VM shutdown and
  explicit final input freeze. Never run unrestricted cargo tests on a live node.
- Optional snapshot helper1347627 was cancelled with all three originals intact;
  do not resume its PID. Five earlier UI archive relocations remain completed.
- Private Android debug.keystore remains untracked; never stage it. Preserve
  wallets, app data, stopped/uninstalled choices. Framework startup incident is
  closed separately from the still-open paid-file regression.

## Historical starting candidate

Recent commits, newest first:

- `ce3ec965` — keep the companion PiP test compatible with the project target.
- `0d8decb3` — integrate retained Cloud video PiP and companion lifecycle code.
- `4d938cd5` — add the read-only release UAT checklist.
- `3f96be2c` — handle nullable tunnel addresses in firewall status.
- `ef6c10f0` — add the central firewall and tunnel status screen.
- `b50bf615` — explain unaffordable fee-bump quotes.
- `973dbeb4` — register the on-chain purchase HTTP handler.
- `6547ae05` — integrate two-phase on-chain purchase recovery.
- `558f097f` — bound Federated Nodes sync and show progress feedback.

The worktree was clean after the last commit. Recheck before editing.

## Verified evidence

- Framework LND startup incident: **closed with operator acceptance**. The
  wallet/channel identity and balances survived a controlled reboot. Do not
  confuse this historical incident with the open paid-file release regression.
- Backend isolated suite: **1,958 passed, 0 failed, 5 ignored** in
  `/tmp/archy-integrated-full-backend.log`.
- Integrated on-chain focused suite: **46 passed, 0 failed**.
- On-chain payment UI focused tests: **66 passed, 0 failed**.
- Bump-fee UI/backend focused tests: **10 UI + 10 backend passed**.
- Federated sync focused tests: **2 passed**.
- Firewall router tests: **7 passed**; Vue type-check passed after
  `3f96be2c`.
- Companion PiP/browser tests: **8 passed** on the integrated candidate; the
  companion branch reported **9/9** across its two targeted suites.
- Android PiP validation: local SDK, unit tests and debug APK build passed;
  no physical Android device is attached.
- Current UI production build: passed after the PiP target-compatibility fix.
  The current private archive is `/tmp/archy-current-candidate-ui.tar.gz` with
  SHA-256
  `0aff96999cf0c2c46ec68537021d214519da6e2d1189c9d0dc65717f8c3e1f24`.
- A previous full dashboard receipt had **1,411 passed, 1 failed**; do not
  call the full UI suite green until that remaining failure is reproduced or
  explicitly classified.

## Deployment state

- Reliability backend and the prior corrected UI are deployed to development,
  Yaya and Framework with identities, sessions, containers, catalogs and
  stopped/uninstalled choices preserved.
- The new current UI archive has **not** been deployed. The old deployment
  script is pinned to an earlier archive and qualification receipt; create a
  fresh receipt for the current archive before using it.
- Yaya IndeeHub remains the original live `1.0.0` deployment. Private candidate
  images are imported and digest-pinned locally, but no catalog activation,
  migration or app lifecycle mutation occurred.
- V4V is live on Yaya with native Nostr login, Browse launch, artwork, player
  controls and background-player behavior verified in browser UAT. Companion
  physical-device acceptance remains open.
- The V4V registry artifact endpoints now return `401` anonymously. Do not
  publish another image or catalog without explicit privacy verification.
- No new OTA, ISO, public catalog, real payment, wallet operation or live
  firewall change has been performed.

## IndeeHub preparation

Private backup and artifacts are under
`~/.local/state/archipelago/session-recovery/`.

- Yaya PostgreSQL custom-format backup SHA-256:
  `167df2e80a7ba64e21909ad8ddeb2751ea0c02428d7210eb82a774a6cf220d42`.
- Isolated restore preserved 32 original application tables, retained 107
  migrations, applied exactly 3 additive migrations and passed an idempotent
  rerun. The fixture used no network uplink and did not mutate Yaya.
- Private frontend/API images are imported and digest-pinned. The active seven
  IndeeHub containers, volumes, identities, session key, catalog and stopped
  intents remain unchanged.
- Local qualification is green: 113 frontend tests, 204 backend tests,
  production frontend/API builds, rental checks at 390/1440px, Backstage checks
  at 320/390/1440px, panel-on 35/35 and registration 190/190.
- Live distributed acceptance is blocked by no live project/media, disabled
  publication worker, absent installer registration pins and unverified
  cross-node relay delivery.

## All task groups

1. **IndeeHub publishing/paid viewing — open:** source and migration work is
   qualified; private cutover is staged; publish → discover → pay → timed
   playback → resume without repayment is not accepted.
2. **Nostr login/companion grey screen — partial:** dashboard fixes live;
   physical companion acceptance open.
3. **Peering/discovery — partial:** repairs live; wider reciprocal recovery and
   relationship UX acceptance open.
4. **Framework monitoring — partial:** source repair and reboot acceptance are
   complete; normal owner-browser/TOTP confirmation remains.
5. **Immich/Nextcloud — open:** assessment/design only; connector not enabled.
6. **Web5 connection journey — partial:** cosmetic and flow fixes exist; final
   UX review remains.
7. **Companion launch speed — partial:** dashboard loading fixes deployed;
   physical companion performance remains.
8. **Fleet acceptance — open:** broad supported-function matrix remains.
9. **AIUI/provider/funding — partial:** setup/browser checks pass; paid provider
   and companion acceptance remain.
10. **Offline/network map — partial:** fixture-tested; real outage/recovery open.
11. **Web5/Cloud/tab speed — partial:** improvements exist; full performance
   proof open.
12. **Fleet metrics/FIPS — partial:** dev/Yaya checks pass; fleet-wide failure
   and transport qualification open.
13. **V4V Yaya demo — partial/live:** browser native-login/player acceptance
   passes; physical companion background media remains.
14. **Peer files/Indee streaming — partial:** transfers and cached recovery pass;
   timed distributed playback remains.
15. **MeshCore — queued:** no two-radio acceptance claim.
16. **Web5 card/footer UX — partial:** implemented/deployed; final visual UAT.
17. **HTTPS apps — partial:** routing repairs pass; exact hostname/trust and
   companion acceptance open.
18. **Firewall/tunnel UI/settings — source slice done:** read-only screen,
   independent statuses, refresh spinner and mobile route tests pass; live
   persistence/reboot/rollback qualification remains.
19. **Reusable media/PiP guide — partial:** audio guide and companion PiP source
   are present; physical Android acceptance remains.
20. **Companion background media — queued/partially implemented:** audio/video
   should continue on the phone after app close, with native controls, queue,
   artwork, authorization and video PiP. Physical V4V acceptance is open.

21. **Public files/folders — queued, after prior tasks:** Make public in both More
    menus, discovery by non-peered/non-federated nodes, the same peer-sharing
    pricing interface with blue tints instead of orange, optional charging and
    a tiny blue marker at the icon's top-left. Full scope is backlog task21.

22. **Mesh file sharing — queued, final task after21:** Share file opens From node
    / From computer; node picker has tree navigation, search across all own files,
    retained multiple selections at the bottom and Send. Plan node-to-node
    delivery and support paid files using the usual recipient payment interface.
    Full scope is backlog task22.

## Additional release-regression items

- Paid-file recovery must never issue a second payment; source protections and
  tests exist, but end-to-end seller settlement/receipt acceptance remains.
- Framework bump quote logs on 2026-10-07 showed three read-only failures:
  `Not enough wallet change for this fee; choose a lower rate`. The UX fix is
  implemented and tested; live acceptance is not performed.
- Fast fee defaults are implemented and tested, preserving explicit slower and
  custom selections.
- OTA/ISO publication is blocked until UAT, rollback evidence, exact candidate
  hashes, ngit/Gitea mirror parity, signed catalog and release ledger checks
  pass. Never force-push or publish a partial mirror.

## Immediate next actions

1. Reproduce/classify the one historical full-dashboard UI failure and rerun the
   full suite on the current candidate.
2. Create a fresh current-archive deployment receipt; deploy the UI to dev only,
   verify hash, health, session key, containers and catalogs, then run browser
   smoke tests.
3. Prepare the private IndeeHub candidate catalog with actual imported image
   digests and installer registration pins; qualify updater rollback before any
   activation.
4. Run authenticated IndeeHub UAT with controlled project/media fixtures and
   no repeat payment; verify cross-node relay delivery and timed playback.
5. Install/test the companion debug APK on a physical Android device for V4V
   audio background playback and Cloud video PiP.
6. Run the release UAT checklist at
   `docs/release-uat-checklist-20261007.md`.
7. Only after all gates pass, perform exact mirror parity checks, sign the
   catalog, prepare OTA/ISO artifacts and request release approval.

## Safety rules for the next agent

Use `scripts/test-backend-isolated.sh` for backend tests. Do not run real
payments, bump transactions, wallet recovery, catalog publication, firewall
changes, broad compose down/up, volume deletion or wallet recreation while
qualifying. Keep raw node logs and credentials private. Treat any failure in a
release receipt or preservation check as a stop-and-investigate condition.

## Resumed qualification — 2026-10-07

- Current source at `a9a19751` passed the complete dashboard suite: **1,435
  tests / 174 files**, zero failures. All 14 BalanceAmount cases passed in
  411 ms; the historical 20-second timeout did not reproduce. No balance
  implementation change was made to obtain this result.
- Production UI build passed, with all 768 captured tracked UI/catalog inputs
  unchanged. Archive SHA256:
  `c62591d8c761bbac7701c1f7264ebdcd822e38276f68b4d5b86d0006c7a8d124`.
  Served index SHA256:
  `8e36064a7357c20ddcc3f1e7118d13f81d68e3d35a7724ef22cbea16303683c0`.
- Deployed that UI to **development only**. Backend digest, session key,
  container IDs/start times and four catalog files were unchanged; health and
  served index checks passed. Actual authenticated browser smoke passed at
  390/1440px with no page errors, horizontal overflow or payment operations.
- Durable qualification artifacts:
  `~/.local/state/archipelago/release-qualification/ui-resumed-8e36064a7357/`.
  Dev rollback:
  `/var/lib/archipelago/support/reliability-ui-20261007T161407Z-427000/rollback.sh`.
- Retained updater work was found on `work/stopped-update-recovery` at
  `2512a9f6`: twelve commits, not integrated in this candidate. Its own
  `docs/managed-update-recovery-implementation.md` explicitly requires Rust
  compilation and disposable Podman/systemd/PostgreSQL qualification. Do not
  recreate this work or activate IndeeHub based solely on its fake-runtime tests.
- The private activation notes require a coherent fresh database/media backup,
  operation-owned admission hold and verified runtime rollback. The older
  isolated migration fixture is not a cutover backup.
- Operator requested delivery of an APK before physical checks; USB debugging
  is not required. Companion **0.5.35/build55** is reserved for the integrated
  Cloud PiP candidate. Packaging and current Android qualification are underway;
  this does not establish native background-audio acceptance.

## Companion APK delivered — 2026-10-07

- Canonical clean package/sign/verify completed for **0.5.35/build55**, package
  `com.archipelago.app.debug`. v1/v2/v3 and the existing signing certificate pass.
  APK SHA256: `f4b8337889e8dcc95f32e531abbb23b05ec003c7e34b0c1ba4f7c197dffc99b9`.
- Corrected a Kotlin expression-return compilation error in CloudVideoPip. The
  prior failed build is retained as failed evidence, not acceptance.
- Current Android suite: **17 tests**, zero failures/errors/skips (3 origin
  checks, 6 download checks and 8 fullscreen lifecycle cases).
- At the operator's request, the exact signed APK is served from archi-dev-box
  at `/packages/archipelago-companion-0.5.35.apk`; HTTP bytes verified against
  the build. No standard fleet download, dashboard, service or catalog changed
  in this delivery step. Test Cloud PiP against the matching dev dashboard.
- Physical install/PiP acceptance remains pending. This APK does **not** implement
  a dedicated native background-audio service; that V4V requirement remains open.
- Yaya SSH access restored using an ephemeral control connection. No password
  was stored on disk. Yaya's dashboard and IndeeHub have not been changed here.
- Current session permits reviewed escalations again; the prior permissions
  blocker is superseded. Packaging logs, Android XML and delivery receipt are
  retained under `~/.local/state/archipelago/release-qualification/companion-055/`.
- Retained updater source now has complete-backup inventory/hash validation
  with 19 pure tests passing. Its disposable PostgreSQL commitment fixture
  passes unchanged/additive schema and rejects four data/schema/history
  mutations. Full Rust and real supervised Podman/systemd qualification remain
  required before integration or IndeeHub activation.


## Operator PiP acceptance and continued release work — 2026-10-07

- Operator confirmed the delivered companion PiP works (“works great”). Record
  this as operator acceptance of the reported PiP flow, not independent proof
  of every rotation/network/expiry case or native V4V background-audio support.
- Retained updater is now integrated locally at `7fb7ee80`. The combined isolated
  backend suite passed **2,000 tests, zero failures, five ignored**. Prior notes
  saying integration/Rust qualification are pending are historical.
- Real disposable Quadlet/Podman AutoRemove primitive qualification passed:
  injected target failure, restoration from the original writable-layer image,
  preserved volume bytes and original configuration. The full seven-member
  application drain/cutover adapter is still not qualified by this primitive test.
- Evidence: `/tmp/archy-resumed-20261007-integrated-full-backend.log` and
  `/tmp/archy-resumed-20261007-runtime-primitives-recheck.log`.
- Active work: require restoration of each fresh database backup in an owned,
  network-isolated PostgreSQL before IndeeHub target startup. Hash verification
  alone is insufficient. All 21 controller tests pass; real restore/fault checks
  passed, including the actual production restore method, rejection of truncated
  and wrong-database dumps, owned fixture cleanup and retained admission fences.
  Evidence: `/tmp/archy-20261007-fresh-backup-restore.log`.
  This script change needs a fresh embedded-controller backend
  build; do not call the older 2,000-test result evidence for this new change.
- Task21 is appended at the end, including the operator's explicit requirement
  to reuse peer-sharing pricing with blue tints. It is not implemented yet.


### Current status overview requested by the operator

“Nearly finished” means implemented with a bounded acceptance/review step left;
“In progress” still includes substantive implementation or distributed testing.
No percentage is inferred from test counts.

| Task | Status | Remaining work |
| --- | --- | --- |
| 1 IndeeHub publishing/paid viewing | In progress | Full supervised cutover, distributed publication/payment/timed playback |
| 2 Native signer/companion grey screen | Nearly finished | Physical login/resume/session recovery acceptance |
| 3 Peering/discovery | In progress | Reciprocal offline/reconnect and expanded connection UX |
| 4 Framework Monitoring | Nearly finished | Owner-browser Monitoring check |
| 5 Immich/Nextcloud | Slightly started | Design assessment exists; connectors unimplemented |
| 6 Web5 connection journey | In progress | Deferred final flow review and expanded UX |
| 7 Companion launch speed | In progress | Actual device measurements and remaining latency work |
| 8 Fleet acceptance | In progress | Full supported-function/fault matrix |
| 9 AIUI/provider/funding | In progress | Provider/funding and companion acceptance |
| 10 Offline/network map | In progress | Real outage and recovery qualification |
| 11 Web5/Cloud/tab speed | In progress | Complete performance evidence |
| 12 Fleet metrics/FIPS | In progress | Fleet failure and transport qualification |
| 13 V4V Yaya demo | In progress | Browser demo live; native phone background playback remains |
| 14 Peer files/Indee streaming | In progress | Distributed timed playback |
| 15 MeshCore | Queued | Two-radio work and acceptance |
| 16 Web5 cards/footer | Done | Dev/Yaya mobile and desktop geometry, long labels and keyboard navigation passed |
| 17 HTTPS apps | In progress | Exact hostname/trust and companion acceptance |
| 18 Firewall/tunnel | In progress | Read-only UI done; settings persistence/reboot/rollback qualification |
| 19 Media guide/Cloud PiP | Nearly finished | PiP operator-accepted; finish reusable contract and remaining edge cases |
| 20 Native background media | In progress | Operator confirms APK57 background playback works; notification artwork missing. Full controls/queue/recovery matrix remains. |
| 21 Public files/folders | Queued, before22 | Shared pricing interface with blue tints, unrelated-node discovery and tiny marker |
| 22 Mesh file sharing | Queued, final task | Node/computer choice, tree/search picker, retained multi-selection, delivery design and usual paid-file flow |

Completed subitems: Framework LND startup incident, companion download/viewer
acceptance, physical upload acceptance, APK55 delivery and reported Cloud PiP
flow. Earlier complete UI/Android suites pass; a fresh backend rebuild is required
for the final restore barrier. OTA/ISO is not ready: payment/recovery, IndeeHub,
fleet/device qualification, mirror review/parity and final artifact gates remain.


### Fresh backup barrier qualification result

- Final controller:21 pure tests pass. Real production restore-barrier tests
  pass on PostgreSQL15.17 and16.13, including valid restoration, wrong-database
  and truncated-dump refusal, retained admission, owned fixture cleanup, and
  four original data/schema/history mutation rejections.
- The first PG15 attempt failed during pg_restore. That attempt's command error
  log was in an automatically removed fixture directory, so its exact cause is
  not proven. Readiness inspection found the bootstrap Unix-socket server could
  be mistaken for the final server. The controller/fixture now wait for TCP
  readiness; both final version runs pass. Do not erase the failed attempt.
- The backend rebuild begun before the readiness edit was interrupted deliberately
  because its embedded source was stale and it competed with restore tests.
  **Final embedded-controller backend rebuild/suite and deployment remain pending.**
  The prior 2,000-pass receipt applies to `7fb7ee80`, not this changed controller.
- Durable logs:
  `~/.local/state/archipelago/release-qualification/indeehub-backup-restore-20261007/`.
- No live app/container, database, wallet, catalog, payment or public file was
  changed. Disposable fixtures used network-none and no live volumes.


### Mesh sharing addition — operator request

Task22 now follows task21 at the end. Its detailed scope is in
`post-1.9.0-work-backlog.md`. This is a recorded requirement/design task, not a
claim that the picker or transport/payment flow has been implemented. Existing
public-sharing blue pricing and marker requirements are retained unchanged.


## Companion56 native-audio qualification — 2026-10-07

- Operator withdrew the accidental screensaver/Play/Skip prompts. No changes
  were made for them, so nothing required reverting.
- Companion0.5.36/build56 is delivered at
  `http://192.168.63.240/packages/archipelago-companion-0.5.36.apk`.
  SHA256 `6bee2a2a13231525997946926bb47c473e01b81c03737d22d1bce18af9982c6e`.
  Canonical signing, clean packaging and HTTP byte verification passed. The
  standard fleet APK and live backend/catalogs were preserved.
- 29 Android tests pass (including native bridge, foreground notification and
  task-removal retention on SDK28/35). These are automated tests, not phone
  acceptance. Durable receipt:
  `~/.local/state/archipelago/release-qualification/companion-056/`.
- The native MediaSession/foreground service controls the same retained
  authenticated WebView player, with queue controls, bounded thumbnail data,
  session/origin checks and logout/stop cleanup. No protected stream URLs or
  credentials are sent to Android and no second decoder/payment is started.
- Initial complete dashboard qualification passed1440 tests/175files and its
  production build. That UI was deployed on dev with backup and unchanged
  backend, catalog/session/intent hashes and container IDs/start times.
- Live LAN HTTP browser inspection then found `crypto.randomUUID` absent. The
  new bridge now generates UUIDv4 using `crypto.getRandomValues`, which that
  browser supports. A regression covers this exact condition; final full UI
  suite/build and replacement deployment are underway. Do not ask the operator
  to accept native audio until the matching corrected Yaya UI is deployed.
- Yaya remains at its previous UI until qualification completes. Physical
  Android background, lock-screen, swipe-away, reconnect and stop acceptance
  remains required. CloudPiP55 remains operator accepted.
- Dedicated IndeeHub agent owns backend changes and the full isolated seven-app
  fixture. Its first latest suite had2002pass/1failure/5ignored: the new fixture
  incorrectly expected no directory, although an empty directory is valid.
  Assertion corrected to require no journal files. Final isolated recheck:
  **2,003 passed, zero failed, five ignored**, committed as `b9c75b21`. This
  is not a live IndeeHub delivery; optimized binary and full VM cutover still
  remain before Yaya activation.

- First HTTP-fix full UI run:1440 passed, one60-second Home wallet-display
  timeout and a worker `snapshotSaved` timeout under severe memory/I/O pressure.
  Focused rerun:all12 Home wallet-cache tests and all6 companion bridge tests
  passed (18/18). Clean full rerun is underway; retain both failed and passing
  logs. The initial1440-green full receipt predates the UUID fix.
- Extended real V4V browser qualification supports optional simulated Android
  transport while using the actual authenticated iframe and bundled free demo
  media. It checks native command routing and JPEG thumbnail delivery; no
  physical Android result is implied. The new end-to-end run remains pending.


### Companion56 dashboard delivered and live V4V qualification

- Final full dashboard suite passed **1,441 tests /175files**, zero failures,
  with the LAN HTTP UUID fix. Production build passed. All770 recorded UI and
  catalog inputs matched. Source checkpoint:`235f6d04`.
- Exact UI archive SHA256:
  `31b5370351511072c01f71915881e3fb2becf01a621ad725aa13ad89b48b60e1`.
  Served index SHA256:
  `4d9424a4ad7548221a07e2c4f14019029ac5fc4e3f1d27ff5eca72b5ff0f6a56`.
- Deployed on **dev192.168.63.240 and Yaya192.168.63.169**, with rollback
  backups. Backend, catalog/session/stopped/uninstalled hashes and container
  IDs/start times were unchanged on both. Authenticated dashboard browser
  checks passed at390/1440 widths on each node; no overflow or page errors.
- Actual Yaya V4V native Nostr login, bundled free demo playback, companion
  command routing (pause/play/next/previous/shuffle), JPEG thumbnail delivery
  and same-iframe reopening passed at390/1440. Android transport was simulated
  for these browser checks; physical lock-screen/background acceptance is
  still open. No payment occurred.
- Operator was given APK56 URL and **Yaya http://192.168.63.169** as the test
  server, with background/lock-screen/notification-control checks. Durable
  artifacts and all failed/successful test receipts:
  `~/.local/state/archipelago/release-qualification/companion-056/ui/`.
- New operator request:hide control bar during fullscreen “What is Web5.”
  Asked whether dashboard bar or video's own controls; pending reply. Current
  stated assumption is dashboard bar. Narrow fullscreen entry/exit suppression
  is being checked separately; it is not yet in the above deployed artifact.
- IndeeHub optimized build is now running against the reusable release cache;
  VM remains off until that finishes. Exact existing catalog-signing request
  was recovered; no duplicate request sent. Private safe helper and receipt
  are in `indeehub-yaya-private-staging-5d0ea64/`; no signature yet.


### Fullscreen prompt withdrawn

Operator replied “wrong window ignore” to the fullscreen-control question.
Reverted only that request's uncommitted player, test and optional browser-check
changes to the accepted native-audio source checkpoint. The temporary fullscreen
UI reached dev only; Yaya only received staged bytes, never activation. Dev is
being restored to the previously qualified companion-audio archive/index above.
Native audio/APK56 and the release work remain in scope. Do not resume this
withdrawn request or the earlier withdrawn screensaver/Play/Skip requests.

- Withdrawal completed:dev again serves qualified index
  `4d9424a4ad7548221a07e2c4f14019029ac5fc4e3f1d27ff5eca72b5ff0f6a56`;
  backend/catalog/session/intent/container preservation and390/1440 dashboard
  checks passed. All770 UI/catalog source hashes again match the qualified
  native-audio baseline. Yaya never activated the withdrawn fullscreen build.

### Task16 complete — actual Web5 footer acceptance

Monitoring, Identities and Federation footer placement passed on dev and Yaya
with the deployed qualified UI at390px and1440px. Browser-only stress content
covered tall neighbours, shrinking content and long action labels; footer
geometry remained in normal flow, at the card bottom, with space above and no
horizontal overflow. Keyboard focus and Monitoring navigation also passed.
No server state, identity, wallet or relationship was changed.

Evidence (browser scenario deliberately outside the repository):
`~/.local/state/archipelago/release-qualification/web5-footer-20261007/`.
This closes task16; it does not close the separate Framework owner-browser
Monitoring gate or deferred connection UX/removal work.


## Companion56 physical background failure — 2026-10-07

- Operator reports audio stops about five seconds **after locking the phone or
  switching apps**, with no playback notification. Foreground playback works.
  This is a failed acceptance check, superseding the earlier nearly-finished
  status; automated browser and Robolectric passes do not close it.
- USB debugging is unavailable (operator corrected the initial can-connect
  response). Do not keep requesting USB or claim device logs were inspected.
- Actual delivered APK manifest includes CompanionAudioService, mediaPlayback
  foreground type and required foreground-service permissions. This verifies
  packaging only, not service startup on the phone. No root cause confirmed.
- Companion retains its dashboard WebView across navigation. Operator was asked
  to use the existing Restart app action, reconnect to Yaya, play a free demo,
  and check for the notification before backgrounding, to rule out a retained
  page from before the matching UI deployment. Operator subsequently confirmed
  it still failed after following the guide. Do not request another blind retry.
- No replacement APK, real payment, phone reset or wallet change performed.
- IndeeHub agent committed final dispatch fixes as 2bfa84ef: isolated backend
  suite 2,004 passed, zero failed, five ignored. VM-only binary preparation and
  isolated full update/recovery rehearsal are ongoing; no live activation.


## Independent workstreams — operator authorized 7 October

- IndeeHub agent owns seven-service update/recovery qualification. Its VM-only
  executable completed (SHA25614100aa55192b79ede0e9ec37317efd9125065f1336b0084d8f1b6ee37c59490);
  all seven baseline images imported. Actual RPC rehearsal is underway in an
  isolated VM. This executable predates the new paid-file fixes and is not a
  release artifact. Yaya remains unchanged.
- Companion agent owns no-USB native playback diagnostics and APK57. Code is
  ready, Android tests/build underway. This is diagnostic, not a claimed fix;
  physical background acceptance remains failed. See its dedicated investigation.
- Paid-file agent owns guard fixes18b08720: unresolved on-chain attempts block
  alternate Cashu/Lightning payment; unsafe legacy spending fails closed. Fresh
  Fedimint file purchases are temporarily unavailable; cached delivery/recovery
  remains.62 UI tests and typecheck pass; isolated backend suite is queued.
  Both nodes' previously purchased20MiB fixtures still reopen with exact hashes,
  with no new payment or spend-ledger change. See dedicated qualification doc.
- Root owns Fleet recovery65d265f2: explicit retained-report outage notice,
  Retry, rejection of old replies and unmount cleanup.21 tests, Vue typecheck
  and isolated390/1440 Chromium keyboard/layout recovery checks pass. Production
  combined UI build and deployed acceptance remain.
- Wrong-window PWA request was inspection only and was withdrawn; no PWA change.
- Heavy builds are serialized around the constrained host; no live wallet/app
  shutdown was used to free resources. No OTA/ISO/catalog publication occurred.


## Latest resumed checkpoint — APK57 and combined qualification

- Operator reports APK0.5.37 background playback now works; notification exists
  but song artwork is missing. Do not keep asking for the failed-flow diagnostic
  report or USB. Audio agent is correcting the admitted-app-origin artwork path;
  earlier physical failure remains historical, with no definitive root cause.
- Diagnostic APK source committed7383d47b. Download:
  http://192.168.63.240/packages/archipelago-companion-0.5.37.apk
  SHA2569b63bd928ab396278791ab3b514dbb53daaf4b7de9311e8e4f505acc02594a2c.
  33 Android tests, canonical clean packaging and existing signer passed.
- Combined UI:1,448 tests/176 files pass; production build and390/1440 Chromium
  Fleet outage/recovery fixture pass. Frozen701-input artifact/evidence resides
  in `~/.local/state/archipelago/release-qualification/combined-ui-20261007/`.
  Source subsequently reopened for artwork changes; they are not in that artifact.
- IndeeHub VM caught real manager-startup replacement of saved containers in
  generic environment-drift repair. Early saved-recipe guards and outer sweep
  exclusions are under combined isolated backend qualification. The seven-service
  fixture was restored/rebaselined with failed evidence retained; this is not
  rollback acceptance. Normal saved-unit Start/Stop/Restart also needs correction
  before activation. Lifecycle patch prepared separately during source freeze.
- Paid-file agent owns the one active Rust compile; it is progressing but page-I/O
  bound. IndeeHub owns next source/build slot. No live wallet/app teardown is used
  to free memory. Current guard tests do not qualify the pending lifecycle patch.
- Public sharing read-only assessment checkpoint is `/tmp/archy-public-sharing-plan.md`.
  Implementation remains queued. Public paid delivery still needs authenticated
  identity/transport independent of federation; do not bypass payment journals.
- No new OTA/ISO/catalog publication. Yaya UI/backend/IndeeHub remain unchanged
  during this checkpoint. Dev-only combined UI deployment is being qualified.


Dev combined UI attempt rolled back automatically when independent botfights
replacement changed the post-deployment inventory. HTTP verified the old index
hash matches the preflight baseline. The new artifact remains qualified but is
not deployed. Existing health-monitor restarts were confirmed read-only; retain
this new qualification issue rather than weakening the preservation gate.

Five completed historical UI release archives were moved from RAM-backed `/tmp`
to durable `release-qualification/retained-tmp-archives`, with exact SHA256 checks
and original paths retained as symlinks. This released496,544,104 tmpfs bytes;
no build inputs, running services or wallet files changed. Indee's owned VM is
being shut down at its transaction-idle boundary to relieve compilation pressure;
its disk/evidence are retained for a dedicated4GB qualification window later.


## Original1.9.0 ISO added to ngit release records

Operator correctly reported missing ngit software release (Git tag alone was
already mirrored). Published [1.9.0-alpha](https://gitworkshop.dev/nevent1qqsfy0xpwnn9y423x6vnxxqtfrplm0n3fyfz9fzcheru2a42gj368sqzyp6xqpkcw7daqn3jry69t99yfzdyy3h8gqv0zhjhte8azny4v6746qcyqqq82mc22hxen) with the original ISO and both
checksums; both relays acknowledged, online readback resolves exact three assets.
No new build, source ref, OTA/catalog or Gitea asset changed. Full local ISO hash
and pinned signature pass. Direct URL attempt hit1800-second timeout; retained
failure and used the established verified-asset-event workflow. Full receipts:
`~/.local/state/archipelago/release-qualification/ngit-iso-190-20261007/`.

Other latest checkpoints: artwork UI0c1d1b57 passes26 focused tests, app-project
typecheck and Chromium A/B; native test60bd728a passesSDK28/35. APK57 unchanged;
production UI build/physical artwork acceptance remain. Frozen707-input snapshot
and build script:/tmp/archy-artwork-ui-build.py. HTTPS task17 evidence51694119
retains22/24 full-navigation cases and12/12 response-only follow-up; normal trust
and LAN-IP certificate match fail. Indee final lifecycle/health guard b0b95810 is
under the combined isolated backend run, log
`/tmp/archy-paid-indee-lifecycle-backend-20261007.log`; VM is off. Before its next
boot, apply the documented GRUB manager mask and validate read-only exec context.


### Continuing agent ownership after ngit publication

- Paid-file agent owns final combined backend run and source-hash verification;
  latest log `/tmp/archy-paid-indee-lifecycle-backend-20261007.log` was still
  compiling, not a test pass. Indee final source is b0b95810; no new mutations.
- Companion agent now owns the frozen artwork UI production build, browser
  qualification and guarded dev delivery, then coordinated Yaya UI delivery.
  Script `/tmp/archy-artwork-ui-build.py`; snapshot under
  `release-qualification/artwork-ui-20261007`. Preserve rollback and container/
  backend/catalog/session/intent checks. APK57 remains unchanged. Root's older
  combined UI attempt rolled back; don't mistake it for a deployed artwork fix.
- Indee agent owns safe masked4GB VM context probes, corrected executable and
  actual seven-service update/rollback/normal lifecycle acceptance. VM is off.
- Root's confirmed-idle snapshot relocation helper PID1347627 is SIGSTOP-paused
  because it competed with the compiler. Exact command argument:
  `/tmp/archy-relocate-frozen-snapshots.py`; log
  `/tmp/archy-relocate-frozen-snapshots.log`; root exec session38007. Paid agent
  owns resuming it after the suite in a suitable no-compiler/VM-heavy window.
  Do not leave it paused indefinitely. Original paths remain until each full
  copy/hash/mode verification completes; then preserved as disk symlinks. The
  three sources are historical GitWorkshop, ngit-review190 and companion-PiP
  snapshots, not current build inputs. Never remove unverified originals.
- Exact operator HTTPS hostname requested asynchronously; no reply yet.


## Corrected maintenance scope and artwork delivery

- Combined isolated backend qualification completed: 2,006 passed, zero failed,
  five ignored, with all 532 input hashes stable. This qualifies b0b95810 plus
  the payment guards, not the subsequent maintenance launch change 01a55d2d.
- The actual hardened synthetic VM reproduced direct Podman exec failure (126)
  from the manager context. User-scope execution passed for API/PostgreSQL and
  preserved the inherited lifecycle flock. Commit 01a55d2d launches the pinned
  maintenance helper through that scope. The VM then shut down cleanly. Corrected
  executable and full transaction/rollback/lifecycle acceptance remain pending.
- Frozen artwork production UI passed with 707 unchanged source inputs. Browser
  checks at 390/1440px verified JPEG delivery using actual Yaya admission with
  synthetic cover data, without signing/payment. Dev UI deployment succeeded:
  index 92050cbda69954f57f4b0bdd73d89623f6aae9fc66e4c074ee4f46bdcb9c3b2a,
  178 retained-path checks and 32 unchanged container IDs/start times, including
  delayed verification. Live dev browser checks and Yaya delivery are next.
  The earlier failed/rolled-back UI attempt remains historical evidence.
- APK 0.5.37 is unchanged; operator-confirmed background playback remains accepted.
  Physical notification artwork and the full controls/recovery matrix remain open.
- Paid-file qualification scripts are retained under
  `release-qualification/paid-file-recovery-20261007/`: readiness/provenance audit,
  synthetic isolated runner and next-stage gate. No new or repeat payment occurred.
- Paid-file agent resumed the verified historical snapshot relocation helper
  PID1347627 after VM shutdown; Indee's next compiler waits for this bounded slot.
  Do not restart a second copy process or remove unverified originals.
- Root corrected stale guide/backlog claims in 2ccc0381. Cloud PiP's reported
  operator flow is accepted; the broader physical edge matrix remains open.
- Indee app discovery/publication and timed-viewing source exist, but live
  two-node acceptance is pending. Producer wallet binding and rental payment
  method coverage are still substantive gaps; seven-service migration alone
  cannot close the complete IndeeHub task.


### Delivered artwork and selected IndeeHub demo

Artwork dashboard delivery is now complete on both dev and Yaya, with index
92050cbda69954f57f4b0bdd73d89623f6aae9fc66e4c074ee4f46bdcb9c3b2a.
Yaya preserved all 174 checked paths and 30 container IDs/start times, including
delayed verification; actual deployed browser checks pass at 390/1440px. Receipt
and investigation commit d23da226. APK57 unchanged; physical cover confirmation
remains open.

The operator selected `web54321-balanced-2.mp4` for IndeeHub. Authenticated Yaya
Cloud metadata confirmed an exact match, 203331877 bytes, without reading video
bytes or publishing it. Private source path/metadata receipt is retained in
`release-qualification/indeehub-selected-demo-20261007/source.json`. Preserve the
original. File-identification blocker is cleared; paid-test authorization is not.

Task11 baseline: 24 actual deployed Apps/Web5/Cloud route-shell checks passed on
dev/Yaya at 390/1440px, no page errors/overflow/mutation attempts. Single samples
under concurrent build load: cold document 485–2035ms; warm router 123–1115ms.
This is not useful-data, tap feedback, phone, distribution or before/after proof.
Retained initial transition-selector failure and corrected evidence under
`release-qualification/navigation-baseline-20261007/`.

Rental payment review found missing shared cross-rail admission at
content.rental-purchase. Reviewed patch c2b22c0a is isolated on
work/rental-admission-20261007, awaiting current executable input-hash capture
before integration and isolated execution. Public-sharing UI groundwork is
separately isolated on work/public-sharing-ui-groundwork; no public menu is
activated or legacy policy remapped.


### Optional historical-copy helper cancelled safely

The archival helper PID1347627 is no longer paused or queued. Root verified its
exact stopped command, all three original directories still intact as real
directories, no relocation rename staging and no completed relocation receipt,
then terminated only that helper. Do not resume/restart the PID (it may be reused).
Partial disk copy is retained with
`release-qualification/retained-tmp-snapshots/cancelled-helper.json`; no original
was deleted or replaced. This optional maintenance no longer delays builds/VM
acceptance. The five earlier successfully relocated UI archives remain intact.

The selected Yaya demo source hash is now independently verified read-only:
`766f5f6b9d2a6fd90cb7541447fbba666773635bd90def145bcd479a1b8b0591`,
203331877 bytes; before/after metadata matched. No video duplicate was retained,
no publication or payment occurred. Receipt: the selected-demo directory's
`source-hash.json`.


### Corrected executable and VM manager-start acceptance

The corrected normal/debug executable completed in 18m48s with all captured
inputs unchanged: SHA3cbe5a5c3a74e6463ab0874c74e23dfca68f0bfc3fe54883f0edf69abb37ac0d,
420030040 bytes. Stripped same-code VM copy:
753f8ba6ac342c8f4b9a19c8079a51dfd1da4dcb517d4ea4d0e54035c22f5788.
This is an integration artifact, not an optimized release, and predates rental
guard 4dde14bf. VM manager-start acceptance passed: completed 62-app reconcile
logged all seven IndeeHub members NoOp, all baseline IDs remained running, API
and manager returned HTTP200. Actual update refusal/rollback/lifecycle tests
are next; do not infer their results from startup acceptance.

The chosen video also passed authenticated Chromium metadata loading: 1920x1080,
249.766667 seconds. Source metadata acceptance is separate from rental playback.
Public-sharing UI groundwork 6bcdd106 (37 tests + app typecheck) and Mesh picker
groundwork 3360354a (10 tests) remain on isolated branches, not integrated or
menu-enabled. Their broader backend/transport/inheritance work remains open and
their end-of-backlog delivery order is retained. Companion agent is now handling
Fleet acceptance; paid-file agent owns the next combined isolated backend run
after the VM window. Optional archive helper was cancelled; never resume its PID.


### Operator artwork deferral supersedes pending confirmation

The user reports the notification image still does not appear after deployment,
and explicitly requests parking it until the end of all tasks. Retain physical
FAIL/open, background playback accepted, and stop further artwork investigation.
Backlog task23 records the deferred follow-up after all earlier tasks.

VM update requests so far hit lifecycle-lock admission before reviewed-plan
validation. They prove no mutation, not the missing-plan gate or rollback. Indee
agent is tracing actual lock errno/context; startup preservation is still valid.
Root/paid agent Fleet provenance fix is isolated in
`/tmp/archy-fleet-provenance-20261007`, companion agent Fleet alert/provenance UI
is also isolated. Neither is live or compiled into the VM artifact. Their final
review/integration must precede the next combined backend source freeze.
