---
name: new-grid
description: Generate a content grid component following AIUI glass-morphism patterns
allowed-tools: Read, Write, Edit, Glob, Grep
---

Create a new content grid component at `packages/app/src/components/content/{Name}Grid.vue`.

## Requirements

1. **Read a reference**: Read `BookGrid.vue` or `PlaceGrid.vue` as a template — they show the standard pattern.

2. **Follow conventions**:
   - `<script setup lang="ts">` with props and emits
   - Glass morphism styling (bg-white/5, rounded-xl, hover:bg-white/10)
   - Dark/light mode support via `useTheme()`
   - Search input at top (if the content type has enough items)
   - Grid of cards with image fallback, title, subtitle, metadata
   - Touch targets min 44x44px
   - Empty state message when no items match
   - Custom scrollbar class

3. **Props**: Accept array of items + title string
4. **Emits**: `select-{type}` event when a card is clicked
5. **Responsive**: Works on mobile (full width) and desktop (sidebar width)

The user will specify the content type and its fields.
