# Companion background audio investigation — 2026-10-07

Status: **OPEN — operator now reports APK57 background playback working; notification artwork missing.**

Latest operator feedback after installing 0.5.37: "works now, doesn't show the song
image in the notification widget though only." Record reported background audio
and notification presence as working for that test. Do not infer full queue/control,
restart, process-death or video acceptance, or a confirmed cause for the earlier
failure: build57 added diagnostics rather than a known playback correction.
Artwork investigation is now the priority. Historical APK56 failure follows.

APK 0.5.36 (build 56) plays V4V while foregrounded, but the operator reports audio
stopping about five seconds after locking the phone or switching applications,
with no playback notification. Restarting the companion did not fix it. USB is
unavailable. Do not ask for USB again or repeat the restart/demo-track instructions.
The V4V track the operator already used is valid; no separate demo is required.

The 29 Android tests and dashboard/browser native-bridge simulation passed for
build 56. Those checks did not establish real WebView-to-Android messaging or
OS background playback. Physical evidence overrides the earlier nearly-finished
assessment. No confirmed root cause or correction has yet been established.

## Source investigation

- The packaged manifest contains the nonexported mediaPlayback foreground service
  and required permissions. The bridge accepts only the paired dashboard main
  frame and sends metadata/control, never the authenticated stream URL.
- Existing Robolectric tests call the bridge handler directly. Browser tests use
  a mocked native bridge. Neither tests injection on the affected phone.
- Unsupported WebView messaging, origin rejection, malformed state, and absent
  native messages previously had no user-readable diagnosis. Startup exceptions
  are handled but only emit a generic page error.
- The separate native browser overlay for apps opened outside the dashboard does
  not attach this dashboard media bridge. Whether the operator used that route
  is unknown; do not assert this is the cause or broaden trusted origins.
- Missing notification does not prove notification permission is the cause. The
  diagnostic separates whether a native state/service was ever established.

## Diagnostic build 0.5.37 / 57

Android-only, with no speculative playback behavior change. Adds Menu → Playback
diagnostics, Refresh, and Copy report. The report includes app/Android/WebView
versions, notification/channel status, native session/service presence, allowlisted
bridge acceptance/rejection and service stage counts, and at most 12 recent stage
transitions. No URLs, titles, session identifiers, credentials, raw exceptions,
automatic upload, or persistent logging. Counters are in memory and reset on app
process restart. The operator chooses whether to copy/share the report.

Interpretation:

- BRIDGE_UNSUPPORTED / NO_APPROVED_ORIGIN: bridge did not attach.
- BRIDGE_ATTACHED with no MESSAGE_RECEIVED: investigate dashboard client/loading
  and whether playback opened outside its retained iframe.
- SENDER_REJECTED / INVALID_STATE: investigate the corresponding protocol boundary;
  do not weaken origin or input checks.
- SERVICE_REQUESTED without FOREGROUND_ACTIVE: investigate Android service startup.
- FOREGROUND_ACTIVE followed by stopping: investigate lifecycle, service ownership,
  notification settings and WebView background policy with those actual stages.

The build is diagnostic, **not a fixed/accepted background-audio release**. After
installation, play the same V4V track, reproduce once, reopen Menu → Playback
diagnostics and copy the report. No need to use a different track or USB.

## Validation

- All **33 Android unit/Robolectric tests passed**, zero failures/errors/skips,
  including SDK 28/35 privacy and bounded-history regressions. XML was preserved
  before the canonical clean package build.
- Canonical clean assemble/sign verification passed v1/v2/v3 and the original
  signer. APK SHA256:
  `9b63bd928ab396278791ab3b514dbb53daaf4b7de9311e8e4f505acc02594a2c`.
- Delivered and HTTP bytes verified at
  <http://192.168.63.240/packages/archipelago-companion-0.5.37.apk>.
  The live dashboard, generic APK/JSON and versioned APK 0.5.36 remained byte-for-byte
  unchanged. Candidate generic APK/JSON were restored after the canonical script.
- 85 captured Android/package source input hashes remained unchanged through
  packaging. No keystore was staged, copied into evidence, or replaced.
- Durable evidence: `~/.local/state/archipelago/release-qualification/companion-057/`.
  No wallet, payment, catalog, OTA, ISO, or native node-service deployment changed.
- At diagnostic delivery physical background playback remained unaccepted. The
  later operator report above establishes it worked in their APK57 test; artwork
  remains unresolved on the phone.

## Companion shell routing check

A read-only browser check on Yaya at 390 px injected `ArchipelagoNative`
(openInApp, openInAppEx, openExternal) alongside the mocked audio bridge, then
used the existing qualification entry `openSession('node-demo-v4v')` from Discover.
It retained one :7475 dashboard iframe, `panelApp` and `mediaApp` were both
`node-demo-v4v`, and native navigation calls were empty. Zero signing/payment
operations were attempted. Thus the managed entry also retains its host frame
when the companion shell is detected. This still does not prove actual Android
WebView injection or the affected phone's navigation path.

An initial Apps-button probe found no V4V card/package on `/dashboard/apps`; it
therefore did not exercise an actual Apps launch button. Keep that failed probe
distinct from the subsequent successful Discover/launcher check. No routing fix
is justified by these results. Scripts/logs are in the durable diagnostic evidence.

## Artwork origin correction

Confirmed source defect: `useAppMediaBridge` admits cover URLs on the verified
app's origin (for example the node's HTTP :7475 port), but `useCompanionAudio`
previously rejected every HTTP cover outside the dashboard's own origin. Thus a
cover could render in the dashboard and never become a native thumbnail.

The media controller now carries its admitted origin explicitly into the player;
an app's `media-state` payload cannot supply/override this authorization. Thumbnail
fetching accepts HTTP only from the dashboard or that admitted origin. HTTPS
artwork remains supported. Cross-origin requests do not send cookies, no referrer
is sent, and redirects fail closed. No stream URL, cover URL, credentials, or
second decoder enters the native bridge. Optional image failure still leaves
playback running. CORS remains enforced; this does not bypass an image server's
access policy or add a server-side image proxy.

Evidence:

- Focused four-file media suite: 26 tests passed. A test-only `Array.at` call was
  corrected for the configured ES library after actual app-project typechecking;
  the affected file was rerun separately.
- `vue-tsc --noEmit -p tsconfig.app.json` passed. The reference-only root tsconfig
  has no files; its bare `--noEmit` result is not used as application evidence.
- Actual Chromium A/B fixture with an HTTP app on a separate port: old composable
  never produced a thumbnail; corrected composable produced a real JPEG. Browser
  checks also verified no cross-origin cookie, no followed redirect, and playback
  remained active after the optional image error.
- No native runtime or APK version change. The focused positive native JPEG
  regression passed on SDK 28 and 35 (2 tests, zero failures/errors/skips). It
  verifies the actual service places a real JPEG in the notification large icon
  and media-description bitmap, retains it on position-only updates, and clears
  it for an explicitly artless following song. Initial test compilation used
  unavailable Java desktop ImageIO APIs; replacing that test-only fixture with a
  fixed Chromium-generated JPEG resolved the harness issue. Production Kotlin
  remained unchanged/up-to-date. All owned test JVMs exited after success.
- This identifies and fixes a real artwork boundary defect, not the operator's
  exact cover URL. Physical notification-artwork acceptance remains open until
  the qualified UI reaches the node and the operator verifies it.

Evidence folder: `~/.local/state/archipelago/release-qualification/companion-artwork-20261007/`.

## Actual app artwork access limitation

A separate read-only Chromium probe retained the managed node-demo-v4v iframe,
but the actual app required Nostr login before initializing its media catalog.
With signing and payments blocked, the probe timed out before obtaining a cover
URL. No image access/CORS policy was bypassed, no credentials were forwarded,
and no real app artwork was decoded in that probe. The successful browser A/B
fixture and native JPEG test establish the corrected pipeline, not that the
operator's particular cover is anonymously accessible. Physical artwork acceptance
remains pending after the qualified UI deployment; APK 0.5.37 stays unchanged.

## Artwork UI delivery completed (7 October, after APK57 feedback)

The UI correction is now deployed to **dev (192.168.63.240) and Yaya
(192.168.63.169)**. Both serve index SHA256
`92050cbda69954f57f4b0bdd73d89623f6aae9fc66e4c074ee4f46bdcb9c3b2a`.
The archive SHA256 is
`66473241eb43aab979a010db62046eb1eba3814d369aef147ded4bce0ba06a6e`.
APK 0.5.37 and native runtime remain unchanged; no APK replacement is needed
for this UI correction. Actual notification artwork on the phone still needs
operator acceptance after the updated dashboard loads.

Qualification was a frozen 707-input production build, with all input hashes
unchanged before/after. Evidence combines the preceding 1,448-test/176-file full
UI baseline with the 26 focused artwork tests, app-project typecheck, browser A/B
fixture and two positive native JPEG tests. It does **not** claim a full UI-suite
rerun at this updated source.

The frozen production artifact passed at 390/1440 using actual Yaya session,
verified catalog and installed app identity with an isolated synthetic media
iframe. It emitted a real JPEG to the mocked native bridge, omitted cross-origin
credentials from thumbnail fetches, rejected redirects and retained playback.
Initial intercepted-main-document attempts caused Chromium network failures and
login redirects; another attempt on dev could not admit the absent demo package.
Those failed harness logs are retained. Navigating to an actual Yaya document
before inserting frozen HTML resolved the harness boundary without disabling
browser security or changing authentication.

Guarded UI-only delivery preserved backend bytes, catalogs, session secret,
stopped/uninstalled intent, every package download and AIUI file. Dev checked
178 preservation paths and 32 containers; Yaya checked 174 paths and 30 containers.
IDs/start times remained unchanged, including a delayed 15-second check. Both
nodes have protected backups and automatic rollback scripts. No node service,
wallet, catalog, payment or signing operation changed.

Post-deployment live browser checks passed at 390/1440: dev dashboard/Fleet
recovery smoke and Yaya's synthetic artwork-to-native-bridge pipeline. The latter
still uses a mock native bridge and fixture cover; it does not establish that the
operator's actual cover passes its server's anonymous/CORS policy or appears on
the phone. Served index hashes were verified again after those checks.

Build, failed/successful browser logs, source manifest, protected preflight data,
qualification archive and `delivery-receipt.json` are in
`~/.local/state/archipelago/release-qualification/artwork-ui-20261007/`.


## Operator artwork result and deferral

After the guarded dev/Yaya UI deployment, the operator reported that the notification song image still did not appear. Physical artwork acceptance therefore FAILED and remains open; the earlier synthetic/browser/native fixture evidence did not establish this phone outcome. Background playback and notification presence remain operator-accepted. The operator explicitly moved artwork to the end of all remaining tasks. No further artwork diagnostics, APK or UI rebuild will be undertaken until that deferred task resumes.
