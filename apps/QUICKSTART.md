# Quick Start Guide - Archipelago Apps

This guide will help you get all prepackaged apps running in your development environment.

## Prerequisites

1. **Container Runtime**: Podman or Docker
   ```bash
   # Check if available
   podman --version  # or docker --version
   ```

2. **Node.js** (for custom apps): v18+
   ```bash
   node --version
   ```

3. **Archipelago Backend**: Running in dev mode
   ```bash
   cd core
   ARCHIPELAGO_DEV_MODE=true cargo run --bin archipelago
   ```

## Building Apps

### Build All Apps

```bash
cd apps
./build.sh
```

This will build all apps that have Dockerfiles. Standard apps (bitcoin-core, lnd, etc.) will use their official images, while custom apps (router, did-wallet) will be built from source.

### Build Specific App

```bash
./build.sh router
./build.sh did-wallet
```

## Running Apps via Archipelago

Once the backend is running, you can install and start apps via:

1. **UI**: Navigate to http://localhost:8100 and use the Apps/Marketplace interface
2. **RPC**: Use the container-install RPC method

```bash
curl -X POST http://localhost:5959/rpc/v1 \
  -H "Content-Type: application/json" \
  -d '{
    "method": "container-install",
    "params": {
      "manifest_path": "apps/router/manifest.yml"
    }
  }'
```

## Port Access

In development mode, apps are accessible on offset ports:

- **Router**: http://localhost:18084
- **DID Wallet**: http://localhost:18083
- **Nostr RS Relay**: http://localhost:18081
- **Strfry**: http://localhost:18082

See [PORTS.md](./PORTS.md) for complete port mapping.

## Development Workflow

### For Custom Apps (router, did-wallet)

1. **Make changes** to source code in `apps/<app-id>/src/`
2. **Rebuild** the container:
   ```bash
   ./build.sh <app-id>
   ```
3. **Restart** the container via Archipelago UI or RPC

### For Standard Apps

Standard apps use official images. To customize:
1. Create a custom Dockerfile that extends the official image
2. Add your customizations
3. Update the manifest to use your custom image

## Testing Locally

You can test apps directly without Archipelago:

```bash
# Build
./build.sh router

# Run
docker run -p 18084:8080 \
  -v /tmp/archipelago-dev/router:/app/data \
  archipelago/router:latest
```

## Next Steps

- Read [DEVELOPMENT.md](./DEVELOPMENT.md) for detailed development information
- Check [PORTS.md](./PORTS.md) for port assignments
- Review individual app READMEs for app-specific details
