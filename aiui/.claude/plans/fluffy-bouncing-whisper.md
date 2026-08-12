# Plan: Code Mode UI — Orange Input, Design System Context, File Browser Context

## Context
The user wants three connected features that enhance the coding experience in AIUI:
1. Visual indication when in code mode (orange input container, "Code" label)
2. Ability to select design system items as coding context
3. Ability to select files from file browser as coding context

After this, the user wants to circle back and create a flawless version of content extraction/tab surfacing.

## Changes

### 1. Orange Code Mode Input Container
**Files**: `ChatWindow.vue`, `ChatInput.vue`

**ChatWindow.vue** (line 106-115):
- Pass `activeTab` to ChatInput as a prop: `:active-tab="activeTab"`
- Change placeholder logic: `activeTab === 'code' ? 'Code...' : isStreaming ? 'Waiting for response...' : 'Message AIUI...'`

**ChatInput.vue**:
- Add `activeTab` prop (optional string, default `''`)
- Conditionally style the container div (line 79-81):
  - When `activeTab === 'code'`: use `bg-accent/15 border border-accent/25 backdrop-blur-xl` instead of `path-glass-bubble`
  - Keep the `rounded-2xl px-4 py-3 flex items-end gap-2 transition-all duration-300` classes
- Conditionally style the send button orange when in code mode

### 2. Design System Item Selection for Coding Context
**Files**: `useCodeContext.ts`, `DesignSystemGrid.vue`

**useCodeContext.ts**:
- Add `selectedDesignTokens: ref<string[]>([])` to module state (stores item IDs)
- Add `toggleDesignToken(id)` — adds/removes from selection array
- Add `clearDesignTokens()` — clears selection
- Add `isDesignTokenSelected(id)` — checks if item is in selection
- Clear on `exitCodeMode()`
- Export all new state/actions

**DesignSystemGrid.vue**:
- Import `useCodeContext`
- When `codeMode` is true, show a selection indicator (accent ring + checkmark) on items
- `selectItem` should call `toggleDesignToken(item.id)` when in code mode (instead of `openDesignSystemItem`)
- When NOT in code mode, keep existing behavior (open detail view)
- Selected items get `ring-2 ring-accent/50 bg-accent/10` styling

### 3. File Browser Selection for Coding Context
**Files**: `useCodeContext.ts`, `ProjectGrid.vue`

**useCodeContext.ts**:
- Add `selectedFiles: ref<string[]>([])` — paths of files selected for context
- Add `toggleFileSelection(path)` — adds/removes from selection
- Add `clearFileSelection()` — clears all
- Add `isFileSelected(path)` — checks if file in selection
- Clear on `exitCodeMode()`
- Export new state/actions

**ProjectGrid.vue**:
- Import `useCodeContext`
- When `codeMode` is true, file clicks toggle selection instead of (or in addition to) opening
- Show visual selection state (accent highlight/checkmark) on selected files in FileTreeNode

## Files to Modify
1. `packages/app/src/components/chat/ChatWindow.vue` — pass activeTab prop
2. `packages/app/src/components/chat/ChatInput.vue` — conditional orange styling + "Code" placeholder
3. `packages/app/src/composables/useCodeContext.ts` — add design token + file selection state
4. `packages/app/src/components/content/DesignSystemGrid.vue` — toggle selection in code mode
5. `packages/app/src/components/content/ProjectGrid.vue` — toggle file selection in code mode

## Verification
1. `pnpm typecheck` — no type errors
2. `pnpm lint` — no new lint errors
3. Manual: `/code` command → input turns orange with "Code..." placeholder
4. Manual: In code mode, design system tab → clicking items toggles selection (accent ring)
5. Manual: In code mode, file browser → clicking files toggles selection
6. Manual: Exiting code mode clears all selections
