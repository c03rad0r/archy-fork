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

### Task 5 assessment — source and version checks, 7 October

Cloud currently sends every category through File Browser, using `/Photos`,
`/Music`, `/Documents` and `/`. Its client obtains a File Browser token from
`app.filebrowser-token`; that credential is not an Immich or Nextcloud identity.
An installed application therefore cannot safely become a new filesystem mount
or inherit access to all of that application's accounts.

The candidate catalog declares Immich2.7.4 and Nextcloud29. This is catalog
metadata, not verification of each installed node's actual image/version.
Before enabling a connector, probe the running version and supported API.

| Source | Verified interface for the catalog version | Proposed initial behavior |
| --- | --- | --- |
| Immich |2.7.4 `POST /api/search/metadata` accepts page/size and returns `nextPage`; asset metadata, original and thumbnail endpoints require `asset.read`, `asset.download` and `asset.view`; `/api/users/me` requires `user.read` | Explicitly connect the intended user's scoped key; list photographs/videos, stream permitted thumbnails/originals; preserve albums and original asset identifiers |
| Nextcloud | Authenticated WebDAV under `/remote.php/dav/files/{user}/`; PROPFIND exposes stable file ID, MIME, ETag and permissions; GET downloads bytes | Use the user's Login Flow/app-password authorization; navigate folders without copying storage; apply source permissions on every request |

Primary references checked against the catalog versions:
[Immich2.7.4 API schema](https://raw.githubusercontent.com/immich-app/immich/v2.7.4/open-api/immich-openapi-specs.json),
[Nextcloud29 WebDAV](https://docs.nextcloud.com/server/29/developer_manual/client_apis/WebDAV/basic.html),
[Nextcloud29 Login Flow](https://docs.nextcloud.com/server/29/developer_manual/client_apis/LoginFlow/index.html).
The Immich specification SHA256 was
`d6378294dcddcf772ffdefe470da17d62a5503a74fe1bd6a28f921196901d121`.
Current upstream docs can describe newer APIs; do not substitute them silently.

Proposed implementation boundaries (design, not enabled features):

- Add source adapters behind owner-authenticated node endpoints, keeping scoped
  upstream credentials in private node storage. Bind every connection to the
  selected upstream account and installation; never use administrator-wide
  enumeration or expose keys in browser storage, URLs or logs. Resolve only
  installed service endpoints; reject redirected credential forwarding.
- Represent each item by `(source, installation, account, upstream ID)`, with
  display path, MIME, size, revision and operation capabilities. Identical names
  across sources remain separate; a matching name is not proof of duplicate
  bytes or ownership. Display a source label beside integrated category results.
- Start with browse/preview/download and Open in source. Editing, rename, move,
  delete, trash and versions remain source-owned until individually implemented
  and tested through its API; never modify its data directory directly. No public
  or paid sharing is implied by importing a source listing.
- Stream bytes through authenticated bounded endpoints, preserving valid range
  and revision semantics. Recheck authorization for both previews and originals;
  encrypted/unavailable content must not fall through to privileged disk reads.
- Page Immich results and lazily expand Nextcloud folders. A bounded, cancellable
  per-account metadata index can support whole-library category/search results;
  do not claim WebDAV folder enumeration is a global paginated search API.
  Label incomplete indexing and stale results, and invalidate on revocation,
  disconnect, uninstall or source revision changes. Never duplicate original
  file storage merely to populate Cloud.
- Before enablement, qualify two users with disjoint/private/shared libraries,
  expired/revoked credentials, denied preview/download, install/remove/reinstall,
  source outage/recovery, duplicate names, renamed items, large libraries,
  bounded cancellation, MIME classification and account-scoped cache deletion.
  These cases have not been executed; no source connector is accepted yet.

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
  node. Updated operator instruction on6October supersedes the original
  no-native-signer brief: use Nostr sign-in and the native signer, replacing the
  alpha password gate for this demo. Preserve explicit signing consent and
  cryptographic authentication; newly registered users gain no operator powers.
- Inspect the actual existing Portainer source, branch/image revision, Compose
  stack, access/authentication and data before changes; retain the working V4V
  site, stack and persistent state. Do not confuse this with the public Archipelago
  software demo at demo.archipelago-foundation.org.
- Follow the current app-development guide and supported app packaging/gate/
  lifecycle conventions. Define the app card/icon/category, launch URL/readiness,
  network/auth boundaries, health, configuration and persistent mounts properly.
  Integrate the supported native signer without implicitly granting permissions.
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
  Sovereign Streaming banner treatment. Updated request: use the final intro
  cymatic still as the background and a music/play graphic on the right, or adapt
  the app's For You banner treatment. Inspect the actual intro ending and assets.
- Fix the reported real managed-app regression: closing V4V leaves playback
  running without the native bottom player. Qualify against the signed catalog
  and real installed app, not only browser-injected package fixtures.
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

### Private demo distribution correction

The operator authorized removal of the publicly downloadable V4V demo container
on 6 October. Preserve Yaya's working installed demo, its data, and a private
recovery image. V4V application source and images must remain private until the
operator explicitly approves publication. Audience restriction on a signed node
catalog does not make the referenced registry image private. Updated deployment
must use a qualified private delivery path; do not republish the demo image.

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
  app-to-shell messages, and support the updated native Nostr sign-in requirement.
- Test navigation, repeated close/reopen, pause/resume/seek, track changes,
  disconnect/recovery and unavailable media on desktop and the actual companion.
  Check track, queue and position continuity, accessible compact controls and
  mobile background behavior. Record platform limitations rather than promising
  playback after an operating system terminates the app.
- Keep catalog and player integration within the Yaya-only demo scope until
  separately approved for general release.

## Federated Nodes state sync reliability and progress

- Federated Nodes sync currently gives too little feedback and can take a long
  time when one peer is unreachable. Show an accessible spinner, the number of
  eligible nodes, current phase and a clear success/partial-failure result.
- Bound concurrent peer syncs so one slow or dead peer cannot serialize the
  entire federation refresh. Prefer the authenticated FIPS route when
  available, preserve the existing bounded fallback, and keep per-peer errors
  visible without discarding successful results.
- Keep sync state convergent across retries and reconnects; prevent duplicate
  clicks, stale responses or an incomplete refresh from looking like success.
  Qualify healthy, slow, offline, mixed-transport, large-federation and mobile
  layouts before acceptance.

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

## 15. MeshCore public-channel reliability and settings clarity — perform last

Added by the operator on 2026-10-06, explicitly at the end of this backlog.
Framework is Heltec V4; Archi dev is Heltec V3. The operator configured both for
UK in Frequency Plan but reports no messages between them, no other radios
visible, Framework reporting “mesh listener isn't running”, and intermittent
502 Bad Gateway on the mesh page. This request reopens Framework radio
investigation for this scoped task; earlier hardware deferral is not a pass.

- Diagnose actual firmware/protocol, device identity and serial ownership,
  listener lifecycle, channel identity/key and every relevant RF parameter on
  both radios. Matching regional labels alone does not prove matching settings.
  Preserve radio identity and existing settings; do not flash or reset merely
  because discovery is empty. Use the authorized UK plan and verify applicable
  hardware/firmware constraints before transmitting.
- Trace public-channel transmission and reception end to end on both nodes:
  UI acknowledgement, serial command/result, radio delivery and listener event,
  persisted message and rendered conversation. Distinguish sent from received;
  report listener/USB/RF failures accurately. Do not claim no nearby radios means
  broken discovery without a known transmitting peer.
- Investigate the missing-listener error and intermittent 502 separately.
  Cover service readiness, process ownership, competing serial clients, USB
  detach/reconnect, background/resume, stale connections, backend/radio restart,
  bounded retries and recovery without duplicate listeners/messages.
- Compare current official MeshCore documentation and established MeshCore apps
  when needed. Qualify actual public-channel behavior and interoperability;
  do not substitute another protocol's radio settings or infer compatibility.
- Simplify the satellite/settings tab using the existing design system. Explain
  and organize Radio Settings versus Frequency Plan, show actual applied/read-back
  values, and distinguish a proposed change from one confirmed on the radio.
- After earlier backlog work: implement, deploy, test real messages in both
  directions between Framework and dev, investigate failures, repair and repeat.
  Include desktop and companion, private-key-safe diagnostics, long-running
  listener stability, restart/reconnect and meaningful negative cases. Record
  firmware/builds, radios, conditions and results. Simulated or headless checks
  alone do not constitute RF acceptance or a claim of flawless operation.

### MeshCore channel scope clarification

The operator explicitly requires the standard MeshCore public channel(s) used
by the wider community, not an Archipelago-specific/custom channel. Verify the
current official default channel identity/key/configuration and interoperability
with standard MeshCore clients on the selected UK RF plan. Two Archipelago radios
agreeing with each other on a private/custom channel is not acceptance. Custom
channel creation/configuration is deferred to later work and must not be added
as a substitute for repairing the standard public-channel experience.


## 16. Web5 card footer alignment

**Done — 2026-10-07 browser/deployment acceptance:** Monitoring, Identities and
Federation cards passed on dev/Yaya at390px/1440px, including tall/shrinking
neighbours, long action labels, bottom geometry, no overlap/overflow, keyboard
focus and Monitoring navigation. Stress content existed only in the disposable
browser DOM. Evidence:`~/.local/state/archipelago/release-qualification/web5-footer-20261007/`.
The separate Framework Monitoring owner-browser check remains open.

- Keep Monitoring buttons anchored to the bottom of their card when adjacent
  cards grow, including search/loading/results changes. Apply the same footer
  behaviour to matching cards without changing the design system or adding
  unnecessary minimum height.
- Preserve normal document flow and space between content and actions; avoid
  absolute positioning that overlaps content. Verify tall neighbours, shrinking
  results, long labels, keyboard access and mobile/desktop layouts.
- Record actual browser geometry checks and deployment acceptance separately.

## 17. HTTPS embedded app authentication (reported 6 October)

- User reports opening apps inside an iframe on an HTTPS node shows the app gate, while opening the same app in a separate tab works. Reproduce both modes with the same authenticated session before identifying a cause.
- Trace generated launch origins, cookie attributes and scope, bootstrap redirects, iframe navigation and gate session exchange. Preserve authentication and public-management access restrictions; do not bypass the gate or expose credentials to embedded apps.
- Test HTTPS iframe and tab, HTTP LAN compatibility, desktop/mobile companion, reload, expired sessions, denied access and logout. Record actual-node evidence separately from fixtures. This remains open, not a confirmed diagnosis.

### Task 17 progress

Yaya's app gate could not read its root-only TLS leaf key; logs confirmed
permission denied. Correcting only its group/mode restored authenticated HTTPS
iframe loading, while unauthenticated requests still return401. Source fixes
cover startup migration, hostname regeneration, first boot and explicit rotation;
tests pass. Exact operator hostname/app, trusted TLS and companion acceptance
remain open. See `docs/https-app-gate-followup-20261006.md`.

## 18. Firewall and tunnel UI/settings

Status: handover read and acknowledged on6October; implementation pending.
The private handover and acknowledgement live in the separate mining review
checkout. Do not commit its deployment addresses or operational details here.

- Keep this task after the previously deferred MeshCore work. The subsequent
  connection UX additions below follow all other tasks and their acceptance.
- Network → Local network: make the Firewall Active row clickable and add a
  bottom-anchored Firewall & tunnels button. Both open one central settings
  screen; app pages may link to it, but are not the primary configuration UI.
- Show firewall services/ports, protocols, allowed sources and effective state;
  tunnel connection/handshake status, peers and endpoints; manifest-backed app
  and service selection, existing tunnel, public port and domain when relevant.
- Treat transport reachability separately from browser/app-gate authentication.
  Raw TCP services need a copyable protocol endpoint, not an HTTP proxy host or
  certificate requirement. Keep mining service exposure separate from admin UI.
- Own the complete route: public forwarding, container/host listener and node
  tunnel firewall, restricted to the tunnel peer. Show independent checks and
  name the precise failing stage. Unmanaged/unreachable VPS must show VPS setup
  required with concrete instructions, never a misleading success indication.
- Preserve unrelated rules/tunnels/apps; validate before applying, persist scoped
  changes, roll back failures, remove only owned exposure rules on disable, and
  provide repair/reconcile for detected drift.
- Preserve the manual node repair. The handover's later external mining success
  supersedes its earlier unverified-forwarding notes. Treat this as received
  evidence, not newly performed validation. Reboot/persistence gates remain open.
- Use the requested shared browser-check skill when located; it is not available
  in this session's skill catalog. Keep its browser scenarios outside repositories
  as requested, while retaining repository regression and release requirements.
- Review and integrate once through ngit, preserve the already integrated mining
  work, and mirror accepted commits to Gitea. No release gate is waived.

## Deferred expansion of tasks 3/6: connection UX and final functional review

Operator sequencing on6October: finish all other tasks, related deployments,
tests, app deployments and UAT first. Then perform this expansion and the final
functional/UX review. These are additions to existing groups, not closed work.

- Distinguish peer relationships from federation membership in labels, actions
  and removal confirmations, including nodes with both relationships. Remove only
  the selected relationship; describe exactly what changed.
- Anchor removal buttons at card bottoms. Show an immediate per-action spinner,
  prevent duplicate submissions, and show an accurate completion/error toast.
- Add a bottom-anchored Network map button to the Connected Nodes container,
  linking directly to the network map screen on desktop and mobile.
- Include peer and trusted-node counts in the Connected Nodes top summary row,
  with explicit labels and a compact responsive layout. Derive counts from the
  authoritative relationship/trust state, update them automatically, and avoid
  double-counting identities in the overall node total when categories overlap.
- Lay out these card-footer actions for mobile as well as desktop: maintain
  bottom alignment within the card, clear labels, comfortable tap targets and
  consistent spacing. Stack actions when needed instead of squeezing them;
  prevent clipping, overlap and horizontal overflow. Check narrow phone widths,
  enlarged text and loading states while preserving the existing design system.
- Measure and reduce removal latency. Verify whether an authenticated existing
  FIPS connection is preferred; implement that priority where supported, with
  bounded fallback and no weakening of identity or authorization checks.
- Hide rejected, expired and otherwise resolved Nostr requests from actionable
  lists. Prevent relay replay or stale refresh results from resurrecting them;
  retain any history needed for diagnostics separately.
- Give incoming peer requests useful notifications linking directly to the
  correct Federation/Peers request and its accept/reject actions.
- Update relationship, request, availability and operation states automatically
  across screens without routine manual sync. Handle reconnect, missed events,
  out-of-order replies and failed refreshes without invented success states.
- Federated Nodes sync is currently reported as unreliable and slow. Diagnose
  the actual transport, queue and reconciliation path; prevent indefinite
  waits; show a loader with a useful phase/status message and elapsed progress;
  provide bounded retry and a truthful success, partial-success or failure
  result. Preserve the last known state while a refresh is in flight and make
  the next retry safe and idempotent.
- When a request changes the 3D map layout, rotate its node to the front, top
  centre and clearly expose the request action. Cover multiple requests,
  completion, user camera interaction, reduced motion and mobile viewports.
- Review complete desktop/mobile connection flows after the other work passes:
  put useful node capabilities and actions first, reduce unnecessary navigation
  and scrolling, and assess direct links to permitted peer Cloud files. Preserve
  the design system, trust boundaries and meaningful loading/error feedback.
- Qualify real reciprocal removal/requests, offline and reconnect behavior,
  duplicate/replayed events, automatic UI convergence, notifications/deep links,
  map positioning and responsive card geometry before claiming acceptance.

## 19. Reusable media developer guide and companion Cloud video PiP

Added by the operator after the V4V player refinements. Preserve the earlier
instruction to complete the other work before the deferred final connection UX
review; this new media task is part of that preceding work.

- Document the reusable audio manifest, handshake/state/control protocol,
  security boundaries, login redirects, queue ownership, artwork, lifecycle,
  mobile layout and reproducible test/deployment steps for developers and agents.
  Use V4V as the reference; distinguish implemented source from live acceptance.
- Implement companion picture-in-picture for Cloud videos first. Investigate
  viewer controls and native Android support together, preserving the same video
  and authorized/FIPS transport rather than opening an unauthenticated second URL.
- Scope PiP to video, never expose the dashboard/management UI in the PiP window.
  Test entry/exit, play/pause/close, return, aspect ratio, rotation, background,
  completion, unsupported/disabled PiP, session expiry and network interruption.
- Require physical Android companion acceptance and signed APK distribution.
  Document the supported video/PiP contract for other apps after qualification;
  do not claim the audio postMessage bridge already implements video PiP.

Source inspection so far: MainActivity declares no supportsPictureInPicture flag,
and the native sources contain no PiP entry/controller implementation. Existing
WebViewFullscreen handles fullscreen; this is not evidence of native PiP support.

## 20. Companion background media for V4V and other apps

**In progress — background playback operator-confirmed on APK0.5.37 (2026-10-07):**
The earlier APK0.5.36 failed after locking/switching apps. After installing0.5.37,
the operator reports playback now works and the notification is present; song
artwork is missing. Record this as acceptance of the reported background flow,
not proof of the complete controls/queue/reconnect matrix or a confirmed root
cause. Artwork correction and remaining lifecycle checks are ongoing. USB access
is unavailable.

Later media-platform task requested by the operator. When a user closes an
embedded V4V demo or another authorized media app on the companion, playback
must continue through the phone's native media session instead of stopping or
playing invisibly without controls. Apply the same lifecycle contract to audio
and video, with video using picture-in-picture where the device supports it.

- Keep the current track/video, position, queue, artwork and authorization when
  the app surface closes; reopening resumes the same session without a second
  stream or duplicate payment.
- Expose native notification/lock-screen/headset controls for play/pause,
  previous/next, seek where supported, shuffle and stop, with controls kept in
  sync with the app and the dashboard bottom player.
- For video, move to a native PiP surface on close or explicit PiP request when
  supported, preserve aspect/orientation and return to the same WebView session;
  fall back to audio-only/background rules when PiP is unavailable.
- Enforce origin, session, entitlement and transport boundaries during
  background playback; expiry, logout, network loss and user stop must halt or
  degrade truthfully and release media resources.
- Qualify V4V on physical Android at phone, lock-screen, background, rotation,
  reconnect, completion and reopen states, then publish the reusable contract
  for other audio/video apps. No live app deployment or payment is accepted
  from browser-only tests.


## 21. Public files and folders — deferred end-of-list task

Added by the operator on 2026-10-07. Perform after the existing tasks, including
previously deferred work. Status: queued; no public visibility has been changed.

- Add **Make public** to the More menus for both files and folders. Public means
  discoverable by any node, including nodes with no peering or federation
  relationship to the owner. Relationship approval must not be required merely
  to discover a public item.
- Reuse the existing **Share with peers pricing interface**, including optional
  charging and its supported pricing/payment choices. Use the existing brand
  **blue tints instead of orange** for this public-sharing variant; do not create
  a separate pricing interaction or change the peer-sharing colour treatment.
- Place a very small blue public-state marker at the **top-left of the file or
  folder icon**. Keep the marker readable and accessible without covering the
  thumbnail or replacing the existing icon.
- Separate public discoverability from paid access: listed paid content must
  still enforce its price/entitlement. Reuse settlement and recovery protections
  so retries do not charge twice.
- Make folder scope explicit in the reused flow, including existing/new children,
  nested items and individual visibility/pricing overrides. Preserve private
  items until the owner actually applies the action; provide a way to withdraw
  public sharing and reflect that state in menus, listings and markers.
- Qualify file/folder menus, shared pricing UI with blue styling, tiny marker,
  unrelated-node discovery, paid access, withdrawal and mobile/desktop layouts.
  No live file publication or real payment is authorized merely by adding this
  task to the backlog.


## 22. Mesh Share file: node/computer picker and paid delivery — final task

Added by the operator after task21 on 2026-10-07. Append after public sharing;
status: queued. Plan the node-to-node sharing design before implementation.
Preparation: [source-grounded sharing plan](mesh-node-file-sharing-plan-20261007.md);
no implementation or public visibility change is enabled.

- Clicking **Share file** in Mesh opens a source-choice modal with **From node**
  and **From computer**. From computer opens the local device file picker;
  plan upload/staging and delivery within the same send flow.
- **From node** opens a polished file-browser modal using the existing design
  system, with folder-tree navigation and search across **all of the user's own
  files**, not just the currently open folder. Preserve ownership/permissions;
  show useful paths for matching names and truthful loading/empty/error states.
- Allow multiple file selections. Retain selections across tree navigation and
  searches, collect them in a selection area at the bottom, allow individual
  removal, and provide a clear **Send** action with a selected-file count.
  Keep the browser and selection area usable on mobile and desktop.
- Plan and compare the supported node-to-node delivery paths before choosing
  the implementation: reuse existing authenticated file sharing, discovery and
  FIPS transports where suitable. Determine what the Mesh message carries,
  how recipient identity/access is checked, where file bytes reside, and how
  local-computer files become available without duplicate uploads or exposing
  unrelated/private files. Distinguish Mesh messaging from bulk-file transport;
  cover sender offline, recipient offline, reconnect, expiry and resumable
  delivery with explicit progress and failure/retry behaviour.
- Support sharing an existing paid file and configuring a paid share through
  the existing pricing flow. When a recipient opens a paid file, present the
  **usual purchase/payment interface** and supported payment methods; verify
  settlement/entitlement before releasing paid content. Reuse ownership and
  recovery logic so repeat opens, retries and lost replies do not charge again.
- Define mixed free/paid multi-file messages, recipient previews, per-file price
  and delivery states, existing entitlements and source deletion/withdrawal.
  Selecting or sending a file must not silently make it globally public;
  preserve the distinction from task21 public discovery.
- Qualify both source choices, tree navigation, whole-library search, retained
  multi-selection/removal, Send, cross-node receipt/open/download, free and paid
  access, duplicate/reconnect recovery and responsive/accessibility behaviour.
