---
name: fix-tab
description: Diagnose and fix a broken content panel tab (extraction, routing, rendering)
allowed-tools: Bash(*), Read, Edit, Glob, Grep, Agent
---

Diagnose and fix a broken content panel tab. The user will specify which tab (e.g., "app", "code", "image", "nostr").

## Diagnostic pipeline — check each layer:

1. **System prompt**: Does `useAI.ts` SYSTEM_PROMPT tell the AI to use the tag format for this content type? If not, add instructions.

2. **Tag regex**: Does `contentExtraction.ts` have a regex for this content type's tags? If not, add one.

3. **Extraction function**: Does `extractAll{Type}s()` or `extract{Type}s()` exist and work correctly? Test with sample input.

4. **Query classifier**: Do `is{Type}Query()` and/or `is{Type}LikeResponse()` exist in `contentFiltering.ts`?

5. **Tab filtering**: Is the tab included in `filterTabsByContext()` and `preferredFirstTab()`? Check the boolean flag is wired.

6. **useContentPanel.ts**: Is the extraction called in `updatePanelFromText()`? Is the panel ref populated? Is the tab added to `availableTabs`?

7. **ContentPanel.vue**: Is there a grid render block for `activeTab === '{tab}'`? Are imports present? Is the tab in TAB_LABELS?

8. **ContentGridView.vue**: Same checks for the wide desktop view.

9. **ChatPage.vue**: Are the panel data props passed to ContentGridView?

10. **Grid component**: Does the grid component exist and render correctly?

11. **Detail component**: Does the detail component exist?

Fix each broken layer. Run `pnpm typecheck` after all fixes.
