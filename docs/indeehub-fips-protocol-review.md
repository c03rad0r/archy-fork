# IndeeHub distributed viewing: protocol review

Status: preliminary design, 2026-10-05. Not implemented or qualified. Reconcile
with actual IndeeHub source before selecting the final event/API contract.

## Updated constraints

The older `phase4-streaming-ecash-plan.md` mixes producer content sales with
bandwidth resale and an optional iroh swarm. Its implementation assertions are
historical. The operator now requires FIPS for inter-node media bytes, producer
payments, timed viewing, and an Archipelago catalog across instances. A successful
bandwidth payment alone must not unlock a producer's protected film.

The old document also describes a fail-open paid-serving path. Audit the current
implementation; never adopt fail-open behavior for paid media or key delivery.
Keep free software updates outside any paid-content gate.

## Primary specifications checked

- [NIP-71 video events](https://github.com/nostr-protocol/nips/blob/master/71.md):
  defines ordinary and addressable video metadata, including variants. Evaluate
  addressable normal-video events for stable film identity and metadata updates.
  This is a draft optional specification, not a complete rental/access protocol.
- [NIP-94 file metadata](https://github.com/nostr-protocol/nips/blob/master/94.md):
  describes file hashes, MIME types, sizes and locations. Reuse compatible fields
  rather than inventing incompatible meanings for standard tags.
- [Blossom BUD-01](https://github.com/hzrd149/blossom/blob/master/buds/01.md):
  specifies SHA256-addressed HTTP blob retrieval. Its public cross-origin server
  conventions must not be copied onto dashboard/RPC authentication boundaries.
  Hash addressing can complement a FIPS-backed gateway; Blossom alone does not
  establish payment or timed viewing rights.
- [Cashu NUT-18](https://github.com/cashubtc/nuts/blob/main/18.md): receiver requests
  can describe amount, unit, accepted mints and token delivery. Reconcile supported
  mint preferences/methods with our deployed wallets, not just the latest schema.
- [NUT-04](https://github.com/cashubtc/nuts/blob/main/04.md) and
  [NUT-23](https://github.com/cashubtc/nuts/blob/main/23.md): verify current mint
  quote/payment accounting and BOLT11 behavior against supported mints. Keep quote
  identifiers private. Successful invoice payment and successful token issuance
  are distinct recovery steps; never repeat payment to recover an issuance reply.

These are source/specification findings. Compatibility with existing clients and
mints remains to be tested. Pin specification revisions when implementing so a
moving document cannot silently change the wire contract.

## Proposed separation of responsibilities

1. **Discovery:** publisher-authorized signed metadata; stable film/version ID,
   public title/artwork/teaser and explicit supported paid-content extension.
   Deduplicate, reconcile updates/deletions and recover missed events. Do not put
   private viewing keys, receipts, quotes or wallet credentials on public relays.
2. **Producer offer and settlement:** reuse recipient-capability negotiation from
   file purchases. Bind amount, recipient, content version and duration to one
   durable purchase ID. Verify settlement at the seller before issuing access.
   Support the existing Lightning-to-ecash-address flow without requiring LND.
3. **Viewing entitlement:** a versioned authenticated grant with content, buyer,
   validity and replay rules. This is application-specific until interoperability
   is demonstrated; do not present it as defined by the metadata/payment NIPs.
4. **Playback gateway:** browser/companion use normal authenticated media requests
   to their node. The node obtains protected segments over FIPS and verifies
   content integrity and entitlement. Seek/retry/resume reuse the purchase.
5. **Caching:** peers may cache authorized ciphertext. Key delivery and subsequent
   segment access remain gated. Already delivered plaintext or keys cannot be
   made uncopyable or retroactively revoked; do not promise DRM guarantees.

No silent media fallback to Tor/LAN/iroh satisfies the operator's FIPS requirement.
If FIPS is unavailable, retain paid ownership and explain retry/recovery rather
than charge again. Separate routing diagnostics from normal playback controls.

## Decisions and proofs required before publishing the test film

Identify the exact Yaya Cloud video, preserve its original and obtain the intended
price and viewing-window semantics. Define activation versus expiry, clock skew,
multiple devices, publisher outage, refund policy and content-version replacement.
Use isolated/regtest funds for automated tests; new real payments require a bounded
amount authorization. Publish no unrelated Cloud file.

Qualification must cover settlement with lost replies, duplicate payment callbacks,
wrong mint/recipient/content, denied keys, expired grants, FIPS outage and recovery,
range/HLS seek, mobile background/resume, source/peer restart and storage recovery.
Verify actual transport and producer balance changes rather than relying on UI
labels. Include fresh/upgrade tests and any required IndeeHub image/catalog update
at the end of the implementation.
