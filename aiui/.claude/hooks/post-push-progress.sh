#!/usr/bin/env bash
# PostToolUse Bash hook: detect git push/commit and prompt Claude to update PROGRESS.md.
# Returns structured feedback with recent commits so Claude can write a session log entry.
# Uses python3 instead of jq for JSON (guaranteed on macOS).
set -euo pipefail

INPUT=$(cat)

# Extract command from JSON using python3
CMD=$(python3 -c "
import json, sys
try:
    data = json.loads(sys.stdin.read())
    print(data.get('tool_input', {}).get('command', ''))
except: pass
" <<< "$INPUT")

# Only trigger on git push or git commit commands
if ! echo "$CMD" | grep -qE '\bgit\s+(push|commit)\b'; then
  exit 0
fi

# Gather context for the progress update
BASE="${CLAUDE_PROJECT_DIR:-$(pwd)}"
BRANCH=$(git -C "$BASE" branch --show-current 2>/dev/null || echo "unknown")
PROGRESS_FILE="$BASE/PROGRESS.md"
TIMESTAMP=$(date '+%Y-%m-%d %H:%M')

# Get recent commits (branch vs main, or last 10)
if git -C "$BASE" rev-parse --verify main &>/dev/null; then
  COMMITS=$(git -C "$BASE" log --oneline main..HEAD 2>/dev/null | head -15)
  if [ -z "$COMMITS" ]; then
    COMMITS=$(git -C "$BASE" log --oneline -10 2>/dev/null)
  fi
else
  COMMITS=$(git -C "$BASE" log --oneline -10 2>/dev/null)
fi

# Get changed files in recent commits
CHANGED_FILES=$(git -C "$BASE" diff --name-only main..HEAD 2>/dev/null | head -20 || \
  git -C "$BASE" diff --name-only HEAD~5..HEAD 2>/dev/null | head -20 || \
  echo "unknown")

# Build the feedback message and output as JSON using python3
python3 -c "
import json, sys

message = '''Progress Update Needed

A git push/commit was detected on branch \`$BRANCH\` at $TIMESTAMP.

Recent commits:
\`\`\`
$COMMITS
\`\`\`

Changed files:
\`\`\`
$CHANGED_FILES
\`\`\`

Please update PROGRESS.md:
1. Add a session log entry under '## Session Log' with format: ### $TIMESTAMP — $BRANCH
2. Summarize what was accomplished (2-4 bullet points based on the commits above)
3. Update any roadmap checkboxes if tasks were completed
4. Commit the PROGRESS.md update'''

output = {
    'hookSpecificOutput': {
        'hookEventName': 'PostToolUse',
        'progressUpdate': message
    }
}
print(json.dumps(output))
"
