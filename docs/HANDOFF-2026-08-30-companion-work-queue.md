# HANDOFF — companion-agent work queue (2026-08-30)

**For: the companion agent.** Compiled from the 2026-08-30 issue-triage
session. The tracker now labels the companion-owned issues `companion-agent`
(#128, #139); this document adds the pointers and one small residual that
isn't worth its own issue until it's being fixed.

## Pointers

- **App source:** `Android/` in this repo (Kotlin/Gradle). Release notes
  live in `Android/COMPANION_RELEASE.md`.
- **Served artifact:** `neode-ui/public/packages/archipelago-companion.apk`
  + `archipelago-companion.json` (currently **0.5.27 / versionCode 47**).
  Shipping a companion change means refreshing both in the same commit
  (versionCode +1) plus a COMPANION_RELEASE.md entry; nodes serve the file
  from the web bundle. The deploy/verify pipeline (aapt badging, size
  checks, node redeploy) is documented in
  `docs/HANDOFF-2026-07-23-companion-apk-deploy.md`.
- **Web bridge:** `window.ArchipelagoNative` (JS interface the WebView
  injects); `isCompanionApp()` in `neode-ui/src/utils/openExternal.ts` is
  the canonical detection helper; `appLauncher.ts` shows the gating pattern.

## Work queue

### 1. Residual of #61 — companion-gate the store banner + intro overlay (small)

What #61 fixed was the AUTO-popup: `CompanionIntroOverlay` skips its
mounted auto-show when `IN_COMPANION_APP` (the `ArchipelagoNative` bridge
is present). Two paths are still ungated, so a user already inside the
companion WebView still gets "install the companion" pitches:

- `<CompanionBanner />` in `neode-ui/src/views/Discover.vue:156` renders
  unconditionally.
- `openCompanionIntro()` (`neode-ui/src/composables/useCompanionIntro.ts`)
  is an explicit trigger that intentionally bypasses the once-per-browser
  gate — but nothing companion-checks its callers.

Fix: gate the banner render and the intro-trigger entry points on
`isCompanionApp()`, same pattern as `appLauncher.ts` (lines ~236/~341).
Verify inside the companion WebView (banner absent, no manual path can pop
the overlay). Land it in the web UI here; the APK doesn't change.

### 2. #128 — GrapheneOS phone backup & restore (feature)

Reporter's problem: losing your phone, or wiping it to cross a border.
Reporter's suggestion: "part of the companion app or passport prime combo".
The companion owns the phone side: trigger a GrapheneOS backup, transport
it, and restore it onto a wiped device — coordinated with the node's
existing encrypted-backup envelope (ADR-005: ChaCha20-Poly1305 +
Argon2id, `core/archipelago/src/backup.rs`). **Reuse that envelope; do not
invent a second backup format.** Node-side storage/quota/scheduling is
tracked separately on the roadmap — coordinate before assuming node-side
surface beyond the existing backup RPCs.

### 3. #139 — Nostr Bunker: companion-side remote signer (feature)

"Remote signer with companion app?" — the phone side of NIP-46: a bunker
client in the companion (pairing with a node-side bunker service via
QR/URI, a signature approve/deny UX that makes what's being signed legible,
and saved-remote-bunker management). Background research already exists:
`docs/nostr-signer-login-research.md`. The node-side bunker hosting is
roadmap-tracked separately; this issue's companion label covers the
phone-side integration.

## Working rules (same as the node repo)

- Small commits, pushed immediately; vitest for web-side changes; the
  Kotlin app's on-device flows get verified on a real device before the
  APK ships.
- Node-side Rust changes are out of scope for the companion queue —
  anything that needs them goes through the labeled issues on the tracker.
- Done = artifact refreshed (APK + json meta) so a web-bundle deploy can
  serve it, plus the issue updated with what shipped.
