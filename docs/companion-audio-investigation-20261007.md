# Companion background audio investigation — 2026-10-07

Status: **OPEN — physical acceptance failed.**

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
- Physical background playback remains **unaccepted**; the diagnostic report is
  the next evidence required from the affected phone.

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
