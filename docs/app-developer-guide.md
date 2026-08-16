# Archipelago App Developer Guide

Build and package containerized apps for Archipelago.

## Overview

Apps run as rootless Podman containers on user nodes. You describe an app in `apps/<app-id>/manifest.yml`; the backend validates that manifest, compiles it into rootless container/runtime behavior, and the release pipeline generates catalog surfaces from the same manifest-owned metadata.

Archipelago's app contract is deliberately manifest-first. A developer should be able to describe images or local builds, ports, volumes, generated files, dependencies, health/readiness, data ownership, networking, secrets, and supported bridge integrations in the app manifest without asking for a custom OS image or app-specific backend patch. When a real app needs a capability that is not represented yet, the preferred path is to add a reusable manifest/orchestrator primitive that other apps can use too.

The historical marketplace-publish design is not the active local developer contract for `1.8-alpha`. For this release, local manifests are the source of truth and catalog JSON is generated from them.

## App Manifest

Every app needs a manifest at `apps/<app-id>/manifest.yml`. The root key is `app`; runtime, catalog, and integration fields live below that key.

### Template Manifest

```yaml
# apps/my-app/manifest.yml
app:
  id: my-app                    # Unique, lowercase kebab-case
  name: My App
  version: 1.0.0                # Semantic versioning
  description: My App does one thing well.

  container:
    image: docker.io/myorg/my-app:1.0.0
    pull_policy: if-not-present
    network: archy-net
    entrypoint: ["sh", "-lc"]
    custom_args:
      - /app/start.sh
    derived_env:
      - key: PUBLIC_URL
        template: https://{{HOST_MDNS}}:8180
    secret_env:
      - key: APP_PASSWORD
        secret_file: my-app-password

  dependencies:
    - storage: 1Gi

  resources:
    cpu_limit: 2
    memory_limit: 512Mi

  security:
    capabilities: []
    readonly_root: true
    no_new_privileges: true
    network_policy: isolated

  ports:
    - host: 8180
      container: 8080
      protocol: tcp

  volumes:
    - type: bind
      source: /var/lib/archipelago/my-app
      target: /data
      options: [rw]

  environment:
    - APP_MODE=production

  health_check:
    type: http
    endpoint: http://localhost:8080
    path: /health
    interval: 30s
    timeout: 5s
    retries: 3

  files:
    - path: /var/lib/archipelago/my-app/config.yml
      content: |
        bind: 0.0.0.0:8080
      overwrite: false

  metadata:
    icon: /assets/img/app-icons/my-app.svg
    category: tools
    tier: optional
    repo: https://github.com/myorg/my-app
    launch:
      open_in_new_tab: false
```

### Required Fields

| Field | Description |
|-------|-------------|
| `app.id` | Unique identifier, lowercase, kebab-case only |
| `app.name` | Human-readable name |
| `app.version` | Version string containing at least one digit; semantic versions are preferred |
| `container.image` or `container.build` | Exactly one image source must be present |
| `security.readonly_root` | Should remain `true` for normal apps |
| `security.no_new_privileges` | Should remain `true` for normal apps |

### Current Manifest Fields

| Field | Purpose |
|-------|---------|
| `app.id`, `app.name`, `app.version`, `app.description` | App identity and release metadata |
| `app.container.image` | Registry image to pull |
| `app.container.build` | Local build definition with `context`, `dockerfile`, `tag`, and optional `build_args` |
| `app.container.pull_policy` | Pull behavior, usually `if-not-present` |
| `app.container.network` | Podman network setting such as `archy-net` or `pasta`; dangerous namespace-sharing modes are rejected |
| `app.container.entrypoint` / `custom_args` | Entrypoint and command override |
| `app.container.derived_env` | Environment values rendered from host facts. The complete placeholder set is `{{HOST_IP}}`, `{{HOST_MDNS}}`, `{{DISK_GB}}`, `{{BITCOIN_HOST}}`; an unknown name or an unbalanced `{{` is a parse error, so typos fail loudly |
| `app.container.secret_env` | Environment values read from `/var/lib/archipelago/secrets/<secret_file>`, injected as podman secrets (never visible in `podman inspect` or unit files) |
| `app.container.generated_secrets` | Secrets the orchestrator creates on first use (`hex16`/`hex32`/`base64`/`bcrypt`) — self-healing, 0600, no host provisioning |
| `app.container.generated_certs` | Self-signed TLS certs materialised before create; CN/SANs rendered from host facts |
| `app.container.network_aliases` | Extra DNS names on the app network so stack members answer to short baked-in hostnames (`api`, `minio`, `relay`) |
| `app.container.data_uid` | UID:GID ownership repair for app data directories |
| `app.hooks` | Allow-listed lifecycle hooks (`post_install`: `exec` inside the app's own container, `copy_from_host` from allow-listed roots) — see `manifest-hooks-design.md` |
| `app.dependencies` | Storage requirements and app dependencies |
| `app.resources` | CPU, memory, and disk limits |
| `app.security` | Capabilities, read-only root, no-new-privileges, network policy, optional AppArmor profile |
| `app.ports` | Host-to-container port mappings |
| `app.volumes` | `bind`, `volume`, or `tmpfs` mounts |
| `app.files` | Generated files under declared bind-mounted host paths |
| `app.environment` | Static `KEY=value` environment entries |
| `app.health_check` | HTTP or TCP health check settings |
| `app.devices` | Explicit device paths |
| `app.metadata` | Catalog-facing presentation metadata such as icon, category, tier, repo/source, author, feature bullets, and launch hints |
| `app.interfaces.main` | Optional primary UI launch surface with `port`, `protocol`, and `path` |

Additional extension keys may exist for current integrations, for example Bitcoin, Lightning, or app-specific launch/interface metadata. Treat extension keys as transitional unless they are documented as reusable platform primitives.

### Iframe embedding — the rules

The dashboard opens apps in an **embedded frame** (My Apps → app session) by
default. Whether that works is decided by HTTP headers, not by wishes, so
know the mechanics:

- Browsers refuse to render a page in an iframe when the response carries
  `X-Frame-Options: DENY`/`SAMEORIGIN` (the dashboard and the app are
  different origins — different port at minimum) or a CSP `frame-ancestors`
  directive that excludes the dashboard's origin.
- Many upstream apps ship exactly those headers (Alby Hub sends
  `X-Frame-Options: DENY`). In a normal deployment that is correct hardening;
  behind Archipelago's app gate the clickjacking threat those headers address
  is already handled — every proxied request is authenticated by the gate
  first.
- Therefore **the gate neutralizes frame blocking on proxied responses**: it
  removes `X-Frame-Options` and strips only the `frame-ancestors` directive
  from the app's CSP. The rest of the app's CSP (script-src, connect-src, …)
  passes through untouched — the gate never weakens the app's own content
  policy, only its framing policy. You do not need a bespoke reverse proxy,
  header patches, or app config to be embeddable.

### Apps with their own login: `auth: open`

If your app carries a complete account system of its own — and especially if
non-browser clients must reach it (git over HTTP, mobile apps, payment
webhooks) — declare its gated port `auth: open` with an `auth_rationale`
instead of `auth: gated`. The daemon still fronts the port exactly like a
gated one (loopback pin, external binds, frame-header fixes, retry page,
Tor onion), but serves it without the dashboard-login challenge, so your
app's own authentication is the one users and API clients meet. Gitea and
BTCPay Server ship this way. The node operator can override your default in
either direction at runtime (Settings → app → Access control), so never
treat the gate as your app's authorization layer — enforce your own auth on
every sensitive route regardless. See “Ports & the app gate” in
[`app-manifest-spec.md`](app-manifest-spec.md).

Set `metadata.launch.open_in_new_tab: true` only when embedding is broken by
things headers can't fix — the app frame-busts in JavaScript, requires being
the top-level origin (OAuth redirect flows, WebAuthn), or sets
`SameSite=Strict` session cookies that never accompany framed requests. Test
in the real embedded app session, **not** a plain browser tab: tabs don't
enforce framing headers, so a tab proves nothing about the iframe.

**The dashboard also self-heals**: if a running app's frame fails to load,
the session offers "open in tab" and *remembers the app as a tab app* — every
later launch opens a tab directly, with the tab-launch icon on its button.
The memory clears itself when the app later embeds successfully, and expires
weekly so fixes get re-probed. This safety net is not a licence to skip the
manifest flag: declaring `open_in_new_tab: true` up front spares your users
the one dead-pane encounter the detector needs.

(Historical note: before the gate handled this, embeddable-but-blocking apps
each carried a hand-built nginx strip proxy — gitea's port-3000 proxy is the
surviving example. Do not copy that pattern for new apps.)

### Launch Interfaces

If an app exposes a user-facing web UI, declare its primary launch surface in
`interfaces.main`. Runtime package listings prefer this interface over inferred
port mappings, which matters for apps that expose non-UI service ports or use a
companion wait/proxy UI.

```yaml
interfaces:
  main:
    name: Web UI
    description: Primary app interface
    type: ui
    port: 8180
    protocol: http
    path: /
```

For simple HTTP apps without `interfaces.main`, Archipelago can still infer the
launch URL from the first declared TCP host port when the app has an HTTP health
check. TCP-only service ports, such as Bitcoin RPC/P2P, are not treated as UI
launch URLs.

Interface keys must use lowercase ASCII letters, digits, hyphens, or
underscores. Supported interface types are `ui`, `api`, and `metrics`; only
`type: ui` is treated as a launchable app surface. Supported protocols are
`http` and `https`, and `path` must start with `/`.

### Nostr Signer Bridge (NIP-07)

Apps embedded in the Archipelago iframe can use the node's Nostr identity to sign
events without managing their own keys. Archipelago injects a **NIP-07 provider**
(`window.nostr` with `getPublicKey()` / `signEvent()` / `nip04` / `nip44`) that bridges
to the host. Your app code uses standard NIP-07 — no Archipelago-specific API.

**How injection works.** After install, the host copies `nostr-provider.js` into the
app container and patches the app's web server so every page loads it and the app is
iframe-embeddable. This is **best-effort** and depends on your server config exposing
the right hooks. For an **nginx-served SPA** (the supported reference shape, e.g.
IndeeHub) your `nginx.conf` must satisfy this contract:

1. **Be iframe-embeddable.** Do not send a hard `X-Frame-Options: DENY`. The host
   strips a `SAMEORIGIN`/`DENY` `X-Frame-Options` header line if present; restrictive
   CSP `frame-ancestors` will still block embedding.
2. **Keep an exact-match `location = /sw.js {` block.** The provider's no-cache
   `location = /nostr-provider.js` block is inserted immediately before it.
3. **Keep an SPA fallback line `try_files $uri $uri/ /index.html;`.** A
   `sub_filter` that injects `<script src="/nostr-provider.js"></script>` before
   `</head>` is inserted right after it. (nginx must have `ngx_http_sub_module` —
   stock `nginx:alpine` does.)
4. **If you proxy an API that does NIP-98 URL verification**, expose
   `proxy_set_header X-Forwarded-Prefix /api;`; the host rewrites it to honor the
   outer reverse proxy's prefix.

The patch is **idempotent** (it checks for an existing `nostr-provider` reference
before editing) and re-runs on reinstall. If you rename or remove any of the anchor
strings above, injection silently no-ops and `window.nostr` will be undefined in your
app — so guard those lines in your config (see the contract comment block at the top of
IndeeHub's `nginx.conf` for a template).

> Non-nginx servers (Next.js `node server.js`, etc.) are not auto-patched today. Either
> serve via nginx, or ship `nostr-provider.js` yourself and reference it in your HTML;
> the canonical script lives at `/opt/archipelago/web-ui/nostr-provider.js` on the node.

Declare iframe intent in the manifest so the launcher embeds (vs. opens a new tab):

```yaml
metadata:
  launch:
    open_in_new_tab: false   # default; set true only if the app cannot be iframed
```

## Security Requirements

Two different things enforce these, and it's worth knowing which is which:

- **The Rust parser (`core/container/src/manifest.rs`)** is the hard gate. A
  manifest that violates one of its rules fails to parse, so the app cannot be
  installed at all.
- **`scripts/validate-app-manifest.sh`** is the submission preflight. It applies
  the *policy* rules the parser doesn't encode, and grades them `fail`/`warn`.

### Mandatory

1. **No `:latest` tag** — Pin a specific version: `myapp:1.0.0`. Checked by the
   preflight script (a `fail` for new apps, a `warn` for existing manifests being
   migrated), **not** by the parser — a `:latest` manifest still installs, so
   pinning is on you.
2. **Read-only root filesystem** — `security.readonly_root: true` (use volumes
   for writable data). This is the parser's default when you omit it.
3. **No privilege escalation** — `security.no_new_privileges: true`. Also the
   parser's default when omitted.
4. **Minimal capabilities** — Drop all caps, only add required ones. The
   allow-list below *is* parser-enforced: anything outside it is a parse error.
5. **No host network unless explicitly approved** — keep
   `security.network_policy` isolated or bridge (`isolated` is the default).

### Allowed Capabilities

The parser currently accepts this allow-list. Keep capability requests minimal; some accepted capabilities still require release review before a public package should depend on them.

| Capability | When Needed |
|-----------|-------------|
| `CHOWN` | App needs to change file ownership |
| `DAC_OVERRIDE` | App needs to bypass file permissions |
| `FOWNER` | App needs ownership-related file operations |
| `NET_ADMIN` | Network administration; requires extra scrutiny |
| `NET_BIND_SERVICE` | App binds to ports below 1024 |
| `NET_RAW` | Raw network sockets; requires extra scrutiny |
| `SETUID`, `SETGID` | App manages user switching |
| `SYS_ADMIN` | Broad administrative capability; avoid for normal apps |

### Forbidden

- Namespace-sharing network modes such as `container:<name>` or `ns:<path>`
- **Host bind mounts outside `/var/lib/archipelago/`.** This is an allow-list,
  not a blocklist of "system paths": `volumes[].source` must be absolute and
  start with `/var/lib/archipelago/`, or be a plain named volume (no slashes),
  or be one of two reviewed exceptions (`/run/user/1000/podman/podman.sock`,
  `/var/run/dbus`). `..` anywhere in the path is rejected. Everything else fails
  to parse, so your app's data belongs under `/var/lib/archipelago/<app-id>`
- Capabilities outside the allow-list above — including `SYS_PTRACE`
- Privileged containers or rootful execution
- Hardcoded secrets in environment variables or images — use `secret_env` or
  `generated_secrets`

## Container Best Practices

### Volumes

```yaml
volumes:
  - type: bind
    source: /var/lib/archipelago/my-app
    target: /data
    options: [rw]
```

Data is stored at `/var/lib/archipelago/{app-id}/` on the host.

Generated files must live under a declared bind-mounted host path:

```yaml
files:
  - path: /var/lib/archipelago/my-app/config.yml
    content: |
      bind: 0.0.0.0:8080
    overwrite: false
```

Use `overwrite: false` for first-run defaults that users or the app may later modify. Use `overwrite: true` only for generated files the platform must own.

`files[].content` supports its own placeholder set — a different one from
`derived_env`:

| Placeholder | Renders to |
|---|---|
| `{{HOST_IP}}` / `{{HOST_MDNS}}` | Host facts (`hostname -I` / the node's `.local` name) |
| `{{NETWORK_GATEWAY}}` | The gateway of the app's Podman network, i.e. aardvark's DNS address. Use it as an nginx `resolver` so container names re-resolve per request instead of pinning a stale IP and 502-ing after a restart |
| `{{secret:NAME}}` | The trimmed contents of the `0600` secret `NAME` from the service-owned secrets dir. `NAME` must be a bare filename. Never logged |

### Health Checks

Define a health check endpoint in your container:

```dockerfile
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
  CMD curl -f http://localhost:8080/health || exit 1
```

### Logging

- Log to stdout/stderr (Podman captures container logs)
- Never log secrets, passwords, or keys
- Use structured logging (JSON) for machine parsing

### Networking

Apps get their own network namespace. To connect to other Archipelago apps:

```yaml
# If your app needs to talk to Bitcoin
dependencies:
  - bitcoin-knots

container:
  network: archy-net
  derived_env:
    - key: BITCOIN_RPC_HOST
      template: "{{BITCOIN_HOST}}"   # resolves to whichever Bitcoin app is installed
    - key: BITCOIN_RPC_PORT
      template: "8332"
```

Prefer `{{BITCOIN_HOST}}` over hardcoding `bitcoin-knots` — a node may be running
Bitcoin Core instead, and the placeholder resolves to whichever is present.

The `archy-net` Podman network provides DNS resolution between containers. Use `derived_env` for host facts like `HOST_MDNS` instead of hardcoding node-specific URLs.

## Catalog Generation

Catalog JSON is generated from manifests during release work. Do not manually edit generated fields in `app-catalog/catalog.json` or `neode-ui/public/catalog.json` when the same value belongs in the manifest.

Manifest-owned catalog fields currently include:

- app title from `app.name`;
- version from `app.version`;
- description from `app.description`;
- Docker image from `app.container.image`;
- category from `app.category` or `app.metadata.category`;
- tier from `app.metadata.tier`;
- icon from `app.metadata.icon`;
- repo URL from `app.metadata.repo`, `repoUrl`, or `source`.

### 1. Build and Push Your Image

```bash
podman build -t docker.io/myorg/my-app:1.0.0 .
podman push docker.io/myorg/my-app:1.0.0
```

### 2. Generate Catalogs

```bash
python3 scripts/generate-app-catalog.py
```

### 3. Verify Drift

```bash
python3 scripts/check-app-catalog-drift.py --release --strict
```

Before release, the canonical catalog and UI public catalog should match:

```bash
cmp -s app-catalog/catalog.json neode-ui/public/catalog.json
```

## Testing Your App

### Validate Your Manifest

Before anything else, check your manifest against the schema and the app-submission
rules:

```bash
./scripts/validate-app-manifest.sh apps/my-app/manifest.yml
```

It reports `STATUS: APPROVED` or `STATUS: REJECTED` with the specific failures —
including the ones that block submission, such as an unpinned `:latest` image
tag (new apps must pin a concrete version). It needs `python3` and PyYAML; it
tells you if either is missing. The Rust parser in `core/container/src/manifest.rs`
remains the canonical validator — this script is the fast local preflight.

### Local Testing

```bash
# Run your container locally
podman run -d --name my-app \
  -p 8180:8080 \
  --read-only \
  --security-opt no-new-privileges \
  --user 1000:1000 \
  docker.io/myorg/my-app:1.0.0

# Verify it works
curl http://localhost:8180/health

# Check logs
podman logs my-app
```

### On an Archipelago Node (before your app is in the catalog)

The App Store lists **signed-catalog apps and Nostr-discovered apps only** —
a manifest on the node's disk never appears in the store by itself. That is
deliberate: the store is a trust surface. But the orchestrator installs from
disk manifests just fine, so you can test the complete install/run/uninstall
lifecycle on your own node before your app is published anywhere.

**1. Stage the manifest where it survives reboots.**

`/opt/archipelago/apps/` is *rebuilt on every backend start* from the runtime
payload that ships inside the frontend bundle
(`/opt/archipelago/web-ui/archipelago-runtime/apps/`). If you copy your
manifest only into `/opt/archipelago/apps/`, the next restart silently deletes
it. Stage into the payload directory instead — the boot sync then promotes it
for you:

```bash
sudo mkdir -p /opt/archipelago/web-ui/archipelago-runtime/apps/my-app
sudo cp apps/my-app/manifest.yml /opt/archipelago/web-ui/archipelago-runtime/apps/my-app/
sudo systemctl restart archipelago   # manifests are loaded at startup
```

Watch `journalctl -u archipelago` after the restart — the orchestrator
validates every manifest on load and tells you about problems immediately
(for example a host-port collision with another installed app).

**2. Install over JSON-RPC.**

The repo ships the same session helper the release lifecycle gate uses.
Three things to know before using it: it needs `jq`; it reuses a cached
session from `/tmp/archy-rpc-session-<uid>` unless `ARCHY_FORCE_LOGIN=1` is
set (a stale cache fails every call quietly); and it sets `set -euo pipefail`,
so run it inside a script or subshell — sourcing it into your interactive
shell makes the first failed step kill the whole chain without printing
anything.

```bash
bash <<'EOF'
export ARCHY_PASSWORD='<your dashboard password>' ARCHY_FORCE_LOGIN=1
# Stock nodes serve HTTPS on 443; dev boxes behind plain nginx use:
#   export ARCHY_HOST=127.0.0.1 ARCHY_SCHEME=http
source tests/lifecycle/lib/rpc.bash
rpc_login && echo "login ok"
# Both fields are required: `dockerImage` is normally supplied by the App
# Store from the signed catalog — pre-catalog, you pass your manifest's
# image yourself (it must match, and must come from a trusted registry).
rpc_call package.install '{"id":"my-app","dockerImage":"docker.io/myorg/my-app:1.0.0"}'
EOF
```

**3. Verify the lifecycle, not just the install:**

```bash
rpc_call package.status '{"id":"my-app"}'        # state + health (run inside the same subshell pattern)
podman ps --filter name=my-app                   # container is up
rpc_call package.stop '{"id":"my-app"}'          # …and start, restart
sudo systemctl restart archipelago               # app must survive this
rpc_call package.uninstall '{"id":"my-app","preserve_data":true}'
rpc_call package.install '{"id":"my-app"}'       # data still there?
```

The app's detail page is `https://<node>/dashboard/apps/my-app`; a gated web
UI is reachable through the app gate on its manifest port once running.

Only after this loop is green does the app belong in a catalog submission —
catalog inclusion is what makes it appear in the App Store.

### Validate Manifest

```bash
cargo test --manifest-path core/Cargo.toml -p archipelago-container
python3 scripts/check-app-catalog-drift.py --release --strict
```

## Updating Your App

1. Build and push the new version: `docker.io/myorg/my-app:1.1.0`.
2. Update `app.version` and `app.container.image` or `app.container.build.tag`.
3. Run catalog generation and drift checks.
4. Validate install/start/stop/restart/uninstall/reinstall behavior before shipping.

The broader app update policy for `1.8-alpha` is still being finalized. Until that policy is locked, app manifests should be explicit and pinned so update detection compares concrete image/tag metadata rather than mutable tags.

## App Icon

- **The tile plate (dark rounded background, border, sheen) is applied by
  the system** — every surface renders your icon with the house
  `archy-app-icon` treatment automatically. What the system deliberately
  does NOT add is an inner margin (icons that already carry whitespace
  would double-margin), so your file needs it baked in: **square canvas,
  ~12% margin per side**. Edge-to-edge marks look wrong next to every
  other tile. Don't hand-tune it: run your mark through the normalizer
  and commit its output —
  ```bash
  scripts/normalize-app-icon.py your-mark.svg apps-icon-output.svg
  ```
  It wraps any SVG (full-bleed upstream logos included) onto the house
  canvas with the standard margin, preserving the original artwork
  untouched inside.
- Provide a URL to your app icon (PNG, WebP, or SVG)
- Recommended size: 256x256 pixels
- Square aspect ratio
- If no icon URL, a generic placeholder is shown in the marketplace

## Release Validation Expectations

Every supported app must satisfy the lifecycle contract:

- install
- launch
- stop
- start
- restart
- uninstall while preserving data
- reinstall with preserved data
- report truthful health/status
- survive backend restart
- survive host reboot

For apps with special dependencies, launch must explain dependency wait states instead of showing a dead iframe. Examples include Bitcoin sync/IBD, Lightning wallet readiness, Nostr signer bridge injection, Tailscale login/auth, and app-specific setup screens.

Runtime changes should be validated with focused tests first, then the release lifecycle harness on the validation host when host access is intentionally resumed.
