# ADR: One provisioning engine — tollgate-installer + Archy post-config

Status: proposed (2026-10-31).
Supersedes the native `install_tollgate` path as the default.

## Context

Archy’s native OpenWrt writer (`core/openwrt/src/tollgate/install.rs`) can
install and configure `tollgate-wrt`, but it does not discover routers on the
LAN, flash firmware, verify SSH host keys, roll back a failed deploy, or
re-acquire the transport after the package’s post-install restart moves the
wired management address. The official `tollgate-installer` does all of those.

The native path also carried a real correctness defect: its “already
installed” guard looked for the repository name (`tollgate-module-basic-go`)
instead of the package (`tollgate-wrt`), so a version bump could be a silent
no-op. That is fixed in the current branch, but the feature gap remains.

Upstream `tollgate-installer` changes that are Archy-specific (brand/config
payload) cannot be assumed to merge before the Archy release. The engine
therefore has to work with the installer’s existing headless API, and Archy
must apply its own product configuration afterwards.

## Decision

Use the official installer as the **single install/flash engine**, driven by
Archy’s backend over its loopback HTTP API. Archy keeps ownership of product
configuration, topology, status, and UI.

```text
Archy backend  ──HTTP──▶  tollgate-installer (node service, host network)
     │                         │
     │                         ├─ discovery / trust / flash / install / verify
     │                         └─ rollback / transport re-acquisition
     └─ config payload (mints, price, SSID) + final status/health
```

One engine, one version source, one UI, no container requirement.

## Ownership

| Concern | Owner |
|---|---|
| LAN discovery, SSH host-key trust | tollgate-installer |
| Firmware flash / clean slate | tollgate-installer |
| Package install + runtime deps | tollgate-installer |
| Rollback, transport re-acquisition | tollgate-installer |
| Version selection/verification | tollgate-installer (`/api/config` `feed_release_tag`) |
| Mints, price, step, private SSID | Archy (payload or post-config) |
| Payout identity | one writer: installer if supplied, else Archy |
| `br-tollgate` topology / NDS interface | Archy |
| Firewall API reachability | Archy (renderer when available; shim otherwise) |
| Final status/health, UI | Archy |

## Version ownership

The installer resolves the feed release tag (`stable` by default, or an exact
`TOLLGATE_FEED_RELEASE_TAG`) and verifies the installed package against it.
Archy reads `/api/config` and displays that tag. Archy does **not** keep a
second independent install pin. `TOLLGATE_VERSION` remains only for the
deprecated native fallback and status compatibility.

## Trust flow

1. Archy calls `openwrt.installer.identify`.
2. If the router’s key is unknown, the installer returns
   `ssh_fingerprint`.
3. Archy shows the fingerprint; the operator verifies it on the router’s own
   console.
4. Archy calls `openwrt.installer.trust`, then retries the deploy with
   `trust_fingerprint`.
5. The trust store lives in the installer’s state directory
   (`/var/lib/archipelago/tollgate-installer`), not in the browser.

## Network assumptions

- The installer runs as a node service on `127.0.0.1:8099`, on the host
  network. The node is on the same LAN as the router during setup.
- If discovery cannot see the router (different subnet, remote node), the
  operator enters the router IP explicitly.
- A container is optional packaging only. If used, it needs
  `network: host`, a state volume, and a loopback bind. A bridged container
  cannot discover the LAN.

## Topology: br-lan, br-private, br-tollgate

The upstream package uses `br-lan` for the paid network and `br-private` for
management. Archy deliberately adds `br-tollgate` (192.168.99.1/24) so paid
guests are not on the admin LAN.

The package’s nft guard originally hardcoded `{ "br-lan", "br-private", "lo" }`
and dropped `:2121` from `br-tollgate`. Upstream #784 (merged 2026-10-26)
replaces that with defines rendered from `nodogsplash.gatewayinterface` and
`network.private_bridge.name`. The installed `0.6.0-r1` predates #784.

Post-config therefore:

1. sets `nodogsplash.gatewayinterface=br-tollgate`;
2. runs `/usr/local/bin/tollgate-nft-interfaces-render --reload` when it
   exists (new package);
3. otherwise applies the literal-set shim (old package).

The shim can be deleted once the Archy release installs a package containing
#784.

## Migration

1. Adapter + UI + ADR land with the official engine as the default.
2. Native path remains behind `ARCHY_TOLLGATE_ENGINE=native` for one release.
3. Hardware validation on OpenWrt 25.12 (apk) and 24.10 (opkg).
4. Removal PR deletes `install_tollgate`, `install_tollgate_apk_native`, and
   the native pin; `version.rs` stays for status.

## Consequences

- The installer binary/service becomes a release dependency.
- Archy’s UI stays the only user-facing surface; the installer’s HTML is not
  used.
- Upstream Archy-specific changes are not required; the adapter uses the
  installer’s stable API and applies Archy config afterwards.
- `br-tollgate` and the firewall shim remain until the package release
  containing #784 is the one Archy installs.
