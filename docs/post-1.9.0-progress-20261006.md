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
