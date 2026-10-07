# Audio playback and video picture-in-picture integration

## Status and reference

The `archipelago-v1` audio bridge is implemented in the dashboard. V4V is the
reference app; its updated real-node acceptance is tracked in
[v4v-native-player-20261006.md](v4v-native-player-20261006.md). Local tests are not
proof that a particular deployed app or companion version supports every feature.

Cloud video PiP is implemented in companion0.5.35/build55; the operator accepted
the delivered Cloud PiP flow on2026-10-07. Audio and video use separate channels.
Native background audio is a companion0.5.36/build56 candidate:29 Android tests
and six companion-bridge dashboard tests pass; physical background/lock-screen/task
removal acceptance remains required. Do not infer those phone results from
browser or Robolectric tests.

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
  title: track.title, artist: track.artist,
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
only for the paired node's exact HTTP(S) origins. Its version1 capability check
is separate from `archipelago-v1` app audio integration. The Cloud host arms a
random request/session ID with video dimensions and playing state, then enters
fullscreen on the existing video in the same user gesture before requesting PiP.
Native commands and replies carry that session; a different session is ignored.
`state` updates playback controls and `release` retires the session. `restored`
returns to the same video; closing requests pause/cleanup. No video URL, cookie,
bearer token or second player crosses the channel. The entire dashboard must
never be used as the PiP surface. This is currently a Cloud-host contract, not
permission for arbitrary embedded apps to call native PiP directly.

## Companion native audio: build57 background playback operator-confirmed

On 7 October APK0.5.36 failed after locking or switching apps, without a native
notification. Restarting did not resolve it. Build57 added local diagnostics;
the operator subsequently reports that background playback now works and the
notification is present, but song artwork is missing. This accepts the reported
background flow only; the complete controls, queue and recovery matrix below
remains open. No definitive cause of the earlier failure is established. USB
access is unavailable. Artwork handling is under investigation/correction.


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
as a bounded data URL. Native code does not fetch artwork URLs or receive auth
credentials. Cross-origin images without CORS may have no notification thumbnail;
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
