#!/usr/bin/env bash
# tests/lifecycle/lib/ui-probes.bash
#
# HTTPS proxy + iframe URL probes. Sourced from bats files. Pairs with
# lib/rpc.bash but tests the URL surface a real user actually clicks
# (dashboard, /app/<id>/ proxy paths, direct-port iframes), not just the
# JSON-RPC API.
#
# Pattern: every probe is a skip-or-assert pair:
#   - if the container that backs the URL is not running → skip
#     (cleanly reports the dependency, doesn't false-fail)
#   - if it IS running → the URL MUST return 200
# That catches the "container up but UI broken" failure mode that the
# RPC-only tests miss (.198 today: archy-bitcoin-ui Up 12 minutes,
# but is the iframe actually serving usable HTML? this layer answers).

# Curl options for a probe: short timeout, follow redirects, ignore self-
# signed cert (the alpha fleet uses one), no proxy environment leak.
PROBE_CURL_OPTS=(-skfL -m 8 --noproxy "*")

# ────────────────────────────────────────────────────────────────────
# Container-state oracle
# ────────────────────────────────────────────────────────────────────

# True iff `name` is currently in the running state per podman.
probe_container_running() {
  local name="$1"
  [[ "$(podman inspect --format '{{.State.Running}}' "$name" 2>/dev/null)" == "true" ]]
}

# ────────────────────────────────────────────────────────────────────
# URL probes
# ────────────────────────────────────────────────────────────────────

# Probe an HTTPS URL — assert 200 and non-empty body.
# Usage:  probe_https_200 URL "human description"
probe_https_200() {
  local url="$1"
  local label="${2:-$url}"
  local body status
  body=$(curl "${PROBE_CURL_OPTS[@]}" -w '%{http_code}' "$url" 2>/dev/null) || {
    echo "probe_https_200: $label ($url) — curl failed (network/timeout)" >&2
    return 1
  }
  status="${body: -3}"
  body="${body:0:-3}"
  if [[ "$status" != "200" ]]; then
    echo "probe_https_200: $label ($url) returned $status (want 200)" >&2
    return 1
  fi
  if [[ -z "$body" ]]; then
    echo "probe_https_200: $label ($url) returned empty body" >&2
    return 1
  fi
  return 0
}

# Probe a URL backed by a container — skip if container is not running,
# assert 200 if it is. This is the standard shape for app UI tests.
# Usage: probe_app_url CONTAINER URL "human description"
probe_app_url() {
  local container="$1"
  local url="$2"
  local label="${3:-$url}"
  if ! probe_container_running "$container"; then
    skip "$label: backing container '$container' is not running"
  fi
  # An app's proxy/UI takes time to serve 200 after a (re)start — the backend
  # may still be unlocking/syncing (lnd) and the companion nginx reloading.
  # Retry up to ~90s rather than single-shot, so a readiness race isn't a fail.
  local deadline=$(( $(date +%s) + 90 ))
  while (( $(date +%s) < deadline )); do
    if probe_https_200 "$url" "$label"; then
      return 0
    fi
    sleep 3
  done
  run probe_https_200 "$url" "$label"
  [ "$status" -eq 0 ]
}

# Probe the archipelago dashboard itself (the SPA shell at https://node/).
# Asserts 200 and that the body looks like the Vue index, not an nginx
# default page. Catches "frontend tarball was extracted with the wrong
# layout" — see feedback_release_tarball_layout.md.
probe_dashboard_shell() {
  local host="${ARCHY_HOST:-127.0.0.1}"
  local url="${ARCHY_SCHEME:-https}://$host/"
  local body
  body=$(curl "${PROBE_CURL_OPTS[@]}" "$url" 2>/dev/null) || {
    echo "probe_dashboard_shell: $url — curl failed" >&2
    return 1
  }
  # Vue shell carries one of these markers: <div id="app">, the SPA bundle
  # tag, or the manifest link. Nginx default does not.
  if echo "$body" | grep -qE 'id="app"|<script.*\.js"|manifest\.webmanifest'; then
    return 0
  fi
  echo "probe_dashboard_shell: $url returned 200 but body doesn't look like the Vue shell" >&2
  echo "first 200 bytes: ${body:0:200}" >&2
  return 1
}

# Probe the catalog endpoint that the dashboard uses to populate tiles.
# Returns 0 if catalog is reachable AND has at least one entry.
probe_dashboard_catalog() {
  local host="${ARCHY_HOST:-127.0.0.1}"
  local body
  body=$(curl "${PROBE_CURL_OPTS[@]}" "${ARCHY_SCHEME:-https}://$host/catalog.json" 2>/dev/null) || {
    echo "probe_dashboard_catalog: /catalog.json fetch failed" >&2
    return 1
  }
  if ! echo "$body" | jq -e 'length > 0' >/dev/null 2>&1; then
    echo "probe_dashboard_catalog: /catalog.json is not a non-empty array/object" >&2
    return 1
  fi
  return 0
}
