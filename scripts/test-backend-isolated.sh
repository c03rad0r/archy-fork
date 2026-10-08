#!/usr/bin/env bash
# Compile normally; execute unit tests away from real wallets, service buses,
# container storage, processes and networking. Never silently fall back to host.
set -euo pipefail
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
command -v systemd-run >/dev/null
command -v unshare >/dev/null
command -v setpriv >/dev/null
sudo -n true || { echo 'Isolated backend tests require noninteractive sudo for systemd namespaces.' >&2; exit 1; }
metadata=$(mktemp)
trap 'rm -f "$metadata"' EXIT
case "${ARCHY_TEST_PACKAGE:-archipelago}" in
  archipelago) test_target=(-p archipelago --bin archipelago) ;;
  archipelago-container) test_target=(-p archipelago-container --lib) ;;
  *) echo 'Unsupported isolated test package' >&2; exit 2 ;;
esac
if ! cargo test --manifest-path "$REPO/core/Cargo.toml" "${test_target[@]}" \
  --locked --no-run --message-format=json --config 'profile.test.package.archipelago.opt-level=0' --config 'profile.test.package.archipelago.debug=0' > "$metadata"; then
    python3 - "$metadata" <<'PYDIAG'
import json,sys
for line in open(sys.argv[1]):
 try: item=json.loads(line)
 except json.JSONDecodeError: continue
 rendered=item.get('message',{}).get('rendered') if item.get('reason')=='compiler-message' else None
 if rendered: print(rendered,file=sys.stderr,end='')
PYDIAG
    exit 1
fi
executable=$(python3 - "$metadata" <<'PY'
import json,sys
found=[]
for line in open(sys.argv[1]):
 try: item=json.loads(line)
 except json.JSONDecodeError: continue
 if item.get('reason')=='compiler-artifact' and item.get('profile',{}).get('test') and item.get('executable'):
  found.append(item['executable'])
assert len(found)==1, f'Expected one unit test executable, got {len(found)}'
print(found[0])
PY
)
[[ -x "$executable" ]]
unit="archy-isolated-tests-$(date +%s)-$$"
sudo -n systemd-run --unit="$unit" --wait --pipe --collect \
  --property="WorkingDirectory=$REPO/core" \
  --property=PrivateNetwork=yes --property=PrivateTmp=yes --property=PrivateDevices=yes \
  --property=ProtectSystem=strict --property=ProtectHome=read-only \
  --property=NoNewPrivileges=yes \
  --property='TemporaryFileSystem=/run:rw /var/lib/archipelago:rw /var/lib/containers:rw /root:rw' \
  --setenv=ARCHY_TEST_ISOLATED=1 \
  /usr/bin/unshare --pid --fork --mount-proc --kill-child \
  /usr/bin/setpriv --bounding-set=-all,+chown,+dac_override,+fowner,+setuid,+setgid,+kill \
  "$executable" --test-threads=4 "$@"
