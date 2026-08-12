---
name: test-prompts
description: Test AI prompt quality by simulating queries and checking extraction results
allowed-tools: Bash(*), Read, Edit, Glob, Grep, Agent
---

Test the AIUI AI prompt and content extraction pipeline end-to-end. This skill does NOT call the actual AI — it uses the seed prompts and extraction functions directly.

## Steps

1. **Read seed prompts**: Read `packages/app/src/__tests__/fixtures/seedPrompts.ts` to get all test cases.

2. **Run extraction tests**: For each seed prompt, run the test via `pnpm --filter @aiui/app test -- --run -t "seed"` and report results.

3. **Test edge cases**: Create and test these additional scenarios by calling extraction functions in a test:
   - Mixed content response (films + songs + books in one response)
   - App recommendation response (should trigger app tab)
   - News query with web search results
   - Place/restaurant recommendations
   - Code response with 3+ code blocks
   - Nostr-related query
   - Empty/minimal response

4. **Verify tab routing**: For each scenario, check that `filterTabsByContext()` returns the expected tabs in the expected order.

5. **Report**: Summary of what works, what's broken, and what's missing. Include specific test cases that fail.
