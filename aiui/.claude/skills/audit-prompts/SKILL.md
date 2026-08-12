---
name: audit-prompts
description: Deep audit of AI system prompts — find gaps, test extraction coverage, verify tag formats
allowed-tools: Bash(*), Read, Glob, Grep, Agent
---

Perform a comprehensive audit of AIUI's AI prompt system. Do NOT make changes — report findings only.

## Steps

1. **Read the full system prompt**: Read `packages/app/src/composables/useAI.ts` and reconstruct the complete system prompt including all dynamic sections (persona, Wavlake, memory, web search, Archy context, code context).

2. **Catalog all tag formats**: List every `[[type:...]]` and `[[type_ext:...]]` format defined in the prompt. Cross-reference with regexes in `contentExtraction.ts`.

3. **Check for gaps**: For each content type in `ContentTab` (contentFiltering.ts), verify:
   - Is there a tag format in the system prompt?
   - Is there a matching extraction regex?
   - Is there a query classifier?
   - Is there a grid + detail component?
   - Is the tab wired in ContentPanel.vue and ContentGridView.vue?

4. **Test extraction coverage**: Read the seed prompts in `src/__tests__/fixtures/seedPrompts.ts`. For each seed, verify:
   - Does the extraction function find the expected number of items?
   - Are there edge cases that would break extraction?

5. **Analyze prompt quality**: Check for:
   - Conflicting instructions
   - Missing edge case handling (e.g., "what if the AI can't find a match?")
   - Overly vague instructions
   - Missing content types that should have tag formats

6. **Check proxy tools**: Read `server/claude-proxy.ts` and verify tool definitions match what the prompt claims.

7. **Report**: Create a structured summary with:
   - Content type coverage matrix (tag/extraction/grid/detail/prompt)
   - Identified gaps and inconsistencies
   - Priority recommendations
