# V4V native player and music banner follow-up

## Source changes

- The existing native player now offers previous, next and shuffle. Each command
  delegates to V4V's existing action, so V4V owns the queue and shuffle ordering.
  Shuffle state and the current cover come back from the same app snapshot.
- Cover artwork sits under a dark contrast layer; malformed or credential-bearing
  URLs are ignored. Remote artwork requires HTTPS. No media URL, payment token or
  second audio element is handed to the dashboard.
- The app iframe remains mounted while hidden. The app panel and bottom bar have
  complementary transitions, disabled for reduced-motion preferences.
- V4V's same-host default-port media handshake survives a same-app login redirect
  or absent referrer. Parent window, exact origin and per-session nonce checks
  remain required. Unrelated host/sibling-port referrers are rejected.
- The Sovereign Music banner uses the app's final settled cymatic field, not an
  early intro frame: the original pattern, zero offset, particle lift 0.88 and
  `renderFrame(0)`, matching `setSettledPattern`. The capture uses a fixed random
  seed and the app's actual `cymatics.mjs`. A decorative record/play symbol and
  waveform appear on the right, subdued on small screens.

## Validation

- App media protocol: 4 tests passed, including login redirect boundaries and
  delegation of all three queue controls.
- Dashboard bridge/audio/catalog: 18 tests passed.
- Native player rendering: 1 test passed, including cover, authoritative shuffle,
  control delegation, visibility on reopen and absence of a second audio element.
- Vue type check passed.
- Real Chromium component rendering at 390 and 1440 pixels: all six buttons
  visible; previous/next/shuffle reached the controller; no horizontal overflow.
  Reduced-motion rendering exercised. This is an isolated component fixture,
  not live app integration acceptance.
- The operator reported that the existing bottom-bar transition is now working.
  This is separate from the automated evidence above.

## Still required after deployment

On Yaya, play a real song, close the app, operate previous/next/shuffle from the
native bar, and reopen the same iframe. Verify uninterrupted audio, matching
song/cover/shuffle, and the app's actual queue. Repeat at mobile and desktop
sizes and through the new Nostr login. Verify banner visually with the signed
node-only catalog. This follow-up does not claim those live checks passed yet.


## Resumed dashboard deployment — 6 October

The previously completed full dashboard suite passed 1,307 tests in 165 files;
the production build passed. Both logs survived the session interruption, and the
UI source remains unchanged from that build. The resulting dashboard was deployed
to the dev node with index SHA-256
`a03f3e7a366613f82dd9fe014dc04301f223b9896f6684bf2ee5aa06aa59b2de`.
Served bytes match; backend, session key and app container identities/start times
were preserved. A rollback archive/script was saved in the node support directory.
Post-deployment browser checks are in progress. This is dashboard deployment only;
Yaya and the new private V4V app image still require deployment/acceptance.
