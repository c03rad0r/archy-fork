# Mesh node/computer sharing: implementation plan

Status: source-grounded preparation for backlog task22, after task21. No new
sharing behaviour is enabled. Existing IndeeHub/payment/media work remains first.

## Existing implementation and gaps

`neode-ui/src/views/Mesh.vue` currently accepts one browser File. It reads the
whole file for inline transfer or uploads it to `/api/blob`, then sends a
`mesh.send-content` reference. Transport advice distinguishes inline/Reticulum
resource, choice, Tor-only and impossible states. Preserve this advice for small
free attachments; do not assume radio messaging is a bulk-file transport.

`stores/mesh.ts` and `api/rpc/mesh/typed_messages.rs` implement ContentRef messages
with CID, metadata, sender onion and expiring recipient-scoped capability. The
recipient fetch path supports a locally cached blob. This payload is an immediate
blob capability, not a durable paid-content offer: never put paid bytes through
it before entitlement validation.

The current view resolves federation onion addresses by fuzzy display-name
matching. That cannot establish the recipient identity for the new paid/private
flow. Use a verified mesh-key-to-node-DID binding and authenticated transport
route; missing binding must be an explicit connection-required state, never a
best-guess match or silently created peer relationship.

`api/filebrowser-client.ts` provides authenticated directory listing and resumable
upload. It has no whole-library search operation. A recursive walk in the browser
would be slow, incomplete on partial failures, and unbounded for large libraries.
A scoped backend search/index or verified provider-native search is required.
Task5's external source connectors are not yet implemented: do not imply their
files are included in the first picker.

## Proposed user flow

1. Share file opens From node / From computer. Retain the intended recipient and
   caption while choosing, returning or cancelling.
2. From node uses a reusable owned-library picker: lazy folder tree, breadcrumbs,
   global-search results with full paths, loading/error/empty states. Search is
   scoped to the signed-in user's accessible library, not the current folder.
3. Keep a selection map by immutable source identity and file revision, independent
   of the displayed folder/query. Bottom selection tray shows count, removable
   entries, and Send. Deleted/inaccessible/stale revisions require review, not a
   silent substitution. Folders navigate; recursive directory sending is not
   implied by this file-only task.
4. From computer supports multiple files and resumable private staging on the
   sender node, with bounded streaming, per-file progress and stable upload IDs.
   Reopening/retrying reuses staged bytes; cancelled staging has a defined expiry.
5. Reuse the usual pricing component for each paid offer (or explicit selected
   group operation), retaining each file's existing policy. Sending does not
   publish globally or alter unrelated recipients' access.
6. Show per-file free/paid price and delivery state in the outgoing batch. A
   successful send means message accepted/queued, not recipient download complete.
   Partial success keeps failed selections and retries only their message IDs.

## Transport and payment decisions

Use an authenticated, versioned attachment-offer message containing a stable
message/item ID, seller DID, content ID/revision, safe display metadata and an
optional price/method summary. Treat price text as advisory: opening revalidates
the authenticated current offer. Never include file paths, local credentials,
prepayment media capabilities or a transferable paid unlock in the message.

Bulk bytes use the existing authenticated content transport and durable purchase
journal. Prefer the required FIPS path where supported; do not silently downgrade
a required transport to unauthenticated HTTP or the older blob endpoint. A node
with only radio connectivity may receive the offer but cannot be promised bulk
retrieval. Preserve existing explicitly selected small free inline attachments.

Paid open routes into the normal purchase interface. It must reuse seller/content/
revision/purchase identity and settlement records, including cross-rail admission,
lost-response recovery and cached delivery. Sending/selecting/previews never pay.
Reopening an existing entitlement never creates another payment. Fresh Fedimint
file purchase is currently disabled by the payment-safety patch and must not be
reintroduced through Mesh.

Message outbox retry/deduplication is separate from file-transfer retry and payment
recovery. Sender offline means pending/unavailable download, recipient offline
means durable queued delivery where supported, expired private access offers a
new authorization request without a new charge for an existing entitlement.
Withdrawn/deleted sources display an explicit state; previously delivered bytes
cannot be retroactively revoked. Mixed free/paid batches retain per-file outcomes.

## Implementation order and acceptance

- Define verified identity binding, offer schema/version handling and owned-library
  search/revision semantics before wiring Send. Older recipients get a truthful
  unsupported-offer state, never an insecure legacy fallback.
- Build source-choice and reusable picker with keyboard focus return, labelled
  tree navigation, cancellable search and retained selection on narrow screens.
- Add private resumable staging and durable batch/outbox IDs, then recipient offer
  rendering and normal purchase/download entry. Keep selection and payment state
  separate; avoid one whole-batch payment without supported atomic semantics.
- Qualify two owned nodes: free and mixed paid offers, duplicate names/paths,
  forbidden paths/symlinks, global search beyond current folder, pagination,
  partial errors, changed source revisions, cancelled/retried uploads, duplicate
  messages, wrong recipient, forged identity/price, sender/recipient restart,
  expired access, source withdrawal and interrupted cached download.
- Verify no payment on selection/send/preview, no paid plaintext capability before
  settlement, no repayment after lost replies, no global visibility change, and
  explicit incomplete indexing. Run meaningful handler/UI tests and responsive
  browser checks; real spending requires the existing bounded authorization.

Open decisions: authenticated identity binding UX for radio-only contacts;
provider-backed global search/index ownership; durable staging retention/quota;
attachment-offer protocol version negotiation. These block claiming a finished
implementation, not preparation of the picker design.
