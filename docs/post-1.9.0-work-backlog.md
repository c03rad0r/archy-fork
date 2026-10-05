# Work requested after 1.9.0-alpha

Status: queued by the operator on 2026-10-05. Complete the current release first;
these requests do not silently expand its artifact scope. No implementation or
acceptance is claimed by this backlog.

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
