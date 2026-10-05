# NPM certificate failure: repair and next-release gate

Status: OPEN for release — affected Angor endpoint repaired and publicly verified;
durable source correction and release regression acceptance remain pending.

The operator requested immediate repair on Shorty's node and a complete, tested
fix for subsequent releases. The independent review agent acknowledged receipt
of the repair/handoff on 2026-10-01. This does **not** establish receipt by the
owner of another release session. That owner must acknowledge this document and
record implementation and acceptance before shipping the next OTA or ISO.

## Evidence and immediate repair

The live investigator reported NPM certificate failures at 18:06, 18:07 and
18:10 UTC on 2026-10-01: the CA received HTTP 404 for its HTTP-01 challenge.
The running container mounts `/var/lib/archipelago/nginx-proxy-manager` at
`/data`, but host nginx served the challenge from the obsolete nested
`/var/lib/archipelago/nginx-proxy-manager/data/letsencrypt-acme-challenge`.
NPM writes to `/data/letsencrypt-acme-challenge` inside its container. These are
different host directories. Host nginx owns public ports 80 and 443.

The investigator backed up `/etc/nginx/sites-available/archipelago` as
`/etc/nginx/sites-available/archipelago.before-angor-acme-1790878342`, corrected
the default HTTP challenge root to the actual mount's challenge directory,
passed `nginx -t`, and reloaded nginx. A temporary challenge file written inside
NPM returned its exact expected body over the public domain's port 80.
This initial probe proves the repaired challenge route; it alone does not prove issuance,
certificate attachment, public HTTPS routing, renewal, or release persistence.
No wallet or channel data is involved in this repair.

## Independently confirmed source inconsistencies

- `apps/nginx-proxy-manager/manifest.yml`: base app directory mounts at `/data`;
  separate `letsencrypt` directory mounts at `/etc/letsencrypt`; only the admin
  port is published. NPM's own public listeners are not published by this path.
- `image-recipe/configs/nginx-archipelago.conf`: default challenge location uses
  the obsolete nested `data/letsencrypt-acme-challenge` root.
- `scripts/sync-npm-public-hosts.sh`: both SQLite DB and challenge root use the
  nested directory. Missing DB exits successfully without syncing any hosts.
- `scripts/container-doctor.sh::fix_npm_public_hosts`: its independent nested
  DB existence guard prevents the synchronizer from running on manifest installs.
- `core/archipelago/src/api/rpc/package/config.rs` and `runtime.rs`: legacy
  creation/repair still mount the nested `data` directory at `/data`.
- `scripts/first-boot-containers.sh`: legacy first boot creates and mounts nested
  data, and publishes different public-listener ports from the manifest path.
- The synchronizer exports selected NPM fields to host nginx. It checks enabled
  hosts with a certificate, but does not filter deleted rows, validate inserted
  configuration values, or preserve all NPM access/custom-location behavior.
  It restores the old generated file on syntax failure, but not reload failure.

## Required implementation

1. Define one authoritative method for finding the active NPM data mount and
   certificate store across fresh installs and supported legacy layouts. Do not
   blindly change mounts and strand the operator's existing DB, hosts or account.
   Detect ambiguous dual databases explicitly. Back up before any migration;
   preserve existing certificates, private keys, renewal files, account records,
   custom settings, permissions and uninstall decisions. Repeated migration must
   be harmless. Failed migration must leave the original usable state intact.
2. Make default and named-host HTTP challenge routes use that active directory.
   Include pre-certificate issuance and renewal under forced HTTPS. Do not expose
   account/private-key/database directories through nginx.
3. Correct the synchronizer and doctor gate together, with reliable lifecycle
   invocation after host/certificate edits and service startup. A certificate
   created in NPM must actually become the certificate served by public nginx.
   Avoid requiring users to run a repair command for each host or renewal.
4. Define how the host bridge preserves NPM routing and security settings.
   Handle disabled/deleted hosts, host edits, certificate replacement/deletion,
   multiple domains, custom locations and access restrictions correctly. Do not
   silently publish a restricted NPM host as an unrestricted host-nginx proxy.
   Validate DB-derived configuration, serialize concurrent writers, avoid
   unnecessary reloads, and retain a working config on generation/test/reload
   failure. Report actionable failure causes instead of apparent success.
5. Carry the correction through actual OTA migration and fresh ISO paths.
   Include source, runtime scripts, nginx configuration, and app catalog as
   applicable. Verify candidate package contents and installed behavior rather
   than assuming a source edit is shipped by every packaging path.

## Acceptance matrix — all relevant gates require recorded results

Use disposable fixtures/test domains for destructive/error cases and ACME
staging for repeated issuance/renewal. Avoid production CA retry loops. Backend
unit tests on installed nodes must use `scripts/test-backend-isolated.sh` per
`AGENTS.md`. Do not reboot an operator node without the necessary recovery/access
arrangements and authorization; use a disposable VM for release lifecycle tests.

- [ ] Fresh manifest installation: actual mount, DB path, admin API, public
      challenge file and first certificate request work without manual repair.
- [ ] Legacy nested-data upgrade: hosts, accounts, certs, renewal data and
      custom settings remain intact; the active DB is still the original DB.
- [ ] Current flat-data upgrade: same preservation assertions; no empty database
      is initialized and no old nested DB is silently chosen instead.
- [ ] Ambiguous dual DBs, missing/corrupt DB, backup failure and permission errors
      fail safely with useful diagnostics; no deletion or identity replacement.
- [ ] Challenge file created in container is fetched byte-for-byte from public
      port 80 for an unconfigured domain, named host and forced-HTTPS host.
- [ ] Staging first issuance completes through the normal NPM UI/API. One live
      production issuance on the affected node is independently verified with
      hostname, trust chain, validity and actual served certificate.
- [ ] Certificate binding and public HTTPS route reach the intended Angor
      backend; normal API health and a representative read-only indexed request
      are checked separately from TLS. Backend readiness failures stay explicit.
- [ ] Staging renewal succeeds through the normal scheduling/renewal path and
      public nginx reloads the renewed certificate without manual intervention.
- [ ] Host create/edit/disable/delete, certificate replacement, multi-domain
      routing, restrictions and custom locations match supported NPM behavior.
- [ ] Injection/invalid data, concurrent sync, nginx syntax failure and reload
      failure preserve the previous working service; retries converge safely.
- [ ] NPM restart, manager restart, host nginx restart, controlled VM reboot and
      repeated reconciliation preserve public routing and certificates.
- [ ] Migration is idempotent; upgrade rollback retains original data and
      usable routing. Existing unrelated public hosts continue working.
- [ ] Signed OTA and RAW ISO candidate contents contain the same correction;
      installed OTA legacy/flat layouts and fresh ISO pass the relevant checks.
- [ ] Release owner acknowledges receipt and records exact commit/artifact IDs,
      test commands/results, live evidence, remaining limits and release decision.

## Handoff acknowledgements

- 2026-10-01: independent review agent received the parent investigator's live
  fix details and explicitly acknowledged responsibility for source review and
  this test/handoff checklist. No production code or node changes by reviewer.
- 2026-10-01: next-release owner explicitly acknowledged this handoff in
  `/tmp/npm-release-handoff-ack.txt` and adopted the matrix as a required
  1.8.23-alpha OTA/raw ISO gate. Received the operator report that Shorty
  certificate npm-8 is issued and public HTTPS health returns 200. Independent
  release validation and the durable fleet correction remain pending.

## Final live repair evidence — investigator report, 2026-10-01

The live investigator issued the certificate through NPM's own API using an
ephemeral in-memory local admin token, without changing passwords or disclosing
the token. Certificate ID 8 expires at `2026-12-30 17:16:35`; existing proxy host
ID 2 now uses that certificate with forced HTTPS enabled.

The running container publishes only its admin listener (container 81 to host
`127.0.0.1:8081`). The investigator therefore added a domain-scoped host-nginx
configuration at `/etc/nginx/conf.d/angor-indexer-npm.conf`. It serves HTTP 80 and
HTTPS 443 on IPv4 and IPv6, uses the corrected ACME root and NPM certificate 8,
and forwards to `http://127.0.0.1:8998`. `nginx -t` and reload passed.

External requests with normal TLS verification confirmed:

- `/health`: HTTP 200, indexed height 969474.
- `/`: valid mainnet JSON response.
- `/api/v1/fees/recommended`: valid JSON response.
- HTTP requests redirect to HTTPS with HTTP 301.
- CORS preflight `OPTIONS /api/tx` with origin `https://angor.io`: HTTP 204,
  allowed origin `*`, method POST and header Content-Type. This was a preflight
  check, not a transaction submission.
- An existing shop endpoint continued to return HTTPS 302.

The temporary challenge probe was removed. These are investigator-reported live
checks, not independently repeated node checks by the review agent. They establish
that the affected endpoint now serves trusted HTTPS and responds as an indexer.
They do not establish renewal, automatic host bridge updates, restart/reboot or
packaged-release correctness; the source repair remains the release owner's work.

A separate read-only public probe of the existing Shorty's website failed TLS
hostname verification. Its configuration was unchanged by this repair, and no
pre-repair baseline establishes when that mismatch began. The release owner must
investigate it separately and verify existing-host compatibility; do not attribute
it to this repair without evidence or silently mark that gate passed.

The investigator located the actual main release session, delivered the handoff,
and observed its explicit acknowledgement. The owner then recorded receipt in
`/tmp/npm-release-handoff-ack.txt` and in the acknowledgement section above.
Publication remains held for the NPM release gate. Unavailable external acceptance
must be stated explicitly and cannot be silently treated as passed.

## Urgent public dashboard exposure gate — 2026-10-01

Investigator reports the Angor relay hostname reached the default Archipelago
login because its certificate existed without a corresponding host-nginx route.
The investigator owns the immediate Shorty nginx repair; the release session
will not modify that configuration concurrently. Exact final evidence is pending.

- [ ] Unknown public HTTP Host / TLS SNI and direct public-IP requests cannot
      expose the dashboard, login assets or RPC, including IPv6 and any trusted
      reverse-proxy/tunnel path. Test spoofed forwarding headers explicitly.
- [ ] LAN/private/tailnet dashboard access remains available as intended.
- [ ] Public HTTP ACME challenge access survives those restrictions.
- [ ] NPM host creation/edits automatically propagate HTTP/TLS routing.
- [ ] Relay hostname serves the intended relay and WebSocket upgrade using its
      correct certificate; certificate existence is not route acceptance.
- [ ] These protections survive manager/nginx restart, renewal and OTA/ISO.

## Live security containment and project discovery follow-up

2026-10-01: relay certificate existed but named public route was absent; default
HTTP/HTTPS vhosts exposed the dashboard to public clients, including direct WAN
IP access. Investigator added live `angor-relay-npm.conf` forwarding TLS cert11
to loopback8091 with WebSocket upgrade; added `00-dashboard-source-guard.conf`
private-source geo/map guard to both management default vhosts, preserving public
ACME challenge paths. Backup: `/etc/nginx/sites-available/archipelago.before-public-guard-1790879144`.
Nginx syntax validation/reload passed. External tests: public IP root and RPC
404 over HTTP and HTTPS (IP HTTPS certificate validation bypassed only for this
negative routing probe); spoofed private Host, X-Forwarded-For and X-Real-IP and
unknown Host POST RPC all404. Tailnet dashboard200; indexer health200 height969475;
relay trusted TLS NIP11 metadata200, WebSocket101, read-only Nostr REQ returned EOSE.
These are live containment results, not fleet/IPv6/reboot/security-audit completion.
Release owner acknowledged security scope in `/tmp/npm-release-handoff-ack.txt`.

User then reported no Angor projects after changing BOTH indexer and relays.
Read-only kind3030 subscription limit5: new relay returned zero events + EOSE;
`wss://relay.angor.io/` returned five events + EOSE. Advised retaining original
Angor relays alongside own relay; a newly hosted relay does not automatically
contain global project metadata. Full app project discovery acceptance remains
required. Also observed own `/api/v1/query/Angor/projects?limit=10` returns404;
reference MempoolIndexerAngorApi.GetProjectsAsync uses this older specialized
route, whereas current deployment docs recommend stock Mempool. Verify actual
client version/discovery path rather than claiming fees/health prove compatibility.

## Shorty live qualification: cached-runtime guard regression — 2026-10-05

The operator signed the final NPM candidate. Release-root verification and exact
reviewed payload comparison pass; signed SHA256
`479f6193835a16dd4ab167e5c22306a878ac39b77c2e2793971e807874fbc0cb`.
This signature authorizes private qualification; it is not release publication.

Shorty baseline public shop/www/indexer/relay trusted HTTPS passes. Its prior
NPM image bytes match the pinned2.14.0 image. Consistent stopped-NPM state,
backend, unit, nginx, helpers and app metadata were backed up under
`/var/lib/archipelago/support/190-npm-20261005`. Migration reached the new private
network/listeners but failed the guard acceptance check and was rolled back.
The test initially expected the emergency guard's legacy variable; further
inspection found a real source defect, not merely that assertion mismatch.

Confirmed cause: `ensure_runtime_assets_ready` applies the management guard,
then `run_runtime_assets` installs the cached OTA's nginx template verbatim.
Shorty's cached template predates the guard. It overwrote protection before a
subsequent nginx reload; restoring the older backend repeated that path. A live
public IPv4 root probe returned200. Immediate containment applied the tested
source guard; HTTP/HTTPS root and HTTP RPC again return404. The cached legacy
runtime template is now also guarded, with its original saved privately, so
that old startup installer cannot remove protection on restart. Both emergency
and current guards are present in the active configuration. Do not claim this
attempt passed or that the broader migration is complete.

Source fix: runtime installation now renders/validates the guarded candidate
before atomic replacement under the nginx transaction lock; syntax/reload
failure restores the previous protected bytes. Rollback protects the restored
runtime template before permitting an older binary to start.11 focused tests
pass; real isolated nginx verifies the actual legacy install, old-binary copy,
invalid-template rollback, public IPv4/IPv6 denial, ACME/private access and the
existing120-case Host/SNI/forwarded-header/UI/assets/RPC/WS matrix.
The first backend suite passed1,681/0failed/4ignored before the final rollback
addition; final rerun and optimized build are required. Logs:
`/tmp/archy-190-guard-runtime-final-unit.log`,
`/tmp/archy-190-guard-runtime-final-network.log`,
`/tmp/archy-190-shorty-activation.log` (failed attempt),
`/tmp/archy-190-shorty-prepare.log`.

Only NPM's container restarted; Bitcoin, LND, ElectrumX, Angor indexer and relay
IDs/start times are unchanged. Restored shop/www/indexer/relay HTTPS returns200.
Shorty's old backend remains active under containment. Rebuild and requalify the
migration, external security and restart persistence before closing this gate.
The signed catalog contents are unchanged and need no further operator signature.

### NPM multi-domain TLS regression found by public acceptance

After the private-listener migration, local first requests passed, but public
Angor health intermittently returned502. Host nginx recorded upstream certificate
hostname mismatches when different public domains used the same NPM TLS listener.
The generated bridge inherited upstream TLS session reuse. This matches nginx's
[documented cross-SNI session-cache behaviour](https://trac.nginx.org/nginx/ticket/1340).

The bridge now explicitly sets `proxy_ssl_session_reuse off` while retaining
SNI, hostname/chain verification and the existing trusted certificates. A real
NPM fixture with two distinct certificates reproduces failure with the old
configuration on the second hostname; the fixed fixture passes40 alternating
trusted TLS requests plus ACLs, WSS, certificate replacement, restart, failed-bind
rollback and disable/delete propagation.24 Python NPM regressions pass.
`/tmp/archy-190-npm-multicert-before.log` is the expected failing reproduction;
`/tmp/archy-190-npm-multicert-integration.log` is the fixed flat-layout pass.
Nested-layout issuance/renewal qualification is running separately.

The exact helper correction is temporarily installed on Shorty and transactional
sync succeeds.40 mixed local TLS requests across four hostnames pass. Public
read-only Angor browser acceptance now passes TLS, WSS, funding/event commitment,
Explore discovery of the known fixture and full project details/statistics:
`/tmp/archy-190-shorty-migrated-angor-browser-2.log`. This remains one known fixture,
not all35-project recovery.32 external IPv4 management-denial checks and tailnet
access pass;10 HTTP/HTTPS ACME routes return the exact probe written in NPM's data
mount. Six NPM database tables and42 certificate/renewal files exactly match the
pre-migration backup (`/tmp/archy-190-shorty-state-preservation.log`).

The previously running optimized build was stopped because it predates this
embedded-helper correction. A complete new build/deployment remains mandatory.
Shorty currently has the prior A5 candidate backend plus protected runtime nginx
and this qualified helper; an A5 restart can reinstall its older helper. Do not
claim final persistence until the new binary is deployed and restart is retested.
No certificate verification was disabled for a public application or upstream.
Raw-IP/unknown-SNI negative routing probes alone bypass hostname matching.
