# External access and website publishing

Approved on 2026-10-08. Worktree: `archy-external-access`; branch:
`work/external-access-websites`; base: `c57119e9`. The release checkout,
services, build directories, artifacts and publication refs are not work areas.

## Product contract

Setup offers Allow external connections and Publish a website. Both use node-owned
connection state. Each app or site can select FIPS, public HTTPS, Tor, and (static
sites only) Nostr publication together. Each route has independent status and
revocation. Configured, locally available, and externally verified are different
states. Never infer public reachability from a saved record or running daemon.

Reuse existing FIPS IPv6 ingress, AppGate and IPv4 loopback backends. Never widen
container bindings as a blanket IPv6 migration. Preserve local-only APIs and
wallet/admin exclusion policies. Existing routes are not adopted or revoked
without an explicit ownership handoff. Private route success does not satisfy a
public website's prerequisite. Removing one publication must retain shared routes.

All required components are open source and self-hostable. No Tailscale or
proprietary control-plane dependency. Public routing uses frp with TLS terminating
on the node; gateways are selectable and replaceable. DNS and gateways remain
operator dependencies, even when their software is open source.

Consider Nostr first wherever an existing standard fits: FIPS identity/discovery,
NIP-5A/nsyte/Blossom for optional static-site replication, existing signing flows,
and ngit contribution/review. Public announcements and replication require an
explicit choice. Keys and infrastructure credentials never enter model context.

Mynymbox is an optional external Bitcoin/Lightning domain checkout. Explain its
registrant-of-record model. Generate exact DNS record instructions for the chosen
route; support existing domains and free addresses. Preserve mail records and
verify authoritative DNS, hostname routing, TLS and external HTTP independently.
FIPS/onion addresses do not require a purchased domain. No automatic purchases.

AIUI creates node-owned static website projects with isolated previews, revisions,
download, publish, rollback and unpublish. Local/open model operation is supported;
no silent fallback to a proprietary model. A published website has a separate
origin from management and cannot receive dashboard cookies, signing authority or
RPC access. Public copies may survive unpublishing from Nostr/Blossom.

The user also requested removal of the File Browser Setup card because the app
is already bundled in the ISO. Keep the installed app and launcher unchanged.

## Implementation and acceptance ledger

- [x] Isolated worktree and branch created.
- [ ] Persistent, versioned project and multi-route configuration; conflict-safe writes.
- [ ] Both Setup entry points and shared connection readiness.
- [ ] DNS guidance with Mynymbox handoff and correct per-route records.
- [ ] Static site workspace, local model generation, isolated preview and version history.
- [ ] FIPS publication and revocation through owned listeners/policies.
- [ ] Public HTTPS/frp configuration, scoped enrollment and node TLS lifecycle.
- [ ] Tor publication preserving service identity through restart.
- [ ] NIP-5A signing and Blossom replication with explicit consent and pinned versions.
- [ ] Per-app restricted access without sharing administrator credentials.
- [ ] External verification, certificate renewal, restarts, rollback and route isolation.
- [ ] Framework acceptance with confirmed identity, access and release coordination.
- [ ] ngit review and exact accepted-commit mirror parity before any release.

The user authorized Framework as a test node if deployment is needed. Access and
current release-agent reservation must be confirmed before live work. Preserve all
wallet/channel/app data. Source tests are not node acceptance. Backend unit tests
run only through `scripts/test-backend-isolated.sh`; use a worktree-local target.

## Development evidence (2026-10-08, not release acceptance)

The isolated branch now contains versioned node-owned projects, multi-route
preferences, both Setup screens, local Ollama draft generation, sandboxed static
previews, revision restore and FIPS-only static publication/revocation. AIUI can
hand HTML to Setup for explicit import. Public HTTPS, Tor and Nostr adapters and
per-app grants remain outstanding; selecting a route does not enable it.

The File Browser Setup card has been removed as requested. Its catalog entry and
launcher remain intact. No installed applications were changed.

The backend compilation passed, including the supervisor snapshot repair. Eleven focused backend tests passed
through the isolated runner, including the actual publishing and FIPS interface
modules. The initial frontend typecheck and six publishing tests passed. Later
AIUI handoff checks subsequently passed: AIUI typechecking, dashboard typechecking,
and 30 bridge/import tests, including rejection of messages from another frame or
origin. The earlier combined dashboard run passed 52 tests. These are source
checks, not full application deployment acceptance.

Framework access and availability were confirmed by the operator. Read-only SSH
inspection identified framework-pt and its installed FIPS 0.4.1. Yaya access was
also confirmed; its reverse proxy has the existing archipelago.builders route.
The operator subsequently confirmed Yaya is free and explicitly authorized a
separate test route. The standalone production publisher driver ran on Framework
under `archy-publishing-smoke.service`, using only
`/home/archipelago/publishing-smoke`. The main backend was not replaced or
restarted. No wallet, channel, application data or DNS settings were changed.

Live checks completed:

- Two temporary static sites used FIPS ports 32000 and 32001. Yaya fetched the
  first over FIPS with HTTP 200 and the restrictive CSP intact.
- Restarting only the test publisher retained the sites. Unpublishing the first
  closed its listener and removed its rule while the second still returned 200.
- Temporarily removing the test state file closed the second listener and removed
  its allowance. Restoring the file restored the publication. This validates the
  repaired stale-snapshot failure case.
- NPM proxy host 9 routed only `free.archipelago.builders` to Framework's second
  FIPS site. Certificate 16 was issued successfully. Public HTTPS returned the
  expected page with normal certificate verification; `/rpc` returned 404 and
  `/../../etc/passwd` was rejected with 400.
- Unpublishing the remaining site left no website allowances or listeners. The
  proxy request timed out without returning the old page (not a claimed 404 or
  verified friendly error page).
- The temporary publisher was stopped; its empty owned firewall drop-in was
  removed and the FIPS baseline reapplied. Framework's main backend remained
  active. Test proxy host 9 was deleted; certificate cleanup is checked separately.

This proves the static serving module and the existing-proxy/FIPS path. It does
not prove dashboard RPC integration on Framework, full-node reboot recovery,
certificate renewal, automated gateway enrollment, Tor or Nostr publishing. The
test HTTPS setup terminated TLS at the operator's proxy; end-to-node TLS for a
new frp gateway is still separate outstanding work.

## Research links

- FIPS master `57bc5108f708258c67dfc713e98e6bbb5a828e95` (2026-10-07), latest release
  v0.5.2: https://github.com/jmcorgan/fips . Gateway forwards accept IPv6 targets;
  Archipelago already has a separate FIPS-to-IPv4 relay and an IPv6-capable AppGate.
- frp: https://github.com/fatedier/frp (Apache-2.0).
- nsyte: https://github.com/sandwichfarm/nsyte (MIT).
- NIP-5A: https://github.com/nostr-protocol/nips/blob/master/5A.md (draft).
- Mynymbox: https://mynymbox.io/domainregistration and
  https://mynymbox.io/docs?doc=domains/dns-records .

## Tor website adapter

Website publication uses a dedicated child Tor process with `SocksPort 0` and
`ControlPort 0`, explicit owned configuration, and 0700 identity/runtime directories
under `publishing/onions`. It does not regenerate app Tor configuration or restart
the system Tor daemon. Each onion forwards only to its own static listener on
127.0.0.1:32100–32131. Removing a website closes that listener before reloading
this owned process; the other onions and all private keys are retained. The
process exits when there are no published onion websites. No key wipe is part of
unpublishing. The UI distinguishes having an onion address from verified external
reachability.

A standalone candidate on Framework served the second temporary onion to Yaya's
Tor client with HTTP 200 and the expected CSP. After unpublishing the first onion,
the second still returned 200. Republishing the first retained its hostname; a
publisher restart also retained that hostname. The existing system Tor process
remained PID 1449 throughout these checks. A fresh external fetch after restart
and final cleanup are recorded below when complete.

Source validation now includes per-transport revoke isolation, Tor configuration
path/port constraints and shared connection preferences. All 12 isolated backend
tests passed; the latest selected frontend run passed 36 tests and typechecking.
The AIUI package typecheck passed separately. The final integrated backend check
passed. NPM test certificate 16 was successfully deleted after proxy
host 9; no test proxy remains on Yaya.

The fresh external fetch of the first onion after republish and publisher restart
returned HTTP 200 with the original hostname and expected page. Both test onions
were then unpublished and the smoke unit stopped. Only system Tor PID 1449
remained; no website listeners or owned FIPS drop-in remained. Both onion identity
directories were preserved. The final state-directory durability change passed
the 12-test isolated backend suite as well.
