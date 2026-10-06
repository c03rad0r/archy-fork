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
      bind: 127.0.0.1
      auth: gated

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
| `app.container.derived_env` | Environment values rendered from host facts. The complete placeholder set is `{{HOST_IP}}`, `{{HOST_MDNS}}`, `{{DISK_GB}}`, `{{BITCOIN_HOST}}`, `{{NODE_IDENTITY_PUBKEYS}}`; an unknown name or an unbalanced `{{` is a parse error, so typos fail loudly. `{{NODE_IDENTITY_PUBKEYS}}` is the Nostr public keys of the identities the app signer (the NIP-07 bridge's identity picker) offers, the node's own appliance identity excluded, as sorted, comma-joined 64-char hex. It is resolved when the app is installed or started, so changes to your identities, including removals, take effect on the app's next restart: until then a removed identity keeps its access; with no such identity the app refuses to start rather than render an empty value |
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
| `app.metadata` | Catalog-facing presentation metadata such as icon, category, tier, repo/source, author, feature bullets, and [launch hints](#browser-iframe-and-companion-launch-modes) |
| `app.interfaces.main` | Optional primary UI launch surface with `port`, `protocol`, and `path` |

Additional extension keys may exist for current integrations, for example Bitcoin, Lightning, or app-specific launch/interface metadata. Treat extension keys as transitional unless they are documented as reusable platform primitives.

### Iframe embedding — the rules

#### What Archipelago decides, and what the app must declare

Archipelago works out the reachable hostname and browser scheme at launch
time. An app must not bake a LAN IP, Tailscale IP, FIPS address, `.local`
name, or the dashboard's current `http`/`https` scheme into its UI URL.
Declare the UI once in `interfaces.main`, put the matching port behind the
app gate, and use relative URLs for the app's own assets and links.

| Concern | App author | Archipelago |
|---|---|---|
| UI location | Declare `interfaces.main.port`, `protocol`, and `path` | Uses the address through which this browser reached the node |
| Exposure | Bind a `gated`/`open` port to `127.0.0.1` | Publishes it on supported LAN, Tailscale, FIPS, and Tor ingress |
| HTTP/HTTPS | Serve the declared upstream protocol locally | Keeps HTTP pages on HTTP; on an HTTPS dashboard, gate-fronted app ports use HTTPS on the same port |
| Embedded or top-level | Default to iframe; declare an exception when required | Chooses iframe, browser tab, or companion-native view from generated launch metadata |
| Navigation | Use relative same-app URLs and normal absolute external URLs | Preserves the selected node address and routes external links out of the companion app view |

`interfaces.main.protocol` describes the service behind the gate. It does
not tell application code to hard-code that scheme into browser links: the
gate can terminate TLS in front of a locally plain-HTTP container.

There are two important limits:

- `auth: none` bypasses the gate, so Archipelago cannot add TLS or make that
  port safe to embed from an HTTPS dashboard. Use it for protocols, not
  ordinary web UIs.
- Same-origin mounts such as `/app/archipelago-source/` are platform-owned
  integrations. A normal app cannot request an arbitrary dashboard path in
  its manifest; use `interfaces.main` and a gated port.

When the platform does provide one of those same-origin mounts, the nginx
location must pass its exact mount as `X-Forwarded-Prefix` to the app gate:

```nginx
location /app/example/ {
    proxy_pass http://127.0.0.2:8123/;
    proxy_set_header X-Forwarded-Prefix /app/example;
}
```

The trailing slash on `proxy_pass` strips the mount from the upstream request;
the header lets the gate put it back into its login form, login-page assets,
and successful redirect. Omitting it makes a fresh mobile-browser session post
to the dashboard's root `/__archipelago-gate/login`, which is not an app-gate
endpoint and will normally return 405. This header is host integration config,
not app-controlled manifest metadata, and must be a fixed literal path.

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
- Therefore **the gate neutralizes frame blocking on gate-fronted responses**: it
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

### Browser, iframe, and companion launch modes

Launch behavior is generated from the manifest. Application code should not
sniff for a particular node IP or companion user-agent.

```yaml
metadata:
  launch:
    # Use only for OAuth/WebAuthn, JS frame-busting, or another top-level
    # browser requirement that the gate cannot repair.
    open_in_new_tab: false

    # Keep a different, app-specific parent-frame integration alive in the
    # Android companion. Standard Archipelago NIP-07 no longer needs this.
    requires_host_frame: false
```

- Desktop/PWA: iframeable apps stay in the dashboard.
  `open_in_new_tab: true` apps open in a browser tab.
- Android companion: ordinary apps open in the native in-app browser with its
  own navigation controls. `requires_host_frame: true` apps stay in the
  dashboard iframe so `window.parent.postMessage` integrations remain alive.
- Never set both flags. A top-level page cannot simultaneously require its
  parent frame.
- Relative app paths are resolved against the active dashboard origin before
  a native launch, so the same package works through LAN, Tailscale, and FIPS.

Test all four relevant paths before submission: HTTP dashboard iframe, HTTPS
dashboard iframe with the node CA installed, companion launch, and every
external link or login redirect that leaves the app.

### Nostr Signer Bridge (NIP-07)

Apps embedded in the Archipelago iframe can use the node's Nostr identity to sign
events without managing their own keys. Archipelago injects a **NIP-07 provider**
(`window.nostr` with `getPublicKey()` / `signEvent()` / `nip04` / `nip44`) that bridges
to the host. Your app code uses standard NIP-07 — no Archipelago-specific API.

**How injection works.** The dashboard owns the consent UI and postMessage
host, and ships the canonical `nostr-provider.js`, but generic containers are
not silently rewritten. Package the provider explicitly with a manifest
`copy_from_host` hook (or bake the same provider into the image) and inject it
into every HTML document your app serves. IndeeHub's manifest is the
hook-based reference; Archipelago Source's outer same-origin nginx mount is a
platform-owned reference.

For an **nginx-served SPA**, use this contract:

1. **Be iframe-embeddable.** The app gate removes `X-Frame-Options` and only
   the CSP `frame-ancestors` directive from responses, but your own config
   should still express the intended embedded deployment rather than relying
   on repair.
2. Serve `/nostr-provider.js` with `Cache-Control: no-cache, no-store`. Never
   precache the provider or the dashboard `/nostr-signer` navigation in an app
   service worker; signing protocol updates must reach existing installations.
3. Inject a versioned provider URL such as
   `<script src="/nostr-provider.js?v=tab-signer-v4"></script>` before
   `</head>` in every SPA document. The token prevents an older iframe-only
   provider from surviving a dashboard update in the browser's asset cache.
   `sub_filter` is suitable when nginx has
   `ngx_http_sub_module` (stock `nginx:alpine` does).
4. **If you proxy an API that does NIP-98 URL verification**, expose
   `proxy_set_header X-Forwarded-Prefix /api;`; the host rewrites it to honor the
   outer reverse proxy's prefix.

Make the hook **idempotent** and fail its verification step if the provider is
not present after install. A silent no-op leaves `window.nostr` undefined and
is not release-ready.

> Non-nginx servers (Next.js `node server.js`, etc.) should ship the provider
> themselves and reference it in their HTML; the canonical host copy is
> `/opt/archipelago/web-ui/nostr-provider.js`.

Choose the launch mode for the app itself; the signer works in either shape:

```yaml
metadata:
  launch:
    open_in_new_tab: false
    requires_host_frame: false
```

The provider supports both launch shapes. In a dashboard iframe it talks to
the dashboard parent directly. In a browser tab or the companion's standalone
WebView it creates a dashboard-origin signer frame, which renders the same
identity chooser and consent card over the app and relays NIP-07 requests to
the authenticated node session. It deliberately does not depend on
`window.opener`, so `noopener` tab launches remain safe and functional.
The app gate's successful login supplies the host-wide session and CSRF cookie
pair in a fresh external browser; the signer broker validates that session
directly and does not require the browser to have visited or logged into the
dashboard first. Existing session-only browser tabs are repaired on their next
gate-fronted app response. Do not add a second dashboard-login prerequisite in
application code.

For that reason a NIP-07 app does **not** need `requires_host_frame: true`.
Use the flag only if the app has some other parent-frame protocol. If a
top-level app sends its own `Content-Security-Policy`, its `frame-src` must
permit the dashboard origin; apps intended to work over every node address can
allow `http:` and `https:` while relying on the provider's strict same-host
parent validation. A policy limited to `frame-src 'self'` will block the
broker when the app is running on a different port.

**Consent UI belongs to the platform.** Do not build a second signer modal,
request a top-level window, or overlay the entire dashboard. A standard NIP-07
call pauses while Archipelago shows its contained consent card inside the
active app surface. After approval, the shared Nostr identity ring provides a
short signing loader and completion state. The same host-owned flow renders in
desktop browsers, installed PWAs, and the Android companion WebView.
Silent background requests and remembered approvals deliberately keep the
broker frame hidden; only an identity choice or an actual consent prompt may
reveal it. If an app performs NIP-98 bootstrap and then navigates, it must wait
for the provider Promise to finish rather than independently reloading while
the consent result is still visible. The canonical provider coordinates its
automatic IndeeHub-style session reload with the broker's hide notification.
For top-level apps, that broker document must remain transparent. When hidden,
its iframe must stay loaded but be reduced to a non-interactive 1px surface and
parked physically off-screen. Removing/display-hiding the full-viewport iframe,
or leaving it full-size with only `visibility:hidden`, can make Android WebView
and mobile Chromium retain its last black/grey compositor surface above a
healthy app until refresh. Keeping one parked broker also prevents a visible
hide/recreate flash between `getPublicKey` and `signEvent`. The canonical
provider owns this lifecycle; apps must not copy or manipulate its iframe.

Apps should treat the NIP-07 Promise as an ordinary asynchronous operation:
disable only the initiating control, preserve the user's draft, handle a user
denial as a normal rejected request, and render the returned result when it
resolves. Never infer approval from elapsed time and never ask the user for an
`nsec` as a fallback.

Archipelago recognizes a synchronous, user-triggered `getPublicKey()` as an
account-selection action. An Archipelago-packaged app should still ask the host
to show the identity chooser explicitly before login, especially when other
asynchronous work happens between the click and the NIP-07 call. This prevents
a returning user from being silently locked to the identity chosen on first use:

```js
await window.archipelagoNostr?.selectIdentity?.()
const pubkey = await window.nostr.getPublicKey()
```

`archipelagoNostr.selectIdentity()` is an optional host enhancement, not part of
NIP-07. Apps must continue to work when it is absent (for example with a normal
browser extension). Invoke it only from a deliberate login/account-switch
action; routine signing calls should continue using the remembered identity.

If a first-launch choice should create the app account automatically, use the
provider's sticky identity subscription and call the ordinary extension-login
action from it:

```js
const unsubscribe = window.archipelagoNostr?.onIdentitySelected?.(() => {
  if (!alreadyLoggedIn()) loginWithNip07()
})
```

The callback runs immediately when an identity was selected just before the
React/Vue component mounted, closing the load-event race seen in browser tabs
and Companion WebViews. Call `unsubscribe()` when the component unmounts. The
selected public key remains available to the immediately following
`getPublicKey()` call; do not add a timeout, reload, or second lookup between
those operations. A plain `archipelago:identity` message remains available for
backward compatibility, but it is not a reliable framework lifecycle API.

Submission testing for a Nostr-signed app must include:

1. `getPublicKey` allow, deny, and remembered consent;
2. `signEvent` with a readable event-kind/content preview;
3. the contained review → identity-ring loader → completion sequence;
4. changing the selected identity and confirming remembered consent does not
   cross identity boundaries;
5. HTTP and HTTPS dashboard frames, a `noopener` browser-tab launch, and the
   Android companion's standalone WebView;
6. choosing an identity immediately when the first-launch picker appears, to
   prove the app's account store is ready before the result arrives; and
7. Companion → **Open in browser** in a browser with no prior dashboard
   localStorage: complete the app gate, then prove the contained signer can
   choose an identity and sign without asking for a second node login; and
8. after the first identity choice and after NIP-98 authentication, confirm the
   underlying app paints immediately—no black frame and no manual reload.

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

Stage the other files declared by the manifest too. A local-build app needs its
build context under the corresponding runtime payload `docker/<app-id>/`
directory, so boot sync can promote it to `/opt/archipelago/docker/<app-id>/`.
Copy the normalized icon to its declared public path under
`/opt/archipelago/web-ui/` for a local test; the normal frontend bundle must carry
that asset for release. Do not replace the signed catalog to make a local test
app appear in the store.

Check the **My Apps** tile while installation is in progress and after it
finishes: name, icon and UI classification come from the manifest. An API-only
app belongs in Services. A UI app must remain in My Apps while installing, with
launch disabled until it is ready. Verify the real Launch button opens the
embedded app, rather than testing only its direct port URL.

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

### Adapters for shared services

A service that reuses an installed stack can declare `install_prerequisites`
with the required component app ids and keep the runtime relationship in
`dependencies`. This refuses an incomplete installation before creating the
adapter instead of reporting a successful installation with no usable backend.
For example, Angor Indexer requires `mempool-api` (shown to users as its owning
Mempool app), shares that index and declares only an `api` interface. API-only
interfaces belong in Services and do not generate browser launch buttons.


## Launch acceptance: credentials, signer, and HTTP nodes

An app is not ready just because its container is healthy. Before submission,
verify its first launch from My Apps, app details, a browser tab and Companion:

- Declare a real UI interface and stage the app icon in the web UI assets. Check
  the installing tile as well as the completed installation: a UI app belongs
  in My Apps and must not appear as an iconless service.
- If the app needs a password or first-run token, provide the shared credential
  interstitial **before** launch, with copy controls and setup instructions.
  Generating a secret in the manifest does not register this screen. Implement
  `package.credentials` in `core/archipelago/src/api/rpc/package/install.rs`
  and register the app in `CREDENTIAL_INTERSTITIAL_APPS` in
  `neode-ui/src/stores/appLauncher.ts`. Both changes require a platform update;
  app-only sideloads cannot add this RPC integration. File Browser and DATUM
  are examples. Read generated secrets from the node's configured data directory;
  never put them in a manifest, static browser bundle, default-password fallback,
  logs, screenshots, or test reports. Keep the RPC dashboard-authenticated.
- Explain initial configuration and client connection details. For DATUM this
  includes its administrator password, Bitcoin payout address, and the node's
  Stratum address on port 23334. App-to-app connections use container DNS
  (`http://datum:7152`), never a container IP address.
- Native Nostr apps should open the host identity chooser once on an explicit
  app launch when unauthenticated, then finish the app's ordinary NIP-07 login.
  The app may call `archipelagoNostr.selectIdentity()` at initial mount for this
  first-launch flow; this is the exception to the routine-signing rule above.
  Do not assume the platform's eager-picker app list contains a new app ID.
  Consume an already selected identity through `getSelectedIdentity()` or the
  sticky `onIdentitySelected()` subscription to avoid a second chooser.
  Preserve manual login/account switching, external extensions and remote
  signers. Cancellation must leave a usable login screen without reopening a
  prompt loop; signing still requires the platform's normal consent.
- Test the actual HTTP LAN/Tailscale address, not only localhost or HTTPS.
  `crypto.subtle` and clipboard APIs may be unavailable on those addresses.
  Keep authenticated encryption: use a vetted compatible implementation when
  WebCrypto is absent, and secure randomness (`crypto.getRandomValues`). Test
  existing-message decryption, tamper rejection and an HTTP round trip.
- Verify first launch, cancellation/retry, reload, owner/viewer authorization,
  and persisted data after app recreation. Use the shared browser-check suite
  outside the repository; record which nodes and browser engines were tested.

## Native audio player and video integration

Audio apps can integrate with the native bottom player while retaining their own
playback engine, queue and authenticated media. Follow the
[audio message protocol, manifest example and acceptance checklist](app-media-integration.md).
V4V is the reference implementation for close/reopen, artwork and queue controls.
That guide also tracks the separate companion Cloud video PiP work; audio bridge
support must not be advertised as already providing native video PiP.
