# Indeehub — Bitcoin Documentary Streaming

Bitcoin documentary streaming platform featuring God Bless Bitcoin and other educational content about Bitcoin, sovereignty, and decentralized technology.

Self-hosted Next.js app with Nostr identity sign-in via Archipelago's NIP-07 provider.

## Building the Image

The app image is built from the **indeehub-frontend** project at `~/Projects/indeehub-frontend`.

### Option 1: Use the build script

```bash
# From archy repo root
./apps/indeedhub/build-from-prototype.sh
```

### Option 2: Build from source directory

```bash
cd ~/Projects/indeehub-frontend
podman build -t localhost/indeedhub:latest -f ~/Projects/archy/apps/indeedhub/Dockerfile .
```

## Installing from App Store

1. **Build the image** using one of the options above (must exist before install)
2. Go to **Dashboard -> App Store** (Marketplace)
3. Find **Indeehub** and click **Install**
4. On first launch, pick a Nostr identity to sign in with
5. The app appears in **My Apps** once the container is running

## Port

- Web UI: 8190 (maps to container port 3000)

## Container

- Image: `localhost/indeedhub:latest` (built locally, not pulled from a registry)
- Runtime: Node.js 20 (Next.js standalone)
- Port: 8190 -> 3000
- Read-only root filesystem with tmpfs for /tmp and .next/cache

## Nostr Identity

On first launch, Archipelago shows a cypherpunk identity picker modal. Select which of your identities to use for NIP-07 signing. The NIP-07 provider is injected automatically via nginx proxy.

## Services

The app connects to the following external services (configured at build time):
- Indeehub API (content, auth, streaming)
- AWS S3 (media storage via CloudFront CDN)
- Nostr relays (via NIP-07 provider from Archipelago)
