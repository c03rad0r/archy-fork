#!/usr/bin/env bash
# PreToolUse Bash guard: block dangerous shell commands.
# Denies: rm -rf, git reset --hard, git push -f, git clean -fd, chmod -R 777,
# fork bombs, block device overwrites, mkfs, paths escaping project root.
# Uses python3 instead of jq for JSON (guaranteed on macOS).
set -euo pipefail

INPUT=$(cat)
CMD=$(python3 -c "
import json, sys
try:
    data = json.loads(sys.stdin.read())
    print(data.get('tool_input', {}).get('command', ''))
except: pass
" <<< "$INPUT")
BASE="${CLAUDE_PROJECT_DIR:-}"
[[ -z "$BASE" ]] && BASE=$(python3 -c "
import json, sys
try:
    data = json.loads(sys.stdin.read())
    print(data.get('cwd', ''))
except: pass
" <<< "$INPUT")
[[ -z "$BASE" ]] && BASE="$(pwd)"

# Normalize: collapse whitespace, strip leading/trailing
CMD_NORM=$(echo "$CMD" | tr -s '[:space:]' ' ' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

deny() {
  local reason="$1"
  python3 -c "
import json
print(json.dumps({
    'hookSpecificOutput': {
        'hookEventName': 'PreToolUse',
        'permissionDecision': 'deny',
        'permissionDecisionReason': '$reason'
    }
}))
"
  exit 0
}

# Dangerous patterns (case-insensitive where sensible)
case "$CMD_NORM" in
  *"rm -rf"*|*"rm -fr"*|*"rm -f -r"*|*"rm -r -f"*)  deny "Destructive rm -rf blocked by security hook" ;;
  *"git reset --hard"*)                               deny "git reset --hard would lose uncommitted work" ;;
  *"git push --force"*|*"git push -f"*|*"git push -f "*) deny "git push --force would rewrite history" ;;
  *"git clean -fd"*|*"git clean -f -d"*)             deny "git clean -fd deletes untracked files" ;;
  *"chmod -R 777"*|*"chmod -R 0777"*)                deny "chmod -R 777 is a security risk" ;;
  *":(){ :"*"};:"*)                                   deny "Fork bomb pattern blocked" ;;
  *"> /dev/sd"*|*">/dev/sd"*)                        deny "Block device overwrite blocked" ;;
  *"mkfs "*|*"mkfs."*)                               deny "Disk format command blocked" ;;
esac

# Check for path traversal escaping project root (../ outside project)
# Only if we have a sensible base
if [[ -n "$BASE" ]] && [[ -d "$BASE" ]]; then
  # Simple heuristic: command contains .. and would resolve outside project
  if echo "$CMD_NORM" | grep -qE '\.\./|/\.\.'; then
    # Extract plausible paths and check - allow ../ within project
    if echo "$CMD_NORM" | grep -qE '(rm|mv|cp|cat|chmod|chown)\s+.*\.\.'; then
      # Could be risky; be conservative for rm/mv/cp
      if echo "$CMD_NORM" | grep -qE '\brm\b.*\.\.'; then
        deny "Path traversal with rm blocked"
      fi
    fi
  fi
fi

exit 0
