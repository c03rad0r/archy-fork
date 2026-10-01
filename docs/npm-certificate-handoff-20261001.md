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
