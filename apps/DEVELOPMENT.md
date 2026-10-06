# Archipelago Apps — Development Guide

## App Overview

### Bitcoin & Lightning
| App | Ports | Version |
|-----|-------|---------|
| bitcoin-knots | 8332 (RPC), 8333 (P2P) | v28.1 |
| lnd | 9735 (P2P), 10009 (gRPC), 8080 (REST) | v0.17.4-beta |
| btcpay-server | 23000 (HTTP) | v1.13.5 |
| mempool | 4080 (HTTP) | v2.5.0 |
| electrumx | 50001 (TCP), 50002 (SSL) | latest |
| fedimint | 8173 (API), 8174 (Web) | v0.10.0 |

### Nostr
| App | Ports | Version |
|-----|-------|---------|
| nostr-rs-relay | 8081 (WebSocket) | v0.9.0 |
| nostrudel | 8082 (HTTP) | v0.40.0 |

### Self-Hosted
| App | Port | Version |
|-----|------|---------|
| nextcloud | 8084 | v28 |
| jellyfin | 8096 | v10.8.13 |
| immich | 2283 | release |
| photoprism | 2342 | v240915 |
| vaultwarden | 8222 | v1.30.0-alpine |
| homeassistant | 8123 | v2024.1 |
| filebrowser | 8083 | v2.27.0 |
| searxng | 8888 | 2024.11.17 |
| ollama | 11434 | v0.5.4 |
| grafana | 3001 | v10.2.0 |
| portainer | 9000 | v2.19.4 |
| penpot | 8089 | v2.4 |

## Building Apps

```bash
cd apps
./build.sh           # Build all custom apps
./build.sh <app-id>  # Build specific app
```

Custom apps with local source: `router`, `did-wallet`. All other apps use official container images.

## App Structure

Each app directory contains:
- `manifest.yml` — Container configuration
- `Dockerfile` — Image definition (custom apps only)
- `README.md` — App-specific docs (custom apps only)
- `src/` — Source code (custom apps only)

## Running in Development

The Archipelago backend manages containers via rootless Podman. Install and start apps through the web UI Marketplace or via RPC:

```bash
curl -X POST http://localhost:5959/rpc/v1 \
  -H "Content-Type: application/json" \
  -d '{"method": "container-install", "params": {"manifest_path": "apps/router/manifest.yml"}}'
```

### Manual Testing (Podman)

```bash
# Build
./build.sh router

# Run directly with Podman
podman run -p 18084:8080 \
  -v /tmp/archipelago-dev/router:/app/data \
  localhost/archipelago/router:latest
```

## Integration Checklist

Adding a new app requires updates in multiple places:

- add `apps/<app-id>/manifest.yml`;
- add a Dockerfile and source directory only when the app is built locally;
- choose non-conflicting ports from [PORTS.md](./PORTS.md);
- declare `interfaces.main` for user-facing web UIs;
- declare generated secrets instead of hardcoding credentials;
- run `./scripts/validate-app-manifest.sh apps/<app-id>/manifest.yml`;
- regenerate catalogs with `python3 scripts/generate-app-catalog.py`;
- verify drift with `python3 scripts/check-app-catalog-drift.py --release --strict`;
- test install, launch, stop, start, restart, uninstall, and reinstall.

## Port Assignments

See [PORTS.md](./PORTS.md) for complete mapping. Dev ports are offset by +10000.

Before submitting an app, complete **Launch acceptance: credentials, signer, and HTTP nodes** in `docs/app-developer-guide.md`. A generated password needs an authenticated credential interstitial; native Nostr login needs a tested first-launch chooser. Container health alone is not launch acceptance.
