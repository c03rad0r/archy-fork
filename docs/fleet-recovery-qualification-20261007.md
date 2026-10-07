# Fleet recovery qualification — 7 October 2026

Status: source regressions and isolated browser checks qualified; deployed-browser and distributed
transport acceptance remain open. This supplements tasks 8, 10 and 12 without
claiming that the full Fleet acceptance matrix is complete.

## Findings and changes

A failed status poll only surfaced an error during the initial load. Once reports
were cached, network failures silently left the previous view in place. The view
now keeps those reports, selection and sort order, with an explicit failed-refresh
notice and Retry. An initial failure without reports remains an unavailable state.
A successful refresh clears the notice. Report timestamps continue ageing; a
monitoring failure is not relabelled as proof that a peer is offline.

Concurrent refreshes could overwrite newer status/alerts with older replies.
Status, alerts and refresh completion now track request generations. Replies
arriving after unmount cannot repopulate the session cache or change the view.
Malformed status envelopes cannot advance the successful-refresh timestamp.

## Evidence

Focused Vitest run: **21 tests passed in four files**, zero failures, including
actual Fleet view rendering with retained cards, outage notice and successful
Retry. Other cases cover a missing cache, late status/alert replies, a malformed
status response, late responses after unmount, real zero versus unavailable
metrics, report ageing, sorting and selected-node history isolation.

Logs: `/tmp/archy-fleet-recovery-tests-final.log` and
`/tmp/archy-fleet-recovery-typecheck.log` (Vue typecheck passed).

An isolated real Chromium harness rendered the actual Fleet component and styles
at 390px and 1440px. Cached cards and failure notice remained visible; keyboard
focus/Enter on Retry recovered the view; genuine zero remained 0%; no horizontal
overflow or page errors occurred. RPC was replaced with synthetic replies; all
non-fixture network requests were blocked. This was source-served component
qualification, not a deployed artifact or real outage. Initial harness attempts
failed because its standalone app initialization/import was incomplete; corrected
before both widths passed. Failed logs retained.

Browser scenario: `/tmp/archy-fleet-recovery-browser.cjs`,
server: `/tmp/archy-fleet-browser-server.mjs`,
result: `/tmp/archy-fleet-recovery-browser.log`. The owned server was stopped.
This is local component/composable evidence, not a live-node outage test.
No node service, peer relationship, wallet or payment was changed.

Combined frontend regression suite subsequently passed **1,448 tests in 176 files**,
zero failures, using one worker. Log:`/tmp/archy-combined-ui-20261007-full.log`.
Input hashes:`/tmp/archy-combined-ui-20261007-inputs.json`.

Production build passed from a separate frozen copy of all 701 qualified inputs;
all hashes were verified again after build. Real Chromium loaded this production
dist at 390px and1440px, with synthetic Fleet outage/recovery replies: retained
reports, keyboard Retry, genuine zero, layout and error checks passed. Payment
and signing were blocked. This is artifact qualification, not a real outage.

Durable artifact and evidence:
`~/.local/state/archipelago/release-qualification/combined-ui-20261007/`.
Archive SHA256:`16523c80ecebbb4ba1f6d289ead81ab86d59daf67093ce1d8d68d5bce3ffaae5`.
Index SHA256:`65e5a09eb736f73650ef5ac52bdebf66d1c877e69511edd7bd3095847f6734de`.

The initial dev deployment preservation gate refused before any writes because
three container start times changed independently after preflight. Failed log
retained at `/tmp/archy-combined-ui-dev-deploy.log`; actual deployed-node
acceptance remains separate until the retry and browser checks pass.

## Remaining acceptance matrix

| Area | Established evidence | Remaining |
| --- | --- | --- |
| Report rendering | Local normalization/render tests; earlier dev/Yaya metrics evidence in progress ledger | Updated sender/receiver comparison across supported versions |
| Status freshness | Clock-driven ageing, unknown versus stale, no synthetic metric zero | Real partition, reconnect and transport delivery |
| Refresh failures | Visible retained reports; empty failure; Retry recovery | Deployed artifact and actual-node outage; isolated 390/1440 browser passed |
| Concurrent replies | Newest status/alerts win; no unmount cache repopulation | Distributed delay/out-of-order delivery |
| Selection/sorting | Retained across failure; history request isolation | Larger-fleet navigation and keyboard checks |
| Authorization | No permission changes in this patch | Unauthorized/expired-trust actual-handler matrix |
| Remote actions/updates | Outside this patch | Disposable-node success/partial failure/restart/update matrix |
| FIPS | Existing preferred authenticated transport unchanged | Measured latency, fallback, reconnect and mixed capabilities |

No current-node restart or deliberate network outage was performed. Live fault
qualification must preserve wallets, installed applications and operator intent.


## Dev deployment attempt and verified rollback

The immediate retry passed preflight but failed the final container-preservation
check because `botfights` was independently replaced during the deployment. The
helper automatically restored the prior UI. A subsequent HTTP fetch verified
index SHA256 `4d9424a4ad7548221a07e2c4f14019029ac5fc4e3f1d27ff5eca72b5ff0f6a56`,
matching the preflight baseline. Do not claim the new UI is deployed.

Read-only systemd and management journal checks show pre-existing health-monitor
restarts of datum, gashboard and angor-relay, and later botfights recovery from a
stuck stopping state. These checks explain the inventory change; they do not
establish why the health checks failed. No live app was stopped by this UI task.
Keep the preservation gate, leave the prior UI in place, and retry only after
qualification conditions are stable. Yaya was not changed.

Logs: `/tmp/archy-combined-ui-dev-deploy-retry.log` and the private preflight
receipt `/tmp/archy-combined-ui-dev-preflight-retry.json`.
