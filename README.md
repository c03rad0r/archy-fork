# Archipelago

> Self-sovereign Bitcoin node OS and manifest-driven app platform.

Archipelago is a bootable personal server OS for Bitcoin infrastructure,
self-hosted apps, mesh communication, decentralized identity, and federation.
Apps are packaged as declarative `manifest.yml` files and run as rootless
Podman containers managed by the Rust backend.

[![Debian 13](https://img.shields.io/badge/Debian-13%20Trixie-a80030)](https://www.debian.org/)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange)](https://www.rust-lang.org/)
[![Vue.js](https://img.shields.io/badge/vue.js-3.5-brightgreen)](https://vuejs.org/)
[![Version](https://img.shields.io/badge/version-1.8.0--alpha-blue)]()

## What is here

- `core/` - Rust workspace: backend API, container runtime, security, OpenWrt
  helpers, and performance/resource management.
- `neode-ui/` - Vue 3 + TypeScript frontend.
- `apps/` - app manifests and custom app container sources.
- `docker/` - supporting container build contexts for UI companion surfaces.
- `image-recipe/` - bootable image/ISO build inputs.
- `Android/` - Android companion app.
- `scripts/` - development, release, deployment, and validation tooling.
- `docs/` - architecture, app packaging, operations, API, and roadmap docs.

## Platform model

Archipelago is built as a developer-ready app platform, not a fixed appliance:

- Apps are declared in `apps/<app-id>/manifest.yml`.
- The Rust parser in `core/container/src/manifest.rs` is the canonical schema.
- The orchestrator compiles manifests to rootless Podman/Quadlet runtime state.
- App data lives under `/var/lib/archipelago/<app-id>/`.
- Secrets are generated or read from `/var/lib/archipelago/secrets/` and
  injected through Podman secrets rather than static environment values.
- Release and app catalogs are signed and verified against a pinned trust
  anchor.

Start with:

- [Architecture](docs/architecture.md)
- [Developer Guide](docs/developer-guide.md)
- [App Developer Guide](docs/app-developer-guide.md)
- [App Manifest Spec](docs/app-manifest-spec.md)
- [Nostr Git Source Hosting Plan](docs/nostr-git-source-hosting.md)
- [Troubleshooting](docs/troubleshooting.md)

## Quick start

### Frontend

```bash
cd neode-ui
npm install
npm start
```

The dev UI runs at `http://localhost:8100` with a mock backend on `:5959`.

### Backend

```bash
cd core
cargo build
cargo test --all-features
```

Linux is the supported backend runtime and release-build target. macOS is fine
for frontend work and many Rust compile/test loops, but host integration tests
that touch Podman, systemd, networking, or image build paths require Linux.

### App manifests

```bash
./scripts/validate-app-manifest.sh apps/filebrowser/manifest.yml
python3 scripts/generate-app-catalog.py
python3 scripts/check-app-catalog-drift.py --release --strict
```

`scripts/generate-app-catalog.py` requires Python with PyYAML installed.

## Documentation map

The full, grouped index lives at **[docs/README.md](docs/README.md)**. The most
common entry points:

| Doc | Purpose |
|-----|---------|
| [Architecture](docs/architecture.md) | System layers, crates, data paths, security model |
| [Developer Guide](docs/developer-guide.md) | Local setup, code workflow, testing |
| [API Reference](docs/api-reference.md) | JSON-RPC API overview |
| [App Developer Guide](docs/app-developer-guide.md) | How to package and test apps |
| [App Manifest Spec](docs/app-manifest-spec.md) | Manifest schema and validation rules |
| [Nostr Git Source Hosting Plan](docs/nostr-git-source-hosting.md) | ngit/NIP-34 contribution workflow and maintainer model |
| [Apps README](apps/README.md) | Packaged app catalog overview |
| [Image Recipe](image-recipe/README.md) | Bootable image build flow |
| [Roadmap](docs/ROADMAP.md) | Shipped, in-progress, and planned work |
| [Archive](docs/archive/) | Historical plans, audits, and handoffs |

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. For
security issues, follow [SECURITY.md](SECURITY.md) and do not open a public
issue.

## License

Archipelago is licensed under the [MIT License](LICENSE). Third-party notices
are listed in [NOTICE](NOTICE) and generated license inventories in component
release artifacts.
