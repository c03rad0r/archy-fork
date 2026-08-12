#!/bin/bash
#
# Archipelago Container Reconciler
# Ensures every container matches the canonical spec from container-specs.sh.
# Safe to run repeatedly (idempotent). Run on any node.
#
# Usage:
#   sudo ./reconcile-containers.sh                 # Fix everything
#   sudo ./reconcile-containers.sh --check-only    # Audit only, no changes
#   sudo ./reconcile-containers.sh --force          # Override user-stopped
#   sudo ./reconcile-containers.sh --force-recreate # Recreate matched containers
#   sudo ./reconcile-containers.sh --tier=2         # Only reconcile tier 2
#   sudo ./reconcile-containers.sh --container=lnd  # Only reconcile lnd
#
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# ── Parse arguments ──────────────────────────────────────────────────
CHECK_ONLY=false
FORCE=false
FORCE_RECREATE=false
CREATE_MISSING=false
FILTER_TIER=""
FILTER_CONTAINER=""
for arg in "$@"; do
  case "$arg" in
    --check-only)         CHECK_ONLY=true ;;
    --force)              FORCE=true ;;
    --force-recreate)     FORCE_RECREATE=true ;;
    --create-missing)     CREATE_MISSING=true ;;
    --tier=*)             FILTER_TIER="${arg#*=}" ;;
    --container=*)        FILTER_CONTAINER="${arg#*=}" ;;
    -h|--help)
      echo "Usage: $0 [--check-only] [--force] [--force-recreate] [--create-missing] [--tier=N] [--container=NAME]"
      echo ""
      echo "  --check-only      Audit only, no changes."
      echo "  --force           Override user-stopped state."
      echo "  --force-recreate  Recreate matched existing containers even if they"
      echo "                    otherwise match the spec. Use with --container or"
      echo "                    --tier for scoped image/config refreshes."
      echo "  --create-missing  Override SPEC_OPTIONAL for containers that have on-disk"
      echo "                    data but no live container (recovery from failed updates)."
      echo "  --tier=N          Only reconcile containers in tier N."
      echo "  --container=NAME  Only reconcile the named container (spec key)."
      exit 0 ;;
  esac
done

# ── Colors ───────────────────────────────────────────────────────────
RED='\033[0;31m'    GREEN='\033[0;32m'    YELLOW='\033[0;33m'
BLUE='\033[0;34m'   CYAN='\033[0;36m'     BOLD='\033[1m'
NC='\033[0m'

ok()    { echo -e "  ${GREEN}[OK]${NC}      $*"; }
fixed() { echo -e "  ${CYAN}[FIXED]${NC}   $*"; }
skip()  { echo -e "  ${YELLOW}[SKIP]${NC}    $*"; }
fail()  { echo -e "  ${RED}[FAIL]${NC}    $*"; }
info()  { echo -e "  ${BLUE}[INFO]${NC}    $*"; }
header(){ echo -e "\n${BOLD}$*${NC}"; }

# ── Source specs ─────────────────────────────────────────────────────
source "$SCRIPT_DIR/container-specs.sh" || { echo "Cannot source container-specs.sh"; exit 1; }
detect_environment

PORT_ALLOC_FILE="/var/lib/archipelago/port-allocations.env"
[ -f "$PORT_ALLOC_FILE" ] && . "$PORT_ALLOC_FILE"

port_available() {
  local port="$1"
  ss -ltn 2>/dev/null | awk -v p=":$port" '$4 == p || $4 ~ p "$" { found=1 } END { exit found ? 1 : 0 }'
}

alloc_port() {
  local key="$1" preferred="$2" var="PORT_${key//[^A-Za-z0-9]/_}" cur=""
  eval "cur=\${$var:-}"
  if [ -n "$cur" ] && port_available "$cur"; then
    printf '%s' "$cur"
    return
  fi
  if port_available "$preferred"; then
    cur="$preferred"
  else
    cur=""
    for p in $(seq 8085 9999); do
      if port_available "$p"; then cur="$p"; break; fi
    done
  fi
  [ -n "$cur" ] || cur="$preferred"
  sudo mkdir -p "$(dirname "$PORT_ALLOC_FILE")" 2>/dev/null || true
  if ! grep -q "^$var=" "$PORT_ALLOC_FILE" 2>/dev/null; then
    printf '%s=%s\n' "$var" "$cur" | sudo tee -a "$PORT_ALLOC_FILE" >/dev/null
  fi
  printf '%s' "$cur"
}

# ── Podman command ───────────────────────────────────────────────────
# Run as archipelago user — podman sees rootless containers directly.
# Use sudo only for chown/mkdir operations.
PODMAN="podman"
PODMAN_IMAGE_CHECK_TIMEOUT="${PODMAN_IMAGE_CHECK_TIMEOUT:-10}"

podman_bounded() {
  timeout --kill-after=2s "${PODMAN_IMAGE_CHECK_TIMEOUT}s" "$PODMAN" "$@"
}

# ── Pre-flight ───────────────────────────────────────────────────────
header "╔══════════════════════════════════════════════════╗"
header "║  ARCHIPELAGO CONTAINER RECONCILER                ║"
header "╚══════════════════════════════════════════════════╝"
echo ""
info "Host: $(hostname) ($HOST_IP)"
info "Disk: ${DISK_GB}GB | RAM: ${TOTAL_MEM_MB}MB | Low-mem: $LOW_MEM"
info "Mode: $($CHECK_ONLY && echo 'CHECK ONLY (no changes)' || echo 'APPLY FIXES')"
echo ""

# Ensure archy-net exists
if ! $PODMAN network exists archy-net 2>/dev/null; then
  if $CHECK_ONLY; then
    info "archy-net missing (would create)"
  else
    $PODMAN network create archy-net 2>/dev/null && info "Created archy-net" || fail "Cannot create archy-net"
  fi
fi

# Load user-stopped list
USER_STOPPED_FILE="/var/lib/archipelago/user-stopped.json"
USER_STOPPED=""
if [ -f "$USER_STOPPED_FILE" ]; then
  USER_STOPPED=$(cat "$USER_STOPPED_FILE" 2>/dev/null)
fi
is_user_stopped() {
  [ "$FORCE" = "true" ] && return 1
  echo "$USER_STOPPED" | grep -q "\"$1\"" 2>/dev/null
}

# ── Inspection helpers ───────────────────────────────────────────────
container_exists() {
  # Avoid SIGPIPE-from-grep-q failing under `set -o pipefail`.
  local names
  names=$($PODMAN ps -a --format '{{.Names}}' 2>/dev/null)
  echo "$names" | grep -qx "$1"
}

container_running() {
  local names
  names=$($PODMAN ps --format '{{.Names}}' 2>/dev/null)
  echo "$names" | grep -qx "$1"
}

container_image() {
  $PODMAN inspect "$1" --format '{{.ImageName}}' 2>/dev/null
}

container_image_id() {
  $PODMAN inspect "$1" --format '{{.Image}}' 2>/dev/null
}

spec_image_id() {
  podman_bounded image inspect "$SPEC_IMAGE" --format '{{.Id}}' 2>/dev/null
}

container_network() {
  # Use actual Networks map — NetworkMode is unreliable (always shows 'bridge' in rootless)
  local nets
  nets=$($PODMAN inspect "$1" --format '{{range $k,$v := .NetworkSettings.Networks}}{{$k}} {{end}}' 2>/dev/null)
  # Return first network name, trimmed
  echo "$nets" | awk '{print $1}'
}

container_memory() {
  $PODMAN inspect "$1" --format '{{.HostConfig.Memory}}' 2>/dev/null
}

container_health_cmd() {
  $PODMAN inspect "$1" --format '{{with .Config.Healthcheck}}{{range .Test}}{{println .}}{{end}}{{end}}' 2>/dev/null \
    | awk 'NR > 1 { print }' \
    | paste -sd ' ' -
}

normalize_health_cmd() {
  printf '%s' "$1" | sed 's/\\"/"/g; s/[[:space:]][[:space:]]*/ /g; s/^ //; s/ $//'
}

host_port_listening() {
  local port="$1"
  ss -ltn 2>/dev/null | awk -v p=":$port" '
    $4 == p || $4 ~ p "$" { found=1 }
    END { exit found ? 0 : 1 }
  '
}

prepare_bind_source() {
  local source="$1"
  [ -n "$source" ] || return 0

  case "$source" in
    /run/user/*/podman/podman.sock)
      if [ ! -S "$source" ]; then
        local runtime_dir="${source%/podman/podman.sock}"
        XDG_RUNTIME_DIR="$runtime_dir" systemctl --user start podman.socket 2>/dev/null || true
        for _ in 1 2 3 4 5 6 7 8 9 10; do
          [ -S "$source" ] && return 0
          sleep 0.25
        done
      fi
      ;;
  esac

  case "$source" in
    /var/lib/archipelago/*)
      sudo mkdir -p "$source" 2>/dev/null
      ;;
    *)
      # Non-data bind mounts can be files/sockets/devices. Creating the full
      # path would turn e.g. podman.sock into a directory and break Portainer.
      if [ -e "$source" ]; then
        return 0
      fi
      fail "bind source missing: $source"
      return 1
      ;;
  esac
}

ensure_catatonit() {
  command -v catatonit >/dev/null 2>&1 && return 0
  $CHECK_ONLY && { info "catatonit missing (would install)"; return 0; }

  if command -v apt-get >/dev/null 2>&1; then
    sudo apt-get update >/dev/null 2>&1 || true
    sudo apt-get install -y catatonit >/dev/null 2>&1 || true
  elif command -v dnf >/dev/null 2>&1; then
    sudo dnf install -y catatonit >/dev/null 2>&1 || true
  elif command -v apk >/dev/null 2>&1; then
    sudo apk add catatonit >/dev/null 2>&1 || true
  fi

  command -v catatonit >/dev/null 2>&1 || { fail "catatonit missing; Portainer compose builds may fail"; return 1; }
}

ensure_portainer_host_paths() {
  ensure_catatonit
  if $CHECK_ONLY; then
    [ -d /var/lib/archipelago/portainer/compose ] || info "Portainer compose dir missing (would create)"
    [ -e /data ] || info "/data host path missing (would link to /var/lib/archipelago/portainer)"
    return 0
  fi

  sudo mkdir -p /var/lib/archipelago/portainer/compose 2>/dev/null || true
  sudo chown -R 1000:1000 /var/lib/archipelago/portainer 2>/dev/null || true
  if [ ! -e /data ]; then
    sudo ln -s /var/lib/archipelago/portainer /data 2>/dev/null || true
  elif [ -d /data ] && [ ! -L /data ] && [ ! -e /data/compose ]; then
    sudo ln -s /var/lib/archipelago/portainer/compose /data/compose 2>/dev/null || true
  fi
}

container_has_mount() {
  local name="$1" source="$2" target="$3"
  $PODMAN inspect "$name" --format '{{range .Mounts}}{{println .Source "|" .Destination}}{{end}}' 2>/dev/null \
    | awk -F'|' -v src="$source" -v dst="$target" '
        { gsub(/[[:space:]]+$/, "", $1); gsub(/^[[:space:]]+/, "", $2); }
        $1 == src && $2 == dst { found=1 }
        END { exit found ? 0 : 1 }
      '
}

# Read one environment variable's current value from a running/stopped container.
# Returns empty string if the var is not set.
container_env_val() {
  local name="$1" key="$2"
  $PODMAN inspect "$name" --format '{{range .Config.Env}}{{println .}}{{end}}' 2>/dev/null \
    | awk -F= -v k="$key" '$1==k { sub(/^[^=]+=/, ""); print; exit }'
}

# Env keys whose values bake network topology into the container. If the spec's
# value for one of these keys ever differs from the running container's value
# (host IP changed, DHCP lease rotated, LAN re-subnetted, container dependency
# moved between archy-net and bridge), the container MUST be recreated.
# This is the systemic fix for the fedimint April-11 stale-IP class of bug
# where a container's URL env was never reconciled after network changes.
#
# Match by suffix to keep the list small. Covers:
#   *_URL  (FM_P2P_URL, FM_API_URL, FM_BITCOIND_URL, NBXPLORER_BTCRPCURL, ...)
#   *_HOST (BTCPAY_HOST, CORE_RPC_HOST, ...)
#   *_ENDPOINT (NBXPLORER_BTCNODEENDPOINT, ...)
URL_ENV_SUFFIXES="_URL _HOST _ENDPOINT"

image_exists() {
  podman_bounded image exists "$1" >/dev/null 2>&1
}

resolve_spec_image() {
  image_exists "$SPEC_IMAGE" && return

  local image_path image_name image_tag candidate repo
  image_path="${SPEC_IMAGE#*/}"
  image_name="${SPEC_IMAGE##*/}"
  image_tag="${image_name#*:}"
  image_name="${image_name%%:*}"

  for candidate in \
    "${ARCHY_REGISTRY_FALLBACK:-}/${image_path}" \
    "80.71.235.15:3000/archipelago/${image_name}:${image_tag}" \
    "80.71.235.15:3000/lfg2025/${image_name}:${image_tag}"; do
    case "$candidate" in /*) continue;; esac
    if image_exists "$candidate"; then
      info "$SPEC_NAME — using local image alias $candidate"
      SPEC_IMAGE="$candidate"
      return
    fi
  done

  repo=$(podman_bounded images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null \
    | grep -E "/${image_name}:${image_tag}$" \
    | head -1 || true)
  if [ -n "$repo" ]; then
    info "$SPEC_NAME — using local image alias $repo"
    SPEC_IMAGE="$repo"
  fi
}

# Convert memory string to bytes for comparison
mem_to_bytes() {
  local m="$1"
  case "$m" in
    *g|*G) echo $(( ${m%[gG]} * 1073741824 )) ;;
    *m|*M) echo $(( ${m%[mM]} * 1048576 )) ;;
    *)     echo "$m" ;;
  esac
}

# ── Build podman run command from spec ───────────────────────────────
build_run_cmd() {
  local cmd="$PODMAN run -d --name $SPEC_NAME"
  cmd+=" --restart $SPEC_RESTART"

  # Network
  if [ "$SPEC_NETWORK" = "host" ]; then
    cmd+=" --network=host"
  elif [ "$SPEC_NETWORK" = "archy-net" ]; then
    cmd+=" --network archy-net"
  fi

  # Memory
  [ -n "$SPEC_MEMORY" ] && cmd+=" --memory=$SPEC_MEMORY"

  # Capabilities
  cmd+=" --cap-drop ALL"
  for cap in $SPEC_CAPS; do
    cmd+=" --cap-add $cap"
  done

  # Security
  [ -n "$SPEC_SECURITY" ] && cmd+=" --security-opt $SPEC_SECURITY"

  # Read-only
  [ "$SPEC_READONLY" = "true" ] && cmd+=" --read-only"

  # Tmpfs
  for t in $SPEC_TMPFS; do
    cmd+=" --tmpfs $t"
  done

  # Health check
  if [ -n "$SPEC_HEALTH_CMD" ]; then
    cmd+=" --health-cmd=\"$SPEC_HEALTH_CMD\" --health-interval=120s --health-timeout=10s --health-retries=3"
  fi

  # Ports
  for p in $SPEC_PORTS; do
    cmd+=" -p $p"
  done

  # Volumes
  for v in $SPEC_VOLUMES; do
    cmd+=" -v $v"
  done

  # Environment
  for e in $SPEC_ENV; do
    cmd+=" -e \"$e\""
  done

  # Image
  cmd+=" $SPEC_IMAGE"

  # Custom args
  [ -n "$SPEC_CUSTOM_ARGS" ] && cmd+=" $SPEC_CUSTOM_ARGS"

  # Entrypoint override
  [ -n "$SPEC_ENTRYPOINT" ] && cmd+=" $SPEC_ENTRYPOINT"

  echo "$cmd"
}

# ── Counters ─────────────────────────────────────────────────────────
COUNT_OK=0   COUNT_FIXED=0   COUNT_CREATED=0   COUNT_SKIPPED=0   COUNT_FAILED=0
FAILED_LIST=""

# ── Reconcile one container ──────────────────────────────────────────
reconcile() {
  local name="$1"

  if ! load_spec "$name"; then
    skip "$name — no spec defined"
    COUNT_SKIPPED=$((COUNT_SKIPPED + 1))
    return
  fi

  if [ -n "$SPEC_SKIP_REASON" ]; then
    skip "$name — $SPEC_SKIP_REASON"
    COUNT_SKIPPED=$((COUNT_SKIPPED + 1))
    return
  fi

  [ "$name" = "portainer" ] && ensure_portainer_host_paths

  # Filter by tier
  [ -n "$FILTER_TIER" ] && [ "$SPEC_TIER" != "$FILTER_TIER" ] && return

  # User-stopped
  if is_user_stopped "$name"; then
    skip "$name — user-stopped"
    COUNT_SKIPPED=$((COUNT_SKIPPED + 1))
    fix_ownership "$name"
    return
  fi

  # Optional apps: only reconcile if already installed (container exists).
  # The install RPC creates the container; the reconciler just keeps it running.
  # --create-missing overrides this so we can recover from failed-update rollbacks
  # that deleted a container without restoring it (on-disk data still present).
  if [ "$SPEC_OPTIONAL" = "true" ] && ! container_exists "$name" && ! $CREATE_MISSING; then
    skip "$name — not installed"
    COUNT_SKIPPED=$((COUNT_SKIPPED + 1))
    return
  fi

  # Resolve registry aliases before create/recreate. ISOs and older installers
  # may seed the same image under a fallback registry tag.
  resolve_spec_image

  # Local images: skip if image doesn't exist and container doesn't exist
  if [ "$SPEC_LOCAL_IMAGE" = "true" ]; then
    if ! image_exists "$SPEC_IMAGE" && ! container_exists "$name"; then
      skip "$name — image not available"
      COUNT_SKIPPED=$((COUNT_SKIPPED + 1))
      return
    fi
  fi

  # Check dependencies
  for dep in $SPEC_DEPENDS; do
    if ! container_running "$dep"; then
      skip "$name — dependency $dep not running"
      COUNT_SKIPPED=$((COUNT_SKIPPED + 1))
      return
    fi
  done

  local action="OK"
  local reasons=""

  if container_exists "$name"; then
    local cur_image cur_image_id want_image_id cur_network cur_memory
    cur_image=$(container_image "$name")
    cur_image_id=$(container_image_id "$name")
    want_image_id=$(spec_image_id)
    cur_network=$(container_network "$name")
    cur_memory=$(container_memory "$name")
    local spec_memory_bytes expected_network

    spec_memory_bytes=$(mem_to_bytes "$SPEC_MEMORY")

    if [ "$FORCE_RECREATE" = "true" ]; then
      action="RECREATE"
      reasons+="force-recreate "
    fi

    # Same-tag local rebuilds leave running containers on the old image ID.
    # Recreate when the currently tagged spec image points at a different ID.
    if [ "$action" = "OK" ] && [ -n "$want_image_id" ] && [ -n "$cur_image_id" ] && [ "$cur_image_id" != "$want_image_id" ]; then
      action="RECREATE"
      reasons+="image-id "
    fi

    # Check network mismatch
    # For archy-net and host: exact match required
    # For bridge/default: accept any non-archy-net, non-host network
    if [ "$SPEC_NETWORK" = "archy-net" ]; then
      if [ "$cur_network" != "archy-net" ]; then
        action="RECREATE"
        reasons+="network($cur_network→archy-net) "
      fi
    elif [ "$SPEC_NETWORK" = "host" ]; then
      if [ "$cur_network" != "host" ]; then
        action="RECREATE"
        reasons+="network($cur_network→host) "
      fi
    else
      # Default/bridge: anything that isn't archy-net or host is fine
      if [ "$cur_network" = "archy-net" ] || [ "$cur_network" = "host" ]; then
        action="RECREATE"
        reasons+="network($cur_network→bridge) "
      fi
    fi

    # Check memory limit (0 = no limit)
    if [ "${cur_memory:-0}" = "0" ] && [ "${spec_memory_bytes:-0}" != "0" ]; then
      action="RECREATE"
      reasons+="memory(none→$SPEC_MEMORY) "
    fi

    # Healthcheck drift matters: a stale check can leave an otherwise working
    # service permanently unhealthy (for example ElectrumX images do not ship
    # curl, so the healthcheck must use python's socket module).
    if [ "$action" = "OK" ] && [ -n "$SPEC_HEALTH_CMD" ]; then
      local cur_health spec_health
      cur_health=$(normalize_health_cmd "$(container_health_cmd "$name")")
      spec_health=$(normalize_health_cmd "$SPEC_HEALTH_CMD")
      if [ "$cur_health" != "$spec_health" ]; then
        action="RECREATE"
        reasons+="healthcheck "
      fi
    fi

    # Check URL/HOST env drift — catches stale network topology baked into
    # container env (fedimint April-11 bug: FM_P2P_URL pointed at old IP).
    # Only checks URL-shaped keys; other env drift (passwords rotated, etc.)
    # is intentionally ignored to avoid thrashing.
    if [ "$action" = "OK" ] && [ -n "$SPEC_ENV" ]; then
      for kv in $SPEC_ENV; do
        local env_key="${kv%%=*}"
        local env_val_spec="${kv#*=}"
        local is_url_key=false
        for suffix in $URL_ENV_SUFFIXES; do
          case "$env_key" in *"$suffix") is_url_key=true; break ;; esac
        done
        [ "$is_url_key" = "true" ] || continue
        local env_val_cur
        env_val_cur=$(container_env_val "$name" "$env_key")
        if [ "$env_val_cur" != "$env_val_spec" ]; then
          action="RECREATE"
          reasons+="env($env_key:$env_val_cur→$env_val_spec) "
          break
        fi
      done
    fi

    # Check bind mounts. This catches companion UIs recreated from older specs,
    # especially bitcoin-ui: its image intentionally does not bake nginx.conf,
    # so the rendered RPC proxy config must be mounted from the host.
    if [ "$action" = "OK" ] && [ -n "$SPEC_VOLUMES" ]; then
      for v in $SPEC_VOLUMES; do
        local mount_source mount_rest mount_target
        mount_source="${v%%:*}"
        mount_rest="${v#*:}"
        mount_target="${mount_rest%%:*}"
        [ -n "$mount_source" ] && [ -n "$mount_target" ] || continue
        if ! container_has_mount "$name" "$mount_source" "$mount_target"; then
          action="RECREATE"
          reasons+="mount($mount_target) "
          break
        fi
      done
    fi

    # Rootless Podman can occasionally leave a container running while its
    # rootlessport listener is gone. The container still looks healthy in
    # `podman ps`, but host-network UIs and backend status probes fail against
    # 127.0.0.1. Treat missing host listeners as spec drift.
    if [ "$action" = "OK" ] && [ -n "$SPEC_PORTS" ]; then
      for p in $SPEC_PORTS; do
        local host_port="${p%%:*}"
        [ -n "$host_port" ] || continue
        if ! host_port_listening "$host_port"; then
          action="RECREATE"
          reasons+="port($host_port-not-listening) "
          break
        fi
      done
    fi

    # Check if running
    if ! container_running "$name" && [ "$action" = "OK" ]; then
      action="START"
      reasons+="not-running "
    fi
  else
    action="CREATE"
    reasons+="missing "
  fi

  # Fix ownership regardless
  fix_ownership "$name"

  case "$action" in
    OK)
      ok "$name"
      COUNT_OK=$((COUNT_OK + 1))
      ;;
    START)
      if $CHECK_ONLY; then
        info "$name — would start ($reasons)"
      else
        if $PODMAN start "$name" >/dev/null 2>&1; then
          fixed "$name — started ($reasons)"
        else
          fail "$name — start failed"
          COUNT_FAILED=$((COUNT_FAILED + 1))
          FAILED_LIST+=" $name"
          return
        fi
      fi
      COUNT_FIXED=$((COUNT_FIXED + 1))
      ;;
    RECREATE)
      if $CHECK_ONLY; then
        info "$name — would recreate ($reasons)"
      else
        info "$name — recreating ($reasons)"
        $PODMAN stop "$name" >/dev/null 2>&1
        $PODMAN rm "$name" >/dev/null 2>&1
        if eval "$(build_run_cmd)" >/dev/null 2>&1; then
          fixed "$name — recreated ($reasons)"
        else
          fail "$name — recreate failed: $(eval "$(build_run_cmd)" 2>&1 | tail -1)"
          COUNT_FAILED=$((COUNT_FAILED + 1))
          FAILED_LIST+=" $name"
          return
        fi
      fi
      COUNT_FIXED=$((COUNT_FIXED + 1))
      ;;
    CREATE)
      if $CHECK_ONLY; then
        info "$name — would create ($reasons)"
      else
        for v in $SPEC_VOLUMES; do
          local host_dir="${v%%:*}"
          prepare_bind_source "$host_dir" || {
            COUNT_FAILED=$((COUNT_FAILED + 1))
            FAILED_LIST+=" $name"
            return
          }
        done
        if eval "$(build_run_cmd)" >/dev/null 2>&1; then
          fixed "$name — created"
        else
          fail "$name — create failed"
          COUNT_FAILED=$((COUNT_FAILED + 1))
          FAILED_LIST+=" $name"
          return
        fi
      fi
      COUNT_CREATED=$((COUNT_CREATED + 1))
      ;;
  esac
}

# ── Fix ownership ────────────────────────────────────────────────────
fix_ownership() {
  local name="$1"
  [ -z "$SPEC_DATA_DIR" ] && return
  [ ! -d "$SPEC_DATA_DIR" ] && return
  [ "$SPEC_DATA_UID" = "100000:100000" ] && return

  local expected_uid="${SPEC_DATA_UID%%:*}"
  local current_uid
  current_uid=$(stat -c '%u' "$SPEC_DATA_DIR" 2>/dev/null)

  if [ "$current_uid" != "$expected_uid" ]; then
    if $CHECK_ONLY; then
      info "$name — ownership: $current_uid → $SPEC_DATA_UID"
    else
      sudo chown -R "$SPEC_DATA_UID" "$SPEC_DATA_DIR" 2>/dev/null
      info "$name — fixed ownership → $SPEC_DATA_UID"
    fi
  fi
}

# ── Ensure secrets exist ─────────────────────────────────────────────
ensure_secrets() {
  local SECRETS_DIR="/var/lib/archipelago/secrets"
  sudo mkdir -p "$SECRETS_DIR" 2>/dev/null
  sudo chmod 700 "$SECRETS_DIR" 2>/dev/null

  for svc in bitcoin-rpc-password mempool-db-password btcpay-db-password mysql-root-db-password; do
    if [ ! -f "$SECRETS_DIR/$svc" ]; then
      if $CHECK_ONLY; then
        info "Would generate secret: $svc"
      else
        openssl rand -hex 16 | sudo tee "$SECRETS_DIR/$svc" >/dev/null
        sudo chmod 600 "$SECRETS_DIR/$svc"
        info "Generated secret: $svc"
      fi
    fi
  done

  # FED-07: the gateway's bcrypt credential is generated by the daemon's
  # container::secrets::ensure_gateway_credential (bcrypt, 0600, idempotent,
  # self-healing) — this script no longer generates it locally (that removed
  # the htpasswd host dependency AND the shipped-default fallback that used
  # to fire when htpasswd was absent). It never shipped, and never ships,
  # a fallback value.
  #
  # Legacy migration only: earlier builds wrote the plaintext under
  # fedimint-gateway-password instead of the manifest's canonical
  # fedimint-gateway-hash.pw name. Carry a pre-existing value forward under
  # the canonical name rather than regenerating (migrations never destroy
  # data or rotate a working credential); the legacy file is left in place —
  # plan 01-16 owns its retirement.
  if [ -f "$SECRETS_DIR/fedimint-gateway-password" ] && [ ! -f "$SECRETS_DIR/fedimint-gateway-hash.pw" ]; then
    if $CHECK_ONLY; then
      info "Would migrate legacy fedimint-gateway-password -> fedimint-gateway-hash.pw"
    else
      sudo cp "$SECRETS_DIR/fedimint-gateway-password" "$SECRETS_DIR/fedimint-gateway-hash.pw"
      sudo chmod 600 "$SECRETS_DIR/fedimint-gateway-hash.pw"
      info "Migrated legacy fedimint-gateway-password -> fedimint-gateway-hash.pw"
    fi
  fi
  if [ ! -f "$SECRETS_DIR/fedimint-gateway-hash" ]; then
    info "fedimint-gateway credential not yet generated — the daemon will generate a per-install bcrypt hash (no shipped default); the gateway container is skipped until then"
  fi

  # Reload after generation
  detect_environment
}

# ── Ensure bitcoin.conf ─────────────────────────────────────────────
ensure_bitcoin_conf() {
  local BITCOIN_CONF="/var/lib/archipelago/bitcoin/bitcoin.conf"
  sudo mkdir -p /var/lib/archipelago/bitcoin 2>/dev/null
  if [ ! -f "$BITCOIN_CONF" ] || ! sudo grep -q "^rpcauth=" "$BITCOIN_CONF" 2>/dev/null; then
    if ! $CHECK_ONLY && [ -n "$BITCOIN_RPC_PASS" ]; then
      local salt hash rpcauth
      salt=$(openssl rand -hex 16)
      hash=$(echo -n "$BITCOIN_RPC_PASS" | openssl dgst -sha256 -hmac "$salt" -hex 2>/dev/null | awk '{print $NF}')
      rpcauth="${BITCOIN_RPC_USER}:${salt}\$${hash}"
      # Only rpcauth + printtoconsole here — all other options are in SPEC_CUSTOM_ARGS
      # to avoid duplicate bind conflicts. printtoconsole=0: datadir debug.log
      # already has everything; console duplication spammed journald during IBD.
      sudo tee "$BITCOIN_CONF" >/dev/null << BTCEOF
rpcauth=${rpcauth}
printtoconsole=0
BTCEOF
      info "Generated bitcoin.conf"
    fi
  fi
  # Strip duplicate server/rpc/listen lines from existing conf files to avoid
  # conflicts with custom args. Knots can persist runtime args in
  # bitcoin_rw.conf, so clean both files.
  for conf in "$BITCOIN_CONF" "/var/lib/archipelago/bitcoin/bitcoin_rw.conf"; do
    if [ -f "$conf" ]; then
      sudo sed -i '/^server=/d; /^txindex=/d; /^rpcbind=/d; /^rpcallowip=/d; /^rpcport=/d; /^listen=/d; /^bind=/d; /^dbcache=/d; /^rpcthreads=/d; /^rpcworkqueue=/d' "$conf" 2>/dev/null
    fi
  done
  sudo chown -R 100101:100101 /var/lib/archipelago/bitcoin 2>/dev/null
}

# ── Ensure lnd.conf ─────────────────────────────────────────────────
ensure_lnd_conf() {
  local LND_CONF="/var/lib/archipelago/lnd/lnd.conf"
  sudo mkdir -p /var/lib/archipelago/lnd 2>/dev/null
  if [ ! -f "$LND_CONF" ] && [ -n "$BITCOIN_RPC_PASS" ]; then
    if ! $CHECK_ONLY; then
      sudo tee "$LND_CONF" >/dev/null << LNDEOF
[Application Options]
listen=0.0.0.0:9735
rpclisten=0.0.0.0:10009
restlisten=0.0.0.0:8080
debuglevel=info
noseedbackup=true

[Bitcoin]
bitcoin.mainnet=true
bitcoin.node=bitcoind

[Bitcoind]
bitcoind.rpchost=bitcoin-knots:8332
bitcoind.rpcuser=$BITCOIN_RPC_USER
bitcoind.rpcpass=$BITCOIN_RPC_PASS
bitcoind.rpcpolling=true
bitcoind.estimatemode=ECONOMICAL

[autopilot]
autopilot.active=false
LNDEOF
      info "Generated lnd.conf"
    fi
  fi
}

# ── Ensure bitcoin-ui nginx.conf ────────────────────────────────────
ensure_bitcoin_ui_nginx_conf() {
  local CONF_DIR="/var/lib/archipelago/bitcoin-ui"
  local CONF_PATH="$CONF_DIR/nginx.conf"
  [ -n "$BITCOIN_RPC_PASS" ] || return
  if $CHECK_ONLY; then
    [ -f "$CONF_PATH" ] || info "Would generate bitcoin-ui nginx.conf"
    return
  fi

  local auth_b64 tmp
  auth_b64=$(printf '%s' "${BITCOIN_RPC_USER}:${BITCOIN_RPC_PASS}" | base64 | tr -d '\n')
  sudo mkdir -p "$CONF_DIR" 2>/dev/null
  tmp="${CONF_PATH}.tmp.$$"
  sudo tee "$tmp" >/dev/null << EOF
server {
    # Loopback ONLY — this is the fourth copy of this declaration (the others
    # are the Rust template in container/bitcoin_ui_nginx.conf.template, the
    # image, and the manifest). Host networking means this nginx binds the
    # HOST's address, so \`listen 8334;\` served the Bitcoin screen on every
    # interface with no login. The app gate owns the external addresses now.
    listen 127.0.0.1:8334;
    server_name _;
    root /usr/share/nginx/html;
    index index.html;

    location /bitcoin-rpc/ {
        proxy_pass http://127.0.0.1:8332/;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header Authorization "Basic ${auth_b64}";
        add_header Access-Control-Allow-Origin *;
        add_header Access-Control-Allow-Methods "POST, GET, OPTIONS";
        add_header Access-Control-Allow-Headers "Content-Type, Authorization";
        if (\$request_method = OPTIONS) { return 204; }
    }

    location /bitcoin-status {
        proxy_pass http://127.0.0.1:5678/bitcoin-status;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        add_header Cache-Control "no-store";
    }

    location / {
        try_files \$uri \$uri/ /index.html;
    }
}
EOF
  if ! sudo cmp -s "$tmp" "$CONF_PATH" 2>/dev/null; then
    sudo mv "$tmp" "$CONF_PATH"
    sudo chmod 644 "$CONF_PATH"
    info "Generated bitcoin-ui nginx.conf"
  else
    sudo rm -f "$tmp"
  fi
}

# ── Ensure BTCPay databases ─────────────────────────────────────────
ensure_btcpay_db() {
  if container_running "archy-btcpay-db"; then
    $PODMAN exec archy-btcpay-db psql -U postgres -tc \
      "SELECT 1 FROM pg_database WHERE datname='nbxplorer'" 2>/dev/null | grep -q 1 || \
    $PODMAN exec archy-btcpay-db psql -U postgres -c \
      "CREATE DATABASE nbxplorer;" 2>/dev/null || true
  fi
}

# ══════════════════════════════════════════════════════════════════════
# MAIN
# ══════════════════════════════════════════════════════════════════════
START_TIME=$(date +%s)

header "Phase 0: Prerequisites"
ensure_secrets
detect_environment
ensure_bitcoin_conf
ensure_lnd_conf
ensure_bitcoin_ui_nginx_conf

TIER_NAMES=("Databases" "Core Infrastructure" "Services" "Applications" "Frontend UIs")

for tier in 0 1 2 3 4; do
  [ -n "$FILTER_TIER" ] && [ "$FILTER_TIER" != "$tier" ] && continue

  header "Tier $tier: ${TIER_NAMES[$tier]}"

  for name in "${ALL_CONTAINER_SPECS[@]}"; do
    [ -n "$FILTER_CONTAINER" ] && [ "$name" != "$FILTER_CONTAINER" ] && continue

    # Load spec to check tier before reconciling
    if load_spec "$name" && [ "$SPEC_TIER" = "$tier" ]; then
      reconcile "$name"
    fi
  done

  # After databases, ensure BTCPay DB schemas exist
  [ "$tier" = "0" ] && ensure_btcpay_db

  # Brief pause between tiers
  [ "$tier" -lt 4 ] && ! $CHECK_ONLY && sleep 2
done

# ── Summary ──────────────────────────────────────────────────────────
ELAPSED=$(( $(date +%s) - START_TIME ))
TOTAL=$((COUNT_OK + COUNT_FIXED + COUNT_CREATED + COUNT_SKIPPED + COUNT_FAILED))

echo ""
header "╔══════════════════════════════════════════════════╗"
header "║  RECONCILIATION REPORT                           ║"
header "╚══════════════════════════════════════════════════╝"
echo ""
echo -e "  Total:   ${BOLD}$TOTAL${NC}"
echo -e "  OK:      ${GREEN}$COUNT_OK${NC}"
echo -e "  Fixed:   ${CYAN}$COUNT_FIXED${NC}"
echo -e "  Created: ${CYAN}$COUNT_CREATED${NC}"
echo -e "  Skipped: ${YELLOW}$COUNT_SKIPPED${NC}"
echo -e "  Failed:  ${RED}$COUNT_FAILED${NC}"
[ -n "$FAILED_LIST" ] && echo -e "  Failed:  ${RED}$FAILED_LIST${NC}"
echo -e "  Duration: ${ELAPSED}s"
echo ""

[ "$COUNT_FAILED" -gt 0 ] && exit 1
exit 0
