#!/bin/sh
set -eu
# Resolve through the container runtime's DNS, including after dependency
# recreation. Never bake a container IP into the indexer endpoint.
DNS_RESOLVER=$(awk '/^nameserver[[:space:]]/ {print $2; exit}' /etc/resolv.conf)
case "$DNS_RESOLVER" in
  ''|*[!0-9a-fA-F.:]*) echo 'Container DNS resolver is unavailable' >&2; exit 1 ;;
esac
case "$DNS_RESOLVER" in *:*) DNS_RESOLVER="[$DNS_RESOLVER]" ;; esac
export DNS_RESOLVER
envsubst '${DNS_RESOLVER}' < /etc/angor-nginx.conf.template > /tmp/nginx.conf
exec nginx -c /tmp/nginx.conf -g 'daemon off;'
