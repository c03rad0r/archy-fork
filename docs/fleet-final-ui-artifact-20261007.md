# Fleet final UI artifact qualification — 7 October 2026

Status: qualified UI artifact archived; **not deployed**. Corrected Fleet backend
qualification and coordinated backend/UI delivery remain required. This receipt
does not authorize an OTA/ISO or replace actual-node Fleet transport acceptance.

## Source and test boundaries

The full combined UI suite passed **1,459 tests in 177 files**, zero failures,
from a separate frozen copy of all **771 tracked UI inputs**. Input hashes were
verified before and after the run. One worker, 1 GB Node heap bound. Baseline:
`~/.local/state/archipelago/release-qualification/fleet-combined-ui-20261007/`.
Its `provenance.json`, `inputs.json`, `test-results.json`, `test.log` and
`test-receipt.json` preserve the exact source and result.

A later reviewed alert/cache provenance correction was integrated as `ff522086`.
Exactly three UI files differ from that full-suite baseline:
`FleetAlerts.vue`, `useFleetData.ts`, and `FleetPartialFailure.test.ts`.
The final snapshot gate verified that all three match the qualified isolated
source hashes and that no other UI inputs changed. This delta passed **31 focused
Fleet tests in five files** and the exact app-project typecheck
(`vue-tsc --noEmit -p tsconfig.app.json`, 2 GB heap, exit 0).
This is full baseline plus focused delta qualification, **not a full-suite rerun
after the final delta**.

Alert rows label unsigned collector reports explicitly. Cached node/alert identity
claims remain unverified until a fresh corrected server response; cached content
and metrics remain visible. These labels require the corrected backend provenance
gate and do not themselves authenticate identities.

## Frozen build and artifact

Final directory:
`~/.local/state/archipelago/release-qualification/fleet-final-ui-20261007/`.
The final 771-input frozen source passed the production Vite/PWA build with a
1 GB heap bound. Input hashes remained unchanged before and after build and the
subsequent narrow browser fixture. Installed dependencies were reused; no install
or catalog-copy prebuild ran. The tracked public catalog remained frozen.

- Index SHA256: `2469e5f2ea2f16598982174e6a0944b3bddadc2e0e587e2f6b3a4253366f0078`
- Archive: `qualified-ui.tar.gz`
- Archive SHA256: `063752912fdcae2d1abf2abaccd53bfad39acce23c9a5d896ce52ba1e1996080`
- Receipts: `qualification-ready.json`, `build-receipt.json`, `qualification-final.json`
- Manifests: `inputs.json`, `dist-inputs.json`

An existing isolated source-component browser harness was pointed at the exact
frozen final source and styles. At **390px and 1440px**, the collector alert header
remained contained, its unverified provenance label was visible, and there were
no page errors or horizontal overflow. All RPC responses were synthetic and all
non-loopback requests were blocked. This narrowly verifies final source layout;
it is **not a production-dist browser run**, a live authorization check, or proof
of distributed transport behavior. Logs and fixture scripts are archived beside
the artifact. Owned browser, server and compiler processes were stopped.

## Explicit remaining gates

No services, wallets, payments, catalogs, APKs, default downloads, or live UI
files were changed by this qualification. No deployment occurred. Fleet backend
tests and coupled delivery remain pending; real-node mixed-version, trust,
partition/reconnect and supported-action acceptance remain separate.

Companion background playback remains operator-accepted. Notification artwork
still failed the operator's phone check and is explicitly deferred to task 23 at
the end of the remaining work. Existing artwork source is included in the combined
snapshot; no new artwork investigation or acceptance claim was made.
