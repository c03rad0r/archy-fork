# Audio playback and video picture-in-picture integration

## Status and reference

The `archipelago-v1` audio bridge is implemented in the dashboard. V4V is the
reference app; its updated real-node acceptance is tracked in
[v4v-native-player-20261006.md](v4v-native-player-20261006.md). Local tests are not
proof that a particular deployed app or companion version supports every feature.

Cloud video PiP is implemented in companion0.5.35/build55; the operator accepted
the delivered Cloud PiP flow on2026-10-07. Audio and video use separate channels.
Native background audio is operator-confirmed on companion 0.5.37/build57 for
the reported background playback and notification flow. The full physical
controls, queue, task-removal and recovery matrix remains open. Notification
artwork remains failed on the physical phone after the qualified dashboard
correction was deployed on dev and Yaya. The operator explicitly deferred further
artwork work until all other tasks are finished. APK57 remains unchanged. Browser and Robolectric
results do not establish phone acceptance.

## Declare the audio integration

Use the existing app manifest and normal signed-catalog installation process:

```yaml
app:
  # Other required manifest fields omitted here.
  metadata:
    launch:
      requires_host_frame: true
      media_controls: archipelago-v1
```

Declare the app's normal authenticated UI interface and readiness check. Use a
pinned image and persistent data mounts. An initial route such as `/browse` belongs
in `interfaces.main.path`; opening the retained player must not navigate back to
that route or create another iframe. A node-only demo must remain in its signed,
audience-restricted catalog until separately approved for public distribution.

Audio controls do not require Nostr signing. Apps needing Nostr login must follow
[the app developer guide](app-developer-guide.md) separately: explicit identity
selection, scoped consent, server-verified authentication and cancellation. Never
put signer permissions, wallet operations or signing keys into the media bridge.

## Public adapter and independent example

[`examples/audio-app`](../examples/audio-app/) contains an independently authored
adapter and a small player using files the user selects locally. It includes no
V4V code, assets, recordings or authentication implementation. Serve the example
as an authenticated app; configure the exact dashboard origin in its deployment
owned `dashboard-origin` metadata. Do not derive trust from a query parameter.

Import `attachAudioBridge` from `archipelago-audio.mjs` and supply your actual
player's `snapshot`, `actions` and `unlocked` callbacks. Call `publish()` when
playback changes and `dispose()` when the player is destroyed. The sample uses
one queue for both in-app and dashboard actions, and never passes stream URLs or
credentials to the host. Its Node tests run with
`node --test examples/audio-app/archipelago-audio.test.mjs`.

The generic eligibility change is under qualification: new sessions require the
authenticated daemon's verified catalog, matching app identity and installed
version, plus an unambiguous iframe-compatible manifest. Community storefront
fallbacks cannot grant this integration. An already admitted session retains its
original frame/origin/nonce solely so its existing controls and cleanup remain
usable if catalog freshness expires. App removal, version or frame replacement
releases that admission; a new session must qualify again.

## Integrate a new audio app

1. Add the manifest opt-in above and install the reviewed app through the normal
   catalog path. Confirm that the authenticated daemon reports the installed
   app/version and that its UI opens in the retained dashboard frame. A manually
   injected iframe or community listing is not an admission test.
2. Import the example adapter into the app, configure the exact deployment-owned
   dashboard origin, and attach it after constructing your player. `snapshot()`
   must return `title`, `artist`, `playing`, `shuffle`, `position`, `duration` and
   optional `artwork`; return `null` when no authorized track is available.
3. Implement `actions.play/pause/seek/next/previous/shuffle` using the app's existing
   player and queue. `seek` receives seconds; the adapter clamps to the current
   duration. `snapshot` is a protocol command handled by the adapter, not a second
   player action. Unsupported actions should remain unavailable in the app's own
   interface too; do not report controls that merely appear to succeed.
4. Supply `unlocked()` from actual application authentication/entitlement state.
   The adapter does not authenticate a user, buy a file or authorize playback.
   Subscribe `publish()` to player events, queue changes and login/logout state.
   The adapter has no playback polling timer; without these subscriptions the
   host will keep showing the last published snapshot.
5. On logout or entitlement loss, stop the player, clear its authorized source
   and publish the locked state. `available: false` hides host state but does not
   itself pause the app's audio element. On permanent teardown, unsubscribe your
   player callbacks and call `dispose()` (which also requests the app's pause
   action). Do not dispose simply because the dashboard panel closes.
6. Attach again in the document reached after an app-local login redirect. The
   adapter registers its listener before sending ready, reannounces on
   `pageshow`, and disposes on non-persisted `pagehide`; a back/forward-cache
   return keeps the live adapter. Preserve these lifecycle rules when wrapping it
   in a framework component.

The adapter serializes commands and checks the current handshake and unlocked
state before running each queued action. Authentication can still change while
an asynchronous app action is running: the application must abort or reject its
own unauthorized media request. Never use the host nonce as a media access token.

## Keep a single player and queue

The app continues to own its audio element, queue, authorization and playback
position. The dashboard retains the same iframe when the app panel closes and
shows the native bar. Reopening reveals that frame. Stop, logout and frame disposal
must release playback. Starting another audio source must not leave two players
running. Do not give the dashboard a protected stream URL to play a second copy.

Delegate previous, next and shuffle to the same actions used by the app's own
controls. Publish the resulting state back: do not optimistically maintain an
independent dashboard queue. For example, if Previous restarts a song after three
seconds in the app, it must do the same from the native bar.

## Message protocol: version 1

All messages use `window.postMessage` with an exact target origin, never `*`.
The dashboard accepts messages only from the retained frame's `contentWindow`
and its exact loaded origin, for an app declaring the manifest opt-in above.
The app accepts messages only from `window.parent` at its approved dashboard
origin. An unrelated sibling app, a popup or a matching hostname on a different
unapproved port is not an authorized parent.

Do not rely solely on `document.referrer`: an app-local login redirect changes
it. V4V's deployment uses the same hostname/scheme dashboard on its default port
and validates any supplied referrer against that topology. Apps deployed with a
different topology need an explicitly trusted installed parent origin, not an
origin accepted from a query parameter or the first message received.

| Direction | Type | Fields beyond `type` and `version: 1` |
| --- | --- | --- |
| App → host | `archipelago:media-ready` | None; emit after the bridge is ready, including after login navigation. |
| Host → app | `archipelago:media-connect` | `session`: per-frame random handshake nonce. |
| App → host | `archipelago:media-state` | Same `session`, plus playback fields below. |
| Host → app | `archipelago:media-control` | Same `session`, `command`, and `position` for seek. |

Only accept controls for the active handshake session. V4V validates session
format, checks origin and parent-window identity, and rejects controls while
locked. The nonce correlates this frame's messages; it is not a replacement for
origin/source validation or application authentication.

Example state:

```js
window.parent.postMessage({
  type: 'archipelago:media-state', version: 1, session,
  available: true,
  title: track.title, artist: track.artist ?? '',
  artwork: track.cover, shuffle: player.shuffleEnabled,
  playing: player.playing,
  position: player.currentTime, duration: track.duration,
}, approvedDashboardOrigin);
```

- `title` and `artist`: strings, limited to 256 characters.
- `position` and `duration`: finite, nonnegative seconds; clamp seek to duration.
- `playing`, `shuffle` and `available`: booleans reflecting actual app state.
- `artwork`: optional cover URL, at most 2,048 characters. The host accepts HTTPS
  or the app's own origin and rejects URL credentials. Do not include bearer
  tokens or private signing material in cover URLs. The bar shades the artwork
  to keep controls legible. Missing artwork must not prevent playback.
- Send `available: false` on logout, locked state or unavailable player, without
  leaking the previous user's track details.

Commands are `play`, `pause`, `seek`, `next`, `previous`, `shuffle` and `snapshot`.
`seek` carries `position` in seconds. Reject unknown commands and malformed fields.
Serialize actions that cannot overlap; publish a fresh snapshot after the action
settles. Register the message listener before emitting ready; release listeners,
subscriptions and timers on page disposal and restore them correctly on browser
back/forward-cache resume.

## Required app qualification

Use real installed metadata and signed catalogs as well as isolated unit fixtures.
The current host reference checks are
`tests/lifecycle/v4v-native-login.cjs` and `v4v-native-playback.cjs`.
They require explicit real-auth authorization and prohibit payment operations.
Adapt selectors and authentication proof validation to your app; never weaken
those guards merely to make a test pass.

1. Fresh install and update preserve app data, login, readiness and launch route.
2. Login navigation and reload still establish the media handshake. Reject forged
   origins, sibling frames, wrong sessions and malformed state/commands.
3. Play a real authorized test track, close the panel and prove its playback time
   advances in the same frame. Pause/resume, seek, previous/next and shuffle must
   match the app. Reopen without resetting the song or duplicating audio.
4. Verify correct title/artwork/shuffle after track changes, no stale state after
   logout, and handoff to another audio source without simultaneous playback.
5. Check 320/360/390px and desktop widths, long titles, touch targets, safe-area
   navigation, keyboard labels and reduced-motion preferences.
6. Test actual Android companion background/resume and exit behavior. A browser
   viewport test cannot prove OS background playback or PiP support. Do not promise
   playback after the operating system kills the process.

## Cloud video PiP contract and remaining acceptance

Start with Cloud's existing authorized video viewer and companion fullscreen host.
Provide an explicit PiP control when supported, with clear unavailable behavior.
Retain the same playback and authenticated connection; never put the entire node
management UI into a small PiP window. Preserve aspect ratio, play/pause, close,
return-to-viewer, background/resume, rotation, seek and completion behavior.

Cover supported Android/API levels, system PiP disabled, unsupported devices,
logout/session expiry, FIPS disconnect/reconnect, and process recreation. Validate
on a physical companion with ordinary and FIPS-accessed Cloud videos before APK
publication. Document the proven video integration contract for other app authors
only after that implementation is qualified.


The dashboard uses the main-frame-only `ArchipelagoCloudVideo` channel, admitted
only for the paired node's exact HTTP(S) origins. Its native `capabilities` action reports version 1 separately from the
`archipelago-v1` app audio integration. The current dashboard composable detects
the injected bridge and checks readiness during entry; a visible button is not
a guarantee that Android permits PiP. The Cloud host arms a
random request/session ID with video dimensions and playing state, then enters
fullscreen on the existing video in the same user gesture before requesting PiP.
Native commands and replies carry that session; a different session is ignored.
`state` updates playback controls and `release` retires the session. `restored`
returns to the same video; closing requests pause/cleanup. No video URL, cookie,
bearer token or second player crosses the channel. The entire dashboard must
never be used as the PiP surface. This is currently a Cloud-host contract, not
permission for arbitrary embedded apps to call native PiP directly.

## Reuse video PiP in a dashboard-owned viewer

The concrete host reference is
[`MediaLightbox.vue`](../neode-ui/src/components/cloud/MediaLightbox.vue).
These are dashboard-side integration points, not an exported video protocol for
third-party iframe apps. An app author can use the audio adapter above today;
embedding a new app's video into native PiP still needs a reviewed host integration
and its own qualification.

For companion entry, create `useCompanionVideoPip(videoRef)` from
[`useCompanionVideoPip.ts`](../neode-ui/src/composables/useCompanionVideoPip.ts) in the viewer's
Vue setup using the **existing** `HTMLVideoElement`. Wire an explicit button to
`enter()`, disable it while `busy.value`, and render `error.value` as an accessible
alert. Wait for nonzero video dimensions. Keep `enter()` in the original click
handler: it arms native state and requests that video's fullscreen together,
then requests native PiP only after both succeed. Do not first await unrelated
fetches or fullscreen the containing dashboard.

The composable retains the element and listens for play/pause/end/error; native
commands call that same element. It rejects stale-session events, times out an
unconfirmed bridge call after five seconds and shows a failure instead of
claiming entry. Normal native return clears the PiP session without pausing the
video; native close, end/error, `stop()` and viewer unmount stop it. As Cloud does,
call `stop()` before changing the item or hiding the viewer. Keep the viewer
mounted during companion PiP: unmount is intentionally a stop condition.

For browser PiP, check `isPipSupported()` and use `togglePip(video)` from
[`utils/pip.ts`](../neode-ui/src/utils/pip.ts). The toggle checks support at call
time and treats permission/transient failures as best effort. Ownership transfers
only on the actual `enterpictureinpicture` event, not on the button click:
[`usePipSession().adopt(video)`](../neode-ui/src/composables/usePipSession.ts) moves the original element into a document-level
host **before** closing/unmounting the viewer. The singleton then owns cleanup.
Browser `leavepictureinpicture` calls `release()`, pauses and clears/removes the
video; it does not implement companion's return-to-viewer behavior. Adopting a
second element releases the previous one. Avoid another cleanup path that clears
an adopted element's source during handoff.

Neither path refreshes credentials, reopens expired downloads or retries a paid
purchase. The caller remains responsible for the authorized media source and
transport. On logout, node switch, revoked access or unrecoverable transport loss,
stop companion playback and release any browser-owned session, then clear the
source using the normal viewer flow. Browser adoption outlives the component,
so component unmount alone is not a logout cleanup mechanism. Re-establish access
through the normal application flow before offering playback again. Physical
session-expiry and FIPS reconnection behavior remains unverified below.

The native implementation is
[`CloudVideoPip.kt`](../Android/app/src/main/java/com/archipelago/app/ui/screens/CloudVideoPip.kt).
It admits only the current paired dashboard origin and main frame; a matching
host on another scheme/port or a child iframe is not equivalent. Requests contain
an ID and action (`capabilities`, `arm`, `enter`, `state`, `release`); `arm` uses its
request ID as the session, and later actions carry it. Native entry additionally
requires the actual fullscreen custom view, the device PiP feature and Android
permission. The aspect ratio is bounded to Android's supported range. Callers
should reuse the composable rather than duplicate this private bridge protocol.

## Acceptance and reproducible checks

This table separates reported acceptance from source behavior and test coverage.
It is not a new test receipt. This guide update was a source/documentation review;
no tests, APK build, deployment or phone checks were run for it.

| Area | Evidence already recorded | Remaining acceptance |
| --- | --- | --- |
| Reusable audio adapter | Independent example and protocol tests; V4V reference and existing dashboard qualification recorded above | Qualify each new installed app, its authentication and its own queue; the example is not app certification |
| Companion Cloud PiP | Delivered 0.5.35/build55 flow accepted by operator on 2026-10-07; source tests cover fullscreen gating, origin/session rejection and unavailable behavior | Disabled/unsupported system PiP, rotation, full close/return/completion matrix, session expiry, FIPS interruption and process recreation on the physical phone |
| Companion background audio | Operator reports 0.5.37/build57 background playback works and notification appears | Full controls, headset/network interruption, task removal, retained queue/position and recovery matrix; process-kill continuity is not promised |
| Notification artwork | Local/browser/native image checks recorded below; phone still showed no cover | Explicitly deferred to task 23 after other work; do not treat it as passed |
| Browser video PiP | Existing source fixtures cover element adoption before viewer close, singleton release and call-time support check | Browser/device and authenticated-source checks for each new viewer; native acceptance does not certify this path |

For a change to these contracts, run only the relevant existing checks in an
available qualification window. From the repository root:

```sh
node --test examples/audio-app/archipelago-audio.test.mjs
cd neode-ui
./node_modules/.bin/vitest run src/composables/__tests__/useCompanionVideoPip.test.ts src/components/__tests__/MediaLightboxPip.test.ts src/composables/__tests__/usePipSession.test.ts
cd ../Android
./gradlew :app:testDebugUnitTest --tests 'com.archipelago.app.ui.screens.CloudVideoPipTest'
```

The audio-specific checks are listed below. Record the source commit, selected
checks and result separately from server URL/dashboard revision, APK version and
physical-device results. Serve the matching reviewed dashboard and signed APK
through the established deployment process; verify the delivered artifact rather
than assuming a rebuilt source file reached the phone. On the paired server use
an already authorized track/video. Neither this guide nor a fixture permits a
payment, new signing consent or live catalog change just to obtain test media.
If background playback fails, use **Menu → Playback diagnostics → Copy report**
on build57; do not request credentials, stream URLs or paid-media details.

## Companion native audio: build57 background playback operator-confirmed

On 7 October APK0.5.36 failed after locking or switching apps, without a native
notification. Restarting did not resolve it. Build57 added local diagnostics;
the operator subsequently reports that background playback now works and the
notification is present, but song artwork is missing. This accepts the reported
background flow only; the complete controls, queue and recovery matrix below
remains open. No definitive cause of the earlier failure is established. USB
access is unavailable. The artwork origin correction passes 26 focused dashboard
tests, production browser checks at 390/1440px and two native JPEG tests on API
28/35. Guarded dashboard deployment and deployed browser checks passed on dev
and Yaya (receipt d23da226). Physical artwork acceptance failed: the operator subsequently reported that the actual phone still shows no cover.
Further artwork work is explicitly deferred until the end of all tasks.


Apps continue using the existing version1 app audio protocol above. They do not
need a second Android stream or a separate queue implementation. The dashboard
owns a main-frame `ArchipelagoAudio` channel restricted to paired node origins;
embedded app frames cannot invoke it directly. The foreground `mediaPlayback`
service owns the retained WebView session after the Activity/task closes and
exposes an Android MediaSession with playback metadata, play/pause, previous/next,
seek, shuffle and Stop. Playback state is confirmed by the existing app player,
not optimistically advanced by native controls.

Dashboard→native messages contain version1, a random session, monotonically
increasing sequence, bounded title/position/duration, playback/control flags and
optional JPEG thumbnail bytes. Artwork is fetched by the already authenticated
page with same-origin credentials only, capped at512KiB, resized to192px and sent
as a bounded data URL. The qualified correction admits HTTP covers on the
verified app origin, including its distinct port, while rejecting arbitrary HTTP
origins and URL credentials. Cross-origin requests omit cookies and require CORS;
redirects are rejected and no referrer is sent. Native code does not fetch artwork
URLs or receive auth credentials. Cross-origin images without CORS may have no notification thumbnail;
missing artwork never blocks playback. Native image decoding bounds dimensions.

Native→dashboard controls carry the same session. Retired sessions, stale sequence
numbers, foreign origins and subframes cannot revive/control playback. A short
heartbeat resynchronizes state; if the dashboard stops responding for90seconds,
the native owner stops instead of advertising a live session indefinitely.
Playback stays in the same authenticated WebView/iframe; app authorization and
entitlement checks remain with that player. App removal, frame replacement,
logout, navigation/disconnect or explicit Stop release the corresponding session.
A non-playing task removed from Recents is released. A playing one is retained;
reopening reattaches the existing document, queue and position. A killed process
is not automatically restarted into an authenticated stream.

The implementation uses the platform MediaSession with the existing WebView
player, rather than adding another decoder. No boot receiver or new storage
permission is involved. Pair this APK with the matching dashboard build: an older
dashboard does not send the native audio protocol merely because the APK changed.

Qualification commands:

```sh
cd neode-ui
./node_modules/.bin/vitest run src/composables/__tests__/useCompanionAudio.test.ts src/composables/__tests__/useAppMediaBridge.test.ts src/components/__tests__/GlobalAudioPlayerExternal.test.ts
cd ../Android
./gradlew :app:testDebugUnitTest
```

Before marking physical acceptance, use V4V on the matching server: play a track,
close its panel, press Home, lock the phone, pause/resume/seek/skip/shuffle from
native controls, remove the companion task while playing, reopen into the same
queue/position, then Stop. Repeat logout, headset disconnect and network loss.
Confirm download, fullscreen and Cloud PiP still work. App force-stop/process
kill is a stop condition, not a promise of uninterrupted playback.
