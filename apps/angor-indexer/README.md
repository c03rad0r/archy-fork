# Angor Indexer

Headless mainnet API endpoint for Angor. The service reuses this node's Mempool
backend and Electrum index instead of creating a second blockchain database.
An unpruned, fully synced Bitcoin node is required. Installing against a pruned
node must show the existing archival-node requirement; it must never silently
unprune or replace its Bitcoin data.

## Connect Angor

Install **Angor Indexer** in the store. Its API appears under **Services**.
In Angor settings, use `http://<node-address>:8998/` as the custom indexer origin.
The `/health` endpoint reports readiness against Mempool's indexed block height;
it returns 503 while that backend is unavailable. Index building may take time.

Browser clients require a reachable HTTPS origin with a trusted certificate.
Configure your HTTPS reverse proxy to forward to port 8998, then use that HTTPS
origin in Angor. Do not disable browser TLS checks. The API supports both
`/api/v1/` and `/api/` paths, transaction broadcast, and CORS without cookies.

This endpoint intentionally exposes public blockchain queries and transaction
broadcast through the app gate without dashboard-cookie login. It has no Bitcoin
RPC password, wallet keys, or persistent wallet data. The backend stays on the
managed container network; its private port does not become publicly exposed.
You can change network access using the node's normal access controls.

## Relay

A relay is optional. Angor can continue using its configured external relays.
Install **Angor Relay** separately to host project metadata locally, then add
`ws://<node-address>:8091/` in Angor, or a trusted `wss://` proxy origin for browser
clients. Its storage and configuration are separate from the node's internal
relay; installing or uninstalling it does not change the internal relay.

## Packaging

Build the pinned image with:

```
podman build -t source.archipelago-foundation.org/chaum/angor-indexer:1.0.1 apps/angor-indexer/container
```

The image runs as UID 101 with a read-only root filesystem and no capabilities.
Only temporary nginx state is writable. Runtime DNS is read from resolv.conf so
Mempool recreation does not require editing IP addresses or restarting this app.
No app-specific Rust installer is required.

Source documentation: [Angor's official deployment guide](https://github.com/block-core/angor/blob/869dd43cf38332dd7128a284a6bf4c1cac44c1a7/docker/DEPLOY-INDEXER-AND-RELAY.md).
The unmodified icon comes from [angor.io/images/logo-text.svg](https://angor.io/images/logo-text.svg), retrieved 2026-09-30.

Tests and release acceptance are recorded in the next-release checklist. The
health probe establishes backend availability, not a guarantee that every
address query is indexed at the latest Bitcoin tip.

Install Mempool Explorer first. The declarative `install_prerequisites` check
refuses a new adapter installation if its Mempool API component is absent, before
creating an installed-app record. It does not install or resync Bitcoin for you.
