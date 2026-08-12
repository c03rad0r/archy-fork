# Container subsystem testing — scorecard and roadmap

The bar (verbatim from the v1.7.52 owner):

> "best performant, minimal code, tested containers possible in the world.
> No bloated code, no problems installing a single one, no problems
> uninstalling, every one needs to be tested 20+ times in every state
> before we make another update, not a single container failure outside
> of hardware or internet failure is allowed."

This document is the live tracker for whether we're meeting that bar.
Every PR that touches the container subsystem updates the scoreboard
below. **If you can't honestly tick the box, the change isn't ready.**

---

## Production-quality pass — 2026-06-21 (current, v1.7.99-alpha)

The migration's aim, restated as **five pillars** (every app must satisfy all five):

1. **Quadlet-everywhere** — every container is a declarative systemd Quadlet
   unit under `user.slice`, never inside `archipelago.service`'s cgroup. Kills
   FM3 (restarting/updating archipelago SIGKILLs every container in its cgroup);
   systemd becomes the per-app supervisor.
2. **Level-triggered reconciler** — a 30s idempotent reconcile loop drives
   desired→current from manifests + secrets. Self-healing, not edge-triggered.
3. **Lifecycle bulletproof** — every app passes the full matrix
   (install / UI reachable / stop / start / restart / reinstall / reboot-survive
   / archipelago-restart-survive / uninstall) **5× green on .228** — run ON the node
   (`ARCHY_ITERATIONS=5`).
   (Multinode / fleet testing is tracked separately.)
   before any release.
4. **Data-driven apps** — install/uninstall needs only the app's manifest +
   catalog entry. **No host OS changes** (no apt, no /etc, no host units) and
   **no archipelago binary code per app**. Only *core* apps (bitcoin, lnd,
   electrumx, fedimint + gateway/clientd) may carry bespoke handling if truly
   unavoidable.
5. **Rootless + security-first (non-negotiable)** — containers run in the
   unprivileged `archipelago` user namespace; never root, no `--privileged`,
   drop-all-caps + add-back only what a manifest declares. Secrets are `0600`,
   owned by the service user. Security is king.

**Per-app definition of done:** all five pillars hold → lifecycle matrix 5×
green on .228 (run ON the node) → catalog/registry updated (`app-catalog/catalog.json`
+ `releases/app-catalog.json`, rebuilt image pushed to the mirror) → tracker
cell ticked. Only then move to the next app. (Fleet/multinode verification is a
separate pass, tracked internally.)

**.228 testing constraint:** do NOT touch `bitcoin-knots`, `electrumx`, or
`lnd` on .228 — they are synced and healthy; destructive cycles there would
cost hours of resync.

### Session work log (resolved)

The 2026-06-21 mid-session resume block that lived here (generated-secrets
rollout for fedimint-gateway/-clientd, icon/naming fixes) is **done and
shipped**: the generated-secrets system is a platform primitive
(`container.generated_secrets`, see `docs/app-manifest-spec.md`), the
manifests declare it, and the single-node gate went green on .228 on
2026-06-23. Day-to-day open items live in the issue tracker —
don't add session logs here.

---

## Test layers

| Layer | What it asserts | Toolchain | Latency / iteration |
|---|---|---|---|
| L0 — Rust unit | Pure-function behaviour (manifest parsing, secret resolution, structural invariants) | `cargo test --workspace --bins` | ~5s |
| L1 — RPC API | The JSON-RPC API responds correctly per app (`container-list`, `package.{install,start,stop,restart,uninstall}`, `bitcoin.getinfo`, etc.) | bats + lib/rpc.bash | ~30s per suite |
| L2 — UI surface | The URLs a user actually clicks (dashboard, `/app/<id>/`, direct-port iframes) return 200 with non-empty bodies | bats + lib/ui-probes.bash | ~10s per suite |
| L3 — Lifecycle survival | Containers survive operational events (archipelago restart, host reboot, kill -9 mid-install, OOM) | bats (gated) | ~60s per scenario |
| L4 — Browser journey | Real DOM-level user flow (login → install → wait → click → use) | playwright (TBD) | ~30-120s per journey |
| L5 — Chaos / failure-path | Failure modes recover gracefully (corrupt config, deleted bolt DB, network partition) | bats (chaos-gated) | ~120s per scenario |
| L6 — Performance | Cold install latency, reconcile-tick cost, podman call count per lifecycle event | timed bats + Prometheus (TBD) | ~60s per benchmark |

Release gate: **L0+L1+L2+L3 green × 20 iterations** on .228 (run ON the node; 5× for
now). Multinode/fleet testing is a separate pass. L4+L5+L6 are quality gates
we add as they mature; not blocking the v1.7.52 tag.

## Coverage matrix — current state

Legend: ● fully covered, ◐ partial, ○ missing

### Per-app × per-state matrix (L1 + L2)

| App | Container present | Valid state | RPC reachable | UI URL 200 | Stop | Start | Restart | Reinstall | Reboot survives | Archipelago-restart survives |
|---|---|---|---|---|---|---|---|---|---|---|
| bitcoin-knots | ● | ● | ● | ● (port 8334) | ● | ● | ● | ● | ○ | ◐ regression-gate only |
| bitcoin-core | ◐ shares with knots | ◐ | ○ | ◐ | ○ | ○ | ○ | ○ | ○ | ◐ regression-gate |
| lnd | ● | ● | ● (lncli) | ● (`/app/lnd/`) | ● | ● | ● | ● | ○ | ◐ regression-gate |
| electrumx | ● | ● | ● (TCP 50001) | ● (`/app/electrumx/`) | ● | ● | ● | ● | ○ | ◐ regression-gate |
| btcpay-server | ● | ● | ◐ frontend-port | ● (`/app/btcpay/`) | ● | ● | ● | ● | ○ | ○ |
| mempool | ● | ● | ● (`/api/v1/backend-info`) | ● (`/app/mempool/`) | ● | ● | ● | ● | ○ | ○ |
| fedimint | ● | ● | ◐ container-only | ● (`/app/fedimint/`) | ● | ● | ● | ● | ○ | ○ |
| filebrowser | ○ | ○ | ○ | ● probe-only | ○ | ○ | ○ | ○ | ○ | ◐ via companions |
| archy-bitcoin-ui | ◐ via companions | ◐ | n/a | ● (port 8334) | ○ | ○ | ○ | n/a | ◐ via companions | ● |
| archy-lnd-ui | ◐ via companions | ◐ | n/a | ● (`/app/lnd/`) | ○ | ○ | ○ | n/a | ◐ via companions | ● |
| archy-electrs-ui | ◐ via companions | ◐ | n/a | ● (`/app/electrumx/`) | ○ | ○ | ○ | n/a | ◐ via companions | ● |

Done: 50 of 110 cells. Goal: 110/110 ● for the listed apps before
v1.7.52 tags.

### Layer-by-layer status

| Layer | Tests | Suites | Status |
|---|---:|---:|---|
| L0 unit | 631 | n/a | ● green |
| L1 RPC | 70 | bitcoin-knots, lnd, electrumx, btcpay, mempool, fedimint, required-stack, package-update-smoke | ● for the 6 core apps |
| L2 UI | 9 | ui-coverage | ● for dashboard + 7 proxy paths + bitcoin-ui:8334 |
| L3 lifecycle survival | 14 | companion-survives-archipelago-restart, backend-survives-archipelago-restart, required-stack-destructive, use-quadlet-backends-install | ◐ companions ● ; backends ◐ regression-gate (will fail until Phase 3 Quadlet ships); quadlet post-condition gate ✅ skip-clean today, hard gate when flag flipped |
| L1 wallet-receive / drift / secrets | 5 | bitcoin-receive, port-drift, secret-completeness | ● guards the v1.7.9x wallet fleet failures |
| L4 browser journey | 0 | none | ○ not started |
| L5 chaos | 0 | none | ○ not started |
| L6 performance | 0 | none | ○ not started |

### Wallet / Bitcoin fleet-failure regression suites (added after v1.7.90-alpha)

Three production failures shipped on v1.7.90-alpha despite the existing harness,
because nothing exercised the receive path, port-mapping drift, or secret
completeness on a live node. New suites close those gaps (all run on the archy
host, read-only, so they join `run.sh`/`run-gate.sh` automatically):

| Suite | Failure it guards | Asserts |
|---|---|---|
| `bitcoin-receive.bats` | .116 ("Operation failed" on receive) and .228 (false "wallet is locked") | LND REST reachable on the **manifest** host port; `lnd.newaddress` returns a `bc1…` address on a running node; receive errors are specific, never the generic catch-all |
| `port-drift.bats` | .116 (lnd REST stuck on host 8080 vs manifest 18080) | every installed backend's live `podman inspect` PortBindings match its manifest `ports:` (the external mirror of the orchestrator's `host_port_bindings_drifted`) |
| `secret-completeness.bats` | .198 (bitcoin-knots needs `bitcoin-rpc-txrelay-rpcauth`, never generated → stack cascade) | every `secret_file` referenced by an installed backend manifest exists in the secrets dir |

Backed by L0 unit tests (`cargo test … drift missing_secret lnd`) and a vitest
for the frontend reason-code mapping (`bitcoinReceive.test.ts`). The release
gate `scripts/create-release.sh` now runs `tests/release/run.sh` (which includes
these) and **aborts the release on failure** — previously it ran no tests at all.

## Run commands

```bash
# L0 unit:
cd core && cargo test --workspace --bins

# Single bats suite:
ARCHY_PASSWORD=password123 tests/lifecycle/run.sh bitcoin-knots

# Full bats suite (read-only):
ARCHY_PASSWORD=password123 tests/lifecycle/run.sh

# Full + destructive (for the verification fleet):
ARCHY_PASSWORD=password123 ARCHY_ALLOW_DESTRUCTIVE=1 tests/lifecycle/run.sh

# 5× release-gate run:
ARCHY_PASSWORD=password123 ARCHY_ALLOW_DESTRUCTIVE=1 ARCHY_ITERATIONS=5 \
  tests/lifecycle/run-gate.sh

# CASCADE tier (uninstall → no-ghost → reinstall) — opt-in, NOT in the canonical
# gate. Installs/uninstalls a THROWAWAY app (default grafana; skips if already
# installed). Run on-node to also assert data-dir removal:
ARCHY_PASSWORD=password123 ARCHY_ALLOW_CASCADE_DESTRUCTIVE=1 \
  tests/lifecycle/run.sh cascade-uninstall
```

### CASCADE tier — uninstall/reinstall regression guard (Workstream F)

The 5× gate is DESTRUCTIVE-only (stop/start/restart/survive); it never exercised
uninstall/reinstall, where the worst lifecycle bugs lived. `cascade-uninstall.bats`
closes that gap and encodes the fixes for two field bugs:

| Suite | Failure it guards | Asserts |
|---|---|---|
| `cascade-uninstall.bats` | **#13 uninstall ghost** (immich/grafana stayed in My Apps after uninstall) and **#14 reinstall stops** (stalled on stale state/data) | fresh install reaches `running` via a truthful (non-silent) progression; uninstall makes the entry **disappear from `server.get-state` package-data** (no ghost, no stuck uninstall stage) + removes the container + (on-node) the data dir; reinstall returns to `running`; node left as found |

Throwaway-app + precondition-skip (won't touch an app that's already installed),
so it's safe on a populated node. Override the app via `ARCHY_CASCADE_APP` /
`ARCHY_CASCADE_IMAGE` / `ARCHY_CASCADE_CONFIG` / `ARCHY_CASCADE_DATA_DIR`.
Gated on `ARCHY_ALLOW_CASCADE_DESTRUCTIVE=1`. Verified 7/7 on .228 (2026-06-24).

### All-apps lifecycle matrix (Workstream F)

The per-app suites cover ~8 core apps in depth; `all-apps-matrix.bats` covers
**every installed app in breadth, automatically** — it derives the app set from
`server.get-state` package-data (no hardcoded list) and grows coverage as nodes
install more apps. **Read-only**, so it joins `run.sh`/`run-gate.sh` on every node.

| Suite | Guards (fleet-wide) | Asserts (per installed app) |
|---|---|---|
| `all-apps-matrix.bats` | apps STUCK transitional (the #13/#14 ghost generalized), error/failed apps, unreachable UI apps (port-drift generalized) | settles to a non-transitional state within a window; not error/failed; recognized (non-garbage) state; every **running UI app** (manifest `ui=="true"`) exposes a non-null lan-address |

Tunables: `ARCHY_MATRIX_SETTLE_SECS` (45), `ARCHY_MATRIX_UI_SECS` (30),
`ARCHY_MATRIX_ALLOW_STOPPED` (ids allowed non-running). Verified 5/5 on .228
(17 apps) and .116 (20 apps incl. grafana/nextcloud/photoprism/gitea), 2026-06-24.

To exercise the Phase 3.2 Quadlet-backend path on a target node without
editing config.json (which would require an archipelago restart and
trigger FM3 until 3.5 ships), set the env var on `archipelago.service`:

```bash
sudo systemctl edit archipelago     # add: [Service]\nEnvironment=ARCHIPELAGO_USE_QUADLET_BACKENDS=1
sudo systemctl restart archipelago  # one cgroup-cascade hit; survivable on a debug node
```

After the restart, `package.install` for any orchestrator-managed backend
will route through `install_via_quadlet`, and the
`use-quadlet-backends-install.bats` suite turns from skip → hard gate.

## LoC budget

Goal: minimum-viable container subsystem.

| Module | LoC today | Target | Δ | Status |
|---|---:|---:|---:|---|
| `core/container/src/dependency_resolver.rs` | — | — | -270 | ● deleted |
| `core/container/src/health_monitor.rs` | 196 | 0 | -196 | ◐ pending health migration into reconciler (Phase 3.5) |
| `core/container/src/podman_client.rs::create/start/stop` | ~400 | ~150 | -250 | ◐ pending Quadlet migration (Phase 3.5) |
| `core/archipelago/src/container/dev_orchestrator.rs` | 410 | 0 | -410 | ○ pending dev_mode strategy decision |
| `core/archipelago/src/container/data_manager.rs` | 96 | 0 | -96 | ○ couples with dev_orchestrator |
| `core/container/src/bitcoin_simulator.rs` | 219 | 0 | -219 | ○ couples with dev_orchestrator |
| `core/container/src/port_manager.rs` | 175 | 0 | -175 | ○ couples with dev_orchestrator |
| `core/archipelago/src/api/rpc/package/install.rs::install_bitcoincoin_rpc_repair` | ~150 | 0 | -150 | ◐ pending fold into orchestrator pre-start |
| imperative `install_fresh` in prod_orchestrator | ~120 | 0 | -120 | ◐ Phase 3.2 wired behind `use_quadlet_backends` flag (default off); 3.3 in-place migration ✅; 3.4 health-gated startup (`Notify=healthy`) ✅ + `TimeoutStartSec=600` race fix ✅; 3.4a unit drift-sync each reconcile ✅; flip default after 5× green |

**Today: -270 LoC committed. Outstanding deletes possible: ~1,616 LoC** (if Phase 3 ships fully + dev_mode resolved).

Net target for v1.7.52: container subsystem ≈ **half** of today's LoC.

## Performance KPIs (TBD — measure first, then target)

We don't have a performance harness yet. Add as L6 lands:

| KPI | Today | Target | Notes |
|---|---|---|---|
| cold install: bitcoin-knots manifest → `running` healthcheck | unknown | < 30s once image is local | excludes the ~1GB image pull |
| cold install: lnd | unknown | < 60s once image is local | wallet unlock dominates |
| reconcile-tick wall time (no-op pass over all installed apps) | unknown | < 250ms | the current orchestrator does many `podman inspect` calls |
| podman shell-outs per package.install (orchestrator path) | 7-10 | 1-2 (Quadlet) | post-Phase-3 |
| daemon startup (boot → port 5678 listening) | unknown | < 5s | reconcile is async after this |

## Release gates

1.8.0 ships only when ALL of (see the issue tracker for the live
priority-ordered list of what's still open across these):

1. ☑ Bitcoin-stops fix verified live on a fresh node (`tests/lifecycle/bats/bitcoin-knots.bats`
   stop/restart tier, part of the green single-node gate)
2. ☑ `ARCHY_ITERATIONS=5 tests/lifecycle/run-gate.sh` returns 0 **run ON .228** — GREEN 2026-06-23, 5/5, 0 failures
3. ☐ Multinode/fleet — tracked separately,
   the actual next exit criterion, NOT satisfied yet
4. ☐ The L3 `backend-survives-archipelago-restart` suite passes fleet-wide default-on
   (Phase 3 Quadlet is merged + validated but still opt-in via `ARCHIPELAGO_USE_QUADLET_BACKENDS`
   on .228/.198 only — not the default)
5. ☑ Cargo: 0 warnings, 0 unused (confirmed 2026-07-01 release build); full test suite green
   per last confirmed run
6. ☑ LoC: Phase 3 Quadlet merged (opt-in) — satisfies the "at least one of" bar; default-flip
   itself is tracked as its own item in the unified tracker
7. ☑ Layman-readable changelog — `CHANGELOG.md` backfilled through v1.8.00-alpha
   (per `feedback_changelog_layman.md`)
8. ☐ Tag pushed to origin + gitea-local + gitea-vps2 (per `feedback_ship_ritual.md`) —
   version decided 2026-07-08 (`1.8.0-alpha`); tag once the pre-tag items above close

## How to update this document

When you land a change that materially moves any cell of the matrix or
any LoC row, update this file in the same commit. Reviewers checking
the PR can read the diff to TESTING.md as the answer to "what did
this commit improve?". Without the update, the change is half-shipped.
