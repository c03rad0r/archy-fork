# AIUI Project Memory

## Session Startup
1. Run `preview_start` with name `"app"` immediately — runs both Vite (:5173) + Claude proxy (:3141) via `packages/app/scripts/dev.sh`
2. Always commit work before ending a session
3. Work on `development` branch, merge to `main` only when production ready

## User Preferences
- NO worktrees, NO temporary branches — just `development` and `main`
- Always use combined dev script (proxy + frontend), never bare `vite`
- Commit frequently to avoid losing work

## Current State (2026-03-04)
- Branch: `overnight/2026-03-03`, all committed and pushed to remote (git.tx1138.com)
- Typecheck passes clean

## What's Been Built
- Chat: AI streaming with stop generation, web search, article integration, paste & extract
- Content panel tabs: Films, Music, Magazine, News, Books, TV Series, Images, Places, Code, Design System, Nostr, **Apps**
- Detail views for each content type (side-by-side desktop, overlay mobile)
- **Apps tab**: curated DB of ~30 Nostr/Bitcoin apps, AppsGrid + AppDetail with search/category filtering/how-to
- Design system viewer (grid + detail) for tokens, colors, typography, components
- Nostr feed scaffold with note/article/zap filtering
- Content extraction: contentExtraction.ts + contentFiltering.ts (overhauled classifiers)
- **Bare domain extraction** from AI text (e.g. "check out damus.io")
- Banner fallback composable (primary → API → gradient)
- Image fallbacks: Wikipedia + Google Books sources
- Loading skeletons per content type variant
- Project grid with breadcrumb nav and inline creation
- Filesystem Vite plugin for local project browsing
- PWA with star icon, TMDB proxy, Jamendo for music
- **Slash command palette**: /code, /nostr, /design, /search show in palette with auto-send
- **Chat action buttons**: wrapped in glass container (backdrop blur, border, shadow)
- **Settings modal**: Memory + Advanced Settings via gear icon
- **Chat history**: dedicated clock icon button
- **Web search**: Brave API primary, SearXNG rotation fallback, DuckDuckGo fallback
- iOS HIG mobile UX rules in `.cursor/rules/15-mobile-ux.mdc`

## Key Files
- Dev script: `packages/app/scripts/dev.sh`
- Launch config: `.claude/launch.json` (name: "app")
- Main page: `packages/app/src/pages/ChatPage.vue`
- Content panel: `packages/app/src/components/content/ContentPanel.vue`
- Content grids: `packages/app/src/components/content/*Grid.vue`
- Detail views: `packages/app/src/components/content/*Detail.vue`
- **Apps**: `packages/app/src/data/apps.ts` (curated DB), `AppsGrid.vue`, `AppDetail.vue`
- AI composable: `packages/app/src/composables/useAI.ts`
- Content extraction: `packages/app/src/composables/contentExtraction.ts`
- Content filtering: `packages/app/src/composables/contentFiltering.ts`
- Content panel logic: `packages/app/src/composables/useContentPanel.ts`
- Image fallbacks: `packages/app/src/composables/useImageFallback.ts`
- Banner fallback: `packages/app/src/composables/useBannerFallback.ts`
- Chat input: `packages/app/src/components/chat/ChatInput.vue`
- Prompt palette: `packages/app/src/components/chat/PromptPalette.vue`
- Chat message: `packages/app/src/components/chat/ChatMessage.vue`
- Settings modal: `packages/app/src/components/chat/SettingsModal.vue`
- Web search plugin: `packages/app/vite-web-search.ts`
- Prompt templates store: `packages/app/src/stores/promptTemplates.ts`

## Recent Session Work (2026-03-04)
See `session-2026-03-04.md` for details.
