# Archipelago 1.9.0-alpha release acceptance

Status: **OPEN — unpublished.** Operator requires the `-alpha` suffix: final version
`1.9.0-alpha`, tag `v1.9.0-alpha`, and matching OTA/ISO artifact names. Earlier
unsuffixed candidate evidence below is historical, not a final artifact pass.
Operator selected the 1.9.0 series instead of the provisional
1.8.23-alpha. This is the current summary; retain the detailed history and all
requirements in [the regression ledger](post-1.8.22-regressions-20261001.md) and
[the earlier acceptance record](release-1.8.23-acceptance.md). No unexecuted test
is a pass. Provide the operator a node-specific action/expected-result checklist
whenever human acceptance is needed.

## Current evidence

- Alpha backend: isolated suite **1,681 passed**, zero failed, four explicit
  ignores; separate container runtime suite **82 passed**. Optimized binary
  SHA256 `560aa6006cd9ef8be95b1f7831cf3b53854e911622b50022bb4402ce0f8b010a`
  is deployed on dev, yaya and Shorty with private rollback backups. Framework
  retains the preceding A5 candidate; its earlier acceptance remains recorded.
- Frontend: **1,222 passed across 150 files**; production build and real mobile/
  desktop media/menu checks pass. Dev serves index SHA256
  `c18b24024a78789fe65c74c5ce27efe2125ae869016ab65e33c5a2680b543f17`.
  Yaya now serves the same index and companion package; Framework retains the
  preceding qualified upload UI `67de835a…`.
- Companion **0.5.34/build54**: 12 native tests, clean build, v1/v2/v3 signatures
  and unchanged signer pass. Viewer and phone download are operator accepted.
  Dev APK SHA256 `ceb58a7dc5f1398fe84f30255ec9ed79834f5db5f8fbc03eac52e14186fab1a1`.
  Fleet publication remains part of the final release.
- NPM corrected gateway/client-IP integration: 23 Python checks and complete
  disposable real-image integration passed. Catalog generator now requires both
  migration-backup and legacy-gateway capabilities; its generator/drift selection
  regression passes. Candidate metadata drift zero and registry trust passed.
- Cuprate/NetBird/BTCPay grouping previously passed actual yaya desktop/mobile,
  hard reload and BTCPay category/icon checks. No product installs were performed.
- Real one-sat Lightning purchase, exact bytes, buyer ownership/cache and operator
  free reopen passed. Original tester recovery accepted separately.
- Transparent transaction rail, compact origin-screen upload bar/cancellation and
  cooperative-close controls have recorded desktop/mobile browser acceptance.
- Framework original LND startup incident is closed with operator acceptance.

## Open release gates

- [ ] **NPM:** corrected private signature, dev/yaya selection and yaya override
      retirement now PASS (2026-10-05). Remaining: full boot/OTA/ISO and
      full legacy-backend migration/rollback
      acceptance; retain the completed
      fresh/nested disposable, public staging issuance/forced renewal and actual
      yaya state-preservation checks.
- [ ] **Shorty NPM:** shop certificate12/Force SSL and manual-route migration pass. Final
      corrected backend restart, 302 continuous denial probes and the external
      32-case security matrix pass. Remaining: packaged boot/OTA acceptance.
      Preserve management containment and current public app routing.
- [ ] **Security:** verify final deployed/booted artifacts against public raw IP,
      unknown Host/SNI, forged forwarding headers, IPv4/IPv6, assets/RPC/WS;
      preserve private access, ACME issuance/renewal and public app TLS/WSS.
- [ ] **Fees/Bump:** deployed dev/yaya Fast UI and real isolated funded regtest
      (CPFP/RBF, fee history, restart, confirmation/reorg) pass. Authenticated
      Framework read-only quote/status acceptance remains. Preserve approved green UI.
      No production spending or channel closure is authorized by this checklist.
- [ ] **Paid files:** finish buyer restart/outage and updated-seller persistence
      acceptance; preserve atomic ownership and safe retries without repayment.
- [x] **Uploads:** real dev/yaya interrupted-network, offset recovery, lost
      replies, hashes, cancellation and compact origin-screen display pass.
      Physical companion background/reconnect and Framework Cloud flow are
      operator accepted. Final packaged-artifact checks remain below.
- [ ] **File Browser credentials:** unique managed login, default-password
      removal, account/file preservation and rollback pass real Podman fixtures
      including actual Quadlet restart. Dev/yaya/Framework migration and Cloud
      acceptance pass. Remaining: packaged OTA/ISO startup. Docker behavior is not inferred from Podman fixtures.
- [ ] **Apps:** complete upgrade inventory matrix for installed/stopped/removed/
      restarting/legacy aliases; Immich/retired-app removal and unexpected-service
      identification; Portainer/Gitea migration from actual request namespace.
- [x] **UI:** category-view clear-search control passes on served dev/yaya UI
      at390/1440px: click and Escape clear the field, retain focus and stay inside
      the existing field. `/tmp/archy-190-final-search-live.log`. Earlier grouping
      and transaction-rail results are retained.
- [ ] **Angor:** dev full-chain acceptance after unpruned Bitcoin sync; retain
      the operator-accepted historical discovery limitation (34 unrecovered announcements);
      recovery is follow-up work, not a publication blocker.
      Publish tested explorer/API app update and optional relay in signed catalog.
- [ ] **Post-release demo deployment:** operator requests updating the existing
      public software demo at https://demo.archipelago-foundation.org/ through its
      established Portainer/Gitea workflow after release. Inspect the exact
      stack/source, preserve rollback, verify served 1.9.0-alpha and demo flows.
      This is not the Yaya v4v website or a Portainer version upgrade.
- [ ] **Final artifacts:** finish versioned build; exact candidate deployment;
      OTA update/rollback and raw ISO boot/install; signature/checksum validation;
      publish Git/ngit, app images/catalog and artifacts; verify public downloads
      and fleet discovery; remove temporary catalog selectors after publication;
      supply LAN SCP command for the raw ISO.

## Retained regression scope

Mempool version/update clearing/deduplication; Minibits and Cashu same-mint payment
handling; LND startup/Receive/unknown balances; Bitcoin warmup and IBD dashboards;
pruning and X250 kiosk picker; AIUI background; launch readiness/card geometry;
GitWorkshop; Gitea/Portainer; safe network diagnostics; operator uninstall/stop
choices; companion images and generated service configuration; radio payload and
UK MeshCore dev V3 acceptance; previously reviewed/merged PRs. Detailed original
requirements and evidence remain in the linked ledger, not silently dropped.

## Explicit boundaries

Framework V4 radio is now reported working by the operator (2026-10-05).
No reflash is requested; retain this as operator evidence, separate from automated
hardware coverage. Previously
accepted Primal comment and lost-response Cashu receipt follow-ups remain separate.
Only one Angor project has full public browser acceptance; 34 missing announcements
are not proven globally lost. Do not claim complete recovery from one fixture.

### Further live evidence

Framework's existing one-sat purchased-file ownership entry and exact 121-byte
cache remain present after the Bump management restart. Purchase timestamp
09:15:03 UTC precedes manager start 10:42:52 UTC on 2026-10-02. SHA256 remains
`d55f7a6acd77bdc3c35c65ecac6d1492096e99252d07d433e6286544540cde7e`.
This establishes buyer persisted bytes/ownership across that actual manager
restart. It does not establish a full-machine reboot or seller outage scenario.
No payment or restart was performed for this read-only check.

Yaya post-deployment checks pass: native Bitcoin/LND unchanged, private UI200,
marked public HTTP/HTTPS404, missing HTTPS challenge404, exact challenge bytes
written inside NPM over local HTTP/HTTPS and public HTTP, existing public site
trusted HTTPS/authentication preserved. This is not yet the new signed-catalog
migration without the temporary network override.

### Versioned build and final suites

The optimized 1.9.0 backend build passed; SHA256
`e1b94e6de9b5cc3dfcec16994bc3d0f710f3b7bcc6e5af045c6e53bb94b6abf0`.
The final frontend suite passed **1,187 tests in 146 files**, zero failed.
All 48 script unit tests passed, as did app build-context, manifest-shell,
ISO overlay, network-doctor, pruning and LND UI readiness checks. The release
harness now includes NPM bridge, guard, catalog capability and isolated actual
nginx security tests. All 120 public-network rejection cases passed again.

Yaya category search clearing passes desktop/mobile: click, Escape, focus and
contained icon, unchanged 40/52px field heights. Portainer's actual namespace
still reads Git smart HTTP refs and Compose from the expected branch; native
services and the production site were not changed by those probes.

Fresh Angor relay queries still recover only one of the 35 original signed
announcements. Eight relays returned results/EOSE; two archive endpoints were
unavailable. A release-scope decision was requested rather than silently waiving
this external-data requirement. The reference HTTP endpoint was readable through
Python, while the Node HTTP client received HTML; relay queries used the recorded
35 exact event IDs, and accepted only matching validly signed kind3030 events.

A new isolated runtime harness executes the production backend against actual
Bitcoin/LND regtest processes, with private process/network/filesystem namespaces,
normal account setup/login and disposable wallets. Its CPFP, child RBF, recipient,
fee-budget, duplicate-submit and backend-restart checks passed. Confirmation and reorg recovery also passed after the fixture announced a
competing empty block. The earlier disconnect-only fixture failed to notify the
expected new chain state and is retained as a failed attempt. Full run evidence:
`/tmp/archy-fee-regtest-run3.log`. No production wallets or funds were used.

### Current deployment and final history correction

The versioned backend `e1b94e6de9b5cc3dfcec16994bc3d0f710f3b7bcc6e5af045c6e53bb94b6abf0`
and the production UI are deployed on dev and yaya. Manager health200, served
index byte match and unchanged unrelated container IDs/start times passed on
both. Private rollback directories are `support/190-versioned-20261002`.
Actual category search clearing passes desktop/mobile on both nodes.

A final source audit found fee-only child history grouping was still absent.
The correction is now implemented with conservative receipt/ownership/input/
fee-only/current-chain verification, replacement-aware totals, linked fee
history and current-child Bump targeting. Nine focused UI tests and the new
production dashboard build passed. This correction is NOT in the deployed
backend above. Its isolated backend suite and extended actual regtest acceptance
remain in progress. The concurrent optimized compile was deliberately stopped
to reduce build contention and must be restarted after isolated compilation.

Corrected NPM candidate remains unsigned. The operator was given the exact
private signing command and asked for the affected physical companion route.
No publication or release-scope waiver is inferred from silence.

### Payment audit follow-up

Found a separate older Cashu repeat-download fallback that allowed a new spend
when an ownership record existed but its cached bytes were missing; an unreadable
index was also treated as empty. The payment guard now reads ownership strictly
and returns a recovery error before mint/spend in either case. Successful cache
hits retain the zero-payment response and same-seller filename alias handling.
The new regression exercises first purchase, exact/alias cache hits, different
seller, missing bytes and damaged index preservation. Final suite/rebuild are
running; no live wallet was modified. Previously documented lost-response ecash
receipt limitations remain separate from this correction.

Framework read-only optional Files-copy verification could not proceed because
the SSH control connection expired and BatchMode login was rejected. Existing
buyer cache/restart evidence remains valid; no password or account was changed.

Actual served Fast-send controls pass on dev/yaya at390/1440px: initial Fast,
explicit Standard selection and reopen reset to Fast. No spending RPC submitted.
Two earlier harness attempts had ambiguous Close/Send locators during modal
transitions; the corrected final run passes all four cases. This is send-form
acceptance, not a real cooperative-close transaction or omitted-fee wallet spend.

Final isolated suite after fee-history and missing-cache payment corrections:
**1,668 passed, zero failed, four explicit hardware/external ignores**. The
optimized build and extended real-regtest run are chained in
`/tmp/archy-190-complete-validation.py`; log
`/tmp/archy-190-complete-validation.log`. They have not yet completed.
The packaged radio flasher self-test also passes (`archy-esptool4.8.1`,
ESP32-S3 stub ready); this does not change Framework radio deferral.

## Signed qualification completed — 2026-10-05

Operator confirmed signing; exact private catalog verifies against the pinned
release root. Final optimized backend SHA256
`cfddec834a53609f8bef924f3905da76df45f22426cac2628c4c2cb06bea5d09`
and final dashboard index SHA256
`4b8f6ceb4ebe1e3b8ce1a0786f3e8a9d38e6d42ba174bf64bd54f4b23d7c13ff`
are now deployed on dev and yaya. Both health checks, served byte matches and
unrelated container identity/start-time checks passed. Root-only rollback
directories: `support/190-final-20261005-20261002` (literal generated name).
Both cached catalogs exactly match the new signed candidate.

The optimized actual Bitcoin/LND regtest passed with the new history assertions:
CPFP, child replacement, unchanged recipient, bounded fee, duplicate submission,
one payment with replacement-aware fee history, backend restart, confirmation
and reorg. No production funds were spent. Log: `/tmp/archy-fee-regtest-run4.log`.

Yaya selected the managed legacy-compatible gateway from the signed variant.
The temporary `90-qualification-host-gateway.conf` was backed up and removed
only after inspecting the generated managed network. NPM restarted successfully.
Complete selected DB tables and certificate bytes match the private baseline;
loopback and existing tunnel publications remain intact, admin API is healthy,
and an actual NPM-namespace upstream request returns the expected authenticated
site response. A private managed migration archive exists.

A subsequent manager restart and120 seconds of repeated reconciliation retained
all container identities/start times and did not recreate the removed override.
Migration checks passed again. Logs: `/tmp/archy-190-npm-override-retirement.log`
and `/tmp/archy-190-npm-persistence.log`. This is not full-machine reboot evidence.

Post-migration public integration passes: exact challenge bytes from NPM on
local HTTP/HTTPS and public HTTP, missing HTTPS challenge404, private UI200,
public-marked management HTTP/HTTPS404, public application trusted TLS and
authentication retained. Portainer's actual namespace reads Git refs and Compose
at verified tip `3ae171d6b0c728665a860520fe393c0abb772798`. Native Bitcoin/LND
unchanged. Deployed Fast-default/reopen/slower-select browser checks pass on
both nodes at390/1440px, without submitting transactions.

Additional external IPv4 probes from Shorty passed24 raw-IP/unknown/forged-host
cases with forged forwarding headers and root/RPC/assets/WebSocket paths.
Important boundary: the public front gateway returns its static Default Site
for unknown HTTP roots, rejects assets/RPC/WS with400/404, and rejects unknown
TLS names during handshake. Those are not dashboard responses. The first
harness required404 everywhere and failed on that public Default Site; retained
logs record the corrected interpretation. These probes validate the deployed
public gateway path, not direct WAN access to the node nginx. External IPv6
remains unverified; prior isolated IPv4/IPv6 guard tests remain separate.
No Shorty nginx/NPM configuration was changed.

Remaining operator inputs: normal Shorty NPM shop SSL ownership correction
(existing admin login unavailable), affected physical companion upload route,
and the retained Angor34-announcement recovery/release-scope requirement.
OTA/catalog publication, final ISO build/boot and fleet discovery remain held.

Read-only dev chain check2026-10-05: unpruned Bitcoin at830743/970017, verification progress0.63846, IBD true, warnings empty. Full-chain Angor acceptance remains pending sync; no service or wallet change made.

## Operator checks accepted; upload UX amendment — 2026-10-05

Operator reports the requested human checks worked perfectly: Shorty shop SSL,
physical upload flow and Framework dashboard/purchased-file checks. This is
operator acceptance, not a claim of newly independent device testing. Read-only
Shorty verification confirms shop certificate_id12 and Force SSL enabled.
Remaining migration/artifact/security and Angor requirements still apply.

Operator supersedes the globally persistent upload-bar requirement: keep the
bar only on the screen where the batch originated, continue transfers across
navigation, show explicit Complete on successful server save, and use a
completion notification elsewhere. Source now retains the originating route,
removes the global floating bar, keeps the original44px inline bar, and reports
success/error/cancellation distinctly.25 focused store/component/notification
tests pass. The subsequent full frontend suite passed1,193 tests; production
build and actual served desktop/mobile real-upload checks passed. Deployed to
dev/yaya with index SHA256
`86bb728017b118d8e98f032419e7fbfd6ecd78b7e464c982a2075cc38c582814`;
no apps or backend services restarted. Retain this as the preceding UI evidence,
not evidence for the later resumable-upload implementation. No new payment was requested or performed.

### Resumable upload addition — 2026-10-05

Operator requests recovery after a background pause or connection loss. The
installed File Browser identifies as2.63.23/e8a388f8 and supports TUS. Cloud now
has a candidate chunked upload implementation: random same-folder staging path,
server-offset reconciliation, transient retry/online/visibility recovery,
cancellation, final SHA256 verification and rename. Lost final chunk/rename
responses are reconciled without restarting or accepting a same-size old file.
The original-screen-only44px bar, Complete label and off-screen notification
remain. Fifteen focused protocol tests pass; full build/deployed fault injection
are in progress. This is not yet live acceptance.

Recovery requires the selected File to remain available in the running page.
An OS-killed app or expired server upload session may require reselecting the
file. Do not promise uninterrupted background execution or restart persistence.
This gate is additional to the already accepted physical upload flow.

## ngit PR integration — 2026-10-05

Both requested proposals are merged and pushed to Gitea and ngit main at
`2c1bcacf`; ngit independently reports both as `applied`.

- `494d2483`: opt-in NODE_IDENTITY_PUBKEYS for app owner allow-lists. Review
  corrected ECMAScript/Rust whitespace differences and added strict public-key
  validation. Appliance identity excluded; no private keys or signing capability
  given to apps. Existing manifests and the native signing flow are unchanged.
  Documentation explicitly describes linking all offered user identities.
- `c18ebd7f`: nostr0.44.7 and nostr-relay-pool0.44.3. The standalone relay pool's
  maintenance advisory remains; SDK0.45 migration is a separate follow-up.
- Combined isolated backend suite:1,678 passed, zero failed,4 explicit ignores.
  Real loopback hostile-relay test rejects altered content, author and signature
  reusing a known DB event ID while accepting a valid event. NIP04/NIP44 normal
  encryption and hostile/oversized payload tests pass. The initial relay harness
  returned before connection establishment; corrected to wait for an actual
  connection before fetch, and the complete rerun passes.
- Evidence: /tmp/archy-190-ngit-complete-tests.log, origin/ngit push logs and
  /tmp/archy-190-ngit-postmerge.json. This is source publication, not OTA/ISO or
  catalog publication. Later File Browser credential changes need a new suite.

## File Browser secure automatic login — NEW REQUIRED GATE

Operator requests unique per-node credentials, working Cloud from first launch,
no admin/admin and fleet-wide testing. Framework's reported authentication issue
recovered, which is not proof that this gate is fixed. Yaya rejects the saved
password with403 despite healthy File Browser2.63.23. Never count that as a
passed upload test.

Confirmed source issues: first-boot paths still try noauth/admin defaults; the
post-install hook assumes admin/admin and uses an incompatible password-change
request shape; the generated ISO path updates a running DB and uses a different
DB filename; Cloud hardcodes admin and invents admin/admin on missing secrets.

Candidate scripts/filebrowser-credentials.py now provisions a random username
and256-bit password offline with the pinned app image, backs up the selected
DB/config, preserves custom accounts, tests automatic login plus folder access
in a network-isolated container, rejects unauthenticated access, rotates only a
proven admin/admin login, and atomically publishes a0600 credential record.
Fresh real-image acceptance passes. Legacy/default/custom/restart/rollback,
first-boot/Quadlet/runtime wiring, live yaya/dev/Framework qualification and final
artifacts remain OPEN. No live File Browser account or DB has been modified.

Upload resume: source/build/full frontend1,208 tests passed; subsequent48 focused
protocol/client tests passed after filename escaping correction. UI deployed on
dev/yaya index SHA256
`31ac7bcc704c18f88a8b9800fb46bc7941651983e1a99b97d036a8d9e95a58b5`.
Actual dev1440/390px real-server fault injection passed partial offset123456,
offline reconnect, lost final PATCH and rename replies, exactSHA256, encoded
filenames, original-screen-only44px bar, notification, cancel and empty files.
Yaya is blocked at the credential gate above. Physical suspended/killed-app
acceptance is not inferred from these viewport tests.

## 2026-10-05 resumed release qualification

- Latest full frontend: 1,210 tests passed across 148 files. Production Cloud UI
  and AIUI builds passed. Dev and yaya serve index SHA256
  `3e10a25db75e4712310eb34c98bf7595ad5db3a444f9126a73715b1d40493a33`;
  UI archive SHA256 `586d1864c5c4027086194f6b5951a9b770c4e7ce9ba9ec3128e2ac0c1bde55e7`.
  Private UI backups: `/var/lib/archipelago/support/cloud-auth-20261005`.
- Real dev browser upload tests pass at 1440/390px, including interrupted JWT
  refresh, partial write, offline recovery, lost final PATCH/rename responses,
  exact SHA256, encoded filenames, cancellation, empty file, origin-only 44px
  bar and completion notification. Initial run overlapped UI deployment and
  failed navigation/bar timing; kept as failed evidence. Clean rerun explicitly
  verifies successful navigation and passes both viewports. Physical OS suspension
  and yaya authentication/upload acceptance remain separate gates.
- Real File Browser image matrix passed fresh, legacy-default, legacy-custom,
  legacy-noauth and forced-failure exact DB rollback. Existing file bytes and
  user IDs/permissions preserved; custom credentials preserved; admin/admin and
  anonymous access rejected. Actual disposable Quadlet pre-start and restart
  also pass, with stable managed credentials. Four Python unit tests pass.
- File Browser startup integration now covers the direct runtime, Quadlet,
  first boot and ISO script. Binary bootstrap installs its matching helper before
  reconciliation. Fixed bundled first-boot missing NET_BIND_SERVICE and duplicate
  creation attempt for a stopped File Browser. Live credential migration is still
  pending the optimized backend build; no production DB/account modified yet.
- Final combined isolated backend suite: 1,681 passed, zero failed, four ignored.
  An earlier run failed the Nostr relay fixture after a normal ping closed its
  text-only receive loop. Fixed the fixture to answer pings; the complete rerun
  passes. No failed run is counted as acceptance.
- NPM: 23 Python tests pass, including exact emergency BTCPay route recognition,
  operator edit preservation, missing certificate/alias refusal and transactional
  rollback. Existing emergency Angor routes now also require complete TLS
  replacements before retirement. Actual disposable flat-layout NPM integration
  passed namespace reachability, legacy gateway, ACME exact bytes, forced HTTPS,
  WSS, certificate replacement, password/network ACLs and forged-header rejection,
  restart, disable/delete, and forced bind-failure restoration. This is not a
  staging-CA issuance/renewal or ISO/reboot pass.
- Shorty read-only inspection confirms shop certificate12 and Force SSL with both
  hostname aliases; old manual shop route still uses certificate10. No live
  Shorty routing change in this qualification. Migration remains pending.
- Evidence logs: `/tmp/archy-190-final-combined-backend.log`,
  `/tmp/archy-190-cloud-auth-ui-dev-live-2.log`,
  `/tmp/archy-190-filebrowser-final-integration.log`,
  `/tmp/archy-190-filebrowser-quadlet.log`,
  `/tmp/archy-190-npm-final-integration.log`.
- OTA/catalog/raw ISO publication remains held. Framework radio deferred;
  Angor 34 unrecovered original announcements and dev full-chain acceptance
  remain open. README alpha/funds notice is separately published to both remotes.

Additional qualification: real nested-layout NPM integration passed the same
namespace/ACME/TLS/WSS/access-control/restart/rollback matrix as flat layout
(`/tmp/archy-190-npm-final-nested-integration.log`). Container crate isolated
suite: 82 passed, zero failed. Corrected Nostr hostile-relay test passed a separate
isolated repeat (`/tmp/archy-190-nostr-relay-repeat.log`). Dev Bitcoin read-only
status: height832232 of970036, verification0.641006, IBDtrue, prunedfalse. Full-chain
Angor acceptance therefore remains blocked on synchronization, not passed.

## Live File Browser ownership regression — publication hold

2026-10-05 dev candidate backend SHA256
`3e01da72fcea0a61852f3d9038e67630e328c65d6433da749671d60b91c37ffa`
built successfully, then failed live credential migration before DB mutation.
The helper could not create its private backup under the legacy data-directory
owner (host UID100000). The original real-image fixtures aligned data ownership
to the image UID and therefore missed the shipped manifest's different mapping.
The managed File Browser has DAC_OVERRIDE for that layout; the helper did not.

Restored prior backend SHA256
`cfddec834a53609f8bef924f3905da76df45f22426cac2628c4c2cb06bea5d09`
and original File Browser Quadlet with the staged rollback script. Both services
are active. Yaya backend was not changed. No candidate credential record was
published; failed setup stopped at backup-directory creation before DB changes.

Source helper now includes the managed server's DAC_OVERRIDE storage capability;
new real-image legacy-owner and actual-Quadlet fixtures reproduce that mapping.
Rollback fixture now forces an account-policy failure after noauth migration so
it still verifies restoration after a real DB mutation. These revised tests and
combined backend validation are in progress. A corrected embedded-helper build
and new live qualification remain required. Do not reuse the failed binary as
final release or mark the credential gate passed from earlier fixture results.

Release-note drift corrected to1.9.0/current upload behavior and File Browser/
Nostr additions. The checker now rejects stale descriptions/dates for an existing
version; its regression passes. Latest notes UI built and deployed dev/yaya index
SHA256 `97aab07e67eccc1bb3d215534b537b83e1372bf5c36b505b6127a24a2b629e23`.
Phone background/reconnect acceptance question is pending, not passed.

## Corrected credential qualification and mirror policy — 2026-10-05

The DAC_OVERRIDE correction passed all six real-image cases, including legacy
manifest ownership and restoration after an actual DB mutation. Actual disposable
Quadlet first start/restart passed with legacy ownership. Corrected helper SHA256
`e9e2fd94534130f10f19f81ebe0d4e382dca4338118393c7d1e30535a8478eb5`
also migrated the dev node's actual File Browser storage successfully: managed
login/folder200, private credential record, unchanged unrelated containers, and
manager/File Browser restored active. The old backend remains deployed pending
the corrected optimized build. Yaya credential/backend acceptance remains open.

Evidence: `/tmp/archy-190-filebrowser-ownership-integration.log`,
`/tmp/archy-190-filebrowser-ownership-quadlet.log`,
`/tmp/archy-190-filebrowser-ownership-live-dev.log`,
`/tmp/archy-190-filebrowser-ownership-dev-cloud.log`.
Updated isolated backend suite: 1,681 passed, zero failed, four ignored
(`/tmp/archy-190-filebrowser-ownership-backend.log`).
Browser protocol recovery also passes explicit CDP frozen-page/offline/reconnect
at both widths (`/tmp/archy-190-cloud-frozen-dev.log`); physical phone acceptance
is still pending and is not inferred from browser automation.

Operator selected ngit as the canonical contribution/review platform; Gitea
mirrors accepted main and release tag objects without requiring duplicate PRs.
Rule, contributor docs and read-only parity gate are committed as `138a541d`,
pushed to both mirrors and main/local parity verified. Disposable bare-repository
regression covers missing refs, partial pushes, divergence, annotation drift,
unpublished local commits, intentionally separate branches and inaccessible
remotes. Final release gate must additionally check the actual release tag.

## Alpha candidate: live Cloud and ACME qualification — 2026-10-05

Operator requires final version **1.9.0-alpha** and tag **v1.9.0-alpha**. Cargo,
frontend package/lock, changelog and What's New now agree; the unused unsuffixed
What's New block was removed. The optimized alpha build is in progress. Current
backend qualification SHA256 `e218e40f5c16c3d0cc4dc06c0a378c14b56b9087ded5c515b8a48351ceea3bcf`
is deployed on dev and yaya but predates this suffix change; it is not the final
artifact. Both returned backend health200 and managed Cloud login/folder200 with
unrelated container IDs/start times unchanged.

A real upload rerun initially failed after concurrent token refresh. A new unit
regression reproduced the race: mutable shared failure state let another login
turn a network interruption into a credential rejection. Authentication now
shares an in-flight request and returns its own retryability result. Regression
failed before and passes after. Full frontend: **1,211 passed / 148 files**.
Full alpha backend isolated suite: **1,681 passed, zero failed, four ignored**.
Deployed alpha UI index SHA256 on dev/yaya:
`67de835a25db59a483314eff583b809c3468c8529080bfa74c9962e44a6f54f9`.
Both real browser upload suites pass 390/1440px including partial writes, frozen
page/offline return, interrupted refresh, lost final replies, exact saved hash,
encoded filenames, origin-only bar, completion notification and cancellation.

Framework access was restored with the supplied updated SSH credential. Its
LND reports chain/graph sync; balance and channel queries work. Confirmed the
legacy File Browser still accepted admin/admin, then applied the exact qualified
helper with a private backup and bounded File Browser/manager stop-start. Both
managed and compatibility logins/folder reads200; admin/admin403; credential mode
0600; all other container IDs/start times unchanged. Prior backend retained until
final alpha deployment. Dashboard RPC session needs second-factor login; no
wallet funds were spent. Operator now reports Framework radio working; no reflash.

Local Pebble ACME **fresh and legacy nested layouts passed** actual pre-host
issuance, HTTP challenges, forced-HTTPS renewal, new certificate served, unknown
management404, trusted WSS, access controls, restart and forced-bind rollback.
Fixture fixes: modern NPM meta schema; explicit slirp loopback CA route; disable
random test-CA nonce rejection for deterministic route/renewal coverage. This is
an isolated test CA, not a public Let's Encrypt staging/ISO/reboot pass. All test
containers were cleaned up. Source NPM regression remains23/23.

Evidence: `/tmp/archy-190-alpha-backend-tests.log`,
`/tmp/archy-190-alpha-frontend-tests.log`,
`/tmp/archy-190-cloud-concurrent-login-before.log`,
`/tmp/archy-190-cloud-concurrent-login-after.log`,
`/tmp/archy-190-alpha-cloud-dev.log`, `/tmp/archy-190-alpha-cloud-yaya.log`,
`/tmp/archy-190-framework-secure-cloud.log`,
`/tmp/archy-190-framework-cloud-compatibility.log`,
`/tmp/archy-190-npm-acme-flat-6.log`, `/tmp/archy-190-npm-acme-nested.log`.

Public demo target clarified: https://demo.archipelago-foundation.org/, currently
reported1.8.8. Existing Docker Compose demo deployment located read-only; do not
confuse it with Yaya's v4v stack. Update after release, preserving rollback and
qualifying mock backend compatibility with new Cloud uploads. No demo deployed yet.
No OTA/catalog/ISO has been published.

## 2026-10-05 additional mobile media and file-action qualification

Operator accepted both physical phone upload recovery and Framework Cloud
folder/upload/open checks. These manual gates are closed.

A subsequent Cloud screenshot and touch-action report introduced new release
requirements: safe-area-aware photo/video viewing, separated touch controls,
fullscreen/exit, permanent translucent file-card actions, and the same actions
inside the viewer. Source and component tests are in the regression ledger.
The complete updated frontend suite passes: **1,222 tests in 150 files**.
Production frontend build passes. Chromium checks cover phone portrait,
landscape and desktop geometry, native fullscreen, action-menu access while
fullscreen, video decoding/playback, no accidental preview from a menu tap,
and cancellation before deletion. The deployed dev box reads the operator's
actual screenshot and 4K video without changing either file.

The new Android fullscreen callback implementation compiles and its three
Robolectric lifecycle tests pass with zero failures/errors. It still requires a
clean APK, signature verification and physical companion acceptance; compilation
and browser fullscreen do not establish that acceptance. Version 0.5.33/build53
is reserved for the companion update. No native fullscreen APK published yet.

Final OTA/frontend/ISO checksums and source attribution must be regenerated after
these additions. Candidate ISO build215 is superseded for publication purposes.
Previously recorded NPM fleet migration, exact signed OTA/rollback and ISO install
qualification, full-chain/Angor scope, mirror parity and public-demo requirements
remain open unless separately closed by direct evidence. No prior publication
hold is waived by the mobile test results.

### Companion download qualification update

Operator accepted the viewer but reported companion-only download failure.
Browser download of the same file matches its exact bytes. Added native saving
through Android's system file picker with authentication, streamed progress,
cancellation and error cleanup. The clean download/fullscreen suite passes all
12cases. Canonical clean companion0.5.34/build54 packaging passes v1/v2/v3 and
existing-signer verification. Dev serving is verified byte-for-byte with SHA256
`ceb58a7dc5f1398fe84f30255ec9ed79834f5db5f8fbc03eac52e14186fab1a1`.
The operator reports “works, we can proceed” after the phone save/open/cancel
check. This physical companion download gate is **accepted (2026-10-05)**. The APK is staged for fleet OTA/ISO and official/demo downloads;
only the dev-box test download is updated now. No fleet/public release is claimed.

### Demo upload compatibility — 2026-10-05

The prior demo backend lacked TUS, returned folder metadata for completed files,
and only accepted JSON-body renames. Added the actual Cloud upload protocol with
per-visitor sessions, reserved-byte quota, committed offsets, checksum metadata,
query-based rename and partial cancellation. Literal encoded filenames survive
unchanged. Deleting seeded files cannot subtract uncharged bytes from the quota;
deleting a folder releases its pending upload reservations.

Four real HTTP scenarios run the production `resumableUpload` client against an
isolated demo process and pass: lost chunk/save responses with exact5MiB+123bytes,
visitor isolation, zero-byte/replacement/cancellation, simultaneous quota, stale
offsets/oversized chunks and a real interrupted TCP request. The first run's10s
cold-start limit failed before startup; the bounded60s run passes. Evidence:
`/tmp/archy-190-demo-upload-final-2.log`.

The existing mesh/federation parity harness initially discarded demo cookies,
creating a new visitor on every request. It now retains its session and always
runs demo-only on loopback, never against the live container runtime. It also
found two newly missing radio configuration handlers. Demo now explicitly reports
hardware unavailable and refuses to claim an applied radio configuration. The
full parity run passes, including those assertions:
`/tmp/archy-190-demo-rpc-parity-final-2.log`. Both checks are release harness stages.
These changes are locally qualified; the public demo remains unchanged pending
final release, fresh AIUI packaging and deployed browser acceptance.

### Yaya accepted mobile candidate deployment

The same dev UI index `c18b2402…` and signed companion0.5.34/build54 APK
`ceb58a7d…` are now served on Yaya. All75 changed files match their reviewed
source hashes, including HTTP-served index/APK/metadata. Container identities
and start times are unchanged; the qualified catalog and AIUI are preserved.
Private rollback backup: `/var/lib/archipelago/support/190-mobile-20261005`.
Evidence: `/tmp/archy-190-yaya-mobile-deploy.log`. Phone acceptance was on the dev
APK; this byte-identity deployment check is not another physical-phone test.

Source review proposal: [ngit5957be8c](https://gitworkshop.dev/nevent1qqs9j4a73jyu6xfrpzrqcx2tqkldnuc8wzfas2zdkq6s2qlvftdaakqpz3mhxue69uhhyetvv9ujumn8d96zuer9wcq8s3xt),
covering daac47ca,5aa74d05,b8266c28,ba8b1f29. Proposal publication succeeded;
remote main refs and release tags have not been advanced. Do not call it merged
or the mirrors synchronized from proposal upload alone.

Private final NPM catalog candidate is ready for the operator's signature.
Compared with the previously signed candidate, only NPM's variant version2.14.0,
immutable image digest and the catalog timestamp changed.64 apps/63 manifests,
zero drift, registry trust and the pinned-image integration pass. This signature
is for migration qualification, not full-release acceptance or publication.
The operator was separately asked to resolve Angor's outstanding discovery scope.

Yaya post-deployment browser checks pass at390/1440px for grouping, icons, hard
refresh and BTCPay Commerce-only placement. The first harness session had an
expired login cookie and used catalog fallback; after normal login, the signed
catalog endpoint returns200/64apps and the complete rerun passes without401.
Evidence: `/tmp/archy-190-yaya-final-ui-smoke-authenticated.log`.

## Shorty live qualification: cached-runtime guard regression — 2026-10-05

The operator signed the final NPM candidate. Release-root verification and exact
reviewed payload comparison pass; signed SHA256
`479f6193835a16dd4ab167e5c22306a878ac39b77c2e2793971e807874fbc0cb`.
This signature authorizes private qualification; it is not release publication.

Shorty baseline public shop/www/indexer/relay trusted HTTPS passes. Its prior
NPM image bytes match the pinned2.14.0 image. Consistent stopped-NPM state,
backend, unit, nginx, helpers and app metadata were backed up under
`/var/lib/archipelago/support/190-npm-20261005`. Migration reached the new private
network/listeners but failed the guard acceptance check and was rolled back.
The test initially expected the emergency guard's legacy variable; further
inspection found a real source defect, not merely that assertion mismatch.

Confirmed cause: `ensure_runtime_assets_ready` applies the management guard,
then `run_runtime_assets` installs the cached OTA's nginx template verbatim.
Shorty's cached template predates the guard. It overwrote protection before a
subsequent nginx reload; restoring the older backend repeated that path. A live
public IPv4 root probe returned200. Immediate containment applied the tested
source guard; HTTP/HTTPS root and HTTP RPC again return404. The cached legacy
runtime template is now also guarded, with its original saved privately, so
that old startup installer cannot remove protection on restart. Both emergency
and current guards are present in the active configuration. Do not claim this
attempt passed or that the broader migration is complete.

Source fix: runtime installation now renders/validates the guarded candidate
before atomic replacement under the nginx transaction lock; syntax/reload
failure restores the previous protected bytes. Rollback protects the restored
runtime template before permitting an older binary to start.11 focused tests
pass; real isolated nginx verifies the actual legacy install, old-binary copy,
invalid-template rollback, public IPv4/IPv6 denial, ACME/private access and the
existing120-case Host/SNI/forwarded-header/UI/assets/RPC/WS matrix.
The first backend suite passed1,681/0failed/4ignored before the final rollback
addition; final rerun and optimized build are required. Logs:
`/tmp/archy-190-guard-runtime-final-unit.log`,
`/tmp/archy-190-guard-runtime-final-network.log`,
`/tmp/archy-190-shorty-activation.log` (failed attempt),
`/tmp/archy-190-shorty-prepare.log`.

Only NPM's container restarted; Bitcoin, LND, ElectrumX, Angor indexer and relay
IDs/start times are unchanged. Restored shop/www/indexer/relay HTTPS returns200.
Shorty's old backend remains active under containment. Rebuild and requalify the
migration, external security and restart persistence before closing this gate.
The signed catalog contents are unchanged and need no further operator signature.

### NPM multi-domain TLS regression found by public acceptance

After the private-listener migration, local first requests passed, but public
Angor health intermittently returned502. Host nginx recorded upstream certificate
hostname mismatches when different public domains used the same NPM TLS listener.
The generated bridge inherited upstream TLS session reuse. This matches nginx's
[documented cross-SNI session-cache behaviour](https://trac.nginx.org/nginx/ticket/1340).

The bridge now explicitly sets `proxy_ssl_session_reuse off` while retaining
SNI, hostname/chain verification and the existing trusted certificates. A real
NPM fixture with two distinct certificates reproduces failure with the old
configuration on the second hostname; the fixed fixture passes40 alternating
trusted TLS requests plus ACLs, WSS, certificate replacement, restart, failed-bind
rollback and disable/delete propagation.24 Python NPM regressions pass.
`/tmp/archy-190-npm-multicert-before.log` is the expected failing reproduction;
`/tmp/archy-190-npm-multicert-integration.log` is the fixed flat-layout pass.
Nested-layout issuance/renewal qualification is running separately.

The exact helper correction is temporarily installed on Shorty and transactional
sync succeeds.40 mixed local TLS requests across four hostnames pass. Public
read-only Angor browser acceptance now passes TLS, WSS, funding/event commitment,
Explore discovery of the known fixture and full project details/statistics:
`/tmp/archy-190-shorty-migrated-angor-browser-2.log`. This remains one known fixture,
not all35-project recovery.32 external IPv4 management-denial checks and tailnet
access pass;10 HTTP/HTTPS ACME routes return the exact probe written in NPM's data
mount. Six NPM database tables and42 certificate/renewal files exactly match the
pre-migration backup (`/tmp/archy-190-shorty-state-preservation.log`).

The previously running optimized build was stopped because it predates this
embedded-helper correction. A complete new build/deployment remains mandatory.
Shorty currently has the prior A5 candidate backend plus protected runtime nginx
and this qualified helper; an A5 restart can reinstall its older helper. Do not
claim final persistence until the new binary is deployed and restart is retested.
No certificate verification was disabled for a public application or upstream.
Raw-IP/unknown-SNI negative routing probes alone bypass hostname matching.

### NPM final source qualification and demo packaging

Backend source e0b2181a: all1,681 isolated tests pass (zero failures, four
explicit ignores). Corrected nested-layout NPM integration also passes real
local ACME issuance/renewal,40 cross-certificate TLS requests, WSS, ACLs,
restart, failed-bind rollback and host enable/delete propagation. Real public
Let’s Encrypt staging issuance plus forced renewal pass on migrated Shorty;
production certificate files and NPM container identity/start time are unchanged.
Evidence: `/tmp/archy-190-npm-guard-final-backend-tests.log`,
`/tmp/archy-190-npm-multicert-nested-acme.log`,
`/tmp/archy-190-shorty-migrated-staging-acme.log`.
Optimized build and final deployment/restart acceptance are still pending.

Demo image preparation found the web Dockerfile copied historical prebuilt AIUI.
It now builds the current source with the canonical script and frozen lockfile,
with explicit source revision for archive/container builds. Both CI workflows
track AIUI changes and pass the checkout revision. Actual image qualification
and public demo deployment remain pending. The demo/release VPS currently has
under1GiB free disk; capacity must be resolved before image/artifact publication.
No running container, volume, release or repository was deleted.

### Authorized release-storage cleanup — 2026-10-05

Operator approved removing only the old v1.8.13-alpha and v1.8.15-alpha ISO
attachments, then clarified that broader retention changes should be dropped if
that suffices. Both local ISO archives were independently hashed against their
published checksum files before removal. Gitea API deletion removed attachment
IDs219 and226 only; all other assets in both releases were verified unchanged.
Public server free space increased from887MiB to5.9GiB. Remaining historical
releases, OTA files, registry packages and application data were preserved.
Gitea's prior read-only storage doctor found no orphaned archives, attachments or
package blobs. No storage-doctor fix or package garbage collection was run.
Further removals are not planned; check exact final upload/image sizes first.

### Corrected binary deployed and restart verified

Final security backend SHA256
`560aa6006cd9ef8be95b1f7831cf3b53854e911622b50022bb4402ce0f8b010a`
built from e0b2181a after the complete1,681-test isolated pass. Deployed dev,
yaya and Shorty: health200, both embedded helper files match reviewed source,
active HTTP/HTTPS defaults are guarded, and all existing container IDs/start
times are unchanged. Backup per node: `support/190-guard-560aa6006cd9`.
The first dev check incorrectly inspected sites-available rather than the actual
regular sites-enabled file; automatic backend rollback ran. Corrected check
uses the helper's active-dashboard resolution, and the second deployment passes.
This was a qualification-script path error; retain the first failed log.

Shorty's final backend manager restart passed302 continuous public HTTP/HTTPS
RPC denial probes, followed by healthy management. Existing public32-case and
read-only Angor acceptance are rerunning against this exact deployed binary.
Logs: `/tmp/archy-190-guard-dev-deploy-2.log`,
`/tmp/archy-190-guard-yaya-deploy.log`, `/tmp/archy-190-guard-shorty-deploy.log`,
`/tmp/archy-190-final-shorty-restart-security.log`.

ISO release packaging now explicitly selects the qualified dashboard/AIUI
payload rather than capturing a live node's cached runtime files or choosing
AIUI by timestamp. Three executable builder-branch fixtures pass qualified,
missing and partial payloads; missing inputs fail without a live-node fallback.
The release wrapper sets this path and the release harness includes the test.
Accepted companion54 APK/metadata replace the stale copies in the packaging
staging directory; exact SHA remains ceb58a7d…fab1a1. Final ISO and OTA package
acceptance/signatures remain pending.

### Final demo images and exact-node follow-up

Shorty final backend restart follow-up passes the external32-case management
denial matrix and trusted public TLS/WSS/official Angor browser fixture. Evidence:
`/tmp/archy-190-final-shorty-public-security-after-restart.log` and
`/tmp/archy-190-final-shorty-angor-browser.log`. This is the known recovered
project, not all35-project discovery. Signed catalog479f6193 is privately active
on dev, yaya and Shorty; no public catalog publication has occurred.

Built demo images pass isolated real-image qualification: optional upstream DNS
failure returns502 for that service while the main demo remains200; fresh AIUI
provenance, backend healthcheck, four upload protocol/recovery cases, and normal
mobile intro/login/dashboard pass. The test uses a temporary container-only DNS
file; host DNS and the live public demo are untouched. Test:
`tests/lifecycle/demo-images.py`; evidence:
`/tmp/archy-190-demo-images-acceptance-final-2.log`. Earlier failure in the added
DNS test was an incorrect assumption about Podman's generated resolver, repaired
by explicitly mounting the disposable resolver fixture.

Qualified web image169e59da is built from157c9ec0; backend2722fa29 frome6e46a14.
Public demo deployment remains scheduled after release with rollback. RC2 raw
ISO assembly is running; no boot/install or final OTA pass is claimed yet.

### RC2 actual installation and first-boot retry correction

RC2 passed mounted payload checks and completed a full UEFI installation to an
80GiB disposable NVMe disk with encrypted data. Installed backend560, both
security helpers, dashboardc18, AIUI415 and APK54 match qualified bytes. After
boot from the installed disk, SSH, dashboard200 and backend health/version pass.
Actual installed nginx passes32 IPv4/IPv6 public-source denial requests including
unknown Host/SNI and forged forwarding headers (`/tmp/archy-190-vm-guard-test.log`).
The VM's EDAC hardware initialization service reports unsupported virtual
hardware; this is not claimed as physical ECC/EDAC acceptance.

Actual first boot exposed `log: command not found` in the unbundled setup: the
logger was declared below that path's early exit. Moving it before first use
restores diagnostics. Retry testing also demonstrated that unconditional
`podman system migrate` stops existing apps and races manager reconciliation.
The unbundled path changes no ID mappings and no longer migrates; bundled setup
only migrates when it actually adds mappings. Podman documents this stop behavior
in its [migration reference](https://docs.podman.io/en/latest/markdown/podman-system-migrate.1.html).

Corrected actual-VM retry preserves container IDs/start times and produces clean
logs: `/tmp/archy-190-vm-firstboot-logging-fix-2.log`. Executable isolated retry
regression passes twice with stored-secret preservation and fails against the
prior script. Added to release harness. RC2 must not be published: rebuild the
ISO with this script and qualify its boot/install path before signing. The OTA
backend/frontend payloads are unchanged by this installer-only correction.

Demo archive33ec4111…6fae8 matches after private server transfer; both tested image
IDs loaded successfully without changing running demo containers. Clearing only
unused build cache recovered1.743GB; no additional historical ISO, image, volume
or application data was deleted. Final publication capacity must be rechecked.

### Operator accepts Angor discovery limitation — 2026-10-05

The operator explicitly accepted releasing with incomplete historical project
discovery documented as a known limitation ("that's fine for now"). Funding
commitments for all35 reference projects were verified;34 original signed
announcements remain unrecovered from the sources checked. This releases the
all-project-discovery publication hold, not a claim that recovery passed. Track
recovery separately and retain the limitation in release notes. Other artifact,
upgrade, security and publication checks remain required.

### Work explicitly queued after this release

The operator requested returning to distributed IndeeHub, signer/companion login,
node peering, Framework Monitoring and external-app Cloud integration after
1.9.0-alpha. All details are retained in
[the post-release backlog](post-1.9.0-work-backlog.md). These additions do not
expand or delay the current release's artifact scope.

### Final RC3 installed-artifact qualification — 2026-10-05

Final raw ISO `archipelago-installer-1.9.0-alpha-unbundled-x86_64.iso` is byte-for-byte
the tested RC3 (SHA256 `aad5f0350428976969043079a63b0e7a8264dcee2574526bd34719c798c750af`).
Actual installation completed to a disposable80GiB disk with encrypted data.
Installed legacy BIOS/SATA and UEFI/SATA boot, mounted encrypted data, healthy
backend1.9.0-alpha with completed startup recovery, and private dashboard pass.
The initial SeaBIOS/NVMe boot could not find the disk; the same installed disk
boots with SATA. Installer logs show both GRUB installations succeeded. This
is not a claim of physical legacy-BIOS/NVMe support.

The final installed script retries without changing container IDs/start times
or managed File Browser credentials. Normal dashboard setup/login, Cloud token
and authenticated listing pass; anonymous and admin/admin access are rejected.
Initial test omitted CSRF and correctly received403; corrected normal-cookie/
CSRF flow passes. Onboarding also replaces the initial SSH password as expected.
The fixture was corrected to retain its own generated password privately; no
production credentials or access controls were changed.

After UEFI boot the installed nginx passes32 IPv4/IPv6 public-source rejection
cases including unknown Host/SNI, forged forwarding headers, assets, RPC and
WebSocket upgrade requests. Early health showed startup recovery still running;
subsequent explicit status/recovery assertions pass after61seconds.
Evidence: `/tmp/archy-190-rc3-installed-test-2.log`,
`/tmp/archy-190-rc3-uefi-acceptance-2.log`,
`/tmp/archy-190-rc3-uefi-healthy.log`. Unsupported virtual EDAC remains separate
from the supported service checks.

Artifacts are ready for operator signing, not publication. The final signed
OTA apply/rollback test necessarily follows signing. Mirror/tag parity, public
artifact downloads, catalog promotion, fleet discovery and demo deployment
remain required. The Angor historical limitation is explicitly accepted and
documented in [known limitations](release-1.9.0-known-limitations.md).

### Signed OTA qualification — 2026-10-05

Operator signed final OTA manifest and raw-ISO checksum document; both verify
against the pinned release root. Existing signed catalog also verifies. On the
disposable installed VM, the actual published1.8.22 backend and frontend were
verified against their published hashes, installed as the baseline, and used to
discover/download/apply the final signed1.9.0 artifacts through normal authenticated
RPC. Component verification, automatic manager restart, post-OTA verification,
exact backend/UI hashes, Cloud access and persistent credential/file checks pass.

A deliberately missing new frontend plus the real pending-verification marker
triggered automatic rollback: exact1.8.22 binary/UI restored, file and credentials
preserved.32 public-source IPv4/IPv6 management-denial requests still pass with
the restored old binary/template. Reapplying the signed1.9.0 OTA succeeds with the
same post-update assertions. Final whole-VM reboot qualification is running.
Logs: `/tmp/archy-190-vm-ota-cycle-2.log`,
`/tmp/archy-190-vm-ota-rollback.log`,
`/tmp/archy-190-vm-ota-rollback-security.log`,
`/tmp/archy-190-vm-ota-reapply.log`. Fixture setup corrections (missing systemd
drop-in directory and underscore in update_state.json) preceded the passing run;
no failed fixture run is counted as acceptance.

Publication storage required temporary upload headroom beyond the previous
cleanup. Under the operator's existing old-ISO retention authorization, removed
only1.8.18 ISO attachment235 after verifying its retained local copy against the
public signed checksum. All other assets unchanged;1.8.19/21/22 server ISOs remain.
Server free space is7.2GB before upload. Use an SSH-tunneled direct Gitea upload
so the public reverse proxy does not buffer an additional multi-GB copy.

Historical mirror audit identified three missing ngit tags1.8.16/17/18; their
local annotated tag objects exactly matched Gitea and were copied to ngit without
rewriting history. Unrelated proposal-only branch differences are inventoried
in `/tmp/archy-190-historical-mirror-audit.log`; full branch parity is not claimed.
Final main/tag parity remains a separate publication gate.

### Final reboot caught a stale OTA runtime script — corrected package

The whole-VM reboot itself reached healthy1.9.0, but the first-boot retry check
failed: the OTA runtime overlay still shipped the old first-boot script and
bootstrap installed it over the ISO's corrected version. Retry logged missing
`log` and changed running container IDs/start times. This is a real package
regression, not a passed check. The original signed frontend archive3047a19f is
obsolete and MUST NOT be published.

Repacked only `archipelago-runtime/scripts/first-boot-containers.sh` with the
already qualified source repair. Full archive comparison proves every other
entry, content and mode unchanged. Corrected frontend SHA256 is
`6d5135fa8e79b1bc84c8ed972770ebf99fe6611d819e5c1453f38f1593c26e66`.
ISO/backend/dashboard/AIUI/APK/catalog bytes are unchanged; ISO and catalog
signatures remain valid. Only the corrected OTA manifest needs renewed signing.

Added a release-manifest payload gate that rejects stale, absent or duplicate
first-boot scripts. Four archive fixture cases and existing executable first-boot
retry regression pass; the stale real archive fails, corrected real archive
passes. Actual backend bootstrap successfully promotes the corrected member on
the disposable node. Post-promotion retry/Cloud checks are in progress.
Logs: `/tmp/archy-190-stale-ota-reproduced.log`,
`/tmp/archy-190-repackage-runtime-2.log`,
`/tmp/archy-190-corrected-manifest-integrity.log`,
`/tmp/archy-190-vm-corrected-runtime.log`,
`/tmp/archy-190-vm-corrected-retry.log`. Renewed signature and exact signed apply
remain required. Keep earlier rollback evidence for the unchanged backend, but
do not claim the obsolete frontend is the final accepted artifact.

Corrected runtime post-promotion retry now PASS: container IDs/start times and
credentials preserved, Cloud token/listing work, default/anonymous access denied.
Corrected OTA pre-sign receipt is ready; existing ISO/catalog signatures retained.

### Corrected signed OTA final reboot — PASS

The renewed OTA signature verifies. Published1.8.22 baseline discovered, downloaded
and applied corrected frontend6d5135fa plus backend560aa600 through normal RPC.
Exact payload hashes, automatic restart/verification, saved file/credentials and
Cloud login pass. A full VM reboot (boot time2026-10-05T22:52:47Z) then passed
healthy startup, actual first-boot retry with unchanged container IDs/start times
and credentials, managed Cloud access, default/anonymous login denial and32
IPv4/IPv6 management-denial requests. Logs:
`/tmp/archy-190-vm-corrected-signed-ota.log`,
`/tmp/archy-190-vm-corrected-signed-reboot.log`.

Installed and cached runtime first-boot scripts on dev, yaya and Shorty now match
the qualified source, with private backups and no app/service operation:
`/tmp/archy-190-firstboot-final-node-deploy.log`. Existing ISO/catalog signatures
remain valid. The obsolete frontend must remain archived only.

Source/tag parity, asset publication/public hashes, public catalog selection,
fleet discovery and demo deployment are the remaining release operations.
Retain coverage boundaries: virtual BIOS/SATA and UEFI/SATA tested, no physical
legacy-BIOS/NVMe claim; IPv6 guard tested in isolation, no actual WAN IPv6 route;
Framework authenticated read-only fee quote and a new live seller-outage exercise
were not independently completed (funded regtest, prior operator purchase/free
reopen and persisted ownership/file evidence remain separate). Dev Bitcoin is
still in IBD; live full-chain Angor evidence is from Shorty. Historical discovery
limitation was explicitly accepted, not relabeled a passing recovery test.

### Public OTA assets verified and metadata promotion

Canonical release proposal5957be8c89cd192308860c194b05bed9f3077093d8284db0350503ec4adbded8
was marked applied. Local, ngit and Gitea main/tag parity passed; the annotated
v1.9.0-alpha tag points to7abd04a7. Follow-up FIPS backlog proposal
6015e09dcaba7004057dadb50b9dc09b5ada8e5cfdfa9be458ded7a0dad5002b
was also marked applied with exact accepted main mirrored.

Public backend, corrected frontend6d5135fa, signed manifest, signed catalog479f6193
and companion0.5.34 APK all passed full download byte-count and SHA256 checks:
`/tmp/archy-190-publish-ota.log`. Public manifest/catalog bytes also match their
locally verified pinned-root signatures. Promote those exact bytes to the live
metadata paths; no artifact or signature changed. ISO upload and demo deployment
are separate ongoing operations, not inferred passed from OTA asset publication.


### Missing ngit ISO release entry published — 7 October 2026

The operator correctly identified that the1.9.0 tag existed on ngit, but its
software release entry did not. Gitea already contained the original ISO and both
checksum assets. Published the missing [ngit1.9.0-alpha release](https://gitworkshop.dev/nevent1qqsfy0xpwnn9y423x6vnxxqtfrplm0n3fyfz9fzcheru2a42gj368sqzyp6xqpkcw7daqn3jry69t99yfzdyy3h8gqv0zhjhte8azny4v6746qcyqqq82mc22hxen) using the
existing Archipelago application/publisher, original tag commit7abd04a7 and alpha
channel. This adds release metadata; no ISO, signature, source ref or OTA/catalog
bytes were replaced.

Release event:`923cc174e6525551369933180b48c3fdbe71491222a458be47c576aa44a3a3c0`.
Both `wss://relay.ngit.dev` and `wss://gitnostr.com` acknowledged the three asset
records and release publication. Independent online `ngit release view` resolves
all three files with exact hashes/sizes and no unresolved asset IDs:

- ISO:2,695,954,432 bytes, SHA256`aad5f0350428976969043079a63b0e7a8264dcee2574526bd34719c798c750af`.
- Plain checksum:121 bytes; exact existing public/local bytes.
- Signed checksum:405 bytes; exact existing public/local bytes, pinned-root
  signature verified again.

The direct URL-backed ngit attempt failed its built-in1800-second acquisition
timeout and published no release result. Retained that failure. Reused the
established1.8.22 workflow: freshly hash the complete local ISO, verify its signed
checksum, sign its public asset records using the existing configured identity,
and attach those records with ngit's supported `--asset-event` interface. The
previous complete public-download hash receipt remains separate from today's
successful public range/sidecar checks; today's full URL attempt did not pass.

Publication used an isolated repository whose local main and annotated release
tag match both remote mirrors. Existing development main/candidate refs were
not rewritten. The complete advertised audit inventories12 unrelated branch
differences; main/release-tag parity does not establish full historical parity.

Durable public receipts and failed-attempt evidence:
`~/.local/state/archipelago/release-qualification/ngit-iso-190-20261007/`.
Existing known limitations are retained in the release notes. Current IndeeHub,
artwork and payment follow-ups are not included in this original ISO.

### Publication status and subsequent storage regression

OTA metadata promotion proposal
`0e90b3847ff377ad5c9e400037651b565defa4ee229eac785a7897c30423a4e7`
was marked applied. Exact main/tag parity and public metadata bytes passed. Dev,
Yaya and Shorty now use the public catalog; their app IDs/start times and guard
configuration were preserved. Thirty-two external management-denial probes and
tailnet UI access passed after the change. Framework reaches the public manifest
and its saved current version is already1.9.0-alpha; this is a read-only result,
not a fresh authenticated update RPC acceptance.

The public demo was deployed with immutable qualified images and preserved
configuration/rollback. Public version is1.9.0-alpha-demo. Mobile login/dashboard
passed on retry after the initial30-second form wait timed out. Public resumable
upload recovery/exact bytes/visitor isolation and replace/zero-byte/cancel cases
passed. Fixed-quota and interrupted-TCP scenarios retain their isolated evidence.

The raw ISO and both checksum files passed full public-download hash checks.
The public signed checksum also verifies against the pinned release root. Logs:
`/tmp/archy-190-publish-iso.log` (ISO PASS before the idle upload tunnel closed),
`/tmp/archy-190-publish-iso-checksums.log` (small sidecars retried over HTTPS).
No ISO re-upload or artifact change was needed. A subsequent
follow-up backend suite exposed the pre-existing storage-prefix bug documented in
[known limitations](release-1.9.0-known-limitations.md). Its correction is being
qualified separately. Artifact-byte verification is not a claim that this newly
identified issue has been fixed in the already published release.

Public assets: [1.9.0-alpha release](https://source.archipelago-foundation.org/lfg2025/archy/releases/tag/v1.9.0-alpha).
The separate follow-up branch now passes1,683 backend tests (four existing ignored)
and all1,222 frontend tests after correcting the storage classifier. These are
source test results, not inclusion in the published artifacts or live acceptance.
