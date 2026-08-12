#!/usr/bin/env bash
# Archipelago RPC exposure probe — audit item C-6 / KEY-01 (F-01).
#
# Answers two DIFFERENT questions that the audit's original C-6 command
# conflated, and that must never be conflated again:
#
#   1. EXPOSURE        — is the *unauthenticated* RPC surface reachable from
#                        this vantage point at all?  Measured with
#                        `auth.isOnboardingComplete`, which really is on the
#                        unauthenticated allowlist
#                        (core/archipelago/src/api/rpc/middleware.rs:9).
#                        A 200 means the door F-01 depends on is open from here.
#
#   2. SESSION ENFORCEMENT — is the session check still rejecting everything
#                        that is NOT allowlisted?  Measured with `seed.status`,
#                        which is deliberately absent from the allowlist, so a
#                        401 is the *correct* result.
#
# Why this matters: the audit (docs/security/ENTROPY-SEED-AUDIT-2026-07-31.md,
# C-6) probes with `seed.status` and calls a 200 a failure. But `seed.status`
# is not allowlisted, so it returns 401 by design — the audit's failure
# criterion can never fire, and the probe would report the surface as CLOSED
# while F-01's actual door stands wide open. This script fixes that.
#
# SAFETY RULES
#   * Default mode is read-only BY CONSTRUCTION: the request method is taken
#     from the fixed READONLY_METHODS array and never from an argument.
#   * Every mutating request lives inside one explicit `--destructive` branch.
#   * `--destructive` issues a real `seed.restore`. Against a node WITHOUT the
#     10-01 gate that DESTROYS the node's identity. Disposable nodes only.
#   * No real key material is ever handled: the refusal check uses the
#     published BIP-39 all-`abandon` + `art` test vector. This script never
#     generates and never prints a mnemonic.
#   * No node address, onion address, username or password is embedded here.
#     Record node LABELS in evidence documents, not raw addresses.
#
# Usage:
#   scripts/security/rpc-exposure-probe.sh --target <host|onion|ULA> \
#       [--scheme http|https] [--port N] [--label <name>] [--insecure] \
#       [--destructive]
#
# Over Tor:  torsocks scripts/security/rpc-exposure-probe.sh --target <onion> ...
#
# Exit codes:
#   0  every control behaved as expected
#   1  a control failed (seed.status was not 401, or --destructive was not refused)
#   2  usage error

set -euo pipefail

# ── The ONLY methods the default path may ever call. Read-only, no side
#    effects, none of them mutate identity. Never build this from an argument.
READONLY_METHODS=("health" "auth.isOnboardingComplete" "seed.status")

# Published BIP-39 test vector (32 bytes of 0x00). Public test data — NOT a
# real mnemonic, and deliberately unlike audit item C-5, which mints real ones.
TEST_MNEMONIC_WORDS='["abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","abandon","art"]'

# The refusal prefix emitted by 10-01's gate
# (core/archipelago/src/api/rpc/onboarding_gate.rs). Load-bearing: the error
# sanitizer (middleware.rs:47-71) only lets messages with a known prefix
# through, and "Not supported" is on that list.
REFUSAL_PREFIX="Not supported:"

TARGET=""
SCHEME="http"
PORT="80"
LABEL="unlabelled"
DESTRUCTIVE=0
INSECURE=0
TIMEOUT=15
FAIL=0

if [ -t 1 ]; then
    C_RED=$'\033[31m'; C_GRN=$'\033[32m'; C_YEL=$'\033[33m'; C_OFF=$'\033[0m'
else
    C_RED=""; C_GRN=""; C_YEL=""; C_OFF=""
fi

usage() {
    cat <<'EOF'
rpc-exposure-probe.sh — measure the unauthenticated Archipelago RPC surface (C-6 / KEY-01)

Usage:
  rpc-exposure-probe.sh --target <host|onion|ULA> [options]

Options:
  --target <host>      Host, onion address or IPv6 ULA to probe.  Required.
                       Bare IPv6 addresses are bracketed automatically.
  --scheme http|https  Default: http
  --port <n>           Default: 80
  --label <name>       Vantage-point label printed on every verdict line
                       (e.g. lan, tor, mesh, loopback).  Default: unlabelled
  --insecure           Accept a self-signed TLS certificate (https only).
  --destructive        Enable the ONE mutating branch: the KEY-01 refusal
                       check.  DISPOSABLE NODES ONLY.
  --help               This text.

Checks in default (read-only) mode — four requests total:
  1. health                      on /rpc/v1  liveness from this vantage point
  2. auth.isOnboardingComplete   on /rpc/v1  EXPOSURE signal; 200 = the
                                             unauthenticated surface is
                                             reachable from here (C-6)
  3. seed.status                 on /rpc/v1  SESSION-ENFORCEMENT control;
                                             anything but 401 is CRITICAL
  4. auth.isOnboardingComplete   on /rpc/    same exposure signal on nginx's
                                             second proxy path

With --destructive, one extra request:
  5. seed.restore  with the published BIP-39 all-abandon/art test vector.
     PASS only if the response carries an error beginning "Not supported:".

Safety rules:
  * Read-only mode cannot mutate identity: methods come from a fixed array,
    never from an argument.
  * --destructive issues a real seed.restore.  On a node WITHOUT 10-01's gate
    this DESTROYS that node's identity.  Never run it against a node in use.
  * No real mnemonic is ever generated, handled or printed by this script.
  * Never paste raw node/onion/ULA addresses into committed evidence — record
    the --label and the status codes.

Byte-identity check (run ON the node, not from here — this script has no
node-local file access):
  before: sudo sha256sum /var/lib/archipelago/identity/node_key /var/lib/archipelago/identity/nostr_secret
  after:  sudo sha256sum /var/lib/archipelago/identity/node_key /var/lib/archipelago/identity/nostr_secret
The two outputs must match character for character.

Exit codes: 0 ok · 1 a control failed · 2 usage error
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --target)      TARGET="${2:-}"; shift 2 ;;
        --scheme)      SCHEME="${2:-}"; shift 2 ;;
        --port)        PORT="${2:-}"; shift 2 ;;
        --label)       LABEL="${2:-}"; shift 2 ;;
        --insecure)    INSECURE=1; shift ;;
        --destructive) DESTRUCTIVE=1; shift ;;
        --help|-h)     usage; exit 0 ;;
        *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

if [ -z "$TARGET" ]; then
    echo "error: --target is required" >&2
    usage >&2
    exit 2
fi

case "$SCHEME" in
    http|https) ;;
    *) echo "error: --scheme must be http or https" >&2; exit 2 ;;
esac

# Bracket bare IPv6 / ULA targets so the mesh transport can be probed.
HOSTPART="$TARGET"
case "$TARGET" in
    \[*\]) ;;
    *:*)   HOSTPART="[$TARGET]" ;;
esac

BASE="${SCHEME}://${HOSTPART}:${PORT}"

CURL_OPTS=(-sS --max-time "$TIMEOUT" -H 'Content-Type: application/json')
if [ "$SCHEME" = "https" ] && [ "$INSECURE" = "1" ]; then
    CURL_OPTS+=(--insecure)
fi

BODY_FILE="$(mktemp)"
cleanup() { rm -f "$BODY_FILE"; }
trap cleanup EXIT

# rpc_call <path> <method> <params-json>
# Sets HTTP_CODE and RESP_BODY. Never fails the script on a network error;
# an unreachable vantage point is a RESULT, not a crash.
HTTP_CODE=""
RESP_BODY=""
rpc_call() {
    local path="$1" method="$2" params="$3"
    local payload
    payload=$(printf '{"jsonrpc":"2.0","id":1,"method":"%s","params":%s}' "$method" "$params")
    # NOTE: curl's %{http_code} is already "000" when no response arrived, so
    # the failure fallback must REPLACE the captured value, never append to it
    # (a `|| echo 000` here yields "000000" and misroutes every verdict).
    if ! HTTP_CODE=$(curl "${CURL_OPTS[@]}" -o "$BODY_FILE" -w '%{http_code}' \
        -X POST "${BASE}${path}" -d "$payload" 2>"${BODY_FILE}.err"); then
        HTTP_CODE="000"
    fi
    case "$HTTP_CODE" in
        [0-9][0-9][0-9]) ;;
        *) HTTP_CODE="000" ;;
    esac
    RESP_BODY=$(cat "$BODY_FILE" 2>/dev/null || true)
    if [ "$HTTP_CODE" = "000" ]; then
        RESP_BODY=$(cat "${BODY_FILE}.err" 2>/dev/null || true)
    fi
    rm -f "${BODY_FILE}.err"
}

verdict() {
    # verdict <colour> <tag> <method> <code> <note>
    printf '[%s] %-30s %-4s %s%-10s%s %s\n' \
        "$LABEL" "$3" "$4" "$1" "$2" "$C_OFF" "$5"
}

echo "RPC exposure probe — label=${LABEL} endpoint=${BASE}"
echo "  audit item C-6 · KEY-01 (F-01) · read-only mode$([ "$DESTRUCTIVE" = "1" ] && echo " + DESTRUCTIVE")"
echo

# ── 1. Liveness ──────────────────────────────────────────────────────
rpc_call "/rpc/v1" "${READONLY_METHODS[0]}" "null"
case "$HTTP_CODE" in
    200) verdict "$C_GRN" "REACHABLE" "${READONLY_METHODS[0]}" "$HTTP_CODE" "endpoint answers from this vantage point" ;;
    000) verdict "$C_YEL" "UNREACHABLE" "${READONLY_METHODS[0]}" "---" "no answer: ${RESP_BODY:0:120}" ;;
    429) verdict "$C_YEL" "RATELIMIT" "${READONLY_METHODS[0]}" "$HTTP_CODE" "rate limited — rerun later, this is not a refusal" ;;
    *)   verdict "$C_YEL" "UNEXPECTED" "${READONLY_METHODS[0]}" "$HTTP_CODE" "endpoint answered but not 200" ;;
esac

# ── 2. EXPOSURE signal (the honest C-6 measurement) ──────────────────
rpc_call "/rpc/v1" "${READONLY_METHODS[1]}" "null"
case "$HTTP_CODE" in
    200) verdict "$C_YEL" "EXPOSED" "${READONLY_METHODS[1]}" "$HTTP_CODE" "unauthenticated RPC surface IS reachable from here (C-6 result)" ;;
    000) verdict "$C_YEL" "UNREACHABLE" "${READONLY_METHODS[1]}" "---" "no answer: ${RESP_BODY:0:120}" ;;
    429) verdict "$C_YEL" "RATELIMIT" "${READONLY_METHODS[1]}" "$HTTP_CODE" "rate limited — rerun later" ;;
    *)   verdict "$C_GRN" "NOT-EXPOSED" "${READONLY_METHODS[1]}" "$HTTP_CODE" "unauthenticated surface did not answer 200 from here" ;;
esac

# ── 3. SESSION-ENFORCEMENT control ───────────────────────────────────
rpc_call "/rpc/v1" "${READONLY_METHODS[2]}" "null"
case "$HTTP_CODE" in
    401) verdict "$C_GRN" "PASS" "${READONLY_METHODS[2]}" "$HTTP_CODE" "session enforcement active for non-allowlisted methods" ;;
    000) verdict "$C_YEL" "UNREACHABLE" "${READONLY_METHODS[2]}" "---" "no answer: ${RESP_BODY:0:120}" ;;
    429) verdict "$C_YEL" "RATELIMIT" "${READONLY_METHODS[2]}" "$HTTP_CODE" "rate limited — inconclusive, rerun later" ;;
    *)   verdict "$C_RED" "CRITICAL" "${READONLY_METHODS[2]}" "$HTTP_CODE" "expected 401 — session enforcement is NOT working"
         FAIL=1 ;;
esac

# ── 4. Same exposure signal on nginx's second proxy path ─────────────
rpc_call "/rpc/" "${READONLY_METHODS[1]}" "null"
case "$HTTP_CODE" in
    200) verdict "$C_YEL" "EXPOSED" "${READONLY_METHODS[1]} (/rpc/)" "$HTTP_CODE" "alternate proxy path also reachable" ;;
    000) verdict "$C_YEL" "UNREACHABLE" "${READONLY_METHODS[1]} (/rpc/)" "---" "no answer: ${RESP_BODY:0:120}" ;;
    429) verdict "$C_YEL" "RATELIMIT" "${READONLY_METHODS[1]} (/rpc/)" "$HTTP_CODE" "rate limited — rerun later" ;;
    *)   verdict "$C_GRN" "NOT-EXPOSED" "${READONLY_METHODS[1]} (/rpc/)" "$HTTP_CODE" "alternate proxy path did not answer 200" ;;
esac

# ── 5. KEY-01 refusal check — the ONE mutating branch ────────────────
if [ "$DESTRUCTIVE" = "1" ]; then
    echo
    echo "${C_RED}================================================================${C_OFF}"
    echo "${C_RED} DESTRUCTIVE MODE — this issues a REAL seed.restore.${C_OFF}"
    echo "${C_RED} Against a node WITHOUT 10-01's gate this REPLACES that node's${C_OFF}"
    echo "${C_RED} identity (node_key, nostr_secret, fips_key). DISPOSABLE NODES ONLY.${C_OFF}"
    echo "${C_RED}================================================================${C_OFF}"
    echo
    echo "Capture the identity digest ON the node before and after this run:"
    echo "  sudo sha256sum /var/lib/archipelago/identity/node_key /var/lib/archipelago/identity/nostr_secret"
    echo

    rpc_call "/rpc/v1" "seed.restore" "{\"words\":${TEST_MNEMONIC_WORDS}}"
    case "$HTTP_CODE" in
        000)
            verdict "$C_YEL" "UNREACHABLE" "seed.restore" "---" "no answer: ${RESP_BODY:0:120}"
            ;;
        429)
            verdict "$C_YEL" "RATELIMIT" "seed.restore" "$HTTP_CODE" "rate limited — INCONCLUSIVE, this is not a refusal"
            ;;
        *)
            if printf '%s' "$RESP_BODY" | grep -q "$REFUSAL_PREFIX"; then
                verdict "$C_GRN" "REFUSED" "seed.restore" "$HTTP_CODE" "gate refused with the expected '${REFUSAL_PREFIX}' prefix"
            elif printf '%s' "$RESP_BODY" | grep -q '"result"[[:space:]]*:[[:space:]]*[^n]'; then
                verdict "$C_RED" "ACCEPTED" "seed.restore" "$HTTP_CODE" "IDENTITY WAS REPLACED — the gate is absent or bypassed"
                FAIL=1
            else
                verdict "$C_RED" "UNKNOWN" "seed.restore" "$HTTP_CODE" "neither the refusal prefix nor a result — inspect manually"
                FAIL=1
            fi
            ;;
    esac
    echo
    echo "Response body (verbatim, for the evidence record):"
    printf '  %s\n' "${RESP_BODY:0:600}"
    echo
    echo "Now re-run the digest command ON the node. The two outputs MUST match:"
    echo "  sudo sha256sum /var/lib/archipelago/identity/node_key /var/lib/archipelago/identity/nostr_secret"
fi

echo
if [ "$FAIL" = "0" ]; then
    echo "${C_GRN}All controls behaved as expected.${C_OFF} (An EXPOSED verdict is a recorded"
    echo "finding, not a control failure — that is what C-6 exists to measure.)"
else
    echo "${C_RED}A control FAILED — see the CRITICAL/ACCEPTED/UNKNOWN line above.${C_OFF}"
fi
exit "$FAIL"
