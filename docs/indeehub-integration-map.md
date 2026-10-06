# IndeeHub distributed source: integration map

Implementation planning checkpoint, October 6, 2026. This document does not claim
that cross-node publication, payment or timed viewing works yet.

## Existing paths and required changes

| Area | Existing implementation | Required integration |
|---|---|---|
| Backstage ownership | IndeeHub `projects.controller.ts` uses HybridAuthGuard, subscription and permission guards; writes reach `projects.service.ts` | Preserve project ownership checks. Commit publication and a retryable announcement outbox together; relay failure must not duplicate or lose the publication. |
| Browse source | `src/stores/contentSource.ts` lists IndeeHub, TopDoc and the configured local API | Add **Archipelago** only with its working adapter. Keep the current default until qualified. |
| Catalog loading | `src/stores/content.ts` merges local published projects and falls back to mock data on errors | Distributed-source errors must preserve the last verified catalog and show stale/unavailable status. They must not silently substitute unrelated demo titles. |
| Nostr publication | Existing signer/auth plumbing handles app login, not distributed video publication | Producer signs public video metadata; bind it to the serving node and offer revision. Persist verified discovery and handle revisions/deletions deterministically. |
| Rental model | `backend/src/rents/rents.service.ts` uses BTCPay and a two-day window measured from creation | Snapshot creator-selected price/window. Start a new distributed entitlement on the first authorized playback, not when a pending invoice is created. Keep existing rentals compatible. |
| Receiving wallet | Current core file-invoice path requires LND | Add correlated Lightning-to-Cashu receiving for first-use producers, with durable mint-quote recovery. A Lightning address or balance change alone is not proof of this purchase. |
| Core IndeeHub adapter | `core/archipelago/src/content_indeehub.rs` fetches local titles for AIUI | This is not a distributed publisher or a rental/payment gateway. Do not treat its successful catalog fetch as acceptance of this feature. |
| Media delivery | New core peer-file paths enforce FIPS and stream bounded chunks/cache files | Reuse the transport and stream primitives; add per-request timed entitlement checks. Do not expose an unrestricted owned-file cache for expiring rentals. |

## App contract

Use the manifest-first development contract. Any new node/app capability must be
reusable and scoped to its application, authorized producer and content. An app
must not receive the dashboard session, node signing secret or unrestricted wallet
RPC access. Native Nostr signing stays in the existing host consent flow.

Publishing is an explicit Backstage action. Do not automatically publish private
projects or browse the operator's Cloud directories looking for content. The only
selected demo source is the operator-designated Yaya video; preserve its original.

## Standards and application-specific semantics

The current [NIP-71](https://github.com/nostr-protocol/nips/blob/master/71.md)
was rechecked on October 6: addressable video metadata provides stable references
and revisions. [NUT-18](https://github.com/cashubtc/nuts/blob/main/18.md) describes
receiver payment requests. Neither defines this app's rental rights. Keep the
paid-content extension versioned and explicit; ordinary metadata must not disclose
private media keys, wallet quotes, tokens or management endpoints.

## Test spending and creator pricing

Creators choose prices and viewing terms. **One sat is an initial test amount,
not a product-wide price or default.** On October 6 the operator authorized
node-to-node test spending. The expanded self-imposed limit is 25 sats total,
including fees, between operator-owned nodes, after the operator reported topping
up both nodes and authorized longer tests. Record each payment and verify the
recipient, settlement, delivery and retry without repayment. No payment has been
made under this authorization at this checkpoint. No paid AI use is authorized.

The demo's final viewing window is not yet confirmed. Exercise configurable
windows with simulated payments while awaiting that choice; do not publish an
assumed commercial offer.
