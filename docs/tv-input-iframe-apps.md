# TV input: keyboard/gamepad inside iframe apps — design

**Goal (2026-07-23, user requirement):** on a TV/kiosk node, keyboard and
gamepad control must work *inside iframe apps* (IndeeHub, Jellyfin, fedimint
UI, AIUI…), easily and globally — no per-app hacks.

## What already exists

- `neode-ui/src/composables/useControllerNav.ts` — a complete spatial-nav
  system for the shell: reads gamepads (`navigator.getGamepads`), moves focus
  between `data-controller-container` regions, plays nav sounds. It stops at
  iframe boundaries: nothing is forwarded into frames.
- Keyboard focus DOES enter iframes natively (click/Tab into the frame), and a
  focused iframe receives all keys — same- or cross-origin. The gap is
  gamepad→app and deliberate focus handoff shell↔frame.
- Kiosk Chromium is our process (archipelago-kiosk-launcher), X11, and the ISO
  already ships `xdotool`. Most app iframes are **cross-origin**
  (`http://host:port`), so shell-side script injection is impossible for them;
  only the nginx-proxied `/app/...` ones are same-origin.

## Recommended architecture — two layers, both global

### Layer 1 (OS, kiosk nodes): gamepad → virtual keyboard, kernel-level

A small host daemon (`archipelago-gamepad-keys`) on kiosk nodes:

- reads game controllers via evdev (`/dev/input/event*`, capability
  BTN_GAMEPAD), hotplug-aware (udev monitor or 5s rescan — same pattern as the
  audio router);
- emits a **uinput virtual keyboard**: D-pad/left-stick → arrow keys, A →
  Enter, B → Escape, X → Space (play/pause), Y → `f` (fullscreen in most
  players), shoulders → Tab / Shift+Tab, Start → Enter, Select → Escape;
- ships exactly like the audio router: `image-recipe/configs/` script + unit,
  spliced into the ISO, `include_str!` self-heal in `bootstrap.rs`, gated on
  the kiosk being installed. The `archipelago` user is in `input` group OR the
  unit runs as root (uinput needs it anyway — run as root, it's ~100 lines of
  evdev→uinput with no network).

Why this layer wins: the browser sees a real keyboard, so **every iframe —
any origin, any app — just works** the way it does for a physical keyboard
today. Video players, web games, AIUI: all of them already have keyboard
bindings. Zero app cooperation, zero web-platform security fights.

### Layer 2 (shell): deliberate focus handoff into/out of frames

Small extension to `useControllerNav`:

- When spatial nav selects an app-session container and the user presses
  A/Enter: call `iframe.focus()` (works cross-origin) — keys (real or
  virtual) now flow into the app.
- A dedicated **exit chord** the daemon maps from the gamepad (e.g. Home
  button → F12 or a rarely-used key): the shell listens with a *capturing*
  window listener; on seeing it, `iframe.blur()` + return focus to the shell
  nav. Keyboard users get the same via a documented chord (e.g. long
  Escape / Ctrl+Escape — plain Escape stays with the app, players use it).
- Same-origin frames (the `/app/...` proxied set) can additionally get the
  full spatial-nav treatment by running the existing nav over
  `iframe.contentDocument` — nice-to-have after the layers above land.

### Optional layer 3 (per-app polish): postMessage contract

For OUR app UIs only (AIUI, fedimint, launcher pages): a tiny
`archipelago:input` postMessage contract for semantic actions (back, home,
context-menu) where raw keys aren't expressive enough. Documented in the app
packaging docs; never required for an app to be usable.

## What NOT to do

- ❌ CDP (`--remote-debugging-port` + Input.dispatchKeyEvent): works but adds
  a privileged debug port to the kiosk and a daemon↔browser coupling; the
  uinput route gets the same result at kernel level with no attack surface.
- ❌ Per-app nav scripts injected into iframes: cross-origin makes this
  impossible for most apps, and it's exactly the per-app hack the requirement
  rules out.

## Implementation order

1. `archipelago-gamepad-keys` daemon (evdev→uinput, ~python3 stdlib or small
   Rust bin) + unit + ISO splice + bootstrap self-heal. Test on Framework PT
   with any USB/BT controller.
2. `useControllerNav`: A-button → `iframe.focus()` on the focused app session;
   exit-chord capture listener to reclaim focus.
3. (Later) same-origin spatial nav inside `/app/...` frames; postMessage
   contract for our own app UIs.
