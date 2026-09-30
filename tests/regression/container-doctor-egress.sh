#!/usr/bin/env bash
# Simulate failures without namespaces, network access, or real repair commands.
set -euo pipefail
source "$(dirname "$0")/../../scripts/container-doctor.sh"
id() { if [[ "$*" == '-u archipelago' ]]; then echo 1000; else echo "${TEST_UID:-0}"; fi; }
pgrep() { if [[ "$HAS_NETWORK" == 1 ]]; then echo 123; else return 1; fi; }
sleep() { :; }
# Any mutation fails the test immediately, including within command substitution.
tripwire() { echo 'FAIL: diagnostic attempted a mutation' >&2; exit 99; }
podman() { tripwire; }
podman_rootless() { tripwire; }
sudo() { tripwire; }
systemctl() { tripwire; }
pkill() { tripwire; }
kill() { tripwire; }
rm() { tripwire; }
mkdir() { tripwire; }
timeout() {
    if [[ "$2" == bash ]]; then return "$HOST_STATUS"; fi
    [[ "$2" == nsenter ]] || exit 98
    PROBES=$((PROBES + 1))
    if [[ "$PROBES" == 1 ]]; then return "$FIRST_STATUS"; fi
    return "$SECOND_STATUS"
}
check_case() {
    local label=$1 expected=$2 expected_probes=$3
    PROBES=0
    local status=0
    check_rootless_netns_egress > /dev/null || status=$?
    [[ "$status" == "$expected" && "$PROBES" == "$expected_probes" ]] || {
        echo "FAIL: $label status=$status probes=$PROBES"; exit 1;
    }
    echo "PASS: $label"
}
HAS_NETWORK=1 HOST_STATUS=0 FIRST_STATUS=0 SECOND_STATUS=0
check_case healthy 1 1
TEST_UID=1000 check_case rootless-caller 1 0
HAS_NETWORK=0 check_case no-network 1 0
HOST_STATUS=1 check_case host-offline 2 0
FIRST_STATUS=1 check_case transient-recovery 1 2
FIRST_STATUS=1 SECOND_STATUS=1 check_case repeated-egress-failure 2 2
FIRST_STATUS=126 SECOND_STATUS=126 check_case namespace-access-failure 2 2
# Failure must remain an unresolved warning on every scheduled invocation.
FIRST_STATUS=1 SECOND_STATUS=1
for attempt in 1 2 3 4 5; do
    PROBES=0
    run_fix netns-egress check_rootless_netns_egress > /dev/null
done
[[ "$CHECKS_WARNED" == 5 && "$FIXES_APPLIED" == 0 && "$CHECKS_PASSED" == 0 ]]
FIRST_STATUS=0 PROBES=0
run_fix netns-egress check_rootless_netns_egress > /dev/null
[[ "$CHECKS_PASSED" == 1 && "$FIXES_APPLIED" == 0 ]]
echo 'PASS: repeated failure warnings never trigger repair or report a successful check'
