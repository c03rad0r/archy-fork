# AIUI Plan 2 — Extended Roadmap

## Context & Philosophy

This plan continues from M0–M7 (all complete). Every item below must honour the core philosophy:
- **Glass morphism only** — `glass`, `glass-card`, `glass-button`. No light mode, no gray-900 hacks.
- **Open source / MIT/Apache-2.0** — no proprietary dependencies
- **Decentralised-first** — no vendor lock-in, pluggable everything
- **Bitcoin only** — sats, Lightning, Cashu, Fedimint. AIUI is never a wallet — always deep-link
- **Privacy-first** — no telemetry, no tracking, E2E encryption
- **Mobile-first, everywhere-perfect** — desktop enhances mobile, never replaces it
- **Plugin-everything** — all integrations go through typed plugin interfaces
- **< 250 KB gzipped initial load** — everything else lazy-loaded

---

## M8: Chat UX Polish

### M8.1 — Message Editing & Regeneration
Edit any sent message in place; all messages after it are cleared and AI regenerates from that point. Pencil icon appears on hover. Textarea replaces bubble on click. `Escape` cancels, `Enter` submits.

### M8.2 — Conversation Branching
Fork from any assistant message. Branch indicator in chat header (e.g. "Branch 2 of 3"). Branch switcher as a compact glass pill above the forked message. Each branch stored as a separate conversation in IDB.

### M8.3 — Reply-to Threading
Click any message → "Reply" option. Reply shows a quoted excerpt of the target message above the input. Thread line connects quoted block to source. Visual only — does not send separate context to AI, just prepends `> quote` to the user message.

### M8.4 — Conversation Search
`Cmd+F` / search icon opens a slide-down glass panel above chat. Real-time filtering highlights matching messages. Up/down arrows jump between matches. `Escape` closes.

### M8.5 — Auto-Title Generation
After the first AI response in a new conversation, send a background request: `"Give a 4-word title for this conversation: {first user message}"`. Replace "New Chat" silently. No loading state — title updates smoothly.

### M8.6 — Context Window Visualiser
Slim progress bar at top of chat column. Estimates token count from message lengths (1 token ≈ 4 chars). Shows percentage of model's context window used. Bitcoin-orange fill → red when > 80%. Tooltip: "~12,400 / 200,000 tokens used".

### M8.7 — Conversation Export
Three-dot menu on each conversation → Export. Options: Markdown (download .md), JSON (full data), Plain text. Uses File System Access API when available, falls back to `<a download>`. No server involved.

### M8.8 — Import Conversations
Settings → Import → drag-and-drop or file picker for AIUI JSON export or Claude.ai export JSON. Merges into existing conversations without overwriting. Shows import summary (N conversations added).

### M8.9 — Long-press / Right-click Context Menus
Messages: Copy, Edit, Delete, Reply, Branch from here. Content cards: Favourite, Share, Open detail, Copy title. Uses a reusable `ContextMenu.vue` glass-card component positioned at cursor. Closes on outside click or `Escape`.

### M8.10 — Scroll Position Memory
When switching between conversations, restore the previous scroll position. Store position per conversation ID in a `Map<string, number>` (not persisted — session only). Virtual scroller should seek to the stored offset on mount.

---

## M9: AI Experience

### M9.1 — Multi-Model Comparison Mode
Split-screen: same prompt sent to two models simultaneously. Side-by-side layout on desktop, swipeable tabs on mobile. Model selector per pane. Shows streaming output in both. Useful for comparing Claude vs OpenRouter models.

### M9.2 — System Prompt Editor
Settings → Personas. Create named personas (e.g. "Film Critic", "Bitcoin Analyst"). Each has a system prompt, model preference, and accent colour. Select persona per conversation via a pill menu above the input. Default persona applies to all new conversations.

### M9.3 — Prompt Template Library
`/` in chat input opens a command palette (glass dropdown). Templates listed with title + preview. Variables in templates use `{{variable}}` syntax — on selection, a mini form appears to fill them. Templates stored in IDB, importable/exportable as JSON.

### M9.4 — Vision Input
Drag-and-drop or paste image into chat input. Image preview appears as a thumbnail above the input. On send, image encoded as base64 and included in the message content array (Claude vision format). Only enabled when active model supports vision. Max 4 images per message.

### M9.5 — Response Feedback
Thumbs up / thumbs down on each AI message (appears on hover). Stored locally in IDB per message ID. Shown in conversation export. Future: aggregate across sessions for personal preference tracking. Never sent anywhere.

### M9.6 — Token & Cost Estimator
Settings toggle to show token counts. Each message shows estimated token count in a tiny badge (bottom-right of bubble). Running total shown in context window bar. Cost estimate based on current model's pricing (hardcoded table, updated with model releases).

### M9.7 — AI Memory Panel
Settings → Memory. A list of "always remember" facts injected into every system prompt. e.g. "I live in London", "I prefer sats over fiat". Edit/delete/add. Max 20 items. Stored encrypted in IDB. Shown as a collapsed "Memory" section in the system prompt.

### M9.8 — Model Capabilities Badge
Model selector shows capability badges: Vision 👁, Tools 🔧, Long context 📄. Tooltip explains each. Greys out vision input button when selected model doesn't support it. Updates dynamically when switching providers.

### M9.9 — Temperature & Params Slider
Advanced settings section (collapsed by default) beneath the model selector. Sliders for: Temperature (0–1), Max tokens (256–8192), Top-P. Values persisted per conversation in IDB. Reset to defaults button.

### M9.10 — Stop Sequence Configuration
Advanced settings: configurable stop sequences (comma-separated). Applied to all requests for that conversation. Useful for structured output tasks. Shown as a small tag list below the slider panel.

---

## M10: Advanced Content Renderers

### M10.1 — Full Article Renderer
When AI returns a long-form article (> 800 words with headings), render it in the panel as a paginated article view. Features: auto-generated table of contents (sticky left sidebar on desktop), estimated reading time, font-size control, print mode. Uses existing markdown-it instance.

### M10.2 — PDF Viewer
Content type `pdf` renders via `pdfjs-dist` (lazy loaded, ~400 KB). Page navigation, zoom, text selection, search within PDF. Chat preview: thumbnail of page 1. Panel play: full viewer. Files loaded from URL (no local file upload in v1).

### M10.3 — Map Renderer
Content type `place` upgrades from static card to interactive Leaflet map (lazy loaded). OpenStreetMap tiles (no API key needed). Pins for all places mentioned in conversation. Cluster pins when > 10 places. Panel play: fullscreen map with place list sidebar.

### M10.4 — Recipe Renderer
New content type `recipe`. Tag: `<recipe_ext title="..." servings="..." time="...">`. Structured display: ingredients checklist (tap to strike through), numbered steps, metadata chips (time, servings, calories). "Scale recipe" slider (0.5×–4×) recalculates quantities.

### M10.5 — Event Renderer
New content type `event`. Tag: `<event_ext title="..." date="..." location="..." url="...">`. Shows: date chip, location, countdown. Add to calendar buttons: ICS download, Google Calendar URL, Apple Calendar. Glass card in chat, full detail in panel.

### M10.6 — Math Renderer
Detect `$...$` (inline) and `$$...$$` (block) LaTeX in chat messages. Render using KaTeX (lazy loaded, ~70 KB). Fallback: display raw LaTeX in a code block. No re-renders during streaming — batch render on stream end.

### M10.7 — Mermaid Diagram Renderer
Detect ` ```mermaid ` fenced code blocks. Render using Mermaid.js (lazy loaded, ~500 KB). Support: flowchart, sequence, gantt, entity-relationship. Dark theme matching glass design. Copy SVG button. Pan/zoom on mobile.

### M10.8 — Audio Waveform Player
Upgrade PlayerBar for locally-loaded audio. Use WaveSurfer.js (lazy loaded) to show waveform visualization. Waveform rendered in Bitcoin orange on dark background. Click to seek. Existing queue/next/prev preserved.

### M10.9 — Table Renderer
Markdown tables rendered as interactive tables: column sort (click header), row filter (search input above table), CSV export button. Uses existing markdown-it but overrides the table token renderer. Max 500 rows before virtualisation kicks in.

### M10.10 — Timeline Renderer
New content type `timeline`. AI returns a series of `<event_ext>` tags. Panel renders them as a vertical timeline: date on left, event card on right, connecting line. Animate entries in as they appear during streaming.

### M10.11 — Code Runner
Fenced code blocks with a "Run" button for HTML/CSS/JS. Opens a sandboxed `<iframe srcdoc="...">` in the panel. Output console below. `sandbox="allow-scripts"` only — no network access, no storage. Python: future (Pyodide).

### M10.12 — Video Renderer
New content type `video`. Native `<video>` element with custom glass controls. HLS.js for adaptive streams (lazy loaded). YouTube URL detection → nocookie embed fallback. Panel play: fullscreen. Chat preview: thumbnail + play button.

---

## M11: Nostr Ecosystem

### M11.1 — Publish Nostr Notes
Compose panel in the Nostr tab. Write a note → sign via NIP-07 → broadcast to configured relays. Shows send status per relay. Can attach content card references (film, song, etc.) as URL mentions. Character counter (280 soft limit, no hard cap).

### M11.2 — Nostr DMs (NIP-17)
Encrypted direct messages using NIP-17 sealed gifts. DM inbox tab in Nostr section. Contact list from follows. Message threads per contact. Messages encrypted client-side, stored in IDB. No plaintext ever sent to relay.

### M11.3 — Relay Management UI
Settings → Nostr Relays. Add/remove relay URLs. Health column: latency (ms), status (connected/disconnected/error). Test connection button. Read/write toggle per relay. Import relay list from NIP-65 event.

### M11.4 — Nostr Profile Editor
Settings → Nostr Identity (extends M6.3). Edit: display name, bio, avatar URL, banner URL, website, NIP-05 address, Lightning address. Preview renders as a profile card. Publish as kind:0 event via NIP-07.

### M11.5 — Zaps (NIP-57)
On any Nostr note or profile, show a Zap ⚡ button. Opens a zap dialog: amount input (in sats), optional message. Fetches LNURL-pay from profile's Lightning address. Shows QR + deep-link. Confirms via Lightning payment. Never holds funds.

### M11.6 — NIP-05 Verification Badge
Nostr profiles with NIP-05 show a ✓ badge. Verified by fetching `/.well-known/nostr.json?name=...` from the NIP-05 domain. Cached in IDB for 24 hours. Badge tooltip shows the full NIP-05 identifier.

### M11.7 — Nostr Search (NIP-50)
Search input in Nostr tab. Sends `REQ` with `search` field to NIP-50 supporting relays (nostr.wine, relay.nostr.band). Results show as note cards with author, content, timestamp. Filter by content type.

### M11.8 — Thread View
Clicking a Nostr note opens a thread view in the panel. Fetches root event and all replies (kind:1, `#e` tag). Renders as a threaded tree (indent by depth, max 5 levels). Loads lazily from relays. Reply button opens compose with reply reference.

### M11.9 — Nostr Lists (NIP-51)
View and manage: follow list (kind:3), mute list (kind:10000), pin list (kind:10001), bookmark list (kind:10003). Each as a panel tab in the Nostr section. Add/remove items. Publish via NIP-07.

### M11.10 — Long-Form Content (NIP-23)
Nostr long-form articles (kind:30023) rendered in the article renderer (M10.1). Discovery tab in Nostr section shows recent articles from follows. Clicking opens the full article in panel play. Share as Nostr note button.

---

## M12: Bitcoin Ecosystem

### M12.1 — On-Chain Address Display
Detect Bitcoin addresses in chat (bech32 segwit, legacy). Render as a glass card: address (truncated), QR code, "View on mempool.space" link, copy button. Balance lookup via mempool.space API (lazy, opt-in). Never sends private keys.

### M12.2 — Fedimint Ecash
Detect Fedimint ecash tokens in chat (e-cash token format). Display: federation name, amount in sats, "Receive in Fedi" deep-link button. QR of the token string. Copy button. Same approach as Cashu — AIUI is never a wallet.

### M12.3 — BOLT12 Offers
Detect `lno1...` BOLT12 offer strings. Render as glass card: decoded amount (if fixed), description, "Pay with wallet" deep-link. QR of the offer. BOLT12 is static (reusable), unlike BOLT11 invoices.

### M12.4 — Nostr Wallet Connect (NWC)
Settings → Connect Wallet. Paste NWC connection string (`nostr+walletconnect://...`). AIUI can then: check balance, pay invoices (with user confirmation). Uses NIP-47. All operations require explicit user tap. Stored encrypted in IDB.

### M12.5 — LNURL-auth Login
Settings → LNURL-auth. Generates a LNURL-auth QR code. Scanning with a Lightning wallet proves ownership of the Lightning node. Sets a persistent identity (pubkey) used for local preference sync. No password needed.

### M12.6 — Live Sat/Fiat Price
Settings toggle: show amounts in sats or fiat equivalent. Price fetched from mempool.space `/api/v1/prices` every 60 seconds. Used across: Cashu cards, Lightning invoices, cost estimator, zap dialog. Stored in a `useBitcoinPrice` composable.

### M12.7 — Mempool.space Tx Viewer
Detect txid hashes (64 hex chars) and block heights in chat. Render as a glass card with: confirmations, fee rate, amount, link to mempool.space. Block height renders block summary. Updates live via mempool.space WebSocket.

### M12.8 — BOLT11 Decoder Card
Full BOLT11 invoice decode before paying: show amount, description, expiry countdown, destination node alias (if known). Expiry shown as a red countdown when < 5 minutes. "Pay" button triggers deep-link or NWC payment (M12.4).

---

## M13: Content Discovery

### M13.1 — "For You" Feed
A new "For You" tab in the content panel. Surfaces content types you've interacted with most (from favorites + conversation history). Uses a simple frequency map (no ML). Refreshes on each app open. Fully local, no server.

### M13.2 — Content Tagging
On any content card: "Add tag" (plus icon). Tags are user-defined strings stored in IDB alongside the item. Filter any content grid by tag. Tag cloud view in favorites panel. Export tags with content JSON.

### M13.3 — Smart Playlists
Music tab → Smart Playlists. Auto-generated from: recently played, most played, by genre tag, by decade. Each playlist is a computed view over the song IDB store. Play button queues the whole playlist. No manual curation needed.

### M13.4 — Similar Content
Below any open content detail: "More like this" section. Populated by sending a background AI request: `"List 3 films similar to {title} as film_ext tags"`. Results appear after 2–3 seconds. Cached in IDB per item for 7 days.

### M13.5 — Recently Viewed
A "Recent" tab in the content panel. Ordered list of the last 50 content items you opened (any type). Each entry: thumbnail, title, type, time ago. Tap to re-open. Stored in IDB, cleared on data wipe.

### M13.6 — Content Collections
User-created collections (like playlists but for any content type). Create collection → name it → add any content card to it via long-press menu. Collections shown as a grid of 4-thumbnail mosaics. Shareable as a Nostr list (NIP-51).

### M13.7 — Trending in Conversations
A "Trending" section: content items referenced most frequently across all your conversations in the last 30 days. Computed on load from IDB. Shows a small "referenced N times" badge. Pure local analytics.

### M13.8 — Content Sharing via Nostr
Any content card: Share → "Post to Nostr". Generates a note with the content title, year, a short AI-generated description, and the content tag as a URL. Signs and broadcasts via NIP-07. Opens compose preview before posting.

---

## M14: Plugin Marketplace

### M14.1 — Plugin Discovery UI
Settings → Plugins → Discover. Fetches a static community registry JSON (hosted on GitHub Pages or IPFS). Lists plugins with: name, description, type, author, version, rating. Install button triggers M14.7 (import by URL).

### M14.2 — Plugin Settings Panel
Each installed plugin has a gear icon → settings panel. Plugin declares its settings schema (JSON Schema). AIUI renders the settings form automatically using a `PluginSettingsForm.vue` component. Settings stored encrypted in IDB under plugin ID.

### M14.3 — Plugin Permissions UI
On install: permissions dialog lists requested capabilities (e.g. "Access chat messages", "Make network requests", "Read favorites"). User grants/denies each. Permissions stored per plugin. Plugin can check granted permissions at runtime via `context.hasPermission()`.

### M14.4 — Plugin Dev Mode
`VITE_PLUGIN_DEV=true` enables: hot-reload of plugins from `src/plugins/dev/`, error inspector panel (shows plugin errors without crashing app), plugin performance profiler (time per `init()` call).

### M14.5 — Built-in Plugin: Wikipedia
Plugin type `search`. `/wiki {query}` in chat input fetches Wikipedia summary via the Wikipedia REST API. Returns a `article` content card inline. No API key needed. Rendered via the article renderer (M10.1).

### M14.6 — Built-in Plugin: OpenLibrary
Plugin type `search`. Searches Open Library (openlibrary.org) for books. Returns `book_ext` tagged results. Cover images from Open Library covers API. Free, no API key.

### M14.7 — Plugin Import by URL
Settings → Plugins → Install from URL. Paste a GitHub raw URL or IPFS CID. AIUI fetches the plugin manifest (`aiui-plugin.json`), validates schema, shows permissions dialog (M14.3), then installs. Plugins are community Tier 2 (sandboxed iframe).

### M14.8 — Plugin Versioning & Auto-Update
Installed plugins store their version. On app start, check registry for newer versions (background fetch). Badge on Plugins settings icon when updates available. Update all button. Changelog shown before updating.

---

## M15: Settings & Personalisation

### M15.1 — Accent Colour Picker
Settings → Appearance. Colour wheel or preset swatches to change the accent colour (default Bitcoin orange #F7931A). Updates `--color-accent` CSS variable in real time. Persisted in IDB. Affects all gradient buttons, badges, active states.

### M15.2 — Glass Intensity Slider
Settings → Appearance. Three presets: Subtle / Default / Strong. Maps to blur(12px)/blur(18px)/blur(28px) and background opacity 0.25/0.35/0.50. Updates glass CSS variables. Live preview as you drag.

### M15.3 — Font Size Settings
Settings → Appearance. Three sizes: Compact (13px base), Default (15px), Large (17px). Sets `--font-size-base` CSS variable. Scales all rem-based text. Persisted in IDB.

### M15.4 — Content Type Visibility
Settings → Content. Toggle visibility of each of the 11 content type tabs in the panel. Hidden types still extract from AI messages but don't show in the panel. Useful for users who only care about music + films.

### M15.5 — Keyboard Shortcut Map
Settings → Shortcuts. Lists all keyboard shortcuts. Each row shows action + current binding. Click to rebind (record next key combo). Conflicts highlighted in red. Stored in IDB. Uses the existing keybindings system.

### M15.6 — Browser Push Notifications
Settings → Notifications. Opt-in for: "Generation complete" (when a long response finishes while tab is backgrounded). Uses the Web Notifications API + Service Worker `showNotification()`. Notification click focuses the tab and scrolls to the response.

### M15.7 — Auto-Archive Old Conversations
Settings → Storage. Slider: archive conversations older than N days (7/30/90/never). Archived conversations move to an "Archive" folder, not deleted. Unarchive individually. Archive stored in a separate IDB object store.

### M15.8 — Full Data Export
Settings → Data → Export All. Creates a JSON archive: all conversations, favorites, settings, tags, collections. Optionally encrypted with the current passphrase. Single file download. Compliant with GDPR right to portability.

### M15.9 — Data Wipe
Settings → Data → Wipe Everything. Two-step confirmation. Clears: all IDB stores, service worker cache, localStorage. Does not clear the API key vault unless explicitly checked. Shows what will be deleted before confirming.

### M15.10 — Default Conversation Settings
Settings → Chat. Set global defaults: default model, default persona, web search on/off, show token counts. These apply to all new conversations. Per-conversation overrides still possible.

---

## M16: Mobile UX Polish

### M16.1 — Bottom Sheet Component
Reusable `BottomSheet.vue`. Gesture-driven: drag down to dismiss, swipe up to expand. Snap points: 40% / 80% / 100% height. Backdrop tap to close. Used by: context menus, share sheets, relay management, plugin settings. Replaces modals on mobile.

### M16.2 — Swipe to Navigate Conversations
On mobile, swipe left/right on the chat area to move between conversations. Animated slide transition. Visual edge indicator (thin line at sides) to hint swipeability. Threshold: 80px swipe distance, 0.3 velocity.

### M16.3 — Pull-to-Refresh on Content Panels
Each content grid supports pull-to-refresh. Custom glass spinner animation. Triggers: re-fetch from AI context, reload Nostr feed, clear image cache for that type. Haptic feedback on release.

### M16.4 — Haptic Feedback
Use `navigator.vibrate()` for: message send (10ms), favourite toggle (15ms), error (pattern: 50ms–50ms–50ms), pull-to-refresh trigger (20ms). Wrapped in `useHaptics()` composable that checks support before calling. Settings toggle to disable.

### M16.5 — Web Share API
All content cards and conversations: Share button triggers native `navigator.share()` where available. Falls back to a glass share sheet (copy link, copy text, Nostr share). Adapts to iOS (files not supported) vs Android (files supported).

### M16.6 — Pinch-to-Zoom on Images & Maps
Images in the panel support pinch-to-zoom via touch events. Min scale 1×, max 4×. Double-tap resets to 1×. Map renderer uses Leaflet's built-in touch zoom. Implemented with a `usePinchZoom()` composable (no library needed).

### M16.7 — iOS PWA Polish
Meta tags: `apple-mobile-web-app-capable`, `apple-mobile-web-app-status-bar-style: black-translucent`. Safe area insets via `env(safe-area-inset-*)` on all fixed elements (chat input, player bar, nav). Splash screens for common iPhone sizes.

### M16.8 — Long-press Context Menus on Mobile
On mobile, long-press (500ms) on messages or content cards opens the context menu (M8.9) as a bottom sheet (M16.1). Haptic on trigger (20ms). Prevents default browser long-press menu via `@contextmenu.prevent`.

### M16.9 — Scroll Position Memory
Restore scroll position when switching tabs, conversations, or navigating back. Store position per route + conversation ID in a `Map` (session only). Content grids also remember their scroll offset.

### M16.10 — Landscape Mode Optimisation
Detect landscape on mobile. Rearrange layout: chat takes 50% width, content panel 50% (instead of overlay). Player bar becomes minimal (just controls, no waveform). Smooth transition on rotate via CSS transitions on layout classes.

---

## M17: Accessibility & Internationalisation

### M17.1 — Keyboard Navigation Audit
Full Tab order review across all pages. All interactive elements reachable. Focus trap in modals and bottom sheets. `Escape` closes any overlay. Roving tabindex in content card grids. Arrow keys navigate card grids.

### M17.2 — ARIA Audit
All icon buttons: `aria-label`. All dynamic content: `aria-live="polite"`. Dialogs: `role="dialog"`, `aria-modal`, `aria-labelledby`. Content card grids: `role="list"` + `role="listitem"`. Loading states: `aria-busy`.

### M17.3 — High Contrast Mode
`@media (prefers-contrast: more)` stylesheet. Increases border opacity from 0.18 → 0.5. Text opacity: all `/90` → `100%`. Removes backdrop blur (performance + clarity). Accent remains orange. Toggle also available in Settings.

### M17.4 — Automated Accessibility Tests
Axe-core integrated into Playwright E2E tests. Run `pnpm test:a11y` which opens each page and asserts zero critical axe violations. CI fails on new violations. Reports saved as HTML artefacts.

### M17.5 — i18n Foundation
Add `vue-i18n`. Extract all hardcoded strings into `src/i18n/en.json`. Add `es.json` (Spanish) and `fr.json` (French) with machine-translated initial values (marked as needing review). Language auto-detected from `navigator.language`, overridable in Settings.

### M17.6 — RTL Layout Support
`dir="rtl"` on `<html>` for Arabic/Hebrew locales. Use logical CSS properties (`padding-inline-start` not `padding-left`). Flex row reversal handled by `rtl:flex-row-reverse` Tailwind variant. Test with Arabic locale.

### M17.7 — Dyslexia-Friendly Font Option
Settings → Appearance → Font. Option: "OpenDyslexic". Loaded via self-hosted WOFF2 (MIT licensed). Sets `--font-sans` CSS variable. Letter spacing +0.05em, line height 1.6.

### M17.8 — Skip Navigation Link
Hidden "Skip to main content" link as the first focusable element. Visible on Tab focus. Jumps to `<main>` landmark. Standard accessibility pattern — costs nothing, helps screen reader users significantly.

---

## M18: Performance

### M18.1 — Bundle Analysis & Splitting
Run `vite-bundle-visualizer` in CI. Identify any component loaded eagerly that should be lazy. Target: core bundle stays < 150 KB gzipped. Create per-route chunk boundaries in Vue Router.

### M18.2 — Image Lazy Loading with Blur-up
All content card images: `loading="lazy"` + `decoding="async"`. Low-quality placeholder (16×16 px, base64 inline) shown until full image loads. CSS transition from blurred placeholder to sharp image. `IntersectionObserver`-based (via `useIntersectionObserver`).

### M18.3 — Request Deduplication
`useFetch()` composable wraps all API calls. Identical in-flight requests share a single Promise (keyed by URL + body hash). Cancel via `AbortController` on component unmount. Prevents duplicate AI requests on fast re-renders.

### M18.4 — Web Worker for Heavy Tasks
Move `contentExtraction` parsing and AES-256-GCM encryption/decryption into a Web Worker (`src/workers/heavy.worker.ts`). Main thread posts messages, worker responds. Use `comlink` (MIT, ~1 KB) for typed RPC. Keeps UI thread free.

### M18.5 — Prefetch on Hover
Content cards: on `mouseenter` (desktop) or 100ms touch hold (mobile), prefetch the detail data. E.g. fetch TMDB details for a film card before the user clicks. Store in a short-lived cache (5 min). Makes panel open feel instant.

### M18.6 — Memory Leak Audit
Systematically add `onUnmounted` cleanup to all composables that use: `setInterval`, `setTimeout`, `addEventListener`, WebSocket connections, `IntersectionObserver`, `ResizeObserver`. Add a dev-mode leak detector that logs active listeners on route change.

### M18.7 — Background Sync Queue
If an IDB save fails (e.g. storage quota exceeded), queue the operation in a `SyncQueue`. On next app focus (`visibilitychange`), retry the queue. Show a subtle warning badge in settings if queue is non-empty.

### M18.8 — OPFS Storage Backend (Optional)
Implement an alternative storage backend using Origin Private File System (OPFS) via SQLite WASM (`@sqlite.org/sqlite-wasm`, Apache 2.0). Feature-flagged: `VITE_STORAGE=opfs`. Faster for large datasets (1000+ conversations). Falls back to IDB if OPFS unavailable.

---

## M19: Developer Experience & Quality

### M19.1 — Storybook
Add Storybook 8 to `packages/app`. Stories for all `ui/` components. Glass morphism theme applied to Storybook canvas (`background: #0a0a0a`). Run with `pnpm storybook`. Stories used as visual regression baseline.

### M19.2 — Visual Regression Tests
Playwright screenshot tests for: ChatPage, ContentPanel, each renderer card, PassphraseDialog, BottomSheet. Compare against baseline snapshots on every PR. Fail if pixel diff > 0.5%. Update baseline with `pnpm test:update-snapshots`.

### M19.3 — Bundle Size CI Gate
Add a GitHub Actions step: build → measure gzipped bundle → fail if > 250 KB. Use `bundlesize` (MIT). Track history: post bundle size as a PR comment showing diff from base branch.

### M19.4 — Comprehensive Mock Data
Expand `src/mocks/` with realistic data for all 11 content types (20+ items each). Add a mock Nostr relay (in-process WebSocket server) for E2E tests. Add mock TMDB responses for all test films.

### M19.5 — E2E Cross-Browser Matrix
Playwright config: run tests on Chromium + Firefox + WebKit. CI matrix: macOS (WebKit) + Linux (Chromium + Firefox). Mobile viewports: iPhone 14 (390×844) + Galaxy S21 (360×800).

### M19.6 — Proxy Integration Tests
Test `claude-proxy.ts` with a mock Anthropic API (intercepted by `nock` or `msw`). Assert: SSE streaming format, tool_use round-trips, error handling (401, 429, 500), client disconnect kills child process.

### M19.7 — Performance Benchmarks (Lighthouse CI)
Run Lighthouse in CI on each PR against a built + served app. Track: LCP, FID, CLS, TTI. Fail if LCP > 3s or CLS > 0.15. Post scores as PR comment. Store history in a JSON file committed to `reports/` branch.

### M19.8 — Dependency Audit
Weekly GitHub Actions job: `pnpm audit` for vulnerabilities, `license-checker` to flag non-MIT/Apache dependencies. Auto-create an issue if violations found. Block releases on critical vulnerabilities.

---

## M20: Collaboration & Sharing

### M20.1 — Share Conversation via Nostr
Export a conversation as a Nostr long-form article (kind:30023). Title = conversation title. Content = formatted Markdown. Sign via NIP-07. Optionally encrypt for a specific npub (NIP-44). Shareable via `nostr:naddr1...` link.

### M20.2 — Read-Only Conversation Viewer
A `/view/:nostrAddr` route that renders a shared Nostr conversation (from M20.1) in read-only mode. No auth needed for public conversations. Shows content cards inline. Works as a landing page for shared links.

### M20.3 — Collaborative Playlist (Nostr NIP-51)
Create a shared content list (NIP-51 kind:30004). Invite others by npub to contribute. Each contributor signs their additions. AIUI merges all list events from the relay into a unified view. Useful for collaborative music or film curation.

### M20.4 — Conversation Templates
Pre-built conversation starters: "Bitcoin deep dive", "Film analysis", "Nostr onboarding", "Music discovery". Each is a system prompt + first user message. Shown on the new conversation screen as glass cards. Import/export as JSON. Share via Nostr.

### M20.5 — Export as Audio Podcast
Experimental (M20.5): Text-to-speech for a conversation using Web Speech API (`speechSynthesis`). Reads AI responses only. Controls: voice selector, speed, skip. Export as WAV (Web Audio API). Background music track from the player queue mixed in (opt-in). Pure client-side.

### M20.6 — Community Content Packs
Import a curated set of content (films, songs, books) from a community-maintained JSON file. Hosted on GitHub or IPFS. Registry listed in the plugin marketplace (M14.1). Examples: "2024 Best Films", "Bitcoin Music Playlist", "Essential Nostr Reads".

---

## Automated Session Execution Order

Each session should:
1. Read `PROGRESS.md` — find the next `[ ]` item
2. Read the task spec above
3. Implement the task
4. Run `pnpm typecheck && pnpm lint && pnpm test`
5. Commit: `type(scope): description`
6. Update `PROGRESS.md`

### Priority Queue

**M8 (Chat Polish):** M8.1 → M8.4 → M8.5 → M8.6 → M8.2 → M8.3 → M8.7 → M8.8 → M8.9 → M8.10
**M9 (AI Experience):** M9.1 → M9.4 → M9.2 → M9.3 → M9.7 → M9.5 → M9.6 → M9.8 → M9.9 → M9.10
**M10 (Renderers):** M10.6 → M10.7 → M10.3 → M10.1 → M10.9 → M10.4 → M10.5 → M10.11 → M10.2 → M10.8 → M10.10 → M10.12
**M11 (Nostr):** M11.3 → M11.1 → M11.5 → M11.6 → M11.7 → M11.8 → M11.4 → M11.2 → M11.9 → M11.10
**M12 (Bitcoin):** M12.1 → M12.6 → M12.8 → M12.7 → M12.3 → M12.2 → M12.5 → M12.4
**M13 (Discovery):** M13.5 → M13.1 → M13.2 → M13.3 → M13.4 → M13.6 → M13.7 → M13.8
**M14 (Plugins):** M14.5 → M14.6 → M14.7 → M14.1 → M14.2 → M14.3 → M14.4 → M14.8
**M15 (Settings):** M15.1 → M15.2 → M15.3 → M15.4 → M15.5 → M15.6 → M15.7 → M15.8 → M15.9 → M15.10
**M16 (Mobile):** M16.1 → M16.7 → M16.4 → M16.5 → M16.2 → M16.8 → M16.3 → M16.6 → M16.9 → M16.10
**M17 (a11y/i18n):** M17.8 → M17.1 → M17.2 → M17.3 → M17.4 → M17.5 → M17.6 → M17.7
**M18 (Perf):** M18.2 → M18.6 → M18.3 → M18.5 → M18.1 → M18.4 → M18.7 → M18.8
**M19 (DX):** M19.4 → M19.6 → M19.5 → M19.3 → M19.1 → M19.2 → M19.7 → M19.8
**M20 (Collab):** M20.4 → M20.1 → M20.2 → M20.6 → M20.3 → M20.5

**Total: 116 tasks across 13 milestones**
