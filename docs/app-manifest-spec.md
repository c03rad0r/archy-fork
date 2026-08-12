# App Manifest Specification

_Accurate as of 2026-07-08. The canonical schema is the Rust parser in
`core/container/src/manifest.rs` (`AppManifest` → `AppDefinition`); if this
document and the code disagree, the code wins. See
[`app-developer-guide.md`](app-developer-guide.md) for the authoring workflow._

Every app is a directory `apps/<id>/` containing a `manifest.yml` with a single
top-level `app:` block. Apps are declarative — the orchestrator owns the entire
lifecycle; there is no per-app installer code.

One honest caveat: seven first-party apps still get Rust-side pre-start work
through a hardcoded `match app_id` in `ProdOrchestrator::run_pre_start_hooks` —
`bitcoin-ui`, `filebrowser`, `lnd`, `archy-nbxplorer`, `btcpay-server`,
`fedimint-clientd` and `grafana` render or repair a config before start. That is
orchestrator code rather than a per-app installer, but it is not
manifest-declared, and the direction of travel is to replace each case with a
reusable manifest primitive.

## Top-level fields (`app:`)

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `id` | string | ✅ | Lowercase ASCII letters, digits and single hyphens only — **no underscores**, no leading/trailing `-`, no `--` (`is_valid_app_id`). Should match the directory name, though nothing enforces that: the loader keys off `app.id`, so a mismatch silently registers the app under the id in the file rather than the folder. |
| `name` | string | ✅ | Display name. |
| `version` | string | ✅ | App version shown in the UI. |
| `description` | string | — | One-line description. |
| `container` | ContainerConfig | — | Image/build source + runtime shape (below). |
| `dependencies` | list | — | `- storage: "10GB"`, `- { app_id: bitcoin, version: … }`, or a bare string. |
| `resources` | ResourceLimits | — | `cpu_limit` (int), `memory_limit` (e.g. `"512m"`), `disk_limit`. |
| `security` | SecurityPolicy | — | See [Security](#security). |
| `ports` | list of PortMapping | — | `- { host: 8080, container: 80, protocol: tcp }`. |
| `volumes` | list of Volume | — | See [Volumes](#volumes). |
| `files` | list of GeneratedFile | — | Config files written before create: `{ path, content, overwrite }`. `path` must sit under a declared bind mount. |
| `environment` | list of string | — | `- KEY=value` pairs (static). |
| `health_check` | HealthCheck | — | `{ type, endpoint/path, interval, timeout, retries }`. `type` is free-form today; `http` is what the monitor exercises. |
| `devices` | list of string | — | Host device paths; must start with `/dev/`. |
| `interfaces` | map | — | Launch surfaces, keyed by name (`main`): `{ name, description, type, port, protocol, path }`. |
| `hooks` | LifecycleHooks | — | Allow-listed lifecycle hooks. See [Hooks](#hooks). |
| _anything else_ | — | — | Unknown keys are absorbed into an `extensions` map (serde flatten) and treated as transitional metadata — e.g. `container_name`, `metadata`, `category`, `bitcoin_integration`, `lightning_integration`. These are **not** typed schema; do not rely on them being validated. |

## `container:` (ContainerConfig)

Exactly **one** of `image` or `build` must be present (image XOR build).

| Field | Type | Notes |
|-------|------|-------|
| `image` | string | Registry reference. Pull source. |
| `image_signature` | string | Optional signature reference for image verification. |
| `pull_policy` | string | Default `if-not-present`. |
| `build` | BuildConfig | Local build: `{ context, dockerfile (default "Dockerfile"), tag, build_args }`. |
| `network` | string | Literal podman `--network` value (`archy-net`, `host`, a stack network, …). Omitted = rootless default isolated network. |
| `network_aliases` | list of string | Extra DNS names on `network` (podman `--network-alias`) — lets stack members answer to short baked-in hostnames (`api`, `minio`, `relay`). |
| `entrypoint` | list of string | Entrypoint override. |
| `custom_args` | list of string | Extra positional args appended after the image. |
| `derived_env` | list | `- { key, template }` — template rendered against host facts at apply time. The allow-list is exactly `{{HOST_IP}}`, `{{HOST_MDNS}}`, `{{DISK_GB}}`, `{{BITCOIN_HOST}}` (`DERIVED_PLACEHOLDERS`); an unknown name or unbalanced `{{` fails validation. `{{BITCOIN_HOST}}` resolves to whichever Bitcoin app is running (`bitcoin-knots` or `bitcoin-core`, defaulting to knots). Never hard-code host specifics. |
| `secret_env` | list | `- { key, secret_file }` — value read from `/var/lib/archipelago/secrets/<secret_file>` and injected as a **podman secret**, so it never appears in `podman inspect` or unit files. `secret_file` must be a bare filename (no `/`, no `..`). |
| `generated_secrets` | list | `- { name, kind }` — orchestrator materialises the secret on first use (0600, rootless service user, idempotent + self-healing). `kind ∈ hex16 | hex32 | base64 | bcrypt` (bcrypt writes `<name>` = hash and `<name>.pw` = plaintext). |
| `generated_certs` | list | `- { crt, key, common_name?, sans? }` — self-signed TLS materialised before create; CN/SANs rendered against host facts. |
| `data_uid` | string | `"UID:GID"` applied to the app's bind-mounted data dir before create (rootless subuid mapping, e.g. Postgres). |

## Security

```yaml
security:
  readonly_root: true        # default true
  no_new_privileges: true    # default true
  capabilities: [CHOWN]      # default [] (cap-drop ALL, add back only these)
  network_policy: isolated   # isolated | bridge | host (default isolated)
  apparmor_profile: null     # optional profile name
```

Validation (enforced at `AppManifest::validate()`):

- Capabilities must come from the reviewed allow-list (CHOWN, DAC_OVERRIDE,
  FOWNER, NET_ADMIN, NET_BIND_SERVICE, NET_RAW, SETGID, SETUID, SYS_ADMIN).
- `network_policy` must be exactly `isolated`, `bridge`, or `host`.
- No `container:`/`ns:` network modes; devices must be `/dev/*`.
- Bind-mount sources are confined to `/var/lib/archipelago` (reviewed
  exceptions: the rootless podman socket and dbus).
- `derived_env` templates may only use the placeholder allow-list;
  `secret_env`/`generated_secrets` names must be bare filenames.
- Hook steps are validated against the hook allow-list (below).

## Volumes

```yaml
volumes:
  - type: bind               # bind | volume | tmpfs
    source: /var/lib/archipelago/myapp/data
    target: /data
    options: [rw]            # allow-list: rw, ro, z, Z, shared, …
  - type: tmpfs
    target: /tmp
    tmpfs_options: "rw,noexec,nosuid,size=256m"
```

## Hooks

Declarative, allow-listed operations that run against the app's **own
container** — never the host (design: `manifest-hooks-design.md`).

```yaml
hooks:
  post_install:              # runs once after install, container running
    - copy_from_host:        # src relative to an allow-listed root (data dir / web-ui);
        src: web-ui/nostr-provider.js   # no absolute paths, no '..'
        dest: /usr/share/nginx/html/nostr-provider.js
    - exec: ["sh", "-c", "nginx -s reload"]   # podman exec inside the container
  pre_start: []              # reserved in the schema; executor not yet wired
```

## Installation semantics

The orchestrator compiles the manifest into a rootless Podman container that
survives backend restarts and reboots, and a level-triggered reconciler
converges drift every 30 seconds (`BootReconciler::DEFAULT_INTERVAL`).
Multi-container apps are sets of per-member manifests installed together via
the stack orchestrator (`api/rpc/package/stacks.rs`) on an app-local network.

**Quadlet is not the default path.** `config.use_quadlet_backends` defaults to
`false`, so ordinary apps still take the legacy `podman create + start` path;
the Quadlet-unit-under-`user.slice` backend is opt-in per node (config key or
`ARCHIPELAGO_USE_QUADLET_BACKENDS`) and stays behind the flag until the
lifecycle harness has gone green against it. Companion UI containers are the
exception that do use Quadlet today.

## Distribution

Manifests ship two ways:

1. **Signed catalog** (primary): `releases/app-catalog.json` embeds the full
   manifest per app and carries an Ed25519 detached signature verified against
   the pinned release-root anchor. Nodes overlay catalog manifests over disk
   files — **catalog wins** for image-only apps; `apps/<id>/manifest.yml` on
   disk remains the fallback and is still required for build-source apps.
2. **Decentralized marketplace**: Nostr NIP-78 discovery with DID-signed
   manifests ([`marketplace-protocol.md`](marketplace-protocol.md)). Note the
   marketplace uses its own flatter manifest schema, not this one.

## Minimal example

```yaml
app:
  id: myapp
  name: My App
  version: 1.0.0
  description: Does something useful
  container:
    image: docker.io/vendor/myapp:1.0.0
    generated_secrets:
      - { name: myapp-admin-password, kind: hex16 }
    secret_env:
      - { key: ADMIN_PASSWORD, secret_file: myapp-admin-password }
  ports:
    - { host: 8090, container: 8080 }
  volumes:
    - { type: bind, source: /var/lib/archipelago/myapp, target: /data, options: [rw] }
  health_check:
    type: http
    path: /health
  interfaces:
    main:
      type: ui
      port: 8090
```

Validate with `scripts/validate-app-manifest.sh` and regenerate the catalog
with `scripts/generate-app-catalog.py` (drift-checked in CI by
`scripts/check-app-catalog-drift.py`).
