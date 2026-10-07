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
