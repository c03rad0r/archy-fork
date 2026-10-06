# Node-scoped demo apps and persistent media

Status: implementation under qualification; **not deployed or accepted**.

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
