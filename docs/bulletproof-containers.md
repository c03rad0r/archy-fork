# Bulletproof Containers

**Status**: historical design record (agreed 2026-04-22). The *architecture* —
level-triggered, desired-state reconciliation — was adopted and is live. Several
specifics below were not built as written, so read this for the incident history
and the reasoning, not as a description of the code. For how the lifecycle
actually works today, read [Container lifecycle](container-lifecycle.md).

What became of the plan, verified against the tree:

| Item | Outcome |
|---|---|
| Level-triggered reconciler | ✅ Shipped, but as `container/boot_reconciler.rs` + `container/prod_orchestrator.rs`. The `core/archipelago/src/reconcile/` module laid out below (`desired.rs`/`current.rs`/`diff.rs`/`apply.rs`/…) **was never created** — no file in it exists |
| FM5 post-OTA probe + auto-rollback | ✅ Shipped — `update-pending-verify.json` (`update.rs:100`) |
| FM4 `host.archipelago` alias | ✅ Shipped — `AddHost=host.archipelago:10.89.0.1` in generated units |
| FM1/FM3 Quadlet ownership | ◐ Partial. Companion UIs run as Quadlet units; **main app containers do not** — `use_quadlet_backends` still defaults false, so the "v1.7.48+ full migration" below has not happened |
| FM2 bitcoin.conf drift | ◐ Solved differently. There is no `reconcile::derived::render_bitcoin_conf`; instead bitcoind is run with an explicit `-conf` derived from secrets at each start and stale datadir configs are removed (`remove_stale_bitcoin_conf`) |
| FM6 podman corrupt-state self-heal | ❌ **Not implemented.** No `podman system renumber` recovery, no startup probe for "invalid internal status". The failure that made a node unreachable in 2026-04 would still need manual SSH |

Note also that the unit paths below say `/etc/containers/systemd/`; units are
actually written per-user to `~/.config/containers/systemd/`
(`quadlet.rs:DEFAULT_REL_UNIT_DIR`), since the whole path is rootless.

**Target**: zero-manual-intervention container lifecycle. A user installs,
uninstalls, reboots, updates, or loses power — every combination must leave the
node in a known-good state without SSH.

---

## Why we're doing this

The v1.7.38 and v1.7.39 rollouts on 2026-04-22 exposed a cluster of container-lifecycle failures that required manual SSH recovery on every affected node. If a user had been on those nodes, they'd have been stuck with "can't reach" or 500 errors and no path forward. We can't ship beta with this class of failure on the table.

The pattern under every failure: **the canonical source of truth had the right answer, but derived state drifted away from it and nothing noticed or fixed it.**

### The six failure modes

| # | Symptom | Root cause |
|---|---|---|
| FM1 | `archy-bitcoin-ui` + `archy-lnd-ui` disappeared from `podman ps -a` after a daemon restart | Archipelago owns container creation imperatively; no owner recreates companions after a crash mid-transition |
| FM2 | ElectrumX "Daemon connection problem" | `bitcoin.conf`'s `rpcauth` drifted from `/var/lib/archipelago/secrets/bitcoin-rpc-password` — config written once at install, never re-derived |
| FM3 | archipelago.service `status=226/NAMESPACE` crash-loop SIGKILL'd every child container | Containers were children of archipelago's cgroup; systemd teardown killed them. `KillMode=control-group` default |
| FM4 | `host.containers.internal` inside containers resolved to LAN gateway (192.168.1.254) | Known podman bug on bridge networks pre-5.3 ([#22644](https://github.com/containers/podman/issues/22644)) |
| FM5 | Nginx 500 fleet-wide after OTA | Tarball root dir was `drwx------` (700), extracted identically on every node. Fixed in v1.7.40 at build time; still need post-OTA auto-rollback |
| FM6 | Rootless podman's `libpod/bolt_state.db` vanished → whole registry node unreachable | No detection of corrupt state; required manual `rm -rf /run/user/$UID/libpod` + `podman system renumber` |

---

## Architecture decision

**Adopt balena-style, level-triggered, desired-state reconciler built on Quadlet + sdnotify.**

This is the one architecture that would have prevented all six failures, because each one is "reality drifted from the intended config and nothing noticed" — the exact problem reconcilers are designed for.

### Why not the alternatives

- **Keep imperative + patch per-failure** — we've been doing this. Five releases in a day. Doesn't scale.
- **Migrate to LXC (StartOS's path)** — 6-month project. Our investment in podman (`install.rs`, `docker_packages.rs`, `image_versions.rs`) is substantial. Quadlet gives us StartOS's isolation property without the migration.
- **Ship k3s / MicroShift** — 400-800 MB RAM baseline on top of bitcoind/electrs. Overkill for a home node OS.
- **Edge-triggered like Umbrel** — their `app.ts` has an explicit TODO admitting they don't handle failure events. We'd inherit the same bug class.

### The four patterns (from mature players)

1. **Desired-state-first, level-triggered reconcile.** balena-supervisor, Kubernetes operators, NixOS. A supervisor owns a manifest of *what should run*; on every tick it diffs against *what is running* and issues steps.
2. **Every container is its own systemd unit, not a child of the daemon.** Red Hat's Quadlet pattern: a `.container` file is parsed by a systemd *generator* into a normal `.service`. The daemon can crash without taking any containers with it.
3. **sdnotify readiness + HealthCmd + rollback.** Podman v3.4+ has real rollback: bad image fails health check, systemd considers service failed, Podman re-tags the previous image digest.
4. **Credentials and config derived from canonical secrets on every apply.** Not trusted across upgrades; re-rendered idempotently from single source of truth.

### Fix-per-failure

| Failure | Fix |
|---|---|
| FM1 | Move companions to Quadlet `.container` files in `/etc/containers/systemd/`. systemd (not archipelago) owns them |
| FM2 | `reconcile::derived::render_bitcoin_conf(secrets)` — pure function, runs every tick, atomic rewrite + HUP on drift |
| FM3 | `KillMode=mixed` in archipelago.service + containers in their own `archipelago-apps.slice`. Quadlet units already live outside archipelago's cgroup |
| FM4 | Ship `/etc/containers/containers.conf` with `host_containers_internal_ip = "10.89.0.1"` + `default_rootless_network_cmd = "pasta"`; also `--add-host=host.archipelago:10.89.0.1` in every unit |
| FM5 | Post-OTA `curl -k https://127.0.0.1/` health probe in new binary startup. If non-200 within 90s, rollback to `web-ui.bak` + binary-backup |
| FM6 | Startup probe: `podman info` with timeout. On "invalid internal status", clear `/run/user/$UID/{containers,libpod,podman}` + `podman system renumber` + reconcile tick rebuilds from Quadlet units |

---

## New code layout (lands in v1.7.48)

```
core/archipelago/src/reconcile/
  mod.rs           run_reconcile_loop, reconcile_once — called from main.rs
  desired.rs       DesiredState built from packages.json + catalog + secrets
  current.rs       snapshot via `systemctl list-units archy-*.service` + `podman ps -a --format json`
  diff.rs          pure: reconcile(desired, current) -> Vec<Step>   (unit-testable without podman)
  apply.rs         step executor with timeouts, structured logs, backoff
  quadlet.rs       write `.container` / `.volume` / `.network` units atomically
  derived.rs       render_bitcoin_conf, render_containers_conf, render_nginx_app_routes
  backoff.rs       restart-history tracking (moved from health_monitor.rs)
```

### Step types (idempotent)

```rust
enum Step {
    WriteQuadletUnit(path, content),
    WriteDerivedFile(path, content),
    WriteSecret(path, content),
    DaemonReload,
    EnsureStarted(unit),
    StopUnit(unit),
    RestartUnit(unit),
    PullImage(ref),
}
```

### Triggers

- 30s interval tick
- install/uninstall RPC
- update-applied event
- explicit `/rpc/v1/reconcile.tick`
- podman event stream (if available)

Level-triggered + idempotent — every call considers full desired vs current diff. Missed ticks/events are irrelevant.

### Edits to existing code

- **`src/main.rs`**: replace `tokio::spawn(crash_recovery::start_stopped_containers)` with `tokio::spawn(reconcile::run_reconcile_loop(state))`. Keep self-heal perms + PID-marker crash detection.
- **`src/api/rpc/package/install.rs`**: stop calling `podman run` directly. Writes desired state + Quadlet unit + signals reconciler. Reconciler does pull + `systemctl start`.
- **`src/api/rpc/package/runtime.rs`** + `lifecycle.rs` + `stacks.rs`: same pattern — mutate desired state, reconciler applies.
- **`src/crash_recovery.rs`**: keep PID-marker + snapshot. Delete `start_stopped_containers` (reconciler handles cold boot). Keep `user-stopped.json` as `AppSpec.desired_state: Started | UserStopped | Uninstalled`.
- **`src/health_monitor.rs`**: strip restart logic. Keep memory-leak detection; push unhealthy events as `Trigger::ContainerUnhealthy(name)`.
- **`src/bitcoin_rpc.rs`**: add `pub fn derive_rpcauth_line(user, pass) -> String` (HMAC-SHA256 per Bitcoin Core's `rpcauth.py`).
- **`src/update.rs`**: post-swap health probe + auto-rollback (v1.7.41).

---

## Shipping order

Each release is independently deployable. Not a big-bang rewrite.

### v1.7.41 — Post-OTA health probe + auto-rollback (closes FM5)
- In `update.rs`: write `/var/lib/archipelago/update-pending-verify.json` just before service restart, with `applied_at`, `new_version`, `previous_version`, deadline.
- In `main.rs` startup: read marker, spawn verification task. Wait 15s for full startup, then `curl -k https://127.0.0.1/` with retries up to 90s.
- On 200: delete marker.
- On non-200 after window: call `rollback_update(data_dir)` (already exists), restart service to boot the old binary.
- Smallest diff, highest ROI.

### v1.7.42 — containers.conf + host.archipelago alias (closes FM4)
- Idempotent write of `/etc/containers/containers.conf` on startup (archipelago compares hash, rewrites only on drift).
- Add `--add-host=host.archipelago:10.89.0.1` to every generated container in `install.rs` / `docker_packages.rs`.
- ElectrumX `DAEMON_URL` migrates from `host.containers.internal` → `host.archipelago`.

### v1.7.43 — `reconcile::derived` for bitcoin.conf / lnd.conf (closes FM2)
- Pure function `render_bitcoin_conf(secrets) -> String`.
- Tick every 30s: read secret, derive `rpcauth`, compare to on-disk, atomic rewrite (via `tempfile::NamedTempFile::persist`) + `podman exec ... kill -HUP 1` on drift.
- Same pattern for `lnd.conf`.
- First user of the eventual `reconcile::` module — ships the `derived.rs` piece early.

### v1.7.44 — Podman state self-heal on startup (closes FM6)
- Startup probe: `podman info --format '{{.Host.OS}}'` with 10s timeout.
- On "invalid internal status" or similar:
  - `systemctl --user stop podman.socket podman.service`
  - `rm -rf /run/user/$UID/{containers,libpod,podman}`
  - `podman system renumber`
  - Trigger reconcile tick (will rebuild containers from their source of truth)
- Surface clear error on `/health` if recovery fails — don't silently serve 502.

### v1.7.45–47 — Quadlet migration per companion (closes FM1 + FM3)
One companion per release so regressions have a narrow blame window:

- **v1.7.45**: `archy-bitcoin-ui` → Quadlet `.container` unit
- **v1.7.46**: `archy-lnd-ui` → Quadlet
- **v1.7.47**: `archy-electrs-ui` → Quadlet

Each:
1. Write `.container` file to `/etc/containers/systemd/<name>.container`
2. `systemctl daemon-reload`
3. `systemctl enable --now <name>.service`
4. Remove the `podman run` path from `install.rs` for that name
5. Add Goss probe for the lifecycle test matrix

### v1.7.48+ — Full reconcile module
- `core/archipelago/src/reconcile/` replaces imperative `install.rs` container management.
- Main app containers (bitcoin-knots, bitcoin-core, lnd, electrumx, btcpay-server, mempool, fedimint) become Quadlet units.
- `install.rs` shrinks to ~300 lines of "write desired state, poke reconciler."
- Biggest diff, lands last.

---

## Test harness (parallel track)

### Stack

- **Outer runner**: `bats-core` — TAP-style bash testing, readable by anyone
- **Verifier**: `goss` — YAML assertions on ports, processes, HTTP endpoints, files. Reused by CI + live probe
- **Chaos layer**: Chaos Toolkit JSON experiments (steady-state-hypothesis → method → rollback → verify)
- **VM layer**: `vmtest` (Go) for reboot-survival + ISO-boot tests, or raw QEMU+SSH
- **Tor probe**: curl through archipelago's own tor SOCKS5 (`--socks5-hostname 127.0.0.1:9050`), 60-180s retry window
- **Live probe**: small Rust agent on every fleet node, ships same Goss YAMLs to Prometheus. Neither Umbrel nor StartOS has this — real differentiator.
- **Reproducibility**: btrfs subvolume snapshots primary (fast), QEMU qcow2 for ISO/kernel-level repro

### Directory layout

```
tests/lifecycle/
  bats/
    _helpers.bash              # install_app, wait_healthy, assert_no_orphans
    00_bootstrap.bats
    10_install.bats            # per-app install
    20_ui_reachable.bats       # direct port + HTTPS proxy + iframe
    30_tor_reachable.bats      # .onion probe
    40_stop_start.bats
    50_restart.bats
    60_reboot.bats             # vmtest-driven
    70_reinstall.bats          # idempotence + data preservation
    80_uninstall.bats          # leak check
    90_soak.bats               # 2-6h hold, periodic probe
  goss/
    bitcoin-knots.yaml
    bitcoin-core.yaml
    lnd.yaml
    electrumx.yaml
    btcpay-server.yaml
    mempool.yaml
    fedimint.yaml
  chaos/
    kill9_archipelago_mid_install.json
    wipe_bolt_db.json
    kill9_bitcoind.json
    reboot_during_ota.json
    corrupt_bitcoin_conf.json
    systemctl_restart_mid_install.json
    fill_disk_99_percent.json
    kill_tor.json
    delete_nginx_snippet.json
    clock_jump_30min.json
  vm/
    iso_boot_smoke.go
    reboot_survival.go
  ci/
    vm_runner.sh
    collect_artifacts.sh
probe/archy-probe/             # Rust bin, reuses goss YAMLs, ships to fleet
Makefile                       # `make beta-matrix`, `make chaos`, `make soak`
```

### Minimum beta matrix

7 apps × 9 lifecycle events × 10 chaos scenarios. Pass = every MUST-ship cell green on fresh rootless-podman single-node CI.

| Case \ App | knots | core | lnd | electrumx | btcpay | mempool | fedimint |
|---|---|---|---|---|---|---|---|
| Fresh install | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| UI direct port | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| UI HTTPS proxy | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| UI iframe | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Tor .onion reachable | ✓ | ✓ | ✓ | — | ✓ | ✓ | ✓ |
| Stop → ports released | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Restart → integrations | — | — | ✓↔btc | ✓↔btc | ✓↔btc,lnd | ✓↔electrs | — |
| Reboot survival | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Reinstall idempotent | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Uninstall no orphans | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| 6h soak | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

**Harness scaffold lands in v1.7.41.** First lifecycle tests blocking v1.7.45. Full matrix + chaos suite blocking beta tag.

### Chaos scenarios (10)

Ordered by likelihood × severity:

1. `kill -9 archipelagod` mid-install → systemd restart, in-flight install resumes or cleanly rolls back
2. `rm bolt_state.db` while service stopped → restart regenerates, no data loss in named volumes
3. `systemctl restart archipelago` mid-install → no orphans, no half-state
4. Reboot mid-OTA → old version intact OR new version active, never half
5. Corrupt `bitcoin.conf` → container restart-loops; UI surfaces banner; reconcile re-derives; other apps unaffected
6. Fill `/var` to 99% → graceful degradation, disk-pressure report
7. Revoke rootless-netns → self-heal within Tor descriptor window
8. `pkill -9 tor` → supervisor restarts; onions reachable within 3–5 min
9. Delete nginx conf snippet → reconciler rewrites or `archipelago doctor` flags drift
10. Clock jump +30min → daemons survive; Tor recovers

---

## Decision log

| Decision | Answer | Rationale |
|---|---|---|
| Scope | 6+ incremental releases, not big-bang rewrite | Each closes one failure class, narrow blame window |
| Quadlet migration | Yes | Isolation from daemon crashes, systemd-native recovery, free from Red Hat's production patterns. Minimum podman version becomes 4.4+ (fine for modern Debian) |
| Live probe to Prometheus | Yes, part of beta | Genuine differentiator — neither Umbrel nor StartOS has this. Adds Grafana dep |
| Test gating | Scaffold in v1.7.41, first tests blocking v1.7.45, full matrix blocking beta tag | Gradual rather than all-or-nothing |

---

## Key sources

### Architecture
- Umbrel [app.ts](https://raw.githubusercontent.com/getumbrel/umbrel/master/packages/umbreld/source/modules/apps/app.ts) — edge-triggered, TODO on failure handling
- StartOS [repo](https://github.com/Start9Labs/start-os), [v0.4 podman→LXC announce](https://community.start9.com/t/startos-v0-4-0-alpha-10-has-replaced-podman-new-commands-for-terminal/4062)
- balena-supervisor [repo](https://github.com/balena-os/balena-supervisor), [Supervisor API](https://docs.balena.io/reference/supervisor/supervisor-api)
- Quadlet: [Dan Walsh 2023 blog](https://www.redhat.com/en/blog/quadlet-podman), [podman-systemd.unit(5)](https://docs.podman.io/en/latest/markdown/podman-systemd.unit.5.html)
- Podman rollback: [auto-update blog](https://www.redhat.com/en/blog/podman-auto-updates-rollbacks), [podman-auto-update(1)](https://docs.podman.io/en/latest/markdown/podman-auto-update.1.html)
- Kubernetes operator pattern: [Kubebuilder reconcile](https://deepwiki.com/kubernetes-sigs/kubebuilder/5.2-reconciliation-loop), [good practices](https://book.kubebuilder.io/reference/good-practices)
- NixOS containers: [wiki](https://wiki.nixos.org/wiki/NixOS_Containers)

### Known bugs & references
- `host.containers.internal` → LAN: [podman #22644](https://github.com/containers/podman/issues/22644), [#23782](https://github.com/containers/podman/issues/23782)
- `bolt_state.db` recovery: [podman #17730](https://github.com/containers/podman/issues/17730), [staticdir mismatch #20872](https://github.com/containers/podman/issues/20872)
- aardvark-dns flakiness: [#20396](https://github.com/containers/podman/issues/20396), [#22407](https://github.com/containers/podman/issues/22407)
- systemd 226/NAMESPACE: [Arch forum](https://bbs.archlinux.org/viewtopic.php?id=156963), [systemd #29526](https://github.com/systemd/systemd/issues/29526)
- [systemd CGROUP_DELEGATION](https://systemd.io/CGROUP_DELEGATION/), [systemd.kill(5)](https://www.freedesktop.org/software/systemd/man/latest/systemd.kill.html)

### Test harness prior art
- Umbrel [ci.yml](https://github.com/getumbrel/umbrel/blob/master/.github/workflows/ci.yml) — Vitest + qemu matrix fan-out
- [YunoHost package_check](https://github.com/YunoHost/package_check) — closest analog, scored per-app lifecycle harness on LXC
- [bats-core](https://github.com/bats-core/bats-core)
- [Goss](https://github.com/goss-org/goss), [dgoss](https://github.com/aelsabbahy/goss-docker)
- [Chaos Toolkit](https://chaostoolkit.org/)
- [vmtest (Go)](https://github.com/anatol/vmtest)

### Tor
- [rend-spec-v3](https://github.com/torproject/torspec/blob/main/rend-spec-v3.txt) — descriptor lifetime + republish cadence
- [stem](https://stem.torproject.org/) — Python Tor controller for `HS_DESC UPLOADED` waits
