---
name: trace
description: End-to-end trace of a query through prompt, extraction, tabs, and rendering
allowed-tools: Bash(*), Read, Glob, Grep, Agent
---

Trace how a specific user query flows through the entire AIUI pipeline. The user will provide a sample query (e.g., "best nostr apps", "recommend some films", "bitcoin news").

## Trace each stage:

1. **Query classifiers**: Run the query through each classifier in `contentFiltering.ts`:
   - `isNewsQuery()`, `isMusicQuery()`, `isBookQuery()`, `isTVQuery()`, `isImageQuery()`, `isPlaceQuery()`, `isRecipeQuery()`, `isCodeQuery()`, `isNostrQuery()`, `isAppQuery()`, `isWebsitesQuery()`
   - Report which ones return true

2. **Preferred tab**: What does `preferredFirstTab()` return for this query?

3. **System prompt**: What would `buildSystemPrompt()` include? Read `useAI.ts` and trace all dynamic sections.

4. **Expected AI response**: Based on the system prompt instructions, what tags would the AI likely use? Construct a realistic sample response.

5. **Extraction**: Run the sample response through each extraction function and report what gets found:
   - `extractAllFilms()`, `extractAllSongs()`, `extractAllPodcasts()`, `extractAllBooks()`, `extractAllTVSeries()`, `extractAllImages()`, `extractAllPlaces()`, `extractApps()`, `extractCodeBlocks()`, `extractRecipes()`

6. **Tab filtering**: What tabs would `filterTabsByContext()` return? In what order?

7. **Rendering**: Which grid component would render? Trace through ContentPanel.vue and/or ContentGridView.vue.

8. **Report**: Complete flow diagram showing: Query -> Classifiers -> Prompt -> Expected Response -> Extraction -> Tabs -> Grid
