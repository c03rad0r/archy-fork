#!/usr/bin/env bats
# tests/lifecycle/bats/secret-completeness.bats
#
# Regression guard for the .198 failure class: a manifest references a
# `secret_env.secret_file` that was never generated on the node, so secret
# resolution hard-fails and the container won't start — cascading the whole
# Bitcoin stack. (bitcoin-knots gained `bitcoin-rpc-txrelay-rpcauth`, which old
# nodes lacked, so bitcoind never came up and reinstall "just stopped".)
#
# For every installed backend, assert every secret_file it references exists in
# the secrets dir. Runs on the archy host.
#
# Tiers: read-only.

SECRETS_DIR="${ARCHY_SECRETS_DIR:-/var/lib/archipelago/secrets}"

_apps_dir() {
  local d
  for d in "${ARCHIPELAGO_APPS_DIR:-}" /opt/archipelago/apps \
           "$BATS_TEST_DIRNAME/../../../apps"; do
    [[ -n "$d" && -d "$d" ]] && { echo "$d"; return 0; }
  done
  return 1
}

_manifest_for() {
  local app="$1" dir mf
  dir=$(_apps_dir) || return 1
  for mf in "$dir/$app/manifest.yml" "$dir/$app/manifest.yaml"; do
    [[ -r "$mf" ]] && { echo "$mf"; return 0; }
  done
  return 1
}

_secret_files_in() {
  # Emit each `secret_file:` value referenced by the manifest.
  grep -E '^[[:space:]]*secret_file:' "$1" 2>/dev/null | awk '{print $2}'
}

_secret_exists() {
  local f="$SECRETS_DIR/$1"
  [[ -e "$f" ]] && return 0
  sudo -n test -f "$f" 2>/dev/null
}

@test "every installed backend's referenced secrets exist on disk" {
  command -v podman >/dev/null 2>&1 || skip "podman not available"
  [[ -d "$SECRETS_DIR" ]] || sudo -n test -d "$SECRETS_DIR" 2>/dev/null || skip "secrets dir not present"

  local checked=0 missing="" app cname mf sf
  # container-name : manifest-app-id  (the bitcoin stack that cascades)
  for pair in \
    "bitcoin-knots:bitcoin-knots" "lnd:lnd" "electrumx:electrumx" \
    "mempool-api:mempool-api" "btcpay-server:btcpay-server" \
    "archy-nbxplorer:archy-nbxplorer" "fedimint:fedimint" \
    "fedimint-gateway:fedimint-gateway"; do
    cname="${pair%%:*}"; app="${pair##*:}"
    podman container exists "$cname" 2>/dev/null || continue
    mf=$(_manifest_for "$app") || continue
    while read -r sf; do
      [[ -n "$sf" ]] || continue
      checked=$((checked + 1))
      _secret_exists "$sf" || missing+="${app} -> ${sf}\n"
    done < <(_secret_files_in "$mf")
  done

  [[ "$checked" -gt 0 ]] || skip "no installed backends with secret references to check"
  if [[ -n "$missing" ]]; then
    echo "installed apps reference missing secrets:" >&2
    echo -e "$missing" >&2
    return 1
  fi
}
