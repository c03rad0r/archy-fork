#!/usr/bin/env bash
# ============================================================================
# Claude Code Overnight Automation — One-File Setup
# ============================================================================
# Run from your project root:
#   bash setup.sh
#
# This single script creates everything:
#   loop/loop.sh      — main orchestrator
#   loop/prompt.md    — template prompt for Claude
#   loop/plan.md      — your task checklist
#   ~/.claude/hooks/  — sleep prevention + autonomous stop hook
#   ~/.claude/settings.json — hook registrations
# ============================================================================
set -euo pipefail

BOLD='\033[1m'
DIM='\033[2m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
RED='\033[0;31m'
NC='\033[0m'

ok()   { echo -e "  ${GREEN}+${NC} $1"; }
warn() { echo -e "  ${YELLOW}!${NC} $1"; }
err()  { echo -e "  ${RED}x${NC} $1"; }
info() { echo -e "  ${DIM}$1${NC}"; }

echo ""
echo -e "${BOLD}  Claude Code Overnight Automation${NC}"
echo -e "  ${DIM}────────────────────────────────────${NC}"
echo ""

# ── Prerequisites ────────────────────────────────────────────────────────────
echo -e "  ${BOLD}Checking prerequisites...${NC}"
echo ""

MISSING=0

if command -v claude &>/dev/null; then
  ok "Claude CLI: $(which claude)"
else
  err "Claude CLI not found. Install: https://docs.anthropic.com/en/docs/claude-code"
  MISSING=1
fi

if command -v git &>/dev/null; then
  ok "Git: $(which git)"
else
  err "Git not found."
  MISSING=1
fi

if [[ "$(uname)" == "Darwin" ]]; then
  ok "macOS (caffeinate + launchd available)"
else
  warn "Not macOS — sleep hooks need Linux equivalents (see README)"
fi

if git rev-parse --is-inside-work-tree &>/dev/null; then
  PROJECT_DIR="$(git rev-parse --show-toplevel)"
  ok "Project: $PROJECT_DIR"
else
  PROJECT_DIR="$(pwd)"
  warn "Not a git repo — using: $PROJECT_DIR"
fi

[[ "$MISSING" -eq 1 ]] && { echo ""; err "Fix the above and re-run."; exit 1; }

echo ""

# ── Create loop/ directory ───────────────────────────────────────────────────
echo -e "  ${BOLD}Creating loop files...${NC}"
echo ""

LOOP_DIR="$PROJECT_DIR/loop"
mkdir -p "$LOOP_DIR"

# ── loop.sh (embedded) ──────────────────────────────────────────────────────
if [[ -f "$LOOP_DIR/loop.sh" ]]; then
  warn "loop/loop.sh exists — skipping"
else
  cat > "$LOOP_DIR/loop.sh" << 'LOOPEOF'
#!/usr/bin/env sh
# Claude Code Overnight Automation — Loop Script
# Usage: ./loop/loop.sh
# Config via env vars: ITERATION_COUNT, ITERATION_DELAY, CLAUDE_AUTONOMOUS, etc.
set -u

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/.." && pwd)}"
PROMPT_FILE="${PROMPT_FILE:-$PROJECT_DIR/loop/prompt.md}"
LOG_FILE="${LOG_FILE:-$PROJECT_DIR/loop/loop.log}"
ITERATION_COUNT="${ITERATION_COUNT:-10}"
ITERATION_DELAY="${ITERATION_DELAY:-30}"
CLAUDE_BIN="${CLAUDE_BIN:-claude}"
RATE_LIMIT_WAIT="${RATE_LIMIT_WAIT:-3600}"
MAX_RATE_LIMIT_RETRIES="${MAX_RATE_LIMIT_RETRIES:-5}"
CLAUDE_EXIT=0

cd "$PROJECT_DIR"

log() { echo "$1" | tee -a "$LOG_FILE"; }
banner() {
  log ""; log "════════════════════════════════════════════════════════════════"
  log "  $1"; log "  $(date '+%Y-%m-%d %H:%M:%S')"
  log "════════════════════════════════════════════════════════════════"; log ""
}
section() { log ""; log "────────────────────────────────────────"; log "  $1"; log "────────────────────────────────────────"; log ""; }

plan_has_tasks() { grep -q '^\- \[ \]' "$PROJECT_DIR/loop/plan.md" 2>/dev/null; }
remaining_tasks() { grep -c '^\- \[ \]' "$PROJECT_DIR/loop/plan.md" 2>/dev/null || echo "0"; }
next_task() { grep -m1 '^\- \[ \]' "$PROJECT_DIR/loop/plan.md" 2>/dev/null | sed 's/^- \[ \] //' || echo "(none)"; }

check_rate_limit() {
  [ "${CLAUDE_EXIT:-0}" -eq 0 ] && return 1
  tail -50 "$LOG_FILE" 2>/dev/null | grep -v "^Rate limit" | grep -v "^Sleeping" | grep -v "^═" | grep -v "^─" \
    | grep -qi -e "rate.limit" -e "too.many.requests" -e "429" -e "quota.exceeded" -e "usage.limit" -e "limit.reached" 2>/dev/null
}

banner "OVERNIGHT AUTOMATION STARTED"
log "  Project:    $PROJECT_DIR"
log "  Prompt:     $PROMPT_FILE"
log "  Autonomous: ${CLAUDE_AUTONOMOUS:-0}"
log "  Iterations: $ITERATION_COUNT (${ITERATION_DELAY}s delay)"
log "  Rate limit: wait ${RATE_LIMIT_WAIT}s, retry ${MAX_RATE_LIMIT_RETRIES}x"
log "  Tasks left: $(remaining_tasks)"
log "  Next task:  $(next_task)"
log ""

i=1; rate_limit_retries=0
while [ "$i" -le "$ITERATION_COUNT" ]; do
  if ! plan_has_tasks; then
    banner "ALL TASKS COMPLETE"; log "  No remaining [ ] tasks. Stopping."; break
  fi

  section "ITERATION $i/$ITERATION_COUNT"
  log "  Remaining: $(remaining_tasks)"; log "  Next:      $(next_task)"; log ""

  export CLAUDE_PROJECT_DIR="$PROJECT_DIR"
  export CLAUDE_AUTONOMOUS="${CLAUDE_AUTONOMOUS:-1}"

  if [ -f "$PROMPT_FILE" ]; then
    log "  Starting Claude..."; log ""
    "$CLAUDE_BIN" -p --dangerously-skip-permissions < "$PROMPT_FILE" 2>&1 | tee -a "$LOG_FILE"
    CLAUDE_EXIT=$?; log ""; log "  Exit code: $CLAUDE_EXIT"
  else
    log "  ERROR: $PROMPT_FILE not found"; exit 1
  fi

  if check_rate_limit; then
    rate_limit_retries=$((rate_limit_retries + 1))
    if [ "$rate_limit_retries" -ge "$MAX_RATE_LIMIT_RETRIES" ]; then
      section "RATE LIMITED — SCHEDULING RETRY"
      PLIST_LABEL="com.claude-loop.overnight-retry"
      PLIST_PATH="$HOME/Library/LaunchAgents/${PLIST_LABEL}.plist"
      RETRY_TIME=$(date -v+${RATE_LIMIT_WAIT}S '+%H:%M' 2>/dev/null || date -d "+${RATE_LIMIT_WAIT} seconds" '+%H:%M')
      RETRY_HOUR=$(echo "$RETRY_TIME" | cut -d: -f1); RETRY_MIN=$(echo "$RETRY_TIME" | cut -d: -f2)
      cat > "$PLIST_PATH" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>${PLIST_LABEL}</string>
  <key>ProgramArguments</key><array>
    <string>/bin/sh</string><string>-c</string>
    <string>cd ${PROJECT_DIR} && caffeinate -i ./loop/loop.sh >> ${LOG_FILE} 2>&1; launchctl unload ${PLIST_PATH}; rm -f ${PLIST_PATH}</string>
  </array>
  <key>StartCalendarInterval</key><dict><key>Hour</key><integer>${RETRY_HOUR}</integer><key>Minute</key><integer>${RETRY_MIN}</integer></dict>
  <key>EnvironmentVariables</key><dict>
    <key>CLAUDE_AUTONOMOUS</key><string>1</string>
    <key>CLAUDE_PROJECT_DIR</key><string>${PROJECT_DIR}</string>
    <key>PATH</key><string>/usr/local/bin:/usr/bin:/bin:$HOME/.local/bin</string>
  </dict>
  <key>StandardOutPath</key><string>${LOG_FILE}</string>
  <key>StandardErrorPath</key><string>${LOG_FILE}</string>
</dict></plist>
PLIST
      launchctl load "$PLIST_PATH" 2>/dev/null || true
      log "  Scheduled retry at ~${RETRY_TIME}"; exit 0
    fi
    section "RATE LIMITED — WAITING"
    log "  Attempt $rate_limit_retries/$MAX_RATE_LIMIT_RETRIES"; log "  Sleeping ${RATE_LIMIT_WAIT}s..."
    sleep "$RATE_LIMIT_WAIT"
    if ! plan_has_tasks; then banner "ALL TASKS COMPLETE"; break; fi
    log "  Retrying..."; continue
  fi

  rate_limit_retries=0
  section "ITERATION $i COMPLETE"
  log "  Remaining: $(remaining_tasks)"; log "  Next:      $(next_task)"
  i=$((i + 1))
  if [ "$i" -le "$ITERATION_COUNT" ] && [ "$ITERATION_DELAY" -gt 0 ]; then
    log "  Pausing ${ITERATION_DELAY}s..."; sleep "$ITERATION_DELAY"
  fi
done

banner "LOOP FINISHED"
log "  Completed $((i - 1)) iterations"; log "  Remaining: $(remaining_tasks)"; log ""
LOOPEOF
  chmod +x "$LOOP_DIR/loop.sh"
  ok "Created loop/loop.sh"
fi

# ── prompt.md and plan.md are created later after interactive input ────────

echo ""

# ── Install hooks ────────────────────────────────────────────────────────────
echo -e "  ${BOLD}Installing hooks...${NC}"
echo ""

HOOKS_DIR="$HOME/.claude/hooks"
mkdir -p "$HOOKS_DIR"

# prevent-sleep.sh
if [[ -f "$HOOKS_DIR/prevent-sleep.sh" ]]; then
  warn "prevent-sleep.sh exists — skipping"
else
  cat > "$HOOKS_DIR/prevent-sleep.sh" << 'HOOKEOF'
#!/usr/bin/env bash
set -euo pipefail
PID_FILE="${CLAUDE_CAFFEINATE_PID:-$HOME/.claude/caffeinate.pid}"
if [[ -f "$PID_FILE" ]]; then
  old_pid=$(cat "$PID_FILE")
  kill -0 "$old_pid" 2>/dev/null && kill "$old_pid" 2>/dev/null || true
  rm -f "$PID_FILE"
fi
caffeinate -i &
echo $! > "$PID_FILE"
exit 0
HOOKEOF
  chmod +x "$HOOKS_DIR/prevent-sleep.sh"
  ok "Installed ~/.claude/hooks/prevent-sleep.sh"
fi

# stop-hook-autonomous.sh
if [[ -f "$HOOKS_DIR/stop-hook-autonomous.sh" ]]; then
  warn "stop-hook-autonomous.sh exists — skipping"
else
  cat > "$HOOKS_DIR/stop-hook-autonomous.sh" << 'HOOKEOF'
#!/usr/bin/env bash
# "Ralph Wiggum" — blocks Claude from stopping until all plan tasks are done.
# Requires CLAUDE_AUTONOMOUS=1 to activate.
set -euo pipefail
BASE="${CLAUDE_PROJECT_DIR:-}"
if [[ -z "$BASE" ]] && command -v jq &>/dev/null; then
  BASE=$(jq -r '.cwd // empty' 2>/dev/null || true)
fi
[[ -z "$BASE" ]] && BASE="$(pwd)"
PLAN_FILE="${CLAUDE_PLAN_FILE:-plan.md}"
ALT_FILES="loop/plan.md todo.md loop/todo.md"
AUTO_SLEEP_HOOK="$HOME/.claude/hooks/allow-sleep.sh"
plan=""
for f in "$PLAN_FILE" $ALT_FILES; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == /* ]]; then path="$f"; else path="$BASE/$f"; fi
  if [[ -f "$path" ]]; then plan="$path"; break; fi
done
if [[ -z "${CLAUDE_AUTONOMOUS:-}" ]] || [[ "$CLAUDE_AUTONOMOUS" == "0" ]]; then
  [[ -x "$AUTO_SLEEP_HOOK" ]] && "$AUTO_SLEEP_HOOK" || true; exit 0
fi
if [[ -z "$plan" ]] || [[ ! -f "$plan" ]]; then
  [[ -x "$AUTO_SLEEP_HOOK" ]] && "$AUTO_SLEEP_HOOK" || true; exit 0
fi
incomplete=$(grep -c -E '^\s*[-*]?\s*\[\s*\]' "$plan" 2>/dev/null || echo 0)
if [[ "${incomplete:-0}" -gt 0 ]]; then
  echo '{"decision":"block","reason":"Plan has '"$incomplete"' incomplete task(s). Continue with the next item."}'
  exit 0
fi
[[ -x "$AUTO_SLEEP_HOOK" ]] && "$AUTO_SLEEP_HOOK" || true
exit 0
HOOKEOF
  chmod +x "$HOOKS_DIR/stop-hook-autonomous.sh"
  ok "Installed ~/.claude/hooks/stop-hook-autonomous.sh"
fi

# allow-sleep.sh
if [[ -f "$HOOKS_DIR/allow-sleep.sh" ]]; then
  warn "allow-sleep.sh exists — skipping"
else
  cat > "$HOOKS_DIR/allow-sleep.sh" << 'HOOKEOF'
#!/usr/bin/env bash
set -euo pipefail
PID_FILE="${CLAUDE_CAFFEINATE_PID:-$HOME/.claude/caffeinate.pid}"
if [[ -f "$PID_FILE" ]]; then
  pid=$(cat "$PID_FILE")
  kill -0 "$pid" 2>/dev/null && kill "$pid" 2>/dev/null || true
  rm -f "$PID_FILE"
fi
exit 0
HOOKEOF
  chmod +x "$HOOKS_DIR/allow-sleep.sh"
  ok "Installed ~/.claude/hooks/allow-sleep.sh"
fi

echo ""

# ── Register hooks in settings.json ─────────────────────────────────────────
echo -e "  ${BOLD}Configuring Claude settings...${NC}"
echo ""

SETTINGS_FILE="$HOME/.claude/settings.json"

if [[ -f "$SETTINGS_FILE" ]]; then
  cp "$SETTINGS_FILE" "${SETTINGS_FILE}.backup.$(date +%s)"
  info "Backed up settings.json"
fi

if [[ -f "$SETTINGS_FILE" ]] && grep -q "stop-hook-autonomous" "$SETTINGS_FILE" 2>/dev/null; then
  ok "Hooks already registered"
else
  if command -v python3 &>/dev/null; then
    python3 << 'PYEOF'
import json, os
p = os.path.expanduser("~/.claude/settings.json")
h = os.path.expanduser("~/.claude/hooks")
s = json.load(open(p)) if os.path.exists(p) else {}
if "hooks" not in s: s["hooks"] = {}
for event, script in [("UserPromptSubmit","prevent-sleep.sh"),("Stop","stop-hook-autonomous.sh"),("SessionEnd","allow-sleep.sh")]:
    if event not in s["hooks"]: s["hooks"][event] = []
    if not any(script in json.dumps(x) for x in s["hooks"][event]):
        s["hooks"][event].append({"matcher":"","hooks":[{"type":"command","command":f"{h}/{script}"}]})
with open(p,"w") as f: json.dump(s,f,indent=2); f.write("\n")
PYEOF
    ok "Registered hooks in settings.json"
  else
    warn "python3 not found — add hooks to ~/.claude/settings.json manually"
  fi
fi

echo ""

# ══════════════════════════════════════════════════════════════════════════════
# INTERACTIVE SETUP — collect tasks and project context
# ══════════════════════════════════════════════════════════════════════════════

echo -e "  ${DIM}════════════════════════════════════════════════════════════${NC}"
echo ""
echo -e "  ${BOLD}Now let's set up your tasks and project context.${NC}"
echo ""

# ── Collect tasks ────────────────────────────────────────────────────────────
if [[ -f "$LOOP_DIR/plan.md" ]]; then
  echo -e "  ${YELLOW}loop/plan.md already exists.${NC}"
  echo -ne "  Overwrite with new tasks? [y/N] "
  read -r OVERWRITE_PLAN
  [[ "$OVERWRITE_PLAN" =~ ^[Yy] ]] || SKIP_PLAN=1
fi

if [[ "${SKIP_PLAN:-0}" != "1" ]]; then
  echo -e "  ${BOLD}Enter your tasks${NC} — one per line."
  echo -e "  ${DIM}Be specific. Claude will execute these literally.${NC}"
  echo -e "  ${DIM}Example: \"Add JWT authentication with refresh tokens\"${NC}"
  echo -e "  ${DIM}Press Enter on an empty line when done.${NC}"
  echo ""

  TASKS=()
  TASK_NUM=1
  while true; do
    echo -ne "  ${CYAN}Task $TASK_NUM:${NC} "
    read -r TASK_LINE
    [[ -z "$TASK_LINE" ]] && break
    TASKS+=("$TASK_LINE")
    TASK_NUM=$((TASK_NUM + 1))
  done

  if [[ ${#TASKS[@]} -eq 0 ]]; then
    warn "No tasks entered — writing example plan"
    cat > "$LOOP_DIR/plan.md" << 'PLANEOF'
# Task Plan

## Phase 1
- [ ] **1.1** — First task description
- [ ] **1.2** — Second task description

## Final
- [ ] **FINAL** — Run full test suite, fix failures, tag release
PLANEOF
  else
    echo "# Task Plan" > "$LOOP_DIR/plan.md"
    echo "" >> "$LOOP_DIR/plan.md"
    i=1
    for task in "${TASKS[@]}"; do
      echo "- [ ] **$i** — $task" >> "$LOOP_DIR/plan.md"
      i=$((i + 1))
    done
    echo "" >> "$LOOP_DIR/plan.md"
    echo "- [ ] **FINAL** — Run full test suite, fix any failures" >> "$LOOP_DIR/plan.md"
    ok "Wrote ${#TASKS[@]} tasks to loop/plan.md"
  fi
  echo ""
fi

# ── Collect project context ──────────────────────────────────────────────────
if [[ -f "$LOOP_DIR/prompt.md" ]] && [[ "${SKIP_PLAN:-0}" == "1" ]]; then
  SKIP_PROMPT=1
fi

if [[ "${SKIP_PROMPT:-0}" != "1" ]]; then
  echo -e "  ${BOLD}Project context${NC} — tell Claude about your project."
  echo -e "  ${DIM}Stack, test commands, coding style, anything important.${NC}"
  echo -e "  ${DIM}Example: \"TypeScript + React, run 'npm test', use Prettier formatting\"${NC}"
  echo -e "  ${DIM}Press Enter on an empty line when done (or just Enter to skip).${NC}"
  echo ""

  RULES=()
  while true; do
    echo -ne "  ${CYAN}>${NC} "
    read -r RULE_LINE
    [[ -z "$RULE_LINE" ]] && break
    RULES+=("$RULE_LINE")
  done

  # Build prompt.md
  cat > "$LOOP_DIR/prompt.md" << 'PROMPTEOF'
You are executing a project roadmap autonomously. Read these files first:

1. `loop/plan.md` — Your task checklist (mark items `- [x]` as you complete them)
2. Read any project documentation (README, CLAUDE.md, etc.) for conventions

PROMPTEOF

  if [[ ${#RULES[@]} -gt 0 ]]; then
    echo "## Project Rules" >> "$LOOP_DIR/prompt.md"
    echo "" >> "$LOOP_DIR/prompt.md"
    for rule in "${RULES[@]}"; do
      echo "- $rule" >> "$LOOP_DIR/prompt.md"
    done
    echo "" >> "$LOOP_DIR/prompt.md"
    ok "Added ${#RULES[@]} project rules to prompt"
  fi

  cat >> "$LOOP_DIR/prompt.md" << 'PROMPTEOF'
## For each task in loop/plan.md:

1. Find the first unchecked `- [ ]` item
2. Understand what needs to be done
3. Implement it following the project's existing patterns and conventions
4. Run the project's type checker / linter / tests — fix all errors
5. Commit with a conventional message: `type(scope): description`
6. Mark the task `- [x]` in `loop/plan.md`
7. Move to the next unchecked task immediately

## Rules

- If tests fail, fix them before moving on
- If a task is difficult, make at least 30 genuine attempts before skipping
- Always run linter + type checker after code changes
- Do not stop until all tasks are checked or you are rate limited
PROMPTEOF
  ok "Created loop/prompt.md"
  echo ""
fi

# ── Summary & launch ─────────────────────────────────────────────────────────
TASK_COUNT=$(grep -c '^\- \[ \]' "$LOOP_DIR/plan.md" 2>/dev/null || echo "0")

echo -e "  ${DIM}════════════════════════════════════════════════════════════${NC}"
echo ""
echo -e "  ${BOLD}${GREEN}Ready to go!${NC}"
echo ""
echo -e "  ${BOLD}Tasks:${NC}  $TASK_COUNT in loop/plan.md"
echo -e "  ${BOLD}Prompt:${NC} loop/prompt.md"
echo -e "  ${BOLD}Log:${NC}    loop/loop.log (created on first run)"
echo ""
echo -e "  ${DIM}────────────────────────────────────────────────────────────${NC}"
echo ""
echo -e "  ${BOLD}To start:${NC}"
echo -e "     ${GREEN}./loop/loop.sh${NC}"
echo ""
echo -e "  ${BOLD}To start with sleep prevention (macOS):${NC}"
echo -e "     ${GREEN}caffeinate -i ./loop/loop.sh${NC}"
echo ""
echo -e "  ${BOLD}Monitor:${NC}"
echo -e "     ${DIM}tail -f loop/loop.log${NC}"
echo ""
echo -e "  ${BOLD}Config:${NC}"
echo -e "     ${DIM}CLAUDE_AUTONOMOUS=0${NC}  — let Claude stop freely (testing)"
echo -e "     ${DIM}ITERATION_COUNT=20${NC}   — more iterations"
echo -e "     ${DIM}ITERATION_DELAY=60${NC}   — longer pause between rounds"
echo ""
echo -e "  ${DIM}────────────────────────────────────────────────────────────${NC}"
echo ""
echo -ne "  ${BOLD}Start the loop now?${NC} [y/N] "
read -r START_NOW

if [[ "$START_NOW" =~ ^[Yy] ]]; then
  echo ""
  echo -e "  ${GREEN}Launching...${NC}"
  echo ""
  exec ./loop/loop.sh
fi

echo ""
echo -e "  ${DIM}Run ./loop/loop.sh whenever you're ready.${NC}"
echo ""
