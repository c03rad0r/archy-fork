#!/bin/bash
set -e

# Source pinned image versions (single source of truth)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
[ -f "$SCRIPT_DIR/image-versions.sh" ] && . "$SCRIPT_DIR/image-versions.sh"

# Fix corrupted IndeedHub containers + SearXNG
# All images were exported as the same (wrong) image during multi-node deploy.
# This script: stops broken containers, removes them, recreates with correct images.

echo "=== IndeedHub Container Fix Script ==="
PODMAN_IMAGE_CHECK_TIMEOUT="${PODMAN_IMAGE_CHECK_TIMEOUT:-10}"

# Detect node IP (Tailscale or LAN)
NODE_IP=$(hostname -I | awk '{for(i=1;i<=NF;i++) if($i ~ /^100\./) print $i}')
if [ -z "$NODE_IP" ]; then
  NODE_IP=$(hostname -I | awk '{print $1}')
fi
echo "Node IP: $NODE_IP"

NETWORK="indeedhub-build_indeedhub-network"

# Load custom images if tar exists
if [ -f /tmp/indeedhub-images.tar ]; then
  echo "Loading custom images from tar..."
  podman load < /tmp/indeedhub-images.tar 2>&1 | tail -5
fi

# Verify correct images are available
echo "Verifying images..."
for img in "${INDEEDHUB_REDIS_IMAGE}" "${MINIO_IMAGE}" "${INDEEDHUB_POSTGRES_IMAGE}" "${NOSTR_RS_RELAY_IMAGE}" "${SEARXNG_IMAGE}" "localhost/indeedhub:local" "localhost/indeedhub-build_api:local" "localhost/indeedhub-build_ffmpeg-worker:local"; do
  if ! timeout --kill-after=2s "${PODMAN_IMAGE_CHECK_TIMEOUT}s" podman image exists "$img" 2>/dev/null; then
    echo "ERROR: Missing image $img"
    exit 1
  fi
done
echo "All images verified."

# Ensure network exists
if ! podman network exists "$NETWORK" 2>/dev/null; then
  echo "Creating network $NETWORK..."
  podman network create "$NETWORK" 2>/dev/null || true
fi

# Stop all affected containers
echo "Stopping containers..."
for c in indeedhub indeedhub-build_api_1 indeedhub-build_ffmpeg-worker_1 indeedhub-relay indeedhub-redis indeedhub-minio indeedhub-postgres searxng; do
  podman stop "$c" 2>/dev/null || true
done

# Remove all affected containers
echo "Removing containers..."
for c in indeedhub indeedhub-build_api_1 indeedhub-build_ffmpeg-worker_1 indeedhub-relay indeedhub-redis indeedhub-minio indeedhub-postgres searxng; do
  podman rm -f "$c" 2>/dev/null || true
done

# 1. PostgreSQL (must start first — others depend on it)
echo "Creating postgres..."
podman run -d --name indeedhub-postgres \
  --restart unless-stopped \
  --network "$NETWORK" --network-alias postgres \
  -v indeedhub-postgres-data:/var/lib/postgresql/data \
  -e POSTGRES_USER=indeedhub \
  -e POSTGRES_PASSWORD=indeehhub-archy-2026 \
  -e POSTGRES_DB=indeedhub \
  "$INDEEDHUB_POSTGRES_IMAGE"

# Wait for postgres to be ready
echo "Waiting for postgres..."
for i in $(seq 1 15); do
  if podman exec indeedhub-postgres pg_isready -U indeedhub 2>/dev/null; then
    echo "Postgres ready."
    break
  fi
  sleep 2
done

# 2. Redis
echo "Creating redis..."
podman run -d --name indeedhub-redis \
  --restart unless-stopped \
  --network "$NETWORK" --network-alias redis \
  -v indeedhub-redis-data:/data \
  "$INDEEDHUB_REDIS_IMAGE" \
  redis-server --appendonly yes

# 3. MinIO
echo "Creating minio..."
podman run -d --name indeedhub-minio \
  --restart unless-stopped \
  --network "$NETWORK" --network-alias minio \
  -v indeedhub-minio-data:/data \
  -e MINIO_ROOT_USER=indeeadmin \
  -e MINIO_ROOT_PASSWORD=indeeadmin2026 \
  "${MINIO_IMAGE}" \
  server /data --console-address ":9001"

# 4. Nostr Relay
echo "Creating relay..."
podman run -d --name indeedhub-relay \
  --restart unless-stopped \
  --network "$NETWORK" --network-alias relay \
  -v indeedhub-relay-data:/usr/src/app/db \
  "${NOSTR_RS_RELAY_IMAGE}"

# 5. API
echo "Creating api..."
podman run -d --name indeedhub-build_api_1 \
  --restart unless-stopped \
  --network "$NETWORK" --network-alias api \
  -e ENVIRONMENT=production \
  -e PORT=4000 \
  -e DOMAIN="$NODE_IP" \
  -e FRONTEND_URL="http://$NODE_IP" \
  -e DATABASE_HOST=postgres \
  -e DATABASE_PORT=5432 \
  -e DATABASE_USER=indeedhub \
  -e DATABASE_PASSWORD=indeehhub-archy-2026 \
  -e DATABASE_NAME=indeedhub \
  -e QUEUE_HOST=redis \
  -e QUEUE_PORT=6379 \
  -e "QUEUE_PASSWORD=" \
  -e S3_ENDPOINT=http://minio:9000 \
  -e AWS_REGION=us-east-1 \
  -e AWS_ACCESS_KEY=indeeadmin \
  -e AWS_SECRET_KEY=indeeadmin2026 \
  -e S3_PRIVATE_BUCKET_NAME=indeedhub-private \
  -e S3_PUBLIC_BUCKET_NAME=indeedhub-public \
  -e S3_PUBLIC_BUCKET_URL=/storage \
  -e "BTCPAY_URL=" \
  -e "BTCPAY_API_KEY=" \
  -e "BTCPAY_STORE_ID=" \
  -e "BTCPAY_WEBHOOK_SECRET=" \
  -e NOSTR_JWT_SECRET=archipelago-indeehhub-jwt-secret-2026 \
  -e NOSTR_JWT_EXPIRES_IN=7d \
  -e AES_MASTER_SECRET=0123456789abcdef0123456789abcdef \
  -e "ADMIN_API_KEY=" \
  -e NODE_OPTIONS=--max-old-space-size=1024 \
  --health-cmd "wget --no-verbose --tries=1 --spider http://localhost:4000/nostr-auth/health || exit 1" \
  --health-interval 60s \
  --health-timeout 30s \
  --health-retries 5 \
  --health-start-period 60s \
  localhost/indeedhub-build_api:local \
  sh -c "echo 'Running database migrations...' && npx typeorm migration:run -d dist/database/ormconfig.js && echo 'Migrations complete.' && npm run start:prod"

# 6. FFmpeg Worker
echo "Creating ffmpeg-worker..."
podman run -d --name indeedhub-build_ffmpeg-worker_1 \
  --restart unless-stopped \
  --network "$NETWORK" --network-alias ffmpeg-worker \
  -e ENVIRONMENT=production \
  -e DATABASE_HOST=postgres \
  -e DATABASE_PORT=5432 \
  -e DATABASE_USER=indeedhub \
  -e DATABASE_PASSWORD=indeehhub-archy-2026 \
  -e DATABASE_NAME=indeedhub \
  -e QUEUE_HOST=redis \
  -e QUEUE_PORT=6379 \
  -e "QUEUE_PASSWORD=" \
  -e S3_ENDPOINT=http://minio:9000 \
  -e AWS_REGION=us-east-1 \
  -e AWS_ACCESS_KEY=indeeadmin \
  -e AWS_SECRET_KEY=indeeadmin2026 \
  -e S3_PRIVATE_BUCKET_NAME=indeedhub-private \
  -e S3_PUBLIC_BUCKET_NAME=indeedhub-public \
  -e S3_PUBLIC_BUCKET_URL=/storage \
  -e AES_MASTER_SECRET=0123456789abcdef0123456789abcdef \
  localhost/indeedhub-build_ffmpeg-worker:local

# 7. IndeedHub Frontend
echo "Creating indeedhub frontend..."
podman run -d --name indeedhub \
  --restart unless-stopped \
  --network "$NETWORK" \
  -p 7778:7777 \
  --label "com.archipelago.app=indeedhub" \
  --label "com.archipelago.title=IndeedHub" \
  --label "com.archipelago.version=0.1.0" \
  --label "com.archipelago.category=media" \
  --label "com.archipelago.port=7777" \
  localhost/indeedhub:local

# Fix IndeedHub for iframe: remove X-Frame-Options, inject nostr-provider, hardcode container IPs
sleep 3
if podman ps --format '{{.Names}}' 2>/dev/null | grep -q "^indeedhub$"; then
  podman exec indeedhub sed -i "/X-Frame-Options/d" /etc/nginx/conf.d/default.conf 2>/dev/null || true

  # Inject nostr-provider.js if available
  if [ -f /opt/archipelago/web-ui/nostr-provider.js ]; then
    podman cp /opt/archipelago/web-ui/nostr-provider.js indeedhub:/usr/share/nginx/html/nostr-provider.js 2>/dev/null || true
  fi

  # Add nostr-provider location block + sub_filter
  if ! podman exec indeedhub grep -q "nostr-provider" /etc/nginx/conf.d/default.conf 2>/dev/null; then
    podman exec indeedhub cat /etc/nginx/conf.d/default.conf > /tmp/ih-nginx.conf 2>/dev/null
    sed -i "/location = \/sw.js {/i\\    location = /nostr-provider.js {\n        add_header Cache-Control \"no-cache, no-store, must-revalidate\";\n        expires off;\n    }\n" /tmp/ih-nginx.conf
    sed -i "/try_files.*index.html/a\\        sub_filter_once on;\n        sub_filter '</head>' '<script src=\"/nostr-provider.js\"></script></head>';" /tmp/ih-nginx.conf
    podman cp /tmp/ih-nginx.conf indeedhub:/etc/nginx/conf.d/default.conf 2>/dev/null || true
    rm -f /tmp/ih-nginx.conf
  fi

  # Fix X-Forwarded-Prefix for NIP-98 URL reconstruction in iframe context
  # The outer Archipelago nginx sets X-Forwarded-Prefix to /app/indeedhub;
  # the inner nginx must pass it through (appending /api) instead of hardcoding /api
  podman exec indeedhub sed -i 's|proxy_set_header X-Forwarded-Prefix /api;|proxy_set_header X-Forwarded-Prefix $http_x_forwarded_prefix/api;|' /etc/nginx/conf.d/default.conf 2>/dev/null || true

  # Replace DNS-based upstream resolution with hardcoded container IPs
  # (podman DNS resolver 127.0.0.11 is unreliable, causing 502 errors)
  API_IP=$(podman inspect indeedhub-build_api_1 --format "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}" 2>/dev/null)
  MINIO_IP=$(podman inspect indeedhub-minio --format "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}" 2>/dev/null)
  RELAY_IP=$(podman inspect indeedhub-relay --format "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}" 2>/dev/null)

  if [ -n "$API_IP" ] && [ -n "$MINIO_IP" ] && [ -n "$RELAY_IP" ]; then
    podman exec indeedhub cat /etc/nginx/conf.d/default.conf > /tmp/ih-nginx.conf 2>/dev/null
    sed -i "s|resolver 127.0.0.11 valid=30s ipv6=off;||g" /tmp/ih-nginx.conf
    sed -i "s|set \$api_upstream http://api:4000;|set \$api_upstream http://$API_IP:4000;|g" /tmp/ih-nginx.conf
    sed -i "s|set \$minio_upstream http://minio:9000;|set \$minio_upstream http://$MINIO_IP:9000;|g" /tmp/ih-nginx.conf
    sed -i "s|set \$relay_upstream http://relay:8080;|set \$relay_upstream http://$RELAY_IP:8080;|g" /tmp/ih-nginx.conf
    sed -i "s|proxy_set_header Host \$host;|proxy_set_header Host \$http_host;|g" /tmp/ih-nginx.conf
    podman cp /tmp/ih-nginx.conf indeedhub:/etc/nginx/conf.d/default.conf 2>/dev/null || true
    rm -f /tmp/ih-nginx.conf
    echo "Patched IndeedHub nginx with container IPs (API=$API_IP MINIO=$MINIO_IP RELAY=$RELAY_IP)"
  fi

  podman exec indeedhub nginx -s reload 2>/dev/null || true
  echo "Applied IndeedHub iframe fix."
fi

# 8. SearXNG (standalone — no cap-drop ALL, searxng needs write access to /etc/searxng/)
echo "Creating searxng..."
podman run -d --name searxng \
  --restart unless-stopped \
  -p 8888:8080 \
  "${SEARXNG_IMAGE}"

echo ""
echo "=== Verifying container status ==="
sleep 5
podman ps -a --filter name=indeedhub --filter name=searxng --format "table {{.Names}}\t{{.Status}}" 2>&1
echo ""
echo "=== FIX COMPLETE ==="
