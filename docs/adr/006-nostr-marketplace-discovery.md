# ADR-006: Nostr Relays for Marketplace Discovery

## Status

Accepted

## Context

Archipelago needs a mechanism for users to discover and install third-party applications. The traditional approach is a centralized app store (like Apple App Store, Google Play, or Umbrel's marketplace). However, a centralized store introduces:

- A single point of failure and censorship
- A trust dependency on the store operator
- Barriers to entry for app developers (gatekeeping)
- Privacy concerns (the store operator knows what every user installs)

As a sovereign computing platform, Archipelago should align with decentralized principles.

## Decision

Use **Nostr relays** (NIP-78 application-specific data, kind 30078 events) for decentralized app manifest discovery instead of a centralized marketplace server.

### How It Works

1. **App developers** publish signed manifests as Nostr events to public relays
2. **Archipelago nodes** query multiple relays for available app manifests
3. **Trust scoring** uses verification count across relays, developer reputation (DID-linked), and optional community endorsements
4. **Users** see a merged, deduplicated list of available apps with trust indicators

### Trust Tiers

- **Verified**: Published by known developers, seen on 3+ relays, DID-verified
- **Community**: Seen on 2+ relays, valid manifest, unsigned or new developer
- **Unverified**: Single relay, new developer, use at own risk

## Consequences

### Positive

- No single point of failure — apps remain discoverable even if relays go offline
- No gatekeeping — any developer can publish apps
- Privacy-preserving — no central server tracking installs
- Censorship-resistant — apps can't be removed by a single entity
- Aligns with Nostr ecosystem already used for node identity

### Negative

- Discovery can be slower (querying multiple relays)
- Quality control relies on trust scoring rather than human curation
- Spam/malicious manifests require robust filtering
- Users need to understand trust tiers (not a simple "everything is safe" model)

### Mitigations

- Cache relay responses locally for fast subsequent loads
- Built-in curated app list for essential apps (Bitcoin, LND, etc.)
- Container security model (readonly_root, capability dropping) limits damage from malicious apps
- Manifest signature verification before installation
