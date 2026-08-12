# Resilience Harness

Black-box state-machine tester for archipelago app containers.

Drives the live RPC against a real archipelago + podman runtime on a target
host. For each app in `app-catalog/catalog.json`, runs every state transition
a user could trigger and asserts the system stays in the expected state.

## Why this exists

We shipped v1.7.43-alpha on .228 with three independent bugs that no unit test
caught:

1. `indeedhub-api` crashlooped 8500+ times because `stacks.rs` was missing 5
   env vars (`QUEUE_HOST`/`QUEUE_PORT`/`DATABASE_PORT`/`S3_PRIVATE_BUCKET_NAME`/
   `AES_MASTER_SECRET`) — the install "succeeded" (containers running) but the
   API never became healthy.
2. `bitcoin-ui` shipped with a stale baked-in `Authorization: Basic …` header
   from the registry image, so every `/bitcoin-rpc/` call returned 401.
3. The container-absence scanner evicted apps from the UI 14 seconds into
   install (before image pull finished).

All three were exactly the kind of bug a "did the user-visible flow actually
work end to end?" test would catch — and the kind a single-file unit test
will never catch. This harness is the gate.

## Running

Against the .228 test node:

    scripts/resilience/resilience.sh archipelago@192.0.2.10

Or non-interactive (CI):

    RESILIENCE_SSH_PASS=… RESILIENCE_UI_PASS=… \
      scripts/resilience/resilience.sh archipelago@192.0.2.10

Filters:

    # Smoke test (3 apps, no reboot, ~15min)
    scripts/resilience/resilience.sh archipelago@192.0.2.10 smoke

    # Single app
    scripts/resilience/resilience.sh archipelago@192.0.2.10 bitcoin-knots

    # Subset
    scripts/resilience/resilience.sh archipelago@192.0.2.10 bitcoin-knots,lnd

Without a filter, the harness sweeps **every** app in the catalog
(~24 apps × 7 per-app transitions + 2 batch transitions) and runs the
batch transitions (archipelago.service restart, host reboot) at the end.
Full sweep is ~3-4 hours and **reboots the target host** as part of the
run — only point it at a dedicated test node.

## What it tests

Per-app transitions:

| # | Transition           | Pass criteria                                  |
|---|----------------------|------------------------------------------------|
| 1 | install              | All containers reach `running` within 10 min  |
| 2 | ui_probe             | HTTP 2xx/3xx via `https://<host>/app/<id>/`   |
| 3 | auth_probe           | (bitcoin-rpc only) returns 200 not 401        |
| 4 | stop                 | All containers reach `exited` state            |
| 5 | start                | All containers reach `running` state           |
| 6 | restart              | All containers `running` after restart         |
| 7 | uninstall            | All containers absent, no residue              |

Batch transitions (full sweep only):

| # | Transition                    | Pass criteria                       |
|---|-------------------------------|-------------------------------------|
| 8 | archipelago.service restart   | Container set unchanged across     |
| 9 | host reboot                   | Container set unchanged across     |

Coverage by design — discovery rather than encoded metadata. The harness
snapshots `podman ps -a` before install, again after install stabilizes,
and the difference IS this app's container set. Works equally well for
single-container apps and 7-container stacks (indeedhub) without per-app
configuration.

## Output

JSON-lines results at `scripts/resilience/reports/<run_ts>/results.jsonl`:

    {"ts":"…","app":"bitcoin-knots","transition":"install","status":"PASS","detail":"bitcoin-knots,archy-bitcoin-ui"}
    {"ts":"…","app":"bitcoin-knots","transition":"auth_probe","status":"PASS","detail":"bitcoin-rpc HTTP 200"}

Exit code: `0` if every cell green, `1` if any red, `2` if setup failed
before tests began. Use as a release gate — refuse to tag if any cell red.

## Auth flow

The harness uses the same `auth.login` RPC that the UI uses, then carries
`session=…` and `csrf_token=…` cookies plus the `X-CSRF-Token` header on
every subsequent call. Re-logs in after archipelago.service restart and
host reboot.

## Caveats / known gaps

- App proxy probe (`/app/<id>/`) only validates the proxy responds — for
  apps with deeper protocol behavior (lnd, fedimint, mempool) this only
  catches "container alive, proxy reachable", not "the protocol is healthy".
- Multi-container stack assertions: the harness checks **every** new
  container is `running`, so it would catch the indeedhub-api restart loop
  while postgres/redis/minio looked fine.
- Host reboot test is destructive and slow — runs once at end of full sweep.
- `package.start`/`stop`/`restart` RPC methods may not exist for all apps;
  failures are recorded and the harness continues.
