# Post-1.9.0 work: qualification checkpoint

2026-10-06. These follow-ups are not a new published OTA/ISO. The immutable
1.9.0-alpha release and public demo are already published. This checkpoint does
not replace the scope in [the complete backlog](post-1.9.0-work-backlog.md).

## Implemented and deployed on dev/Yaya

- AI provider setup, private credential handling and Routstr funding entry:
  actual status endpoints and browser UI checks passed. No paid inference was
  performed. Physical companion and successful paid-provider response remain
  separate acceptance gates.
- Reciprocal approved peering: both actual nodes retain each other as Observer,
  with fresh contact timestamps. Live browser checks on both nodes at 390 and
  1440 pixels show exactly one reciprocal peer and navigate to connection setup.
  No peer RPC fixtures were used for these checks. Restart recovery and broader
  failure/trust/duplicate coverage remain in the acceptance matrix.
- Fleet/monitoring improvements: real metrics verified on dev/Yaya, with honest
  unavailable/stale states. Full Fleet actions, authorization and mixed-version
  failure matrix is not complete.

## Latest incident and candidate

Yaya's internet and physical interfaces were working. Its authentication signing
key had been left root-owned during earlier diagnostic remediation. The previous
loader ignored read/write failures and used an ephemeral key; restarts changed
CSRF tokens while sessions remained valid. The owner/mode were corrected without
rotating the existing key, and the repaired session survived another management
restart. The kiosk again displayed the real Wi-Fi and Ethernet interfaces.

Candidate `7e11f78e` adds bounded stale-CSRF recovery and distinguishes failed
interface retrieval from an empty successful result. It also combines the
container-store ownership, node-scoped catalog/player and FIPS follow-ups.

- Full isolated backend: **1,707 passed, zero failures, four explicit skips**.
- Full dashboard suite: **1,254 passed / 157 files**; production UI build passed.
- Candidate browser: 390/1440 pixels, injected stale-token or failed-interface
  response followed by real authenticated Yaya data; exactly one recovery retry.
- Production backend build is still pending at this checkpoint. These results
  do not establish live acceptance of that combined candidate.

Separate hardening `aa10bd12` + `a2e61382` removes ephemeral-key fallback, preserves
valid bytes, requires private durable creation, rejects damaged/unreadable keys,
propagates storage failure before RPC dispatch, and handles concurrent creation.
Its isolated full suite is compiling; it is not deployed. See
[session recovery](session-recovery-followup.md).

## V4V

Versioned app image and node-only manifest are prepared; the original demo catalog,
actual login-background promotion and app/player bridge are implemented. Focused
player/bridge tests and image build passed. Yaya's existing Portainer app/data
remain untouched. The final image is being loaded into isolated qualification
storage; no Yaya-only catalog has been signed or enabled yet.

A cleanup bug stopped the first isolated fixture: the backend confused containers
from another Podman storage root with ghosts. Store/owner checks are fixed and
focused tests pass. Deploy that fix, prove the final fixture survives cleanup,
then test actual authenticated playback, pause/close/reopen, unchanged iframe,
seek/resume and app relock. Only then enable the signed Yaya-DID catalog and
complete upgrade/restart/rollback and mobile/companion acceptance.

## IndeeHub and FIPS

Signer fixes passed focused tests, production build and authenticated Yaya browser
login/reload. Actual companion background/resume remains unverified. Publish the
required app update at the end, after integrated qualification.

The distributed Archipelago source and full publish/discover/pay-producer/timed
viewing flow are not complete. The design review and selected Yaya video are
prepared. Implement durable entitlements, settlement correlation and original
signed discovery; qualify retries/outages/expiry without double payment. No new
real spending is authorized by this checkpoint.

FIPS-required peer media requests and bounded local-cache HTTP streaming are
implemented in the combined candidate. Seller-side full-buffer reading and the
complete IndeeHub media path remain open; no end-to-end all-media-FIPS claim.

## Other active tasks

| Task | Remaining acceptance or work |
|---|---|
| Connection UX | Flow plan written; broad navigation changes and lifecycle acceptance remain. |
| Connect with Nodes / Nostr requests | Implemented; retain full request/retry/trust matrix and companion acceptance. |
| Offline indicators/order/map | Fixture-tested changes; qualify real outages, stale metrics and recovery. |
| Navigation and app launch speed | Establish before/after distributions; actual companion remains required. |
| Framework Monitoring | Existing kiosk is signed out and RPC returns 401; not a passed monitoring check. |
| Native companion reliability | No ADB device attached at checkpoint; browser tests do not substitute. |
| Immich/Nextcloud libraries | Assessment/proposed authenticated API integration only; no enabled connector. |
| Cosmetic Web5 Wallet label | Removed; legitimate wallet/hardware functions preserved. |
| Mirrors, catalog, app updates, OTA/ISO | Integrate/review once through ngit; mirror exact accepted history to Gitea. Required artifact gates remain. |

## Latency evidence and limitations

The first live dev mobile connection-navigation check exceeded five seconds.
A diagnostic repeat navigated in 763 ms. The saved dev diagnostic session was
stale and restored through remember-me; after capturing refreshed cookies, the
four node/viewport checks passed. Retain the initial failure: this does not prove
that the operator's intermittent delay is solved. Cold peer visibility in these
runs took roughly 3.1–4.6 seconds, including initial navigation/render/tab click;
these are not isolated API latency or a before/after performance comparison.

## Retained earlier limitations

- Angor: operator accepted incomplete historical discovery for release on
  2026-10-05. All 35 reference commitments were verified, but 34 original signed
  announcements remain unrecovered from the queried sources. Recovery is open.
- Framework radio/hardware investigation remains operator-deferred.
- Earlier Framework LND incident remains separately closed with operator
  acceptance; do not reopen it as the explanation for unrelated failures.

## Local evidence

No credentials or raw private inventories are included here. Qualification logs:
`/tmp/archy-session-recovery-backend.log`,
`/tmp/archy-session-recovery-full-ui.log`,
`/tmp/archy-session-recovery-browser.log`,
`/tmp/archy-peering-live-browser-2.log`,
`/tmp/archy-peering-live-browser-diagnostic.log`,
`/tmp/archy-session-key-backend-2.log`,
`/tmp/archy-framework-monitoring-current-3.log`.

## Latest addition: MeshCore (last in sequence)

Operator reopened Framework/dev radio investigation on 2026-10-06: UK-plan public
messages not exchanged, no other radios shown, missing-listener error and mesh
page 502s. The earlier radio deferral is superseded for this new scoped task.
See backlog item15 for the full settings-clarity and two-radio acceptance scope.
No radio settings, firmware or services were changed while recording this task.

## Subsequent qualification — morning 2026-10-06

The 7e11 backend/UI candidate reached dev and passed actual stale/missing-CSRF
rejection and recovery against the interface RPC. Durable signing-key hardening
then passed the full isolated suite: 1,714 tests, zero failures (five listed
ignores, including the subprocess helper exercised by its parent test).

Deployment exposed a second cleanup path in the shell doctor and an EROFS
failure in its embedded repair. The old OTA runtime payload retained on disk
replaced the corrected helper during startup. Dev containment now covers both
the runtime payload and installed helper; the isolated V4V fixture has survived
five subsequent scheduled doctor runs and answers health requests. Source repair
57923b0a uses the host namespace and runs before reconciliation. Its 12 focused
bootstrap tests pass, including stale content, execute-mode repair, idempotence
and installation failure. Production build/live restart qualification is pending;
Yaya has not received this newest backend yet.

The reusable V4V media bridge browser check passes at 390/1440 pixels with real
bundled audio, hidden playback, pause/resume and the same retained iframe. Full
dashboard integration found a mobile defect: the recreated bottom navigation
was not remeasured after a store-driven app closed, covering the audio controls.
7946ef86 repairs that lifecycle and adds accessible player-button names; three
focused navigation/bridge tests pass. Final UI build and actual browser rerun are
pending. No node-only catalog is signed/enabled yet. These results do not close
the remaining IndeeHub, Fleet, FIPS, companion or standard-channel radio gates.

### Deployment gates passed on dev and Yaya

Backend57923b0a (`506dbe55d03617d4d500f67f7c4e5baf50a45c89bedaed48408417b48e13bdc9`)
and dashboard883c5a7c (entry SHA256
`3dc1565ad0a12b47c7d8f0d9a84154e5f7c9be430cf599d9733358d2c39fdc7b`)
are deployed to both nodes. Production builds pass. Prior binaries/UI and app
state are retained under each node's root-only support directory.

Both actual nodes pass:

- Backend health, served dashboard hash and unchanged qualified AIUI entry.
- Existing normal-store container IDs and start timestamps unchanged by rollout.
- A further management restart repairs an inert stale runtime script through the
  real service filesystem sandbox; resulting script matches embedded bytes and
  is executable. The safe runtime payload is restored after the probe.
- Persistent signing-key bytes unchanged through restart (values never logged).
- Authenticated stale/missing CSRF rejected with403 and a replacement CSRF cookie;
  retry succeeds200. Unauthenticated requests get401 without a recovery cookie.

The actual dashboard player browser check remains open. A browser-only package
fixture must survive live state refreshes, and proxying media through Playwright
introduced buffering delays. Qualification is being repeated using the direct
loopback fixture route on the updated Yaya dashboard. Do not substitute these
fixture results for a signed catalog installation or physical companion check.


### Subsequent deployment overlap and source reconciliation

Another session replaced both node backends with a mining-launch artifact after
the successful checks above. Those checks remain evidence for the stated hashes,
not acceptance of the replacement binary. Deployment writes are paused while
reconciling the sources and artifact provenance.

The local mining handoff bundle contains `2fad10c8`, descended from our backend
`57923b0a`, with app presentation and DATUM credential changes. Integration merge
`6c985b8d` retains both original commits and the later dashboard fixes. All 46
focused launcher/catalog tests pass; full backend/dashboard suites and dashboard
production build are in progress. No reviewed main, remote or release changed.

The actual dashboard media check initially passed at mobile and desktop widths,
then repetition exposed a cold catalog-loading race. `69cd4021` loads node launch
policy independently of the public catalog, gates demo launch on current policy,
and cancels a deferred launch when the user closes it or chooses another app.
The repeated actual-browser check is still required on the final integrated UI.

V4V image verification and its immutable digest are recorded in
[node demo qualification](node-demo-catalog-and-media.md). A private unsigned
Yaya-only catalog is prepared with one Sovereign Music banner and a 90-day
expiry. No catalog has been signed or installed, and the original Portainer
stack/data remain unchanged. Signature, managed installation, data migration,
wrong-node rejection and physical companion acceptance remain open.


### Web5 footer alignment and continued qualification

The operator added bottom-aligned Monitoring/card actions to the backlog.
Monitoring, Federation and Identities now use growing card columns with footer
space above the actions. No fixed card height or absolute-positioned button is
introduced. Connected Nodes, Node Visibility and Nostr Relays already use growing
content or automatic footer margins.

A headless browser with the source candidate and authenticated local backend
passed all six card/viewport cases (three cards at 390px and 1440px), measuring
bottom padding while adjacent content expands and shrinks. Seven relevant
component tests pass. Evidence: `/tmp/archy-card-footer-browser.log` and
`/tmp/archy-card-footer-unit.log`. These are candidate checks; node deployment
and operator acceptance remain pending.

The integrated dashboard suite before this footer-only change passed 1,262 tests
in 158 files, and its production build passed. The isolated backend suite is
still compiling. A candidate-production V4V browser run passed mobile hidden
playback, bottom-bar pause/resume, reopen of the same frame and stop. Desktop
currently times out clicking Close and remains under investigation.

The alternate-port preview correctly failed V4V's dashboard-origin restriction.
Candidate HTML substituted at the normal origin also needed Chromium's explicit
local-network permission because synthetic responses lack normal address-space
metadata. That permission is confined to the isolated loopback test context;
no app origin policy or live browser settings were weakened. These fixture
results cannot replace final deployed-node/companion acceptance.


The desktop timeout was identified from a failure screenshot: a visible CPU-load
notification covered the session Close button. The repeat used the notification's
normal dismiss button, then passed the entire desktop sequence. Mobile and
desktop candidate checks now pass hidden playback, pause/resume, reopening the
same iframe and Stop. Logs: `/tmp/archy-integrated-v4v-browser-origin-4.log`
(mobile pass, earlier desktop obstruction) and
`/tmp/archy-integrated-v4v-desktop-2.log` (desktop pass). No forced clicks or
production notification suppression were used. Production UI rebuild including
the new card footers is running; signed-install and physical companion gates
remain open.


### Paid-preview access boundary (new finding during FIPS work)

Source review found that the anonymous preview route returned full paid image
bytes and relied on browser CSS blur. It also served previews for restricted
shares without checking a recipient, and the minimum byte-prefix size could
return a small paid audio/video file in full.

The candidate now produces a fresh, small blurred JPEG on the server, drops
original metadata, bounds raster input/dimensions/decoder concurrency, and fails
closed on unsupported images. Anonymous previews reject specific-recipient and
peer-only content. Audio/video prefixes are at most 10% and 8 MiB, with no
minimum that can reveal the full original. Four isolated regression cases are
compiling; no deployed fix or full preview acceptance is claimed yet.

The preceding integrated backend suite completed: 1,718 passed, zero failures,
five listed ignores. The ignores cover opt-in real AI providers, RNode hardware,
Reticulum daemons, live Minibits and the subprocess permission helper (which its
parent test executes separately). A production build of the earlier integration
is running from detached `11f016a9`; it does not include this new preview fix and
must not be described as the final release candidate.


### Bounded peer delivery and preview qualification

Paid-preview source at `051dc7e3` passed all four isolated regression cases
(`/tmp/archy-preview-boundary-tests.log`). No live deployment is claimed.
The earlier production backend at `11f016a9` also built successfully and was
archived with its SHA256/source receipt under the private qualification directory;
it excludes these later fixes and is not the final candidate.

`2fee0339` replaces whole-file seller buffering with bounded file-backed responses.
Bearer payments prepare a complete private anonymous snapshot before redemption;
free/owner/durable-invoice transfers can stream an open file directly. Rootless
Files reads consume bounded subprocess stdout and require successful completion
before payment. Malformed ranges are rejected, suffix ranges are supported, and
free public previews also stream. New tests cover source deletion during payment,
invalid payment, multi-gigabyte sparse files, truncated preparation and ranges.
Full isolated backend qualification is running from the separate frozen media
worktree (`/tmp/archy-bounded-media-backend-tests.log`); results remain pending.
Buyer-side caching still needs bounded transfer and recovery work.

Buyer follow-up now streams successful ecash/Lightning deliveries into the owned
cache, records incomplete delivery before consuming the response, removes partial
temporary files on cancellation, and blocks duplicate concurrent ecash purchases
per seller. Owned-file opens and saves use local HTTP streaming rather than base64
for current clients; small legacy reads remain supported. Optional Files copies
stream through the existing no-clobber namespace writer. Interrupted receipt/body
handling is not equivalent to a durable end-to-end ecash retry protocol: loss
before response headers or during mint settlement remains an explicit review gate.
No real funds were spent. Nineteen focused UI tests passed before the final two
stream-viewer cases were added; backend/production qualification is pending.

Seller streaming's first full compile found a lifetime error in one new test;
`df7677d2` corrects it. The repeated isolated full suite is compiling from that
frozen source (`/tmp/archy-bounded-media-backend-tests-2.log`). The failed run is
retained and is not counted as a pass.


### Latest peer-content review

Seller streaming at `df7677d2` passed the full isolated suite: 1,729 passed,
zero failures, five explicit skips. Buyer streaming required updating existing
regression fixtures to the new streamed Files-copy boundary; two failed compile
runs are retained. The final buyer UI has 21 focused tests passing and its
production build passes (`/tmp/archy-buyer-cache-ui-tests-final.log`,
`/tmp/archy-buyer-stream-ui-build.log`). No deployment has occurred.

A separate source review found restricted requests trusting an unsigned peer DID.
The candidate now verifies recipient/path/range/time-bound node signatures, and
uses the same visibility gate for metadata, invoice issuance and bytes. The
isolated combined suite is compiling from the frozen authentication worktree;
see `peer-content-authentication.md` for compatibility and remaining gates.

Further review caught an authentication-preparation failure escaping the payment
request's refund branch. Authentication and transport now return through one
result, and local identity validation happens before ecash creation. Inline
preview/legacy RPC bodies are also bounded, including unknown-length responses:
large free videos must not be base64-loaded merely to populate a card preview.
Those final changes still require backend qualification.

Read-only checks at09:49UTC confirm dev and Yaya Monitoring/federation CPU, memory
and disk measurements match, with each tested RPC below0.5seconds. Framework
still returns401 for the saved dashboard session. These are current live-source
checks, not acceptance of the new undeployed file-streaming candidate.

## Streaming/security continuation — October 6, 10:50 UTC

The later candidate supersedes the checkpoint above; no new deployment or release
is implied. Source integration preserves the separate mining-launch commits.
Both-node deployment remains on hold while coordinating that session's writes.

- Full backend at `8ffbf5ff`: **1,738 passed, zero failures, five skips**.
- Candidate `b8e512fe`: **1,739 passed, one failed, five skips**. The new
  corrupt-peer-store test exposed federation parsing that silently returned an
  empty list. `1971aeb3` makes parsing fail explicitly and preserves the source
  file. Its full rerun is pending; the failed run is not an acceptance pass.
- Paid seller responses and buyer caches now stream bounded chunks. Anonymous
  paid-image previews are generated thumbnails; private availability is enforced
  and peer identity proofs bind the recipient, route, range and time.
- On-chain cache follow-up `2e761666` uses the same durable local file path and
  avoids whole-file base64 for current clients. All nine focused payment UI
  tests pass, including cache playback without another payment. Backend tests
  for complete and interrupted streamed delivery are pending.
- Production binary compilation is still for `b8e512fe`, which excludes the
  corrupt-store correction and on-chain follow-up. Do not deploy it as final.
- The UI archive at source `6fba95fe` passed 21 viewer/payment tests and production
  typecheck/build; it excludes the new on-chain UI follow-up.

Remaining payment gates include durable recovery before response headers, seller
capability/identity binding for on-chain delivery, and crash/ambiguous-settlement
recovery without a second spend. No additional real payment was made. Source
and fixture results do not establish live cross-node acceptance.

MeshCore is last in the requested order: the later explicit two-radio request
reopens that scoped Framework work (standard public channel, UK plan, Heltec V3
and V4). The older general hardware deferral is not a reason to omit it.

## Combined qualification — October 6, 11:25 UTC

- Isolated backend at source `d46f6cee`: **1,743 passed, zero failures, five
  explicit skips** (`/tmp/archy-payment-method-final-backend.log`). This includes
  the seller's persisted on-chain/Lightning method, legacy payment records,
  interrupted HTTP delivery and the corrupt-peer-store correction.
- Dashboard: **1,271 passed / 159 files**, followed by a successful production
  typecheck/build. One earlier full run missed a certificate readiness deadline;
  the focused certificate tests passed. A second full run was stopped after
  load-related timeouts. The final sequential run passed without test exclusions
  or raised deadlines (`/tmp/archy-stream-fleet-full-ui-3.log`).
- Fleet source fixes now age status/counts/ordering without successful network
  refresh and discard history responses for a previously selected node. All
  15 focused tests pass. Chromium at 390/1440 pixels verifies stale status and
  reordering with telemetry fixtures and no new report. This is browser fixture
  evidence, not proof of a real node outage or full Fleet acceptance.
- UI archive saved under `stream-fleet-ui-d46f6cee` in the local qualification
  directory, SHA256 `ab6e3807dc6a74e63b8effb3e9948622434861d816d9ec49f0ffa28434760672`.
- A production backend build from `081c8215` (same tested code, later docs only)
  is running. No new candidate deployment or publication is implied.
- Framework's management service is active; actual kiosk is on `/login`, and the
  saved session returns401. Monitoring UI acceptance remains open. Dev has enough
  Cashu balance for initial live tests; no payment has been made in this pass.
- Operator subsequently topped up both nodes and authorized longer node-to-node
  tests. The expanded test cap is **25 sats total including fees**, with one-sat
  transfers and a private per-payment ledger. This is not creator pricing.

Open payment gates and the complete backlog above remain in force. The current
build does not claim durable recovery at every pre-header payment failure or
resolve the previously recorded on-chain bearer-address concern. IndeeHub's
integration boundaries are mapped in [the integration map](indeehub-integration-map.md).
Deployment coordination with the separate mining session is still pending.

## Atomic share publication qualification in progress — October 6

Paid share creation previously added a free/public item, then set its price and
visibility in later RPCs. The Web5 form also selected the final catalog item to
price, which could target another concurrent share. The candidate now sends a
complete policy through `content.publish` or `content.configure`. These distinct
methods fail on an older backend instead of silently ignoring pricing fields.
Legacy `content.add` defaults new entries to hidden until explicitly configured.
A cached old Web5 form may therefore require a refresh to publish; do not restore
an unsafe public default for that compatibility case.

Catalog mutations serialize their read/change/write transaction, replace the
catalog atomically after syncing the temporary file, and reject malformed saved
JSON without overwriting it. Re-sharing a filename retains and returns its saved
ID. New hidden or peer-restricted shares do not export public DWN metadata;
retracting already-exported metadata remains a separate open requirement.

Focused UI checks passed five cases before the compatibility endpoint adjustment;
a rerun and isolated backend suite are in progress. Added regressions exercise
concurrent updates, malformed catalog preservation, stable IDs and rejection of
incomplete policy updates. These changes are not covered by the earlier 1,743
backend result or the archived dashboard build, and are not deployed. The running
`081c8215` production build also predates them.

The operator-funded wallets remain untouched in this pass. The private ledger
retains the 25-sat inclusive test cap and zero spent. Deployment coordination and
the durable payment recovery gates above remain open.

Qualification follow-up: the atomic endpoint change passed five focused UI tests
and `vue-tsc --noEmit`. The added old-server rejection case also passed (six UI
cases total): no fallback to legacy writes, no success event, and the error stays
visible. A final rerun corrects a missing required prop in that test fixture.
Authenticated read-only checks confirm both dev and Yaya have at least 25 Cashu
sats available. No payment, deployment or release occurred. Backend compilation
for the atomic sharing suite remains in progress; it is not marked passed.

## Verified atomic-sharing results — October 6 continuation

- Isolated backend suite passed **1,747 tests, zero failures, five explicit
  skips** (`/tmp/archy-atomic-share-backend-tests.log`). This covers the new
  publication policy, stable share IDs, concurrent catalog updates and corrupt
  catalog preservation. Rust source is `19207282`; `f3264c8d` adds frontend failure
  coverage and documentation only.
- Final focused UI rerun passed **six tests** with the corrected required prop
  (`/tmp/archy-atomic-share-ui-tests-final.log`); typecheck passed earlier.
- Baseline production `081c8215` finished successfully and was archived with
  SHA256 `1f26af4bd25d8676ced4152c0a2ed142c52a654c6684cfd0fa9130cd59b96000`.
  It predates atomic sharing and is explicitly not the final candidate.
- Production build of `f3264c8d` is running, log
  `/tmp/archy-atomic-share-production-backend.log`. No deployment or payment
  occurred. Cross-session deployment coordination remains open.

## IndeeHub catalog and actual-library work

IndeeHub follow-up `f47a466` discards stale source/refresh responses, including
late failures and late ownership-enrichment responses; it also avoids logging
raw HTTP errors that may contain request credentials. All 20 app tests passed.

`926b87a` replaces Browse's random progress and fabricated rentals/saved lists
with authenticated `/library` and `/rents?status=active` records. Nested film IDs
are retained for playback/payment, expired or invalid-expiry rentals are excluded,
and the UI shows the actual rental end timestamp rather than a fixed “48h left”.
The library clears on logout/account changes and ignores late responses; failed
same-account refreshes retain previous records with an error/retry control.
The add-to-library client route now matches `POST /library/:projectId`.
All 25 app tests passed; production typecheck completed and Vite build is running.

These changes are not deployed and do not complete the distributed paid-video
feature. Actual browser/companion acceptance, full library pagination (current
API defaults to 30), persisted viewing progress, FIPS distribution, creator
payment routing and the new entitlement window remain open. No test purchase,
commercial rental policy change, or app-image publication occurred.

## Deployment coordination cleared; IndeeHub browser qualification

The operator confirmed that the other mining session has finished and handed
over. The dev/Yaya deployment hold is cleared. Both live binaries still match
handoff SHA `b35c478fc11895ff2349c89c92c4d4c177f783e598ddf34389b08a36bcebb1f8`;
Yaya's management service is active. Prepared deployment verifies those hashes,
backs up the binary/UI, switches the backend before the new UI entry, checks
health/session-key/container preservation, and rolls back on a failed switch.

IndeeHub production-browser testing found a real direct `/library` redirect:
Browse decided login was missing before session restoration finished. Marking
the route authenticated alone was insufficient because the guard skipped an
already-running restoration. `4b8667f` shares that in-flight promise and waits
for it in the guard. All **26 app unit tests pass**. `5f22e71` also keeps the actual
expiry label within mobile cards, retaining the full expiry in accessible text.
The final production build and browser fixture checks pass at **390 and 1440px**:
direct saved-session entry, saved projects, active rentals, expiry containment,
empty state, outage and retry. Logs: `/tmp/indeehub-session-library-tests.log`,
`/tmp/indeehub-library-final-build.log`, `/tmp/indeehub-library-browser-final.log`.
Earlier failed browser runs remain recorded; one intercepted the document as an
API response, two exposed the auth race, and another used a midnight-UTC fixture
that crossed the year boundary locally. None is counted as a passing run.

Final IndeeHub UI archive SHA:
`e8d7a9f70c0db4c8af10177783bcc773927d6b95a57e68a83764674582054f68`.
Updated dashboard production UI archive SHA:
`a60c8ae5a2d23ebb672c8e10634891cf3b409721960257a330068cd70bcccecb`.
No app image, catalog or live deployment is implied by these bundles.

Live **pre-deployment** checks verify reciprocal peer records against each
node's authenticated public key and successful catalog browsing over FIPS in
both directions. Both funded wallets select the same default mint; its three
advertised sat keysets report zero input fees. Spend ledger remains zero.
Repeat route checks on the deployed candidate before paid transfers. These
checks do not close the durable-payment or distributed-IndeeHub requirements.

## Live deployment and paid-file qualification completed October 6

This supersedes the pre-deployment checkpoint above. The optimized candidate
finished building and was deployed to **dev and Yaya** after the operator cleared
coordination. Both run backend SHA256
`9fe2eb984675d6ed6d1eb1d2facd342101988aa6863fc27416865d733ad231b7`;
served UI entry SHA256 is
`c192dda713b512acad2aca01458d8e06b64df828a8028ee6e41d0c7b647a9264`.
Health, saved login, session-secret preservation and unchanged app container IDs
and start times passed. Each deployment retained a local support-directory
rollback. Bitcoin, Electrum and wallet apps were not stopped.

- Free FIPS file transfers passed in both directions with exact hashes.
- Each node bought one 20 MiB fixture from the other for **1 sat** using the
  explicitly selected Cashu method. Total gross transfer **2 sats; fees 0**.
  Each seller received the expected sat and each buyer paid exactly once.
- Complete cached bytes and HTTP range reads matched. Removing each seller's
  temporary share did not prevent an owned-cache reopen; no further payment.
- Twenty deliberate cached-download interruptions in total, accompanying seeks,
  and an additional management-service restart on each node passed. Owned records
  and cached reads survived, balances stayed unchanged and app containers were
  not restarted.
- Live legacy-add policy stayed hidden; an atomically configured paid share
  required payment on the unpaid path, in both directions, without wallet changes.
- One initial fixture-cleanup attempt encountered a Files ownership permission
  error after successful payment/reopen checks. Exact-hash-guarded privileged
  removal of only the named fixture resolved cleanup, followed by verification.
  The failed attempt remains in the evidence; it is not counted as a clean run.

Evidence logs: `/tmp/archy-fips-free-qualification.log`,
`/tmp/archy-fips-paid-qualification.log`,
`/tmp/archy-fips-paid-qualification-second.log`,
`/tmp/archy-paid-cache-restart-qualification.log`,
`/tmp/archy-share-policy-live.log`. The private spend ledger remains cumulative
with two successful purchases; do not reset it or repay for these tests.

**Still open:** interruption during initial payment/delivery, durable recovery
before successful response headers, timed IndeeHub rentals, producer receiving
and full app integration. Cached-download interruption does not test the earlier
payment boundary. No new release/catalog/app image was published by this work.

## IndeeHub implementation continued after missing-source report

The operator's observation was correct: Yaya's IndeeHub image had no Archipelago
source. Only the Archy sharing backend had been deployed. No IndeeHub app image
has been published or deployed during this continuation.

IndeeHub branch `work/archipelago-auth-and-sharing` now contains:

- `b52407c`: verified signed-offer discovery, deterministic revisions and deletion
  tombstones, persistent browser cache, explicit relay completion/error handling,
  signed publication retry outbox, and configured Archipelago browsing/search.
  **74 tests passed**. Built-browser checks passed at 390/1440px for signed titles,
  forged rejection, source isolation, mobile/desktop search, cache outage/retry,
  displayed price and rejection of legacy payment/playback. Existing library and
  session-restoration browser checks passed again on that bundle.
- `38ed9b6`: timed-rental access policy and PostgreSQL row-locking store.
  **26 tests passed**, including a real isolated PostgreSQL instance and 24
  concurrent first-play requests. Window, expiry, buyer/offer/hash bindings,
  unpaid/revoked rejection and persisted clock-rollback protection were tested.
  Backend build passed. Temporary DB container, volume and credentials removed.

These are component checkpoints, not complete paid-video acceptance. Backstage
project authorization/publication transaction, node offer registration, correlated
receiving/payment recovery and actual FIPS timed playback are still open. The
browser qualification bundle contains an intercepted test relay and **must not be
deployed**. Its playback guard is deliberately disabled until those paths exist.
See the IndeeHub repository's `docs/archipelago-catalog-implementation.md`.

While tracing producer signing, source inspection found an existing dashboard
bridge defect: concurrent consent requests overwrite the one stored promise,
leaving the earlier app request waiting indefinitely. A queue/cancellation fix is
being qualified in this worktree. It preserves the approved signing animation,
checks identity/session changes, bounds the queue and cancels pending work on
close/unmount. This is a confirmed source defect, not yet proof of the physical
companion grey-screen cause. It is not deployed at this checkpoint.

Signer queue checkpoint: **17 focused tests across five files pass**, covering
concurrent requests, denial, error dismissal, closure, queue bounds, identity
changes, reopening a retained session, existing consent scoping, tab signing and
the approved consent presentation. Dashboard typecheck passes. Logs:
`/tmp/archy-signer-queue-related-tests.log`,
`/tmp/archy-signer-queue-typecheck.log`. Production UI build/deployment still
pending; physical companion causality remains unverified.

The signer UI candidate `715e86c9` was deployed to dev only, with UI backup;
backend/session secret/app containers were unchanged. Live served-browser checks
with isolated signing RPC fixtures passed at 390/1440px, but an earlier run saw
two first-request responses after iframe startup. Do not erase that failed run.
Inspection found fallback `http://host:7778` versus runtime `http://host:7778/`
changes the raw iframe src during initial state discovery. Canonicalizing the
computed URL prevents this semantically identical destination from reloading.
The new regression observes no reactive iframe-source change for this update,
while real path changes still propagate and cancel prior pending consent.
29 focused routing/session/signer tests pass; final rebuild/redeployment remains
pending. The first browser attempt was also missing the signed-in local marker
and redirected to login; this fixture error was corrected separately.

## Native signer queue and stable app URL deployed

Final dashboard source `cc9f02df` is deployed on **dev and Yaya**. Served UI index
SHA256 is `2beddd7ea77b9b186031e29ef1a8b5e4184878d0ff1db7e25447a9e9b1406c00`;
archive SHA256 is
`a06562613e29e923486871d20dfa2c557369a7ade8f036942eb8eb29295b0e83`.
Production build/typecheck and the final 29 focused routing/session/signer tests
pass. Existing backend `9fe2eb98...`, session secret and app container IDs/start
times stayed unchanged; no management/app restart was performed for this UI fix.
Both nodes retain a support-directory UI rollback.

Served-dashboard browser fixtures pass at **390 and 1440px on each node**:
exactly one app iframe load, two concurrent consent requests, ordered individual
approvals, exactly one response per request and the preserved completion
presentation. Signing RPCs were intercepted using a qualification-only identity;
**no real key was used, no event published and no payment made**. These checks
exercise the actual deployed dashboard, not a replacement dashboard fixture.
The app iframe/signing backend are isolated fixtures, not actual IndeeHub login
or physical companion acceptance. Harness:
`tests/lifecycle/native-signer-concurrency.cjs`.

Evidence: `/tmp/archy-native-signer-stable-{dev,yaya}-deploy.log` and
`/tmp/archy-native-signer-stable-{dev,yaya}-browser.log`. Earlier failed fixture
login and duplicate-first-response runs remain retained; final acceptance does
not erase them. Artifact receipt is updated with both deployments.

Still track the separate legacy `stores/appLauncher.ts` signing handler, which
has its own consent implementation; this deployment qualifies the AppSession /
shared bridge path. Do not describe every possible app launcher or the physical
companion grey-screen report as fully accepted from these checks.
