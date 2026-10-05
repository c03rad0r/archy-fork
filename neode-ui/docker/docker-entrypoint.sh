#!/bin/sh
set -eu
# Copy nginx config template
cp /etc/nginx/nginx.conf.template /etc/nginx/nginx.conf

# Use the container's own DNS (Docker and Podman use different addresses).
# Resolve the optional external demo only on request, so its DNS outage cannot
# prevent the dashboard from starting. Only IP literals enter nginx syntax.
resolvers=$(awk '$1 == "nameserver" && $2 ~ /^[0-9a-fA-F:.]+$/ {
  if (index($2, ":")) printf "[%s] ", $2; else printf "%s ", $2
}' /etc/resolv.conf)
[ -n "$resolvers" ] || resolvers="1.1.1.1 8.8.8.8"
sed -i "s/__ARCHY_DNS_RESOLVERS__/$resolvers/g" /etc/nginx/nginx.conf

# Ensure client_max_body_size 0 is present (unlimited uploads)
# This is a safety net in case the config template was cached without the directive
if ! grep -q 'client_max_body_size' /etc/nginx/nginx.conf; then
  sed -i 's/http {/http {\n    client_max_body_size 0;/' /etc/nginx/nginx.conf
fi

exec nginx -g 'daemon off;'
