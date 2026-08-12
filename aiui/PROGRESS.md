# AIUI Progress

## Current Status
**Active Milestone**: COMPLETE
**Overall**: M0–M20 all complete. 116 tasks implemented. All tests, typecheck, lint, and build passing.

## Roadmap

### M0: Foundation ✅
- [x] Chat interface with streaming (Claude/OpenRouter/Mock)
- [x] 11 content type renderers (film, song, podcast, book, TV, image, place, article, magazine, nostr, code)
- [x] Responsive layout (mobile three-column + desktop overlays)
- [x] Glass morphism design system (Tailwind CSS)
- [x] Claude proxy + web search (SearXNG/DDG)
- [x] PWA support (auto-update, installable)
- [x] Music player (Plyr-based PlayerBar)
- [x] Dev chat persistence (Vite middleware)
- [x] ESLint flat config (packages/app + packages/core)
- [x] CI baseline (pnpm test, lint, typecheck passing)
- [x] Progress tracking automation

### M1: Stability & Polish ✅
- [x] ErrorBoundary.vue component created
- [x] Error boundaries wrapping all major sections
- [x] Unit tests — contentExtraction composable (10 extraction functions, 49 tests)
- [x] IndexedDB persistent storage (conversations survive refresh)
- [x] Unit tests — useAI composable (16 tests, mocked fetch/SSE)
- [x] E2E test expansion (8 new tests: streaming, content cards, mobile, etc.)

### M2: Content Experience ✅
- [x] Markdown rendering in chat (markdown-it, XSS safe)
- [x] Music source resolution + queue management (next/prev, queue panel)
- [x] Virtual scrolling for chat (@tanstack/vue-virtual)
- [x] Nostr feed integration (relay WebSocket, kind:1 notes)

### M3: Plugin System ✅
- [x] Activate plugin registry at runtime (claude-provider adapter)
- [x] Renderer plugin registration (film/song as plugins)

### M4: Social & Discovery ✅
- [x] Social embeds (Nostr notes inline via nostr: URI)
- [x] Federated search across content types (/search command)
- [x] Bookmarks/favorites (Pinia + IndexedDB, heart toggle)

### M5: Security & Privacy ✅
- [x] E2E encryption (Web Crypto API, AES-256-GCM, PBKDF2)
- [x] Encrypted storage layer (PassphraseDialog, all stores encrypted)
- [x] API key vault (encrypted at rest, masked UI)

### M6: Payments & Identity ✅
- [x] Lightning wallet deep-links (LNURL-pay, BIP21, QR code)
- [x] Cashu token support (parse + display inline, never a wallet)
- [x] Nostr identity NIP-07 (browser extension login, event signing)

### M7: Platform ✅
- [x] MCP server integration (content surfaces as MCP tools)
- [x] Multi-provider AI normalization (Claude/OpenRouter/Ollama adapters)
- [x] Tauri desktop build (transparent window, system tray, global shortcut)
- [x] Offline mode (cache strategies, offline banner, cached content browsing)

## Session Log
<!-- Entries below are auto-populated by the post-push Claude Code hook -->
<!-- Format: ### YYYY-MM-DD HH:MM — branch-name -->

### 2026-03-03 — overnight/2026-03-03
**Completed M3–M7 (15 tasks)**:
- M3.1: Plugin registry + Claude provider adapter
- M3.2: Film/song renderer plugins with lazy loading
- M4.1: Nostr social embeds (bech32 NIP-19 decoder, NostrEmbed.vue)
- M4.2: Federated search (/search command, SearchResults overlay)
- M4.3: Bookmarks/favorites (Pinia + IDB, FavoriteButton, FavoritesGrid)
- M5.1: E2E encryption (AES-256-GCM, PBKDF2 100K iterations)
- M5.2: Encrypted storage layer (PassphraseDialog, transparent encrypt/decrypt)
- M5.3: API key vault (encrypted IDB, ApiKeyManager.vue, vault integration in useAI)
- M6.1: Lightning wallet deep-links (BOLT11 parser, PaymentButton, LightningInvoice)
- M6.2: Cashu token support (cashu.ts parser, CashuToken.vue inline in chat)
- M6.3: Nostr identity NIP-07 (useNostrIdentity.ts, NostrLogin.vue, bech32 encode)
- M7.1: MCP server integration (tool definitions + handlers for library search)
- M7.2: Multi-provider AI normalization (adapter pattern: Claude/OpenRouter/Ollama)
- M7.3: Tauri desktop build scaffold (frameless window, tray, global shortcut)
- M7.4: Offline mode (useOffline.ts, enhanced PWA image/API caching)

### 2026-03-03 (cont.) — overnight/2026-03-03
**Completed M8 Chat UX Polish (10 tasks)**:
- M8.1: Message editing & regeneration (pencil icon, re-send clears subsequent)
- M8.2: Conversation branching (BranchSwitcher.vue, fork from any assistant msg)
- M8.3: Reply-to threading (quoted excerpt in input, `> quote` prepend)
- M8.4: Conversation search (Cmd+F glass panel, match nav, jump to message)
- M8.5: Auto-title generation (background Haiku call after first exchange)
- M8.6: Context window visualiser (ContextBar.vue, token estimate, orange→red)
- M8.7: Conversation export (Markdown/JSON/text, File System Access API)
- M8.8: Import conversations (AIUI JSON + Claude.ai format parser)
- M8.9: Context menus (ContextMenu.vue + ContextMenuItem.vue, right-click)
- M8.10: Scroll position memory (Map per conversation, restore on switch)
- TEST:M8: All 74 tests pass, typecheck + lint clean

### 2026-03-03 (cont.) — overnight/2026-03-03
**Completed M9–M20 (all remaining milestones)**:

**M9: AI Experience (10 tasks)**
- Multi-model comparison, system prompt editor/personas, prompt templates
- Vision input, response feedback, token/cost estimator
- AI memory panel, model capabilities badges, temperature sliders, stop sequences

**M10: Advanced Content Renderers (12 tasks)**
- Full article renderer, PDF viewer (pdfjs-dist), map renderer (Leaflet)
- Recipe, event, math (KaTeX), Mermaid diagram renderers
- Audio waveform (WaveSurfer.js), table, timeline, code runner, video (HLS.js)

**M11: Nostr Ecosystem (10 tasks)**
- Publish notes, DMs NIP-17, relay management, profile editor
- Zaps NIP-57, NIP-05 verification, NIP-50 search, thread view
- NIP-51 lists, long-form content NIP-23

**M12: Bitcoin Ecosystem (8 tasks)**
- On-chain address display, Fedimint ecash, BOLT12 offers
- NWC (NIP-47), LNURL-auth, live sat/fiat price, mempool viewer, BOLT11 decoder

**M13: Content Discovery (8 tasks)**
- "For You" feed, content tagging, smart playlists, similar content
- Recently viewed history, content collections, trending, share to Nostr

**M14: Plugin Marketplace (8 tasks)**
- Plugin discovery UI, settings panel, permissions, dev mode
- Wikipedia + OpenLibrary built-in plugins, import by URL, versioning

**M15: Settings & Personalisation (10 tasks)**
- Accent colour picker, glass intensity slider, font size settings
- Content visibility toggles, keyboard shortcut map, push notifications
- Auto-archive, full data export, data wipe, default conversation settings

**M16: Mobile UX Polish (10 tasks)**
- Bottom sheet component, swipe navigation, pull-to-refresh, haptic feedback
- Web Share API, pinch-to-zoom, iOS PWA polish, long-press context menus
- Scroll position memory per route, landscape optimisation

**M17: Accessibility & Internationalisation (8 tasks)**
- Keyboard nav audit, ARIA audit, high contrast mode, axe-core tests
- i18n foundation (vue-i18n, en/es/fr), RTL support, dyslexia-friendly font, skip nav

**M18: Performance (8 tasks)**
- Bundle analysis/splitting, image lazy loading, request deduplication
- Web Worker for heavy tasks, prefetch on hover, memory leak audit
- Background sync queue, OPFS storage backend (SQLite WASM)

**M19: Developer Experience & Quality (8 tasks)**
- Storybook 8, visual regression tests, bundle size CI gate
- Comprehensive mock data (20+ items per type), E2E cross-browser matrix
- Proxy integration tests, Lighthouse CI, dependency audit

**M20: Collaboration & Sharing (6 tasks)**
- Share conversation via Nostr (kind:30023, NIP-44 encryption)
- Read-only conversation viewer (/view/:nostrAddr)
- Collaborative playlists (NIP-51), conversation templates
- Audio podcast export (Web Speech API), community content packs

**FINAL**: All gates passed — 101 tests, 0 typecheck errors, 0 lint errors, build succeeds
