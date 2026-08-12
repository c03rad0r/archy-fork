# ADR-007: DID-Based Federation Trust

## Status

Accepted

## Context

Archipelago supports federation — multiple nodes forming a trusted group for remote monitoring, app deployment, and state synchronization. Federation requires a trust establishment mechanism:

- **Centralized PKI** (Certificate Authorities): requires internet access, introduces third-party trust
- **Pre-shared keys**: simple but doesn't scale, no identity verification
- **DID-based bilateral verification**: each node verifies the other's cryptographic identity directly

## Decision

Use **bilateral DID-based verification** with single-use invite codes for federation trust establishment.

### How It Works

1. **Node A** generates a single-use invite code containing its DID, .onion address, and a shared secret
2. **Node B** receives the code (out-of-band: QR code, message, etc.) and submits it
3. **Both nodes** verify each other's DIDs by exchanging signed challenges over Tor
4. **Trust is established** — each node stores the other's DID and public key
5. **Ongoing communication** uses DID-authenticated messages over Tor hidden services

### Trust Levels

- **Trusted**: Full access — can view status, deploy apps, sync state
- **Observer**: Read-only access — can view status but not modify
- **Untrusted**: Blocked from federation operations

## Consequences

### Positive

- No third-party trust dependency (no CA, no central server)
- Works fully offline/air-gapped for the verification step
- Strong cryptographic identity (Ed25519 keys)
- Granular trust levels for different access patterns
- Invite codes are single-use (no replay attacks)

### Negative

- Requires out-of-band code exchange (can't auto-discover peers for federation)
- No revocation mechanism beyond removing the peer from the local trust store
- Key rotation requires re-establishing trust with all peers
- Trust is bilateral — each node maintains its own trust decisions

### Mitigations

- Nostr-based node discovery (ADR-003) handles finding nodes; federation handles trusting them
- Tor hidden services provide transport encryption and anonymity
- State sync includes heartbeat/health checks to detect unreachable peers
