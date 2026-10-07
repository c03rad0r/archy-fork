# Archipelago release work handover — 2026-10-07

This document is the resumable state of play for the post-1.9.0 work. The
candidate worktree is `/home/archipelago/Projects/archy-session-key` on branch
`work/post-190-session-key`. Do not use credentials from chat or print private
node, wallet, session or registry data.

## Current candidate

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
