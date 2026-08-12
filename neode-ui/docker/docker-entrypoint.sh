#!/bin/sh
# Copy nginx config template
cp /etc/nginx/nginx.conf.template /etc/nginx/nginx.conf

# Ensure client_max_body_size 0 is present (unlimited uploads)
# This is a safety net in case the config template was cached without the directive
if ! grep -q 'client_max_body_size' /etc/nginx/nginx.conf; then
  sed -i 's/http {/http {\n    client_max_body_size 0;/' /etc/nginx/nginx.conf
fi

exec nginx -g 'daemon off;'
