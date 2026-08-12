#!/usr/bin/env bash
# PreToolUse Edit|Write guard: block edits outside project and to protected paths.
# Denies: paths outside project, .git/, .env*, lockfiles, node_modules/
# Uses python3 instead of jq for JSON (guaranteed on macOS).
set -euo pipefail

INPUT=$(cat)
FILE_PATH=$(python3 -c "
import json, sys
try:
    data = json.loads(sys.stdin.read())
    print(data.get('tool_input', {}).get('file_path', ''))
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

# Resolve to absolute path
if [[ -z "$FILE_PATH" ]]; then
  exit 0
fi
ABS_BASE=$(cd "$BASE" 2>/dev/null && pwd) || true
[[ -z "$ABS_BASE" ]] && ABS_BASE=$(python3 -c "import os,sys; print(os.path.abspath(os.path.normpath(sys.argv[1])))" "$BASE" 2>/dev/null) || true
[[ -z "$ABS_BASE" ]] && ABS_BASE="$BASE"
# Ensure base has trailing slash for prefix check
[[ "$ABS_BASE" != */ ]] && ABS_BASE="${ABS_BASE}/"
if [[ "$FILE_PATH" != /* ]]; then
  ABS_PATH="$ABS_BASE${FILE_PATH#./}"
else
  ABS_PATH="$FILE_PATH"
fi
# Normalize path (collapse .. and ., no symlink resolution needed)
ABS_PATH=$(python3 -c "import os,sys; print(os.path.abspath(os.path.normpath(sys.argv[1])))" "$ABS_PATH" 2>/dev/null) || true
[[ -z "$ABS_PATH" ]] && ABS_PATH="$ABS_BASE${FILE_PATH#./}"

deny() {
  local reason="$1"
  echo "Blocked: $ABS_PATH — $reason" >&2
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

# Protected patterns (path contains or equals)
PROTECTED_PATTERNS=(
  ".git/"
  ".env"
  ".env.local"
  "node_modules/"
  "package-lock.json"
  "pnpm-lock.yaml"
)

for pattern in "${PROTECTED_PATTERNS[@]}"; do
  if [[ "$ABS_PATH" == *"$pattern"* ]] || [[ "$ABS_PATH" == *"/$pattern" ]]; then
    deny "Edit blocked: path matches protected pattern ($pattern)"
  fi
done

# .env.*.local
if [[ "$ABS_PATH" =~ \.env\..*\.local$ ]]; then
  deny "Edit blocked: .env.*.local files contain secrets"
fi

# Ensure path is under project root (ABS_BASE has trailing /)
if [[ "$ABS_PATH" != "$ABS_BASE"* ]] && [[ "$ABS_PATH" != "$BASE"* ]]; then
  deny "Edit blocked: path is outside project directory"
fi

exit 0
