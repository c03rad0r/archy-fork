#!/usr/bin/env bash
# No real service/container operations: all external operations are replaced.
set -euo pipefail
source "$(dirname "$0")/../../scripts/container-doctor.sh"
ss() {
    [[ "${SS_FAIL:-0}" == 0 ]] || return 1
    printf 'LISTEN 0 4096 *:8333 *:*\n'
    # More than a pipe buffer, reliably reproducing grep -q / pipefail SIGPIPE.
    awk 'BEGIN { for(i=0;i<20000;i++) print "LISTEN 0 4096 127.0.0.1:1234 *:*" }'
}
port_is_listening 8333
port_is_listening 1234 udp
if port_is_listening 833; then exit 1; else [[ $? == 1 ]]; fi
if SS_FAIL=1 port_is_listening 8333; then exit 1; else [[ $? == 2 ]]; fi
calls=$(mktemp)
trap 'rm -f "$calls"' EXIT
podman_rootless() {
    case "$1" in
        ps) echo bitcoin-core ;;
        inspect)
            if [[ "$*" == *PODMAN_SYSTEMD_UNIT* ]]; then echo "${TEST_UNIT:-bitcoin-core.service}";
            else echo '8333/tcp 8333'; fi ;;
        restart) echo "podman $*" >> "$calls" ;;
        *) exit 1 ;;
    esac
}
id() { echo 1000; }
systemctl() { echo "systemctl $*" >> "$calls"; }
# Healthy listener and failed ss inspection must not restart anything.
fix_missing_rootless_ports && exit 1
SS_FAIL=1 fix_missing_rootless_ports && exit 1
[[ ! -s "$calls" ]]
# A real missing listener restarts its managed unit, preserving stop timeout.
ss() { echo 'LISTEN 0 4096 *:1234 *:*'; }
fix_missing_rootless_ports
grep -Fx 'systemctl --user restart bitcoin-core.service' "$calls"
: > "$calls"
TEST_UNIT='<no value>' restart_rootless_container bitcoin-core
grep -Fx 'podman restart --time 600 bitcoin-core' "$calls"
echo 'PASS: healthy/missing/failed listener checks and safe managed/unmanaged restart'
