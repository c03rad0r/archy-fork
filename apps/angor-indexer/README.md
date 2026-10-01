# Angor Indexer

Mainnet indexer endpoint for Angor, serving the existing Mempool explorer at
the same origin. The service reuses this node's Mempool frontend/backend and
Electrum index instead of creating another explorer or blockchain database.
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

## Verify the complete client flow

The root URL opens the Mempool explorer. `/health` and fee
estimates establish API availability; they do not prove that project discovery,
address history, or browser CORS works. Test a known funded project's address
history, its original Nostr announcement, the Explore page, and project details
in the actual Angor client. A certificate alone does not establish public routing.

Keep existing discovery relays when adding a new relay. A new relay has no
historical project data and does not automatically replicate other relays.
Even with existing relays, an empty Explore page can be a client discovery
failure: Angor Hub v2.0.0 was observed to stop after a batch whose announcements
all failed on-chain validation. The same failure reproduced with our indexer
and Angor's public indexer. Do not bypass the funding transaction's event-ID
commitment or substitute an unsigned announcement to make a project appear.

For opt-in read-only browser acceptance, install the frontend test dependencies
and Playwright Chromium, then run:

```sh
ANGOR_TEST_INDEXER=https://indexer.example.com/ \
ANGOR_TEST_RELAY=wss://relay.example.com/ \
ANGOR_TEST_RELAYS='["wss://relay.angor.io","wss://relay.example.com/"]' \
node tests/lifecycle/angor-public-browser.cjs
```

The relay under test must already contain the known original public project
announcement documented in the test. The test does not import events, send
funds, change your browser profile, or disable TLS verification. It checks the
funding transaction/event commitment and real browser discovery and details.
Relay signed writes, invalid-signature rejection, persistence, full node sync,
and proxy upgrade/renewal tests remain separate acceptance requirements.

## Packaging

Build the pinned image with:

```
podman build -t source.archipelago-foundation.org/chaum/angor-indexer:1.0.2 apps/angor-indexer/container
```

The image runs as UID 101 with a read-only root filesystem and no capabilities.
Only temporary nginx state is writable. Runtime DNS is read from resolv.conf so
Mempool recreation does not require editing IP addresses or restarting this app.
No app-specific Rust installer is required.

Source documentation: [Angor's official deployment guide](https://github.com/block-core/angor/blob/869dd43cf38332dd7128a284a6bf4c1cac44c1a7/docker/DEPLOY-INDEXER-AND-RELAY.md).
The app icon is based on [Angor’s dark-mode app icon](https://angor.io/images/app-icon-dark-mode.png), retrieved 2026-09-30. At the operator’s request, the outer corners use the same green as the background. The built-in imagegen edit preserved the black mark and filled the square green; the project asset is `neode-ui/public/assets/img/app-icons/angor-green.png`.

Tests and release acceptance are recorded in the next-release checklist. The
health probe establishes backend availability, not a guarantee that every
address query is indexed at the latest Bitcoin tip.

Install Mempool Explorer first. The declarative `install_prerequisites` check
refuses a new adapter installation if its Mempool API component is absent, before
creating an installed-app record. It does not install or resync Bitcoin for you.

## Explorer on the public indexer origin

The linked official deployment guide exposes **Mempool frontend and API together**
on the public indexer URL. It uses standard Mempool images and requires no custom
Angor fork or `ANGOR_ENABLED` flag.

The operator now requires that same browser experience: opening the configured
indexer domain must show the existing Mempool explorer, while Angor API requests
continue working on that origin. Reuse the existing Mempool stack, including its
live WebSocket feed; do not install a second explorer or blockchain database.

**Candidate 1.0.2:** `/` and frontend paths proxy to the existing Mempool
frontend; `/api/`, `/api/v1/`, `/health` and the WebSocket feed retain their
indexer routes. Version 1.0.1 served only service JSON at `/`. The candidate
remains pending deployment/release acceptance, which must cover assets and deep links,
desktop/mobile rendering, WebSocket updates, API/CORS/broadcast, trusted HTTPS,
restart/upgrade and management-access isolation before documenting it as shipped.
