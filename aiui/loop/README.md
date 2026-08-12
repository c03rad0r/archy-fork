# Overnight Claude Automation

Run Claude Code autonomously while you're away. Combines sleep prevention, task-based execution, the Ralph Wiggum Technique (Stop hook blocks until plan is complete), and security hooks that restrict AI to project files and block destructive commands.

## Prerequisites

- **Claude Code CLI** ([claude.ai/code](https://claude.ai/code)) — installed at `~/.local/bin/claude` or in PATH
- **Hooks** — user-level hooks in `~/.claude/` (sleep, Ralph Wiggum)
- **jq** — for security hook scripts (`brew install jq`)

## Flow

### Pre-run (before 5–6pm)

1. **Commit and push** — Snap current work and back up to remote.
2. **Run prepare script** — Creates date-stamped branch and verifies clean state:

   ```bash
   ./loop/prepare.sh
   ```

3. **Edit plan** — Update `loop/plan.md` with evening scope and tasks (see template below).
4. **Commit plan** — Version the plan so you can revert if needed:

   ```bash
   git add loop/plan.md && git commit -m "chore: overnight plan $(date +%Y-%m-%d)"
   ```

5. **Push** (optional but recommended): `git push -u origin overnight/YYYY-MM-DD`

### Overnight

```bash
tmux new -s overnight
caffeinate -i ./loop/loop.sh
# Detach: Ctrl+B, then D
```

### Post-run (next morning)

1. `git status` and `git diff` to review changes.
2. Run `pnpm test && pnpm lint && pnpm typecheck`.
3. Merge branch or revert if needed.

## Quick Start

1. **Edit your plan** — Add tasks to `loop/plan.md` using the evening run format:

   ```markdown
   # Evening Run — YYYY-MM-DD

   ## Scope
   Add tests to chat components.

   ## Tasks
   - [ ] Add unit tests for useAI composable
   - [ ] Fix linter errors in packages/app
   ```

2. **Run overnight** — From project root:

   ```bash
   caffeinate -i ./loop/loop.sh
   ```

## How It Works

| Component | Purpose |
|-----------|---------|
| **UserPromptSubmit hook** | Starts `caffeinate` to prevent Mac sleep when you submit a prompt |
| **Stop hook** | Checks `plan.md` for unchecked tasks; blocks Claude from stopping until all are done (Ralph Wiggum) |
| **SessionEnd hook** | Kills `caffeinate` so Mac can sleep again |
| **PreToolUse (Bash)** | Blocks dangerous commands (rm -rf, git reset --hard, etc.) |
| **PreToolUse (Edit\|Write)** | Blocks edits outside project and to protected paths |
| **loop.sh** | Runs Claude with `--dangerously-skip-permissions` and feeds the prompt from `loop/prompt.md` |

## Security Model

Project-scoped hooks in `.claude/hooks/` restrict the AI during overnight runs:

### Bash guard (`block-risky-bash.sh`)

Blocks: `rm -rf`, `git reset --hard`, `git push --force`, `git clean -fd`, `chmod -R 777`, fork bombs, block device overwrites, `mkfs`, and path traversal with destructive commands.

### File edit guard (`protect-files.sh`)

Blocks Edit/Write when:

- Path is **outside project directory**
- Path contains **`.git/`**
- Path is **`.env`**, **`.env.local`**, **`.env.*.local`**
- Path is **`package-lock.json`** or **`pnpm-lock.yaml`**
- Path contains **`node_modules/`**

Read, Glob, and Grep remain unrestricted.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `CLAUDE_AUTONOMOUS` | `1` | Set to `1` to enable Ralph Wiggum (Stop hook checks plan). `0` disables. |
| `CLAUDE_PLAN_FILE` | `plan.md` | Plan file path (relative to project). |
| `ITERATION_COUNT` | `1` | Number of loop iterations (use >1 for multi-run without Ralph Wiggum). |
| `ITERATION_DELAY` | `600` | Seconds between iterations when `ITERATION_COUNT` > 1. |
| `PROMPT_FILE` | `loop/prompt.md` | Prompt content for Claude. |
| `LOG_FILE` | `loop/loop.log` | Log output (gitignored). |
| `RATE_LIMIT_WAIT` | `3600` | Seconds to wait when rate limited (default 1 hour). |
| `MAX_RATE_LIMIT_RETRIES` | `5` | Max rate limit retries before scheduling launchd job. |

## Rate Limit Handling

The loop script automatically detects rate limits (429, quota exceeded, etc.) and handles them:

1. **Inline retry** — On first rate limit hit, sleeps for `RATE_LIMIT_WAIT` seconds (default 1 hour) and retries.
2. **Escalating retries** — Retries up to `MAX_RATE_LIMIT_RETRIES` times with the same wait.
3. **launchd fallback** — After max retries, creates a self-cleaning launchd plist at `~/Library/LaunchAgents/com.aiui.overnight-retry.plist` that restarts the loop at the estimated reset time. The plist auto-removes after running.

This means you can walk away knowing the automation will survive rate limits overnight.

## Scheduling (Optional)

Install [claude-code-schedule](https://github.com/macalinao/claude-code-schedule) for time-based runs:

```bash
cargo install claude-code-schedule
ccschedule --time 05:30 --message "Review plan.md and complete next task"
```

## continuous-claude (Optional)

For full PR-based workflow (branches, PRs, CI):

```bash
# Install from https://github.com/AnandChowdhary/continuous-claude
continuous-claude -p "Work through loop/plan.md" -m 10 --max-duration 8h
```

## Remote Monitoring

- **Tmux + SSH**: Attach from another machine: `ssh host 'tmux attach -t overnight'`
- **Tailscale**: Use Tailscale for easy remote SSH when away from home network
- **Log tail**: `tail -f loop/loop.log` to watch progress

## Safety

- **Start small** — Test with 1–2 tasks before overnight runs
- **prepare.sh** — Run before starting; fails if working tree is dirty or branch exists
- **Git** — Loop does not auto-commit; you review and merge in the morning
- **`--dangerously-skip-permissions`** — Security hooks still run and block dangerous actions
- **Project-scoped hooks** — Only apply when Claude runs in AIUI; other projects unaffected
