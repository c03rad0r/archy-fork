# Next OTA and raw ISO after 1.8.21

**Status: implementation and acceptance in progress; NOT ready to release.**

This is the consolidated execution checklist for the operator's chat requests.
A targeted node repair is not completion of the release. Finish the remaining
acceptance gates, preserve live wallets and app data, and publish both artifacts
through git and ngit. No universal absence of future failures is claimed.

## Changes already shipped in 1.8.21 or earlier

Keep these fixes in the next build and include relevant regressions:

- Mempool image/catalog version agreement and update-button behavior.
- Minibits integration; Framework automatic LND startup and safe unavailable
  balances. Framework incident closed with operator acceptance.
- Shorter, single-column ecash backup messaging.
- AIUI transparent background on desktop/mobile.
- Cashu paid-file keyset/mint/error/refund corrections, with live purchases.
- mempool.space explorer fallback, preserving local/custom explorer settings.
- Bitcoin install pruning choice and matching automatic-pruning behavior.
- Friendly Bitcoin warmup and LND install/start/sync waiting states.
- Raw ISO publishing and upload support.

The Primal automatic LNURL comment problem was traced to sender behavior and
Minibits metadata. The user accepted clearing the sender's automatic comment;
no unsupported local metadata rewrite or wallet-identity replacement is planned.
See the Framework incident and 1.8.21 execution records for evidence/limits.

## New release scope and gates

| Task | Implemented/verified | Remaining before release |
| --- | --- | --- |
| X250 Bitcoin picker | Inline choices; actual Chromium kiosk selection, readability and pruning layout passed | Include in final UI/build checks |
| App disappearance/readiness | Durable inventory and safe lifecycle repair; delayed HTTP and desktop/mobile hard-refresh checks passed | Final lifecycle/reboot gate on candidate |
| X250 GitWorkshop/Nginx | Missing build contexts restored, dependency/build checks and live UI passed; Nginx slow pull diagnosed; truthful progress label | Verify both artifact payloads contain all build contexts |
| PRs 161/162 | Reviewed, repaired, merged/closed normally; combined regression suite passed | Funded Tor-only candidate purchase, retained change, refund, Files bytes and cached repeat passed; include in signed artifacts |
| Gitea/Portainer | Root cause confirmed; source network/backup/retry/catalog changes; real X250 routing repair and restart verified; private Git, SSH, LFS, registry and browser fixture checks passed | Automatic migration, scratch restore, failed-start recovery and reverse installation order passed. Operator confirms production site works through Portainer; production host reboot also preserved network/Git/Compose access; final candidate delivery and release checks remain |
| Angor headless store service | Implemented standard Mempool adapter and separate optional relay, official logo, headless store entries and declarative dependency guard. API security/outage/DNS tests and five relay lifecycle cycles passed | Final candidate prerequisite/install acceptance, management restart/reboot checks and signed catalog delivery; real indexing on dev waits for Bitcoin sync |

Durable payment receipts after a lost seller response remain a separately
recorded design follow-up. Preserve the truthful unconfirmed-refund warning and
prevent duplicate automatic payment; do not describe an unconfirmed refund as
completed. See PR review for the accepted scope and coverage limits.

## Final release checklist

- [ ] Finish all new-scope implementation and specific acceptance above.
- [x] Remove disposable fixtures and temporary test overrides; verify native
      Bitcoin/LND identity and start-state baselines remain protected.
- [x] Commit and push completed source changes to git and ngit.
- [ ] Run final backend/UI/regression/release gates on the final source; inspect
      skipped tests and report actual hardware/runtime coverage.
- [ ] Prepare compatible signed app catalog; old runtimes must not apply a
      migration before they have backup/recovery support.
- [ ] Version/changelog and OTA payload prepared, validated and signed by user.
- [ ] Raw ISO built; payload hashes/content verified; installer boot tested.
- [ ] User signs ISO checksums; publish OTA and ISO plus verification files on
      git and ngit; independently read back hashes and update discovery.
- [ ] Provide LAN scp command for the new raw ISO.

Latest backend source verification: 1,609 passed, zero failed, four existing
ignored tests. This is one layer of evidence, not a substitute for live gates.

## Angor verification — 2026-09-30

- Isolated backend suite: 1,606 passed, four existing ignored; container suite:
  79 passed. Frontend: 140 files / 1,130 tests passed; production build passed.
- Disposable rootless API gateway: versioned and legacy API paths, query/body
  forwarding, transaction-only POST, method/body limits, CORS, removal of
  dashboard credentials, read-only non-root operation, truthful backend outage
  and DNS recovery after backend recreation passed. No real transaction broadcast.
- Dedicated relay: NIP-11, signed event publish/read, invalid signature rejection
  and event/config persistence across five managed stop/start/restart cycles
  passed. Internal relay identity and start time stayed unchanged. Follow-up
  acknowledgement samples were 2–9 ms through both backend and app gate.
- Published adapter 1.0.1 and relay 1.1.2 to the authenticated maintainer namespace.
  Anonymous registry readback succeeded. Adapter digest:
  `sha256:997be611700b55c521ad801fa92daaca2ae6951ac71407434c85eb9603f77c38`;
  relay mirror digest:
  `sha256:80444ad1304a0e504948b48ea1550c091b18b9f10757f07ce9a68fc261b8f6c1`.
- Delivery target is the development box, as clarified by the operator. Do not
  install Angor on the separate Portainer node. Full indexer availability still
  requires the dev box's Bitcoin sync and Mempool/Electrum indexing to finish.
- Funded PR acceptance passed after the operator funded the dev Cashu wallet
  with 16 sats. Exact net payment was 1 sat; underpayment refunded in full;
  repeat delivery cost zero. Both endpoints ran the combined candidate.
  No spent proofs were reactivated and no native Bitcoin/LND funds were moved.

## Development candidate and cleanup

The combined optimized backend and production UI are deployed on the development
box with a private rollback copy. Native Bitcoin/LND containers were unchanged
during deployment. The operator separately uninstalled/reinstalled Bitcoin Core
to select an unpruned node; RPC confirmed `pruned=false`, and a separate baseline
was recorded after that operator action. Do not compare subsequent checks with
the pre-reinstall container start times.

Completed Gitea setup/private-repository and Portainer integration fixtures were
uninstalled through the supported lifecycle and removed from installed inventory.
Their private evidence/data were retained outside the active manifests. The old
Cuprate UI review container was also removed. Active Angor acceptance fixtures
must be removed on completion; the requested Angor services remain installed.

Funded acceptance used Tor-only peer-file transport, verified exact delivery
bytes and compatibility response fields, and read the result back through
FileBrowser. The original transport preference was restored, and temporary
seller catalog entries/files and the exact buyer test document were removed.
Financial receipt history was retained.

The final managed-install fixture exposed a separate Quadlet quoting defect:
whitespace-free command arguments containing apostrophes lost those characters
in the generated service. The renderer now quotes these arguments and
environment values; the updated isolated backend suite passed (1,607 passed, four opt-in tests
ignored), and the final candidate rebuild is in progress. Do not tag a release before this live regression is verified.

The production Portainer host subsequently rebooted after the routing repair.
A post-boot probe from the actual Portainer namespace again verified the Git
smart-HTTP response type, current branch ref and Compose contents. Its
slirp4netns route and all production app containers survived. The temporary
Portainer fixture was absent. This verifies the repaired production route
across reboot; it does not substitute for final new-runtime delivery checks.

## Follow-up acceptance: app cards and Angor icon

- Mempool duplicate traced to `archy-mempool-web` durable inventory alias being
  restored beside the real `mempool` frontend. Shared scanner canonicalization
  fixes live and absent-container paths without deleting installed markers.
  Frontend suppresses aliases only while a canonical tile exists.
- Readiness text names the app and condition: “Web UI not ready: Gitea”. It shares the status row,
  with full text available through its title; card actions use bottom alignment.
- Angor uses the operator-supplied dark-mode icon with green outer corners.
  Built-in imagegen prompt: fill transparent/white corners with the existing
  flat green, preserve the black symbol, square opaque PNG, no added details.
- Backend alias suite: 1608 passed, 4 ignored. Focused readiness/frame UI tests:
  30 passed. Production UI build passed and is live on dev. Browser checks at
  1440 and 1024 pixels verified named waiting text, bottom-aligned actions,
  equal row heights, no overflow and one Mempool card after hard refresh.
  The 390-pixel mobile icon layout also passed hard refresh. Final-source
  isolated Mempool alias regression passed after the scanner simplification.
- Managed Angor adapter acceptance: five stop/start/restart cycles, missing
  prerequisite refusal, management restart and cleanup all passed. Both temporary
  fixtures and their network were removed. Actual dev API verification remains
  pending after removing an incomplete legacy-created adapter.

## Startup manifest reload race

Live Angor acceptance exposed a separate startup race: runtime asset bootstrap
cleared and copied `/opt/archipelago/apps` in the background while the startup
catalog refresh reloaded it. The daemon logged 62 loaded manifests followed by
54 and then rejected the new disk-only app as unknown. A stable manifest snapshot
confirmed the diagnosis: supported uninstall/reinstall produced the correct
rootless Quadlet service with its declared port and network.

Runtime promotion and the legacy installer-directory repair now finish before
orchestrator construction. The background doctor no longer changes that tree.
The final source backend suite passed 1,608 tests (four existing opt-in tests
ignored). Optimized build and normal-path live startup/restart verification have now passed (see final follow-up below).

Actual dev Angor acceptance passed managed service identity, no capabilities,
UID 101:101, archy-net, public block height, CORS and both fee URL forms. During
Bitcoin initial sync, the real Mempool fee API returns 503; the adapter faithfully
returns the same status and body. Full-sync fee availability remains unverified;
ready-backend API and failure/recovery behavior passed the isolated live fixture.
The temporary `/run/archy-candidate-manifests` snapshot override and snapshot
are now removed; normal startup/reload verification passed.

The latest complete UI suite passed 140 files / 1,132 tests. Release preflight
passed all static, manifest, catalog, type and UI gates. The requested named
waiting message, compact card layout and green Angor icon are deployed to dev;
desktop 1440/1024 and mobile 390 browser checks passed after hard refresh.
The startup-order optimized build and normal-path startup checks are complete.
The later dashboard-address candidate is now deployed with rollback; see the
final live follow-up below. Do not rerun the earlier deployment helper: its
temporary override has already been removed. No new release version/tag, OTA
or ISO has been created.

## Mempool and dashboard follow-up

- Deployed the Mempool alias and runtime-promotion-order backend to dev. Real
  server state contains one healthy `mempool`; the stale `mempool-web` record
  is gone. Real-data browser checks at 1440/390 pixels found exactly one tile
  before and after hard refresh. Bitcoin/LND identities/start times unchanged.
- Removed the candidate manifest override. Normal management startup passed
  two full cycles with 62 manifests retained through both initial catalog
  refreshes. The next cycle hit a single readiness assertion; a subsequent
  read-only check found Angor healthy and the manifest count intact. Remaining
  repeat coverage should use bounded polling to distinguish transient request
  failures from loss of app definitions; do not report five cycles passed yet.
- Bitcoin Core's dashboard was serving HTTP 200 on 8334 while readiness checked
  RPC 8332. Companion URL selection now takes priority over protocol sockets
  for Core/Knots and Electrum aliases, with a regression preserving allocated
  UI ports for other apps. Backend suite: 1,609 passed, four opt-in ignored.
  Optimized build is `/tmp/archy-dashboard-address-build.log`; deployment and
  live IBD verification helper: `/tmp/archy-dashboard-address-deploy.py`.
- Phoenixd has no browser UI. Headless services now omit web-readiness messages;
  actual browser apps name their web interface rather than waiting for
  themselves. Focused 23 UI tests and production build passed; deployed to dev.

## Final live follow-up: all three reported readiness/display defects fixed

- Final optimized backend is deployed on dev. Bitcoin Core's launch address is
  `http://localhost:8334` and `ui-ready` is true during initial block download.
  Live verification recorded height 293,855 with `initialblockdownload=true`.
  Chromium at 1440 and 390 pixels opened the embedded dashboard, read a numeric
  current block height, and repeated that check after hard refresh.
- One healthy Mempool remains in server state and in desktop/mobile My Apps
  after hard refresh. Its durable install markers were preserved.
- Phoenixd remains a running headless service without a web launcher or false
  web-readiness message. Desktop/mobile browser checks passed. For actual web
  apps, the compact copy is “Web UI not ready: [app]”; the reason comes first so
  narrower cards do not truncate it into a misleading self-dependency.
- Five normal management startup and managed Angor restart cycles passed
  across the two acceptance logs. The retry harness uses bounded readiness
  polling; it does not accept a running container alone as API readiness.
  Disk + catalog manifest count remained 62 across startup refreshes, replacing
  the previous 62-to-54 failure. Temporary override and snapshot are removed.
- Final backend tests: 1,609 passed, zero failed, four existing opt-in ignored.
  Final UI tests: 140 files / 1,133 passed. Production UI build passed and is live.
  Bitcoin/LND container identities and start timestamps stayed unchanged.
- Evidence: `/tmp/archy-dashboard-address-deploy.log`,
  `/tmp/archy-bitcoin-ibd-browser.log`, `/tmp/archy-mempool-live-browser.log`,
  `/tmp/archy-service-readiness-browser.log`,
  `/tmp/archy-runtime-order-remaining-cycles.log`, and
  `/tmp/archy-readiness-final-ui-tests.log`.
- These are live development fixes. The new signed catalog, versioned OTA and
  raw ISO still need preparation, artifact verification, signing and publication.

### X250 Nginx Proxy Manager tunnel repair (2026-09-30)

A further live report was a real startup failure, separate from the earlier slow
image pull. An operator-specific Quadlet `web-tunnel.conf` published NPM's HTTP
listener on tunnel port 18080. LND subsequently occupied 18080 on all addresses;
pasta failed before NPM could start, with more than 1,400 systemd retries. The
standard NPM manifest only publishes admin port 8081 and did not introduce this
extra mapping. Changing the standard manifest would not repair this override.

The node's override now uses free tunnel-local port 18081. Its persistent nftables
configuration redirects only HTTP arriving from the configured WireGuard peer on
the original tunnel destination to that port. The peer/public routing is unchanged;
the input rule accepts the translated port and retains the existing interface,
peer and forwarding restrictions. LND's REST port and native processes were not
changed. This deployment-specific topology must not be copied into global app
manifests or applied indiscriminately to other nodes.

An abandoned certificate request also left an unreferenced database record and
a temporary nginx challenge server for the same hostname. After backing up the
entire NPM data directory and both configuration files, the unused failed record
was soft-deleted and the stale challenge file archived. The referenced, valid
certificate, proxy host, keys and user accounts were preserved.

Live checks: NPM admin and API HTTP 200; nginx configuration validation with no
duplicate-host warning; public HTTP redirects to HTTPS; valid public TLS reaches
the site's existing authentication response, matching its direct upstream. NPM
starts with zero automatic restarts and no missing-certificate renewal error.
Bitcoin, LND and the production site container identities/start times were
unchanged by the port repair. Rollback copies and the data archive are retained
in the node's private support directory. No global OTA or ISO was published by
this repair; the remaining release gates above still apply.

#### Follow-up: fleet delivery and false health failures

A longer observation exposed a second, generic defect after the port conflict
was repaired: the health monitor probed all published ports at `127.0.0.1`,
including NPM's tunnel-only listeners. Every monitor interval could therefore
restart a healthy app. The short initial restart check did not catch this.

The next backend now probes the actual `host_ip` from Podman; only wildcard
addresses map to the corresponding loopback family. Regression tests cover
explicit IPv4/IPv6 binds, wildcards, UDP/unpublished/invalid entries, and a real
listener on a different loopback address. NPM's manifest now checks its internal
admin HTTP API. The same check is deployed as a persistent Quadlet drop-in on
the affected node so its older backend stops making false recovery attempts.

The backend embeds `scripts/repair-npm-tunnel.py` and runs it before app
reconciliation, after runtime asset promotion. This makes the targeted legacy
port migration available to both OTA and ISO installations without relying on
an independently installed script. Standard fresh installs are a no-op. Only
the recognized legacy tunnel/firewall profile is migrated; unknown operator
routing, occupied replacement ports and live-only firewall changes fail closed
with a startup warning. Configuration backups, an interrupted-migration journal,
atomic nft transactions and rollback protect the existing routing. Native wallet
services and certificate databases are never modified by this fleet migration.

The Python migration tests run in the release gate. The unsigned next catalog
was regenerated successfully with the new NPM HTTP health check. These changes
are prepared for the next release; existing published OTA/ISO artifacts remain
unchanged and the new signed artifacts still require the release gates above.

Verification for this follow-up: 18 migration tests passed; 43 health-monitor
backend tests passed through the isolated runner. A disposable network-namespace
regression exercised actual peer traffic through the nft redirect while a
separate simulated LND listener retained port 18080. The generated rules also
passed nft validation and atomic replacement. Run that regression with
`sudo unshare --net python3 tests/regression/npm-tunnel-network.py`; it refuses
to run in the host network namespace. The migration is a verified no-op on the
already repaired node and on a standard development install without the override.

After deploying the API health check, a 270-second live observation crossed
multiple health-monitor intervals: NPM stayed healthy with the same container
ID/start time, every API probe returned success, and Bitcoin/LND/production-site
container IDs/start times were unchanged. This supersedes the initial short
restart-only acceptance recorded above. The generic backend fix is committed
for release, while the live node uses the equivalent internal NPM health check.

### Final-gate Angor health-check correction

Final release observation found the adapter healthy over IPv4 but marked
unhealthy by its in-container BusyBox wget: `localhost` resolved to `::1`, where
nginx does not listen. Its manifest now explicitly probes `127.0.0.1`. The live
managed service was refreshed and its real Podman health check passed. The
rootless gateway integration now runs the manifest's health check inside the
actual image, in addition to endpoint/security/outage/DNS recovery assertions;
all passed. A metadata regression covers the address-family requirement. Test
containers and their network were removed by the fixture cleanup.

## 1.8.22-alpha release preparation

Final implementation gate passed: 1,612 isolated backend tests, zero failures,
four existing opt-in tests ignored; 140 frontend files / 1,133 tests; frontend
type check and production build; static/catalog/trust/build-context checks.
The four exclusions require external AI backends, Reticulum subprocess/live
transport, physical RNode hardware, or creation of a live Minibits profile.
They are not claimed as executed by the isolated suite. Existing funded
Cashu/Minibits and Framework acceptance remains recorded above.

The real dev Angor stack now returns HTTP 200 fee estimates through both API
forms; Bitcoin is still in initial sync, so full-chain completion remains an
operational prerequisite rather than a completed test. The image-level health
probe, outage/recovery and live native-state preservation checks passed after
reloading the corrected manifest. Production Portainer again fetched the exact
repository branch and Compose content from its own network namespace.

Version preparation is 1.8.22-alpha. No new release tag or fleet-visible update
manifest is published by the version commit. Optimized candidate deployment,
artifact inspection, ISO smoke/boot checks and offline signatures follow.

### Release blocker discovered during candidate observation: scheduled doctor

The initial `02b840f2` 1.8.22 candidate is rejected for release. On the X250,
2026-09-30 21:10–21:11 UTC, the scheduled `archipelago-doctor.service` explicitly
ran `podman stop --all --time 30`, killed rootless network helpers and ran
`podman system migrate` after a two-attempt external network probe failed.
The journal attributes the stop to that unit, not the app health monitor or a
host reboot. All apps restarted, including Bitcoin, LND and the production site.
The earlier unchanged-container acceptance applies only to immediate deployment;
the later observation failed and must not be represented as a stability pass.
No persistent-data loss has been established. Keep this distinct from the closed
Framework incident; do not wipe or recreate any wallet as a recovery action.

Containment: stopped doctor timers on both test boxes, installed a safe diagnostic
script into both the executable and runtime payload, and rejected/stopped the
old ISO build. Network failure now produces a warning without stopping apps,
killing network processes, migrating Podman or deleting network state. Repeated
failures remain warnings, never a successful repair/check. Regression cases cover
healthy, absent network, non-root invocation, host failure, transient recovery,
repeated endpoint failure and namespace access failure, with mutation tripwires.
Live scheduled-cycle observation and final rebuilt-artifact acceptance are pending.

Recovery also exposed retired `git.tx1138.com` nginx base references in six
companion UI Dockerfiles. They now use the existing primary registry at the same
pinned version. All six images built successfully against that registry; payload
validation rejects the retired host before OTA/ISO packaging.

The post-recovery X250 check passes: Bitcoin authenticated RPC responds and IBD
advances; NPM/Gitea/Portainer APIs respond; Portainer's real namespace fetches
`demo-portainer` at `3ae171d6b0c728665a860520fe393c0abb772798` and its Compose
file; Portainer's original persistent mounts match the earlier backup evidence;
LND wallet/channel databases remain present on their persistent mount. No new
pre-incident cryptographic wallet-identity baseline was available, so these checks
must not be described as an exact identity/balance comparison.

The dev all-container observation also caught a separate Cuprate UI orphan loop:
`companion.rs` removed it because Cuprate was not installed, while generic desired-
state recovery resurrected it from an old running snapshot, using a unit without
nginx's required capabilities. Generic desired-state recovery now excludes missing companions
owned by `companion.rs`; existing companion provisioning/reaping remains the
single owner. Running UIs still receive the existing security configuration repairs. Regression runs repeated reconciliation against stale companion
snapshots and checks that no image/container lifecycle operations occur.

Safe-doctor live acceptance: the X250 completed a 12-minute observation with all
running container IDs, start times and data mounts unchanged. Its journal records
successful doctor runs at 21:22:06, 21:27:51 and 21:33:10 UTC. Both doctor timers
are restored with the safe script. Dev's native Bitcoin/LND stayed running;
all-container dev acceptance remains pending the companion-loop backend fix.
Final-source UI suite: 1,133 passed. Heavy backend compilation is serialized with
remaining build steps to reduce memory/IO pressure on the syncing dev node.
