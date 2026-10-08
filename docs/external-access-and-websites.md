# External access and website publishing

Approved on 2026-10-08. Worktree: `archy-external-access`; branch:
`work/external-access-websites`; base: `c57119e9`. The release checkout,
services, build directories, artifacts and publication refs are not work areas.

## Product contract

AI creation uses Routstr by default, with Claude and OpenAI API-key choices in
the trusted dashboard. The operator explicitly approved these optional API
providers. Reuse the node's ecash receive flow for top-ups; funding must not change
the spending allowance or trigger inference. Preserve saved provider choices.
Ollama is deprioritized and its live-model test is not a release prerequisite for
this work; older progress entries below describe the superseded local-model path.

Setup offers Allow external connections and Publish a website. Both use node-owned
connection state. Each app or site can select FIPS, public HTTPS, Tor, and (static
sites only) Nostr publication together. Each route has independent status and
revocation. Configured, locally available, and externally verified are different
states. Never infer public reachability from a saved record or running daemon.

Reuse existing FIPS IPv6 ingress, AppGate and IPv4 loopback backends. Never widen
container bindings as a blanket IPv6 migration. Preserve local-only APIs and
wallet/admin exclusion policies. Existing routes are not adopted or revoked
without an explicit ownership handoff. Private route success does not satisfy a
public website's prerequisite. Removing one publication must retain shared routes.

All required components are open source and self-hostable. No Tailscale or
proprietary control-plane dependency. Public routing uses frp with TLS terminating
on the node; gateways are selectable and replaceable. DNS and gateways remain
operator dependencies, even when their software is open source.

Consider Nostr first wherever an existing standard fits: FIPS identity/discovery,
NIP-5A/nsyte/Blossom for optional static-site replication, existing signing flows,
and ngit contribution/review. Public announcements and replication require an
explicit choice. Keys and infrastructure credentials never enter model context.

Mynymbox is an optional external Bitcoin/Lightning domain checkout. Explain its
registrant-of-record model. Generate exact DNS record instructions for the chosen
route; support existing domains and free addresses. Preserve mail records and
verify authoritative DNS, hostname routing, TLS and external HTTP independently.
FIPS/onion addresses do not require a purchased domain. No automatic purchases.

AIUI creates node-owned static website projects with isolated previews, revisions,
download, publish, rollback and unpublish. Local/open model operation is supported;
no silent fallback to a proprietary model. A published website has a separate
origin from management and cannot read dashboard cookies or access signing
authority or RPC. Direct FIPS ports share a hostname, so a browser may send
host cookies to the trusted static handler; it neither reflects nor forwards
them, and published HTML runs under a script-blocking sandbox policy. Public
copies may survive unpublishing from Nostr/Blossom.

The user also requested removal of the File Browser Setup card because the app
is already bundled in the ISO. Keep the installed app and launcher unchanged.

## Implementation and acceptance ledger

- [x] Isolated worktree and branch created.
- [ ] Persistent, versioned project and multi-route configuration; conflict-safe writes.
- [ ] Both Setup entry points and shared connection readiness.
- [ ] DNS guidance with Mynymbox handoff and correct per-route records.
- [ ] Static site workspace, local model generation, isolated preview and version history.
- [ ] FIPS publication and revocation through owned listeners/policies.
- [ ] Public HTTPS/frp configuration, scoped enrollment and node TLS lifecycle.
- [ ] Tor publication preserving service identity through restart.
- [ ] NIP-5A signing and Blossom replication with explicit consent and pinned versions.
- [ ] Per-app restricted access without sharing administrator credentials.
- [ ] External verification, certificate renewal, restarts, rollback and route isolation.
- [ ] Framework acceptance with confirmed identity, access and release coordination.
- [ ] ngit review and exact accepted-commit mirror parity before any release.

The user authorized Framework as a free test node and a separate test proxy route
on Yaya. Access has been verified on both actual nodes. Preserve all
wallet/channel/app data. Source tests are not node acceptance. Backend unit tests
run only through `scripts/test-backend-isolated.sh`; use a worktree-local target.

### Current integration checkpoint

The operator reaffirmed the existing Setup walkthrough design during UAT.
The follow-up UI uses the existing numbered goal cards, progress styling and
Back/Continue navigation, with one expanded step. Previously saved connections
are reused; installed Blossom omits the installation step. Blossom installation
uses the normal app-store installer. Navigation itself never saves, signs or
publishes. This UI revision is now active on Framework. The actual dashboard walkthrough
saved the synthetic draft, archived it in local Blossom with a profile identity,
fetched it back to verify exact bytes, and published it through FIPS port 32000.
The 390-pixel mobile layout passed the overflow check. Browser request monitoring
recorded no external requests during this journey.

Live archive acceptance exposed an older `node-*` identity whose `is_node` flag
was false. The container correctly rejected its upload because the canonical
signer allowlist excludes legacy node records. The website identity filter now
matches that rule, including node-name fallbacks and public-key validation, and
checks it again before signing. No public Nostr event or external replica was
created during this failure. The selected 20-test suite covers the regression and
walkthrough navigation; the production typecheck and Vite build passed. A further
new-project connection-inheritance check passed, giving eight Setup and thirteen
Nostr tests for the revised UI.

Blossom is installed and healthy on Framework through the normal app installer.
Protocol, real HTTP/HTTPS tab signing and lifecycle/data-preservation evidence is
recorded in `apps/blossom/README.md`. The combined dashboard/backend candidate
from local commit `28a92fcc` is now deployed privately on Framework. The
authenticated publishing status probe passes; native Bitcoin/LND process IDs and
start times are unchanged. Complete browser acceptance is still in progress. No public Nostr test events or external file replicas have
been created. The earlier standalone proxy route/certificate were removed. A new owned UAT
route now connects the dashboard-published synthetic site to
`https://free.archipelago.builders` through Yaya. Trusted TLS, exact page bytes,
`/rpc` returning 404, and traversal rejection (400) pass. The route remains for UAT;
FIPS/HTTPS revocation retained the Tor publication and archive, and Tor
revocation retained HTTPS. Republishing retained the onion hostname. The existing
Yaya Tor client timed out after republish, while a separate fresh Tor client
fetched the exact restored page; do not treat the first timeout as a confirmed
publisher defect or a universal reachability pass. Temporary notice logging was
removed and the isolated test client was stopped after qualification.

A management-service restart retained exact project/archive state, all app
container IDs/states, and native Bitcoin/LND PIDs/start times. Public HTTPS and
Tor both returned exact content after that restart. A full machine reboot is
separate and awaits the operator's recovery arrangement. The actual dashboard
Blossom iframe passed profile selection, explicit denial with zero uploads, and
an approved local upload through the canonical signer. Physical companion
acceptance remains separate; this was a browser iframe test.

The operator requested a further UX pass informed by all existing guides. The
seven existing goal guides, help tree, shared walkthrough and onboarding patterns
were reviewed. The revised flow uses concise visitor-oriented multiselect cards,
an AIUI-first creation path, optional HTML/model controls, a saved-version preview,
explicit Save and continue, and contextual Help entries. The Home shortcuts now
respect the same dedicated guide routes as the Setup cards. The final polish is active on Framework. The production typecheck and build pass,
as do 23 focused tests. The deployed flow again passed real draft save, local
Blossom archive/readback, FIPS publishing and the mobile overflow check, with zero
external browser requests. It preserves original Setup visuals, a single main
Save and continue action, keyboard focus/scroll handling, and visible revoke
controls even when an already-published route is deselected.
No local Ollama service responded on Framework; live model generation remains
unqualified. No proprietary fallback was used or added.

New source work includes local Blossom website archives, explicit app-only guest
credentials, and an on-demand HTTPS check against exact published page bytes.
Guest tokens cannot authenticate to node login; scope/expiry are checked on each
request, and revocation affects subsequent requests, not established streams.
Only opted-in gated app manifests expose guest access. Persistent credentials use
serialized, atomic 0600 writes and refuse corruption/capacity without evicting an
existing device. HTTPS checks pin validated public DNS addresses, validate TLS,
refuse redirects/proxies and bound response reads. They are point-in-time checks
from the node, not proof of outside-device access or future certificate renewal.

Public-web projects can explicitly publish a FIPS upstream for an existing proxy
without selecting FIPS again. The confirmation still explains its FIPS visibility.
Automated frp enrollment/end-to-node TLS and selective local public Blossom assets
remain unfinished. Source validation and standalone routes must not be described
as acceptance of those features or of the complete dashboard journey.

The current dashboard production build and supported AIUI build both pass and
are activated on Framework. The original backend and full web tree remain
backed up for rollback. The selected
dashboard suite passed 36 tests; subsequent HTTPS UI coverage passed six tests,
and tightened Nostr signing/receipt coverage passed 12 tests. The latest combined
18-test run, TypeScript check and dashboard rebuild passed. Catalog drift is zero
(37 catalog entries, 64 manifests). Full isolated backend validation now passes
1,699 tests, zero failures and four explicit ignores. The focused app-gate run
passes 53 tests, and all three credential tests pass. The deployable backend build
passed. Its stripped deployment artifact SHA-256 is
`11e571a7636779d7a956f9e98dab951f19de12262cf89e5ea478cdc8ba864eae`.
Passing tests and the initial authenticated activation probe do not establish
complete live-node acceptance. The private catalogue signing ceremony remains
pending; six app-sharing policies have not yet been activated.

## Development evidence (2026-10-08, not release acceptance)

Latest addition: [Blossom candidate package and acceptance ledger](../apps/blossom/README.md).
Setup offers catalogue installation and skips that prompt for installed Blossom.
The candidate is built and protocol-tested on Framework, and normal installation
and the real HTTPS tab signer work. Further lifecycle acceptance is in progress.
The operator temporarily disabled dashboard 2FA for tests; restore it afterwards.
Nostr publication now includes a local
preparation/review step showing exact HTML, hash, identity, manifest and destinations;
upload and announcement require explicit consent. No public Nostr events or external
Blossom uploads have been performed. Local Blossom website-asset integration remains
outstanding. Earlier evidence below records its own point in development.

The isolated branch now contains versioned node-owned projects, multi-route
preferences, both Setup screens, local Ollama draft generation, sandboxed static
previews, revision restore and FIPS-only static publication/revocation. AIUI can
hand HTML to Setup for explicit import. Public HTTPS, Tor and Nostr adapters and
per-app grants remain outstanding; selecting a route does not enable it.

The File Browser Setup card has been removed as requested. Its catalog entry and
launcher remain intact. No installed applications were changed.

The backend compilation passed, including the supervisor snapshot repair. Eleven focused backend tests passed
through the isolated runner, including the actual publishing and FIPS interface
modules. The initial frontend typecheck and six publishing tests passed. Later
AIUI handoff checks subsequently passed: AIUI typechecking, dashboard typechecking,
and 30 bridge/import tests, including rejection of messages from another frame or
origin. The earlier combined dashboard run passed 52 tests. These are source
checks, not full application deployment acceptance.

Framework access and availability were confirmed by the operator. Read-only SSH
inspection identified framework-pt and its installed FIPS 0.4.1. Yaya access was
also confirmed; its reverse proxy has the existing archipelago.builders route.
The operator subsequently confirmed Yaya is free and explicitly authorized a
separate test route. The standalone production publisher driver ran on Framework
under `archy-publishing-smoke.service`, using only
`/home/archipelago/publishing-smoke`. The main backend was not replaced or
restarted. No wallet, channel, application data or DNS settings were changed.

Live checks completed:

- Two temporary static sites used FIPS ports 32000 and 32001. Yaya fetched the
  first over FIPS with HTTP 200 and the restrictive CSP intact.
- Restarting only the test publisher retained the sites. Unpublishing the first
  closed its listener and removed its rule while the second still returned 200.
- Temporarily removing the test state file closed the second listener and removed
  its allowance. Restoring the file restored the publication. This validates the
  repaired stale-snapshot failure case.
- NPM proxy host 9 routed only `free.archipelago.builders` to Framework's second
  FIPS site. Certificate 16 was issued successfully. Public HTTPS returned the
  expected page with normal certificate verification; `/rpc` returned 404 and
  `/../../etc/passwd` was rejected with 400.
- Unpublishing the remaining site left no website allowances or listeners. The
  proxy request timed out without returning the old page (not a claimed 404 or
  verified friendly error page).
- The temporary publisher was stopped; its empty owned firewall drop-in was
  removed and the FIPS baseline reapplied. Framework's main backend remained
  active. Test proxy host 9 was deleted; certificate cleanup is checked separately.

This proves the static serving module and the existing-proxy/FIPS path. It does
not prove dashboard RPC integration on Framework, full-node reboot recovery,
certificate renewal, automated gateway enrollment, Tor or Nostr publishing. The
test HTTPS setup terminated TLS at the operator's proxy; end-to-node TLS for a
new frp gateway is still separate outstanding work.

## Research links

- FIPS master `57bc5108f708258c67dfc713e98e6bbb5a828e95` (2026-10-07), latest release
  v0.5.2: https://github.com/jmcorgan/fips . Gateway forwards accept IPv6 targets;
  Archipelago already has a separate FIPS-to-IPv4 relay and an IPv6-capable AppGate.
- frp: https://github.com/fatedier/frp (Apache-2.0).
- nsyte: https://github.com/sandwichfarm/nsyte (MIT).
- NIP-5A: https://github.com/nostr-protocol/nips/blob/master/5A.md (draft).
- Mynymbox: https://mynymbox.io/domainregistration and
  https://mynymbox.io/docs?doc=domains/dns-records .

## Tor website adapter

Website publication uses a dedicated child Tor process with `SocksPort 0` and
`ControlPort 0`, explicit owned configuration, and 0700 identity/runtime directories
under `publishing/onions`. It does not regenerate app Tor configuration or restart
the system Tor daemon. Each onion forwards only to its own static listener on
127.0.0.1:32100–32131. Removing a website closes that listener before reloading
this owned process; the other onions and all private keys are retained. The
process exits when there are no published onion websites. No key wipe is part of
unpublishing. The UI distinguishes having an onion address from verified external
reachability.

A standalone candidate on Framework served the second temporary onion to Yaya's
Tor client with HTTP 200 and the expected CSP. After unpublishing the first onion,
the second still returned 200. Republishing the first retained its hostname; a
publisher restart also retained that hostname. The existing system Tor process
remained PID 1449 throughout these checks. A fresh external fetch after restart
and final cleanup are recorded below when complete.

Source validation now includes per-transport revoke isolation, Tor configuration
path/port constraints and shared connection preferences. All 12 isolated backend
tests passed; the latest selected frontend run passed 36 tests and typechecking.
The AIUI package typecheck passed separately. The final integrated backend check
passed. NPM test certificate 16 was successfully deleted after proxy
host 9; no test proxy remains on Yaya.

The fresh external fetch of the first onion after republish and publisher restart
returned HTTP 200 with the original hostname and expected page. Both test onions
were then unpublished and the smoke unit stopped. Only system Tor PID 1449
remained; no website listeners or owned FIPS drop-in remained. Both onion identity
directories were preserved. The final state-directory durability change passed
the 12-test isolated backend suite as well.

### Walkthrough layout correction — 2026-10-08

The publishing guides now use the same available width and step alignment as
GoalDetail, with the existing small glass action buttons throughout. Step
navigation and grouped actions align left with consistent wrapping and gaps.
The external-access guide no longer renders the entire app inventory. A native
searchable app dropdown offers only guest-enabled apps and reveals grant controls
after a valid selection; installations without eligible apps show a short empty
state and a Browse apps link.

The UI-only update was deployed to Framework with the previous UI retained at
`/opt/archipelago/web-ui.before-guide-layout-uat`. Production build and ten
walkthrough tests passed. Live browser comparisons at 1440px and 390px confirmed
matching original-guide widths/alignment and no horizontal overflow. Management
and wallet services were not restarted for this update.

### Signed private catalogue and guest access — 2026-10-08

After the operator signed the private candidate, verification against the pinned
release root passed locally and on Framework. The node accepted the exact signed
catalogue through `ARCHY_APP_CATALOG_CANDIDATE`; wallet process identities and all
app container IDs/states were unchanged. This remains a private UAT catalogue,
not a published release. The owned override is
`/etc/systemd/system/archipelago.service.d/50-external-access-uat-catalog.conf`;
remove it after the reviewed catalogue release or rollback to restore normal
catalogue refresh. The preceding cache is retained in
`~/external-access-uat/catalog-before-private-candidate.json` on Framework.

Framework now reports guest eligibility for Home Assistant, Immich, Jellyfin,
Nextcloud, PhotoPrism and Strfry. Actual-node checks with a temporary Home Assistant
grant passed anonymous challenge, bearer and browser-cookie access, denial at
Immich, rejection for dashboard login, and revocation of both bearer and cookie
access. One-hour expiry metadata was checked; elapsed expiry remains covered by
unit tests, not a one-hour live wait. The temporary grant was removed. No Nostr
events were posted and no other app data was changed.

### Controlled Framework reboot — 2026-10-08

The operator confirmed physical recovery access and authorized remaining
qualification. A fresh native LND snapshot and static channel backup were retained
privately on the node before reboot; no pending HTLCs were present. A changed boot
ID confirms the full reboot. Native wallet identity, channel set, on-chain and
channel balances matched exactly afterward, and LND reported chain sync without
manual unlock/restart. Backend and signed-catalogue hashes matched. All app
running/stopped states, exact publishing/project/archive state, FIPS address, onion
address and guest eligibility survived. Public HTTPS and Tor returned the exact
synthetic page. Guest scope, dashboard denial and revocation passed again.

Physical companion acceptance remains OPEN: the operator found the native
`datalist` app picker invisible in the companion, and Blossom blank after choosing
an identity. These are tracked as current regressions, not successful companion
acceptance. The picker replacement uses an in-page glass menu; the tab signer
must copy public identity fields instead of passing a Vue reactive Proxy through
postMessage. Blossom also requests the canonical chooser once on opening and
disables the unrelated generic NIP-98 web-app login. Deployment and actual-device
retest are required before closing these reports. Operator will restore 2FA after
the remaining installer/signer tests.

### Selective public archive implementation — 2026-10-08

Each FIPS/public-web or Tor publication can separately expose its exact archived
HTML snapshot at `/<sha256>`, only after an acknowledged action verifies the
local archive receipt matches the published bytes. This is a read-only
hash-addressed snapshot route, not a publicly opened Blossom app or upload API.
GET/HEAD and CORS reads serve only the selected immutable bytes with sandbox and
attachment headers. Unknown hashes, listings and uploads remain unavailable.
Later drafts cannot change the served bytes; publishing an update resets archive
sharing, and removing sharing does not unpublish the page or remove private files.

The focused harness and isolated platform suite each passed 12 publishing tests.
The candidate backend is deployed on Framework with its preceding executable and
publishing state retained under `~/external-access-uat/`. Live trusted HTTPS
readback matched the exact snapshot; unknown hashes/list/upload returned 404.
Revocation returned the selected hash to 404 while the website still served.
The synthetic archive was unshared after the test. No external replica or Nostr
announcement was made. UI deployment and live UI acceptance are still pending.

Stored Publication now has an optional `public_archive` field. Before rolling back
to the preceding binary, account for its deny-unknown-fields parser: retain the
latest state and migrate only this field away, or restore the pre-test state only
if no user changes would be lost. Do not blindly restore an older project file.

### Companion corrections deployed — 2026-10-08

The final dashboard build includes the in-page searchable glass app picker and
the tab signer's explicit cloneable identity fields. Sixteen UI tests passed,
including a structuredClone regression test using a reactive picker identity.
Live touch-browser checks at 390px and 1440px opened all six choices, filtered to
Immich, selected it and exposed the grant controls without horizontal overflow.
The normal Blossom lifecycle rebuilt/restarted the private candidate with
`data-app-id="blossom"`, `data-no-nip98` and one automatic chooser request. Its
previous image and build context are retained for rollback. The live direct app
window reproduced the blank frame before the signer correction; after deployment,
automatic selection returned to the visible file page, the signer iframe was
hidden, no generic login request occurred, refusal prevented upload and explicit
approval stored the synthetic file. Actual phone confirmation is still pending.
The archive UI is deployed with backend capability gating; UI tests cover fresh
consent on snapshot changes and independent revocation. No public release made.

### AI provider direction — 2026-10-08

The operator selected Routstr as the default, with Claude and OpenAI API keys as
alternatives, and deprioritized Ollama. The isolated publishing branch merged the
already accepted provider setup through commit `83ba98ab`, preserving its history.
Website design now opens that trusted dashboard setup and its existing ecash
Receive flow directly. New/missing provider settings default to Routstr; saved
provider selections remain unchanged. AIUI lists Routstr first. The Ollama draft
form was removed from the walkthrough; its compatibility RPC remains available.
The failed Framework Ollama test installation was removed through normal package
uninstall with `preserve_data: true`; no model was downloaded.

Funding and spending permission remain separate. Opening setup/top-up does not
change the allowance, send a payment, start inference, or publish content. API
keys use the existing private node credential store and are not passed to AIUI.
Focused provider/publishing tests and production qualification are in progress;
these changes are not yet deployed or publicly released.

Provider-focused validation: 20 dashboard/setup/signer tests, 22 AIUI provider
and generation tests, and 22 trusted bridge/integration tests pass. Initial
publishing tests required an AI-connection component stub for their isolated
mounts; the provider default test now checks initial state before the suite's
explicit Claude selection. The full backend suite and production builds remain
pending. Framework still runs the preceding candidate; its management service is
active and no Ollama container exists after cleanup.
The funding modal's Scan action now opens the existing wallet scanner and returns
to funding on close; six focused connection-modal tests pass after that wiring.
The first dashboard production build passed; it will be rebuilt for this final
scanner wiring before deployment. AIUI and isolated backend builds are ongoing.

The first full isolated backend run passed 1,717 tests with one outdated default
selection assertion failing (four explicit ignores). The assertion expected an
unconfigured node to choose Claude. Updated coverage distinguishes the new Routstr
default from a saved legacy Auto choice, and the Routstr adapter now rejects zero
allowance before even discovering providers. A rerun is required; no passing full
suite or deployment is claimed yet. Final dashboard and AIUI production builds
have both passed.

Final isolated backend rerun: **1,719 passed, zero failed, four explicit ignores**.
This includes the default Routstr zero-allowance stop and saved Auto behavior.
The deployable backend build is in progress; Framework deployment remains pending.

### Routstr-first Framework deployment — 2026-10-08

The private provider backend built successfully and is active on Framework:
SHA-256 `6002af4c131545e9c93c21a65e31ef8719e6beb2682d47825d0ac95ee724e12a`.
Authenticated publishing health passed. Native Bitcoin/LND process IDs and start
times were identical before and after the management restart. The prior backend
is retained as `~/external-access-uat/backend-before-provider-setup`.

Live qualification found a browser-history race while replacing the connection
modal with Receive: closing the first panel consumed a history entry after the
new panel opened, immediately dismissing it. AI setup now owns one history entry
across connection, funding and scanning. Other BaseModal/Receive callers retain
their default history behavior. Ten modal/connection tests and nine receive tests
pass. The final dashboard production build passed and is deployed; index SHA-256
`d53ef48ec61f0c947df75ca57af9dd44ccf1c8a6db13ff61931b73e0dd0df1d3`.
UI backups are `/opt/archipelago/web-ui.before-provider-setup-uat` and
`/opt/archipelago/web-ui.before-provider-handoff-uat`.

Actual Framework Chromium checks at 390px and 1440px pass: Routstr-first setup,
Claude/OpenAI inputs, empty password fields, direct ecash receive/Lightning
address display, repeated top-up opens, close/return and browser Back. No horizontal
overflow and no provider-setting, allowance or publishing mutations occurred.
The existing Claude credential is recognized by status; its value was never read.
No paid inference, new API-key save, or public content publication was performed.
Real paid-provider responses and physical companion confirmation remain separate
acceptance items. The earlier routing, Nostr, 2FA-restoration and release gates
remain open; this is a private Framework UAT update, not a general release.
