#!/bin/bash
# Regression harness for the first-boot per-device secret regeneration script
# (audit finding F-03, phase 10 / KEY-02).
#
# What this pins, and why it exists at all: the script used to `touch` its
# completion marker unconditionally, outside both success branches, so one
# transient failure at first boot left the node running the ISO-wide shared
# SSH host key and TLS private key forever, silently. The property that must
# never regress is therefore negative — "on failure the marker is NOT created"
# — and a negative property is only assertable if the failure can be forced.
# So the generators are stubbed and the script is driven against a temp root
# through the FIRST_BOOT_SECRETS_ROOT seam.
#
# The script under test is not a file in this repo: it is a heredoc inside
# image-recipe/_archived/build-auto-installer-iso.sh (which is LIVE —
# image-recipe/build-debian-iso.sh execs it). The harness extracts the heredoc
# body between the SECRETSSCRIPT delimiters so it is testing the bytes that
# actually ship, not a copy that can drift.
#
# Usage:  bash tests/first-boot-secrets/run-tests.sh
# Exit 0 only if all three cases PASS.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILDER="$REPO/image-recipe/_archived/build-auto-installer-iso.sh"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

PASS_COUNT=0
FAIL_COUNT=0

ok()   { echo "PASS: $1"; PASS_COUNT=$((PASS_COUNT + 1)); }
bad()  { echo "FAIL: $1"; FAIL_COUNT=$((FAIL_COUNT + 1)); }

# ── Step 0: extract the script under test and syntax-check it ────────────
[ -f "$BUILDER" ] || { echo "FAIL: builder not found at $BUILDER"; exit 1; }

SCRIPT="$WORK/first-boot-secrets.sh"
awk '/^cat > "\$WORK_DIR\/first-boot-secrets.sh" <<.SECRETSSCRIPT.$/ { f = 1; next }
     f && /^SECRETSSCRIPT$/                                          { f = 0 }
     f                                                               { print }' \
    "$BUILDER" > "$SCRIPT"

if [ ! -s "$SCRIPT" ]; then
    echo "FAIL: could not extract the first-boot-secrets.sh heredoc from the builder"
    echo "      (did the SECRETSSCRIPT delimiter or the cat> line change?)"
    exit 1
fi
chmod +x "$SCRIPT"

if bash -n "$SCRIPT"; then
    echo "extracted $(wc -l < "$SCRIPT") lines from the builder; bash -n clean"
else
    echo "FAIL: extracted script does not parse"
    exit 1
fi

# ── Stubs ─────────────────────────────────────────────────────────────────
# A stub dir is prepended to PATH so the script's openssl / ssh-keygen /
# systemctl / logger calls hit these instead of the real tools. Behaviour is
# driven by env vars the stubs read at call time.
#
#   STUB_OPENSSL_MODE            ok | fail        (fail affects `req` only —
#                                                  `pkey`/`x509` validation still
#                                                  works, so a failed generation
#                                                  cannot be mistaken for a failed
#                                                  validation)
#   STUB_SSHKEYGEN_MODE          ok | fail | fail-twice   (uses a counter file)
#   STUB_OPENSSL_MISMATCH        yes | no   — emit a cert whose public key is
#                                             from a DIFFERENT generation than
#                                             the key beside it. Both halves
#                                             still parse individually; only a
#                                             pair check catches it.
#   STUB_COUNTER_DIR             where the counter file lives
#   STUB_SYSTEMCTL_FAILED_UNITS  units `systemctl is-failed` should report failed
#   STUB_SYSTEMCTL_LOG           file the systemctl stub appends its args to
make_stubs() {
    local dir="$1"
    mkdir -p "$dir"

    cat > "$dir/openssl" <<'STUB'
#!/bin/bash
# Stub openssl. Honours -keyout/-out so the script's staging-then-swap and its
# non-empty checks are exercised for real, and implements the `pkey`/`x509`
# parse-back validation the generator does before it swaps.
sub="${1:-}"

# Capability probe. The script asks `openssl req -help` whether it can backdate.
if [ "$sub" = "req" ] && [ "${2:-}" = "-help" ]; then
    [ "${STUB_OPENSSL_NOT_BEFORE:-yes}" = "yes" ] && echo " -not_before val    stub"
    exit 0
fi

case "$sub" in
    pkey)
        f=""; pubout=0
        while [ $# -gt 0 ]; do
            case "$1" in
                -in)     f="$2"; shift 2 ;;
                -pubout) pubout=1; shift ;;
                *)       shift ;;
            esac
        done
        [ -n "$f" ] && [ -s "$f" ] || exit 1
        if [ "$pubout" = 1 ]; then
            # The stub keypair carries the generation it came from; printing it
            # as the "public key" is what lets the harness express a mismatched
            # key/cert pair at all.
            p=$(sed -n 's/^STUB_PUB=//p' "$f"); [ -n "$p" ] || exit 1
            printf -- '-----BEGIN PUBLIC KEY-----\nstubpub-%s\n-----END PUBLIC KEY-----\n' "$p"
        fi
        exit 0
        ;;
    x509)
        # Validation (-noout -in f) plus date readback. The stub cert carries
        # the epochs it was minted with, so the harness can drive the script's
        # date arithmetic without a real certificate.
        f=""; want_start=0; want_end=0; want_pub=0
        while [ $# -gt 0 ]; do
            case "$1" in
                -in)        f="$2"; shift 2 ;;
                -startdate) want_start=1; shift ;;
                -enddate)   want_end=1; shift ;;
                -pubkey)    want_pub=1; shift ;;
                *)          shift ;;
            esac
        done
        [ -n "$f" ] && [ -s "$f" ] || exit 1
        if [ "$want_pub" = 1 ]; then
            p=$(sed -n 's/^STUB_PUB=//p' "$f"); [ -n "$p" ] || exit 1
            printf -- '-----BEGIN PUBLIC KEY-----\nstubpub-%s\n-----END PUBLIC KEY-----\n' "$p"
        fi
        if [ "$want_start" = 1 ] || [ "$want_end" = 1 ]; then
            nb=$(sed -n 's/^STUB_NOTBEFORE=//p' "$f"); na=$(sed -n 's/^STUB_NOTAFTER=//p' "$f")
            [ -n "$nb" ] && [ -n "$na" ] || exit 1
            [ "$want_start" = 1 ] && echo "notBefore=$(date -u -d "@$nb" '+%b %e %H:%M:%S %Y GMT')"
            [ "$want_end" = 1 ]   && echo "notAfter=$(date -u -d "@$na" '+%b %e %H:%M:%S %Y GMT')"
        fi
        exit 0
        ;;
esac

# Count real mints. Comparing certificate dates cannot detect a re-mint when
# the clock is frozen — the second cert carries the same notBefore — so the
# anti-spin assertions count invocations instead.
reqcount="${STUB_COUNTER_DIR:-/tmp}/openssl-req.count"
rn=$(cat "$reqcount" 2>/dev/null || echo 0)
echo $((rn + 1)) > "$reqcount"

[ "${STUB_OPENSSL_MODE:-ok}" = "fail" ] && exit 1
keyout="" out="" nb="" na="" days=""
while [ $# -gt 0 ]; do
    case "$1" in
        -keyout)     keyout="$2"; shift 2 ;;
        -out)        out="$2";    shift 2 ;;
        -not_before) nb="$2";     shift 2 ;;
        -not_after)  na="$2";     shift 2 ;;
        -days)       days="$2";   shift 2 ;;
        *)           shift ;;
    esac
done
# Mirror openssl: -not_before/-not_after win; otherwise notBefore is "now" and
# notAfter is now + days. "now" honours the harness's fake clock.
now="${FIRST_BOOT_SECRETS_NOW:-$(date -u +%s)}"
# YYYYMMDDHHMMSSZ -> something GNU date can parse
asn1() { printf '%s' "$1" | sed -E 's/^([0-9]{4})([0-9]{2})([0-9]{2})([0-9]{2})([0-9]{2})([0-9]{2})Z?$/\1-\2-\3 \4:\5:\6 UTC/'; }
if [ -n "$nb" ]; then nb_epoch=$(date -u -d "$(asn1 "$nb")" +%s 2>/dev/null || echo "$now"); else nb_epoch="$now"; fi
if [ -n "$na" ]; then na_epoch=$(date -u -d "$(asn1 "$na")" +%s 2>/dev/null || echo $((now + 315360000))); else na_epoch=$((now + ${days:-3650} * 86400)); fi
# Each generation gets its own public-key identity. STUB_OPENSSL_MISMATCH makes
# the cert carry a different one, i.e. a cert from another generation beside
# this key — the pair that passes both individual parse checks and still breaks
# nginx.
gen_id=$((rn + 1))
key_pub="$gen_id"
crt_pub="$gen_id"
[ "${STUB_OPENSSL_MISMATCH:-no}" = "yes" ] && crt_pub="$((gen_id + 1000))"

[ -n "$keyout" ] && printf -- '-----BEGIN PRIVATE KEY-----\nstub\nSTUB_PUB=%s\n-----END PRIVATE KEY-----\n' "$key_pub" > "$keyout"
[ -n "$out" ] && printf -- '-----BEGIN CERTIFICATE-----\nstub\nSTUB_PUB=%s\nSTUB_NOTBEFORE=%s\nSTUB_NOTAFTER=%s\n-----END CERTIFICATE-----\n' "$crt_pub" "$nb_epoch" "$na_epoch" > "$out"
exit 0
STUB

    cat > "$dir/ssh-keygen" <<'STUB'
#!/bin/bash
# Stub ssh-keygen -A: writes a host-key set into <-f dir>/etc/ssh, matching
# the real tool's layout, which is what the script globs for.
mode="${STUB_SSHKEYGEN_MODE:-ok}"
counter="${STUB_COUNTER_DIR:-/tmp}/ssh-keygen.count"

n=$(cat "$counter" 2>/dev/null || echo 0)
n=$((n + 1))
echo "$n" > "$counter"

case "$mode" in
    fail)       exit 1 ;;
    fail-twice) [ "$n" -le 2 ] && exit 1 ;;
esac

root=""
while [ $# -gt 0 ]; do
    case "$1" in
        -f) root="$2"; shift 2 ;;
        *)  shift ;;
    esac
done
[ -n "$root" ] || exit 1
mkdir -p "$root/etc/ssh"
for t in rsa ecdsa ed25519; do
    printf -- '-----BEGIN OPENSSH PRIVATE KEY-----\nstub-%s\n' "$t" > "$root/etc/ssh/ssh_host_${t}_key"
    printf -- 'ssh-%s AAAAstub stub@archipelago\n' "$t"            > "$root/etc/ssh/ssh_host_${t}_key.pub"
done
exit 0
STUB

    cat > "$dir/systemctl" <<'STUB'
#!/bin/bash
# Stub systemctl. Records every invocation so the harness can prove the
# self-heal path actually restarts a unit that failed for want of a key, and
# reports is-failed honestly so first-boot and self-heal take different paths.
[ -n "${STUB_SYSTEMCTL_LOG:-}" ] && echo "$*" >> "$STUB_SYSTEMCTL_LOG"
if [ "${1:-}" = "is-failed" ]; then
    unit="${!#}"
    for u in ${STUB_SYSTEMCTL_FAILED_UNITS:-}; do
        [ "$u" = "$unit" ] && exit 0
    done
    exit 1
fi
exit 0
STUB

    # Must not be allowed to touch the host journal during a test run.
    printf '#!/bin/bash\nexit 0\n' > "$dir/logger"

    chmod +x "$dir"/openssl "$dir"/ssh-keygen "$dir"/systemctl "$dir"/logger
}

STUBS="$WORK/stubs"
make_stubs "$STUBS"

# ── Runner ────────────────────────────────────────────────────────────────
# run_case <name> <openssl_mode> <sshkeygen_mode> [prestage] [reuse]
#
#   prestage  baked    — root already holds the shared keys, i.e. a pre-strip
#                        rootfs. Assertions can then prove the swap replaced them.
#             stripped — root holds no key material at all, i.e. the rootfs this
#                        build actually ships. This is the state in which "no key
#                        may appear from anywhere but the generator" is testable.
#   reuse     1        — do not wipe the root or the counters; continue from the
#                        previous run against the same node. Models a reboot or a
#                        timer-triggered retry.
CASE_ROOT=""
CASE_RC=0
CASE_SYSTEMCTL_LOG=""
run_case() {
    local name="$1" openssl_mode="$2" sshkeygen_mode="$3"
    local prestage="${4:-baked}" reuse="${5:-0}"
    CASE_ROOT="$WORK/root-$name"
    CASE_SYSTEMCTL_LOG="$WORK/$name.systemctl"

    if [ "$reuse" != "1" ]; then
        rm -rf "$CASE_ROOT"
        mkdir -p "$CASE_ROOT/var/lib/archipelago" "$CASE_ROOT/var/log" \
                 "$CASE_ROOT/etc/ssh" "$CASE_ROOT/etc/archipelago/ssl"
        if [ "$prestage" = "baked" ]; then
            echo "BAKED-SHARED-HOST-KEY" > "$CASE_ROOT/etc/ssh/ssh_host_rsa_key"
            echo "BAKED-SHARED-TLS-KEY"  > "$CASE_ROOT/etc/archipelago/ssl/archipelago.key"
        fi
        rm -f "$WORK/counters-$name/ssh-keygen.count"
        mkdir -p "$WORK/counters-$name"
        : > "$CASE_SYSTEMCTL_LOG"
    fi

    set +e
    env PATH="$STUBS:$PATH" \
        FIRST_BOOT_SECRETS_ROOT="$CASE_ROOT" \
        FIRST_BOOT_SECRETS_BACKOFF="0 0 0" \
        STUB_OPENSSL_MODE="$openssl_mode" \
        STUB_SSHKEYGEN_MODE="$sshkeygen_mode" \
        STUB_COUNTER_DIR="$WORK/counters-$name" \
        STUB_SYSTEMCTL_LOG="$CASE_SYSTEMCTL_LOG" \
        STUB_SYSTEMCTL_FAILED_UNITS="${STUB_SYSTEMCTL_FAILED_UNITS:-}" \
        STUB_OPENSSL_NOT_BEFORE="${STUB_OPENSSL_NOT_BEFORE:-yes}" \
        STUB_OPENSSL_MISMATCH="${STUB_OPENSSL_MISMATCH:-no}" \
        FIRST_BOOT_SECRETS_NOW="${FIRST_BOOT_SECRETS_NOW:-}" \
        bash "$SCRIPT" > "$WORK/$name.out" 2> "$WORK/$name.err"
    CASE_RC=$?
    set -e
}

fail_detail() {
    echo "    exit=$CASE_RC root=$CASE_ROOT"
    echo "    stderr: $(head -c 300 "$WORK/$1.err" 2>/dev/null)"
}

# ── Case 1: both generators succeed ──────────────────────────────────────
run_case both-ok ok ok
c1=""
[ "$CASE_RC" -eq 0 ] || c1="$c1 exit-nonzero"
[ -f "$CASE_ROOT/var/lib/archipelago/.secrets-regenerated" ] || c1="$c1 marker-missing"
[ -f "$CASE_ROOT/var/lib/archipelago/first-boot-secrets.failed" ] && c1="$c1 stale-failure-record"
[ -s "$CASE_ROOT/etc/archipelago/ssl/archipelago.key" ] || c1="$c1 tls-key-missing"
[ -s "$CASE_ROOT/etc/archipelago/ssl/archipelago.crt" ] || c1="$c1 tls-crt-missing"
grep -q BAKED-SHARED-TLS-KEY "$CASE_ROOT/etc/archipelago/ssl/archipelago.key" && c1="$c1 tls-key-not-replaced"
[ -s "$CASE_ROOT/etc/ssh/ssh_host_ed25519_key" ] || c1="$c1 ssh-host-key-missing"
grep -q BAKED-SHARED-HOST-KEY "$CASE_ROOT/etc/ssh/ssh_host_rsa_key" && c1="$c1 ssh-key-not-replaced"
ls "$CASE_ROOT"/etc/archipelago/ssl/*.new >/dev/null 2>&1 && c1="$c1 dotnew-leftover"
if getent passwd archipelago >/dev/null 2>&1; then
    [ "$(stat -c '%a:%g' "$CASE_ROOT/etc/archipelago/ssl/archipelago.key")" = "640:$(id -g archipelago)" ] || c1="$c1 app-gate-cannot-read-private-key"
fi
if [ -z "$c1" ]; then
    ok "both generators succeed -> exit 0, marker set, keys swapped in"
else
    bad "both generators succeed ->$c1"; fail_detail both-ok
fi

# ── Case 2: openssl fails every attempt -> FAIL CLOSED ───────────────────
# This is the case that would have passed against the old script and is the
# whole reason this harness exists: the old code logged a warning and set the
# marker anyway.
run_case tls-fail fail ok
c2=""
[ "$CASE_RC" -ne 0 ] || c2="$c2 exit-zero-on-failure"
[ -f "$CASE_ROOT/var/lib/archipelago/.secrets-regenerated" ] && c2="$c2 MARKER-SET-ON-FAILURE"
[ -f "$CASE_ROOT/var/lib/archipelago/first-boot-secrets.failed" ] || c2="$c2 no-failure-record"
grep -q 'failed=.*TLS' "$CASE_ROOT/var/lib/archipelago/first-boot-secrets.failed" 2>/dev/null \
    || c2="$c2 failure-record-does-not-name-TLS"
ls "$CASE_ROOT"/etc/archipelago/ssl/*.new >/dev/null 2>&1 && c2="$c2 dotnew-leftover"
grep -qi 'FAILED' "$WORK/tls-fail.err" || c2="$c2 no-loud-stderr"
if [ -z "$c2" ]; then
    ok "openssl fails every attempt -> exit non-zero, NO marker, failure record names TLS"
else
    bad "openssl fails every attempt ->$c2"; fail_detail tls-fail
fi

# ── Case 3: ssh-keygen fails twice then succeeds -> backoff recovers ─────
run_case ssh-flaky ok fail-twice
c3=""
[ "$CASE_RC" -eq 0 ] || c3="$c3 exit-nonzero"
[ -f "$CASE_ROOT/var/lib/archipelago/.secrets-regenerated" ] || c3="$c3 marker-missing"
[ -f "$CASE_ROOT/var/lib/archipelago/first-boot-secrets.failed" ] && c3="$c3 failure-record-present"
[ -s "$CASE_ROOT/etc/ssh/ssh_host_ed25519_key" ] || c3="$c3 ssh-host-key-missing"
attempts=$(cat "$WORK/counters-ssh-flaky/ssh-keygen.count" 2>/dev/null || echo 0)
[ "$attempts" -eq 3 ] || c3="$c3 expected-3-attempts-got-$attempts"
if [ -z "$c3" ]; then
    ok "ssh-keygen fails twice then succeeds -> backoff recovers within one boot (3 attempts)"
else
    bad "ssh-keygen fails twice then succeeds ->$c3"; fail_detail ssh-flaky
fi

# ── Case 4: TLS fails every attempt on a STRIPPED root -> no key at all ──
# Case 2 proves the marker is not set. This proves the stronger property that
# replaced the installer's TLS fallback: on the rootfs we actually ship, a
# failed generation leaves NO key, from any source. If anything ever mints a
# key outside gen_tls — an install-time fallback, a placeholder, a zero-length
# touch to keep nginx happy — this is the case that goes red.
run_case tls-fail-stripped fail ok stripped
c4=""
[ "$CASE_RC" -ne 0 ] || c4="$c4 exit-zero-on-failure"
[ -f "$CASE_ROOT/var/lib/archipelago/.secrets-regenerated" ] && c4="$c4 MARKER-SET-ON-FAILURE"
[ -e "$CASE_ROOT/etc/archipelago/ssl/archipelago.key" ] && c4="$c4 TLS-KEY-EXISTS-AFTER-FAILURE"
[ -e "$CASE_ROOT/etc/archipelago/ssl/archipelago.crt" ] && c4="$c4 TLS-CRT-EXISTS-AFTER-FAILURE"
[ -f "$CASE_ROOT/var/lib/archipelago/first-boot-secrets.failed" ] || c4="$c4 no-failure-record"
grep -q 'failed=.*TLS' "$CASE_ROOT/var/lib/archipelago/first-boot-secrets.failed" 2>/dev/null \
    || c4="$c4 failure-record-does-not-name-TLS"
ls "$CASE_ROOT"/etc/archipelago/ssl/*.new >/dev/null 2>&1 && c4="$c4 dotnew-leftover"
if [ -z "$c4" ]; then
    ok "TLS fails every attempt on a stripped root -> NO key, NO marker, non-zero exit, record names TLS"
else
    bad "TLS fails every attempt on a stripped root ->$c4"; fail_detail tls-fail-stripped
fi

# ── Case 5: self-heal — a failed run, then a later run that succeeds ─────
# The case that proves a node is not permanently dead. Run 1 is a machine whose
# generator fails every retry; run 2 is the same machine minutes later, once the
# transient cause cleared, driven by archipelago-first-boot-secrets.timer. It
# must end with the key present and the marker set, with no human at a console.
# Run 2 also declares nginx/ssh already `failed` — they tried to start without a
# key — so the run must actively restart them, not just reload. A "recovery"
# that leaves the services down is not a recovery.
#
# Run 1 asserts only enough to establish the precondition (it really did fail,
# and it left the node eligible to retry). Whether a key exists after a failure
# is case 4's job — asserting it here too would make a single defect light up
# two cases and blunt the signal.
run_case self-heal fail ok stripped
c5=""
[ "$CASE_RC" -ne 0 ] || c5="$c5 run1-exit-zero"
[ -f "$CASE_ROOT/var/lib/archipelago/.secrets-regenerated" ] && c5="$c5 run1-marker-set"
[ -f "$CASE_ROOT/var/lib/archipelago/first-boot-secrets.failed" ] || c5="$c5 run1-no-failure-record"

STUB_SYSTEMCTL_FAILED_UNITS="nginx ssh" run_case self-heal ok ok stripped 1
[ "$CASE_RC" -eq 0 ] || c5="$c5 run2-exit-nonzero"
[ -f "$CASE_ROOT/var/lib/archipelago/.secrets-regenerated" ] || c5="$c5 run2-marker-missing"
[ -s "$CASE_ROOT/etc/archipelago/ssl/archipelago.key" ] || c5="$c5 run2-key-missing"
[ -s "$CASE_ROOT/etc/archipelago/ssl/archipelago.crt" ] || c5="$c5 run2-crt-missing"
[ -s "$CASE_ROOT/etc/ssh/ssh_host_ed25519_key" ] || c5="$c5 run2-ssh-key-missing"
[ -f "$CASE_ROOT/var/lib/archipelago/first-boot-secrets.failed" ] && c5="$c5 run2-stale-failure-record"
grep -q 'restart nginx' "$CASE_SYSTEMCTL_LOG" 2>/dev/null || c5="$c5 run2-did-not-restart-failed-nginx"
if [ -z "$c5" ]; then
    ok "self-heal: failed run then a later successful run -> key present, marker set, failed units restarted"
else
    bad "self-heal ->$c5"; fail_detail self-heal
fi

# ── Case 6: single-producer invariant ────────────────────────────────────
# The regression that would silently recreate F-03 is not a broken assertion —
# it is somebody adding a second, well-meaning place that mints a key. A second
# producer brings its own idea of success, its own absent retry policy and its
# own absent failure record, and that is what made F-03 silent.
#
# So: every executable key-creating invocation in the builder must live inside
# the first-boot-secrets.sh heredoc, i.e. inside gen_tls/gen_ssh. Comments are
# exempt (they discuss the history); binary-existence checks are not matched
# because they do not carry a key-creating subcommand.
c6=""
SS_START=$(grep -n '^cat > "\$WORK_DIR/first-boot-secrets.sh" <<.SECRETSSCRIPT.$' "$BUILDER" | cut -d: -f1)
SS_END=$(awk -v s="$SS_START" 'NR>s && /^SECRETSSCRIPT$/ { print NR; exit }' "$BUILDER")
if [ -z "$SS_START" ] || [ -z "$SS_END" ]; then
    c6="$c6 could-not-locate-generator-heredoc"
else
    PRODUCERS=$(grep -nE 'openssl[[:space:]]+req|ssh-keygen[[:space:]]+-A|ssh-keygen[[:space:]]+-t' "$BUILDER" \
                | grep -vE '^[0-9]+:[[:space:]]*#' || true)
    while IFS= read -r line; do
        [ -z "$line" ] && continue
        ln=${line%%:*}
        if [ "$ln" -lt "$SS_START" ] || [ "$ln" -gt "$SS_END" ]; then
            c6="$c6 SECOND-PRODUCER-at-line-$ln"
        fi
    done <<< "$PRODUCERS"
    # Sanity: the one producer we expect must actually be in there, otherwise an
    # empty result would pass this case vacuously.
    echo "$PRODUCERS" | grep -q 'openssl[[:space:]]*req' || c6="$c6 no-tls-producer-found-at-all"
    echo "$PRODUCERS" | grep -q 'ssh-keygen' || c6="$c6 no-ssh-producer-found-at-all"
fi
if [ -z "$c6" ]; then
    ok "single-producer invariant: every key-creating invocation is inside gen_tls/gen_ssh"
else
    bad "single-producer invariant ->$c6"
    echo "    generator heredoc spans lines $SS_START-$SS_END of $BUILDER"
fi

# ── Case 7: the Dockerfile heredoc delimiter must be quoted ──────────────
# Lives in this harness rather than a sibling because it guards the same file
# and the same failure mode the rest of these cases exist for: a build-side
# defect that is invisible to `bash -n` and only shows up as damage on a build
# host. Splitting it into its own runner would mean two commands to remember
# and one of them getting skipped.
#
# The bug: `cat > ... <<DOCKERFILE` (unquoted) makes the build shell perform
# command substitution on the Dockerfile body, so a backtick inside a COMMENT
# is executed on the build host and its output spliced into the Dockerfile.
# Six comments did exactly that, and one of them ran `systemctl start
# archipelago-fips.service` against the build machine on every ISO build. The
# comment text was silently deleted from the generated Dockerfile too.
#
# The assertion is on the DELIMITER, not on backticks. With a quoted delimiter
# a backticked comment is inert and perfectly legal — six of them are back in
# the body on purpose. Flagging backticks would be flagging a non-bug, and
# would fail on the very comments this fix restored. Quoting is the fix;
# vigilance about backticks is not.
c7=""
DF_HEREDOCS=$(grep -nE 'cat >>? "\$WORK_DIR/Dockerfile\.rootfs" <<' "$BUILDER" || true)
if [ -z "$DF_HEREDOCS" ]; then
    c7="$c7 no-dockerfile-heredoc-found"
else
    while IFS= read -r hd; do
        [ -z "$hd" ] && continue
        ln=${hd%%:*}
        delim=$(printf '%s' "$hd" | sed -E 's/.*<<-?[[:space:]]*//')
        case "$delim" in
            \'*\'|\"*\")
                : ;;   # quoted — the body is emitted verbatim, nothing executes
            *)
                c7="$c7 UNQUOTED-DELIMITER-at-line-$ln"
                # Only meaningful when unquoted: report what would actually run.
                bare=$(printf '%s' "$delim" | tr -d "\"'")
                endln=$(awk -v s="$ln" -v d="$bare" 'NR>s && $0==d { print NR; exit }' "$BUILDER")
                if [ -n "$endln" ]; then
                    subs=$(awk -v s="$ln" -v e="$endln" 'NR>s && NR<e && (/`/ || /\$\(/) { print NR }' "$BUILDER" | tr '\n' ',')
                    [ -n "$subs" ] && c7="$c7 would-execute-at-lines:${subs%,}"
                fi
                ;;
        esac
    done <<< "$DF_HEREDOCS"
fi
if [ -z "$c7" ]; then
    ok "Dockerfile heredoc delimiters are quoted — a backticked comment cannot execute"
else
    bad "Dockerfile heredoc quoting ->$c7"
fi

# ── Case 8: a cert minted under a wrong clock is detected and repaired ───
# The failure fail-closed cannot catch, because generation SUCCEEDS. This unit
# runs before chrony has corrected the clock; on a node with a dead RTC,
# `openssl req -x509` stamps a notBefore years out. Clock ahead -> clients
# reject the cert as "not yet valid"; clock behind -> notAfter is already in
# the past once time syncs. The old code would have marked the node done and
# never revisited it.
#
# Run 1 mints under a clock set to 2013 (a classic dead-RTC value). The node
# must still be usable — keys installed, marker set, exit 0 — but the bad dates
# must be recorded, not blessed.
# Run 2 is the same node after chrony fixes the clock: the cert must be
# regenerated with sane dates and the record cleared, with nobody at a console.
# Run 3 proves the anti-spin guard: a third run changes nothing.
BAD_CLOCK=1370000000        # 2013-06-01, i.e. a dead RTC
GOOD_CLOCK=1785000000       # 2026-07-25, inside the plausible window
c8=""

FIRST_BOOT_SECRETS_NOW="$BAD_CLOCK" run_case clock ok ok stripped
[ "$CASE_RC" -eq 0 ] || c8="$c8 run1-exit-nonzero"
[ -s "$CASE_ROOT/etc/archipelago/ssl/archipelago.crt" ] || c8="$c8 run1-no-cert-node-unusable"
[ -s "$CASE_ROOT/etc/ssh/ssh_host_ed25519_key" ] || c8="$c8 run1-no-ssh-key"
[ -f "$CASE_ROOT/var/lib/archipelago/.secrets-regenerated" ] || c8="$c8 run1-marker-missing"
grep -q 'failed=cert-dates' "$CASE_ROOT/var/lib/archipelago/first-boot-secrets.failed" 2>/dev/null \
    || c8="$c8 run1-BAD-DATES-NOT-RECORDED"
run1_nb=$(sed -n 's/^STUB_NOTBEFORE=//p' "$CASE_ROOT/etc/archipelago/ssl/archipelago.crt" 2>/dev/null)

FIRST_BOOT_SECRETS_NOW="$GOOD_CLOCK" run_case clock ok ok stripped 1
[ "$CASE_RC" -eq 0 ] || c8="$c8 run2-exit-nonzero"
run2_nb=$(sed -n 's/^STUB_NOTBEFORE=//p' "$CASE_ROOT/etc/archipelago/ssl/archipelago.crt" 2>/dev/null)
[ -n "$run2_nb" ] || c8="$c8 run2-cert-unreadable"
[ "$run2_nb" != "$run1_nb" ] || c8="$c8 CERT-NOT-REGENERATED-AFTER-CLOCK-FIX"
if [ -n "$run2_nb" ]; then
    [ "$run2_nb" -ge 1767225600 ] || c8="$c8 run2-notBefore-still-below-floor"
    # backdated, but not into the implausible past
    [ "$run2_nb" -le "$GOOD_CLOCK" ] || c8="$c8 run2-notBefore-in-the-future"
    [ "$run2_nb" -lt "$GOOD_CLOCK" ] || c8="$c8 run2-notBefore-not-backdated"
fi
[ -f "$CASE_ROOT/var/lib/archipelago/first-boot-secrets.failed" ] && c8="$c8 run2-stale-bad-date-record"

# Anti-spin. Counted, not date-compared: with a frozen clock a re-mint produces
# a byte-identical notBefore, so dates cannot tell "left alone" from
# "regenerated again". Counting mints is the only assertion that distinguishes
# them — the first version of this check compared dates and sailed straight
# past a deliberately broken anti-spin guard.
mints() { cat "$WORK/counters-$1/openssl-req.count" 2>/dev/null || echo 0; }

# A further run with a good clock and a good cert must NOT mint again.
before3=$(mints clock)
FIRST_BOOT_SECRETS_NOW="$GOOD_CLOCK" run_case clock ok ok stripped 1
[ "$(mints clock)" -eq "$before3" ] || c8="$c8 SPINNING-reminted-a-good-cert"

# And a node whose clock stays wrong must not mint a fresh bad cert on every
# timer tick — the loop the fix must not introduce.
FIRST_BOOT_SECRETS_NOW="$BAD_CLOCK" run_case clockstuck ok ok stripped
stuck1=$(mints clockstuck)
FIRST_BOOT_SECRETS_NOW="$BAD_CLOCK" run_case clockstuck ok ok stripped 1
stuck2=$(mints clockstuck)
[ "$stuck2" -eq "$stuck1" ] || c8="$c8 SPINNING-reminted-while-clock-still-wrong($stuck1->$stuck2)"

if [ -z "$c8" ]; then
    ok "wrong clock: cert flagged not blessed, regenerated once time syncs, and no spin either way"
else
    bad "wrong clock ->$c8"; fail_detail clock
fi

# ── Case 9: a key and a cert that are not a pair ─────────────────────────
# The second failure that generation-succeeded hides. Parsing each half back
# proves each is well-formed, never that they belong together; a key from one
# generation beside a cert from another passes both individual parse checks,
# gets blessed, and then nginx refuses to start at the exact moment the marker
# claims first-boot succeeded. Only comparing the two public keys catches it.
c9=""

# 9a — the mismatch arises during generation: fail closed, exactly like any
# other TLS failure. Nothing installed, nothing blessed, no .new left behind.
STUB_OPENSSL_MISMATCH=yes run_case mismatch-gen ok ok stripped
[ "$CASE_RC" -ne 0 ] || c9="$c9 gen-exit-zero-on-mismatched-pair"
[ -f "$CASE_ROOT/var/lib/archipelago/.secrets-regenerated" ] && c9="$c9 gen-MARKER-SET-ON-MISMATCH"
[ -e "$CASE_ROOT/etc/archipelago/ssl/archipelago.key" ] && c9="$c9 gen-MISMATCHED-KEY-INSTALLED"
[ -e "$CASE_ROOT/etc/archipelago/ssl/archipelago.crt" ] && c9="$c9 gen-MISMATCHED-CRT-INSTALLED"
[ -f "$CASE_ROOT/var/lib/archipelago/first-boot-secrets.failed" ] || c9="$c9 gen-no-failure-record"
grep -q 'failed=.*TLS' "$CASE_ROOT/var/lib/archipelago/first-boot-secrets.failed" 2>/dev/null \
    || c9="$c9 gen-failure-record-does-not-name-TLS"
ls "$CASE_ROOT"/etc/archipelago/ssl/*.new >/dev/null 2>&1 && c9="$c9 gen-dotnew-leftover"

# 9b — the mismatch is already on disk from somewhere else: an older build, a
# half-finished manual edit. The node is "done" by every marker, so only the
# needs_tls pair check can notice. One regeneration must repair it.
run_case mismatch-disk ok ok stripped
[ "$CASE_RC" -eq 0 ] || c9="$c9 disk-setup-run-failed"
disk_crt="$CASE_ROOT/etc/archipelago/ssl/archipelago.crt"
# Swap in a cert from a different generation, leaving the dates untouched so
# this can only trip the pair check and not the clock check.
sed -i 's/^STUB_PUB=.*/STUB_PUB=999999/' "$disk_crt"
before9=$(mints mismatch-disk)
run_case mismatch-disk ok ok stripped 1
[ "$(mints mismatch-disk)" -gt "$before9" ] || c9="$c9 MISMATCHED-PAIR-ON-DISK-NOT-REPAIRED"
[ "$CASE_RC" -eq 0 ] || c9="$c9 disk-repair-exit-nonzero"
key_pub=$(sed -n 's/^STUB_PUB=//p' "$CASE_ROOT/etc/archipelago/ssl/archipelago.key" 2>/dev/null)
crt_pub=$(sed -n 's/^STUB_PUB=//p' "$disk_crt" 2>/dev/null)
[ -n "$key_pub" ] && [ "$key_pub" = "$crt_pub" ] || c9="$c9 pair-still-mismatched-after-repair($key_pub/$crt_pub)"

# Anti-spin: the repaired pair matches, so a further run must mint nothing.
before9b=$(mints mismatch-disk)
run_case mismatch-disk ok ok stripped 1
[ "$(mints mismatch-disk)" -eq "$before9b" ] || c9="$c9 SPINNING-reminted-a-matching-pair"

if [ -z "$c9" ]; then
    ok "mismatched key/cert: fails closed when generated, repaired once when found on disk, no spin"
else
    bad "mismatched key/cert ->$c9"; fail_detail mismatch-gen
fi

# ── Summary ───────────────────────────────────────────────────────────────
echo
echo "──────── first-boot-secrets summary ────────"
echo "passed: $PASS_COUNT   failed: $FAIL_COUNT"
[ "$FAIL_COUNT" -eq 0 ] || exit 1
exit 0
