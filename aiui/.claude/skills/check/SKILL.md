---
name: check
description: Run all quality checks (typecheck, lint, test) and auto-fix errors
allowed-tools: Bash(*), Read, Edit, Glob, Grep, Agent
---

Run all quality checks for the AIUI project and fix any issues found. Execute in order:

1. **TypeScript**: Run `pnpm typecheck`. If errors found, read the failing files and fix the type errors.
2. **Lint**: Run `pnpm lint`. If fixable errors, run `pnpm lint --fix` first, then fix remaining manually.
3. **Tests**: Run `pnpm --filter @aiui/app test -- --run`. For each failure:
   - Read the test file and the source file it tests
   - Determine if the test is wrong (outdated assertion) or the source has a bug
   - Fix whichever is incorrect
4. Report a summary: pass/fail counts, what was fixed.

Important: Do NOT change test expectations just to make them pass — understand WHY they fail first.
