# Archipelago App Manifests

Containerized applications for the Archipelago Bitcoin Node OS. All apps run in rootless Podman with security hardening (cap-drop ALL, readonly root, non-root user, memory limits).

## App Categories

### Bitcoin & Lightning
- **bitcoin-knots** — Full Bitcoin node (v28.1)
- **lnd** — Lightning Network Daemon (v0.17.4-beta)
- **btcpay-server** — Payment processor (v1.13.5)
- **mempool** — Block explorer and fee estimator (v2.5.0)
- **electrumx** — Electrum server
- **fedimint** — Federated Bitcoin minting (v0.10.0)

### Nostr
- **nostr-rs-relay** — High-performance Rust relay (v0.9.0)
- **nostrudel** — Nostr web client (v0.40.0)

### Web5 & Identity
- **did-wallet** — Web5 DID Wallet

### Self-Hosted Services
- **nextcloud** (v28), **jellyfin** (v10.8.13), **immich** (release), **photoprism** (v240915)
- **vaultwarden** (v1.30.0-alpine), **penpot** (v2.4)
- **homeassistant** (v2024.1), **filebrowser** (v2.27.0), **searxng** (2024.11.17)
- **ollama** (v0.5.4), **grafana** (v10.2.0), **portainer** (v2.19.4)

### Networking
- **tailscale** (stable), **nginx-proxy-manager** (v2.12.1)

### Custom & External
- **indeedhub** — Bitcoin documentary streaming (custom build)
- **router** — Mesh routing and network management
- **botfights** — External web app

## Manifest Format

Each app has a `manifest.yml` defining container image, resources, dependencies, security policies, health checks, and network config. See [`docs/app-manifest-spec.md`](../docs/app-manifest-spec.md) for the spec.

## Quick Reference

- [PORTS.md](./PORTS.md) — Complete port mapping
- [QUICKSTART.md](./QUICKSTART.md) — Build and run apps
- [DEVELOPMENT.md](./DEVELOPMENT.md) — Development workflow
