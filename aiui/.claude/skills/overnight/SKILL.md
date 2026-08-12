---
name: overnight
description: Commit, branch, and start the overnight automation loop
disable-model-invocation: true
allowed-tools: Bash(*), Read, Write, Edit, Glob, Grep
---

Prepare and launch the overnight automation loop. Do ALL steps in order, stopping on any failure:

1. Stage and commit all uncommitted changes: `git add -A && git commit -m "chore: pre-overnight snapshot"` (skip if working tree is clean)
2. Push current branch to origin
3. Get today's date as YYYY-MM-DD. Check if `overnight/$DATE` branch exists:
   - If yes: `git checkout overnight/$DATE`
   - If no: run `./loop/prepare.sh`
4. Verify `loop/plan.md` has unchecked tasks (`grep -c '^\- \[ \]' loop/plan.md`)
5. Commit plan files if modified: `git add loop/plan.md loop/prompt.md && git commit -m "chore: overnight plan $DATE"` (skip if clean)
6. Push: `git push -u origin overnight/$DATE`
7. Start the loop: run `caffeinate -i ./loop/loop.sh` with `run_in_background: true`
8. Report: branch name, number of tasks, and confirm the loop is running in background
