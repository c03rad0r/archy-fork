# Post-1.9.0 reliability investigation

Status: implementation and qualification in progress. These changes are on
`work/post-190-reliability`, separate from the published 1.9.0-alpha artifacts.
Nothing here constitutes live acceptance or permission to change peer trust.

## Peering: approved request does not establish a relationship

Read-only inspection on 2026-10-05 reproduced the operator's report: the receiving
node retains an approved inbound request while the requesting node retains its
outbound request in Sent state; neither has the reciprocal federation entry.
Both have discoverability enabled. FIPS is running on both (different service
names); checking only `fips.service` would incorrectly report one as inactive.

Source discrepancy: request publication and polling use `handshake_relays()`,
which combines managed relays and configured defaults. Approval, rejection and
cancellation instead use only configuration defaults. Align all three reply
paths with the shared resolver. Add an actual-handler integration test with a
UI-configured local WebSocket relay, signed event verification, recipient-only
NIP-44 decryption, reply types, persisted request states and Observer-only approval.
The isolated test passes: all three reply types reach the managed-only relay,
and a rejected approval remains Pending with no peer added. The complete backend
suite is running; actual-node handshake recovery remains outstanding.

This discrepancy is not yet a proven complete explanation of the live failure.
Read-only relay queries are being checked. Other source risks requiring separate
qualification include best-effort peer-joined callbacks without durable retry,
five-minute background polling, latest-50-event fetching without a cursor, and
pending-file read/modify/write operations without serialization. Do not manually
mark requests completed or elevate trust to make the UI appear connected.

## Web5 navigation cleanup

The Wallet quick-action only toggles a local disconnected flag and refreshes LND
information. Remove it and its independent LND polling; wallet interfaces remain
elsewhere. Rename Find Nodes to Connect with Nodes, including English/Spanish
translations and existing navigation assertions. Four focused component tests
pass. Full frontend tests, type checking and mobile/desktop rendering checks are
still required; no deployment or operator acceptance is claimed.

## Fleet findings to investigate

`normalizeFleetNode` substitutes zero for absent metrics, and timestamp age alone
is interpreted as online/offline. Missing values must not be presented as real
zero measurements, and stale reporting must not be treated as a measured offline
transition. Trace telemetry collection and transport before changing presentation.

## V4V source and demo catalog located

The existing Gitea repository is `v4v/v4v`, branch `demo-portainer`, at
`3ae171d6b0c728665a860520fe393c0abb772798`. The earlier `lfg2025/v4v`
location does not resolve on that server. Read-only inspection of the actual
Portainer documentation confirms a sanitized demo catalog with 52 entries and
47 playable tracks: 21 bundled demo WAVs and 26 publisher-hosted entries. These
counts describe the documented seed, not a fresh playback acceptance result.

Its importer is designed to back up existing state, merge by ID and retain
accounts, payment records and edits. Qualify those behaviors before deployment.
The source explicitly keeps private catalog/media archives out of public source
publication. Preserve that boundary when packaging the Yaya-only demo: hiding a
card is not sufficient protection for private media or credentials. Inspect the
existing node's state and exact deployed revision before modifying its stack.

## Qualification results so far

Frontend type checking passes. Full suite: 1,221 passed, one unchanged paid-file
case hit its 20-second test timeout under concurrent build/upload load. That
file's six tests all passed when rerun alone. Retain the original failure; do not
rewrite it as an entirely green full-suite run. Four targeted navigation tests
passed separately. Mobile/desktop browser acceptance remains outstanding.

The first new Rust test compile exposed two fixture-only String/&str mismatches.
Corrected them and added rejected-relay coverage: failed delivery must leave the
request Pending and must not add a peer. Isolated compilation/execution now passes (one integration test covers four
scenarios). No live peering repair or deployment is claimed.

Public relay queries found no matching reply on the managed relays that completed
the query; some endpoints were unavailable. The default Damus relay requires
authentication for this filter, so its unauthenticated rejection is not evidence
of a missing event. The installed SDK already enables automatic authentication.
Do not infer that the node has the same rejection without authenticated evidence.

### Completed source-test runs

The second full frontend run, limited to two workers, passes all1,222 tests across
150 files. Type checking passes. The first full backend run exposed a real
pre-existing bug: encrypted chat/contact nonces starting with `{` or `[` were
misclassified as legacy JSON. This is not dismissed as a flaky test.

Replace prefix-only classification with full JSON object/array validation using
`IgnoredAny` to avoid building another object tree. Deterministic valid encrypted
fixtures cover both prefixes; existing pre-migration ciphertext compatibility
and tamper/wrong-key tests remain. Full isolated backend rerun passes1,683 tests,
zero failures, four existing ignored. Logs are retained in
`/tmp/archy-followup-backend-suite-2.log` and
`/tmp/archy-followup-ui-suite-2.log`.

Published1.9.0 artifacts remain unchanged, and their release page discloses this
newly discovered issue. Private snapshots were attempted on all four authorized
nodes before further restarts: only Shorty had an affected message store at the
expected paths; its bytes were verified after backup. No message contents or
wallet data were exported. Actual candidate build/deployment and store reload
acceptance still remain; do not equate source-test success with delivery.
