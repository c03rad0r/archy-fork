# Archipelago — Architecture

> **Bitcoin Node OS** — Flash to USB, install on hardware, manage via web UI.

**Stack**: Rust backend + Vue 3 + TypeScript (strict) + Vite + Tailwind CSS + Pinia + rootless Podman (Quadlet)
**Target OS**: Debian 13 (Trixie) — x86_64 and ARM64
**Status**: 1.8.0-alpha — single-node production gate green; multinode pass + release hardening in progress (see [`ROADMAP.md`](ROADMAP.md))

---

## System Layers

```
┌──────────────────────────────────────────────────────┐
│                 YOUR BROWSER                         │
│          Vue 3 SPA (Composition API + Pinia)         │
└──────────────────────┬───────────────────────────────┘
                       │ HTTP / WebSocket
┌──────────────────────┴───────────────────────────────┐
│                    NGINX                             │
│   /rpc/v1 → backend    /app/{id}/ → container        │
└──────────────────────┬───────────────────────────────┘
                       │ port 5678 (127.0.0.1)
┌──────────────────────┴───────────────────────────────┐
│              RUST BACKEND (core/)                    │
│  Auth, ~380 RPC methods, orchestrator + reconciler,  │
│  federation, mesh, identity, wallet, updates         │
└──────────────────────┬───────────────────────────────┘
                       │ Podman REST API socket + systemd Quadlet units
┌──────────────────────┴───────────────────────────────┐
│           ROOTLESS PODMAN CONTAINERS                 │
│  50+ manifest-driven apps the orchestrator owns and  │
│  self-heals; companion UIs run as systemd Quadlet    │
└──────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────┐
│              DEBIAN 13 (Trixie)                      │
│  systemd, UFW, Tor, AppArmor, Reticulum daemon       │
└──────────────────────────────────────────────────────┘
```

## Codebase Stats

| Component | Lines | Files |
|-----------|-------|-------|
| Rust backend (`core/`) | ~117,000 | ~334 |
| TypeScript/Vue (`neode-ui/src/`) | ~69,000 | ~325 |
| Shell scripts (`scripts/`) | — | ~51 |
| Packaged apps (`apps/*/manifest.yml`) | — | 51 |

## Backend Crates (`core/`)

Workspace members (root `core/Cargo.toml`):

| Crate | Purpose |
|-------|---------|
| `archipelago` | Main binary — API (~380 RPC methods), container orchestrator + boot reconciler, mesh, identity/federation, wallet, updates, marketplace |
| `container` (`archipelago-container`) | Podman REST client, canonical manifest schema, Quadlet compiler, health monitor, signed app catalog, image verification |
| `security` (`archipelago-security`) | AppArmor/seccomp container policy generation, secrets manager |
| `openwrt` (`archipelago-openwrt`) | TollGate gateway provisioning over SSH/UCI |
| `performance` (`archipelago-performance`) | Resource limits |

Also on disk but **not** workspace members (standalone/legacy, cleanup tracked in the hardening plan §G): `models`, `helpers`, `js-engine`, `container-init`.

### Key Backend Modules

```
core/archipelago/src/
├── api/handler/            — HTTP routing (/rpc, /health, /dwn, /ws)
├── api/rpc/dispatcher.rs   — RPC dispatch (~380 method arms)
├── api/rpc/package/        — App install/lifecycle/stacks (multi-container)
├── container/              — prod_orchestrator, boot_reconciler, quadlet,
│                             app_catalog (signed, embedded manifests),
│                             version_config, crash_recovery, secrets
├── trust/                  — release-root anchor (pinned Ed25519 pubkey),
│                             detached-signature verify, did:key
├── mesh/                   — Meshtastic + MeshCore + Reticulum transports,
│                             X3DH/double-ratchet crypto, outbox/scheduler,
│                             mesh AI assistant, bitcoin relay
├── federation/             — multi-node federation over Tor, state sync
├── identity.rs / identity_manager.rs — Ed25519 did:key, multi-identity
├── credentials/            — W3C Verifiable Credentials
├── nostr_discovery.rs      — Nostr presence (NIP-33 kind 30078)
├── nostr_handshake.rs      — NIP-44 encrypted peer comms
├── marketplace.rs          — decentralized app marketplace (Nostr NIP-78,
│                             DID-signed manifests, trust scoring)
├── wallet/                 — LND integration, ecash (Fedimint/Cashu)
├── update.rs               — signed OTA: resumable download, rollback,
│                             post-update self-verify window
├── session.rs / auth.rs    — sessions (persisted), Argon2id, TOTP
├── transport/ / network/   — Tor transport, DWN store/sync
└── fips/ / swarm/ / streaming/ — federation IPS anchor, P2P swarm (gated), streaming (WIP)
```

## App Platform (as built)

An app is a directory `apps/<id>/manifest.yml` parsed by the canonical schema
in `core/container/src/manifest.rs`. A manifest declares identity, a container
source (**image XOR build**), and runtime shape: ports, volumes (confined to
`/var/lib/archipelago`), generated config files, environment, devices,
resources, health checks, and the launch interface. Ergonomics are declarative
too: `derived_env` (host-fact templating), `secret_env` (podman secrets — values
never appear in `podman inspect` or unit files), `generated_secrets` /
`generated_certs` (self-healing), `network_aliases`, `data_uid`, and
allow-listed `post_install` hooks that run inside the app's own sandbox.

**Install** creates a rootless container the orchestrator owns; the companion
UI containers run as systemd **Quadlet units under `user.slice`** (the
validated path being flipped to default for all apps), so those survive backend
restarts and reboots outright, and the reconciler rebuilds any container that
vanishes. Multi-container apps (BTCPay, Mempool, Immich, NetBird, IndeeHub) are sets of
per-member manifests installed via the stack orchestrator on an app-local
network with readiness gates and generated cross-service secrets. A
level-triggered **boot reconciler** converges actual state to desired state
every 30 seconds.

**Distribution**: the signed catalog (`releases/app-catalog.json`, Ed25519
detached signature over canonical JSON, verified against the pinned
release-root anchor in `trust/anchor.rs`) embeds the full manifest per app;
nodes overlay catalog manifests over disk files (catalog wins), so apps can
ship without OTA disk files. A curated subset (27 apps) powers the store UI
(`app-catalog/catalog.json`). A parallel **decentralized marketplace**
(Nostr NIP-78 discovery, DID-signed manifests, federation-weighted trust
scoring, Lightning purchase invoices) is implemented as a second,
community-distribution channel.

**Security invariants** enforced at manifest validation: read-only root and
no-new-privileges by default, capability allow-list, `network_policy ∈
{isolated, bridge, host}`, bind mounts confined to `/var/lib/archipelago`, no
privileged containers, rootless only.

## Frontend (`neode-ui/src/`)

```
├── api/                    — RPC client, WebSocket, container client
├── views/                  — Dashboard, Apps, Marketplace, Cloud, Server,
│                             Mesh, Web5, Settings, Monitoring, Fleet, Chat,
│                             onboarding flow (11 screens), kiosk, recovery
├── components/             — EasyHome, ModeSwitcher, BootScreen, SpotlightSearch, …
├── stores/                 — Pinia: app, install, mesh, cloud, goals, uiMode,
│                             controller (gamepad), aiPermissions, …
├── composables/            — useControllerNav, useToast, useNavSounds, …
├── router/                 — ~51 routes
└── style.css               — global glassmorphism theme
```

Three UI modes (Pro/Easy/Chat), gamepad navigation, i18n, PWA. Tested with
Vitest + Playwright. AIUI is a separate external app surfaced via nginx.

## Mesh Networking

Three LoRa transports behind one chat UI and a common `MeshRadioDevice`
surface:

- **Meshtastic** — in-process async serial driver (protobuf over SLIP)
- **MeshCore** — framed-serial protocol; phone companion apps speak this
- **Reticulum (RNS/LXMF)** — host-supervised Python daemon
  (`reticulum-daemon/`, PyInstaller-packaged, one per RNode radio) speaking
  Unix-socket JSON-RPC to the backend; `archy-rnodeconf` ships as an OS-level
  radio config tool

End-to-end encryption uses X3DH key agreement + double-ratchet. Extras: image
and voice attachments, mesh AI assistant (`!ai`), Bitcoin balance relay over
mesh, steganography, store-and-forward outbox.

## Networking

- **Container DNS**: app-local Podman networks with `network_aliases`; aardvark-dns resolution
- **Tor**: system daemon, SOCKS5 on 9050, hidden services per node; all inter-node federation traffic
- **Federation**: invite-based joining, DID-based trust levels, state sync, cross-node app deploy
- **UFW**: `DEFAULT_FORWARD_POLICY="ACCEPT"` required for LAN container access
- **OpenWrt/TollGate**: gateway provisioning via the `openwrt` crate

## Security Model

| Layer | Measures |
|-------|----------|
| OS | Debian hardening, AppArmor, minimal packages |
| Nginx | CSP headers, rate limiting, auth_request, session validation |
| Backend | Input validation, CSRF, session auth, bind 127.0.0.1 only |
| Containers | Rootless Podman, cap-drop ALL + reviewed allow-list, readonly root, no-new-privileges, memory limits |
| Supply chain | Ed25519-signed release manifests + app catalog against a pinned release-root anchor; auto-apply refuses unsigned |
| Crypto | Ed25519 signatures, ChaCha20-Poly1305 encryption, Argon2id password hashing (transparent bcrypt upgrade), constant-time comparisons |
| Network | Tor hidden services, UFW firewall, SSRF prevention |

## Data Paths

| Data | Path |
|------|------|
| App data | `/var/lib/archipelago/{app-id}/` |
| Identity | `/var/lib/archipelago/identity/` |
| Multi-identity | `/var/lib/archipelago/identities/` |
| Federation | `/var/lib/archipelago/federation/` |
| DWN messages | `/var/lib/archipelago/dwn/messages/` |
| Credentials | `/var/lib/archipelago/credentials/` |
| Backups | `/var/lib/archipelago/backups/` (ChaCha20-Poly1305) |
| Secrets | `/var/lib/archipelago/secrets/{app-id}/` (0600, service-user-owned) |
| Sessions | `/var/lib/archipelago/sessions.json` |
| Marketplace cache | `/var/lib/archipelago/marketplace/` |
| Frontend | `/opt/archipelago/web-ui/` |
| Backend binary | `/usr/local/bin/archipelago` |

## Key Features (Working)

- 50+ containerized apps with one-click install/manage; full lifecycle matrix repeatedly green on real hardware
- Bitcoin Core **and** Knots with per-app version pinning and safe switching; LND + Core Lightning
- Multi-node federation with invite-based joining and trust levels
- W3C DID identity (did:key, DID Documents, Verifiable Credentials)
- Nostr: NIP-33 node discovery, NIP-44/NIP-04 encryption, NIP-07 signer bridge for iframe apps, relay hosting
- Decentralized marketplace (NIP-78 discovery, trust scoring, Lightning purchases)
- File sharing with access controls (free/peers-only/paid via LN, on-chain, ecash)
- Encrypted backups (Argon2 + ChaCha20-Poly1305)
- Health monitoring + level-triggered reconciler with tiered auto-restart
- Tri-protocol LoRa mesh (Meshtastic / MeshCore / Reticulum) with E2E crypto
- Signed OTA updates with rollback and post-update self-verification
- Three-mode UI (Pro/Easy/Chat), gamepad navigation, real-time WebSocket updates
- Bootable ISO installer (`image-recipe/`), Android companion app

## Further Documentation

| Doc | Purpose |
|-----|---------|
| [`ROADMAP.md`](ROADMAP.md) | Shipped / in-progress / planned |
| [`developer-guide.md`](developer-guide.md) | Dev setup, workflow, code conventions |
| [`api-reference.md`](api-reference.md) | RPC endpoint reference |
| [`app-developer-guide.md`](app-developer-guide.md) | Building and publishing apps |
| [`app-manifest-spec.md`](app-manifest-spec.md) | The `manifest.yml` schema |
| [`user-walkthrough.md`](user-walkthrough.md) | End-user installation and usage guide |
| [`troubleshooting.md`](troubleshooting.md) | Diagnostic scenarios and solutions |
| [`multi-node-architecture.md`](multi-node-architecture.md) | Federation protocol design |
| [`marketplace-protocol.md`](marketplace-protocol.md) | Decentralized app discovery via Nostr |
| [`archive/`](archive/) | Historical audits, session logs, shipped designs |
