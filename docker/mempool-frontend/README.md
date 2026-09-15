# Mempool frontend DNS recovery

The stock v3.3.1 nginx configuration resolves `mempool-api` only when workers
start. Recreating the backend can change its Podman address while the frontend
continues to serve its static page, leaving all API/WebSocket requests offline.

Build and test the derived image before publishing:

```sh
podman build --pull=never -t source.archipelago-foundation.org/lfg2025/mempool-frontend:v3.3.1-archy1 docker/mempool-frontend
python3 scripts/test-mempool-dns-recovery.py
```

The base is pinned by digest. The wrapper preserves upstream runtime options,
then repairs all four local API/WebSocket routes after placeholder rendering.
DNS is cached for five seconds using the container network resolver. Explicit
rewrites preserve API prefixes and query arguments; backend absence does not
prevent nginx startup. An unexpected upstream configuration fails startup
instead of silently omitting the fix.

Use an Archipelago-specific image tag. Do not replace it with a stock upstream
mirror when updating mempool. Every upstream update must rebuild this wrapper
and pass the recovery test (backend absent, changed IP, HTTP and WebSocket
mapping, repeated repair, and frontend restart).

Publish the tested image before publishing the signed app catalog. Both the
mempool umbrella image mapping and the archy-mempool-web embedded manifest must
point at the patched image. Keep scripts/image-versions.sh in sync. The frontend
health check must reach `/api/v1/backend-info` through nginx, not only `/`.
