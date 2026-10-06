# IndeeHub distribution: current implementation design

Status: design for the post-1.9 follow-up, not implemented/accepted. Earlier
swarm plans describe historical experiments and must not be read as live proof.

## Standards checked on 2026-10-05

- Nostr [NIP-71](https://github.com/nostr-protocol/nips/blob/master/71.md)
  defines video metadata, including addressable normal-video kind34235. Use a
  stable producer/project identifier for revisions. This does not define paid
  viewing rights. Hash/media metadata follows
  [NIP-94](https://github.com/nostr-protocol/nips/blob/master/94.md).
- [NIP-98](https://github.com/nostr-protocol/nips/blob/master/98.md) authenticates
  HTTP requests; it is not proof of payment or permission to another creator's
  project. Keep the existing verified app session and project ownership checks.
- [Cashu NUT-18](https://github.com/cashubtc/nuts/blob/main/18.md) supplies payment
  request negotiation. [NUT-04](https://github.com/cashubtc/nuts/blob/main/04.md)
  covers mint quotes/issuance; method-specific current specifications and older
  deployed mint responses must both be capability-tested. Keep quote identifiers
  private to the receiving wallet. Payment settlement must be correlated to the
  purchase, never inferred from a change in wallet balance.
- [NUT-19](https://github.com/cashubtc/nuts/blob/main/19.md) provides mint-side
  cached responses where supported. It supplements a durable local payment
  journal; it does not replace one or make an arbitrary retry safe.
- [LNURL-pay](https://github.com/lnurl/luds/blob/luds/06.md) supports an invoice
  handoff. A Lightning address by itself is not a signed settlement receipt.

## Required implementation contract

1. Backstage publishes only the selected, owned project. Announcements contain
   public metadata, producer identity, node identity, content hashes, current
   price/window and accepted-method capabilities. Never publish Cloud paths,
   mint quotes, tokens, paid media keys or private management addresses.
2. Each instance's Archipelago source verifies signatures, identity bindings and
   monotonic event revisions. Persist discovery so a relay outage does not empty
   an existing library. Keep other sources and the current default intact.
3. A purchase binds buyer identity, publisher, project revision, amount, currency,
   selected payment method and an unpredictable idempotency identifier. Persist
   the quote before requesting payment. Snapshot the offer so later edits cannot
   silently change the purchased terms.
4. Reuse file-payment capability negotiation and proven settlement primitives.
   Existing file Lightning invoices currently require LND; therefore adding
   first-use Lightning-to-Cashu receiving is real work, not a display label. The
   receiver must verify a correlated invoice/mint receipt and recover issuance
   after a lost response before granting access. Its provisioned ecash address
   must map to the actual receiving wallet. No new real payment is authorized by
   this design; use isolated fixtures until a bounded payment is approved.
5. A paid purchase creates one durable entitlement. Default demo window proposal:
   start at the first successfully authorized media response; persist start and
   expiry atomically. Retries, seek and reconnect reuse it without another
   payment. Check the entitlement for every media/range/key request. Clock
   rollback must not extend an already-started window.
6. Browser/companion playback remains ordinary authenticated local HTTP. Actual
   inter-node media bytes use FIPS, with a bound node identity and authenticated
   purchase capability. No silent Tor/LAN/iroh fallback for required FIPS media.
   Show a recoverable unavailable route without requesting another payment.
7. Range/segment serving streams bounded buffers with backpressure and cancel
   propagation. Do not read an entire paid film into a Vec before serving it.
   Verify length/hash/revision, reject malformed ranges and path escapes, and
   keep cache access subject to the same entitlement. Delivered plaintext cannot
   be made impossible to copy; expiry controls subsequent authorized delivery.

## Qualification that remains required

Publisher/receiver integration must cover altered metadata, forged identities,
wrong mint, rejected/late/duplicate payment, missing transaction response, restart,
clock change, expired window, revocation, missing FIPS route, seek and disconnect.
Measure real media-byte transport and memory use. Test the actual Backstage,
Archipelago listing, purchase and player on mobile/desktop and companion.

Use only the operator-designated Yaya Cloud video, preserve the source, and
publish the IndeeHub app image/catalog update at the end of qualification.
One working fixture is not acceptance of the complete distributed flow.
