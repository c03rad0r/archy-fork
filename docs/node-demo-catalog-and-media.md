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
