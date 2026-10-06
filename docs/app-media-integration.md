# Audio playback and video picture-in-picture integration

## Status and reference

The `archipelago-v1` audio bridge is implemented in the dashboard. V4V is the
reference app; its updated real-node acceptance is tracked in
[v4v-native-player-20261006.md](v4v-native-player-20261006.md). Local tests are not
proof that a particular deployed app or companion version supports every feature.

Cloud video picture-in-picture (PiP) in the Android companion is a separate open
implementation task. Do not advertise the audio bridge as a video/PiP API. Native
fullscreen support alone does not provide Android PiP. A versioned video contract
and examples must be added here when implementation and device tests pass.

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

## Cloud video PiP acceptance scope (not yet implemented)

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
