# Public Web Router

Optional, manifest-first rootless app for node-terminated HTTPS through an
operator-owned frp gateway. Uses pinned frpc0.71.0 and Caddy2.11.7 binaries and a
pinned multi-architecture Python base. No host network, host port, capabilities,
privileged socket, or node signing keys are needed. FIPS connects the isolated
container to explicitly published website listeners.

Setup stores the private enrollment and derived routes in
`/var/lib/archipelago/public-web-router/config/router.json` (0600). The app mounts
that directory read-only, watches for atomic replacement, validates input, and
supervises only its own Caddy and frpc processes. Removing or invalidating config
stops both. The gateway CA is pinned; HTTPS SNI passes through to Caddy. Caddy
keeps certificate keys under the persistent `/data` bind mount. Uninstall and
Disconnect must preserve that data unless the user explicitly requests removal.

The automatic adapter accepts website IDs or guest-enabled app IDs and resolves
saved domains, FIPS addresses and listener ports on the backend. Arbitrary target
URLs/ports and management endpoints are not accepted. App routes require the
installed catalogue policy to enable guest sharing and retain authentication.
Each request carries the expected project/app identity. The app gate rechecks
its live policy before login actions or static exceptions; a stale route cannot
follow a reassigned port or a disabled gate. Existing manual proxies still work.

No Nostr signer integration is requested: routing neither signs nor broadcasts
Nostr events. Blossom/nsite publication continues to use its explicit profile
signer and exact-byte review. Enrollment files contain private credentials and
must never enter that publishing flow.

Public mode requests ACME using TLS-ALPN-01. A dedicated public443 path must reach
the node through the gateway; competing gateways/proxies must not claim it.
Explicit test mode uses a private Caddy CA and is not browser-trusted public TLS.
The process-health probe reports supervision, not external reachability or
certificate issuance. Setup's independent HTTPS exact-content check remains
required before claiming public reachability.

Framework qualification passed the signed private catalogue, normal manifest
installer, owner-RPC enrollment, exact website bytes through isolated Yaya TLS,
and app guest-cookie issue/revocation. Public ACME on port 443 remains untested;
the isolated test uses a private CA. General publication still requires the
repository release gates.

Distribution must include both `apps/public-web-router` and
`docker/public-web-router` in the runtime payload. Build-source manifests defer
to the shipped disk manifest; the catalogue alone cannot install the build
context. On nodes with `web-ui/archipelago-runtime`, update that payload too:
startup restores it into `/opt/archipelago`. Do not patch only the live copy.

The manifest requests CPU/memory limits. Framework's rootless runtime currently
reports no enforced memory cgroup limit; do not present the requested 256 MiB as
an enforced limit on that host. Read-only root, dropped capabilities, slirp and
read-only configuration mounts were verified on the normally installed app.
