# Companion app — soft-keyboard viewport handover

**Audience:** the companion (Android WebView wrapper) developer.
**Reported:** 2026-08-04 by the operator — in chat, when the soft keyboard opens,
padding is added to the bottom tab bar and the page scrolls; the chat window
should instead scale to the height that remains above the keyboard.

## Why this is (most likely) companion-side

The symptom described — content keeps its full height, the browser *pans/scrolls*
the focused input into view, and the fixed bottom bar picks up a visual gap — is
the classic Android `adjustPan` (or edge-to-edge-without-IME-insets) signature.

The web side already implements the correct contract, verified in-repo:

1. **`neode-ui/index.html`** carries
   `interactive-widget=resizes-content` in its viewport meta. In Chrome 108+
   this makes the keyboard resize the **layout** viewport, so
   `window.innerHeight` shrinks.
2. **`neode-ui/src/main.ts` → `syncViewportHeightVar()`** mirrors
   `window.innerHeight` into the CSS var `--visual-viewport-height` on
   `resize`, `orientationchange`, and `visualViewport.resize`. It deliberately
   uses `innerHeight` (not `visualViewport.height`) so the value shares a
   reference frame with `position: fixed` elements like the mobile tab bar.
3. **`neode-ui/src/style.css`** sizes the mobile layout (including the chat
   iframe container) from
   `var(--visual-viewport-height, 100dvh)` minus the tab-bar/safe-area vars.

So on mobile web Chrome, the keyboard shrinks `innerHeight`, the var updates,
and the chat scales. **An Android WebView ignores the `interactive-widget`
meta entirely** — keyboard resize there is governed by the host app. If the
host pans instead of resizing, no amount of web CSS can fix it: the WebView's
`innerHeight` never changes and the system scrolls the page instead.

## What the companion app should do

Pick the branch that matches how the Activity is configured:

### A. Not edge-to-edge (no `WindowCompat.setDecorFitsSystemWindows(window, false)`)

Set the soft-input mode so the WebView is *resized*, not panned:

```xml
<!-- AndroidManifest.xml, on the Activity hosting the WebView -->
<activity
    android:name=".MainActivity"
    android:windowSoftInputMode="adjustResize" />
```

`adjustPan` (and on some OEM builds the historical default `adjustUnspecified`)
produces exactly the reported behaviour.

### B. Edge-to-edge (decorFitsSystemWindows = false)

`adjustResize` alone stops working in edge-to-edge; you must consume the IME
inset yourself and resize the WebView:

```kotlin
ViewCompat.setOnApplyWindowInsetsListener(webViewContainer) { view, insets ->
    val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
    val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
    // Resize the container to end above the keyboard (or the nav bar when closed)
    view.updatePadding(bottom = maxOf(ime.bottom, bars.bottom))
    WindowInsetsCompat.CONSUMED
}
```

(If targeting SDK 35+, edge-to-edge is enforced, so branch B is the one that
applies.)

### Do NOT compensate on the web side

Please don't inject extra bottom padding, margins, or scroll offsets into the
page from the wrapper — the web layout already subtracts the tab bar and safe
areas from `--visual-viewport-height`, and wrapper-side compensation double
counts (that is the "padding added to the tabs" half of the symptom).

## How to verify the fix

In the WebView's remote-debug console (`chrome://inspect`), focus the chat
input and check:

- `window.innerHeight` **shrinks** by roughly the keyboard height → correct
  (branch A/B working). The chat window will scale; no page scroll.
- `window.innerHeight` **unchanged** while `window.visualViewport.height`
  shrinks → the WebView is still panning; the manifest/insets change hasn't
  taken effect.

## Web-side status (for completeness)

- neode-ui (the page the companion actually loads): already correct, no change
  needed.
- AIUI standalone (`aiui/packages/app/index.html`): was missing the
  `interactive-widget` token; added 2026-08-04 for parity. Only affects AIUI
  used outside a node, not the embedded/companion path.
- iOS Safari / iOS WKWebView: `interactive-widget` is not supported there. If
  an iOS wrapper appears later, the equivalent is
  `KeyboardLayoutGuide`/`keyboardWillChangeFrame` driving the WKWebView frame —
  same principle: resize the web content, never pan it.
