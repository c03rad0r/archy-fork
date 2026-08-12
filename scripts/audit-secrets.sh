#!/bin/bash
set -euo pipefail
# SEC-202: Secrets audit — checks for hardcoded credentials in the codebase.
# Scans source files for common secret patterns.

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PASS=0
FAIL=0
RESULTS=()

log() { echo -e "\033[1;34m[AUDIT]\033[0m $*"; }
pass() { echo -e "\033[1;32m[PASS]\033[0m $*"; PASS=$((PASS + 1)); RESULTS+=("PASS: $*"); }
fail() { echo -e "\033[1;31m[FAIL]\033[0m $*"; FAIL=$((FAIL + 1)); RESULTS+=("FAIL: $*"); }

# Patterns to search for (case insensitive)
PATTERNS=(
  "password\s*=\s*['\"][^'\"]*['\"]"
  "api_key\s*=\s*['\"][^'\"]*['\"]"
  "secret\s*=\s*['\"][^'\"]*['\"]"
  "private_key\s*=\s*['\"][^'\"]*['\"]"
  "sk-ant-[A-Za-z0-9_-]{20,}"
  "AKIA[A-Z0-9]{16}"
  "ghp_[a-zA-Z0-9]{36}"
  "glpat-[a-zA-Z0-9_-]{20}"
  # Credentialed URLs: scheme://user:pass@host
  "://[A-Za-z0-9_.-]+:[A-Za-z0-9_.@!%-]{8,}@"
  # sshpass with an inline literal
  "sshpass\s+-p\s*['\"][^'\"]+['\"]"
)

# Path allowlist — anchored to the PATH only, never to line content.
# The old version allow-matched the whole "file:line:content" string against
# bare words like "test" and "\.md$", so any hit whose path or text contained
# "test"/"demo"/"example" was silently dropped, and *.md was never scanned at
# all. That is why live API keys and node passwords survived this audit.
ALLOW_PATHS="(^|/)node_modules/|(^|/)(dist|target|\.git)/|\.example($|\.)|(^|/)package-lock\.json$|(^|/)Cargo\.lock$|(^|/)scripts/audit-secrets\.sh$"

# File types to scan. Markdown and YAML are in scope: docs and CI workflows are
# where the real leaks have historically lived.
SCAN_EXTS='\.(rs|ts|vue|js|mjs|cjs|json|sh|py|md|ya?ml|toml|kt|java|gradle|env)$'

main() {
  log "=== Secrets Audit ==="
  echo ""

  # 1. Check for .env files in version control
  log "1. Checking for .env files in git..."
  local env_files
  env_files=$(cd "$REPO_ROOT" && git ls-files | grep -E '(^|/)\.env($|[.])|(^|/)[^/]*\.env($|[.])' | grep -vE '(^|/)\.env\.example$|(^|/)[^/]*\.env\.example$' || echo "")
  if [ -z "$env_files" ]; then
    pass "No .env files tracked in git"
  else
    fail "Found .env files in git: $env_files"
  fi

  # 2. Check .gitignore includes sensitive patterns
  log "2. Checking .gitignore coverage..."
  local gitignore="$REPO_ROOT/.gitignore"
  if [ -f "$gitignore" ]; then
    local has_env has_key
    has_env=$(grep -c '\.env' "$gitignore" || echo 0)
    has_key=$(grep -c 'credentials\|\.key\|\.pem' "$gitignore" || echo 0)
    if [ "$has_env" -gt 0 ]; then
      pass ".gitignore covers .env files"
    else
      fail ".gitignore missing .env pattern"
    fi
  else
    fail "No .gitignore found"
  fi

  # 3. Scan source for hardcoded credentials
  log "3. Scanning source for hardcoded secrets..."
  local found_secrets=0
  # Scan TRACKED files only — that is exactly the set that would be published.
  local scan_files
  scan_files=$(cd "$REPO_ROOT" && git ls-files | grep -E "$SCAN_EXTS" | grep -vE "$ALLOW_PATHS" || echo "")
  if [ -z "$scan_files" ]; then
    fail "No tracked files matched the scan set (is this a git repo?)"
    return 1
  fi
  for pattern in "${PATTERNS[@]}"; do
    local matches
    matches=$(cd "$REPO_ROOT" && echo "$scan_files" | tr '\n' '\0' \
      | xargs -0 grep -niE "$pattern" 2>/dev/null || echo "")
    if [ -n "$matches" ]; then
      # Filter out false positives: empty strings, variable indirection, and
      # scrubbed <PLACEHOLDER> tokens. NOTE: the previous version wrote the
      # single-quote class as \x27\x27, which GNU grep does not expand in an
      # ERE — so the empty-string rule silently never matched. Use a literal
      # quote via a shell variable instead.
      local q="'"
      local real_matches
      real_matches=$(echo "$matches" | grep -vE "\"\"|${q}${q}|<[A-Z_]+>|None|null|undefined|TODO|placeholder|Option<|\\\$\{[A-Za-z0-9_]+(:-[^}]*)?\}|\\\$[A-Za-z0-9_]+|TestPassword|password123|entertoexit|…|\\.\\.\\." || echo "")
      if [ -n "$real_matches" ]; then
        echo "  WARNING: Pattern '$pattern' found:"
        echo "$real_matches" | head -5 | sed 's/^/    /'
        found_secrets=$((found_secrets + 1))
      fi
    fi
  done
  if [ "$found_secrets" -eq 0 ]; then
    pass "No hardcoded secrets found in source"
  else
    fail "Found $found_secrets secret pattern matches (review above)"
  fi

  # 4. Check deploy-config is gitignored
  log "4. Checking deploy-config.sh is gitignored..."
  if cd "$REPO_ROOT" && git check-ignore scripts/deploy-config.sh > /dev/null 2>&1; then
    pass "scripts/deploy-config.sh is gitignored"
  elif [ -f "$REPO_ROOT/scripts/deploy-config.sh" ]; then
    fail "scripts/deploy-config.sh exists but is NOT gitignored"
  else
    pass "scripts/deploy-config.sh does not exist (using env vars)"
  fi

  # 5. Check for credential files in repo
  log "5. Checking for credential files..."
  local cred_files
  # `testdata/` holds throwaway keypairs generated for unit tests (appgate TLS);
  # they are not credentials for anything real. Narrow, path-anchored exemption.
  cred_files=$(cd "$REPO_ROOT" && git ls-files | grep -Ei '(\.pem$|\.key$|\.p12$|\.pfx$|\.jks$|\.keystore$|id_rsa|id_ed25519|macaroon)' | grep -vE '\.(rs|ts|sh)$|(^|/)testdata/' || echo "")
  if [ -z "$cred_files" ]; then
    pass "No credential files tracked in git"
  else
    fail "Credential files in git: $cred_files"
  fi

  echo ""
  log "=== RESULTS ==="
  for r in "${RESULTS[@]}"; do
    echo "  $r"
  done
  echo ""
  log "Pass: $PASS | Fail: $FAIL"

  [ $FAIL -gt 0 ] && exit 1
  exit 0
}

main "$@"
