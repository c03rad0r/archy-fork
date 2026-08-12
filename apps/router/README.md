# Archipelago Router

Mesh routing and local network management for Archipelago.

## Building

```bash
# From the apps directory
./build.sh router

# Or manually
cd router
docker build -t archipelago/router:latest .
# or
podman build -t archipelago/router:latest .
```

## Development

```bash
cd router
npm install
npm run dev
```

## Ports

- **8084**: Web UI (dev: 18084)
- **5353**: mDNS/Bonjour (dev: 15353)
- **1900**: SSDP (dev: 11900)

## Running Locally

```bash
docker run -p 8084:8080 -p 5353:5353/udp -p 1900:1900/udp \
  -v /tmp/archipelago-dev/router:/app/data \
  archipelago/router:latest
```
