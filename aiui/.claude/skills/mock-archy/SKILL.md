---
name: mock-archy
description: Enable/configure mock Archy data for standalone dev testing
allowed-tools: Bash(*), Read, Edit, Glob, Grep
---

Set up or modify mock Archy data for testing the Archipelago integration without a real Archy host.

## How it works

Mock data is in `packages/app/src/mocks/archy.ts`. When enabled, `useArchy.ts` loads this data instead of waiting for the Archy bridge.

## Enable mock mode

Two ways:
1. Add `VITE_MOCK_ARCHY=true` to `.env.local`
2. Add `?mockArchy` to the URL: `http://localhost:5173/?mockArchy`

## Customization

The user may ask to:
- Add/remove mock apps from the installed list
- Change wallet balance or channel count
- Add/modify files in the mock file list
- Change system info or network status
- Test specific scenarios (e.g., "node is syncing", "wallet offline", "no files")

Edit `packages/app/src/mocks/archy.ts` accordingly.

## Verify

After changes, check that `buildArchyContext()` in `useArchy.ts` produces the expected system prompt section by reading the function and tracing the mock data through it.
