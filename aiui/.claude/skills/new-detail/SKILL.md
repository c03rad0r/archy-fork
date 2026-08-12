---
name: new-detail
description: Generate a detail view component following AIUI glass-morphism patterns
allowed-tools: Read, Write, Edit, Glob, Grep
---

Create a new detail view component at `packages/app/src/components/content/{Name}Detail.vue`.

## Requirements

1. **Read a reference**: Read `BookDetail.vue` or `PlaceDetail.vue` as a template.

2. **Follow conventions**:
   - `<script setup lang="ts">` with single item prop
   - Back button at top (emits 'back' event)
   - Hero image/banner area with gradient overlay and fallback
   - Title, subtitle, and metadata section
   - Description/long text body with proper typography
   - Action buttons (external links, share, etc.) with glass-button styling
   - Dark/light mode via `useTheme()`
   - Smooth scroll, overflow-y-auto

3. **Props**: Accept single item of the content type
4. **Emits**: `back` event for navigation
5. **Responsive**: Full height, works in sidebar and mobile overlay

The user will specify the content type and which fields to display.
