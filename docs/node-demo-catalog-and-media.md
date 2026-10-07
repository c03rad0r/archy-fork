# Node-scoped demo apps and persistent media

Status: managed demo deployed to the authorized node on6October; playback and
full lifecycle acceptance remain open. This is not a global catalog release.

The V4V demo is restricted to Yaya. The global catalog and other nodes must not
receive an install button or banner for this prototype. Its manifest lives in
`demos/node-demo-v4v/`, deliberately outside public `apps/` generation.

## Catalog boundary

A node may load `node-app-catalog.json` beside its normal catalog. This document
must carry a valid pinned release-root signature, `schema: 1`,
`scope: "single-node-demo"`, the exact `target_node_did`, and an unexpired
`expires_at`. Entries use the reserved `node-demo-` namespace, include validated
image manifests, and cannot replace existing catalog IDs. It is a separate
file; global signed bytes remain intact. No public mirror fetch or peer
redistribution is implemented for this file.

The authenticated `/api/node-app-catalog` endpoint returns the original signed
bytes only after these checks. The dashboard combines these entries/promotions
for display without saving them into its normal browser fallback catalog.
Removal, invalid signatures, expiry or another node's DID remove that demo
listing. They do not erase installed app data. Installation still uses the
normal app manifest, image, port and lifecycle enforcement.

Current activation: stage the signed file atomically, then restart the backend
to reload its manifest overlay. Automatic delivery of private catalog revisions
is not claimed. Qualification must test copying the file to a different node,
expiry, signature tampering, backend restart and preservation of public entries.

## V4V app

Source baseline: private V4V `demo-portainer` commit
`3ae171d6b0c728665a860520fe393c0abb772798`. Retain the original demo songs,
attribution and destinations. The demo keeps `PULSEWIRE_LN_MODE=mock` and its
existing password login. It does **not** inject Archipelago's native Nostr signer.
The media bridge is a distinct, non-signing integration.

Candidate bridge source: `5bc5f61b`. Session and receipt secrets are generated
by the manifest only when missing. For this migration, preserve the existing
password hash and seed it privately as `node-demo-v4v-password-hash` before
installation. Missing credentials must fail installation; do not fall back to
the upstream shared password. General public first-run credential provisioning
is outside this node-only demo and must be implemented before a public listing.

The login backdrop was captured from the running app's actual canvas in an
isolated browser context, with the form hidden. Use this asset for the node-only
“Sovereign Music” promotion. The public artifact can include the image; visibility
of the app/promotion is controlled by the scoped catalog.

Before migration, back up the existing Portainer data/media volumes and secret
configuration privately. Qualify a separate copy first. Do not attach both live
containers to the same writable database. Keep the original stack and volumes
available for rollback until the managed replacement passes lifecycle checks.
No live V4V state or Portainer stack has been changed by this implementation yet.

## Persistent player contract

The app declares `metadata.launch.media_controls: archipelago-v1`. The dashboard
retains its iframe while hidden and controls that same player through messages;
it never copies a protected media URL into a second audio player. The app checks
the exact dashboard origin and parent window; the host checks the exact loaded
app origin/window and a per-session nonce. The protocol carries bounded title,
artist, time, duration and playback state, plus play/pause/seek/next/previous
controls. It contains no credentials, signer operations or payment commands.

The bottom bar is hidden while the app player is open. Closing the app shows the
bar while audio continues; Open app reveals the retained session. Closing the
bar pauses playback and releases the hidden frame. Starting a Cloud track pauses
the app player. Late state from the paused player must not steal playback back.
Logout/unmount must release the frame and its state.

Required remaining evidence: actual mounted iframe survives close/reopen,
mobile/desktop controls and layout, fresh install and copied-volume upgrade,
restart/rollback, native companion background/resume, real catalog audience
rejection on another node, and exact final artifact hashes. Unit tests alone
are insufficient for this acceptance.

Qualification checkpoint: the dashboard suite passed 1,241 tests in 155 files.
The app bridge/player tests passed four tests, including pre-login connection,
origin/nonce rejection, locked controls and removal of metadata after relocking.
The isolated container reached HTTP health 200, then exposed the management
reaper's separate-storage ownership bug. Runtime acceptance is blocked on its
tested deployment; the old V4V image and live Portainer volumes are untouched.

## Registry qualification — 2026-10-06

The node-only manifest now pins the staged image by immutable digest:
`sha256:13044ecbeae9eb17bc98cc531ca202db9e9a0db8dc2ce01bb9f8cb789248d020`.
The registry namespace is `chaum/v4v-demo`, where the publisher has package write
access. This does not publish an app catalog entry.

Anonymous registry access verified the raw manifest digest and raw Docker
configuration blob. Its configuration digest matches the qualified local image,
`sha256:cd56bef6ec2c9d5d56b41d370c735fc3b4c12885acb1530be75c79a5dbde923f`.
The raw blob retains the Node `/healthz` probe, 30-second interval, 3-second
timeout, 10-second start period and three retries. `skopeo inspect --config`
normalizes this configuration and omits the Docker Healthcheck field; therefore
that output alone must not be used to decide whether this image retains it.
The accepted push explicitly used Docker v2 schema 2 format.

This is registry verification, not managed-install acceptance. Root-signed
node catalog, copied-data installation, lifecycle checks, physical companion
checks and final dashboard cold-launch regression remain required. Deployment
writes to dev and Yaya are paused because another session installed a mining
candidate on both nodes; reconcile source before replacing either build.

## Signed managed installation — 6October

The operator signed the prepared node-only catalog. Pinned-root verification
passed, and canonical payload comparison matched the reviewed unsigned file.
The signer reordered JSON keys; raw-file reconstruction was not a valid payload
comparison. Neither catalog contents nor its audience were changed.

A fresh consistent backup preserved the original demo data/media and password
hash. The unused managed volumes were refreshed and verified byte-for-byte with
ownership/modes retained. An initial copy attempt found rsync unavailable; the
copy was completed using the standard library before catalog activation. The
original demo remains running on its original port with its volumes untouched.

Catalog activation preserved the backend binary, session key and all preexisting
app container identities/start times. The first public endpoint check exposed
missing nginx routing: the SPA returned HTML for the node catalog. Both dashboard
vhosts now route the two exact catalog endpoint names to the authenticated
backend. The original nginx configuration was backed up and nginx validation and
reload passed. Source template and upgrade repair are updated; their new backend
regression run is pending. Public management guards were not modified.

Both HTTP and HTTPS catalog checks return401 without authentication and200 with
the existing owner session. HTTPS diagnostics ignored the previously documented
legacy certificate trust problem; ordinary browser trust is not claimed fixed.
The signed response contains only the node-demo-v4v entry. The initial manual RPC
omitted required dockerImage and failed before creating the app; the corrected
normal install request used the exact image from the signed manifest.

The installed app reports running/ui-ready and its container health endpoint
returns200. A real deployed dashboard browser, with no catalog/package fixtures,
shows the Sovereign Music listing and launches the retained-password login screen
at390px with no app-gate screen. The operator login/song check has been requested.
Managed restart, playback controls, desktop/browser/companion acceptance and
audience/lifecycle checks remain open until their results are recorded.

Qualification update: normal managed restart returned to running/ui-ready with
container health200; the original demo remained running. Desktop1440px also
passes real listing/launch-to-login checks. The full isolated backend suite for
recovery preflight and catalog route migration passed1,785tests, zero failures,
five existing skips (`/tmp/archy-node-catalog-route-tests.log`).

### Operator changes and live player regression

The operator now explicitly requests Nostr sign-in and native signer integration
instead of the alpha password mode. This supersedes the earlier instruction to
omit native signing for this demo. Preserve cryptographic login and user consent;
qualify actual platform signer, cancellation, logout, browser and companion flows.
A newly qualified app image/manifest and node-only catalog signature are required.

The operator reports music continues after closing the deployed app but the native
bottom player does not appear. Earlier fixture tests do not close this real
installation regression. Test the actual signed catalog and package state, close,
controls and reopening the same frame before accepting the repair.

The requested promotion uses the final intro cymatic still as its background,
with a music/play graphic on the right or the app's For You banner treatment.
The previously captured login background does not satisfy this updated request.

Latest operator observation: the bottom-bar transition is now working; controls
and the artwork background are still missing. Preserve the working transition.
Add legible current-track artwork plus play/pause, previous/next and shuffle
synchronized with the app's actual queue/state. This observation narrows the
reported symptom; it does not replace automated login-boundary and playback tests.

Mobile refinement requested: previous/next and the open-app action must fit neatly
at narrow widths with comfortable touch targets; prefer a compact app icon with
an accessible name. Fresh V4V launch should open Browse. Reopening the retained
playing iframe must preserve the current song, queue and session rather than
resetting it to the initial route.


### Session recovery and private distribution — 6 October, 22:24 UTC

All three development worktrees survived the interrupted session. Private recovery
bundles, patches, untracked source and test logs were saved and verified under
`~/.local/state/archipelago/session-recovery/20261006T221806Z/`.

At the operator's explicit request, the publicly downloadable original V4V demo
package version was deleted through the registry package API (204); authenticated
package enumeration confirms it is absent. Anonymous registry bearer-token pulls
of both its original tag and manifest digest now return404. This does not establish
that no one downloaded it before removal or that external copies can be recalled.

The original image was first exported privately and its configuration digest
verified against the installed-image record. Yaya authenticated dashboard RPC
reports the managed app running/ui-ready both before and after removal. Existing
containers and data were not modified. A future fresh pull of the deleted public
reference will fail; retain the on-node image and private archive until the private
replacement delivery/update path is qualified. No updated demo image is authorized
for public publication. Native login/player update still awaits live deployment.

The prior temporary SSH control connection no longer authenticates. Operator was
asked to restore access; dashboard RPC remains available. IndeeHub, V4V and wallet
recovery work have been reassigned to three active agents. Incomplete test runs are
being resumed, with heavy qualification staggered to avoid disk-pressure timeouts.

## Resumed private native-login deployment

The private candidate is now running on Yaya with image ID
`6ca1fe42d357ffa6a76abbb29db190e27953d647096391783baf57da4ad22366`
and pinned manifest digest
`sha256:4f28702e4f85368e4ad886f06d58b38f869c560c90ca40487bfaebe69090a870`.
The image was copied privately over SSH and loaded locally; it was not published
to a registry. The corrected operator-signed catalog canonical payload hash is
`9abadc9f535cb36d2d722b731b0b1088bd4dda0d30ced2e7fd0ab0b0a86d43e3`.

The first prepared catalog had a changelog string where the typed schema requires
an array. The live parser rejected it before updating the app. The previous signed
catalog/image were restored and the managed app returned to healthy/running.
Typed preflight also caught an unsupported provider bind source. Both errors were
corrected before requesting a new signature; a standalone validator using the
actual Rust catalog types and compiled canonical AppManifest parser now accepts
the corrected catalog and rejects the malformed original. Cryptographic signature
verification alone was insufficient as a schema preflight.

The provider is a byte-verified copy of the installed dashboard provider at
`/var/lib/archipelago/app-support/node-demo-v4v/nostr-provider.js`, mounted read-only
at `/app/vendor/archipelago-nostr-provider.js`. Refresh this copy from the qualified
dashboard provider during subsequent demo updates; it is not an automatic OTA hook.
The app can read it and its SHA256 matches the dashboard source.

Fresh consistent CURRENT managed data/media backups, app secrets/configuration,
and the old image were preserved at
`/home/archipelago/.local/state/archipelago/private-artifacts/v4v-managed-backups/20261006-resumed-private-update-fixed`
on Yaya. These are not copies of the original Portainer demo. An image export can
preserve the exact config/layers while changing manifest compression/digest: the
old signed pin remains cached for immediate rollback, and restoring an exported
image with a different manifest digest requires a separately signed private ref.

The normal update path exposed another lifecycle gap: after normal stop removes a
Quadlet container, update fails with "No containers found" during image inspection.
After the corrected signed manifest had been loaded, normal package.start created
the new managed container from that manifest and cleared the stopped marker. This
stopped-app update error remains a source regression to fix; successful start is
not evidence that the update RPC itself passed.

Final runtime checks verify exact candidate image ID, original named managed
volumes, unchanged session/receipt secrets, node session key/backend binary and
all unrelated app container identities/start times. Password/operator login is
off, native signer and Nostr registration are on, payments remain mock. Health
returns 200. Evidence: `/tmp/archy-v4v-private-running-verified.log`. Real native
login/playback/Companion acceptance remains open until browser/device results are
recorded. The original demo remains running unchanged.

The SSH access interruption described above is superseded: authenticated access
to Yaya and the actual Framework is restored. The latest dev/Yaya dashboard UI
also includes the deployed Lightning retry compatibility fix; native player and
banner changes remain present. The current live V4V browser qualification follows
the first-visit intro and explicit native identity/event consent before checking
actual login success; earlier click timeouts are retained as failed test evidence.
