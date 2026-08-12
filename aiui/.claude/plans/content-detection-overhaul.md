# Plan: Overhaul Content Detection + Add Apps Tab

## Context

The content surfacing system misses many common AI response patterns. Example: AI responds about Nostr (mentioning damus.io, primal.net, snort.social) but the Nostr tab never surfaces. Query/response classifiers use narrow regexes that miss natural language variations. There's no "topic detection" layer, no app detection, and bare domains in AI text aren't extracted as websites.

**Goals:**
1. Fix content detection to handle how AIs actually respond
2. Add Nostr tab surfacing (currently only via `/nostr` command)
3. Add Apps tab with curated Nostr + Bitcoin ecosystem apps (local DB + AI extraction fallback)
4. Extract bare domains from AI text (e.g. "check out damus.io")

---

## Part 1: Expand Query & Response Classifiers

**File:** `packages/app/src/composables/contentFiltering.ts`

### 1A. Add Nostr classifiers (new functions)

- `isNostrQuery(q)` — matches: nostr, npub, nip-\d, damus, primal, snort, amethyst, coracle, zap, relay, note1, nevent, nprofile, fiatjaf, nostrich, "decentralized social"
- `isNostrLikeResponse(text)` — requires literal "nostr" OR 2+ Nostr-specific signals (npub, nip-, client names, relay+wss, zap+lightning)

### 1B. Add App classifiers (new functions)

- `isAppQuery(q)` — matches: app, client, wallet, tool, software, download, install, "what app", "best app for", "recommend.*app"
- `isAppLikeResponse(text)` — matches: "you can use", "popular clients include", "I'd recommend", "available on", "download from"

### 1C. Expand existing classifiers with broader patterns

| Classifier | Add these patterns |
|---|---|
| `isNewsQuery` | "what happened today", "any updates on", "trending", "catch me up", "brief me", "current events" |
| `isMusicQuery` | "genre", "spotify", "bandcamp", "grammys", "billboard", "mixtape", "discography", "banger", "favorite jam" |
| `isBookQuery` | "what should I read", "favorite reads", "reading list", "book club", "memoir", "audiobook", "goodreads", "worth reading" |
| `isTVQuery` | "what's good on netflix", "anything to binge", "hbo", "disney+", "apple tv", "amazon prime", "docuseries", "limited series" |
| `isPlaceQuery` | "hungry", "food near me", "best brunch spot", "happy hour", "speakeasy", "rooftop bar", "food truck" |
| `isWebsitesQuery` | "point me to", "link me", "any good sites", "tools for", "platforms for" |
| `isWebsitesLikeResponse` | "here are some resources", "I'd recommend checking", "you can visit", "useful resources" |
| `isNewsLikeResponse` | "I can't access the web but", "having trouble reaching", "unable to browse but" |

### 1D. Update `preferredFirstTab()` — add nostr + app checks

### 1E. Update `filterTabsByContext()` — add `hasNostr` and `hasApps` params, integrate into tab ordering

---

## Part 2: Bare Domain Extraction

**File:** `packages/app/src/composables/contentExtraction.ts`

Add `extractBareDomainLinks(text)`:
- Detect plain-text domains like "damus.io", "primal.net" not inside markdown links or bold patterns
- Skip positions covered by existing extractors (markdown links, bold-domain, full URLs)
- Require known TLDs (.com, .org, .io, .net, .social, .app, etc.)
- Block file extensions (.js, .ts, .vue, .json, .css)
- Use existing `normUrl()` for dedup

---

## Part 3: Apps Tab — Curated Database + AI Extraction

### 3A. Create app database

**New file:** `packages/app/src/data/apps.ts`

```ts
interface AppEntry {
  id: string
  name: string
  description: string          // One-liner
  longDescription: string      // Why use this, how it works
  category: 'nostr-client' | 'lightning-wallet' | 'bitcoin-wallet' | 'privacy' | 'node' | 'dev-tool' | 'relay'
  platforms: ('ios' | 'android' | 'web' | 'desktop' | 'cli' | 'nodeos')[]
  url: string
  icon?: string
  keywords: string[]           // For matching AI responses
  howTo?: string[]             // Getting started steps
  relatedApps?: string[]       // IDs of related apps
}
```

**Initial curated apps (~25-30):**
- Nostr clients: Damus, Primal, Snort, Amethyst, Coracle, Iris, Nostrudel, nos.social
- Lightning wallets: Phoenix, Mutiny, Breez, Zeus, Alby, Wallet of Satoshi
- Bitcoin wallets: Sparrow, Blue Wallet, Nunchuk, Coldcard, Green
- Privacy tools: Tor, SimpleX Chat, Signal, Mullvad VPN
- Node software: Start9, Umbrel, RaspiBlitz, myNode
- Dev tools: NDK, nostr-tools, Nak

### 3B. Add app extraction

**File:** `packages/app/src/composables/contentExtraction.ts`

Add `extractApps(text, userQuery)`:
1. Match AI text against known app names/keywords from database
2. If app query detected OR 2+ known apps mentioned → return matched apps
3. For unknown apps, create basic entries from context (name + URL if bare domain found)

### 3C. Create UI components

**New files:**
- `packages/app/src/components/content/AppsGrid.vue` — Grid of app cards (icon, name, category badge, one-liner)
- `packages/app/src/components/content/AppDetail.vue` — Detail: icon, name, platforms, long description, how-to steps, link, related apps

Follow existing grid/detail patterns (e.g. `BookGrid.vue`/`BookDetail.vue`).

### 3D. Register in ContentPanel.vue

Add rendering for `activeTab === 'app'`, add `'app'` to `ContentTab` type.

---

## Part 4: Wire Everything Together

**File:** `packages/app/src/composables/useContentPanel.ts`

In `updatePanelFromText()`:
- Call `extractBareDomainLinks(text)`, merge with website sources
- Call `extractApps(text, userQuery)`
- Compute `hasNostr = isNostrQuery(userQuery) || isNostrLikeResponse(text)`
- Compute `hasApps = apps.length > 0`
- Pass `hasNostr` and `hasApps` to `filterTabsByContext()`
- Add `panelApps` ref, title logic for apps/nostr tabs

Same changes in `getContextualInlineContent()`.

Broaden magazine detection: add tech/protocol keywords, surface magazine for 3+ sections with no other structured content.

---

## Part 5: PromptIndex badges

**File:** `packages/app/src/components/chat/PromptIndex.vue`

Add 'Nostr' and 'Apps' badge detection.

---

## Implementation Order

1. `contentFiltering.ts` — classifiers + filterTabsByContext signature
2. `contentExtraction.ts` — `extractBareDomainLinks()` + `extractApps()`
3. `data/apps.ts` — curated app database
4. `useContentPanel.ts` — wire everything
5. `AppsGrid.vue` + `AppDetail.vue` — UI components
6. `ContentPanel.vue` — register tab + components
7. `PromptIndex.vue` — badges
8. Typecheck + manual test

## Verification

1. `pnpm typecheck` passes
2. "tell me about Nostr" → Nostr + magazine tabs surface
3. "best Nostr clients?" → Apps tab with Damus, Primal, Snort
4. "recommend a bitcoin wallet" → Apps tab with Phoenix, Sparrow
5. "what happened with BIP 110?" → Magazine tab (regression)
6. "best movies of 2024" → Films tab (regression)
7. Bare domains in AI text extracted as websites
8. PromptIndex badges show Nostr/Apps
