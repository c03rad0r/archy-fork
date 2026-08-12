# Claude Code Overnight Automation

Run Claude Code headlessly overnight to execute a full task checklist — with rate-limit resilience, macOS sleep prevention, and a stop hook that prevents Claude from quitting until every task is done.

## How It Works

```
loop.sh (orchestrator)
  |
  +--> Reads plan.md for unchecked [ ] tasks
  +--> Pipes prompt.md into `claude -p` (headless mode)
  |      |
  |      +--> Claude reads your plan, specs, and project rules
  |      +--> Implements tasks one by one
  |      +--> Runs typecheck/lint/test after each
  |      +--> Commits, marks [x], moves to next
  |      |
  |      +--> Claude tries to stop
  |             |
  |             +--> Stop Hook intercepts
  |                    +--> Checks plan.md for remaining [ ] tasks
  |                    +--> If incomplete: BLOCKS the stop (Claude continues)
  |                    +--> If all done: allows stop
  |
  +--> Detects rate limits in output
  |      +--> Sleeps 1 hour, retries (up to 5x)
  |      +--> After 5 retries: schedules macOS launchd job to resume later
  |
  +--> Loops N iterations (default 10)
  +--> Exits when all tasks checked or iterations exhausted
```

### The "Ralph Wiggum" Stop Hook

The secret sauce. Claude Code supports a `Stop` hook — a shell script that runs every time Claude tries to end its session. By returning `{"decision":"block"}`, the hook **prevents Claude from stopping**. Combined with `--dangerously-skip-permissions`, Claude becomes a fully autonomous task executor that won't quit until the job is done.

### Sleep Prevention

On macOS, `caffeinate -i` prevents idle sleep during long runs. A hook starts it when Claude begins and kills it when Claude finishes.

### Rate Limit Resilience

If Claude hits API rate limits:
1. **Inline retry**: Sleep 1 hour, then retry the same iteration
2. **Scheduled retry**: After 5 failed retries, create a macOS `launchd` plist that auto-runs the loop later
3. The plist self-destructs after executing

## Prerequisites

- **Claude Code CLI** (`claude` command available in PATH)
  - Install: https://docs.anthropic.com/en/docs/claude-code
  - Must be logged in: run `claude login` first
- **macOS** (for `caffeinate` and `launchd` — see Linux notes below)
- **Git** (the script commits after each task)
- A project with `package.json` or similar build tooling

## Quick Start

```bash
# 1. Clone or copy this folder into your project
cp -r "For Others/templates" ~/my-project/loop

# 2. Run the setup script (creates hooks, updates settings)
cd ~/my-project
bash "path/to/For Others/setup.sh"

# 3. Edit your task list
vim loop/plan.md

# 4. Edit your prompt (project-specific rules)
vim loop/prompt.md

# 5. Start the overnight run
./loop/loop.sh
```

Or just run the setup script — it walks you through everything:

```bash
bash "For Others/setup.sh"
```

## File Structure

After setup, your project will have:

```
your-project/
  loop/
    loop.sh          # Main orchestrator (run this)
    prompt.md        # Instructions piped to Claude each iteration
    plan.md          # Task checklist ([ ] = todo, [x] = done)
    loop.log         # Full output log (auto-created)

~/.claude/
  hooks/
    prevent-sleep.sh           # Starts caffeinate on session start
    stop-hook-autonomous.sh    # Blocks stop until tasks complete
    allow-sleep.sh             # Kills caffeinate on session end
  settings.json                # Hook registrations (auto-updated by setup)
```

## Configuration

All config is via environment variables (set before running `loop.sh` or export in your shell):

| Variable | Default | Description |
|----------|---------|-------------|
| `CLAUDE_AUTONOMOUS` | `1` | Set to `0` to disable the stop hook (Claude can quit freely) |
| `ITERATION_COUNT` | `10` | Max loop iterations |
| `ITERATION_DELAY` | `30` | Seconds to pause between iterations |
| `RATE_LIMIT_WAIT` | `3600` | Seconds to sleep when rate limited (1 hour) |
| `MAX_RATE_LIMIT_RETRIES` | `5` | Retries before scheduling launchd |
| `CLAUDE_BIN` | `claude` | Path to Claude CLI binary |
| `PROMPT_FILE` | `loop/prompt.md` | Path to prompt file |
| `LOG_FILE` | `loop/loop.log` | Path to log file |

### Examples

```bash
# Quick test run (2 iterations, 10s delay, no stop hook)
CLAUDE_AUTONOMOUS=0 ITERATION_COUNT=2 ITERATION_DELAY=10 ./loop/loop.sh

# Full overnight run (20 iterations, 1 min between)
ITERATION_COUNT=20 ITERATION_DELAY=60 ./loop/loop.sh

# Use a custom prompt
PROMPT_FILE=my-prompt.md ./loop/loop.sh
```

## Writing Your Plan

`loop/plan.md` is a markdown checklist. Each line starting with `- [ ]` is a pending task:

```markdown
## Phase 1: Core Features
- [ ] **1.1** — Add user authentication (JWT + refresh tokens)
- [ ] **1.2** — Create user profile page with avatar upload
- [ ] **1.3** — Add settings page with theme toggle

## Phase 2: API
- [ ] **2.1** — REST endpoints for CRUD operations
- [ ] **2.2** — WebSocket support for real-time updates

## Final
- [ ] **FINAL** — Run full test suite, fix any failures, tag release
```

Claude will:
1. Find the first `- [ ]` line
2. Read the spec from your prompt or a separate spec file
3. Implement it
4. Mark it `- [x]`
5. Move to the next

### Tips for good plans

- **Be specific**: "Add JWT auth with refresh tokens, store in httpOnly cookies" > "Add auth"
- **Order matters**: Put foundational tasks first (types, utils, config) before features that depend on them
- **Include testing gates**: "Run `pnpm test` and fix failures" as part of each task
- **Keep tasks small**: 30-60 minutes of work each. Large tasks lead to context window exhaustion
- **Add a FINAL task**: A catchall that runs the full test suite

## Writing Your Prompt

`loop/prompt.md` is what Claude reads at the start of every iteration. Include:

1. **What files to read** (your plan, specs, project conventions)
2. **Project-specific rules** (coding style, frameworks, constraints)
3. **Per-task workflow** (implement → test → commit → mark done)
4. **Hard rules** (what to never do, minimum effort before skipping)

See `templates/prompt.md` for a starting template.

## Operating the Loop

### Starting

```bash
# Foreground (see output live)
./loop/loop.sh

# Background with logging
nohup ./loop/loop.sh > /dev/null 2>&1 &

# With caffeinate (prevents sleep even if hooks fail)
caffeinate -i ./loop/loop.sh
```

### Monitoring

```bash
# Watch the log live
tail -f loop/loop.log

# Check progress
grep -c '\- \[x\]' loop/plan.md  # completed
grep -c '\- \[ \]' loop/plan.md  # remaining

# Check git commits
git log --oneline -20
```

### Stopping

- **Let it finish**: The loop stops automatically when all tasks are checked
- **Kill it**: `Ctrl+C` or `kill %1` — Claude's current task will be interrupted but committed work is preserved
- **Disable stop hook**: Set `CLAUDE_AUTONOMOUS=0` in the environment before the next iteration

### Resuming

Just run `./loop/loop.sh` again. It reads `plan.md` fresh each iteration, so it picks up where it left off (skipping `[x]` tasks).

## Customizing the Prompt

The prompt template has `{{PLACEHOLDER}}` markers. Replace them with your project's specifics:

| Placeholder | What to put |
|-------------|-------------|
| `{{SPEC_FILE}}` | Path to your detailed spec (e.g., `SPEC.md`, `docs/plan.md`) |
| `{{PROJECT_RULES_FILE}}` | Path to your coding conventions file |
| `{{PROJECT_RULES}}` | Inline coding rules (style, frameworks, constraints) |

## Troubleshooting

### Claude exits immediately
- Make sure `claude login` has been run
- Check that `claude -p "hello"` works in your terminal
- Verify `~/.claude/hooks/stop-hook-autonomous.sh` exists and is executable

### Rate limit loop
- Default wait is 1 hour. Increase `RATE_LIMIT_WAIT` if your limits are longer
- Check `loop.log` for the specific rate limit message
- Claude Max subscriptions have higher limits than API keys

### Mac goes to sleep
- Run `caffeinate -i ./loop/loop.sh` as a belt-and-suspenders approach
- Check that `~/.claude/hooks/prevent-sleep.sh` is executable: `chmod +x ~/.claude/hooks/prevent-sleep.sh`

### Tasks not getting marked complete
- Ensure your plan uses exact format: `- [ ]` (dash, space, brackets, space)
- The stop hook matches `^\s*[-*]?\s*\[\s*\]` — standard markdown checkboxes

### Stop hook not working
- Verify `CLAUDE_AUTONOMOUS=1` is set: `echo $CLAUDE_AUTONOMOUS`
- Check hook is registered in `~/.claude/settings.json`
- Test the hook manually: `echo '{}' | bash ~/.claude/hooks/stop-hook-autonomous.sh`

## Linux Notes

The system is macOS-focused but works on Linux with minor changes:

- **Sleep prevention**: Replace `caffeinate` with `systemd-inhibit --what=idle --who=claude-loop --why="Overnight automation" sleep infinity &` or simply disable sleep via `systemctl mask sleep.target`
- **Scheduled retry**: Replace the launchd plist section in `loop.sh` with a `systemd-run --on-calendar` or `at` command
- **Hooks work identically** — they're plain bash scripts

## Security Notes

- `--dangerously-skip-permissions` gives Claude **full system access** within the project. Only run on trusted codebases.
- The loop runs as your user — Claude can read/write anything you can
- API keys in `.env.local` are accessible to Claude during the session
- Review commits after an overnight run before pushing to production
- Consider running in a VM or container for additional isolation

## License

MIT. Use it however you want.
