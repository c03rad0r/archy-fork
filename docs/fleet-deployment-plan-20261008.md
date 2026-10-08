# Fleet guarded delivery plan and read-only dev preflight

Status: **prepared, not deployed**. No service was restarted and no live UI,
backend, catalog, wallet, session, package download or application data was changed.
The Framework startup incident remains separately closed with operator acceptance;
the later paid-file/release checklist remains open. Artwork remains deferred.

## Verified artifact and dev checkpoint

Artifact directory:
`~/.local/state/archipelago/release-qualification/fleet-final-ui-20261007/`.

- Archive SHA256: `063752912fdcae2d1abf2abaccd53bfad39acce23c9a5d896ce52ba1e1996080`
- Candidate index SHA256: `2469e5f2ea2f16598982174e6a0944b3bddadc2e0e587e2f6b3a4253366f0078`
- All **313 regular archive members** were streamed and checked against the
  qualified dist manifest. The archive has a `dist/` prefix, unlike the earlier
  flat deployment archive. No extraction into the live web root occurred.
- Qualification remains full baseline **1,459/177** plus exact three-file delta
  **31/5** and app typecheck, frozen production build, and narrow source-browser
  layout evidence. No new tests or artwork acceptance are inferred here.

Read-only dev snapshots at 2026-10-08 07:29:25 and 07:30:15 UTC established:

- Host: `archi-dev-box` (192.168.63.240), not Framework.
- Served and on-disk UI index:
  `92050cbda69954f57f4b0bdd73d89623f6aae9fc66e4c074ee4f46bdcb9c3b2a`.
- Installed **and running-process** backend:
  `7b85bb0135743200620963ae836867175283d57b6a8534fee2a19b72c3ce77c3`.
  This is not the corrected Fleet backend. Delivery is therefore blocked.
- Management health returned 200; 32 containers and 189 guarded paths were
  inventoried. Both snapshots had identical UI/backend hashes, preservation
  values, container IDs/start times/status and manager identity.
- All paths shared with the prior delivery preflight were unchanged. Eleven
  additional guards cover node identity/installed-state/missing-path presence;
  these are expanded coverage, not eleven content changes.
- Since the prior delivery preflight, eight containers independently changed
  IDs/start times: angor-indexer, botfights, strfry, archipelago-source,
  angor-relay, mempool, filebrowser and gashboard. Do not reuse that old
  preflight. The 50-second current stability observation does not guarantee
  future deployment stability.

Private detailed receipts contain hashes and inventories, not credential values:
`/tmp/archy-fleet-dev-preflight-20261008.json` and
`/tmp/archy-fleet-dev-preflight-20261008-stability.json` (mode 0600).
Their `*-summary.json` files contain the concise results. These are observations,
not deploy-ready backend qualification receipts.

## Required backend integration gates

Before the UI can be delivered, the backend owner must qualify and deploy the
exact corrected binary. The UI helper never installs or restarts a backend.

1. Run the current combined suite through `scripts/test-backend-isolated.sh`;
   retain zero failures, explicit ignores and immutable source input hashes.
   The source must include the reviewed collector provenance patch and all later
   integrated backend fixes. Required Fleet tests are:
   `unsigned_and_legacy_reports_cannot_assign_their_own_trust`,
   `collector_cannot_claim_another_record_identity_or_malformed_id`,
   `collector_history_suffix_cannot_overwrite_another_nodes_history`, and
   `fleet_reads_do_not_promote_old_collector_spoofs_or_history`.
2. Build from the same verified input manifest. Record the exact binary SHA256,
   source commit, test/build manifest hashes, result and embedded-helper hashes.
   A version string, a VM fixture binary, or an earlier passing suite is not proof
   of the final binary. Preserve required live ingress/guard/helper behavior.
3. Use the separately reviewed backend deployment procedure and its protected
   backup/recovery arrangements. Establish lifecycle quiescence and preserve
   node/wallet identity, wallet/channel data, catalogs, sessions, stopped and
   uninstalled intent, application persistence and container identities. This
   document does not authorize a restart while an update/restore is active.
4. After backend delivery, verify installed binary and `/proc/<MainPID>/exe`
   both match the qualified binary; verify expected embedded helper bytes,
   health, management ingress/auth and preservation receipts.
5. With the existing owner session, make only read-only Fleet calls. Verify
   trusted federation records have server-owned federation/trusted/strict-true
   identity fields; collectors are collector/unverified/false; known federation
   identities cannot be shadowed by collectors. For a known federation identity,
   history must report `history_available=false` and empty entries; collector
   alerts retain unverified provenance. Absence of suitable live records is not
   a successful spoofing test: use isolated handler evidence, not live forged
   ingestion. Do not publish or ingest synthetic telemetry on personal nodes.

The backend owner supplies a private `backend-ready.json` derived from those
actual receipts. The prepared UI guard requires `host`, `isolated_failed=0`,
`isolated_passed>2006`, `production_build_passed`, `source_inputs_unchanged`,
matching `test_source_manifest_sha256` and `build_source_manifest_sha256`, the
four names in `fleet_provenance_tests_passed`, `contains_collector_provenance`,
`fleet_contract_readonly_checks_passed`, `deployment_preservation_passed`,
`embedded_helpers_match`, and matching `binary_sha256`/`running_sha256`.
Do not manufacture these fields from intentions or waive a missing gate. The
current old live binary is explicitly rejected.

## Prepared commands and transaction boundaries

Tools are isolated in `scripts/qualification/` on the Fleet acceptance branch:

- `fleet-ui-preflight.py`: read-only artifact and host verification; creates a
  new private evidence file and refuses to overwrite prior evidence.
- `fleet-ui-guard.py`: prepared UI-only mutation/rollback helper, **not executed**.
  It derives from the earlier preservation helper but uses the final Fleet
  receipt, `dist/` prefix, current manager/running-binary checks and expanded
  identity/installed-state guards. It does not restart any service.
- `test-fleet-ui-guard-refusal.py`: one test with five synthetic rejection cases
  passed; failed tests, mismatched source manifests, absent provenance tests,
  missing read-only contract acceptance, or helper mismatch stop before any
  mutation. Both tools also passed Python syntax checks. This is not live
  deployment or successful rollback execution evidence.

Run a **fresh preflight after qualified backend acceptance**, not either receipt
above. Example commands from the reviewed worktree, using a new timestamped path:

```sh
python3 scripts/qualification/fleet-ui-preflight.py \
  --host archi-dev-box \
  --artifact /home/archipelago/.local/state/archipelago/release-qualification/fleet-final-ui-20261007 \
  --out /tmp/fleet-dev-post-backend-preflight-UNIQUE.json
```

Only after all backend gates and review of that new preflight, the prepared UI
command is:

```sh
python3 scripts/qualification/fleet-ui-guard.py deploy archi-dev-box \
  /tmp/fleet-dev-post-backend-preflight-UNIQUE.json \
  /home/archipelago/.local/state/archipelago/release-qualification/fleet-final-ui-20261007/qualification-final.json \
  /home/archipelago/.local/state/archipelago/release-qualification/fleet-final-ui-20261007/qualified-ui.tar.gz \
  /path/to/verified/backend-ready.json
```

The guard rechecks every relevant byte and manager/container snapshot before
writes, creates a protected `/var/lib/archipelago/support/fleet-ui-.../` backup
and rollback script, copies assets, and atomically replaces entry points last.
It excludes packages, AIUI and node-specific catalogs. It then verifies health,
served index, backend bytes/running process, identities, intent, packages, AIUI
and container IDs/start times/status immediately and after 15 seconds. Any
independent drift fails the transaction; do not weaken those checks to force it
through. Its automatic rollback restores prior UI bytes and verifies the old
index hash; it cannot undo independent application or backend changes.

After success, verify the actual deployed UI in a disposable browser context at
390px/1440px with narrowly mocked Fleet failure/provenance cases and payment/
signing blocked. Keep actual backend authorization checks distinct from mocks.
Verify APK/default-download hashes remain unchanged. Repeat fresh backend and
UI gates separately on Yaya; no stale dev receipt is transferable to Yaya.

## Rollback ordering and cached clients

The UI helper emits the exact protected `rollback.sh` path. Running
`sudo -n /var/lib/archipelago/support/fleet-ui-<receipt-id>/rollback.sh`
restores UI bytes only. Recheck the previous served index hash, all preservation
values and health afterward; do not report rollback success solely from an exit
code. New unused fingerprinted assets may remain; old entry points are restored.

If UI delivery fails, retain the qualified corrected backend while restoring the
old UI. A backend failure before new UI exposure can use its separate reviewed
recovery procedure. **After any new UI has been served, blindly downgrading to
an old backend without the provenance correction is unsafe:** active browser or
PWA clients can retain new code even after old UI files are restored. Such a
backend rollback needs a provenance-preserving qualified rollback binary or a
separately reviewed recovery plan; restoring old index bytes alone is insufficient.
