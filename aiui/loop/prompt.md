You are working through an overnight automation plan for the AIUI app. Read these files first:

1. `loop/plan.md` — Your task checklist (mark items `- [x]` as you complete them)
2. `CLAUDE.md` — Project conventions, design system rules, and coding standards

## Project Context

AIUI is an AI content surface UI — a Vue 3 + TypeScript + Tailwind CSS app with chat, content panels (films, music, books, TV, places, news, images, podcasts), and a plugin system. It runs as a PWA and inside Archy (an iframe host).

Key directories:
- `packages/app/src/` — Main application source
- `packages/app/src/composables/` — Shared composition functions
- `packages/app/src/components/content/` — Content grid and detail components
- `packages/app/src/components/chat/` — Chat interface components
- `packages/app/src/styles/main.css` — Glass morphism design system
- `packages/core/src/` — Core library and types

## Working Process

For each task in `loop/plan.md`:

1. Find the first unchecked `- [ ]` item
2. Read the task description carefully — it tells you what to change and where
3. Read the relevant source files before making changes
4. Make the change following CLAUDE.md conventions
5. Run the TEST command specified in the task
6. Fix any errors from the test command before proceeding
7. Commit with conventional commit format: `type(scope): description`
8. Mark the task done: change `- [ ]` to `- [x]` in `loop/plan.md`
9. Move to the next unchecked task immediately

## Testing Gates

Every task specifies a TEST command. You MUST run it and pass before committing:
- `pnpm typecheck` — TypeScript strict mode compilation
- `pnpm lint` — ESLint checks
- `pnpm test` — Vitest unit tests
- Multiple commands joined with `&&` must ALL pass

If a test fails, fix the issue and re-run. Do not skip tests. Do not mark a task as done if tests fail.

## Coding Rules

- **Vue 3 Composition API only** — `<script setup lang="ts">`, never Options API
- **Glass morphism design** — use `.glass`, `.glass-card`, `.glass-button` from `main.css`
- **Dark theme** — `bg-white/5`, `text-white/80`, never `bg-gray-*` or plain `bg-white`
- **Text opacity scale** — `text-white/25` → `/40` → `/60` → `/70` → `/80` → `/90` → `/96`
- **Accent** — `text-accent` (`#F7931A`)
- **Touch targets** — minimum 44x44px for all interactive elements
- **Font minimums** — never smaller than 11px
- **No over-engineering** — only change what the task asks for
- **Keep existing patterns** — match the style of surrounding code

## Commit Format

```
type(scope): description

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`
Scope: `app`, `core`, or specific area like `chat`, `content`, `player`

## Rules

- Never skip a testing gate — if tests fail, fix them before moving on
- If a task is proving difficult, make at least 10 genuine attempts before moving on
- Always read source files before editing them
- Do not stop until all tasks are checked or you are rate limited
- Commit after each completed task
- For research tasks (Phase 8), create the docs directory if needed: `mkdir -p docs/research`
- For test tasks (Phase 4), create the test directory if needed: `mkdir -p packages/app/src/composables/__tests__`
