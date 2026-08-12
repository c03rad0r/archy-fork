# App Packaging Migration Plan

## Goal

Turn Archipelago into a serious app platform while preserving the fundamentals that drove the original architecture:

- Rootless Podman and security-first execution.
- Managed node-OS behavior: health, repair, backups, updates, secrets, and routing.
- Bitcoin/LND/Tor/Web5/mesh integration where the platform genuinely needs deep awareness.
- A developer-friendly app packaging model that avoids app-specific Rust installers as the normal path.

## Current Contract

The runtime contract is manifest-first. App packages live at `apps/<app-id>/manifest.yml` and are validated by the shared container manifest parser.

The current canonical manifest fields are:

- `app`: identity and app-level metadata.
- `container`: image or build source, pull policy, network, entrypoint, custom args, derived env, secret env, and data UID.
- `dependencies`: storage and app dependencies.
- `resources`: CPU, memory, disk.
- `security`: capabilities, read-only root, no-new-privileges, network policy, optional AppArmor profile.
- `ports`, `volumes`, `files`, `environment`, `health_check`, and `devices`.
- `metadata`: current catalog-facing presentation data such as category, tier, icon, repo/source, author, and features.
- extension keys may exist temporarily, but they are transitional and should not become a second contract.

The historical `archy-app.yml` name should be treated as superseded. The active local package filename is `manifest.yml`.

## Current Progress

As of the current `1.8-alpha` workstream:

- `apps/*/manifest.yml` is the source of truth for runtime app definitions.
- The Rust manifest parser validates app identity, image-vs-build source selection, safe environment/secrets, safe ports, safe bind/named/tmpfs volumes, generated files under declared bind mounts, devices, and security/network policy values.
- Manifest-owned generated files exist through `app.files` and have been used for app config material (e.g. strfry, netbird config regeneration).
- Local image builds are represented with `container.build`; pulled images are represented with `container.image`.
- Data ownership repair is represented with `container.data_uid`.
- Derived host facts and secret-file-backed environment variables are represented with `container.derived_env` and `container.secret_env`.
- Catalog metadata generation is implemented by `scripts/generate-app-catalog.py`.
- App-session launch ports/titles and new-tab launch behavior now have a generated TypeScript metadata path from manifests, with manual overrides preserved for companion UIs and aliases that do not have manifest-owned metadata yet.
- Runtime package listings now derive LAN launch URLs from manifest-owned `interfaces.main` declarations or HTTP app ports before falling back to legacy compatibility aliases.
- Release drift checking is implemented by `scripts/check-app-catalog-drift.py --release --strict`.
- The canonical catalog and the UI public catalog are expected to remain byte-for-byte synced after generation.
- Runtime validation has already moved many simple and moderate apps into the manifest/orchestrator path, including Filebrowser, Vaultwarden, Portainer, Uptime Kuma, Grafana, Gitea, Nextcloud, SearXNG, Nostr Relay, PhotoPrism, Jellyfin, and several Bitcoin-adjacent apps.

The remaining migration work is mostly orchestration quality: post-reboot adoption, progress reporting, stale scanner-state handling, update policy, multi-container stack ownership, proxy route generation, and cleanup of obsolete legacy installers/fallbacks.

## Target Architecture

Use a StartOS-inspired package model with Umbrel-like app folders.

```text
apps/example-commerce/
  manifest.yml
  Dockerfile
  icon.svg
  screenshots/
  instructions.md
  hooks/
    post-install.sh
    pre-start.sh
    repair.sh
    health.sh
    backup.sh
    restore.sh
  proxy/
    routes.yml
```

Archipelago becomes the secure compiler/runtime for these packages. The manifest declares what it needs; Archipelago validates it, injects secrets, creates rootless Podman containers, generates nginx/Tor/public routes, registers health checks, displays credentials, and manages lifecycle.

## Core Principles

- App packages are declarative by default.
- Hooks are allowed only as controlled, reviewed escape hatches.
- Rootless Podman stays.
- Arbitrary privileged Compose execution is not allowed.
- Each app has one source of truth.
- Catalog, launch URLs, mobile behavior, credentials, backup paths, and public routes come from the app package or its generated catalog entry.
- Rust backend owns orchestration, not app-specific business logic.
- Core infrastructure can remain special-case where justified.

## What Stays

- Rootless Podman.
- Archipelago orchestrator.
- Health/reconcile/repair loops.
- Host nginx.
- Nginx Proxy Manager integration.
- Tor/public routing goals.
- Bitcoin/LND/mesh/Web5/FIPS/security direction.
- OTA update system.
- App-session/mobile shell.
- Managed secrets and credentials display.

## What Changes

- Complex app stacks stop living in Rust.
- `app-catalog/catalog.json` becomes generated.
- Frontend fallback marketplace data is removed or generated.
- App-session port maps and new-tab launch behavior become generated.
- Public proxy routes become app-declared.
- Install/start/restart/backup/restore become package-driven.
- App updates become app package changes where possible, not full backend code changes.

## Package Schema Direction

Example `manifest.yml`:

```yaml
app:
  id: example-commerce
  name: Example Commerce
  version: 3.23.0
  description: Composable commerce platform
  container:
    image: docker.io/myorg/example-commerce:1.0.0
    pull_policy: if-not-present
    network: archy-net
    entrypoint: ["sh", "-lc"]
    custom_args:
      - /app/start.sh
    derived_env:
      - key: PUBLIC_URL
        template: https://{{HOST_MDNS}}:9010
    secret_env:
      - key: SALEOR_SECRET_KEY
        secret_file: example-commerce-secret-key
  dependencies:
    - storage: 20Gi
  resources:
    cpu_limit: 4
    memory_limit: 2Gi
  security:
    capabilities: []
    readonly_root: true
    no_new_privileges: true
    network_policy: isolated
  ports:
    - host: 9010
      container: 9000
      protocol: tcp
  volumes:
    - type: bind
      source: /var/lib/archipelago/example-commerce
      target: /data
      options: [rw]
  environment:
    - NODE_ENV=production
  health_check:
    type: http
    endpoint: http://localhost:9000
    path: /health
    interval: 30s
    timeout: 5s
    retries: 3
```

Optional generated files, hooks, icons, and screenshots can sit beside the manifest, but the manifest stays the source of truth. Compose-style definitions are not executed directly.

## Security Model

Do not run arbitrary Compose directly. Archipelago validates:

- No privileged containers unless explicitly approved.
- No host filesystem mounts outside approved paths.
- No Docker socket mounts.
- No host network unless explicitly approved.
- No dangerous capabilities by default.
- No arbitrary device access without declaration.
- No rootful execution.
- Pinned images preferred.
- Resource limits required.
- Backup paths declared where the app stores durable data.
- Public routes explicit.
- Secrets referenced by name, not hardcoded.

When the runtime needs app-specific facts that do not belong in the manifest, prefer adding a reusable platform primitive rather than introducing another ad hoc installer path.

This preserves the reason for avoiding raw Umbrel-style Compose while still giving developers a sane package format.

## Lifecycle Model

Every app package should support:

- install
- configure
- start
- stop
- restart
- update
- repair
- health
- backup
- restore
- uninstall
- migrate

Archipelago owns the state machine.

Optional hooks:

- `post-install.sh` for migrations/admin creation.
- `pre-start.sh` for ownership repair.
- `repair.sh` for app-specific remediation.
- `health.sh` for custom health checks.
- `backup.sh` and `restore.sh` only when simple path backups are insufficient.

Hooks run with a controlled environment and restricted permissions.

## Hard Work

The hard work is not writing YAML. The hard work is safely translating app packages into reliable rootless runtime behavior:

- Build a robust package validator.
- Map a safe Compose subset to rootless Podman.
- Handle multi-container networks without hardcoded IPs.
- Handle rootless volume ownership correctly.
- Generate host nginx routes from app metadata.
- Handle public-domain apps without leaking private `192.168.x.x` or `100.x.x.x` URLs.
- Inject secrets without exposing values in logs or frontend bundles.
- Make backup/restore consistent across databases and files.
- Migrate existing hand-built containers to package-owned containers.
- Keep old alpha nodes working while introducing the new system.
- Avoid keeping two permanent systems that drift forever.

## Alpha Node Impact

Existing alpha nodes must not be broken.

Phase 1 behavior:

- Current Rust installers keep working.
- Current app manifests keep working.
- New app package loader exists beside the old system.
- No existing app is automatically migrated.
- Alpha nodes receive compatibility code only.

Phase 2 behavior:

- New installs of selected apps use package mode.
- Existing installs can be detected and adopted.
- App state is preserved.
- Migration is opt-in or happens only for low-risk apps.

Phase 3 behavior:

- Stable migrated apps switch to package mode by default.
- Existing containers are adopted if names/volumes match.
- Data directories are preserved.
- Old Rust installers remain as fallback for at least one release cycle.

Phase 4 behavior:

- Remove old installers only after live alpha validation.
- Keep migration repair code for already-deployed nodes.

## Migration Rules

For every migrated app:

- Preserve `/var/lib/archipelago/<app>` data.
- Preserve generated secrets.
- Preserve credentials shown to users.
- Preserve public ports where possible.
- Preserve container names where needed for adoption.
- Never delete volumes during migration.
- Stop/recreate containers only when necessary.
- Record migration version in app state.
- Provide rollback path to old installer for alpha builds.

## Notes For The Release

- Catalog entries should be generated from manifests so the UI and runtime agree on launch metadata.
- The developer docs should describe the manifest/runtime contract that exists today, not the older publish-model draft.
- If a new capability is needed, add one reusable manifest field or orchestrator primitive and document it here before wiring a one-off app branch.

## First Apps To Migrate

Start with low-risk apps:

- Filebrowser
- Vaultwarden
- Uptime Kuma
- Grafana

Then moderate apps:

- Gitea
- Nextcloud
- SearXNG
- Nginx Proxy Manager metadata integration

Then complex apps:

- Mempool
- BTCPay Server
- NetBird only if safe

Leave for later:

- Bitcoin
- LND
- Electrs/ElectrumX
- Tor
- System update
- Mesh/Web5/FIPS core services

## Complex Stack Reference Goal

Saleor has been removed from the supported release catalog until it has a real
manifest-owned package. A future complex stack should become the showcase
package and prove:

- Multi-container stack support.
- Generated secrets.
- Post-install migration/admin user hooks.
- Dashboard/API/storefront routes.
- Same-origin public GraphQL routing.
- Credentials display.
- Backup paths.
- Health checks.
- Public domain support.
- Alpha-node adoption.

Once a complex stack is clean, the app system is credible.

## Implementation Phases

**Status (2026-07-08):** Phases 1–3 ✅ DONE (per-member manifests won over a
compose subset; all five real multi-container stacks — btcpay, mempool,
immich, netbird, indeedhub — install via `install_stack_via_orchestrator`).
Phase 5 mostly done (orchestrator-first with legacy fallback + per-app
adoption/repair). Phase 4 (routing via `proxy/routes.yml`) NOT started —
routing is still host-nginx driven. Phase 6 (cleanup + developer CLI) NOT
started; the CLI gates external app publishing.

### Phase 1: Package Contract

- Use `apps/<app-id>/manifest.yml` as the package contract.
- Keep the Rust parser/validator as the canonical schema implementation.
- Keep generated catalog output from manifest-owned metadata.
- Finish generated app-session launch metadata so launch behavior cannot drift from manifests.
- Add/keep tests for unsafe package rejection.

### Phase 2: Single-Container Runtime

- Continue hardening package install for one-container apps.
- Compile manifests to rootless Podman/Quadlet runtime behavior.
- Support ports, env, generated files, devices, volumes, resources, health checks, data UID repair, image pull/build availability checks, and launch metadata.
- Keep Filebrowser, Vaultwarden, Portainer, Uptime Kuma, Grafana, SearXNG, Jellyfin, PhotoPrism, and similar apps as regression proofs.

### Phase 3: Multi-Container Runtime

- Decide whether multi-container stacks use a safe `compose.yml` subset or a manifest-native `services` section.
- Support app-local networks.
- Support service dependencies and readiness gates.
- Support internal service names.
- Support generated env/secrets across services.
- Support controlled hooks only where declarative primitives are insufficient.
- Adopt existing multi-container apps without deleting data.

### Phase 4: Routing

- Add `proxy/routes.yml`.
- Generate host nginx routes.
- Generate Tor/public routes.
- Fix same-origin API routing class of bugs permanently.
- Integrate with Nginx Proxy Manager sync.

### Phase 5: Migration

- Add adoption logic for existing containers.
- Add migration metadata.
- Migrate simple apps.
- Migrate a serious multi-container app once the stack model is stable.
- Keep rollback.
- Prove reboot recovery with repeated clean post-reboot lifecycle passes.
- Preserve Nostr signer bridges, Bitcoin dependency wait states, and public launch ports during adoption.

### Phase 6: Cleanup

- Remove duplicated catalog/frontend data.
- Remove migrated Rust stack installers.
- Document package format.
- Add developer tooling: validate, test, package, install locally.
- Remove stale fallback metadata, app-specific lifecycle branches, and compatibility shims only after live validation.

## Developer Tooling

Add commands like:

```bash
archy app validate apps/example-commerce
archy app render apps/example-commerce
archy app install apps/example-commerce
archy app test apps/example-commerce
```

Developers should be able to package an app without understanding Archipelago internals.

## Open Source Story

Public explanation:

> Archipelago uses rootless Podman and a validated app package format. App authors define services declaratively, while the OS enforces security, secrets, routing, backups, health, and lifecycle repair. This gives us Umbrel-like app packaging with StartOS-like managed service discipline.

## Rework Estimate

- Package schema and validator: 1-2 weeks.
- Single-container package runtime: 1-2 weeks.
- Generated catalog/frontend metadata: 1 week.
- Multi-container support: 2-4 weeks.
- Routing/public proxy integration: 1-2 weeks.
- Hooks/secrets/backups: 2-3 weeks.
- First migrations: 2-4 weeks.
- Complex stack reference migration: 1-2 weeks.
- Cleanup/docs/tooling: 2-3 weeks.

Total estimate: 8-14 weeks of serious work for an excellent system.

Minimum viable version: 3-5 weeks.

## Biggest Risks

- Rootless Podman edge cases continue to bite.
- Compose compatibility scope creeps too wide.
- Hooks become an unsafe escape hatch.
- Migration accidentally disrupts alpha nodes.
- Generated metadata drifts from old manual data during transition.
- Old and new systems remain permanently duplicated.

## Risk Controls

- Support a strict Compose subset, not all Compose.
- Validate everything.
- Keep hooks minimal and logged.
- Migrate one app at a time.
- Add live alpha-node checks before each release.
- Generate catalog/app-session data early.
- Set a deadline for deleting migrated legacy installers.

## Immediate Next Steps

1. Expand generated app-session metadata beyond ports/titles/new-tab behavior to cover proxy paths and companion UI aliases where those can be declared safely in manifests.
2. Define the app update policy and wire it into manifest/catalog metadata.
3. Finish post-reboot adoption and stale scanner-state handling for migrated apps.
4. Convert remaining multi-container legacy stacks to a manifest-owned model without deleting data.
5. Add developer tooling around the current `manifest.yml` contract: validate, render, local install, lifecycle test.
6. Migrate a serious multi-container app as the proof package once the stack model is stable.
7. Leave Bitcoin/LND/core services as managed infrastructure until the package system is proven for normal apps.
