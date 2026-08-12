# CLAUDE.md — AIUI Project Guide

## Project Overview

AIUI is a next-generation AI content surface UI. It's a **pnpm monorepo** with two packages:

- `@aiui/app` — Reference application (Vite + Vue 3 + Tailwind CSS)
- `@aiui/core` — Reusable component library

**Stack**: Vue 3 (Composition API), TypeScript ~5.8 (strict), Vite, Tailwind CSS, Pinia, Vue Router, Turborepo
**Node**: >=20.0.0 | **pnpm**: >=10.0.0

## Quick Reference

```bash
pnpm dev            # Run app dev server + Claude proxy
pnpm dev:core       # Watch-build core library
pnpm build          # Build all packages (turbo)
pnpm test           # Run tests (vitest)
pnpm lint           # Lint all packages (eslint)
pnpm typecheck      # Type-check all packages (vue-tsc)
pnpm clean          # Remove dist/ directories
```

Dev server: `http://localhost:5173` | Claude proxy: `http://localhost:3141`

## Core Philosophy

- **Open source only** — MIT/Apache-2.0 licensed dependencies only
- **Decentralized-first** — Pluggable adapters, no vendor lock-in
- **Bitcoin only** — sats/Lightning/Cashu/Fedimint. Never fiat, never altcoins. AIUI is never a wallet — always deep-link to external wallets
- **Privacy-first** — E2E encryption (tweetnacl.js), encrypted local storage (AES-256-GCM), no tracking/telemetry
- **Mobile-first, everywhere-perfect** — Desktop is an enhancement of the mobile experience
- **Plugin-everything** — All integrations go through typed plugin interfaces

## Vue 3 Conventions

**Always use `<script setup lang="ts">`** — never Options API.

### Script section ordering

Imports → Props (`defineProps`) → Emits (`defineEmits`) → Reactive state → Computed → Watchers → Methods → Lifecycle hooks → `defineExpose`

### Naming

| Thing | Convention | Example |
|-------|-----------|---------|
| Components | PascalCase | `ProjectCard.vue` |
| Composables | camelCase, `use` prefix | `useTheme.ts` |
| Props (JS) | camelCase | `projectName` |
| Props (template) | kebab-case | `project-name` |
| Boolean props | `is`/`has`/`can`/`should` prefix | `isVisible`, `canEdit` |
| Emits (template) | kebab-case with colon namespacing | `project:updated` |
| Stores | camelCase, `use` prefix, `Store` suffix | `useSettingsStore` |

### Reactive state rules

- `ref()` for primitives, `reactive()` for objects
- `computed()` for derived values — no side effects in computed
- `shallowRef()` for large collections/objects not requiring deep reactivity
- Always use unique IDs for `:key` — never array index

### Props

Always use object-style with type annotations, never array-style:

```ts
// Correct
defineProps<{ title: string; count?: number }>()

// Wrong
defineProps(['title', 'count'])
```

### Performance

- Lazy load with `defineAsyncComponent` for non-critical components
- Use `onErrorCaptured` for error boundaries
- Always handle loading/error/data states in async operations

## File Structure

```
packages/app/src/
├── components/
│   ├── ui/              # Generic UI components
│   ├── chat/            # Chat interface components
│   ├── content-panel/   # Content panel components
│   ├── renderers/       # Content type renderers
│   └── layout/          # Layout components
├── composables/         # Shared composition functions
├── stores/              # Pinia stores
├── pages/               # Route-level components
├── styles/              # Global CSS, themes, tokens
├── utils/               # Pure utility functions
├── types/               # TypeScript type definitions
├── plugins/             # Plugin system
└── mocks/               # Dev fixtures & mock data

packages/core/src/
├── plugins/             # Plugin system interfaces
└── types/               # Shared TypeScript types
```

## Tailwind & Design System

### Glass Morphism (Archy-derived)

This project uses a glass morphism design language. Key utility classes:

| Class | Purpose |
|-------|---------|
| `.glass` | Standard glass: `rgba(0,0,0,0.35)`, `blur(18px)`, white border 0.18 opacity |
| `.glass-strong` | Stronger blur: `blur(24px)` |
| `.glass-card` | Card variant: `rgba(0,0,0,0.65)`, `border-radius: 1rem` |
| `.glass-button` | Button: 48px height, `rgba(0,0,0,0.6)`, `blur(18px)` |
| `.glass-button-sm` | Compact button variant |
| `.gradient-card` | Gradient background card |

### Spacing

4px grid system: `1`=4px, `2`=8px, `3`=12px, `4`=16px, etc.

### Colors

- Background: `#0a0a0a` (near-black)
- Accent / Bitcoin orange: `#F7931A`
- Primary: `#606060`
- Text opacity scale: `/25` (placeholder) → `/40` (muted) → `/60` (secondary) → `/70` (interactive) → `/80` (body) → `/90` (emphasis) → `/96` (headings) → `text-white` (active)
- No separator borders between major sections

### Typography

`Inter`/`system-ui` for body, `Menlo`/`Monaco` for monospace.

### Responsive breakpoints (mobile-first)

`sm` 640px → `md` 768px → `lg` 1024px → `xl` 1280px → `2xl` 1536px

### Animations

- `animate-fade-up` (900ms), `animate-fade-up-fast` (400ms), `animate-fade-in` (500ms), `animate-scale-in` (250ms)
- Duration: 100ms micro, 200ms fast, 300ms moderate, 500ms normal, 600ms max
- Easing: `ease-out` for entrances (90% of animations), `ease-in` for exits
- Only animate `transform` and `opacity` — avoid animating layout properties
- Always respect `prefers-reduced-motion`

## Content Surfaces Architecture

Every content renderer supports up to five surfaces:

1. **Chat Preview** (~120px max) — inline bubble, identify content at a glance
2. **Chat Play** (~200px max) — inline playback with expand button
3. **Panel Preview** (unlimited) — full browsing, filtering, sorting
4. **Panel Play** — full immersive playback
5. **Panel Edit** — full interaction, sends changes back to chat

On mobile, Panel surfaces open as full-screen overlays, not side-by-side.

```ts
interface RendererDefinition {
  id: string
  name: string
  contentType: string
  surfaces: SurfaceType[]
  chatPreview?: Component
  chatPlay?: Component
  panelPreview?: Component
  panelPlay?: Component
  panelEdit?: Component
  lazyDependencies?: () => Promise<any>
}
```

Chat surfaces must have zero lazy dependencies. Panel surfaces may lazy-load heavy libraries.

## Plugin System

All integrations are plugins. Plugin types: `ai-provider`, `media-source`, `messaging`, `storage`, `renderer`, `file-handler`, `crypto`, `search`, `auth`, `wallet`, `social-embed`, `mcp`, `media`.

```ts
interface AIUIPlugin {
  id: string
  name: string
  version: string
  type: PluginType
  description?: string
  init(context: PluginContext): Promise<void>
  destroy(): Promise<void>
  isAvailable(): Promise<boolean>
}
```

Sandboxing: Tier 1 (trusted built-in), Tier 2 (community — sandboxed iframes), Tier 3 (external processes). Community plugins get no direct DOM access.

## AI Provider Integration

```ts
interface AIProviderAdapter extends AIUIPlugin {
  type: 'ai-provider'
  chat(messages: Message[], options: ChatOptions): AsyncIterable<ChatChunk>
  models(): Promise<Model[]>
  supportsStreaming: boolean
  supportsVision: boolean
  supportsTools: boolean
}
```

Normalize tool calling across providers (OpenAI `tool_calls` vs Claude `tool_use`). Never include API keys in context injection.

## Security & Crypto

- E2E encryption: tweetnacl.js XSalsa20-Poly1305
- Local storage: Web Crypto API AES-256-GCM + PBKDF2 (100K+ iterations)
- API keys: encrypted at rest, never in localStorage, never logged, masked in UI (last 4 chars)
- No `eval()` or `innerHTML` with untrusted content
- Sanitize all user input against XSS
- HTTPS only, CSP headers in production
- Dev bypass: `VITE_DISABLE_CRYPTO=true` (never in production)

## Accessibility

WCAG AA minimum compliance:

- Color contrast: 4.5:1 normal text, 3:1 large/interactive
- Keyboard: all elements focusable via Tab, visible focus indicators, Escape closes modals
- Semantic HTML: use `<header>`, `<nav>`, `<main>`, `<article>`, `<aside>`, `<footer>` — not div soup
- ARIA: `aria-label` for icon buttons, `aria-live="polite"` for dynamic updates, `sr-only` for screen reader text
- Touch targets: min 44x44px with 8px gaps
- All images need `alt` attributes (decorative: `alt=""`)
- Respect `prefers-reduced-motion`

## Performance Budget

- **Initial load**: < 250KB gzipped
- Core bundle: Vue + Tailwind + Pinia + Router + chat UI (~150KB) + markdown + streaming (~50KB)
- Everything else: lazy-loaded on demand
- Virtual scrolling (TanStack Virtual) for chat lists
- Clean up listeners in `onUnmounted`, use `shallowRef` for large data
- Core Web Vitals: LCP < 2.5s, FID < 100ms, CLS < 0.1
- Preconnect to API hosts, debounce inputs (100ms)

## Mobile UX (iOS HIG-Informed)

Follows Apple iOS Human Interface Guidelines. See `.cursor/rules/15-mobile-ux.mdc` for full reference.

- **Typography**: Body 17px, Footnote 13px, Caption 11px minimum — never smaller than 11px
- **Touch targets**: min 44×44px tappable area, 8px gap between targets
- **Side margins**: 16px (`px-4`)
- **Primary actions**: Bottom thumb zone
- **Viewport**: `height: 100dvh` with `env(safe-area-inset-*)` for notched devices
- **Form inputs**: min 16px font (prevents iOS zoom), appropriate `inputmode`
- **Content panels**: Full-screen overlay or bottom sheet on mobile, never side-by-side
- **Transitions**: 150ms micro, 300ms standard, 400ms modal — ease-out entrances, ease-in exits
- **Sheets**: Bottom sheets with close button, don't rely solely on swipe-to-dismiss
- Support both portrait and landscape

## Environment & Dev Mode

Env vars must be prefixed `VITE_`. Secrets go in `.env.local` (gitignored). See `.env.example` for template.

Feature flags via `useFeatureFlags()`: `isDev`, `isTauri`, `isMobile`, `isCryptoEnabled`, `isMockData`

Dev mode enables: mock data, debug panel, verbose logging, disabled encryption, all renderers without lazy loading.

## Git Conventions

### Commit format

```
type(scope): description
```

**Types**: `feat`, `fix`, `refactor`, `style`, `docs`, `test`, `chore`, `perf`
**Scope**: package or area — `core`, `app`, `chat`, `renderer-film`, `plugin-x`

### Branches

`main` (production), `dev` (integration), `feat/description`, `fix/description`

### Rules

- One feature per PR
- All tests pass, TypeScript strict passes, no lint errors
- No force push to main/dev
- Never commit `.env.local`, secrets, or `node_modules`
- Squash merge features, tag releases `v1.0.0`

## Archipelago (Archy) Integration

AIUI runs inside an iframe in Archipelago's Chat mode. All communication with the host happens via `window.postMessage()` through a strict protocol.

### Architecture

```
AIUI (iframe)  ←→  postMessage  ←→  Archy ContextBroker  ←→  Node data
```

AIUI is **quarantined** — it never directly accesses Archy's APIs, stores, or node data. The Archy ContextBroker fetches and sanitizes data before passing it to AIUI.

### Protocol

Use `archyBridge.ts` (`src/services/archyBridge.ts`) for all Archy communication:

```ts
import { archyBridge } from '@/services/archyBridge'

// Request context (respects user permissions)
const apps = await archyBridge.requestContext('apps')
if (!apps.permitted) {
  // Show: "Enable 'Installed Apps' access in Archy Settings"
}

// Request an action
await archyBridge.requestAction('open-app', { appId: 'btcpay-server' })

// Listen for theme/permission updates
archyBridge.onPermissionsUpdate((categories) => { ... })
archyBridge.onThemeUpdate((theme) => { ... })
```

**Context categories** (user toggles each on/off in Archy Settings):
- `apps` — App names, status, health (no credentials)
- `system` — CPU, RAM, disk (no paths or IPs)
- `network` — Connection status, peer count (no IPs)
- `wallet` — Balance, channel count (no keys or seeds)
- `files` — File/folder names (no contents)

### Critical Rules

1. **NEVER** fetch Archy APIs directly — always use `archyBridge`
2. **NEVER** store or log raw user data from context responses
3. **NEVER** make HTTP requests to the host machine
4. Handle `permitted: false` gracefully — tell users what to enable
5. Send `ready` message on mount so Archy knows the iframe loaded
6. Build must output a static SPA servable from any base path
7. All AI provider keys are user-provided and stored locally in AIUI only

### Build & Deploy

AIUI deploys as a Podman container on the Archy node:
- Build: `pnpm build` → `packages/app/dist/`
- Container: nginx:alpine serving the dist
- Proxied at `/aiui/` via Archy's nginx
- Updates independently of Archy — new container image = new version
