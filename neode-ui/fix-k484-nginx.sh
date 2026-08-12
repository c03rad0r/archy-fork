#!/usr/bin/env bash

# Quick fix for k484 nginx SPA routing
# Run this after installing k484 if /admin route doesn't work

echo "🔧 Fixing k484 nginx configuration for SPA routing..."

if ! /usr/local/bin/docker ps --filter name=k484-test --format "{{.Names}}" | grep -q k484-test; then
    echo "❌ k484-test container is not running"
    echo "   Install k484 first through the Neode UI"
    exit 1
fi

# Update nginx config for SPA routing
/usr/local/bin/docker exec k484-test sh -c 'cat > /etc/nginx/conf.d/default.conf << "EOF"
server {
    listen       80;
    listen  [::]:80;
    server_name  localhost;
    root   /usr/share/nginx/html;
    index  index.html;

    location / {
        try_files $uri /index.html;
    }

    error_page   500 502 503 504  /50x.html;
    location = /50x.html {
        root   /usr/share/nginx/html;
    }
}
EOF
'

# Fix logo permissions
/usr/local/bin/docker exec k484-test chmod 644 /usr/share/nginx/html/k484-logo.png 2>/dev/null || true

# Restart nginx
/usr/local/bin/docker restart k484-test > /dev/null

echo "✅ k484 nginx config fixed!"
echo "   Try http://localhost:8103/admin now"

