# Archipelago documentation

Start here. This index groups the docs by what you're trying to do. The
authoritative behaviour is always the code in `core/`; where a doc and the code
disagree, the code wins and the doc is a bug.

## Getting started

- [User Walkthrough](user-walkthrough.md) — setting up and using a node, from hardware to daily use
- [Talking to your node](COMMANDS.md) — the conversational command surface
- [Seed Verification](SEED-VERIFICATION.md) — independently verify your 24-word backup
- [Troubleshooting](troubleshooting.md) — common problems and how to resolve them
- [Gamepad / Controller Navigation](GAMEPAD-NAV.md) — driving the UI from a controller
- [Pine voice commands](pine-voice-commands.md) — the voice-satellite phrase surface

## Architecture

- [Architecture](architecture.md) — the system at a glance
- [Multi-Node Architecture](multi-node-architecture.md) — how nodes relate across a fleet
- [API Reference](api-reference.md) — the JSON-RPC surface

## Contributing to Archipelago itself

- [Developer Guide](developer-guide.md) — building the workspace, the frontend, and an ISO
- [Contributor guide (`CLAUDE.md`)](../CLAUDE.md) — invariants, build/verify, the production test gate
- [Bulletproof containers](bulletproof-containers.md) — why the reconciler is level-triggered
- [Release signing runbook](workstream-b-signing-runbook.md) — the ceremony and key handling
- [1.8.0 Release Hardening Plan](1.8.0-RELEASE-HARDENING-PLAN.md) — the release-blocking checklist
- [Third-party license audit](LICENSE-COMPLIANCE-AUDIT.md) — dependency licensing posture
- [Demo build info](demo-build-info.md) — operating the public demo sandbox

## App development

- [App Developer Guide](app-developer-guide.md) — build and package a containerized app
- [App Manifest Specification](app-manifest-spec.md) — the manifest schema, field by field
- [Manifest → Quadlet unit](quadlet-compilation.md) — how a manifest compiles to a systemd-owned container unit
- [Container lifecycle](container-lifecycle.md) — the reconciler state machine: install/adopt/start/stop/self-heal
- [App secrets](secrets.md) — declaring, generating and injecting per-install credentials
- [Registry-Distributed Manifests](registry-manifest-design.md) — how manifests reach nodes via the signed catalog
- [Decentralized Marketplace Protocol](marketplace-protocol.md) — publishing apps via an external registry
- [Bitcoin RPC Relay](bitcoin-rpc-relay.md) — letting an external wallet reach the node's Bitcoin RPC
- [Companion Pairing QR](companion-pairing-qr.md) — the pairing handoff contract
- [TV input inside iframe apps](tv-input-iframe-apps.md) — keyboard/gamepad routing into embedded apps


## Design docs

These record why a thing is built the way it is. They are design records, not
step-by-step guides, and some predate the current implementation.

- [Registry-Distributed Manifests](registry-manifest-design.md)
- [DHT Distribution](dht-distribution-design.md)
- [Bitcoin Multi-Version](bitcoin-multi-version-design.md)
- [Dual Ecash](dual-ecash-design.md)
- [Hardware Signer](hardware-signer-design.md)
- [Manifest Hooks](manifest-hooks-design.md)
- [Peering & Federation Trust](peering-trust-model.md) — naming/semantics of trust levels vs discovery (#134)
- [kdump + rasdaemon Troubleshooting](kdump-rasdaemon-design.md) — post-mortem and hardware-error capture on nodes (#144)
- [Meshroller Integration](meshroller-integration-design.md)
- [Nostr Git Source Hosting](nostr-git-source-hosting.md)
- [Nostr Identity Import](nostr-identity-import-plan.md) · [Nostr Signer Login (research)](nostr-signer-login-research.md)
- [Streaming Ecash (phase 4)](phase4-streaming-ecash-plan.md)
- [App Packaging Migration](APP-PACKAGING-MIGRATION-PLAN.md)

## Decisions (ADRs)

- [ADR-001: Podman over Docker](adr/001-podman-over-docker.md)
- [ADR-002: DID Key Method for Node Identity](adr/002-did-key-method.md)
- [ADR-003: Nostr Relays for Discovery](adr/003-nostr-for-discovery.md)
- [ADR-004: Tor Hidden Services for Peer Communication](adr/004-tor-for-peer-communication.md)
- [ADR-005: ChaCha20-Poly1305 for Backup Encryption](adr/005-chacha20-backup-encryption.md)
- [ADR-006: Nostr Relays for Marketplace Discovery](adr/006-nostr-marketplace-discovery.md)
- [ADR-007: DID-Based Federation Trust](adr/007-did-federation-trust.md)
- [ADR-008: Dual Key Strategy (Ed25519 + Secp256k1)](adr/008-dual-key-strategy.md)
- [ADR-009: Manifest-Level Container Security](adr/009-manifest-container-security.md)
- [ADR-011: DWN Deprioritization](adr/011-dwn-deprioritization.md)

There is no ADR-010 — the number was never issued, so the gap is not a missing
file.

## Security

- [Security Policy](../SECURITY.md) — how to report a vulnerability
- [PSBT Signing Architecture](security/PSBT-SIGNING-ARCHITECTURE.md)
- [Bitcoin RPC Proxy Exposure](security/BITCOIN-RPC-PROXY-EXPOSURE.md)
- [Entropy Enforcement (KEY-05)](security/KEY-05-ENTROPY-ENFORCEMENT.md)

## Roadmap & history

- [Roadmap](ROADMAP.md) — where the project is going
- [TODO](TODO.md) — working backlog of unscoped forward-looking items
- [archive/](archive/README.md) — superseded design and status documents, kept for provenance
