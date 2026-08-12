# Overnight Plan — 2026-03-04

## Phase 1: Critical Fixes

- [x] P1-1: Brighten SVG fallbacks — increase background lightness from 18% to 28% across all 8 generators in `useImageFallback.ts` (generateSongCoverFallback, generatePodcastCoverFallback, generateNewsFallback, generateImageFallback, generatePosterFallback, generateTVSeriesFallback, generateBookCoverFallback, generatePlaceFallback). Proportionally increase all inner element lightness by +10%. TEST: run `pnpm typecheck` and visually confirm SVGs generate valid data URIs.
- [x] P1-2: Add `.catch(() => {})` to all cover fetch promise chains in grid components — SongGrid.vue (line 157), FilmGrid.vue (line 143), TVSeriesGrid.vue (line 160), BookGrid.vue (line 141), PodcastGrid.vue (line 130). Prevents unhandled rejection if fetch throws unexpectedly. TEST: `pnpm typecheck && pnpm lint`.
- [x] P1-3: Refine mobile keyboard handling — In `useVisualViewport.ts`, add debounce to viewport change handler (50ms) to prevent jittery resizing. In `App.vue`, ensure the `rootStyle` computed applies `overflow: hidden` when keyboard is open. TEST: `pnpm typecheck`.
- [x] P1-4: Verify service worker cleanup — Confirm `dev-dist/sw.js` contains the self-destructing SW and `vite.config.ts` has `devOptions.enabled: false`. If not, fix. TEST: read both files and verify.

## Phase 2: Error Handling Hardening

- [x] P2-1: Wrap JSON.parse calls in try/catch — `useContentDiscovery.ts` sessionStorage parse, all sessionStorage/localStorage reads in composables. Search for `JSON.parse` across all `.ts` and `.vue` files, wrap any unprotected calls. TEST: `pnpm typecheck && pnpm lint`.
- [x] P2-2: Add `.ok` checks before `.json()` on fetch calls — `useBitcoinPrice.ts` (Mempool API), `MempoolTxCard.vue` (tip height), `useNip05Verification.ts` (NIP-05 lookup), `ZapDialog.vue` (Lightning address). Search for `fetch(` → `.json()` patterns without `.ok` check. TEST: `pnpm typecheck && pnpm lint`.
- [x] P2-3: Harden SSE streaming — In `useAI.ts` `readSSE()`, wrap `reader.read()` in try/catch, close reader on error. In `openrouter-adapter.ts`, add same pattern. TEST: `pnpm typecheck`.
- [x] P2-4: Add error handling to async watchers — `PdfViewer.vue` watch calling `renderPage()`, `VideoPlayer.vue` `initHls()` in onMounted. Wrap in try/catch with user-friendly error state. TEST: `pnpm typecheck`.

## Phase 3: Security Hardening

- [x] P3-1: postMessage origin validation — In `archyBridge.ts`, replace `'*'` targetOrigin with configurable origin. Add origin check on incoming message handler. TEST: `pnpm typecheck`.
- [x] P3-2: URL validation — In `contentExtraction.ts`, add URL length limit (2048 chars) to `extractUrlFromText()`. Validate URLs before fetch. TEST: `pnpm typecheck && pnpm lint`.
- [x] P3-3: Content sanitization — Review `html.ts` for innerHTML usage, ensure SVG injection is covered. Replace `innerHTML = ''` with `textContent = ''` in `usePlayer.ts`. TEST: `pnpm typecheck`.
- [x] P3-4: Add CSP meta tag — Add `<meta http-equiv="Content-Security-Policy" ...>` to `index.html` with appropriate directives for the app (allow self, API hosts, image CDNs). TEST: `pnpm typecheck`.

## Phase 4: Test Suite

- [x] P4-1: Unit tests for usePlayer — Create `packages/app/src/composables/__tests__/usePlayer.test.ts`. Test playback state, queue management, play/pause/next/prev. Minimum 8 test cases. TEST: `pnpm test`.
- [x] P4-2: Unit tests for useContentPanel — Create `packages/app/src/composables/__tests__/useContentPanel.test.ts`. Test tab switching, detail opening, panel state management. Minimum 6 test cases. TEST: `pnpm test`.
- [x] P4-3: Unit tests for useVisualViewport — Create `packages/app/src/composables/__tests__/useVisualViewport.test.ts`. Mock visualViewport API, test keyboard detection, viewport height calculation. Minimum 5 test cases. TEST: `pnpm test`.
- [x] P4-4: Content extraction edge case tests — Create `packages/app/src/composables/__tests__/contentExtraction.test.ts`. Test interleaved tags, malformed tags, unicode content, missing fields. Minimum 10 test cases. TEST: `pnpm test`.
- [x] P4-5: Seeded prompt regression tests — Create `packages/app/src/__tests__/seed-conversations.test.ts`. Import all seed conversations from mocks, run content extraction on each, verify expected content types are produced. Minimum 1 test per seed. TEST: `pnpm test`.

## Phase 5: Feature Work

- [x] P5-1: File browser page — Create `packages/app/src/pages/BrowsePage.vue` with file tree navigation. Add route `/browse` to router. Use the existing `vite-fs.ts` plugin for file reading. Show files/folders with icons, breadcrumb nav. TEST: `pnpm typecheck && pnpm lint`.
- [x] P5-2: File tree component — Create `packages/app/src/components/browse/FileTree.vue`. Recursive tree with expand/collapse, file type icons (folder, code, image, document). Use glass morphism styling. TEST: `pnpm typecheck`.
- [x] P5-3: File preview component — Create `packages/app/src/components/browse/FilePreview.vue`. Preview text files with syntax highlighting (reuse code viewer), images inline, show file metadata. TEST: `pnpm typecheck`.
- [x] P5-4: Allow .claude folder in code viewer — Update `vite-fs.ts` to allow `.claude/` path. Update any path validation that blocks dotfiles. Show CLAUDE.md, settings, hooks, memory files. TEST: `pnpm typecheck`.
- [x] P5-5: Archy local search guide — Create `packages/app/src/docs/archy-local-search.md` documenting how file types map to content surfaces (images→ImageGrid, music→SongGrid, etc.), how ContextBroker filtering works. Also add a help section component that can display this in-app. TEST: file exists and is valid markdown.

## Phase 6: Accessibility

- [x] P6-1: Add aria-labels to icon buttons — Audit all icon-only buttons across chat components (ChatHeader.vue, ChatMessage.vue, ChatInput.vue, ChatSearch.vue). Add descriptive `aria-label` to each. TEST: `pnpm lint`.
- [x] P6-2: Add aria-labels to content grids — All Grid components (SongGrid, FilmGrid, TVSeriesGrid, PlaceGrid, BookGrid, PodcastGrid, NewsGrid, ImageGrid). Each card button needs `aria-label` with content title. TEST: `pnpm lint`.
- [x] P6-3: Focus management for dialogs — `ZapDialog.vue`: add focus trap, auto-focus close button, `aria-modal="true"`, `role="dialog"`. Same for `SettingsModal.vue`. Ensure Escape key closes. TEST: `pnpm typecheck`.
- [x] P6-4: Color contrast audit — Check `text-white/40` against dark backgrounds for WCAG AA (4.5:1). Verify `#F7931A` accent contrast. Fix any failing ratios by increasing opacity. Document findings in comments. TEST: `pnpm lint`.
- [x] P6-5: Alt text improvements — `ImageGrid.vue`: use `img.title || img.alt` instead of generic. All content grids: ensure img alt includes meaningful content (title + artist/director/author). TEST: `pnpm lint`.

## Phase 7: Performance & Compatibility

- [x] P7-1: Lazy load heavy renderers — Use `defineAsyncComponent` for PdfViewer, VideoPlayer, MapView. Add loading skeleton components for each. TEST: `pnpm typecheck`.
- [x] P7-2: Add in-memory caching — `useNip05Verification.ts`: cache results with 5-min TTL. `useBitcoinPrice.ts`: cache price with 30s TTL. TEST: `pnpm typecheck`.
- [x] P7-3: Error boundaries for grid items — Create `packages/app/src/components/ui/ErrorBoundary.vue` using `onErrorCaptured`. Wrap each grid item renderer to prevent cascade failures. Show fallback UI on component crash. TEST: `pnpm typecheck`.
- [x] P7-4: Code file size limits — In `useCodeContext.ts` `openFile()`, add file size check before reading (reject > 1MB). Add loading indicator for large files. TEST: `pnpm typecheck`.

## Phase 8: Research & Documentation

- [x] P8-1: iOS app research — Research Capacitor vs WKWebView wrapper vs React Native WebView for shipping AIUI as iOS app. Document in `docs/research/ios-app.md`: pros/cons, App Store requirements, push notification integration, offline capability. Include concrete next steps.
- [x] P8-2: Mac desktop app research — Research Tauri v2 vs Electron for Mac desktop app. Document in `docs/research/mac-desktop.md`: binary size, memory usage, menu bar app pattern (like Raycast), global hotkey/command invocation, tray API. Include concrete next steps.
- [x] P8-3: Plugin system hardening research — Document in `docs/research/plugin-security.md`: signature validation for community plugins, sandboxed iframe execution, permission system per plugin. Reference existing plugin interfaces in `packages/core/src/plugins/`.
