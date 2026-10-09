#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
output=$(PATH=/usr/bin:/bin "$ROOT/scripts/archy-developer-setup" --check 2>&1 || true)
grep -qE '^(ok|missing)[[:space:]]' <<<"$output"
! grep -q 'sudo -n apt-get' <<<"$output"
printf 'archy-developer-setup check is non-mutating\n'
