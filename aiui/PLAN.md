# AIUI Project Plan: Roadmap, Progress Tracking & Automated Coding Tasks

## Context

AIUI is at ~65% architectural completeness. The chat + content surface experience is mature (11 content types, streaming, responsive layout, glass morphism design system). However, critical infrastructure is missing: no persistent storage beyond dev mode, zero error boundaries, no unit tests, plugin system spec'd but unused, no encryption.

The user has Claude Code automation set up for late-night unattended runs. This plan provides:
1. **Progress tracking automation** — PROGRESS.md updated on every commit/push
2. **Detailed task specs** — granular enough for automated Claude sessions to execute autonomously

There are **6 active worktrees** — all automation must work across branches.

---

## Part 1: Progress Tracking Automation

### Task 1.1: Create PROGRESS.md

**File**: `PROGRESS.md` (repo root)

Create a structured progress document with:
- Project status summary (current milestone, % complete)
- Roadmap checklist (M0–M7, matching Part 2 below)
- Session log section (auto-populated by hook)

Format:
```markdown
# AIUI Progress

## Current Status
**Active Milestone**: M1 — Stability & Polish
**Overall**: ~65% architectural completeness

## Roadmap
### M0: Foundation ✅
- [x] Chat interface with streaming
- [x] 11 content type renderers
- [x] Responsive layout (mobile + desktop)
- [x] Glass morphism design system
- [x] Claude proxy + web search
- [x] PWA support

### M1: Stability & Polish ✅
- [x] Persistent storage (IndexedDB)
- [x] Error boundaries
- [x] Unit tests for composables
- [x] E2E test coverage
- [x] CI pipeline

(... remaining milestones from Part 2 ...)

## Session Log
<!-- Auto-populated by post-push hook -->
```

### Task 1.2: Create post-push progress hook

**File**: `.claude/hooks/post-push-progress.sh`

A `PostToolUse` hook for Bash that:
1. Reads stdin JSON, extracts `tool_input.command`
2. Checks if command contains `git push` or `git commit`
3. If match: runs `git log --oneline` for commits on current branch not on `main`
4. Reads current PROGRESS.md roadmap section to identify active milestone
5. Outputs JSON with `hookSpecificOutput` containing a message asking Claude to update the Session Log in PROGRESS.md

The hook script should:
- Extract branch name via `git branch --show-current`
- Get commit list via `git log --oneline main..HEAD` (or last 5 commits if no divergence)
- Format the output as structured feedback

### Task 1.3: Register the hook

**File**: `.claude/settings.json`

Add a PostToolUse entry:
```json
{
  "hooks": {
    "PreToolUse": [ ... existing ... ],
    "PostToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "\"$CLAUDE_PROJECT_DIR\"/.claude/hooks/post-push-progress.sh"
          }
        ]
      }
    ]
  }
}
```

---

## Part 2: Long-Term Delivery Roadmap

### M0: Foundation ✅ (Complete)
All done — chat, renderers, layout, design system, proxy, PWA.

---

### M1: Stability & Polish (Infrastructure)

#### Task M1.1: IndexedDB Persistent Storage

**Why**: Conversations are lost on page refresh in production. Currently only persisted via dev-mode `/api/dev-chats` Vite middleware.

**Files to modify**:
- `packages/app/src/stores/chat.ts` (198 lines) — add IndexedDB adapter
- Create `packages/app/src/utils/idb-storage.ts` — IndexedDB wrapper

**Implementation**:
1. Create `idb-storage.ts` with these functions:
   ```ts
   export async function openDB(): Promise<IDBDatabase>
   export async function saveConversation(conv: Conversation): Promise<void>
   export async function loadAllConversations(): Promise<Map<string, Conversation>>
   export async function deleteConversation(id: string): Promise<void>
   ```
   - DB name: `aiui-store`, version 1
   - Object store: `conversations`, keyPath: `id`
   - Index on `updatedAt` for sorted retrieval

2. Modify `chat.ts`:
   - Replace `saveServerChats()` (line 51-62) with `saveConversation()` calls
   - Replace `loadServerChats()` (line 64-84) with `loadAllConversations()`
   - Keep dev-chats middleware as fallback when `indexedDB` unavailable
   - Debounced save stays at 800ms

**Acceptance criteria**:
- Conversations survive page refresh
- New conversations appear after reload
- Delete removes from IndexedDB
- Falls back to dev-chats middleware if IDB unavailable
- `pnpm typecheck` passes

#### Task M1.2: Error Boundaries

**Why**: Zero `onErrorCaptured` usage. Any component error crashes the whole app.

**Files to modify**:
- Create `packages/app/src/components/ui/ErrorBoundary.vue`
- `packages/app/src/pages/ChatPage.vue` (597 lines) — wrap major sections
- `packages/app/src/components/chat/ChatWindow.vue` (248 lines) — wrap message list
- `packages/app/src/components/content/ContentPanel.vue` — wrap detail views

**Implementation**:
1. Create `ErrorBoundary.vue`:
   ```vue
   <script setup lang="ts">
   import { ref, onErrorCaptured } from 'vue'
   defineProps<{ fallbackMessage?: string }>()
   const error = ref<Error | null>(null)
   const hasError = ref(false)
   onErrorCaptured((err) => {
     error.value = err instanceof Error ? err : new Error(String(err))
     hasError.value = true
     return false // prevent propagation
   })
   function retry() { hasError.value = false; error.value = null }
   </script>
   <template>
     <slot v-if="!hasError" />
     <div v-else class="glass-card p-4 text-center">
       <p class="text-white/60">{{ fallbackMessage || 'Something went wrong' }}</p>
       <button class="glass-button-sm mt-2" @click="retry">Retry</button>
     </div>
   </template>
   ```

2. Wrap in ChatPage.vue:
   - Wrap chat column (around `<ChatWindow>`)
   - Wrap content panel column (around `<ContentPanel>`)
   - Wrap detail view column (around `<DetailView>`)

3. Wrap in ChatWindow.vue:
   - Wrap the message loop (v-for of ChatMessage)

4. Wrap in ContentPanel.vue:
   - Wrap each grid component render
   - Wrap detail component render

**Acceptance criteria**:
- A failing renderer shows error card with retry button
- Chat continues working if content panel errors
- Content panel continues if one card errors
- `pnpm typecheck` passes

#### Task M1.3: Unit Tests for contentExtraction

**Why**: `contentExtraction.ts` is 967 lines of regex parsing with zero tests. It's the most critical composable.

**File to create**: `packages/app/src/__tests__/contentExtraction.test.ts`

**Tests to write** (use vitest):
```ts
describe('contentExtraction', () => {
  describe('extractAllFilms', () => {
    it('extracts film_ext tags with title, year, director')
    it('extracts film:id tags and looks up from library')
    it('returns empty array for text with no film tags')
    it('handles multiple films in one message')
    it('handles malformed tags gracefully')
  })

  describe('extractAllSongs', () => {
    it('extracts song_ext tags with title, artist, year')
    it('extracts song:id tags from library')
    it('extracts songs from markdown bold patterns')
    it('deduplicates songs by title+artist')
  })

  describe('extractAllPodcasts', () => {
    it('extracts podcast_ext tags')
    it('extracts podcast:id from library')
  })

  describe('extractAllBooks', () => {
    it('extracts book_ext tags with title, author, year')
    it('handles optional fields')
  })

  describe('extractAllTVSeries', () => {
    it('extracts tv_ext tags')
    it('parses creator and network fields')
  })

  describe('extractAllPlaces', () => {
    it('extracts place_ext tags with all fields')
    it('handles missing optional fields (rating, price)')
  })

  describe('extractMagazineSections', () => {
    it('extracts sections from markdown headings')
    it('captures content between headings')
    it('extracts hero images')
  })

  describe('stripContentTags', () => {
    it('removes all tag types from text')
    it('preserves non-tag content')
    it('handles nested/adjacent tags')
  })

  describe('extractBoldDomainLinks', () => {
    it('extracts **domain.com** patterns with URLs')
    it('extracts markdown links')
  })
})
```

**How to run**: `pnpm test` (vitest via turbo)

**Acceptance criteria**:
- All tests pass
- Covers the 10 main extraction functions
- Tests edge cases (empty input, malformed tags, duplicates)
- `pnpm test` exits 0

#### Task M1.4: Unit Tests for useAI

**File to create**: `packages/app/src/__tests__/useAI.test.ts`

**Tests to write**:
```ts
describe('useAI', () => {
  describe('provider selection', () => {
    it('defaults to first available provider')
    it('switches provider via setActiveProvider')
    it('lists available models for active provider')
  })

  describe('context injection', () => {
    it('includes film library in system prompt')
    it('includes song library in system prompt')
    it('includes content tag format instructions')
  })

  describe('sendMessage', () => {
    it('adds user message to store')
    it('creates assistant message placeholder')
    it('sets isStreaming to true during stream')
    it('sets isStreaming to false after completion')
    it('handles stream errors gracefully')
  })

  describe('stopGeneration', () => {
    it('aborts active stream')
    it('sets isStreaming to false')
  })
})
```

**Note**: Will need to mock `fetch` for streaming tests. Use vitest's `vi.fn()`.

**Acceptance criteria**:
- All tests pass with mocked fetch/SSE
- `pnpm test` exits 0

#### Task M1.5: E2E Test Expansion

**File to modify**: `packages/app/e2e/content-surfaces.spec.ts`

**Tests to add**:
```ts
test('sends a message and receives streaming response')
test('content panel shows film cards when AI mentions films')
test('clicking a film card opens detail view')
test('mobile viewport shows full-screen overlay for content')
test('stop button halts generation')
test('web search toggle works')
test('new conversation clears messages')
test('panel side toggle switches layout')
```

**Acceptance criteria**:
- `pnpm test:e2e` passes (needs dev server running)

---

### M2: Content Experience (UX)

#### Task M2.1: Markdown Rendering in Chat

**Why**: Chat messages display plain text. Markdown (bold, italic, links, code blocks, lists) should render properly.

**Files to modify**:
- `packages/app/src/components/chat/ChatMessage.vue` (333 lines)
- Add `markdown-it` as dependency

**Implementation**:
1. `pnpm add markdown-it` + `pnpm add -D @types/markdown-it` in `packages/app`
2. In ChatMessage.vue:
   - Import and configure markdown-it with safe defaults (no HTML)
   - After `stripContentTags()`, render remaining text through markdown-it
   - Use `v-html` with the sanitized markdown output
   - Add CSS for rendered markdown (code blocks, lists, links) in main.css
   - Ensure content tags are extracted BEFORE markdown rendering

**Security**: markdown-it with `html: false` prevents XSS. No raw HTML passthrough.

**Acceptance criteria**:
- Bold, italic, links, code blocks, lists render in chat
- Content tags still extract correctly (films, songs, etc.)
- No XSS possible
- `pnpm typecheck` passes

#### Task M2.2: Virtual Scrolling for Chat

**Why**: Long conversations with many messages cause scroll jank.

**Files to modify**:
- `packages/app/src/components/chat/ChatWindow.vue`
- Add `@tanstack/vue-virtual` dependency

**Implementation**:
1. `pnpm add @tanstack/vue-virtual` in `packages/app`
2. Replace the message `v-for` loop with `useVirtualizer`:
   - Estimate row heights (user messages ~60px, assistant ~200px)
   - Use dynamic measurement for actual heights
   - Maintain scroll-to-bottom behavior during streaming
   - Keep overscan at 5 items

**Acceptance criteria**:
- Scrolling is smooth with 100+ messages
- Auto-scroll to bottom during streaming still works
- `pnpm typecheck` passes

#### Task M2.3: Music Source Resolution

**Why**: PlayerBar exists but music source resolution is incomplete. Iframe embedding untested.

**Files to modify**:
- `packages/app/src/composables/usePlayer.ts` (185 lines)
- `packages/app/src/components/player/PlayerBar.vue` (165 lines)

**Implementation**:
1. In usePlayer.ts:
   - Add queue management: `queue: ShallowRef<Song[]>`, `currentIndex: Ref<number>`
   - Add `playNext()`, `playPrevious()`, `addToQueue(song)` methods
   - Fix iframe playback (lines 72-83): create Plyr instance for iframes too
   - Add retry logic for failed music searches (try next source)

2. In PlayerBar.vue:
   - Add next/previous buttons
   - Show queue count
   - Add queue panel (slide-up from player)

**Acceptance criteria**:
- Can play songs from search results
- Next/previous navigation works
- Queue persists across song changes
- `pnpm typecheck` passes

#### Task M2.4: Nostr Feed Integration

**Why**: NostrGrid.vue exists but is non-functional. No relay connection.

**Files to modify**:
- `packages/app/src/components/content/NostrGrid.vue`
- Create `packages/app/src/composables/useNostr.ts`

**Implementation**:
1. Create `useNostr.ts`:
   - Connect to public relays (wss://relay.damus.io, wss://nos.lol, wss://relay.snort.social)
   - Use raw WebSocket (no nostr-tools dependency to keep bundle small)
   - Subscribe to kind:1 (text notes) with limit 50
   - Parse NIP-01 event format manually
   - Export `useNostr()` returning `{ events, isConnected, connect, disconnect }`

2. Update NostrGrid.vue:
   - Use `useNostr()` composable
   - Display events as cards with author npub (truncated), content, timestamp
   - Lazy-load on tab activation only

**Acceptance criteria**:
- Nostr tab shows real posts from public relays
- Connection/disconnection is clean (no leaked WebSockets)
- Handles relay errors gracefully
- `pnpm typecheck` passes

---

### M3: Plugin System (Infrastructure)

#### Task M3.1: Activate Plugin Registry at Runtime

**Why**: `packages/core/src/plugins/registry.ts` exists with `registerPlugin()` but nothing calls it.

**Files to modify**:
- `packages/app/src/main.ts` (26 lines) — add plugin initialization
- Create `packages/app/src/plugins/index.ts` — plugin bootstrap
- Create `packages/app/src/plugins/claude-provider.ts` — first AI provider plugin

**Implementation**:
1. Create `plugins/index.ts`:
   ```ts
   export async function initializePlugins() {
     // Register built-in plugins
     const { claudeProvider } = await import('./claude-provider')
     registerPlugin(claudeProvider)
   }
   ```

2. Create `plugins/claude-provider.ts`:
   - Implement `AIProviderAdapter` interface from `@aiui/core`
   - Wrap existing `useAI.ts` streaming logic as a plugin
   - Export as a Tier 1 (trusted) plugin

3. In `main.ts`:
   - Call `initializePlugins()` before app mount
   - Make it async with error handling

**Acceptance criteria**:
- Plugin registry has at least 1 registered plugin at runtime
- Chat still works through the plugin adapter
- `getPluginsByType('ai-provider')` returns the Claude provider
- `pnpm typecheck` passes

#### Task M3.2: Renderer Plugin Registration

**Why**: Content renderers are hardcoded. Making them pluggable enables community extensions.

**Files to modify**:
- Create `packages/app/src/plugins/renderers/film-renderer.ts`
- Create `packages/app/src/plugins/renderers/song-renderer.ts`
- Modify `packages/app/src/plugins/index.ts` — register renderers
- Modify `packages/app/src/components/content/ContentPanel.vue` — use registry lookups

**Implementation**:
1. Create renderer plugins for film and song (as examples):
   ```ts
   const filmRenderer: RendererDefinition = {
     id: 'film',
     name: 'Film Renderer',
     contentType: 'film',
     surfaces: ['chat-preview', 'panel-preview', 'panel-play'],
     chatPreview: FilmCard,
     panelPreview: FilmGrid,
     panelPlay: FilmDetail,
   }
   ```

2. Register in `plugins/index.ts` via `registerRenderer()`

3. In ContentPanel.vue, look up renderers via `getRendererForContentType()` instead of hardcoded imports (gradual migration — start with film/song, keep others hardcoded)

**Acceptance criteria**:
- Film and song renderers registered via plugin system
- `getAllRenderers()` returns registered renderers
- Content panel still renders correctly
- `pnpm typecheck` passes

---

### M4: Social & Discovery (UX)

#### Task M4.1: Social Embeds

**Why**: Nostr notes referenced in chat should render as rich embeds, not raw text.

**Files to create/modify**:
- Create `packages/app/src/components/chat/NostrEmbed.vue`
- Modify `packages/app/src/components/chat/ChatMessage.vue` — detect and render nostr: URIs

**Implementation**:
1. Create `NostrEmbed.vue`:
   - Accept `noteId` or `npub` prop
   - Fetch note from relays (reuse `useNostr` composable from M2.4)
   - Display: author npub (truncated), content, timestamp, relay source
   - Glass card styling matching existing design system
   - Loading skeleton while fetching
   - Error state if note not found

2. In ChatMessage.vue:
   - Regex detect `nostr:note1...`, `nostr:npub1...`, `nostr:nevent1...` patterns
   - Replace with `<NostrEmbed>` component inline
   - Handle bech32 decoding (NIP-19) for note/npub/nevent

**Acceptance criteria**:
- `nostr:note1...` in chat renders as embedded card
- `nostr:npub1...` renders as profile card
- Graceful fallback if relay unreachable
- `pnpm typecheck` passes

#### Task M4.2: Federated Search

**Why**: Search currently only queries web. Should search across all content types simultaneously.

**Files to create/modify**:
- Create `packages/app/src/composables/useFederatedSearch.ts`
- Modify `packages/app/src/components/chat/ChatInput.vue` — add search mode
- Create `packages/app/src/components/ui/SearchResults.vue`

**Implementation**:
1. Create `useFederatedSearch.ts`:
   ```ts
   interface SearchResult {
     type: 'film' | 'song' | 'podcast' | 'book' | 'article' | 'place' | 'web'
     title: string
     subtitle: string
     thumbnail?: string
     data: unknown // type-specific payload
   }
   export function useFederatedSearch() {
     // Search across: film library, song library, podcast library, web (DDG/SearXNG)
     // Return unified results sorted by relevance
     // Debounce input (150ms)
     // Cancel previous searches on new input
   }
   ```

2. In ChatInput.vue:
   - Add `/search` command prefix detection
   - When typing after `/search`, show SearchResults overlay above input
   - Selecting a result inserts it as a content reference in the message

3. Create SearchResults.vue:
   - Grouped by content type with type icons
   - Keyboard navigation (arrow keys + enter)
   - Glass morphism dropdown styling

**Acceptance criteria**:
- `/search matrix` returns films, songs, articles matching "matrix"
- Results grouped by type
- Selecting a result works
- `pnpm typecheck` passes

#### Task M4.3: Bookmarks/Favorites

**Why**: Users can't save interesting content items for later.

**Files to create/modify**:
- Create `packages/app/src/stores/favorites.ts` — Pinia store
- Create `packages/app/src/components/ui/FavoriteButton.vue`
- Create `packages/app/src/components/content/FavoritesGrid.vue`
- Modify content card components (FilmCard, SongCard, etc.) — add favorite button
- Modify `packages/app/src/components/content/ContentPanel.vue` — add Favorites tab

**Implementation**:
1. Create `favorites.ts` Pinia store:
   ```ts
   interface FavoriteItem {
     id: string
     type: 'film' | 'song' | 'podcast' | 'book' | 'tv' | 'place' | 'article'
     title: string
     data: unknown
     savedAt: number
   }
   // Persist to IndexedDB (reuse idb-storage from M1.1)
   // Methods: addFavorite, removeFavorite, isFavorited, getFavoritesByType
   ```

2. Create `FavoriteButton.vue`:
   - Heart icon toggle (outline = not saved, filled = saved)
   - Animate on toggle (scale bounce)
   - Bitcoin orange when favorited

3. Create `FavoritesGrid.vue`:
   - Tab in ContentPanel showing all saved items
   - Filter by content type
   - Sort by date saved
   - Remove from favorites via swipe or button

4. Add FavoriteButton to existing cards: FilmCard, SongCard, BookCard, etc.

**Acceptance criteria**:
- Can favorite/unfavorite any content item
- Favorites persist across page refresh (IndexedDB)
- Favorites tab shows all saved items
- Filter by type works
- `pnpm typecheck` passes

---

### M5: Security & Privacy (Infrastructure)

#### Task M5.1: E2E Encryption

**Why**: Conversations stored in IndexedDB are plaintext. Need encryption at rest.

**Files to create/modify**:
- Create `packages/app/src/utils/crypto.ts`
- Modify `packages/app/src/utils/idb-storage.ts` — encrypt before write, decrypt on read

**Implementation**:
1. Create `crypto.ts`:
   ```ts
   // Use Web Crypto API (no external dependencies)
   export async function deriveKey(password: string, salt: Uint8Array): Promise<CryptoKey>
   // PBKDF2, 100K iterations, SHA-256

   export async function encrypt(data: string, key: CryptoKey): Promise<{ ciphertext: ArrayBuffer; iv: Uint8Array }>
   // AES-256-GCM, random 12-byte IV

   export async function decrypt(ciphertext: ArrayBuffer, iv: Uint8Array, key: CryptoKey): Promise<string>
   // AES-256-GCM decrypt

   export async function generateSalt(): Promise<Uint8Array>
   // 16 random bytes
   ```

2. Modify `idb-storage.ts`:
   - Add optional encryption parameter to save/load functions
   - When `VITE_DISABLE_CRYPTO=true` (dev mode), skip encryption
   - Store salt alongside encrypted data
   - Key derived from user passphrase (prompted on first use)

**Acceptance criteria**:
- Conversations encrypted in IndexedDB when crypto enabled
- Dev mode (`VITE_DISABLE_CRYPTO=true`) bypasses encryption
- Decryption with wrong passphrase fails gracefully
- `pnpm typecheck` passes
- Unit tests for encrypt/decrypt round-trip

#### Task M5.2: Encrypted Storage Layer

**Why**: All IndexedDB data (conversations, favorites, settings) should use the encryption layer.

**Files to modify**:
- Modify `packages/app/src/stores/favorites.ts` — use encrypted storage
- Create `packages/app/src/components/ui/PassphraseDialog.vue`
- Modify `packages/app/src/main.ts` — prompt for passphrase on startup

**Implementation**:
1. Create `PassphraseDialog.vue`:
   - Modal dialog with passphrase input
   - "Remember for this session" checkbox (holds key in memory)
   - Create new / enter existing passphrase flow
   - Glass card styling, min 16px font (no iOS zoom)

2. Wire encryption into all storage operations:
   - Conversations (chat.ts store)
   - Favorites (favorites.ts store)
   - Future: settings, API keys

**Acceptance criteria**:
- First launch prompts for passphrase creation
- Subsequent launches prompt for passphrase entry
- Wrong passphrase shows error, does not corrupt data
- Session key held in memory (not persisted)
- `pnpm typecheck` passes

#### Task M5.3: API Key Vault

**Why**: API keys (Claude, OpenRouter) are currently stored in plaintext localStorage.

**Files to create/modify**:
- Create `packages/app/src/utils/key-vault.ts`
- Create `packages/app/src/components/settings/ApiKeyManager.vue`
- Modify `packages/app/src/composables/useAI.ts` — read keys from vault

**Implementation**:
1. Create `key-vault.ts`:
   ```ts
   // Encrypted storage for API keys using crypto.ts
   export async function storeApiKey(provider: string, key: string): Promise<void>
   export async function getApiKey(provider: string): Promise<string | null>
   export async function deleteApiKey(provider: string): Promise<void>
   export async function listProviders(): Promise<string[]>
   // Keys encrypted with session-derived key from passphrase
   // Stored in dedicated IndexedDB object store: 'api-keys'
   ```

2. Create `ApiKeyManager.vue`:
   - List configured providers
   - Add/remove API keys
   - Keys masked in UI (show last 4 chars)
   - Test connection button per provider

3. In `useAI.ts`:
   - Replace direct env var / localStorage reads with vault lookups
   - Fallback to env vars for dev mode

**Acceptance criteria**:
- API keys encrypted at rest
- Keys never appear in console/logs
- UI shows masked keys
- Test connection verifies key works
- `pnpm typecheck` passes

---

### M6: Payments & Identity (UX + Infrastructure)

#### Task M6.1: Lightning Wallet Deep-links

**Why**: AIUI is Bitcoin-only. Need to deep-link to external Lightning wallets for payments.

**Files to create/modify**:
- Create `packages/app/src/utils/lightning.ts`
- Create `packages/app/src/components/ui/PaymentButton.vue`
- Create `packages/app/src/components/ui/LightningInvoice.vue`

**Implementation**:
1. Create `lightning.ts`:
   ```ts
   // Generate LNURL-pay links, BIP21 URIs, Lightning: URIs
   export function createLightningUri(invoice: string): string
   export function createBip21Uri(address: string, amount?: number, label?: string): string
   export function detectWallet(): 'strike' | 'muun' | 'phoenix' | 'zeus' | 'generic'
   // Deep-link formats: lightning:BOLT11, bitcoin:?lightning=BOLT11
   ```

2. Create `PaymentButton.vue`:
   - Bitcoin orange gradient button
   - Shows sat amount
   - On click: generates deep-link URI, opens wallet
   - Fallback: show QR code with invoice string
   - Copy invoice to clipboard button

3. Create `LightningInvoice.vue`:
   - Display BOLT11 invoice as QR code (use `qrcode` lib or canvas)
   - Show amount in sats
   - Expiry countdown
   - Copy button

**Acceptance criteria**:
- Payment button generates valid Lightning URIs
- Deep-link opens system wallet picker on mobile
- QR fallback for desktop
- `pnpm typecheck` passes

#### Task M6.2: Cashu Token Support

**Why**: Cashu ecash tokens enable offline micropayments. Display and copy Cashu tokens in chat.

**Files to create/modify**:
- Create `packages/app/src/utils/cashu.ts`
- Create `packages/app/src/components/chat/CashuToken.vue`
- Modify `packages/app/src/components/chat/ChatMessage.vue` — detect cashu tokens

**Implementation**:
1. Create `cashu.ts`:
   ```ts
   // Parse Cashu token format (cashuA...)
   export function parseCashuToken(token: string): { mint: string; amount: number; unit: string } | null
   export function isCashuToken(text: string): boolean
   // No wallet functionality — AIUI is never a wallet
   // Just parse, display, and deep-link to external wallet
   ```

2. Create `CashuToken.vue`:
   - Detect `cashuA...` strings in chat messages
   - Display as card: amount, mint URL (truncated), copy button
   - "Open in wallet" deep-link button
   - Glass card styling with Bitcoin orange accent

3. In ChatMessage.vue:
   - Regex detect Cashu tokens
   - Replace inline with `<CashuToken>` component

**Acceptance criteria**:
- Cashu tokens in chat render as rich cards
- Copy token to clipboard works
- Deep-link to wallet works
- Invalid tokens show graceful fallback
- `pnpm typecheck` passes

#### Task M6.3: Nostr Identity (NIP-07)

**Why**: Enable login via Nostr browser extension (nos2x, Alby, etc.) for identity.

**Files to create/modify**:
- Create `packages/app/src/composables/useNostrIdentity.ts`
- Create `packages/app/src/components/settings/NostrLogin.vue`
- Modify `packages/app/src/stores/` — add user identity store

**Implementation**:
1. Create `useNostrIdentity.ts`:
   ```ts
   // NIP-07: window.nostr API
   export function useNostrIdentity() {
     const isAvailable: Ref<boolean> // window.nostr exists
     const pubkey: Ref<string | null>
     const npub: Ref<string | null> // bech32 encoded
     async function login(): Promise<void> // calls window.nostr.getPublicKey()
     async function sign(event: NostrEvent): Promise<NostrEvent> // calls window.nostr.signEvent()
     function logout(): void
   }
   ```

2. Create `NostrLogin.vue`:
   - "Login with Nostr" button (purple/Nostr brand color)
   - Shows npub when logged in (truncated with copy)
   - Logout button
   - Detects if NIP-07 extension is installed

**Acceptance criteria**:
- Login with nos2x/Alby extension works
- Public key displayed as npub
- Sign events for Nostr posting
- Graceful message if no extension installed
- `pnpm typecheck` passes

---

### M7: Platform (Infrastructure)

#### Task M7.1: MCP Server Integration

**Why**: Model Context Protocol enables rich tool use. AIUI should expose content surfaces as MCP tools.

**Files to create/modify**:
- Create `packages/app/src/plugins/mcp-server.ts`
- Modify `packages/app/src/composables/useAI.ts` — add MCP tool handling

**Implementation**:
1. Create `mcp-server.ts`:
   ```ts
   // Expose AIUI capabilities as MCP tools
   const tools = [
     { name: 'search_films', description: 'Search film library', inputSchema: {...} },
     { name: 'search_songs', description: 'Search song library', inputSchema: {...} },
     { name: 'search_web', description: 'Search the web', inputSchema: {...} },
     { name: 'get_nostr_feed', description: 'Fetch Nostr notes', inputSchema: {...} },
   ]
   // Handle tool_use responses from AI and route to appropriate composable
   ```

2. In useAI.ts:
   - Parse tool_use blocks from Claude responses
   - Route to appropriate handler (film search, web search, etc.)
   - Return tool results back in conversation

**Acceptance criteria**:
- Claude can call tools via MCP format
- Tool results display as content in panel
- `pnpm typecheck` passes

#### Task M7.2: Multi-provider AI Normalization

**Why**: Different AI providers (Claude, OpenRouter, Ollama) have different APIs. Normalize them.

**Files to create/modify**:
- Create `packages/app/src/adapters/claude-adapter.ts`
- Create `packages/app/src/adapters/openrouter-adapter.ts`
- Create `packages/app/src/adapters/ollama-adapter.ts`
- Create `packages/app/src/adapters/types.ts` — unified interface
- Modify `packages/app/src/composables/useAI.ts` — use adapter pattern

**Implementation**:
1. Create `types.ts`:
   ```ts
   interface AIAdapter {
     id: string
     name: string
     chat(messages: Message[], options: ChatOptions): AsyncIterable<string>
     models(): Promise<Model[]>
     supportsStreaming: boolean
     supportsVision: boolean
     supportsTools: boolean
   }
   ```

2. Create adapters for Claude (existing logic), OpenRouter (OpenAI-compatible), Ollama (local).

3. Refactor useAI.ts to select adapter by provider setting.

**Acceptance criteria**:
- Can switch between Claude/OpenRouter/Ollama
- Streaming works with all providers
- Content extraction works regardless of provider
- `pnpm typecheck` passes

#### Task M7.3: Tauri Desktop Build

**Why**: Desktop app via Tauri for native experience with system tray, global shortcuts.

**Files to create/modify**:
- Create `src-tauri/` directory with Tauri config
- Create `src-tauri/tauri.conf.json`
- Create `src-tauri/src/main.rs`
- Modify `packages/app/package.json` — add tauri scripts

**Implementation**:
1. Initialize Tauri in the app package:
   - `pnpm add -D @tauri-apps/cli @tauri-apps/api` in packages/app
   - Configure window: frameless with custom titlebar, transparent background
   - System tray with quick-access menu
   - Global shortcut (Cmd+Shift+A) to show/hide window

2. Tauri config:
   - Window size: 1200x800, min 800x600
   - Transparent background (for glass morphism)
   - Auto-updater enabled
   - File system scope: app data directory only

**Acceptance criteria**:
- `pnpm tauri dev` launches desktop app
- Glass morphism renders correctly with transparent window
- System tray works
- `pnpm tauri build` produces .dmg/.app
- `pnpm typecheck` passes

#### Task M7.4: Offline Mode

**Why**: AIUI should work without internet for browsing saved content.

**Files to create/modify**:
- Modify `packages/app/src/sw.ts` or PWA config — cache strategies
- Create `packages/app/src/composables/useOffline.ts`
- Modify UI components — offline indicators

**Implementation**:
1. Create `useOffline.ts`:
   ```ts
   export function useOffline() {
     const isOnline: Ref<boolean> // navigator.onLine + event listeners
     const pendingSync: Ref<number> // count of items waiting to sync
     function queueForSync(action: SyncAction): void
     function processSyncQueue(): Promise<void>
   }
   ```

2. PWA cache strategies:
   - App shell: cache-first (HTML, JS, CSS, fonts)
   - API responses: network-first with cache fallback
   - Images: cache-first with stale-while-revalidate
   - IndexedDB data: always available offline

3. UI indicators:
   - Subtle banner when offline ("Offline — browsing saved content")
   - Disable AI chat input when offline (grey out with tooltip)
   - Show cached content (favorites, saved conversations)

**Acceptance criteria**:
- App loads without internet
- Saved conversations and favorites accessible offline
- Chat disabled with clear offline indicator
- Reconnection triggers sync
- `pnpm typecheck` passes

---

## Part 3: Automated Session Execution Order

For automated late-night Claude sessions, execute tasks in this order:

### Priority Queue (each session picks next incomplete task):

**M1: Stability & Polish**
1. **Task M1.2** — Error boundaries *(verify existing ErrorBoundary.vue, wrap remaining components)*
2. **Task M1.3** — Unit tests for contentExtraction *(create __tests__/contentExtraction.test.ts)*
3. **Task M1.1** — IndexedDB persistent storage *(create idb-storage.ts, modify chat.ts)*
4. **Task M1.4** — Unit tests for useAI *(create __tests__/useAI.test.ts with mocked fetch)*
5. **Task M1.5** — E2E test expansion *(8 new tests in content-surfaces.spec.ts)*

**M2: Content Experience**
6. **Task M2.1** — Markdown rendering in chat *(add markdown-it, modify ChatMessage.vue)*
7. **Task M2.3** — Music source resolution + queue *(fix usePlayer.ts, update PlayerBar.vue)*
8. **Task M2.2** — Virtual scrolling for chat *(add @tanstack/vue-virtual to ChatWindow.vue)*
9. **Task M2.4** — Nostr feed integration *(create useNostr.ts, update NostrGrid.vue)*

**M3: Plugin System**
10. **Task M3.1** — Activate plugin registry *(create plugins/index.ts, claude-provider.ts)*
11. **Task M3.2** — Renderer plugin registration *(film/song renderer plugins)*

**M4: Social & Discovery**
12. **Task M4.1** — Social embeds *(NostrEmbed.vue, nostr: URI detection in chat)*
13. **Task M4.2** — Federated search *(useFederatedSearch.ts, /search command)*
14. **Task M4.3** — Bookmarks/favorites *(favorites.ts store, FavoriteButton, FavoritesGrid)*

**M5: Security & Privacy**
15. **Task M5.1** — E2E encryption *(crypto.ts with Web Crypto API AES-256-GCM)*
16. **Task M5.2** — Encrypted storage layer *(PassphraseDialog, wire encryption to all stores)*
17. **Task M5.3** — API key vault *(key-vault.ts, ApiKeyManager.vue)*

**M6: Payments & Identity**
18. **Task M6.1** — Lightning wallet deep-links *(lightning.ts, PaymentButton, LightningInvoice)*
19. **Task M6.2** — Cashu token support *(cashu.ts, CashuToken.vue inline in chat)*
20. **Task M6.3** — Nostr identity NIP-07 *(useNostrIdentity.ts, NostrLogin.vue)*

**M7: Platform**
21. **Task M7.1** — MCP server integration *(mcp-server.ts, tool routing in useAI)*
22. **Task M7.2** — Multi-provider AI normalization *(adapter pattern for Claude/OpenRouter/Ollama)*
23. **Task M7.3** — Tauri desktop build *(src-tauri config, transparent window, system tray)*
24. **Task M7.4** — Offline mode *(useOffline.ts, cache strategies, offline UI indicators)*

### Session Protocol

Each automated session should:
1. Read `PROGRESS.md` to find the next incomplete task
2. Read this plan file for the task's detailed spec
3. Execute the task following the spec exactly
4. Run `pnpm typecheck` after changes
5. Run `pnpm lint` after changes
6. Run `pnpm test` if unit tests exist
7. Commit with conventional format: `type(scope): description`
8. Push to current branch
9. Update PROGRESS.md session log (triggered by hook, or manually)

---

## Verification

After implementing Part 1 (progress automation):
1. Run `pnpm typecheck && pnpm lint` — should pass
2. Commit a change and push — hook should fire
3. Verify PROGRESS.md gets a session log entry
4. Test from a different worktree — should work identically

After each M1–M3 task:
1. `pnpm typecheck` passes
2. `pnpm lint` passes
3. `pnpm test` passes (if tests exist)
4. Dev server runs without errors (`pnpm dev`)
5. Manual smoke test: send a message, see content render
