# Archipelago Roadmap

_Last updated: 2026-07-08. This is the public-facing summary. The live,
open engineering work is tracked in the public issue tracker._

## North star

A world-class, **developer-ready app platform**: every app manifest-driven,
manifests distributed via a **signed registry**, and third-party developers
publishing through an **external/decentralized marketplace** — all rootless,
secure, robust, and 100%-uptime-capable.

Five pillars every app must satisfy: Quadlet-everywhere · level-triggered
reconciler · lifecycle-bulletproof (full test matrix, repeatedly green) ·
data-driven (no host changes, no per-app binary code) · rootless +
security-first.

## ✅ Shipped

- **Single-node production gate GREEN** (2026-06-23) — the full destructive
  lifecycle matrix (install / UI / stop / start / restart / reinstall /
  reboot-survive / backend-restart-survive / uninstall) passed 5 consecutive
  times with zero failures on real hardware. This was the first exit criterion.
- **Manifest-driven app platform** — 50+ apps as declarative manifests; all
  multi-container stacks (BTCPay, Mempool, Immich, NetBird, IndeeHub) install
  through the orchestrator; generated-secrets system replaces per-app secret code.
- **Rust orchestrator + level-triggered reconciler** — the bash-script era is
  retired; drift self-heals on a 30-second loop.
- **Registry-distributed manifests** — the signed catalog embeds full manifests
  per app; nodes verify against the pinned release-root key and overlay
  catalog manifests over disk files (catalog wins).
- **Release signing ceremony** (2026-07-02) — release-root Ed25519 key pinned
  in the binary; OTA release manifests and the app catalog are signed;
  auto-apply refuses unsigned manifests.
- **1.8.0 hardening batches** — supply-chain signature verification, deepened
  post-OTA health checks, async-executor and secret-handling fixes, browser
  origin checks, dist shrink.
- **Reticulum third mesh transport** — RNS/LXMF over real LoRa hardware
  (RNode), interop verified against Sideband; joins Meshtastic and MeshCore
  behind one chat UI with X3DH + double-ratchet E2E, attachments, and mesh AI.
- **Bitcoin multi-version** — Core and Knots with per-app version pinning and
  safe switching (fleet rollout pending below).
- **Decentralized marketplace backend** — Nostr NIP-78 discovery, DID-signed
  manifests, federation-weighted trust scoring, Lightning purchase invoices.
- **Quadlet migration validated** — all backends run as `user.slice` Quadlet
  services on the canary node (default flip pending below).
- **Public demo** — multi-visitor sandbox deployed.

## 🔄 In progress

- **Multinode pass** — run the same production gate across the whole test
  fleet, plus cross-node federation/mesh suites. This is the current exit
  criterion.
- **Quadlet default flip** — flip the validated Quadlet path from opt-in to
  default fleet-wide; eliminates the last container-flapping root cause.
- **Container-flapping elimination** — reconciler churn and failed-unit
  self-healing gaps observed on live nodes.
- **Reticulum tail** — ship the daemon binary inside the release tarball;
  final fleet redeploys.

## ⏳ Release-blocking for 1.8.0

Tracked in detail in [`1.8.0-RELEASE-HARDENING-PLAN.md`](1.8.0-RELEASE-HARDENING-PLAN.md):

- **OTA upgrade-from-previous-version soak** on real hardware — the top
  untested release risk.
- **ISO/image hardening** — per-device TLS/SSH keys on first boot, remove
  default credentials and SSH password auth, signed + checksummed ISO,
  registries over HTTPS, unattended-upgrades and firewall defaults.
- **Bitcoin multi-version fleet OTA** — code done; rollout timing is a
  deliberate hold.
- Version bump to `1.8.0-alpha` + tag once the pre-tag items close.

## 🔭 Planned (post-1.8.0)

- **Developer CLI** — `archy app validate / render / install / test`; the gate
  for opening third-party app publishing.
- **External marketplace maturation** — publishing tooling, trust UX, and
  reputation surfaces on top of the shipped backend
  ([`marketplace-protocol.md`](marketplace-protocol.md)).
- **DHT/P2P distribution** — releases and app images over iroh-based swarm
  ([`dht-distribution-design.md`](dht-distribution-design.md); feature-gated
  skeleton exists).
- **P2P encrypted voice/video** over Tor between federated nodes.
- **Dual ecash** — Fedimint + Cashu phases 2–6, networking-sats
  ([`dual-ecash-design.md`](dual-ecash-design.md)).
- **Paid streaming** — streaming ecash for content
  ([`phase4-streaming-ecash-plan.md`](phase4-streaming-ecash-plan.md)).
- **Hardware signer** support ([`hardware-signer-design.md`](hardware-signer-design.md)).
- **Code health** — split god-modules, remove dead crates, route all
  podman/systemctl calls through the wrapper.

## Release pipeline

Feature Testing (internal) → User Testing (controlled hardware) → Beta Live (public).
