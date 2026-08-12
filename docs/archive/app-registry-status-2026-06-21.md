# Archipelago App Registry — Status Survey

**Generated:** 2026-06-21 · **Survey node:** .228 (archi resilience node, 14-app) · **Binary:** v1.7.99-alpha

This document inventories every app in the registry and reports, per app:
manifest-based or not · installed on .228 · migration status (Quadlet/legacy) ·
automated test coverage / release-gate status.

---

## 1. Architecture context — "manifest-based or not"

**Every registry app is manifest-based.** That is the core architecture
(Pillar 4, *data-driven apps*): install/uninstall needs only the app's
`manifest.yml` + catalog entry — no host OS changes, no archipelago binary code
per app. The live registry on .228 is **40 loaded manifests**
(`Loaded 40 app manifest(s) from disk`).

The **only** non-manifest runtime units are:

- **4 companions** — `archy-bitcoin-ui`, `archy-lnd-ui`, `archy-electrs-ui`,
  `archy-fedimint-ui`. Built from `docker/<name>` contexts via
  `core/archipelago/src/container/companion.rs`, *not* the manifest registry.
- **Stack sub-containers** — `immich_*`, `indeedhub-*`, `netbird-*`. Spawned by
  their parent manifest app.

---

## 2. Migration status (Quadlet-everywhere — Pillar 1)

"Migrated" = runs as a **Quadlet unit under `user.slice`**, so it survives an
`archipelago.service` restart (legacy in-cgroup containers get SIGKILLed on
restart and reconciled back).

On .228 migration is **effectively complete** — every installed app is
`QUADLET:running` **except one**:

| Status | Apps |
|---|---|
| ✅ Migrated (Quadlet / user.slice) | bitcoin-knots, electrumx, lnd, fedimint, fedimint-clientd, fedimint-gateway, btcpay-server (+archy-btcpay-db, archy-nbxplorer), mempool, mempool-api, archy-mempool-db, indeedhub (+7 sub-containers), netbird (+server, +dashboard), vaultwarden, jellyfin, filebrowser, portainer, botfights, nostr-rs-relay, homeassistant, + 4 companions |
| ⚠️ NOT migrated (legacy, service cgroup) | **immich_server** — still in `/system.slice/archipelago.service`. The only legacy holdout. (`immich_postgres`/`immich_redis` are pod members.) |

---

## 3. Exhaustive per-app registry table

| App (registry id) | Manifest | Installed on .228 | Migration | Test coverage |
|---|---|---|---|---|
| bitcoin-knots | yes | ✅ | QUADLET | **L1 RPC ●**, L2 UI ● |
| bitcoin-core | yes | ✗ (shares knots) | — | ◐ regression-gate |
| lnd | yes | ✅ | QUADLET | **L1 RPC ●**, L2 ● |
| electrumx | yes | ✅ | QUADLET | **L1 RPC ●**, L2 ● |
| btcpay-server | yes | ✅ | QUADLET | **L1 RPC ●**, L2 ● |
| mempool | yes | ✅ | QUADLET | **L1 RPC ●**, L2 ● |
| mempool-api | yes | ✅ | QUADLET | via mempool stack |
| archy-mempool-db | yes | ✅ | QUADLET | via mempool stack |
| archy-mempool-web | yes | ✗ | — | via mempool stack |
| archy-btcpay-db | yes | ✅ | QUADLET | via btcpay stack |
| archy-nbxplorer | yes | ✅ | QUADLET | via btcpay stack |
| fedimint (Guardian) | yes | ✅ | QUADLET | L1 ◐ container-only, L2 ● |
| fedimint-clientd | yes | ✅ | QUADLET | none |
| fedimint-gateway | yes | ✅ (this session) | QUADLET | none |
| filebrowser | yes | ✅ | QUADLET | L2 probe-only |
| indeedhub | yes | ✅ | QUADLET | none |
| jellyfin | yes | ✅ | QUADLET | none |
| vaultwarden | yes | ✅ | QUADLET | none |
| portainer | yes | ✅ | QUADLET | none |
| botfights | yes | ✅ | QUADLET | none |
| nostr-rs-relay | yes | ✅ | QUADLET | none |
| home-assistant | yes | ✅ (container `homeassistant`) | QUADLET | none |
| netbird | yes | ✅ (+server, +dashboard) | QUADLET | none |
| immich | yes | ✅ | ⚠️ **LEGACY** | none |
| grafana | yes | ✗ (unit *activating*, no container) | staged | none |
| strfry | yes | ✗ (unit *activating*) | staged | none |
| ~~onlyoffice~~ | — | removed 2026-06-21 | — | — |
| aiui | yes | ✗ | — | none |
| core-lightning | yes | ✗ | — | none |
| did-wallet | yes | ✗ | — | none |
| gitea | yes | ✗ | — | none |
| lightning-stack | yes | ✗ | — | none |
| meshtastic | yes | ✗ | — | none |
| morphos-server | yes | ✗ | — | none |
| nextcloud | yes | ✗ | — | none |
| photoprism | yes | ✗ | — | none |
| router | yes | ✗ | — | none |
| searxng | yes | ✗ | — | none |
| uptime-kuma | yes | ✗ | — | none |
| bitcoin-ui | yes | runs as companion `archy-bitcoin-ui` | QUADLET (companion) | L3 companions ● |
| lnd-ui | yes | runs as companion `archy-lnd-ui` | QUADLET (companion) | L3 companions ● |
| electrs-ui | yes | runs as companion `archy-electrs-ui` | QUADLET (companion) | L3 companions ● |
| fips-ui | yes | ✗ | — | none |

Notes:
- `home-assistant` (registry id) runs as container **`homeassistant`** — the
  app-id ≠ container-name. A duplicate `home-assistant.service` quadlet unit
  sits in *activating*; the live container is `homeassistant` (Up 6 days, healthy).
- `grafana` / `strfry` have Quadlet `.container` units but the units are stuck
  *activating* with **no running container** — staged, not live. Worth a
  separate investigation.
- `onlyoffice` was **removed from the registry on 2026-06-21**.

---

## 4. Test-gate reality

**No app has passed the formal release gate.** The gate is `run-gate.sh` green
across the full lifecycle matrix (install / UI reachable / stop / start /
restart / reinstall / reboot-survive / archipelago-restart-survive / uninstall),
**5× on .228 AND .198**. All 8 release-gate checkboxes in
`tests/lifecycle/TESTING.md` are **unchecked (☐)**.

What exists today:

| Layer | Status |
|---|---|
| L0 unit | 631 tests ● green |
| L1 RPC | ● for **6 core apps only**: bitcoin-knots, lnd, electrumx, btcpay, mempool, fedimint |
| L2 UI | ● dashboard + 7 proxy paths + bitcoin-ui:8334 |
| L3 lifecycle survival | companions ● ; backends ◐ (regression-gate only — fails until Phase-3 Quadlet flag flips by default) |
| Per-app L1+L2 matrix | **50 of 110 cells** |
| L4 browser / L5 chaos / L6 perf | ○ 0 — not started |

Regression suites added after v1.7.90-alpha (run read-only, abort releases on
failure): `bitcoin-receive.bats`, `port-drift.bats`, `secret-completeness.bats`.

**The other ~30 registry apps have zero automated coverage.**

---

## 5. Key gaps

1. **immich** is the last legacy (in-cgroup) app — migrate to Quadlet to finish Pillar 1.
2. **grafana / strfry** Quadlet units stuck *activating* with no container — investigate. (onlyoffice removed 2026-06-21.)
3. **fedimint-gateway / fedimint-clientd** (this session) now run but have no lifecycle test coverage.
4. The formal **5× release gate has never been green** — it is the blocker for the v1.7.52 tag.

---

## 6. This session's changes (2026-06-21)

- **Generated-secrets system** deployed to .228 (binary + manifests). Self-healing:
  the root-owned `fedimint-gateway-hash` was regenerated archipelago-owned/readable
  → **fedimint-gateway now starts** (gatewayd webserver up on :8176). `fmcd-password`
  generated for fedimint-clientd.
- **Guardian-UI CSS fix** applied on .228: rebuilt the stale `localhost/fedimint-ui:latest`
  companion image (built 2026-06-12, pre-fix) from the corrected context
  (`@guardian_assets` proxy fallback to :8177). Guardian's own CSS
  (`/assets/bootstrap.min.css`, `/assets/style.css`) **404 → 200 text/css**.
  Root cause: `companion.rs::ensure_image_present` skips rebuild when the
  `:latest` image already exists, so the context fix never re-baked.

*Survey method: live `podman` cgroup inspection on .228 + `/opt/archipelago/apps`
manifest enumeration + `tests/lifecycle/TESTING.md`.*
