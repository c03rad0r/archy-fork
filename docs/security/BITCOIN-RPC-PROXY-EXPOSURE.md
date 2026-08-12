# The Bitcoin RPC proxy that stayed open after it was fixed

**Status:** code fix committed (`f6b5245b`); on-node verification recorded below.
**Found:** 2026-08-02, a test node, while verifying `a05956c4` instead of assuming it.
**Severity:** critical on any affected node — unauthenticated control of Bitcoin Core RPC
through a proxy that injects the node's own credentials.

## Why this document exists

`a05956c4` closed two unauthenticated endpoints on the wallet UI ports. Its commit message
stated:

> The nginx template is `include_str!`'d and re-rendered on every reconcile pass, so this
> ships atomically with the binary.

That is true for most nodes and false for a specific, silent, and not-rare state. The half
that landed correctly (LND) made the half that did not (Bitcoin RPC) *harder* to notice,
because a spot check of the LND endpoint returns a clean `401` and reads as "patched".

## What was observed

Node running the fixed binary (installed 17:21, contains the new template — `auth_request`
present in the binary at 4 occurrences). All probes from the node's own LAN address, no
cookies, no credentials:

| Probe | Result |
|---|---|
| `GET http://192.0.2.240:18083/lnd-connect-info` | `401`, 24 bytes, `{"error":"Unauthorized"}` — **closed** |
| `POST http://192.0.2.240:8334/bitcoin-rpc/` (`getblockcount`) | `200` — `{"result":960774,"error":null}` — **OPEN** |
| `OPTIONS http://192.0.2.240:8334/bitcoin-rpc/` | `204` with `Access-Control-Allow-Origin: *` — **OPEN** |

The rendered config on disk, `/var/lib/archipelago/bitcoin-ui/nginx.conf`, was dated
**2026-06-30** — the pre-fix version, with no `auth_request` and with the wildcard CORS
header the fix removes.

## Root cause

Three facts have to be true at once, and on this node they were:

1. `bitcoin-ui` is listed in the node's durable `user-uninstalled` marker
   (`/var/lib/archipelago/user-uninstalled.json`).
2. `reconcile_app` returns on that marker (`prod_orchestrator.rs:1956`) **before** reaching
   `run_pre_start_hooks`, which is the only thing that renders the nginx config.
3. The container keeps running anyway, because it is owned by **systemd via a Quadlet
   unit** — `archy-bitcoin-ui.service`, `active`, restarted 17:25 after the daemon restart —
   not by the reconciler that is refusing to touch it.

So: *a container systemd keeps alive, that the orchestrator has stopped reconciling, never
receives a config fix shipped inside the binary.* The marker means "must stay removed", but
nothing enforces removal against systemd, and the orchestrator treats the marker as
permission to stop looking.

This is not a one-app accident. On the same node `archy-electrs-ui` is in the identical
state (uninstalled marker + active Quadlet unit + `Up 10 days`). It serves only a static
page with no credential-injecting proxy, so its exposure is low — but it would miss any
future config fix the same way.

## Why it matters beyond this node

An OTA carrying `a05956c4` would have closed the LND leak everywhere and silently failed to
close the Bitcoin RPC proxy on every node in this state — while making those nodes *look*
patched to exactly the check an operator would run first. That is the most misleading
possible outcome of shipping a security fix.

## The fix

`f6b5245b`: a container that is actually running is a live attack surface whatever a marker
says about it, so its security-relevant config is reconciled even behind the marker, and the
container is restarted so nginx loads it.

Deliberately narrow:

- Nothing is created, pulled, built, started or resurrected. The "must stay removed"
  contract can only weaken for a container that is **already running**, which by definition
  means it was never removed.
- A hook error is swallowed, not propagated — an app the user uninstalled must not be able
  to fail the reconcile pass for every app after it.
- The pre-existing marker test passes unchanged; that is what proves the removal contract
  survived. A new regression test pins the whole chain: stale conf in, gate present out,
  container restarted, nothing created.

## What actually closed it on a test node — and what that does NOT prove

Sequence, from file mtimes, container start times and the daemon journal:

| Time (EDT) | Event |
|---|---|
| 18:33 | Probe: `POST /bitcoin-rpc/` → `200` with a real block height. Exposure confirmed live. |
| 18:36 | A **separate rebuild of bitcoin-ui**, done outside this work, rendered the fixed conf and recreated `archy-bitcoin-ui`. `:8334` closes here. |
| 19:06 | The binary carrying `f6b5245b` is installed and the daemon restarted. |
| 19:12 | Probe: `POST /bitcoin-rpc/` → `401`. `OPTIONS` now returns `Access-Control-Allow-Origin: http://192.0.2.240:8334`, not `*`. |

So the node is closed, and the fixed template is proven to work end to end on real
hardware — but **the reconcile fix itself was never exercised.** By the time it was
deployed, the state it repairs had already been cleared by the unrelated rebuild. The
`401` proves `a05956c4`'s template; it does not prove the delivery path `f6b5245b` adds.

That distinction is the whole point of this document, so it is recorded rather than
rounded off: `bitcoin-ui` is *still* in the node's `user-uninstalled` marker, meaning the
next time its config needs to change, this node depends on `f6b5245b` — untested — or on
someone happening to rebuild the app again.

Tracked as broken window 15 — **since closed by the controlled test below.**

## Proving the delivery path on real hardware

Run on a test node, 2026-08-02 20:00–20:03 EDT, with operator approval. The point was to
prove the thing the incidental rebuild had made unprovable: that **reconcile itself**
repairs this state, unaided.

The daemon was stopped first, so the reconciler could not repair the state before the
re-exposure had been confirmed — otherwise a passing probe would prove nothing about
which mechanism produced it.

| Step | Action | Observed |
|---|---|---|
| 1 | Install a faithfully stale conf (no `auth_request`, credential-injecting `proxy_pass`, `Allow-Origin: *`) and restart the container | — |
| 2 | Probe with no cookies | `POST /bitcoin-rpc/` → **`200`**, `{"result":960790}`; `Allow-Origin: *`. **Genuinely re-exposed** |
| 3 | Start the daemon (20:00:36) and touch nothing further | — |
| 4 | Reconcile pass at **20:02:19** | `bitcoin_ui: nginx.conf rendered auth_hash=51f2b5af`, then `WARN prod_orchestrator: rewrote config for a user-uninstalled app whose container is still RUNNING (systemd/Quadlet keeps it alive independently of reconcile) — restarting so it picks the new config up app_id=bitcoin-ui container=archy-bitcoin-ui` |
| 5 | Probe again | `POST /bitcoin-rpc/` → **`401`**; `Allow-Origin: http://192.0.2.240:8334` |
| 6 | Compare state | Conf **byte-identical** to the pre-test known-good; container healthy |

Step 2 is what makes steps 4–6 mean anything: without a confirmed `200`, the later `401`
would be consistent with the state never having been broken at all.

Both halves are now proven on hardware: `a05956c4`'s template (the gate works) and
`f6b5245b`'s delivery path (the gate arrives at a container the reconciler had been
skipping).

## Credential rotation — decided against, 2026-08-02

The operator's call, recorded here so it is not silently re-litigated: **no LND macaroon
rotation, and no Bitcoin RPC password rotation.** The reasoning was that there is no
evidence of exploitation and the vulnerability is being closed rather than lived with.

`scripts/security/rotate-lnd-macaroon.sh` stays in the tree as a tool. Its ordering
guard (refuses to rotate on a binary lacking the fix) remains the right shape for whenever
rotation is wanted — including for the Bitcoin RPC password, which has no equivalent tool
yet.

**Amended 2026-08-08.** This section said the script "has never rotated anything on any
node"; that is no longer true. A rotation was performed on a development node while
responding to the BTCPay Server advisory (that node had been running an affected
`btcpayserver:2.3.9`), and it exposed a gap the script did not cover: BTCPay's inline copy
of the macaroon was left stranded, so its Lightning payments failed silently while both
apps reported healthy. Rotation is now a first-class, password-confirmed dashboard action
that repairs that copy as part of the run — see
[`LND-MACAROON-ROTATION.md`](LND-MACAROON-ROTATION.md). The fleet decision recorded above
is unchanged: no fleet-wide rotation for this leak.

What this decision accepts: any macaroon or RPC password read through either hole before
it was closed stays valid. That is a deliberate, informed trade, not an oversight.

## Operator note

Deploying the fix rewrites the config and restarts `archy-bitcoin-ui` (a brief Bitcoin UI
interruption, nothing else). Any node that ever had `bitcoin-ui` uninstalled while its
Quadlet unit stayed active should be re-probed with the `POST /bitcoin-rpc/` check above —
a `401` is the pass condition. Treat the Bitcoin RPC password on any node that answered
`200` as known to anyone who could reach that port, and rotate it **after** the fix is
deployed, never before.
