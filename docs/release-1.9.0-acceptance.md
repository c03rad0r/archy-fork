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
  SHA256 `a5a6cfc7dcab011963a6f18dc570446b6fa624180d206a108fab27504f4fd41d`
  is deployed on dev, yaya and Framework with private rollback backups.
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
- [ ] **Shorty NPM:** shop certificate12/Force SSL are operator accepted and
      independently verified; qualify and apply the manual-route migration. Preserve live
      management containment and current public app routing.
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
      all-project discovery requirement and 34 unrecovered original announcements.
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
