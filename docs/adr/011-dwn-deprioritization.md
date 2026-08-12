# ADR-011: DWN Deprioritization

## Status

Accepted

## Context

TBD/Block shut down in November 2024, donating Web5 code to the Decentralized Identity Foundation (DIF). The DWN (Decentralized Web Node) specification was heavily backed by TBD — without their engineering team, the spec has lost momentum:

- No maintained Rust DWN SDK exists (the `dwn` crate by unavi-xyz is v0.4.0 with 323 downloads)
- TBD's reference implementation was TypeScript-only
- DIF has not allocated resources to continue DWN development
- The spec itself is complex (personal data stores with protocol-based access control)

Meanwhile, Archipelago's federation over Tor + Nostr relays already serves the core peer data sync use case that DWN was intended for.

## Decision

1. **Keep existing DWN store code** in `core/archipelago/src/network/dwn_store.rs` — it works for peer file catalogs and federation state
2. **Stop calling it "Web5 DWN"** in user-facing text — it's our custom implementation, not a full DWN spec implementation
3. **Do not invest in DWN spec compliance** — the spec is stalled and may not stabilize
4. **Prioritize Nostr + federation** for peer discovery and data exchange
5. **Re-evaluate if DIF produces a viable Rust SDK** or the spec gains new maintainers

## Consequences

- DWN functionality remains available but is not actively developed
- Peer sync uses federation + Nostr instead of DWN protocols
- Reduces maintenance burden — no need to track a stalled spec
- If DWN resurfaces with strong tooling, we can adopt it later
