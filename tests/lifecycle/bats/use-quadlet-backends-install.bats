#!/usr/bin/env bats
# tests/lifecycle/bats/use-quadlet-backends-install.bats
#
# Validates the post-condition of Phase 3.2's `use_quadlet_backends`
# install path. When the orchestrator routed at least one backend
# install through `install_via_quadlet`, this suite asserts that the
# resulting state has the four properties the Phase 3 design promises:
#
#   1. A `.container` unit file exists in ~/.config/containers/systemd/
#      and is well-formed (required sections + directives).
#   2. The corresponding `.service` is active under `systemctl --user`.
#   3. The container is in `podman ps` (running).
#   4. The container's cgroup is under `user.slice/...`, NOT under
#      `archipelago.service` — proving FM3 (cgroup cascade SIGKILL on
#      archipelago restart) is structurally fixed for that container.
#
# Auto-skips if no Quadlet-managed backend exists yet — so it runs as a
# no-op on nodes where `use_quadlet_backends` is still false (today's
# default), and turns into a hard regression gate as soon as anyone
# flips the flag and reinstalls.
#
# Run on a node with rootless podman + systemd-user (every alpha-fleet
# box). No env vars required for the read-only checks. The cleanup
# section at the bottom is gated by ARCHY_ALLOW_DESTRUCTIVE=1.

# bats-core ships no `fail`; bats-assert isn't installed on the alpha fleet.
# Define the same minimal helper the other suites use (see mempool.bats) so a
# tripped assertion reports as a real test failure, not a status-127 crash.
fail() { echo "$@" >&2; return 1; }

quadlet_dir() {
  echo "${XDG_CONFIG_HOME:-$HOME/.config}/containers/systemd"
}

# List Quadlet `.container` units that correspond to backend containers
# (i.e., NOT companions like archy-*-ui, which already shipped via Quadlet
# in v1.7.41 and have their own coverage in companion-survives-archipelago-
# restart.bats). Echoes one container name per line; empty if none found.
backend_quadlet_units() {
  local d
  d="$(quadlet_dir)"
  [[ -d "$d" ]] || return 0
  # Strip the .container extension; filter out archy-*-ui companions.
  # wyoming-* (piper/whisper voice services) are mid-integration and not yet
  # part of the platform contract — exclude until their packaging lands.
  for f in "$d"/*.container; do
    [[ -e "$f" ]] || continue
    local name
    name="$(basename "$f" .container)"
    [[ "$name" =~ ^archy-.*-ui$ ]] && continue
    [[ "$name" =~ ^wyoming- ]] && continue
    echo "$name"
  done
}

# A unit file on disk does NOT imply the app should be running: an
# explicitly user-stopped app keeps its .container file (e.g. the inactive
# half of the bitcoin-core/bitcoin-knots multi-version pair), and its
# .service being inactive / container absent is the CORRECT state. The
# orchestrator persists that intent in user-stopped.json; honour it here so
# the active-state assertions below don't false-fail on stopped-on-purpose
# apps (gate tests 123/124, .228 2026-07-09).
USER_STOPPED_FILE="${ARCHY_DATA_DIR:-/var/lib/archipelago}/user-stopped.json"

is_user_stopped() {
  local name="$1"
  [[ -r "$USER_STOPPED_FILE" ]] || return 1
  jq -e --arg n "$name" --arg s "${name#archy-}" \
    'index($n) != null or index($s) != null' "$USER_STOPPED_FILE" >/dev/null 2>&1
}

# Read the cgroup path of a running container's main process. For
# rootless podman the conmon-run target lands the container's pid1 in
# the cgroup that owns its supervising .service.
container_cgroup_path() {
  local name="$1"
  local pid
  pid="$(podman inspect --format '{{.State.Pid}}' "$name" 2>/dev/null)"
  [[ -n "$pid" && "$pid" != "0" ]] || return 1
  # cgroup v2 line: "0::/path/to/cgroup"
  awk -F: '$1=="0"{print $3}' "/proc/$pid/cgroup" 2>/dev/null
}

# Per-test gate. Each @test calls this so the suite is a clean no-op on
# nodes where use_quadlet_backends is still false (today's default) —
# bats doesn't propagate setup-level skip semantics across @test blocks.
require_quadlet_backends() {
  local count
  count="$(backend_quadlet_units | wc -l)"
  (( count > 0 )) || skip "no backend .container units in $(quadlet_dir) — use_quadlet_backends not enabled or no backends installed"
}

@test "Quadlet unit dir exists or is plausibly creatable" {
  local d
  d="$(quadlet_dir)"
  # Either it already exists, or its parent does (so quadlet can mkdir it).
  [[ -d "$d" ]] || [[ -d "$(dirname "$d")" ]] \
    || skip "no XDG_CONFIG_HOME and no \$HOME/.config — not a desktop-style host"
}

@test "each backend Quadlet unit has the required sections + directives" {
  require_quadlet_backends
  local d
  d="$(quadlet_dir)"
  while read -r name; do
    [[ -z "$name" ]] && continue
    local body
    body="$(<"$d/$name.container")"
    # [Container] section + Image=
    [[ "$body" == *"[Container]"* ]]   || fail "$name: missing [Container] section"
    [[ "$body" == *"Image="*       ]]  || fail "$name: missing Image= directive"
    # [Service] section with Restart=always (backends AND companions alike).
    #
    # This asserted Restart=on-failure until 8908fb4f, on the rationale that
    # backends needed it "so an operator-issued `systemctl stop` actually stays
    # stopped". That rationale was wrong on two counts. systemd never applies
    # Restart= to a unit stopped via `systemctl stop`, which is how archipelago
    # stops apps — so on-failure bought nothing there. And because quadlet
    # renders --rm, a container that exits CLEANLY is deleted and on-failure
    # will not bring it back: bitcoind exits 0 on SIGTERM, so backends were
    # vanishing after a clean exit.
    #
    # Verified on-device before the change landed: `podman stop bitcoin-knots`
    # -> back in 12s, while a dashboard-issued stop stayed stopped for 90s.
    # Operator approved it explicitly, gated on exactly that verification.
    [[ "$body" == *"[Service]"*       ]] || fail "$name: missing [Service] section"
    [[ "$body" == *"Restart=always"* ]] \
      || fail "$name: backend unit must use Restart=always (--rm deletes a cleanly-exited container, and on-failure never restarts it)"
    # [Install] section so `systemctl --user enable` is well-defined.
    [[ "$body" == *"[Install]"* ]]  || fail "$name: missing [Install] section"
    [[ "$body" == *"WantedBy="*  ]] || fail "$name: missing WantedBy= in [Install]"
  done < <(backend_quadlet_units)
}

@test "health is app-level state, NOT a systemd start gate (no Notify=healthy)" {
  require_quadlet_backends
  # Phase 3.4 originally emitted Notify=healthy so `systemctl start` blocked
  # until the healthcheck passed. That was deliberately reverted: gating start
  # on health hung boot reconciliation for dependency-waiting apps (fedimint
  # idles its entrypoint until Bitcoin IBD finishes; lnd until the macaroon
  # unlocks), leaving units stuck in "deactivating". The renderer now emits
  # HealthCmd= for Podman's health state but TimeoutStartSec=0 and NO
  # Notify=healthy (see quadlet.rs render() + contains_stale_health_gate()).
  # This asserts the current invariant: no backend unit gates start on health.
  local d
  d="$(quadlet_dir)"
  while read -r name; do
    [[ -z "$name" ]] && continue
    local body
    body="$(<"$d/$name.container")"
    [[ "$body" != *"Notify=healthy"* ]] \
      || fail "$name: emits Notify=healthy — stale health gate; start would block on health and can hang boot reconcile"
  done < <(backend_quadlet_units)
}

@test "every backend Quadlet unit's .service is active in systemctl --user" {
  require_quadlet_backends
  while read -r name; do
    [[ -z "$name" ]] && continue
    is_user_stopped "$name" && continue
    # Converges-to-active, not instantly-active: a dependency-degraded app
    # (mempool-api while electrumx catches up to the daemon) exits at startup
    # and flaps through 'activating' for a couple of minutes after a lifecycle
    # cycle; systemd's Restart=on-failure heals it. A genuine crash-loop still
    # fails after the settle window (gate 2026-07-09, .228 iterations 1+2).
    local state="" deadline=$((SECONDS + 180))
    while (( SECONDS < deadline )); do
      state="$(systemctl --user is-active "$name.service" 2>&1)" && break
      sleep 5
    done
    [[ "$state" == "active" ]] \
      || fail "$name.service is '$state' — did not reach 'active' within 180s"
  done < <(backend_quadlet_units)
}

@test "every backend Quadlet unit has a running podman container" {
  require_quadlet_backends
  while read -r name; do
    [[ -z "$name" ]] && continue
    is_user_stopped "$name" && continue
    # Same settle window as the active-state assert above: a quadlet --rm
    # container is absent for a few seconds around each systemd retry.
    local state="" deadline=$((SECONDS + 180))
    while (( SECONDS < deadline )); do
      state="$(podman inspect --format '{{.State.Running}}' "$name" 2>/dev/null)" \
        && [[ "$state" == "true" ]] && break
      sleep 5
    done
    [[ "$state" == "true" ]] \
      || fail "$name has no running container within 180s (state=${state:-absent})"
  done < <(backend_quadlet_units)
}

@test "FM3 fix: backend cgroup is under user.slice, not archipelago.service" {
  require_quadlet_backends
  # The whole point of Phase 3 — verify the kernel-level invariant.
  while read -r name; do
    [[ -z "$name" ]] && continue
    local cg
    cg="$(container_cgroup_path "$name")" || skip "$name has no readable PID; container may have crashed mid-test"
    [[ -n "$cg" ]] || fail "$name: empty cgroup path"
    # Acceptable: anything under user.slice (rootless podman lands here when
    # quadlet-managed). Forbidden: anything under archipelago.service's tree.
    [[ "$cg" == *"user.slice"* ]] \
      || fail "$name: cgroup '$cg' is not under user.slice — FM3 cascade still possible"
    [[ "$cg" != *"archipelago.service"* ]] \
      || fail "$name: cgroup '$cg' is under archipelago.service — Phase 3 promise broken"
  done < <(backend_quadlet_units)
}
