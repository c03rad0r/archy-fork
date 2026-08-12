#!/usr/bin/env bats
# tests/lifecycle/bats/port-drift.bats
#
# Regression guard for the .116 failure class: a backend container that is
# "Up" but publishes its ports to the WRONG host ports because the manifest
# changed after the container was created (e.g. lnd REST stuck on host 8080
# while the manifest — and every in-process client — expects 18080).
#
# This mirrors the orchestrator's `host_port_bindings_drifted` check, but from
# the outside: it compares the live `podman inspect` PortBindings against the
# manifest `ports:` for each installed backend. Runs on the archy host.
#
# Tiers: read-only.

_apps_dir() {
  local d
  for d in "${ARCHIPELAGO_APPS_DIR:-}" /opt/archipelago/apps \
           "$BATS_TEST_DIRNAME/../../../apps"; do
    [[ -n "$d" && -d "$d" ]] && { echo "$d"; return 0; }
  done
  return 1
}

_manifest_for() {
  local app="$1" dir
  dir=$(_apps_dir) || return 1
  local mf
  for mf in "$dir/$app/manifest.yml" "$dir/$app/manifest.yaml"; do
    [[ -r "$mf" ]] && { echo "$mf"; return 0; }
  done
  return 1
}

# Emit "host container" pairs from a manifest's ports: block.
_manifest_ports() {
  awk '
    /^[[:space:]]*ports:/ { inports=1; next }
    inports && /^[[:space:]]*[a-z_]+:[[:space:]]*$/ && !/protocol:|host:|container:/ { inports=0 }
    inports && /- host:/    { host=$3 }
    inports && /container:/  { print host, $2 }
  ' "$1"
}

# For a given container + (host,container) port, emit a "DRIFT: …" line on
# mismatch (and nothing otherwise). Stays silent for unpublished / host-net
# ports — those are handled elsewhere and must never be treated as drift.
_drift_line() {
  local cname="$1" want_host="$2" cport="$3"
  local bindings actual
  bindings=$(podman inspect "$cname" --format '{{json .HostConfig.PortBindings}}' 2>/dev/null) || return 0
  actual=$(echo "$bindings" | jq -r --arg k "${cport}/tcp" '.[$k][]?.HostPort // empty' 2>/dev/null)
  [[ -n "$actual" ]] || return 0
  echo "$actual" | grep -qx "$want_host" && return 0
  echo "DRIFT: $cname container-port $cport published on host [$actual] but manifest wants $want_host"
}

@test "backend containers publish ports that match their manifest" {
  command -v podman >/dev/null 2>&1 || skip "podman not available"
  local checked=0 violations="" app cname mf line
  # container-name : manifest-app-id
  for pair in "lnd:lnd" "bitcoin-knots:bitcoin-knots" "electrumx:electrumx"; do
    cname="${pair%%:*}"; app="${pair##*:}"
    podman container exists "$cname" 2>/dev/null || continue
    mf=$(_manifest_for "$app") || continue
    while read -r host cport; do
      [[ -n "$host" && -n "$cport" ]] || continue
      checked=$((checked + 1))
      line=$(_drift_line "$cname" "$host" "$cport")
      [[ -n "$line" ]] && violations+="${line}"$'\n'
    done < <(_manifest_ports "$mf")
  done
  [[ "$checked" -gt 0 ]] || skip "no installed backend containers with published ports to check"
  if [[ -n "$violations" ]]; then
    echo "published-port drift detected:" >&2
    echo "$violations" >&2
    return 1
  fi
}
