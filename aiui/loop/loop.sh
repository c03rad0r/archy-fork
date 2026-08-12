#!/usr/bin/env sh
# Headless loop script for overnight Claude Code automation.
# Set CLAUDE_AUTONOMOUS=1 for Ralph Wiggum (Stop hook blocks until plan is complete).
# Rate-limit aware: detects limits, sleeps until reset, and retries automatically.
set -u

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/.." && pwd)}"
PROMPT_FILE="${PROMPT_FILE:-$PROJECT_DIR/loop/prompt.md}"
LOG_FILE="${LOG_FILE:-$PROJECT_DIR/loop/loop.log}"
ITERATION_COUNT="${ITERATION_COUNT:-10}"
ITERATION_DELAY="${ITERATION_DELAY:-30}"
CLAUDE_BIN="${CLAUDE_BIN:-claude}"
RATE_LIMIT_WAIT="${RATE_LIMIT_WAIT:-3600}"  # Default: wait 1 hour on rate limit
MAX_RATE_LIMIT_RETRIES="${MAX_RATE_LIMIT_RETRIES:-5}"  # Max retries before giving up
CLAUDE_EXIT=0

cd "$PROJECT_DIR"

# Human-readable log with visual separators
log() {
  echo "$1" | tee -a "$LOG_FILE"
}

banner() {
  log ""
  log "════════════════════════════════════════════════════════════════"
  log "  $1"
  log "  $(date '+%Y-%m-%d %H:%M:%S')"
  log "════════════════════════════════════════════════════════════════"
  log ""
}

section() {
  log ""
  log "────────────────────────────────────────"
  log "  $1"
  log "────────────────────────────────────────"
  log ""
}

# Check if plan has remaining tasks
plan_has_tasks() {
  grep -q '^\- \[ \]' "$PROJECT_DIR/loop/plan.md" 2>/dev/null
}

# Show remaining task count
remaining_tasks() {
  grep -c '^\- \[ \]' "$PROJECT_DIR/loop/plan.md" 2>/dev/null || echo "0"
}

# Show next task
next_task() {
  grep -m1 '^\- \[ \]' "$PROJECT_DIR/loop/plan.md" 2>/dev/null | sed 's/^- \[ \] //' || echo "(none)"
}

# Detect rate limit from Claude output (only when Claude exited non-zero)
check_rate_limit() {
  [ "${CLAUDE_EXIT:-0}" -eq 0 ] && return 1
  # Check last 50 lines for rate limit indicators, excluding our own log lines
  tail -50 "$LOG_FILE" 2>/dev/null | grep -v "^Rate limit detected" | grep -v "^Sleeping" | grep -v "^═" | grep -v "^─" | grep -qi \
    -e "rate.limit" \
    -e "too.many.requests" \
    -e "429" \
    -e "quota.exceeded" \
    -e "usage.limit" \
    -e "limit.reached" 2>/dev/null
}

banner "OVERNIGHT AUTOMATION STARTED"
log "  Project:      $PROJECT_DIR"
log "  Prompt:       $PROMPT_FILE"
log "  Autonomous:   ${CLAUDE_AUTONOMOUS:-0}"
log "  Iterations:   $ITERATION_COUNT (${ITERATION_DELAY}s between each)"
log "  Rate limit:   wait ${RATE_LIMIT_WAIT}s, retry up to ${MAX_RATE_LIMIT_RETRIES}x"
log "  Tasks left:   $(remaining_tasks)"
log "  Next task:    $(next_task)"
log ""

i=1
rate_limit_retries=0
while [ "$i" -le "$ITERATION_COUNT" ]; do

  # Check if there are tasks remaining before starting
  if ! plan_has_tasks; then
    banner "ALL TASKS COMPLETE"
    log "  No remaining tasks in plan.md. Stopping."
    break
  fi

  section "ITERATION $i/$ITERATION_COUNT"
  log "  Tasks remaining: $(remaining_tasks)"
  log "  Next task:       $(next_task)"
  log ""

  export CLAUDE_PROJECT_DIR="$PROJECT_DIR"
  export CLAUDE_AUTONOMOUS="${CLAUDE_AUTONOMOUS:-1}"

  # Run Claude with autonomous permissions; prompt from file
  if [ -f "$PROMPT_FILE" ]; then
    log "  Starting Claude session..."
    log ""
    "$CLAUDE_BIN" -p --dangerously-skip-permissions \
      < "$PROMPT_FILE" 2>&1 | tee -a "$LOG_FILE"
    CLAUDE_EXIT=$?
    log ""
    log "  Claude exited with code: $CLAUDE_EXIT"
  else
    log "  ERROR: $PROMPT_FILE not found"
    exit 1
  fi

  # Check for rate limit after Claude exits
  if check_rate_limit; then
    rate_limit_retries=$((rate_limit_retries + 1))
    if [ "$rate_limit_retries" -ge "$MAX_RATE_LIMIT_RETRIES" ]; then
      section "RATE LIMITED — SCHEDULING LAUNCHD RETRY"
      log "  Hit rate limit $rate_limit_retries times. Creating launchd job to retry later."

      # Schedule a retry using launchd for after rate limit resets
      PLIST_LABEL="com.aiui.overnight-retry"
      PLIST_PATH="$HOME/Library/LaunchAgents/${PLIST_LABEL}.plist"
      RETRY_TIME=$(date -v+${RATE_LIMIT_WAIT}S '+%H:%M' 2>/dev/null || date -d "+${RATE_LIMIT_WAIT} seconds" '+%H:%M')
      RETRY_HOUR=$(echo "$RETRY_TIME" | cut -d: -f1)
      RETRY_MIN=$(echo "$RETRY_TIME" | cut -d: -f2)

      cat > "$PLIST_PATH" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>${PLIST_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/sh</string>
    <string>-c</string>
    <string>cd ${PROJECT_DIR} && caffeinate -i ./loop/loop.sh >> ${LOG_FILE} 2>&1; launchctl unload ${PLIST_PATH}; rm -f ${PLIST_PATH}</string>
  </array>
  <key>StartCalendarInterval</key>
  <dict>
    <key>Hour</key>
    <integer>${RETRY_HOUR}</integer>
    <key>Minute</key>
    <integer>${RETRY_MIN}</integer>
  </dict>
  <key>EnvironmentVariables</key>
  <dict>
    <key>CLAUDE_AUTONOMOUS</key>
    <string>1</string>
    <key>CLAUDE_PROJECT_DIR</key>
    <string>${PROJECT_DIR}</string>
    <key>PATH</key>
    <string>/usr/local/bin:/usr/bin:/bin:$HOME/.local/bin</string>
  </dict>
  <key>StandardOutPath</key>
  <string>${LOG_FILE}</string>
  <key>StandardErrorPath</key>
  <string>${LOG_FILE}</string>
</dict>
</plist>
PLIST

      launchctl load "$PLIST_PATH" 2>/dev/null || true
      log "  Scheduled retry at ~${RETRY_TIME}"
      log "  Plist: $PLIST_PATH (auto-removes after running)"
      exit 0
    fi

    section "RATE LIMITED — WAITING"
    log "  Attempt $rate_limit_retries/$MAX_RATE_LIMIT_RETRIES"
    log "  Sleeping ${RATE_LIMIT_WAIT}s until $(date -v+${RATE_LIMIT_WAIT}S '+%H:%M:%S' 2>/dev/null || date -d "+${RATE_LIMIT_WAIT} seconds" '+%H:%M:%S')..."
    sleep "$RATE_LIMIT_WAIT"

    # Check if plan still has tasks before retrying
    if ! plan_has_tasks; then
      banner "ALL TASKS COMPLETE (during rate limit wait)"
      break
    fi
    log "  Retrying..."
    continue  # Retry same iteration
  fi

  # Reset rate limit counter on successful run
  rate_limit_retries=0

  section "ITERATION $i COMPLETE"
  log "  Tasks remaining: $(remaining_tasks)"
  log "  Next task:       $(next_task)"

  i=$((i + 1))
  if [ "$i" -le "$ITERATION_COUNT" ] && [ "$ITERATION_DELAY" -gt 0 ]; then
    log "  Pausing ${ITERATION_DELAY}s before next iteration..."
    sleep "$ITERATION_DELAY"
  fi
done

banner "LOOP FINISHED"
log "  Completed $((i - 1)) iterations"
log "  Tasks remaining: $(remaining_tasks)"
log ""
