# AIUI Debug, Fix & Hardening Plan

## Execution Rules

- Run on `development` branch
- Each `- [ ]` task is one agent iteration — complete fully before moving on
- After each phase: `pnpm typecheck && pnpm lint && pnpm test -- --run`
- If checks fail, fix before proceeding
- Commit at end of each phase with `type(scope): description` format
- Do NOT push — human will review and push

---

## PHASE 0: Baseline Verification

- [ ] **P0.1 — Baseline check**
  - Run: `pnpm install && pnpm typecheck && pnpm lint && pnpm test -- --run`
  - Record output. Note existing failures. Fix any blockers before proceeding.
  - Commit: `chore(app): verify baseline before hardening`

---

## PHASE 1: Critical Security Fixes

- [ ] **P1.1 — Fix CORS wildcard in claude-proxy.ts**
  - File: `packages/app/server/claude-proxy.ts`
  - Replace all `'Access-Control-Allow-Origin': '*'` (lines ~415, 473, 531) with the correct localhost origin or import `ALLOWED_ORIGIN` from `dev-auth.ts`. Match the pattern already used on lines 294/303.
  - Verify: `grep -n "Allow-Origin.*\*" packages/app/server/claude-proxy.ts` returns zero matches.

- [ ] **P1.2 — Fix dev auth token bypass**
  - File: `packages/app/server/dev-auth.ts`
  - Line 11: `if (!token) return true` skips auth entirely when token empty.
  - Fix: Only skip in non-production. `if (!token) { if (process.env.NODE_ENV === 'production') { res.writeHead(401); res.end('Unauthorized'); return false; } return true; }`
  - Add console.warn when auth disabled.

- [ ] **P1.3 — Symlink traversal protection in vite-fs.ts**
  - File: `packages/app/vite-fs.ts`
  - In `walk` function: after constructing `fullPath`, add `if (lstatSync(fullPath).isSymbolicLink()) continue;`
  - In `handleRead`: before `statSync`, check `lstatSync(filePath).isSymbolicLink()` → return 403.
  - Import `lstatSync` from `fs`.

- [ ] **P1.4 — Body size limit on vite-fs.ts mkdir**
  - File: `packages/app/vite-fs.ts`
  - `handleMkdir` reads body with no size cap (lines 185-209).
  - Add `const MAX_BODY_SIZE = 1024`. Track `let size = 0` on `data` events. Return 413 if exceeded.

- [ ] **P1.5 — JSON schema validation in vite-dev-chats.ts**
  - File: `packages/app/vite-dev-chats.ts`
  - After `JSON.parse(body)` on line 66, validate shape: must be object with optional `conversations` (object) and `activeConversationId` (string|null). Reject with 400 if invalid.

- [ ] **P1.6 — CSP headers in nginx-archy.conf**
  - File: `packages/app/server/nginx-archy.conf`
  - Add inside `/aiui/` location block:
    ```
    add_header Content-Security-Policy "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' https: data:; connect-src 'self' https://api.anthropic.com https://openrouter.ai https://wavlake.com https://itunes.apple.com https://openlibrary.org https://covers.openlibrary.org https://en.wikipedia.org https://www.googleapis.com https://image.tmdb.org; frame-ancestors 'self'; base-uri 'self'; form-action 'self';" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header Referrer-Policy "strict-origin-when-cross-origin" always;
    ```

- [ ] **P1.7 — OpenRouter validation + streaming timeout**
  - File: `packages/app/server/claude-proxy.ts`
  - OpenRouter handler: parse `reqBody`, validate has `model` (string), `messages` (array), `stream` (boolean). Return 400 if invalid.
  - Both streaming reader loops: add idle timeout (120s). `let idleTimer = setTimeout(() => reader.cancel(), 120000)`. Reset on each chunk. Clear on completion.
  - Verify: `pnpm typecheck`
  - **Commit**: `fix(app): critical security — CORS, auth, symlink, CSP, body validation`

---

## PHASE 2: Proxy & Server Hardening

- [ ] **P2.1 — Fix tool use loop in claude-proxy.ts**
  - File: `packages/app/server/claude-proxy.ts` (lines 164-222)
  - Unknown tool names: push error tool_result `{ type: 'tool_result', tool_use_id: tu.id, content: 'Error: unknown tool', is_error: true }`.
  - Add turnMessages size guard: `if (JSON.stringify(turnMessages).length > 500_000)` break loop.
  - Add `AbortSignal.timeout(30000)` on each API call within loop.

- [ ] **P2.2 — SSRF mitigation in vite-rss.ts**
  - File: `packages/app/vite-rss.ts`
  - After `tryParseFeed` returns, validate returned article URLs with `isPrivateUrl`. Filter out any private URLs.
  - Add comment documenting residual TOCTOU risk (dev-only middleware).

- [ ] **P2.3 — Sanitize web search context in useAI.ts**
  - File: `packages/app/src/composables/useAI.ts` (lines 399-408)
  - Create helper: `function sanitizeSearchText(s: string): string { return s.replace(/[[\]()#*_~` >]/g, '\\$&').replace(/\n/g, ' ').slice(0, 500) }`
  - Apply to `r.title` and `r.content` in `formatWebSearchContext`.
  - **Commit**: `fix(app): proxy hardening — tool loop, SSRF, prompt injection`

---

## PHASE 3: State & Logic Bugs

- [ ] **P3.1 — Fix contentType assignment in useContentPanel.ts**
  - File: `packages/app/src/composables/useContentPanel.ts` (lines 219-225)
  - The `contentType` ref type is `'film' | 'song' | 'podcast'`. Expand to include all content types or use `ContentTab`. Fix assignments: books→'film' is acceptable if type can't expand, but news/TV should map correctly. If the type is only used for player logic, keep narrow type but fix assignments to be semantically correct.

- [ ] **P3.2 — Fix apiFetching race in useBannerFallback.ts**
  - File: `packages/app/src/composables/useBannerFallback.ts`
  - Change `let apiFetching = false` to `const apiFetching = ref(false)`. Update all references to `.value`.

- [ ] **P3.3 — Fix event listener cleanup in chat.ts**
  - File: `packages/app/src/stores/chat.ts` (lines 111-118)
  - Extract handlers to named functions. Add `import.meta.hot?.dispose()` cleanup for HMR.

- [ ] **P3.4 — Fix news RSS race condition in useContentPanel.ts**
  - File: `packages/app/src/composables/useContentPanel.ts` (lines 134-154)
  - Capture `const tabAtStart = activeTab.value` before RSS fetch. In `.then()`, only set `activeTab.value = 'news'` if `activeTab.value === tabAtStart` (user hasn't manually switched).

- [ ] **P3.5 — Add error logging to silent catch blocks**
  - Files: `useImageFallback.ts`, `useAI.ts`, `stores/chat.ts`
  - Replace all `catch { }` and `catch { /* ignore */ }` with `catch(e) { console.debug('[module] op failed:', e) }`.
  - Keep `console.debug` (not `error`) for expected failures.
  - **Commit**: `fix(app): state bugs — contentType, race conditions, error logging`

---

## PHASE 4: Content Extraction & Filtering

- [ ] **P4.1 — Add recipe instructions to system prompt**
  - File: `packages/app/src/composables/useAI.ts` (SYSTEM_PROMPT)
  - Add after Apps section:
    ```
    **Recipes:** When sharing recipes, use the <recipe_ext> XML tag:
    <recipe_ext title="Name" servings="4" time="30 min" calories="450">
    - ingredient 1
    1. Step one
    </recipe_ext>
    ```

- [ ] **P4.2 — Fix app category validation**
  - File: `packages/app/src/composables/contentExtraction.ts` (line ~1330)
  - Add `const VALID_CATEGORIES = new Set(['nostr-client','lightning-wallet','bitcoin-wallet','privacy','node','dev-tool','relay'])`. Validate before cast, fallback to `'dev-tool'`.

- [ ] **P4.3 — Fix song deduplication**
  - File: `packages/app/src/composables/contentExtraction.ts` (lines 606-638)
  - After combining library + external songs, deduplicate by normalized `title|artist` key. Library songs (with real IDs) take priority.

- [ ] **P4.4 — Fix film/TV tag ambiguity**
  - File: `packages/app/src/composables/contentExtraction.ts` (lines 914-916)
  - Only convert `film_ext` to TV when: query is TV-like AND no explicit `tv_ext` tags present AND response reads TV-like. If AI used both `film_ext` and `tv_ext`, keep both as-is.

- [ ] **P4.5 — Fix isBookLikeResponse false positives**
  - File: `packages/app/src/composables/contentFiltering.ts` (lines 60-62)
  - Increase `by [Author]` threshold from `>= 1` to `>= 2`.

- [ ] **P4.6 — Fix isRecipeLikeResponse for prose recipes**
  - File: `packages/app/src/composables/contentFiltering.ts` (lines 92-94)
  - Add prose detection: recipe keywords + list structure pattern.

- [ ] **P4.7 — Fix looksLikeSong false positives**
  - File: `packages/app/src/composables/contentExtraction.ts` (lines 488-506)
  - Add financial terms to blocklist: `'market cap','etf','price','trading','volume','earnings','valuation','stock','portfolio','investment','yield','inflation','interest rate'`.
  - **Commit**: `fix(app): extraction — dedup, tag ambiguity, false positives, recipes`

---

## PHASE 5: Memory & Cache Hardening

- [ ] **P5.1 — Bound caches in useImageFallback.ts**
  - File: `packages/app/src/composables/useImageFallback.ts`
  - Create helper: `function boundedSet<K,V>(map: Map<K,V>, key: K, val: V, max=500) { if (map.size >= max) { const first = map.keys().next().value; if (first !== undefined) map.delete(first); } map.set(key, val); }`
  - Replace all `.set()` calls on memory caches with `boundedSet()`.
  - Also bound `failedUrls` Set to 1000 entries.

- [ ] **P5.2 — Bound caches in usePlayer.ts + cleanup**
  - File: `packages/app/src/composables/usePlayer.ts`
  - Bound `resultCache` and `nullCacheTimestamps` to 200 entries.
  - Add `destroy()` method that calls `destroyPlayer()`, resets DOM refs, cancels active search controller.
  - **Commit**: `fix(app): bound caches, player cleanup`

---

## PHASE 6: Accessibility

- [ ] **P6.1 — Fix touch targets and aria attributes**
  - Audit all `*Grid.vue` and `*Detail.vue` for: interactive elements < 44px, images without alt, SVG fallbacks without aria-label.
  - ImageGrid.vue: add `role="img"` `aria-label="Image unavailable"` to fallback SVG wrapper.
  - Ensure all interactive genre/tag pills in grids have adequate touch targets (min 44x44 including padding).
  - Non-interactive info pills are exempt.
  - **Commit**: `fix(app): accessibility — touch targets, alt text, aria`

---

## PHASE 7: Security Tests

- [ ] **P7.1 — Write dev-auth tests**
  - New file: `packages/app/src/__tests__/dev-auth.test.ts`
  - Tests: auth bypass when no token (dev), auth required when token set, rate limiting 429, CORS headers correct.

- [ ] **P7.2 — Extend proxy tests**
  - File: `packages/app/src/__tests__/proxy.test.ts`
  - Tests: unknown tool error result, max rounds termination, turnMessages size guard, streaming timeout.

- [ ] **P7.3 — Write vite-fs security tests**
  - New file: `packages/app/src/__tests__/vite-fs.test.ts`
  - Tests: path traversal rejected, symlink blocked, sensitive files blocked, body size limit enforced.

- [ ] **P7.4 — Write SSRF tests for vite-rss**
  - New file: `packages/app/src/__tests__/vite-rss.test.ts`
  - Tests: private IP detection, localhost blocked, non-http blocked, private URLs filtered from results.
  - **Commit**: `test(app): security tests — auth, proxy, fs, SSRF`

---

## PHASE 8: Content Extraction Tests

- [ ] **P8.1 — Write content filtering tests**
  - New file: `packages/app/src/composables/__tests__/contentFiltering.test.ts`
  - Tests: isBookLikeResponse false positive fix, isRecipeLikeResponse prose detection, isMusicQuery rejects financial terms, filterTabsByContext ordering.

- [ ] **P8.2 — Extend content extraction tests**
  - File: `packages/app/src/__tests__/contentExtraction.test.ts`
  - Tests: song dedup, looksLikeSong rejects financial terms, app category fallback, recipe parsing.

- [ ] **P8.3 — Write TV/film resolution tests**
  - Same file as P8.2.
  - Tests: film_ext→TV conversion rules, book_ext in news context, explicit tags respected.
  - **Commit**: `test(app): content extraction — filtering, dedup, tag resolution`

---

## PHASE 9: State & Composable Tests

- [ ] **P9.1 — Write useContentPanel tests**
  - File: `packages/app/src/composables/__tests__/useContentPanel.test.ts`
  - Tests: contentType assignment per content type, RSS tab override prevention, closePanel resets, empty text → no tabs.

- [ ] **P9.2 — Write useBannerFallback tests**
  - New file: `packages/app/src/composables/__tests__/useBannerFallback.test.ts`
  - Tests: primary URL fallthrough, API fetch on exhaustion, apiFetching guard, gradient fallback.

- [ ] **P9.3 — Write chat store tests**
  - New file: `packages/app/src/__tests__/chat-store.test.ts`
  - Tests: createConversation, addMessage, deleteConversation, branchFromMessage, debouncedIDBSave.
  - **Commit**: `test(app): state tests — contentPanel, bannerFallback, chat store`

---

## PHASE 10: AI Integration Tests

- [ ] **P10.1 — Extend useAI tests**
  - File: `packages/app/src/__tests__/useAI.test.ts`
  - Tests: system prompt includes recipe instructions, web search results sanitized, editAndResend truncates, sanitizeHistory merges consecutive roles.

- [ ] **P10.2 — Write useImageFallback cache tests**
  - New file: `packages/app/src/composables/__tests__/useImageFallback.test.ts`
  - Tests: boundedSet evicts at max, generatePosterFallback valid SVG, escapeXml handles specials.
  - **Commit**: `test(app): AI integration, image fallback cache tests`

---

## PHASE 11: Final Verification

- [ ] **P11.1 — Full integration check**
  - Run: `pnpm typecheck && pnpm lint && pnpm test -- --run`
  - Run: `pnpm build` — verify production build succeeds
  - Fix any regressions.

- [ ] **P11.2 — Final commit**
  - Run coverage report if configured: `pnpm test -- --run --coverage`
  - Ensure all changes committed on `development`.
  - **Commit**: `chore(app): hardening complete — all checks pass`

---

## Phase Dependencies

```
P0 → P1 → P2 → P3 → P4 → P5 ─┐
                                ├→ P7 (tests P1,P2)
                          P6 ──┤
                                ├→ P8 (tests P4)
                                ├→ P9 (tests P3,P5)
                                ├→ P10 (tests P2.3,P4.1)
                                └→ P11 (final)
```

P5 and P6 can run in parallel. P7-P10 can run in any order after their fix phases.

## Summary

- **12 phases**, **42 tasks**
- Phases 0-6: fixes (security → proxy → state → extraction → caches → a11y)
- Phases 7-10: tests (security → extraction → state → AI)
- Phase 11: final verification
- Target: all checks pass, 40%+ test coverage on critical paths, secure for Archy deployment
