---
name: add-tool
description: Add a new AI tool (function call) to the Claude proxy for the AI to use
allowed-tools: Bash(*), Read, Edit, Write, Glob, Grep
---

Add a new tool that the AI model can call via Claude's tool_use API. The user will describe what the tool should do (e.g., "search local files", "get app status", "browse media").

## Steps

1. **Read the proxy**: Read `packages/app/server/claude-proxy.ts` to understand the existing tool_use loop and `SEARCH_WEB_TOOL` definition.

2. **Define the tool**: Add a new tool definition following the Claude tool_use format:
   ```ts
   const NEW_TOOL = {
     name: 'tool_name',
     description: 'What this tool does...',
     input_schema: {
       type: 'object',
       properties: { ... },
       required: [...]
     }
   }
   ```

3. **Add handler**: In the tool_use loop (where `search_web` calls are handled), add a handler for the new tool name.

4. **Implement backend**: If the tool needs a new API endpoint (e.g., `/api/media/scan`), create a Vite plugin or add a route to the proxy.

5. **Update system prompt**: Add instructions in `useAI.ts` SYSTEM_PROMPT telling the AI when and how to use the new tool.

6. **Verify**: Run `pnpm typecheck` and test the proxy starts without errors.
