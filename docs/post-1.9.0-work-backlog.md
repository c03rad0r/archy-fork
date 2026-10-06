# Work requested after 1.9.0-alpha

Status: implementation and qualification in progress. 1.9.0-alpha was published
separately; these follow-ups are not in its immutable artifacts. The list below
remains the complete acceptance scope, not a claim that every item is finished.

Current evidence is recorded in [Fleet metrics](fleet-metrics-followup.md),
[peering reliability](peering-reliability-followup.md), and the
[IndeeHub design review](indeehub-distribution-current-design.md). Monitoring and
the tested signer/dashboard candidate are deployed to dev and Yaya with rollback
backups; actual IndeeHub image publication is deferred until the end as requested.
Framework's authenticated Monitoring check awaits an operator dashboard login.
Streaming, storage-source integrations, AIUI setup, V4V packaging/player,
companion hardware checks and the full Fleet acceptance matrix remain open.

Current implementation and qualification evidence: [2026-10-06 checkpoint](post-1.9.0-progress-20261006.md). The scope below remains authoritative.

## 1. Distributed IndeeHub publishing and paid viewing

- Recover and reconcile the earlier design in
  [the streaming plan](phase4-streaming-ecash-plan.md) and
  [the distribution design](dht-distribution-design.md) against current source.
  Their implementation/status statements are historical, not fresh evidence.
- Review current primary Nostr, Cashu and Bitcoin/Lightning specifications before
  selecting the protocol. Distinguish interoperable standards from custom events.
- Publishing through one instance's Backstage must make the content discoverable
  through other instances' **Archipelago** content source. This source is intended
  to become the default eventually; do not change the default without qualification.
- Reuse supported node-to-node discovery/transports and the file-payment method
  negotiation: show methods the recipient actually accepts. Include Cashu and
  Lightning to the automatically provisioned ecash Lightning address from first
  use; do not require a producer to operate LND to receive initial payments.
- Pay the producer's wallet, verify settlement before granting access, and make
  payment retries/delivery recovery idempotent. Keep producer revenue separate
  from any optional hosting/relay bandwidth charges in the older plan.
- Define timed viewing entitlements, start/expiry semantics, reconnect, resume,
  seek, device/session scope and clock/error handling. Evaluate encrypted media
  and authorized key delivery without claiming that delivered video or keys can
  be made impossible to copy or revoked retroactively.
- Use the operator's video uploaded to **Yaya Cloud**, publishing through Backstage
  for the demo. Identify the exact file and preserve the source; do not select an
  unrelated personal video or publish other Cloud contents. Payment-test amounts
  need explicit bounded authorization before real funds are spent.
- Deliver a coherent demo: publish on one node, discover on another, select a
  supported payment method, pay producer, watch, resume without repayment, and
  enforce expiry. Cover publisher outage, duplicate events, wrong mint, failed/
  delayed payment, restart and lost responses. Preserve privacy and access rules.
- Treat this as the foundation for future fully featured node-sharing apps,
  introduced and qualified individually.

## 2. IndeeHub native Nostr signer and companion reliability

- Reproduce intermittent native-signer login failure and companion grey screen
  requiring refresh/re-login. Inspect both browser and actual Android WebView.
- Cover iframe origins, signing permissions, redirects, callback/session state,
  background/resume, expired sessions, refresh and cancellation without weakening
  authentication or exporting signing keys. Preserve useful errors and recovery.
- Consult existing signer bridge documentation and prior origin/reload fixes;
  do not assume those earlier fixes address this fresh report.

## 3. Node peering and discovery

- Diagnose Yaya ↔ Archy dev: requests appear approved/pending but neither node
  appears in the other's peers; the flow is also slow.
- Trace request, delivery, approval, identity, persistence and both-node peer-list
  reconciliation. Test restart, retry/duplicates, offline recovery and reciprocal
  visibility; distinguish requested, approved, connecting and connected states.
- Show Nostr requests in the appropriate discovery/request UI.
- Rename **Find Nodes** to **Connect with Nodes**, consistently with accessibility,
  navigation and translation conventions.

## 4. Framework Monitoring

- Investigate the reported non-working Monitoring screen on Framework with
  read-only diagnostics first. Verify actual metrics, loading/error states,
  permissions and refresh/reconnect on that node. Preserve wallet/radio state.

## 5. Immich / Nextcloud files in Cloud

- Assess integration so installed Immich/Nextcloud files appear under the correct
  Cloud Files categories, without duplicating storage or exposing another user's
  private data. Determine supported APIs, user identity/permissions, thumbnails,
  originals, virtual paths and large-library pagination/indexing.
- Define view/download/edit/delete semantics per source. Preserve application
  ownership, databases, metadata and trash/versioning; do not directly mutate
  application-managed storage to bypass its API.
- Test installation/removal, permissions, unavailable apps, overlapping filenames,
  duplicate detection and category accuracy before enabling an integration.

## 6. Web5 header and node connection flow planning

- Inspect the top-bar **Wallet** label in Web5. The operator requests removing
  this cosmetic label; preserve any actual wallet navigation or accessibility
  function until its role is established, and report if it is more than a label.
- Plan a less hidden, clearer discovery and peering journey on mobile and desktop
  using the existing design system, visual styles and components. This is a flow
  and information-placement change, not a visual redesign.
- Include visible entry to **Connect with Nodes**, incoming/outgoing Nostr
  requests, approval, pending/connecting/connected status, reciprocal peer
  confirmation, offline/retry recovery, and clear return paths. Show how this
  relates to existing peers and node details rather than adding duplicate flows.
- Produce reviewable mobile/desktop flow plans before broad navigation changes;
  cover first connection and repeat use, touch/keyboard access, empty/error
  states, and the current Yaya/dev approval bug. Keep diagnostic implementation
  details out of normal user-facing steps.

## 7. Companion app launch latency

- Reproduce intermittent long app-opening delays on the actual companion and
  compare desktop/mobile browser timing for the same node/app. Measure discovery,
  readiness polling, authentication/signing, route/proxy connection, WebView
  creation and first useful rendered content separately.
- Remove avoidable waits, duplicated checks and retry loops without launching
  before an app can accept connections. Cover cold/warm launches, switching apps,
  background/resume, flaky connectivity, expired authentication and app restarts.
- Keep feedback clear and immediate, with bounded cancellation/retry and preserved
  navigation. Record before/after measurements and test actual Android devices.

## 8. Fleet comprehensive acceptance

- Inventory every current Fleet capability and turn it into a test matrix rather
  than assuming a working overview proves all functions work.
- Cover discovery, identity/deduplication, authorization, adding/removing nodes,
  status/metrics freshness, selections/filters, remote actions, update discovery
  and progress/results, reconnect/offline/restart recovery and error handling.
- Include mixed software versions, slow/unreachable nodes, partial success,
  duplicate/late replies, permission rejection and desktop/mobile behavior.
- Use disposable fixtures for destructive/restart/update scenarios; preserve real
  node wallets, application data and user choices. No new spending is authorized.

## 9. AIUI first-use/provider/funding experience

- When AIUI opens without a usable AI connection, guide the user to setup rather
  than presenting only a failed model response. Distinguish missing configuration,
  invalid credentials, insufficient Routstr funds and temporary provider outage.
- Offer concise choices to add a Claude credential, configure the supported
  OpenAI/Codex connection, or fund Routstr. Verify the actual provider/runtime
  authentication methods before labeling a credential field or promising support.
- Chat may present action buttons; use a private credential form/modal for keys,
  not an ordinary chat message. Keep credentials out of model prompts, history,
  logs and screenshots; use existing secure storage and permission boundaries.
- Open Routstr funding as a coherent modal where supported, show balance/payment
  state and update availability after funding. Preserve any unsent prompt and
  let the user continue when setup succeeds. No surprise automatic charges.
- Match the existing design system and support small screens, keyboard access,
  cancellation, invalid/expired keys, funding delay, reload/background/resume and
  successful first response. Test actual companion and desktop flows.

## 10. Node availability, offline duration and network map

- Make availability easy to scan in Fleet and Connected Nodes using explicit
  status text as well as existing visual indicators. Show last successful contact
  and an elapsed duration, such as "Last seen 2 hours ago"; only say "Offline for"
  when an observed offline transition supports that claim. Never equate an old
  relay advertisement, failed monitoring query or unknown status with proof that
  a node has been continuously offline.
- Place confirmed offline nodes after online nodes by default, with stable order
  within each group. Specify how connecting and unknown/stale nodes are ordered;
  preserve user-selected sorting, filters, selection and scroll position.
- Show offline and unknown/stale nodes distinctly on the network map, with text
  or accessible detail including last contact. Do not present retained historical
  links as live connections or silently drop long-offline nodes from the map.
- Retain useful last-contact history across refresh/restart; reconcile timestamps
  across discovery, peering and monitoring. Handle clock skew, never-seen nodes,
  stale cached data, long outages and reconnects without false precision.
- Test short/long outages, stale relay events, monitor-only failures, network
  partitions, app background/resume and status recovery on desktop and companion.
  Use the existing design system and avoid color-only status communication.

## 11. Web5 connection/sync speed and responsive navigation

- Profile and improve connection establishment, discovery, peer synchronization
  and Web5 data loading. Measure each stage and reduce avoidable serial waits,
  duplicate requests, excessive timeouts and retry storms. Preserve identity,
  trust verification and accurate readiness/status reporting.
- Reproduce delayed taps/navigation for Web5 and Cloud, especially in the Android
  companion, and the Find Nodes / nodes entry into **Federation & Peers**. Record
  tap-to-feedback, route-render and useful-data timings separately on cold/warm
  loads and with realistic slow/offline nodes and larger peer/file lists.
- Every navigation tap must give immediate feedback using existing components;
  render the destination shell promptly and load independent data incrementally.
  One slow node/request must not block the whole screen or tab navigation.
- Keep cached content visibly identified when stale; use bounded loading states
  and useful retry/error feedback. Preserve back navigation, scroll, selection
  and in-progress work; cancel or ignore superseded requests safely.
- Cover all tabs, not just these reported routes. Test rapid tab switching,
  repeated taps, background/resume, expired sessions, unavailable peers, partial
  responses and recovery on actual companion hardware and desktop/mobile browsers.
- Record before/after latency distributions and agree measurable budgets after
  establishing a baseline. Faster feedback alone is not evidence that the
  underlying connection/synchronization delay has been fixed.

## 12. Missing Fleet card metrics and secure FIPS transport

- Reproduce missing metrics on multiple Fleet **Nodes** cards; trace collection,
  authorization, transport delivery, identity matching, subscriptions/cache and
  rendering separately. Verify each metric is real, current and associated with
  the correct node; show unavailable/stale explicitly rather than invented zeros.
- Evaluate and use existing FIPS transport wherever supported and measurably
  beneficial across Fleet, discovery, peering and synchronization. Preserve peer
  authentication, access controls, confidentiality and existing trust boundaries;
  transport reachability must never grant permission to read metrics or act.
- Prefer fresh authenticated updates without duplicate polling or excessive
  subscriptions. Measure propagation latency and resource use, including large
  fleets, while retaining safe fallback for unavailable/incompatible FIPS peers.
- Test spoofed/unauthorized peers, expired trust, disconnect/reconnect, partial
  metrics, stale/out-of-order/duplicate messages, mixed transport capability and
  fallback recovery. Do not claim FIPS is faster until timings demonstrate it.

## Required qualification before user acceptance testing

Operator explicitly requires extensive headless testing using the authorized
nodes before deploying the next features for UAT. For each task, qualify source
regressions, actual browser behavior, integration between appropriate nodes,
permissions and failure/recovery paths first. Use existing access and useful
read-only checks on real nodes; use isolated fixtures for destructive scenarios.
Headless browser success does not substitute for actual companion/WebView checks
where lifecycle, native signing, media or navigation behavior is involved.

Record exact revisions/artifacts, nodes, environments, before/after measurements,
executed scenarios, failures and untested boundaries. Retest fixes and relevant
regressions; do not mark gaps passed or claim perfection. UAT handoff must contain
short node-specific steps with expected outcomes, deployment scope and rollback.
Preserve wallets, files, identities and operator app choices throughout.

If IndeeHub changes are needed, include its app update explicitly: build and test
the versioned image, preserve app data and configuration, qualify fresh install,
upgrade/restart and rollback, publish through the signed app catalog, and verify
existing nodes discover and apply the intended version. Distinguish an app-only
release from any backend/OTA dependency; use the documented decoupled app-update
path where supported. Do not silently require a full OTA for an app-only change.

Sequencing clarification: finish1.9.0-alpha first. Publish any required IndeeHub
app update at the end of the follow-up implementation and qualification, not
ahead of that work or as an untested addition to the current release.

## 13. V4V Portainer demo as a Yaya node app

- After the current release, deploy/package the existing V4V Portainer deployment
  as a demo app on Yaya, showcasing how an ordinary third-party app works on a
  node **without native Nostr signer integration**, as explicitly requested.
- Inspect the actual existing Portainer source, branch/image revision, Compose
  stack, access/authentication and data before changes; retain the working V4V
  site, stack and persistent state. Do not confuse this with the public Archipelago
  software demo at demo.archipelago-foundation.org.
- Follow the current app-development guide and supported app packaging/gate/
  lifecycle conventions. Define the app card/icon/category, launch URL/readiness,
  network/auth boundaries, health, configuration and persistent mounts properly.
  Do not expose the native signer or implicitly grant signing permissions.
- Qualify fresh installation in isolation, then Yaya deployment, desktop/mobile/
  companion launch, normal app functionality, restart, update/rollback and safe
  removal behavior. Verify the deployed app actually runs the intended V4V source
  revision, not a stale build or unrelated image.
- Record a short reproducible demo flow and operator UAT checklist. Preserve
  existing Gitea/Portainer connectivity and unrelated apps; no new payment or
  public sharing of private test content is implied by the demo packaging.

### V4V node-only catalog and Sovereign Music promotion

- Source is on the existing Gitea on the146 server; locate the actual V4V repo,
  branch and deployment revision there rather than guessing a replacement source.
- Add a **Sovereign Music** banner for V4V on Yaya, following the existing
  Sovereign Streaming banner treatment and using the app's own login background.
  Retrieve and inspect the real asset; do not invent a replacement illustration.
- Both demo app availability and its promotion must be confined to Yaya. Evaluate
  a signed per-node/DID-scoped demo catalog or existing supported node-specific
  candidate mechanism. Do not publish the demo to the global catalog or let
  unrelated nodes install it implicitly through a shared catalog cache.
- Test matching/nonmatching identities, copying URLs/catalogs between nodes,
  missing identity, refresh/restart/update and promotion visibility. Catalog
  selection or visibility must not bypass normal artifact-signature enforcement.
- This may become a real app later; preserve an explicit tested promotion path
  from node-only demo to proper public app release without duplicate app IDs,
  conflicting state, lost configuration or automatic exposure before approval.

### V4V persistent playback and existing demo music catalog

- Use the demo song catalog from the existing V4V Portainer demo in the Gitea
  repository identified above. Inspect its actual catalog configuration and media
  sources; preserve artist attribution and payment destinations. Do not substitute
  an invented catalog or copy credentials into public app configuration.
- Integrate V4V with the local player bar: leaving or closing the app view must
  retain playback, expose track information and playback controls, and offer a
  button to reopen V4V at the same track and position without restarting playback.
  Explicit stop remains available. This concerns closing the app view; operating
  system termination/background restrictions require separate qualification.
- Inspect supported app/player integration before choosing an implementation.
  Maintain one playback session, prevent duplicate audio on reopen, authenticate
  app-to-shell messages, and preserve the requirement of no native Nostr signer.
- Test navigation, repeated close/reopen, pause/resume/seek, track changes,
  disconnect/recovery and unavailable media on desktop and the actual companion.
  Check track, queue and position continuity, accessible compact controls and
  mobile background behavior. Record platform limitations rather than promising
  playback after an operating system terminates the app.
- Keep catalog and player integration within the Yaya-only demo scope until
  separately approved for general release.

## 14. FIPS media transport requirement

- Operator explicitly requires FIPS for streaming peer files and distributed
  IndeeHub media. Verify the actual node-to-node media-byte path uses FIPS, not
  merely discovery, metrics, payment negotiation or a connection-status label.
- Reconcile the earlier multi-transport/swarm design with this requirement.
  Preserve normal authenticated browser/companion playback interfaces while
  carrying the inter-node stream over authenticated, authorized FIPS connections.
- Cover progressive/range requests, HLS segments as applicable, seek, pause/resume,
  reconnect, producer/cache-peer outage, backpressure, bandwidth and resource use.
  Preserve encrypted-content and timed-entitlement/payment enforcement end-to-end.
- Define and expose behavior when no usable FIPS route exists. Do not silently
  call another media transport FIPS or mark this requirement complete while the
  actual stream continues over an unrelated fallback. Any fallback policy must
  be explicit and reconciled with the operator's all-streaming requirement.
- Instrument transport selection and verify it in headless multi-node integration
  tests and actual mobile/desktop playback. Keep technical diagnostics available
  without burdening normal playback with implementation details.
