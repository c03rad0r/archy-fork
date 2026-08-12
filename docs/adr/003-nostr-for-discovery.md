# ADR-003: Nostr Relays for Node and App Discovery

**Status**: Accepted
**Date**: 2026-03

## Context

Archipelago nodes need to discover peers and community apps without a central registry. Options: custom P2P protocol, DHT, BitTorrent tracker, Nostr relays, IPFS.

## Decision

Use Nostr relays (NIP-78, kind 30078) for both node discovery and marketplace app manifests.

## Consequences

### Positive
- **Decentralized**: Multiple independent relays; no single point of failure
- **Existing infrastructure**: Thousands of Nostr relays already running globally
- **Censorship-resistant**: If one relay censors, others still serve events
- **Simple protocol**: WebSocket + JSON — easy to implement without heavy dependencies
- **Key management**: Nostr uses secp256k1, same curve as Bitcoin — natural fit
- **NIP-33 replaceable events**: Latest event replaces previous — clean update model
- **Tor-compatible**: WebSocket over Tor SOCKS proxy works natively

### Negative
- **Relay availability varies**: Some relays may be down or rate-limited
- **No guaranteed persistence**: Relays may prune old events
- **Spam potential**: Open publishing means anyone can publish junk manifests
- **Latency**: Querying multiple relays adds latency to discovery

### Mitigation
- Query multiple relays in parallel; deduplicate results
- Cache results locally with 15-minute TTL
- Use trust scoring to rank manifests (DID verification, relay consensus, federation trust)
- Use hashtag filtering (`archipelago-marketplace`) to narrow queries
