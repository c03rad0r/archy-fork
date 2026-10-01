# Post-1.8.22 regressions and retained release checklist

Status: OPEN. New regressions reported after publication on 2026-10-01.
Do not mark complete from source changes alone. Preserve wallets, app state and
operator uninstall decisions. Never send a second payment to recover delivery.
The earlier Framework startup incident remains separately closed with operator
acceptance; this is a new paid-file incident.

## Current tasks

- [ ] Recover the Framework's Lightning paid-file purchase without another payment;
      inspect buyer/seller evidence and verify delivered bytes.
- [ ] Correct seller settlement verification when local-node payment skips polling.
- [ ] Durable seller entitlements and safe buyer retry after navigation/restart;
      do not issue another payment on an uncertain or successful attempt.
- [ ] Cache Lightning purchases, preserve ownership, optional Files copy, free repeat.
- [ ] Diagnose mobile companion uploads on the affected route/device.
- [ ] Real progress in the existing compact upload bar; no increased height.
- [ ] Preserve uploads/progress across screens and original batch destination.
- [ ] Cancel active transfer and queued files; truthful partial/error/server-save status.
- [ ] Transparent transaction-filter container; single horizontal scrolling mobile row.
- [ ] Immich displayed as one app, internal components hidden; diagnose restarting services.
- [ ] Diagnose unwanted CryptPad after upgrade, failed uninstall, and persistent removal.
- [ ] Identify the other removed unexpected service from affected-node records.
- [ ] Upgrade regression matrix: installed/stopped/restarting/removed/legacy apps,
      aliases, dependencies, inventory, desired-state reconciliation and data preservation.

- [ ] Portainer duplicate-network migration: retire the redundant managed repair
      override, preserve operator settings/state, verify generated command,
      actual request namespace, dashboard readiness and repeated reconciliation.

- [ ] Lightning cooperative-close fees: Standard/Medium/Fast/Custom selection,
      explicit default target, strict backend validation and forwarding, error
      handling, mobile layout and no real channel closure during tests.

- [x] Apps search clear control: My Apps, Services and App Store, desktop/mobile,
      existing design tokens, right-aligned icon, no size change, keyboard focus.

## Retained release work (previous acceptance is not new-regression acceptance)

- Mempool patched image/catalog version agreement, update-button clearing, one card.
- Minibits PR160, Lightning address availability, concise single-column backup copy.
- Framework LND startup/Receive and unknown-vs-zero balance behavior.
- Friendly Bitcoin warmup; LND waiting for install/sync; Bitcoin UI during IBD;
  headless Phoenixd without self-waiting or bogus launch action.
- Cashu same-mint paid files, exact amounts/change/refund, errors, stored bytes,
  Files copy and repeat access without re-payment.
- mempool.space public explorer fallback, preserving local/custom configuration.
- Optional install pruning and consistent automatic-pruning policy.
- X250 kiosk version picker layering/contrast and pruning layout.
- AIUI single desktop/mobile background, transparent embedded layers,
  preserved standalone wallpaper.
- PR review/fixes/tests and normal merge/closure (160 previously shipped;
  161/162 merged and included in 1.8.22).
- Installed inventory retained during app restart/hard-refresh.
- Correct iframe/browser launch readiness, useful errors and delayed startup.
- GitWorkshop payload/build contexts, progress and persistence after refresh.
- Gitea/Portainer same-server Git from actual request namespace; URLs, auth,
  fresh installation in either order, migration/rollback, restart/reboot,
  Git/SSH/LFS/registry/browser compatibility and data/stack preservation.
- NPM correct admin port/URL, malformed URL behavior, bind-aware readiness,
  persistent backed-up tunnel/LND port-conflict repair on OTA and ISO.
- Angor headless indexer on DEV BOX only, full unpruned Bitcoin/Mempool/ElectrumX
  prerequisites, optional separate relay, official icon with green white areas.
- Compact named readiness messages and bottom-aligned app-card actions.
- Remove unused integration/build fixtures from Apps/Services, preserve app data.
- Safe network doctor, no all-app stop/reset on failed egress probe.
- No orphan companion resurrection; retain existing companion security repairs.
- Current companion image registry, build contexts, runtime asset promotion order,
  generated-service argument quoting and graceful Bitcoin/LND shutdown.
- OTA + RAW ISO, root signatures/catalog compatibility/checksums, independently
  verified public files, Git/ngit source/releases and fleet discovery.
- Correct LAN SCP command for the new ISO.

## Explicit boundaries/follow-ups

- Full-chain Angor indexing awaits development Bitcoin IBD.
- Primal automatic comment exceeding Minibits metadata limit: previously accepted
  upstream limitation, no unsupported local identity/metadata rewrite.
- Lost-response ecash seller receipt redesign is a separately accepted follow-up;
  do not claim an uncertain refund completed or automatically pay twice.
- Optional external-provider/hardware tests must be labelled if not exercised.

## Initial source evidence

`PeerFiles.vue::payWithLightning` immediately downloaded after buyer payment,
while only seller `handle_content_invoice_status` marked a pending invoice paid.
Seller download checked only that cached flag. This matches the reported error
and is supported by source inspection. An earlier diagnostic's HTTP 404 is not
valid confirmation: it incorrectly base64-decoded lncli's already-hex payment
hash. The corrected live diagnostic recognizes the settled invoice on the
candidate; do not cite the earlier 404 as proof of the original failure sequence.
`content_invoice.rs` stored all entitlements only in process memory with a
one-hour TTL, losing both pending and paid access on restart/expiry.
Lightning download returned transient base64 without the Cashu ownership cache.
CloudFolder's view-local spinner had no byte progress/cancel; batch upload read
`currentPath` independently for each file, allowing navigation to move destinations.
Immich's underscore dependencies are scanner-excluded; hyphen manifest IDs are
not. Live inventory confirmed both hyphenated synthetic entries while the
actual underscore-named containers had remained running for nine days.

## Access / acceptance

Operator provided updated Framework SSH authentication privately in chat.
Do not put credentials or deployment addresses in this public document.
Framework was reached over SSH. Native Bitcoin, LND and all three Immich
container identities/start times were recorded before candidate deployment.
The kiosk is at its login page. Dashboard password authentication succeeds but
requires the operator's second factor; normal uninstall acceptance remains pending.

Confirmed live evidence:

- A 10,000-sat peer-file invoice settled at 12:19:29 UTC. The original status
  diagnostic used an incorrectly decoded hash; see the correction above. Buyer
  identity and confirmation that this is the reported sale remain pending.
- The matching item currently allows free access; preserve that operator setting.
- CryptPad has no container but remains in installed-apps metadata. Uninstall
  repeatedly aborts because the removed catalog ID has no manifest.
- Immich server/database/cache are running; synthetic hyphenated dependencies
  appear stopped and the recovery overlay briefly advertises restarting.
- The other removed service was Core Lightning; uninstall tombstones exist.
- No Android resource-upload POST appears in the inspected recent nginx log.
  This does not establish why the affected companion failed.

## Candidate implementation and validation

Source changes persist seller entitlements with atomic writes, verify settlement
at delivery, recover older Lightning entitlements from the seller's LND invoice,
perform the status handshake for older sellers, and cache delivered Lightning
files. Buyer purchase bytes and the shared ownership index now use atomic,
synced writes and a serialized read/modify/write transaction; a corrupt index
fails the write instead of silently replacing existing ownership. The browser saves the invoice before payment and retries delivery without
another payment. Browser receipts are not yet a node-wide recovery store.

The upload queue now belongs to the shared Cloud store, captures its original
folder, reports actual sent bytes and server completion, and cancels its active
XHR and remaining queue. The fixed-height bar remains available across routes.
Transaction filters use a transparent container and one scrollable row. Immich
aliases normalize to their real component names and internal cards are hidden.
Unknown catalog entries no longer prevent the regular uninstall flow.

Validation so far (additional acceptance still pending):

- Final isolated backend suite: **1,631 passed**, zero failed, four optional
  tests ignored. This includes invoice settlement/amount boundaries, durable
  seller records, concurrent buyer ownership, damaged-index preservation,
  Portainer override retirement/idempotence/customization/backup failures,
  recovery overlays and channel-close fee forwarding/validation.
- Final frontend suite: **1,157 passed** across 142 files. Production build
  passed. Six payment-recovery and twelve channel-close tests are included.
- Real FileBrowser uploads at 1440px and 390px: exact bytes and original folder
  verified after navigation, 44px bar, cancellation and queue stop passed.
- Mobile viewport acceptance is not physical Android companion acceptance.
- Final release backend build passed. Candidate backend and dashboard are now
  deployed on dev and Framework. Another OTA/ISO remains pending; published
  1.8.22 artifacts remain unchanged.

Release gates still include actual-node payment recovery/delivery, durable
CryptPad removal through normal controls, Immich inventory after refresh/restart,
physical companion diagnosis, and remaining upgrade regression acceptance.
No new payments, native-service restarts or wallet changes were used in testing.

## Additional live Portainer regression

The X250 user service exited 125 because the generated command supplied
`--network slirp4netns` twice. The manifest already supplies the network, while
an older Archipelago-created `archy-same-node-network.conf` drop-in adds it
again. Quadlet's Network directives accumulate; they do not override each other.
This repair artifact should have been retired when the declarative fix shipped.

The live repair backed up the override and Portainer state, removed only the
exact redundant override, reloaded user systemd and restarted Portainer. API
status returned HTTP 200 with version 2.45.0; the actual kiosk's package state
reported running and UI-ready. Bitcoin/LND and the production site's container
identities/start times remained unchanged. The source migration now detects
this exact managed override before preparing the persistent restart obligation,
backs up app state, retires the redundant file with a retained copy, and reloads
and restarts through normal reconciliation. Custom overrides are preserved.
Automated migration coverage passed; candidate is now deployed on dev and
Framework. The X250 retains its verified live repair pending the next OTA.

## Channel-close fee selection

The existing close UI sent only the channel point, and the backend forwarded
only `force=false`. LND therefore used its lax default confirmation target.
The candidate reuses the channel-opening fee choices (six/three/one block target,
or custom target/rate), explicitly sends six blocks for legacy clients that omit
fees, and validates query parameters before accessing the wallet. Cooperative
fees are never silently applied to force closes. Close RPC retries are disabled
so a timeout cannot silently repeat this mutation.

Protocol reference: [LND CloseChannel](https://lightning.engineering/api-docs/api/lnd/lightning/close-channel/).
Fee targets are estimates, not guaranteed confirmation times. Tests use mocked
requests; no production channel is closed to verify the feature.

Additional browser acceptance:

- Transaction filters at 390px: computed transparent background, one row and
  horizontal overflow verified.
- Close-channel selector at 1440px and 390px: preset/custom controls visible,
  no overflow, custom 25 sat/vB forwarded. The close request was intercepted;
  no real channel closure or wallet mutation occurred.
- All three Apps search screens at both widths: clear icon stays inside the
  field; click/Escape clear; input retains focus; desktop 40px/mobile 52px heights
  stay unchanged. Shared design-system search-field classes are retained.
- Portainer remained active with zero service restarts and no pending marker.
  Its real network namespace read smart HTTP Git refs and the Compose file.
  Original persistent mounts were unchanged. The old integration test containers
  are absent from dev, Framework and X250. One leftover upload-test folder was
  removed after checking it contained only this task's test files.

## Build resource observation

The final optimized compile coincided with heavy memory/disk pressure and local
Bitcoin/LND RPC timeouts on the development node. After pausing the compiler,
both authenticated RPCs responded again; Bitcoin reported height 506400 and
19.6% verification progress, with LND waiting for chain sync. No native service
was restarted. Compilation resumed in a separate user scope limited to one CPU,
with nice 19 and idle I/O priority. This is evidence of resource contention,
not proof of a new wallet or startup defect. Verify native RPC health again
before candidate deployment.

## Candidate deployment and live acceptance — 2026-10-01

Source: `f4d34554` (later commits update this checklist only).
Backend SHA-256:
`8fb6249d1869bb8c9aea26d0f846de5b3eec328113a5c7f573628306e26e652e`.

- Backed up backend, dashboard and app metadata on both nodes under the root-only
  support directory `post1822-regressions-20261001`. Deployed assets before
  promoting the dashboard entry point; manager health passed first.
- Only the Archipelago manager restarted. Bitcoin/LND IDs and start times stayed
  unchanged; Framework's three Immich containers also stayed unchanged.
- Dev authenticated Bitcoin/LND RPCs responded after deployment. Bitcoin IBD
  continued above height 513000; LND correctly reported not yet chain-synced.
- Both nodes serve dashboard entry bytes identical to the production build.
  All six search-clear cases passed against the live dev dashboard (three
  screens, desktop/mobile), including focus, Escape and unchanged field size.
- Framework's stale CryptPad installed claim was removed while the manager was
  stopped; both legacy IDs were recorded as user-uninstalled. Data was preserved.
  Removal remained after another management restart. Dashboard uninstall-flow
  acceptance still awaits authentication; this repair was performed over SSH.
- Corrected invoice diagnostic recovered the settled seller entitlement, returned
  HTTP 200 with `paid: true`, and saved a mode-0600 record. A different item was
  rejected. Paid status survived another manager restart without native restarts
  or a second payment.
- Delivery acceptance remains OPEN: the catalog's file is absent from both its
  dedicated content path and FileBrowser path; a privileged filename search of
  those trees found no copy. Its free-access setting was preserved. Buyer and
  reported-purchase identity still need confirmation; do not claim the actual
  buyer received the file.
- The malformed-hash diagnostic also exposed that invoice-status currently
  propagates validation errors as a closed connection. Before release, return a
  structured 400 for malformed hashes and an explicit retryable response for
  settlement-service errors, with endpoint coverage.

Physical companion upload diagnosis and remaining release acceptance stay OPEN.
This is a candidate deployment, not a newly signed OTA or ISO.

## Release authorization — 2026-10-01

The operator authorized preparing the next OTA and raw ISO after completing all
checks available here. Keep the missing original file/buyer confirmation,
physical companion upload and full-chain Angor indexing as explicit follow-ups;
this authorization does not establish that those scenarios passed. Complete
the known invoice-status HTTP error fix and available automated release gates
before requesting the offline signatures. Angor Indexer and the optional relay
are already present in the current signed catalog and remain included.

Shorty's Mempool report was inspected read-only: frontend/API had been running
for over two weeks, systemd reported zero restarts, and the API was processing
current blocks. The operator said it looked normal after applying the latest
update. No Mempool repair or native-service restart was performed in this check.
