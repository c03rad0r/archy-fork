# Archipelago 1.9.0 acceptance

The operator changed the release target from 1.8.23-alpha to **1.9.0**. This file retains its historical path so handoff links remain valid.

Status: PREPARING. Do not publish until artifact checks and offline signatures pass.

## Scope

Durable Lightning paid-file entitlements and delivery retries, atomic buyer
ownership, compact persistent upload progress/cancellation, mobile transaction
filters, Immich inventory and retired-app uninstall, Portainer duplicate-network
migration, channel-close fee selection, and shared app-search clearing.

Angor Indexer and the optional Angor Relay remain in the signed catalog. They
were already included in 1.8.22; full-chain acceptance still awaits dev Bitcoin
initial sync. Do not advertise full-chain verification as complete.

## Validation carried into preparation

- Candidate deployed on dev and Framework with matching backend/dashboard bytes.
- Native Bitcoin/LND and Framework Immich container IDs/start times preserved.
- Six live desktop/mobile app-search cases passed.
- Actual uploads verified exact bytes, original destination after navigation,
  compact progress geometry and cancellation. These are browser checks, not
  physical companion acceptance.
- Framework seller's settled invoice recovered to a mode-0600 entitlement and
  remained paid across another manager restart; mismatched item rejected.
- Framework CryptPad stale inventory removed with data preserved; removal
  persisted after management restart. Dev normal package.uninstall RPC also
  completed with preserve_data=true and zero cleanup errors for the retired ID.
- Cooperative-close fee selector tested with intercepted requests; no real
  channel was closed. Real payment recovery never sent another payment.
- Shorty's Mempool inspected read-only; frontend/API healthy for over two weeks,
  systemd zero restarts. Operator reported normal status after their update.

## Follow-ups accepted for release preparation

The operator authorized release preparation after available tests pass.

- Actual failed-purchase delivery still requires buyer/file confirmation. The
  located invoice's source file is missing from Framework's recorded paths.
  Seller settlement recovery does not prove buyer delivery.
- Physical companion upload diagnosis needs the affected device/route.
- Framework rendered inventory/uninstall checks need dashboard second-factor
  authentication; SSH/runtime and dev API acceptance are recorded separately.
- Angor full-chain indexing awaits Bitcoin IBD.
- Prior Primal automatic-comment and lost-response ecash receipt follow-ups
  retain their documented boundaries; this release does not claim to fix them.

## Final release checks

Pending full release gates, versioned builds, exact artifact deployment, ISO
payload/boot acceptance, pinned-root signatures and publication verification.
No universal or future-failure guarantee is implied by these tests.

## Required NPM certificate gate

The release owner acknowledged `docs/npm-certificate-handoff-20261001.md` in
`/tmp/npm-release-handoff-ack.txt`. Shorty's npm-8 issuance and HTTPS health 200
were reported by the operator; durable data-path resolution, safe host routing,
automatic certificate renewal/reload, and the handoff acceptance matrix remain
required before publication. Earlier authorization does not waive this new gate.

## Urgent public dashboard exposure gate — 2026-10-01

Investigator reports the Angor relay hostname reached the default Archipelago
login because its certificate existed without a corresponding host-nginx route.
The investigator owns the immediate Shorty nginx repair; the release session
will not modify that configuration concurrently. Exact final evidence is pending.

- [ ] Unknown public HTTP Host / TLS SNI and direct public-IP requests cannot
      expose the dashboard, login assets or RPC, including IPv6 and any trusted
      reverse-proxy/tunnel path. Test spoofed forwarding headers explicitly.
- [ ] LAN/private/tailnet dashboard access remains available as intended.
- [ ] Public HTTP ACME challenge access survives those restrictions.
- [ ] NPM host creation/edits automatically propagate HTTP/TLS routing.
- [ ] Relay hostname serves the intended relay and WebSocket upgrade using its
      correct certificate; certificate existence is not route acceptance.
- [ ] These protections survive manager/nginx restart, renewal and OTA/ISO.

## Additional isolated security checks — not deployed

The management source-guard prototype passed five unit checks including legacy
address-specific HTTPS, idempotence, backup permissions and syntax/reload rollback.
An actual nginx instance in a private network namespace passed 120 negative
HTTP/TLS cases across IPv4/IPv6, raw/unknown/spoofed Host/SNI and forwarded headers,
including POST RPC and WebSocket upgrade requests. Exact ACME token reads,
private LAN/tailnet/ULA access, named public HTTP app routing and reload passed.
These are scoped checks, not complete fleet, trusted-tunnel, reboot or artifact
acceptance. Shorty's containment was not modified.

The NPM storage/routing prototype passed eight focused tests: fresh/flat/nested/
custom mount selection without mutation, ambiguity/wrong-mount refusal, corrupt
or uninitialized database preservation, duplicate/missing mounts, deleted/disabled
host exclusion, domain injection rejection, and rejection of mixed public and
loopback listener bindings. Automatic application/migration and end-to-end NPM
security/routing remain unfinished and block release.

Final Angor handoff was read and acknowledged in
`/tmp/angor-final-handoff-ack.txt`; see `angor-client-acceptance-20261001.md`.
One complete project flow is investigator-verified; 34 original announcements
remain unrecovered from queried sources. Full recovery acceptance remains open.

## Additional Angor public explorer gate

- [ ] Public indexer hostname serves Mempool UI and its assets/deep links/live
      WebSocket updates while preserving Angor API/CORS/broadcast/readiness.
- [ ] Existing stack reused; no duplicate Mempool app/database and no management
      or Bitcoin RPC exposure.
- [ ] Documentation/catalog/runtime metadata and exact OTA/ISO reflect the tested
      implementation; repeat official-client browser acceptance afterward.

Confirmed official deployment guide describes a shared frontend/API origin.
Current adapter 1.0.1 is API-only; this requirement is not yet implemented.

## NPM real-image integration progress

Disposable real NPM API tests passed for both flat and legacy nested `/data`
layouts: initial account/host creation, multiple domains, custom location,
forwarded-client spoof rejection, exact challenge file access under forced HTTPS,
trusted TLS and WSS upgrade/frame, certificate replacement/reload, password and
network access lists, disable/enable/delete propagation, and restart with the
original database and account authentication preserved. Local fixture certificates
are not public Let's Encrypt staging issuance/renewal evidence; that gate is open.

Certificate replacement initially failed because the updated bridge could not
complete the upstream TLS request after replacing the fixture certificate.
Reloading and validating NPM's own TLS listener on certificate fingerprint changes,
as well as host nginx, resolved the test. Rollback/retry unit coverage was added.

The initial dev guard deployment changed only the inactive sites-available copy;
private HTTP 200 and unchanged entry bytes were insufficient acceptance evidence.
A later public-ingress-marker probe caught this: it incorrectly returned 200.
The resolver now chooses the active sites-enabled copy or resolves its symlink
without replacing the link. Guard backups live outside nginx include directories.
After the correction, live private requests return200 and marked requests 404.
No app was restarted. Direct-backend protection still awaits its candidate build.

Angor candidate UI was exercised against the actual dev Mempool stack in a
throwaway gateway: desktop/mobile rendered, zero failed JS/CSS assets, one
WebSocket connection each. The gateway was removed afterward; this is candidate
integration evidence, not a published or permanently installed app update.

### Same-node NPM networking correction

Read-only inspection of the production NPM namespace reproduced HTTP 502 for its
own configured indexer route. Host requests to the LAN upstream returned 200;
requests from the existing pasta namespace to that same LAN address were refused.
A disposable container on the proposed `slirp4netns:allow_host_loopback=true`
network returned 200 for both the LAN upstream and `host.containers.internal`.
No production NPM/nginx configuration or container was changed in this check.

The candidate now declares this network in the manifest and first-boot path,
with explicit Quadlet/API support and legacy drift detection. The Podman API
`network_options` shape was checked against `podman generate spec` locally.
The real NPM integration fixture now uses the proposed network and a LAN-bound
same-node upstream, rather than placing both fixtures on one custom bridge.
Flat-layout integration passed namespace reachability, host routing, verified
TLS/WSS, ACME file access, certificate replacement/reload, custom routes, password
and IP ACLs, spoof rejection, host lifecycle and NPM restart with preserved DB.
The earlier loopback-only fixture failed because this machine resolves the host
alias to its LAN address; its bind was corrected before repeating the test.

The latest isolated nginx guard test passed 120 public IPv4/IPv6 negative cases
plus private access, ACME, proxy-marker rejection and reload. Python guard/bridge
regressions passed 23 tests, including exact emergency-route retirement, failed
migration rollback and preserving operator-modified routes. Backend compilation
and new direct-listener tests are still pending. Required public staging renewal,
actual upgrade/reboot, yaya and exact OTA/ISO acceptance remain open.


### Active nginx site layout regression

The dev node has a regular `sites-enabled/archipelago` file, not a symlink to
`sites-available`. Both the guard and ACME resolver now select the active file.
Unit coverage verifies copied sites and symlink targets, preserving inactive
operator copies and the links themselves. Nginx configuration backups must not
be created inside `sites-enabled`, whose wildcard include would load them.
The active guard was applied on dev, with private HTTP 200 and public-ingress
marker 404 verified after reload. Shorty remains untouched. Do not count the
initial inactive-file edit as a security deployment pass.

### LoRa flasher added to release gates

Both dev and Framework lack the esptool executable and Python module. The
backend invokes a bare `esptool` and retries even process-spawn failures. The
shell updater installs it opportunistically, but the OTA runtime tool list
omits it. Candidate packaging adds a pinned self-contained `archy-esptool`
with its ESP32-S3 stub to mandatory OTA/ISO payloads. Candidate preflight runs
before stopping the radio; retries are limited to recognized serial transport
failures. Concurrent flash registration now uses one exclusive lock.

CP2102 USB identity does not uniquely identify a Heltec V3. The candidate removes
that unsafe inference from backend and UI and requires explicit board selection.
Actual board models were requested before firmware writes. Dev exposes one
CP2102 serial radio. Framework SSH works but currently exposes no mesh-radio,
ttyUSB or ttyACM port. No radio has been erased or flashed in this investigation.
Physical MeshCore UK acceptance on both nodes remains required and pending.

Dev connection diagnosis: the failed flash stopped its listener before spawn
failed and left the enabled setting true. A read-only protocol probe identifies
the existing radio as Reticulum/RNode. An explicit listener disable/enable restored
`device_connected: true` on `/dev/mesh-radio`, without flashing or native-service
restarts. Candidate configure logic now compares desired enabled state with the
actual listener task, including stopped/finished handles; same-settings reconnect
is covered by a new isolated regression. Probe/configure and flash registration
are coordinated to prevent concurrent serial owners.

The packaged `archy-esptool` build passes in a clean environment, including loading
the actual ESP32-S3 stub through esptool's own loader. It is installed and self-tested
on dev and Framework; a compatibility command supports their current backends.
This verifies tooling availability, not physical flashing. Framework's USB sysfs
inventory shows its hub/storage/network/keyboard/display devices but no serial
radio. Board confirmation and Framework radio detection remain pending.

Additional Podman API acceptance: a disposable API service created a container
with the candidate `netns`/`network_options` payload. Its effective generated
specification preserves `allow_host_loopback=true`. The normal inspect network
mode omits options for API-created containers, unlike the CLI-created case;
candidate drift detection now consults the effective specification before
recreating such a container. Added a regression against repeated recreation.
The probe container and temporary API service were removed. The real isolated
nftables tunnel regression also passed (NPM peer port and LND remain separate).

### Latest acceptance checkpoint: radio connection and release status

The complete frontend suite passed: 143 files, 1,159 tests. Type checking and
both new radio setup tests also passed. The final isolated backend build/test
run is still pending; the previous run exposed an NPM/Router port collision,
which was corrected by assigning NPM's local HTTP listener port 8088. Do not
report the previous run as fully passing or the final run as completed.

Yaya's identity has been confirmed. Its existing managed web-tunnel drop-in
clears manifest port publications and supplies private tunnel HTTP/HTTPS ports
plus the local admin port. This would suppress the candidate bridge's new
loopback HTTP/TLS listeners. Migration must preserve the working tunnel/site,
add the required local listeners, validate the managed firewall/lifecycle,
retain custom overrides, and cover repeat upgrade and rollback. The current
bridge rejects non-loopback listeners, so the supported tunnel topology also
needs explicit qualification. No tunnel or NPM runtime change was applied to
yaya during this diagnosis. Its management source guard was applied and tested:
private dashboard HTTP 200, public-ingress-marked request 404.

Dev radio connection has been restored on its existing Reticulum firmware.
Framework still does not enumerate a USB serial radio. Both nodes now have the
self-tested packaged flasher, but physical MeshCore UK flashing, post-flash
handshake and reconnect acceptance remain open pending board identification and
Framework USB detection. No device firmware has been written.

OTA, app catalog and raw ISO publication remain held. Outstanding acceptance
includes NPM migration/renewal/reboot and exact artifacts, end-to-end delivery
of the reported paid file, physical companion uploads, full Angor discovery
(the 34 missing original announcements), radio hardware tests, and the final
dev/yaya candidate deployment checks. Retained tasks above remain in scope.

### Dev Heltec V3 physical flashing result

Full 8 MiB pre-flash backup saved privately with mode 0600 and checksum. After
removing the confirmed stale radio sidecar, the exclusive read completed.
The normal mesh.flash-device RPC then flashed the official Heltec V3 Companion
USB v1.17.1-d929643 merged image successfully (100%, no error). The image's
size and SHA-256 were checked against the official GitHub release metadata.

Independent serial protocol queries confirmed model Heltec V3, firmware
v1.17.1-d929643 and actual RF readback: 869618 kHz, 62500 Hz bandwidth, SF8,
CR8 (EU/UK Narrow). This is device readback, not merely saved host settings.
The listener was then re-enabled and reported connected as meshcore. No wallet
or native Bitcoin/LND service restart was used. MeshCore remote reboot is not
supported by the current API; that attempted check returned an explicit error.
Physical unplug/replug and communication to Framework remain pending.

Live qualification additionally found incorrect binary DEVICE_INFO/SELF_INFO
parsing and a stale host-side RF-applied marker after full-chip flashing.
Candidate changes decode the current official binary layout, query the actual
firmware version during initialization, and invalidate the RF marker after a
successful flash. The dev marker was backed up and cleared before provisioning;
the independent readback above confirms settings applied. New parser, startup
cancellation and marker lifecycle regressions are queued/running; the prior
1,647 passing tests do not cover these later changes.

Framework's V4 official USB image has been downloaded and verified. Despite the
operator confirming it is plugged in, repeated sysfs/device checks show no
ESP32 USB or serial port. USER/BOOT plus RST bootloader entry was requested;
Framework has NOT been flashed and the two-device acceptance remains open.

### Operator deferral and latest qualification

Operator explicitly deferred Framework radio/hardware work and instructed us to
continue all other release tasks. Framework V4 flashing, USB reconnect and
radio-to-radio acceptance remain UNVERIFIED / OPERATOR-DEFERRED; this is not a
pass. Do not request further Framework radio operations unless needed and the
operator resumes that work. Dev V3 acceptance and durable fleet fixes remain.

The final radio backend suite passed 1,650 tests, zero failed, four ignored,
including sidecar-startup cancellation, current MeshCore metadata parsing, and
post-flash RF-marker invalidation. Frontend baseline remains 1,159 passed.

Correction to the earlier yaya tunnel concern: direct inspection of the active
Quadlet and all drop-ins confirms web-tunnel.conf ADDS private tunnel ports; it
does not contain an empty PublishPort reset. The earlier statement that it
cleared manifest listeners was incorrect. The new loopback publications therefore
coexist declaratively without editing that working tunnel drop-in. The bridge
validator now permits only the known HTTP/TLS tunnel ports bound to a currently
assigned RFC1918 address on an actual WireGuard interface named wg-web; it still
requires a separate loopback upstream, and rejects wildcard/public/unassigned
bindings and any admin-port exception. Python bridge/guard tests: 27 passed.
Actual yaya candidate upgrade and public route acceptance remain pending.

### NPM public certificate and restart qualification checkpoint

- Main backend: 1,651 passed, zero failed, four explicitly ignored hardware /
  external integration tests. Container runtime library: 80 passed, zero failed.
- Bridge/management guard Python suite: 27 passed. Shell syntax and actual ISO
  overlay-content test passed.
- Candidate validator inspected yaya's real runtime/WireGuard interface and
  accepted its existing web tunnel publications while selecting proposed
  loopback HTTP/TLS upstreams. This was read-only, not a runtime upgrade.
- Existing yaya public HTTP ACME route returned the exact random token body
  written inside NPM. Lets Encrypt STAGING initial issuance and renewal dry run
  succeeded using separate temporary account/config/work/log storage. Current
  production certificate and host records were not replaced. Public site HTTP
  and trusted HTTPS retain their authentication requirement (401).
- First renewal harness timed out while Certbot used a 292.7-second randomized
  delay; the remote log confirmed successful simulated renewal. A deterministic
  rerun with --no-random-sleep-on-renew returned exit 0 and success confirmation.
  Only the temporary staging directory was removed afterward.
- Real disposable NPM nested-layout test completed: namespace LAN upstream,
  API host creation, custom locations, client-IP spoof rejection, exact ACME
  token, trusted TLS/WSS, certificate replacement, password/network ACLs,
  enable/disable/delete, and restart with retained DB. Latest restart took 7.6s.
  Earlier rerun exceeded the fixture's 90-second restart window; the test now
  uses the manifest's 180-second budget and records both route/admin statuses
  and container state on failure. Do not erase that earlier observed failure.
- Cleanup was hardened to continue cleaning other fixtures after a timeout.
  Two early test runs passed functional assertions but failed cleanup; their
  leftover disposable containers/networks were explicitly removed. The latest
  full run exited successfully.

Legacy non-Quadlet repair now retains the old container for rollback, restores
it after replacement failure, preserves its exact image and environment values,
and waits for HTTP API readiness. Environment values use an exclusive mode0600
file cleaned on drop, not argv or the host process environment. Invalid/multiline
entries fail before stopping the original. An occupied rollback slot is preserved
for review rather than deleting an unknown container. Live interrupted-migration,
additional operator-override and rollback acceptance remain OPEN; unit success
is not proof of those deployment paths.

The deployment backend and frontend build are in progress. Full candidate dev/
yaya upgrade acceptance, reboot, exact signed OTA/catalog and booted raw ISO
remain open. Framework radio is operator-deferred, not passed. The original paid
file bytes, physical companion upload and 34 missing Angor announcements remain
separately tracked.

### Further completed acceptance

The complete disposable NPM test now includes a deliberately failed replacement
with an occupied host port. Restoring the retained container preserved its exact
container ID, database, configured hosts and authenticated API access; the test
exited successfully and cleaned its fixtures. This verifies the Podman rollback
mechanism, not yet the full installed backend's legacy-repair entry point.

Dev frontend build completed and was deployed with a separate rollback backup.
Served production Transactions layout passed at widths 390 and 1440: transparent
background, no image/blur/shadow/border, nowrap and exactly one row. Mobile rail
is 324px wide with 419px scroll content. Backend candidate compilation is still
in progress, so the latest backend changes are not yet deployed.

Dev Bitcoin remains unpruned and in IBD (observed block543676/header969495,
verification progress0.2284). This is not full-chain Angor acceptance. The operator
identified the seller of the failed Lightning purchase as Amish Paradise.
On 2026-10-02 the operator confirmed the other tester received the file and
accepted closure of this individual recovery. Seller access is no longer needed
for that recovery. This does not establish that the candidate fix delivered it;
durable settlement/delivery regression acceptance remains required before
release. No additional payment was made. Earlier references in this document
to missing original purchase bytes are superseded by this operator acceptance.

### Read-only Shorty migration preflight: remaining ownership conflict

Preflight found both flat and nested NPM databases. The live container's explicit
/data mount identifies the active one, so the candidate resolver now uses that
verified mount (or its saved validated receipt after a managed stop), preserves
both databases, and still refuses multiple databases without an authoritative
selection. Python coverage verifies both explicit choices and unchanged bytes.
This helper update occurred after the deployment binary build started; a final
release rebuild must include it. Do not claim the in-progress binary contains it.

After resolving storage, the two Angor emergency routes match the exact known
handoff templates. Another existing file, shop-btcpay.conf, conflicts with an
enabled NPM record: the manual route supplies HTTPS using certificate10, while
the NPM host currently has certificate_id0 and SSL forcing disabled. The manual
route and NPM record cover the same two public names and backend web port. Blindly
retiring the route would break its HTTPS. No database, host, certificate, route
or runtime was changed on Shorty. The bridge correctly refuses this ownership
conflict and now names its configuration file in the diagnostic. Align TLS/route
ownership and verify the public shop before retiring that manual configuration;
Shorty's full migration acceptance remains OPEN. Preserve live containment.

### Candidate deployment and additional real-upgrade ACME regression

The operator explicitly deferred Framework's physical radio investigation and
requested continuation of all other work. Framework radio remains unverified.
Dev and yaya now run the backed-up unpublished backend candidate SHA256
4c47269b3480ca0362df18dae160c073a19ea33507e04cacdad5b96837990ca6 and production
frontend index 3099c4ba44528a4a4c524f9a26159414558efa16abdd12cc7bfd64376cbb6089.
Both management health checks passed; native Bitcoin/LND container identities
and start times were unchanged. Yaya public site retains trusted TLS and its
401 authentication requirement; NPM admin API200, private dashboard200 and
public-ingress-marked dashboard404. The first yaya staging attempt stopped
before binary replacement because rsync was absent; deployment now uses Python
copying without that dependency and completed successfully.

Actual startup exposed an additional regression: the canonical nginx template
had only HTTP ACME, while the bridge demanded two locations. Startup rewrote the
previously repaired config and the bridge rejected it before fixing the nested
root. Source now adds HTTPS ACME to the shipped template and migrates the exact
recognized legacy default-server layout; custom/ambiguous layouts still fail
closed. The missing-token route must return404 rather than the dashboard SPA.
28 Python checks passed. The real isolated nginx suite now starts from the
legacy missing-HTTPS layout, applies the migration, and passes all120 public
negative cases plus HTTP/TLS exact-token, private-access and reload checks.

Applied the latest helper and its atomic ACME-only repair to yaya: actual token
written inside NPM returned exact200 over host HTTP, host HTTPS and public HTTP;
missing tokens returned404 for allthree. Public site trustedTLS/auth preserved.
Local self-signed host HTTPS was tested with certificate verification disabled;
public site HTTPS used normal certificate verification. Shorty was not modified.
The running backend still embeds the older helper/template; final rebuild is
REQUIRED before restart/reboot/persistent upgrade acceptance can pass.

The prepared unsigned catalog validates with zero metadata drift and trusted
registry hosts. Its only changed entries are NPM and Angor indexer1.0.2. It has
not been signed, installed or published. Yaya's current signed catalog retains
NPM's old pasta network/tunnel-only HTTP+TLS listeners; full NPM runtime/bridge
migration acceptance awaits the reviewed signed catalog. Do not mistake the
backend/UI deployment or ACME-only fix for completed catalog migration.

### 2026-10-02: rebuilt candidate deployed; private catalog qualification prepared

Release-profile candidate build completed successfully. SHA256:
af0648ad4ef8b6183d3c1ff485721fa40b12bb730c6e0322bc5f209ed06fce39.
Focused bootstrap tests:10 passed. Full isolated backend rerun:1651 passed,
zero failed, four explicit hardware/external ignores. Python guard/bridge28
passed again. No new source changes occurred between these checks and deployment.

Deployed this rebuilt backend on dev and yaya, with private previous-binary,
nginx and native-container baselines. Both manager health checks passed. A later
post-startup comparison confirmed Bitcoin/LND identities/start times unchanged.
The installed bridge helper now matches latest source bytes after restart.
Private UI200, marked-public HTTP/HTTPS404 and missing HTTPS challenge404 passed.
Dev HTTPS intentionally binds its LAN/WireGuard addresses, not127.0.0.1; an
initial loopback probe got connection refused, corrected to the actual listener.
This was a test-address error, not a product outage. Local self-signed HTTPS
checks skip certificate verification; public-site TLS checks use normal trust.
Full-machine reboot qualification is still pending.

Prepared a fresh candidate catalog with only NPM and Angor indexer entries changed.
Metadata drift0; registry trust check passed. Unsigned SHA256:
5801309bf21d3f6fd03ce5702d68b383a518f734092bf265f5e0ad243095a25e.
NPM's new manifest is capability-gated by runtime-migration-backup-v1; older
nodes retain the original manifest. This candidate is for private qualification,
not fleet publication. An operator-only hidden-input signer validates the exact
catalog and binary hashes, checks the pinned release root and restores the
unsigned original on failure. Its noninteractive refusal was tested. Signature
is required before testing through the nodes' normal trusted-catalog path.
No catalog, app image, OTA or ISO was published. Framework remains deferred;
all other open requirements retain their previous status.

### Signed catalog and private qualification selector

The operator signed the qualification catalog. Cryptographic release-root
verification passed locally and on yaya; after removing signature envelope fields,
its contents exactly match the reviewed unsigned candidate. No publication.
The catalog is staged under /var/lib/archipelago/qualification on both test nodes,
with previous catalog/app metadata and container identities privately backed up.

Normal mirror loading deliberately forces the public origin first, so simply
prepending a private mirror cannot reliably test an unpublished candidate.
Implemented ARCHY_APP_CATALOG_CANDIDATE as an explicit absolute-file selection:
requires an anchored release-root signature, validates before cache replacement,
retains exact signed bytes, does not alter mirrors/trust, and fails without
public fallback when the selected file is invalid. Added unsigned, tampered,
wrong-key, malformed, missing, oversized, relative-path, valid and idempotent
coverage. Full isolated backend suite:1653 passed,0 failed,4 explicit ignores.
The optimized selector build is still in progress; selection is not enabled yet.
See docs/candidate-catalog-qualification.md for activation and mandatory removal
once the tested public catalog is available. Do not leave test nodes pinned.

Yaya NPM preflight:8088/8444 are free, active public host has no conflicting
host-nginx ownership, database tables and certificate-file hashes saved privately.
No Shorty mutation. Dev public Angor reference acceptance passed TLS/WSS, exact
funding commitment, official Explore and detail/statistics again; this still
checks only the known original project, not all35. Dev Bitcoin continues syncing
(reported sync_progress approximately0.307); full-chain acceptance remains open.
Current tested hardware product codes:dev20CLS7S900 and yaya20CLS6BH00, both Podman
5.4.2; kernels6.12.74+deb13+1-amd64 and6.12.107+deb13-amd64 respectively. Framework
remains deferred. No Docker or new ISO-boot acceptance is implied.


### Signed qualification deployment and legacy upstream compatibility

The optimized candidate selector build completed, SHA256
`9cc9271ee1b4f8f13d798193cef97c1e4304a89a240f11f851eb71568bd105e1`.
Both development acceptance nodes now run it with the exact root-verified private
catalog. The isolated backend suite passed 1,653 tests, zero failures, four
explicit ignores. This is unpublished qualification, not a release.

Actual NPM migration exposed an additional regression that the LAN-upstream
fixture missed: a saved site uses pasta's former host gateway `169.254.1.2`.
Default slirp's gateway differs, so the request timed out from inside NPM even
though admin readiness passed. A disposable container verified the same saved
upstream with `slirp4netns:allow_host_loopback=true,cidr=169.254.1.0/24`.
A temporary qualification Quadlet drop-in now selects that network, using an
empty `Network=` reset before the replacement to avoid multiple network modes.
The existing site's trusted public HTTPS authentication response is restored.

Post-repair checks passed: request from NPM's actual namespace; exact saved user,
host, certificate, ACL and settings rows; unchanged certificate bytes; private
migration backup and prior unit; unchanged unrelated container IDs/start times;
retained WireGuard tunnel ports; host bridge completion; private dashboard200,
marked public HTTP/HTTPS404; missing challenge404; exact challenge body from
inside NPM over local HTTP/HTTPS and public HTTP. Native Bitcoin/LND identities
and start times remain unchanged.

**Release blocker:** integrate and test legacy gateway compatibility in all
supported runtime paths and signed manifest, including fresh/upgrade/restart
cases and LAN/host.containers.internal upstreams. The current signed candidate
alone is insufficient. Temporary user-unit drop-in
`nginx-proxy-manager.container.d/90-qualification-host-gateway.conf` must be
removed after the corrected managed configuration is verified. Do not remove
it before then or claim the migration passed without it. Candidate catalog
service selectors also require the cleanup described in
`docs/candidate-catalog-qualification.md` after final publication.


### Development Angor candidate update

The supported `package.update` RPC selected the private signed catalog and
upgraded only `angor-indexer` to the locally built 1.0.2 image. The actual dev
endpoint rendered the Mempool explorer at widths 390 and 1440 with zero failed
JavaScript/CSS requests and one WebSocket connection each. All unrelated dev
containers retained their exact IDs and start times. This verifies the local
explorer presentation, not complete blockchain indexing or recovery of the 34
missing original Angor project announcements. Bitcoin remains in IBD.

The repaired NPM namespace also reached the saved gateway,
`host.containers.internal`, and the node LAN address with the expected site
authentication response. The durable compatibility blocker remains open.


## Live Lightning purchase: Framework to Shorty — 2026-10-02

Operator explicitly authorized a very small new test purchase, then performed
it from Framework. This is separate from recovering the Amish Paradise sale.
Fixture: `archy-lightning-delivery-test-20261002.txt`, price 1 sat, Lightning only.
Read-only checks confirmed one matching seller invoice, SETTLED for exactly
1 sat, and Framework payment SUCCEEDED for 1 sat with 1 sat routing fee
(1,000 msat). Total spent was 2 sats. The assistant sent no payment.

Framework has exactly one durable purchased-content ownership entry for the
fixture, with backend `lightning`, paid_sats=1 and size_bytes=121. Its cached
file SHA256 equals the original seller fixture:
`d55f7a6acd77bdc3c35c65ecac6d1492096e99252d07d433e6286544540cde7e`.
**PASS: actual node-wallet payment, seller settlement, delivered bytes and
persisted buyer ownership/cache.** Framework runs the candidate backend
`8fb6249d1869bb8c9aea26d0f846de5b3eec328113a5c7f573628306e26e652e`;
Shorty runs `e108b78bbbd21cb7d5d47c8d0b7b9b19b63fb0c44678773603202440ec7d6f5b`.
This also exercises the candidate buyer against the existing seller version.

Live reopen without payment and restart acceptance are not established by
this check. Shorty had no matching durable entitlement JSON in the inspected
location; do not attribute the candidate seller persistence implementation to
this older seller binary. No node or wallet was restarted. The tiny fixture
remains available for a free cached reopen check; remove only its catalog
entry/source file afterwards, preserving buyer ownership and payment records.


### Operator-confirmed free reopen — 2026-10-02

After the verified one-sat purchase, the operator reopened the file on Framework
and confirmed it worked without another payment. **PASS: live paid delivery,
durable buyer ownership/cache, and free repeat access**, with independent
settlement/byte checks above and operator confirmation of the reopen UI.
This closes that specific live acceptance check; it does not establish an
untested restart, outage, or seller-upgrade scenario.

The temporary seller catalog entry and source fixture were removed after
acceptance. Buyer purchased bytes/ownership and all payment records were preserved.
