#!/bin/bash
# Regression harness for scripts/security/host-secrets-audit.sh
# (audit finding F-03, phase 10 / KEY-02, deployed half — decision D-06).
#
# Sibling of run-tests.sh, which covers the ISO-build half. That one proves a
# node never SERVES on a key it did not generate. This one proves a node can
# TELL you whether it is already doing so, and can be fixed without losing the
# operator's session in the middle.
#
# The properties under test are mostly negative or ordering properties, and
# neither kind is assertable against real key material on a real node:
#
#   - "--apply without --yes touches nothing"      needs a tree to diff
#   - "old fingerprints are recorded BEFORE the swap"   needs the swap observed
#   - "a failed generation leaves the live keys byte-identical"  needs failure
#     to be forcible
#   - "a node with no anchor is reported unknown, never per-node"  needs a node
#     with no anchor to exist
#
# So the generators are stubbed and the script is driven against temp roots
# through its HOST_SECRETS_ROOT seam — the same move 10-03's harness makes with
# FIRST_BOOT_SECRETS_ROOT, and the same reason.
#
# Usage:  bash tests/first-boot-secrets/rotation-tests.sh
# Exit 0 only if all seven cases PASS.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$REPO/scripts/security/host-secrets-audit.sh"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

PASS_COUNT=0
FAIL_COUNT=0
ok()  { echo "PASS: $1"; PASS_COUNT=$((PASS_COUNT + 1)); }
bad() { echo "FAIL: $1"; FAIL_COUNT=$((FAIL_COUNT + 1)); }

[ -f "$SCRIPT" ] || { echo "FAIL: script not found at $SCRIPT"; exit 1; }
[ -x "$SCRIPT" ] || { echo "FAIL: $SCRIPT is not executable"; exit 1; }
if bash -n "$SCRIPT"; then
    echo "host-secrets-audit.sh: $(wc -l < "$SCRIPT") lines; bash -n clean"
else
    echo "FAIL: host-secrets-audit.sh does not parse"; exit 1
fi

# A rotation script that restarts sshd instead of reloading it disconnects the
# operator on a remote node with no console. Checked here rather than left to
# review, because it is a one-word edit away at all times.
if grep -qn 'systemctl restart ssh' "$SCRIPT"; then
    echo "FAIL: host-secrets-audit.sh contains 'systemctl restart ssh' — a restart kills the operator's own session"
    exit 1
fi
grep -q 'systemctl reload ssh' "$SCRIPT" || { echo "FAIL: no 'systemctl reload ssh' in the script"; exit 1; }
echo "sshd handling: reload present, restart absent"

# ── Stubs ─────────────────────────────────────────────────────────────────
# Prepended to PATH so openssl / ssh-keygen / systemctl calls land here.
#   STUB_OPENSSL_MODE     ok | fail    (affects `req` only, so a failed
#                                       generation is never mistaken for a
#                                       failed validation)
#   STUB_SSHKEYGEN_MODE   ok | fail    (affects `-A` only, so `-lf` keeps
#                                       working and old fingerprints can still
#                                       be read on the abort path)
#   STUB_COUNTER_DIR      where generation counters live
#   STUB_SYSTEMCTL_LOG    file the systemctl stub appends to
make_stubs() {
    local dir="$1"
    mkdir -p "$dir"

    cat > "$dir/openssl" <<'STUB'
#!/bin/bash
sub="${1:-}"
case "$sub" in
    pkey)
        f=""; pubout=0
        while [ $# -gt 0 ]; do
            case "$1" in
                -in) f="$2"; shift 2 ;;
                -pubout) pubout=1; shift ;;
                *) shift ;;
            esac
        done
        [ -n "$f" ] && [ -s "$f" ] || exit 1
        p=$(sed -n 's/^STUB_PUB=//p' "$f"); [ -n "$p" ] || exit 1
        [ "$pubout" = 1 ] && printf -- '-----BEGIN PUBLIC KEY-----\nstubpub-%s\n-----END PUBLIC KEY-----\n' "$p"
        exit 0
        ;;
    x509)
        f=""; want_pub=0; want_fp=0
        while [ $# -gt 0 ]; do
            case "$1" in
                -in) f="$2"; shift 2 ;;
                -pubkey) want_pub=1; shift ;;
                -fingerprint) want_fp=1; shift ;;
                *) shift ;;
            esac
        done
        [ -n "$f" ] && [ -s "$f" ] || exit 1
        if [ "$want_pub" = 1 ]; then
            p=$(sed -n 's/^STUB_PUB=//p' "$f"); [ -n "$p" ] || exit 1
            printf -- '-----BEGIN PUBLIC KEY-----\nstubpub-%s\n-----END PUBLIC KEY-----\n' "$p"
        fi
        if [ "$want_fp" = 1 ]; then
            # Content-derived, so a rotated cert has a different digest and the
            # old/new comparison in case 7 means something.
            printf 'sha256 Fingerprint=%s\n' "$(sha256sum "$f" | cut -c1-32)"
        fi
        exit 0
        ;;
esac

[ "$sub" = "req" ] || exit 0
c="${STUB_COUNTER_DIR:-/tmp}/openssl-req.count"
n=$(cat "$c" 2>/dev/null || echo 0); n=$((n + 1)); echo "$n" > "$c"
[ "${STUB_OPENSSL_MODE:-ok}" = "fail" ] && exit 1
keyout=""; out=""
while [ $# -gt 0 ]; do
    case "$1" in
        -keyout) keyout="$2"; shift 2 ;;
        -out) out="$2"; shift 2 ;;
        *) shift ;;
    esac
done
# Both halves carry the same generation id, so the script's pair check passes
# for a real generation and would fail for a mismatched pair.
[ -n "$keyout" ] && printf -- '-----BEGIN PRIVATE KEY-----\nrotated\nSTUB_PUB=%s\n-----END PRIVATE KEY-----\n' "$n" > "$keyout"
[ -n "$out" ] && printf -- '-----BEGIN CERTIFICATE-----\nrotated\nSTUB_PUB=%s\n-----END CERTIFICATE-----\n' "$n" > "$out"
exit 0
STUB

    cat > "$dir/ssh-keygen" <<'STUB'
#!/bin/bash
# -lf <pub>  -> a fingerprint derived from the file's contents, so a rotated
#               key necessarily fingerprints differently.
# -A -f <dir> -> a fresh host-key set, each generation distinct.
if [ "${1:-}" = "-lf" ]; then
    f="${2:-}"
    [ -s "$f" ] || exit 1
    printf '256 SHA256:%s %s (ED25519)\n' "$(sha256sum "$f" | cut -c1-24)" "stub@archipelago"
    exit 0
fi

c="${STUB_COUNTER_DIR:-/tmp}/ssh-keygen.count"
n=$(cat "$c" 2>/dev/null || echo 0); n=$((n + 1)); echo "$n" > "$c"
[ "${STUB_SSHKEYGEN_MODE:-ok}" = "fail" ] && exit 1

root=""
while [ $# -gt 0 ]; do
    case "$1" in
        -f) root="$2"; shift 2 ;;
        *) shift ;;
    esac
done
[ -n "$root" ] || exit 1
mkdir -p "$root/etc/ssh"
for t in rsa ecdsa ed25519; do
    printf -- '-----BEGIN OPENSSH PRIVATE KEY-----\nrotated-gen%s-%s\n' "$n" "$t" > "$root/etc/ssh/ssh_host_${t}_key"
    printf -- 'ssh-%s AAAArotated-gen%s stub@archipelago\n' "$t" "$n"           > "$root/etc/ssh/ssh_host_${t}_key.pub"
done
exit 0
STUB

    cat > "$dir/systemctl" <<'STUB'
#!/bin/bash
# Records each call ALONGSIDE whether the rotation record already exists at
# that moment. That is how "old fingerprints were written BEFORE the swap"
# becomes an observable ordering fact rather than an inference from content:
# the first reload happens after the first swap, so the record must already be
# on disk by then.
if [ -n "${STUB_SYSTEMCTL_LOG:-}" ]; then
    rj="no"
    [ -f "${HOST_SECRETS_ROOT:-}/var/lib/archipelago/host-key-rotation.json" ] && rj="yes"
    echo "$* rotjson=$rj" >> "$STUB_SYSTEMCTL_LOG"
fi
exit 0
STUB

    chmod +x "$dir"/openssl "$dir"/ssh-keygen "$dir"/systemctl
}

STUBS="$WORK/stubs"
make_stubs "$STUBS"

# ── Tree builders ─────────────────────────────────────────────────────────
# T0 is a fixed "this node's first boot" instant. Everything is dated relative
# to it so the cases read as timelines rather than as magic numbers.
T0=$(date -u -d '2026-06-01 12:00:00' +%s)

at() { date -u -d "@$1" '+%Y-%m-%d %H:%M:%S'; }

new_root() {
    local name="$1"
    local r="$WORK/root-$name"
    rm -rf "$r"
    mkdir -p "$r/var/lib/archipelago" "$r/var/log" "$r/etc/ssh" \
             "$r/etc/archipelago/ssl" "$r/opt/archipelago" "$r/root" "$r/dev"
    printf '%s' "$r"
}

# Host keys + TLS material dated at <epoch>.
put_material() {
    local r="$1" when="$2" tag="${3:-baked}"
    local t
    for t in rsa ecdsa ed25519; do
        printf -- '-----BEGIN OPENSSH PRIVATE KEY-----\n%s-%s\n' "$tag" "$t" > "$r/etc/ssh/ssh_host_${t}_key"
        printf -- 'ssh-%s AAAA%s stub@archipelago\n' "$t" "$tag"             > "$r/etc/ssh/ssh_host_${t}_key.pub"
    done
    printf -- '-----BEGIN PRIVATE KEY-----\n%s\nSTUB_PUB=0\n-----END PRIVATE KEY-----\n' "$tag" > "$r/etc/archipelago/ssl/archipelago.key"
    printf -- '-----BEGIN CERTIFICATE-----\n%s\nSTUB_PUB=0\n-----END CERTIFICATE-----\n' "$tag" > "$r/etc/archipelago/ssl/archipelago.crt"
    touch -d "$(at "$when")" "$r"/etc/ssh/ssh_host_* \
        "$r/etc/archipelago/ssl/archipelago.key" "$r/etc/archipelago/ssl/archipelago.crt"
}

put_anchor() {
    local r="$1" when="$2"
    : > "$r/var/lib/archipelago/.secrets-regenerated"
    touch -d "$(at "$when")" "$r/var/lib/archipelago/.secrets-regenerated"
}

CASE_RC=0
CASE_OUT=""
CASE_ERR=""
run_script() {
    local root="$1" name="$2"; shift 2
    CASE_OUT="$WORK/$name.out"; CASE_ERR="$WORK/$name.err"
    mkdir -p "$WORK/counters-$name"
    set +e
    env PATH="$STUBS:$PATH" \
        HOST_SECRETS_ROOT="$root" \
        STUB_OPENSSL_MODE="${STUB_OPENSSL_MODE:-ok}" \
        STUB_SSHKEYGEN_MODE="${STUB_SSHKEYGEN_MODE:-ok}" \
        STUB_COUNTER_DIR="$WORK/counters-$name" \
        STUB_SYSTEMCTL_LOG="$WORK/$name.systemctl" \
        bash "$SCRIPT" "$@" > "$CASE_OUT" 2> "$CASE_ERR"
    CASE_RC=$?
    set -e
}

verdict_of() { sed -n 's/.*"verdict": "\([^"]*\)".*/\1/p' "$1" | head -1; }

# A content+mtime+mode snapshot of everything except the script's own outputs,
# so "touched nothing" can be asserted as a whole-tree fact.
snapshot_tree() {
    local r="$1"
    ( cd "$r" && find . -path ./var/lib/archipelago -prune -o \( -type f -o -type l \) -print0 \
        | sort -z | xargs -0 -r stat -c '%n %s %Y %a' ) 2>/dev/null
    ( cd "$r" && find . -path ./var/lib/archipelago -prune -o -type f -print0 \
        | sort -z | xargs -0 -r sha256sum ) 2>/dev/null
}

# ── Case 1: keys newer than the anchor -> per-node ────────────────────────
R=$(new_root per-node)
put_anchor "$R" "$T0"
put_material "$R" "$((T0 + 5))" fresh
BEFORE=$(snapshot_tree "$R")
run_script "$R" per-node --detect
c=""
[ "$CASE_RC" -eq 0 ] || c="$c exit-nonzero"
J="$R/var/lib/archipelago/host-secrets-audit.json"
[ -f "$J" ] || c="$c no-json"
[ "$(verdict_of "$J" 2>/dev/null)" = "per-node" ] || c="$c verdict=$(verdict_of "$J" 2>/dev/null)"
grep -q '"checked_at"' "$J" 2>/dev/null || c="$c no-checked-at"
grep -q '"ssh_host_key_fingerprints"' "$J" 2>/dev/null || c="$c no-fingerprints"
[ "$(snapshot_tree "$R")" = "$BEFORE" ] || c="$c detect-modified-the-tree"
if [ -z "$c" ]; then
    ok "host keys newer than the anchor -> per-node, JSON written, nothing else changed"
else
    bad "host keys newer than the anchor ->$c"; echo "    root=$R rc=$CASE_RC"
fi

# ── Case 2: keys 30 days older than the anchor -> shared ─────────────────
R=$(new_root shared-mtime)
put_anchor "$R" "$T0"
put_material "$R" "$((T0 - 30 * 86400))" baked
run_script "$R" shared-mtime --detect
c=""
J="$R/var/lib/archipelago/host-secrets-audit.json"
[ "$CASE_RC" -eq 0 ] || c="$c exit-nonzero"
[ "$(verdict_of "$J" 2>/dev/null)" = "shared" ] || c="$c verdict=$(verdict_of "$J" 2>/dev/null)"
grep -q 'ssh_host_rsa_key mtime is' "$J" 2>/dev/null || c="$c evidence-does-not-name-the-ssh-key"
grep -q 'archipelago.key mtime is' "$J" 2>/dev/null || c="$c evidence-does-not-name-the-tls-key"
grep -qi 'SHARED' "$WORK/shared-mtime.out" || c="$c no-human-verdict-on-stdout"
if [ -z "$c" ]; then
    ok "host keys 30 days older than the anchor -> shared, evidence names both key classes"
else
    bad "host keys 30 days older than the anchor ->$c"; echo "    root=$R rc=$CASE_RC"
fi

# ── Case 3: the fail-open fingerprint -> shared on direct evidence ───────
# Deliberately dated so the mtime signal says per-node. If this case passes it
# is because signal 2 fired, not because the timestamps happened to agree.
R=$(new_root fail-open)
put_anchor "$R" "$T0"
put_material "$R" "$((T0 + 5))" kept-baked
{
    echo "Mon Jun  1 12:00:00 UTC 2026: regenerating per-device secrets"
    echo "Mon Jun  1 12:00:01 UTC 2026: WARNING: TLS regeneration failed, keeping baked key"
    echo "Mon Jun  1 12:00:02 UTC 2026: WARNING: ssh-keygen -A failed, keeping baked host keys"
} > "$R/var/log/archipelago-first-boot-secrets.log"
run_script "$R" fail-open --detect
c=""
J="$R/var/lib/archipelago/host-secrets-audit.json"
[ "$(verdict_of "$J" 2>/dev/null)" = "shared" ] || c="$c verdict=$(verdict_of "$J" 2>/dev/null)"
grep -q '/var/lib/archipelago/.secrets-regenerated' "$J" 2>/dev/null || c="$c evidence-missing-marker-signal"
grep -q '/var/log/archipelago-first-boot-secrets.log' "$J" 2>/dev/null || c="$c evidence-missing-log-signal"
grep -q 'fail-open fingerprint' "$J" 2>/dev/null || c="$c evidence-does-not-name-the-combination"
if [ -z "$c" ]; then
    ok "marker plus a WARNING: line -> shared, with both signals in evidence, despite per-node mtimes"
else
    bad "marker plus a WARNING: line ->$c"; echo "    root=$R rc=$CASE_RC"
fi

# ── Case 4: stripped rootfs, no host keys -> fail-closed-missing ─────────
# Not `shared`. The distinction is the whole reason signal 4 exists: on a
# 10-03-or-later node an absent key means generation never succeeded, which is
# fail-closed working, and rotating is not the remedy.
R=$(new_root stripped)
put_anchor "$R" "$T0"
printf 'F-03 identity strip\n' > "$R/opt/archipelago/rootfs-identity-stripped"
run_script "$R" stripped --detect
c=""
J="$R/var/lib/archipelago/host-secrets-audit.json"
[ "$CASE_RC" -eq 0 ] || c="$c exit-nonzero"
v=$(verdict_of "$J" 2>/dev/null)
[ "$v" = "fail-closed-missing" ] || c="$c verdict=$v"
[ "$v" = "shared" ] && c="$c CALLED-MISSING-MATERIAL-SHARED"
grep -q 'rootfs-identity-stripped' "$J" 2>/dev/null || c="$c evidence-missing-provenance"
if [ -z "$c" ]; then
    ok "identity-stripped rootfs with no host keys -> fail-closed-missing, not shared"
else
    bad "identity-stripped rootfs with no host keys ->$c"; echo "    root=$R rc=$CASE_RC"
fi

# ── Case 5: no anchor at all -> unknown ──────────────────────────────────
# The signal that must never be guessed. An absent anchor is absence of
# evidence, and reporting per-node here would leave an exposed node looking
# clean (T-10-37).
R=$(new_root no-anchor)
put_material "$R" "$T0" whatever
: > "$R/etc/machine-id"    # present but empty, as on a stripped rootfs
run_script "$R" no-anchor --detect
c=""
J="$R/var/lib/archipelago/host-secrets-audit.json"
v=$(verdict_of "$J" 2>/dev/null)
[ "$v" = "unknown" ] || c="$c verdict=$v"
[ "$v" = "per-node" ] && c="$c CLAIMED-PER-NODE-WITHOUT-EVIDENCE"
grep -q 'no anchor' "$J" 2>/dev/null || c="$c evidence-does-not-explain-why"
if [ -z "$c" ]; then
    ok "no first-boot anchor -> unknown, never per-node"
else
    bad "no first-boot anchor ->$c"; echo "    root=$R rc=$CASE_RC"
fi

# ── Case 6: --apply without --yes is inert ───────────────────────────────
R=$(new_root dry-run)
put_anchor "$R" "$T0"
put_material "$R" "$((T0 - 30 * 86400))" baked
BEFORE=$(snapshot_tree "$R")
BEFORE_STATE=$(ls -A "$R/var/lib/archipelago")
run_script "$R" dry-run --apply
c=""
[ "$CASE_RC" -eq 0 ] || c="$c exit-nonzero"
[ "$(snapshot_tree "$R")" = "$BEFORE" ] || c="$c TREE-CHANGED"
[ "$(ls -A "$R/var/lib/archipelago")" = "$BEFORE_STATE" ] || c="$c STATE-DIR-CHANGED"
[ -f "$R/var/lib/archipelago/host-key-rotation.json" ] && c="$c rotation-record-written"
ls "$R"/etc/archipelago/ssl/*.rotnew >/dev/null 2>&1 && c="$c staging-leftover"
grep -qi 'DRY RUN' "$WORK/dry-run.out" || c="$c no-dry-run-notice"
grep -qi 'one-way' "$WORK/dry-run.out" || c="$c does-not-warn-that-it-is-one-way"
if [ -z "$c" ]; then
    ok "--apply without --yes -> exits 0 and not one byte of the tree changes"
else
    bad "--apply without --yes ->$c"; echo "    root=$R rc=$CASE_RC"
    # `diff` exits 1 when it finds differences, which under `set -o pipefail`
    # would abort the run before the summary — i.e. a failing case would hide the
    # other cases. Report and carry on.
    diff <(echo "$BEFORE") <(snapshot_tree "$R") | head -10 || true
fi

# ── Case 7a: --apply --yes rotates, recording old fingerprints first ─────
R=$(new_root rotate)
put_anchor "$R" "$T0"
put_material "$R" "$((T0 - 30 * 86400))" baked
OLD_SSH_SHA=$(sha256sum "$R/etc/ssh/ssh_host_ed25519_key" | cut -d' ' -f1)
OLD_TLS_SHA=$(sha256sum "$R/etc/archipelago/ssl/archipelago.key" | cut -d' ' -f1)
# The fingerprint the old key WOULD produce, computed independently of the
# script, so the "old" half of the record is checked against an outside source.
OLD_FP_EXPECT=$(sha256sum "$R/etc/ssh/ssh_host_ed25519_key.pub" | cut -c1-24)
run_script "$R" rotate --apply --yes
c=""
ROT="$R/var/lib/archipelago/host-key-rotation.json"
J="$R/var/lib/archipelago/host-secrets-audit.json"
[ "$CASE_RC" -eq 0 ] || c="$c exit-nonzero"
[ -f "$ROT" ] || c="$c no-rotation-record"
grep -q '"old_ssh_fingerprints"' "$ROT" 2>/dev/null || c="$c no-old-ssh-fingerprints"
grep -q '"new_ssh_fingerprints"' "$ROT" 2>/dev/null || c="$c no-new-ssh-fingerprints"
grep -q '"old_tls_sha256"' "$ROT" 2>/dev/null || c="$c no-old-tls"
grep -q '"new_tls_sha256"' "$ROT" 2>/dev/null || c="$c no-new-tls"
grep -q "$OLD_FP_EXPECT" "$ROT" 2>/dev/null || c="$c old-fingerprint-does-not-match-the-pre-rotation-key"
# ORDERING: the first systemctl call happens after the first swap, so the
# record must already exist by then.
FIRST_SYSTEMCTL=$(head -1 "$WORK/rotate.systemctl" 2>/dev/null || echo "")
case "$FIRST_SYSTEMCTL" in
    *rotjson=yes) ;;
    "") c="$c no-service-reload-happened" ;;
    *) c="$c OLD-FINGERPRINTS-NOT-RECORDED-BEFORE-THE-SWAP[$FIRST_SYSTEMCTL]" ;;
esac
grep -q 'reload ssh' "$WORK/rotate.systemctl" 2>/dev/null || c="$c sshd-not-reloaded"
grep -q 'restart ssh' "$WORK/rotate.systemctl" 2>/dev/null && c="$c SSHD-RESTARTED"
grep -q 'reload nginx' "$WORK/rotate.systemctl" 2>/dev/null || c="$c nginx-not-reloaded"
# Material actually replaced.
[ "$(sha256sum "$R/etc/ssh/ssh_host_ed25519_key" | cut -d' ' -f1)" = "$OLD_SSH_SHA" ] && c="$c ssh-key-not-replaced"
[ "$(sha256sum "$R/etc/archipelago/ssl/archipelago.key" | cut -d' ' -f1)" = "$OLD_TLS_SHA" ] && c="$c tls-key-not-replaced"
ls "$R"/etc/ssh/ssh_host_*_key >/dev/null 2>&1 || c="$c NO-HOST-KEYS-LEFT"
ls "$R"/etc/archipelago/ssl/*.rotnew >/dev/null 2>&1 && c="$c staging-leftover"
# The verdict file must reflect the post-rotation state, not the pre-rotation one.
[ "$(verdict_of "$J" 2>/dev/null)" = "per-node" ] || c="$c post-rotation-verdict=$(verdict_of "$J" 2>/dev/null)"
if [ -z "$c" ]; then
    ok "--apply --yes -> old fingerprints recorded BEFORE the swap, keys replaced, sshd reloaded not restarted, verdict re-derived"
else
    bad "--apply --yes ->$c"; echo "    root=$R rc=$CASE_RC"
    echo "    stderr: $(head -c 300 "$WORK/rotate.err" 2>/dev/null)"
fi

# ── Case 7b: a failed generation aborts before touching anything live ────
# The failure mode that loses a remote node forever is a rotation that gets
# halfway. Force the SSH generator to fail after the TLS generator succeeded —
# the exact interleaving in which a naive implementation has already swapped
# the TLS pair — and require the live material to be byte-identical.
R=$(new_root abort)
put_anchor "$R" "$T0"
put_material "$R" "$((T0 - 30 * 86400))" baked
BEFORE=$(snapshot_tree "$R")
STUB_SSHKEYGEN_MODE=fail run_script "$R" abort --apply --yes
c=""
[ "$CASE_RC" -ne 0 ] || c="$c exit-zero-on-aborted-rotation"
[ "$(snapshot_tree "$R")" = "$BEFORE" ] || c="$c LIVE-MATERIAL-CHANGED-ON-AN-ABORTED-ROTATION"
[ -f "$R/var/lib/archipelago/host-key-rotation.json" ] && c="$c rotation-record-written-for-a-rotation-that-never-happened"
ls "$R"/etc/ssh/ssh_host_*_key >/dev/null 2>&1 || c="$c NO-HOST-KEYS-LEFT"
ls "$R"/etc/archipelago/ssl/*.rotnew >/dev/null 2>&1 && c="$c staging-leftover"
grep -qi 'ABORTED' "$WORK/abort.err" || c="$c no-loud-abort-on-stderr"
[ -s "$WORK/abort.systemctl" ] && c="$c reloaded-a-service-during-an-aborted-rotation"
if [ -z "$c" ]; then
    ok "generation failure -> aborts before any swap; live keys byte-identical, no service reloaded"
else
    bad "generation failure ->$c"; echo "    root=$R rc=$CASE_RC"
    # `diff` exits 1 when it finds differences, which under `set -o pipefail`
    # would abort the run before the summary — i.e. a failing case would hide the
    # other cases. Report and carry on.
    diff <(echo "$BEFORE") <(snapshot_tree "$R") | head -10 || true
    echo "    stderr: $(head -c 300 "$WORK/abort.err" 2>/dev/null)"
fi

echo ""
echo "──────── host-secrets-audit summary ────────"
echo "passed: $PASS_COUNT   failed: $FAIL_COUNT"
[ "$FAIL_COUNT" -eq 0 ]
