# Container lifecycle

How Archipelago keeps apps in the state you asked for — install, start, stop,
restart, uninstall — and how it self-heals without ever resurrecting something
you deliberately stopped. Source of truth:
`core/archipelago/src/container/prod_orchestrator.rs` and
`core/archipelago/src/container/boot_reconciler.rs`.

## The model: level-triggered, not fire-and-forget

Archipelago does not start a container and hope. A long-running **reconciler**
compares *desired state* (what the manifests and your explicit choices say
should be running) against *actual state* (what podman reports) and repairs the
difference. It is **level-triggered**: it acts on the current gap every tick, not
on a one-time event, so a container that dies, a unit that vanishes, or a reboot
that clears everything are all just "the gap is non-zero, close it".

The reconciler is spawned once at boot (`BootReconciler`) after an initial
`adopt_existing()` pass, and runs every **30 seconds**. It finishes an in-flight
pull or build before honouring a shutdown signal — it is never interrupted
mid-operation.

Concurrency: each app has its own async mutex guarding all mutating operations
against the reconciler, so a manual `stop` and a reconcile tick can't race, but
reconciles across different apps still run without serialising against each
other.

## Desired state has three inputs

For each app the reconciler asks: *should this be running right now?* The answer
comes from three durable signals, checked in this order:

1. **Explicitly user-stopped** (`user-stopped.json`). If you stopped an app, its
   id is recorded and the reconciler leaves it down — it is **not** a gap to
   repair. Cleared when you start it again. This is what makes a stop *stick*
   across restarts and reboots.
2. **Explicitly uninstalled** (`user-uninstalled.json`). Same idea for uninstall:
   a baseline app you removed stays removed, so self-heal can't reinstall it.
3. **Otherwise, the manifest set** — every catalog/disk app that isn't stopped or
   uninstalled should be running.

Dependencies are pulled in: an app that is up requires its declared
dependencies, so they are kept up too — but a dependency you explicitly stopped
still stays stopped.

## The operations

All go through the orchestrator, all take the per-app lock, all are idempotent:

| Operation   | What it does |
|-------------|--------------|
| **adopt**   | At boot, take ownership of a pre-existing container **by name** rather than recreating it — preserves data, ports and identity across a daemon restart. |
| **install** | Materialise secrets → ensure image (build from a local Dockerfile or use a pre-pulled image) → create and start the container (via Quadlet where enabled). |
| **start / stop** | Bring the container up/down and record the desired-state change. A stop writes the app to `user-stopped.json`. |
| **restart** | Stop then start, preserving the container's data and identity. |
| **remove**  | Stop and remove the container, **preserving `/var/lib/archipelago/<app>`, secrets, credentials and ports** — a reinstall or upgrade lands on the same data. |
| **upgrade** | Recreate at a new image while preserving data (see the version rules below). |
| **health**  | Report the container's health from its declared `health_check`. |

## Self-heal vs. respecting your choice

The one rule that ties it together: **self-heal must never override a deliberate
stop or uninstall.**

- A container that disappeared while its siblings run — a wedged teardown, a
  reboot that cleared it — is a hole to repair, and the reconciler rebuilds it
  from the durable "was running" snapshot.
- A container that is down because you stopped or uninstalled it is a *choice*,
  and the reconciler leaves it alone.

A small set of **baseline apps** are expected to exist from first boot and
self-heal when their container is missing — but the `user_stopped` /
`user_uninstalled` gates are checked first, so even a baseline app you turned off
stays off. Getting this wrong in either direction is a real bug: resurrecting a
stopped app ignores the operator, and failing to rebuild a crashed one is the
fire-and-forget failure the whole design exists to remove.

## Migrations never destroy data

Any recreate path — upgrade, reinstall, repair — preserves the app's data
directory, its generated secrets, its credentials, its ports, and the container
name used for adoption. An update that would roll a version *backwards* is
refused (see the version guard in `container::image_versions`): the update button
never offers a lower version than what is running, so a stale record cannot turn
into a downgrade. Version pins are honoured — a pinned app is not "updated" out
from under the operator by the catalog.

## Inspecting lifecycle state

```bash
# what podman actually has — run as the archipelago service user (rootless)
podman ps -a --format '{{.Names}}\t{{.Status}}'

# the durable desired-state signals
cat /var/lib/archipelago/user-stopped.json
cat /var/lib/archipelago/user-uninstalled.json

# the reconciler's decisions. archipelago.service is a SYSTEM unit that runs
# as User=archipelago (WantedBy=multi-user.target), so this is not --user —
# unlike the companion Quadlet units, which are per-user.
sudo journalctl -u archipelago | grep -iE 'reconcile|adopt|install|user.stopped'
```

## Related

- [Manifest → Quadlet unit](quadlet-compilation.md) — how the unit the reconciler manages is generated
- [App secrets](secrets.md) — the `ensure_generated_secrets` tick that runs before start
- [App Manifest Specification](app-manifest-spec.md) — `health_check`, `dependencies`, `restart` fields
