# Archipelago — contributor guide

This file orients anyone (human or AI) working in this repository: the
invariants that must hold, how to build and verify, and where the deeper
design docs live. The authoritative behaviour is always the code in `core/`.

**Read [`docs/ROADMAP.md`](docs/ROADMAP.md) for where the project is going** and
[`docs/README.md`](docs/README.md) for the full documentation index.

The north star: a world-class, **developer-ready app platform** — every app
manifest-driven, rootless, secure, and 100%-uptime-capable, with third-party
developers publishing via an external/decentralized registry.

Detailed sub-plans:
- App platform / packaging phases + security model → [`docs/APP-PACKAGING-MIGRATION-PLAN.md`](docs/APP-PACKAGING-MIGRATION-PLAN.md)
- Registry-distributed manifests → [`docs/registry-manifest-design.md`](docs/registry-manifest-design.md)
- External/decentralized marketplace for devs → [`docs/marketplace-protocol.md`](docs/marketplace-protocol.md)
- App manifest schema → [`docs/app-manifest-spec.md`](docs/app-manifest-spec.md)
- Production test gate → [`tests/lifecycle/TESTING.md`](tests/lifecycle/TESTING.md)

## Commit & push every unit of work

Work is not "done" until it is committed **and** pushed. Finished work has been
lost by sitting uncommitted in a shared tree across sessions. To prevent that:

- **Commit each feature/fix the moment it works** — one focused, self-contained
  commit per logical change (it compiles and its targeted tests pass). Don't let
  unrelated changes accumulate uncommitted.
- **Push immediately after committing** so nothing lives only on one machine.
- **Never leave a stack of finished work uncommitted** overnight or when handing
  off — if you must pause mid-change, commit a clearly-labelled WIP checkpoint
  rather than leaving the tree dirty.
- **Stage explicitly by path** (`git add <paths>`) when another contributor's
  uncommitted work shares the tree — never `git add -A` / `git commit -a`, which
  clobbers or entangles their changes.
- **Never commit secrets** (mnemonics, private keys, API tokens). Signing is done
  offline; artifacts (catalog/manifest) are signed, not the keys.

## Invariants (never violate)

- **Rootless Podman only.** No rootful, no Docker-socket mounts, no privileged
  containers unless explicitly approved.
- **No per-app Rust installers / no OS-level reliance.** Apps are declarative;
  the orchestrator owns the lifecycle. A hardcoded `podman run` + `sudo chown`
  installer is the anti-pattern being deleted, not a template.
- **Secrets are manifest-declared** (`generated_secrets`, materialised by
  `container::secrets`, 0600/rootless) — never hardcoded, per-app, or logged.
- **Migrations never destroy data** — preserve `/var/lib/archipelago/<app>`,
  secrets, credentials, ports, and adoption container names; keep a rollback path.
- **Verify on a real node before any release tag.**

## Build / verify

- Rust workspace root is `core/` (no Cargo.toml at repo root). Run `cargo` from `core/`.
- If a `cargo test`/build hits `rust-lld: undefined hidden symbol`, it's
  incremental-cache corruption — rebuild with `CARGO_INCREMENTAL=0`.
- Frontend: `neode-ui/` → `npm run build` outputs to `web/dist/neode-ui/`.
  Grep the built bundle for new strings before shipping (the build can silently
  no-op).
- App manifests are delivered inside the **signed catalog** (`releases/app-catalog.json`),
  whose entry overrides the on-disk `/opt/archipelago/apps/*/manifest.yml`
  (origin-wins; disk is the fallback). Editing a disk manifest alone does **not**
  change a catalog-covered app — regenerate and re-sign the catalog.

## Production test gate (definition of done)

`tests/lifecycle/run-gate.sh` must be green across install / UI / stop / start /
restart / reinstall / reboot-survive / archipelago-restart-survive / uninstall.
**Run the gate on the node** (it uses local podman/systemctl/bitcoin probes), not
via RPC from another host, and re-run it after any orchestrator/lifecycle change.
Multinode / fleet testing is a separate pass. See
[`tests/lifecycle/TESTING.md`](tests/lifecycle/TESTING.md).
