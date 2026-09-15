#!/bin/sh
# Resolve the backend again after container IP changes. Run after upstream
# placeholder substitution, so the repair also works on an existing container.
set -eu
conf=${1:-/etc/nginx/conf.d/nginx-mempool.conf}
resolv=${2:-/etc/resolv.conf}
backend=${BACKEND_MAINNET_HTTP_HOST:-127.0.0.1}
port=${BACKEND_MAINNET_HTTP_PORT:-8999}
resolver=$(awk '/^nameserver/ { print $2; exit }' "$resolv")
[ -n "$resolver" ] || { echo 'No DNS resolver configured' >&2; exit 1; }
case "$resolver" in *:*) resolver="[$resolver]" ;; esac
case "$backend" in *[!a-zA-Z0-9._-]*|'') echo 'Invalid backend hostname' >&2; exit 1 ;; esac
case "$port" in *[!0-9]*|'') echo 'Invalid backend port' >&2; exit 1 ;; esac

tmp=$(mktemp "${conf}.archy.XXXXXX")
trap 'rm -f "$tmp"' EXIT HUP INT TERM
awk -v backend="$backend" -v port="$port" -v resolver="$resolver" '
  BEGIN {
    base = "http://" backend ":" port
    print "# Archipelago: refresh backend DNS after container replacement."
    print "resolver " resolver " valid=5s ipv6=off; # archy-dns"
    print "resolver_timeout 3s; # archy-dns"
  }
  /# Archipelago: refresh backend DNS/ || /# archy-dns/ { next }
  /^[[:space:]]*location[[:space:]]/ { location = $2 }
  /^[[:space:]]*proxy_pass[[:space:]]/ && index($2, base) == 1 {
    target = $2
    sub(/;$/, "", target)
    path = substr(target, length(base) + 1)
    if (location != "/api/v1/ws" && location != "/ws" && location != "/api/v1" && location != "/api/") {
      print "Unexpected backend location: " location > "/dev/stderr"
      failed = 1; exit 1
    }
    if (path != "/" && path != "/api/v1" && path != "/api/v1/") {
      print "Unexpected backend URI mapping" > "/dev/stderr"
      failed = 1; exit 1
    }
    # Explicitly preserve prefix substitution and query arguments. A variable
    # proxy_pass without a URI forwards the rewritten URI and original args.
    print "\t\tset $mp_backend " backend ";"
    if (path != location)
      print "\t\trewrite ^" location "(.*)$ " path "$1 break;"
    print "\t\tproxy_pass http://$mp_backend:" port ";"
    count++
    next
  }
  /proxy_pass http:\/\/\$mp_backend:/ { count++ }
  { print }
  END {
    if (failed || count != 4) {
      print "Expected four backend proxies; refusing an incomplete DNS repair" > "/dev/stderr"
      exit 1
    }
  }
' "$conf" > "$tmp"
cat "$tmp" > "$conf"
