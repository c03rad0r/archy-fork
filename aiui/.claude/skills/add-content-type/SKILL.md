---
name: add-content-type
description: Scaffold a complete new content type (tag, extraction, grid, detail, prompt)
allowed-tools: Bash(*), Read, Edit, Write, Glob, Grep, Agent
---

Add a complete new content type to AIUI. The user will provide the content type name (e.g., "event", "product", "video").

Follow ALL steps — this is the full pipeline for a content type:

1. **Tag format**: Add a new `[[{type}_ext:Field1|Field2|...]]` regex to `packages/app/src/composables/contentExtraction.ts` alongside the existing ones (FILM_EXT_RE, SONG_EXT_RE, etc.)

2. **Type definition**: Add the TypeScript interface to `packages/core/src/types/content.ts` if it doesn't exist

3. **Extraction function**: Add `extractAll{Type}s(text, userQuery)` to `contentExtraction.ts` following the pattern of `extractAllFilms` or `extractAllBooks`

4. **Strip tags function**: Add `strip{Type}Tags()` and include it in `stripContentTags()`

5. **Query classifier**: Add `is{Type}Query()` and optionally `is{Type}LikeResponse()` to `contentFiltering.ts`

6. **ContentTab type**: Add the new tab name to the `ContentTab` union in `contentFiltering.ts`

7. **Tab filtering**: Update `filterTabsByContext()` and `preferredFirstTab()` in `contentFiltering.ts`

8. **Grid component**: Create `packages/app/src/components/content/{Type}Grid.vue` following the glass-morphism pattern of existing grids (BookGrid.vue is a good template)

9. **Detail component**: Create `packages/app/src/components/content/{Type}Detail.vue` following the pattern of BookDetail.vue

10. **Wire into ContentPanel.vue**: Add import, grid render block, detail render block, and panel state refs

11. **Wire into ContentGridView.vue**: Add import, props, and grid render block

12. **Wire into ChatPage.vue**: Pass the new panel data as props to ContentGridView

13. **Wire into useContentPanel.ts**: Add panel ref, selected ref, open/close functions, extraction call in `updatePanelFromText()`

14. **System prompt**: Add tag format instructions to the `SYSTEM_PROMPT` in `useAI.ts`

15. **Tab label**: Add to `TAB_LABELS` in `ContentPanel.vue`

16. **Verify**: Run `pnpm typecheck` and fix any errors

Report what was created and the tag format to use.
