# Multi-Node Architecture

## Overview

Archipelago supports federation — multiple nodes can form a trusted cluster to share status, deploy apps remotely, and coordinate services. This document describes the architecture for multi-node orchestration.

## Discovery & Trust Model

### Node Discovery

Nodes discover each other through two complementary channels:

1. **Nostr Relay Discovery**: Each node publishes its identity (DID, onion address, pubkey) to configured Nostr relays as a NIP-78 application-specific event. Other nodes query relays to find peers.

2. **Direct Invite**: A node generates an invite code containing its DID, onion address, and a one-time authentication token. The recipient node uses this code to establish a direct connection.

3. **Tor Hidden Services**: All inter-node communication uses Tor hidden services (.onion addresses) for privacy and NAT traversal.

### Trust Establishment

Federation uses a mutual DID verification model:

```
Node A                                              Node B
   │                                                   │
   │── federation.invite (generates invite code) ──►   │
   │                                                   │
   │   ◄── federation.join (presents invite + DID) ──  │
   │                                                   │
   │── Verify Node B's DID Document over Tor ──────►   │
   │   ◄── Verify Node A's DID Document over Tor ──   │
   │                                                   │
   │── Exchange signed challenge/response ─────────►   │
   │   ◄── Exchange signed challenge/response ──────   │
   │                                                   │
   │   [Mutual trust established]                      │
   │   [Both nodes add each other to federation]       │
```

**Trust Levels**:
- `trusted`: Full federation — can deploy apps, sync state, see all container statuses
- `observer`: Read-only — can see status but cannot deploy or modify
- `untrusted`: Discovered but not yet verified — pending invite acceptance

### ADR: Decentralized Trust over Centralized Authority

**Decision**: Use DID-based mutual verification instead of a central authority or PKI.

**Context**: Archipelago nodes are sovereign — no central server should control trust. Each node maintains its own trust list.

**Consequences**:
- (+) No single point of failure for trust
- (+) Nodes can federate without internet (direct Tor connection)
- (+) Consistent with the DID identity model already in use
- (-) No global revocation mechanism (each node manages its own trust)
- (-) Trust is bilateral — A trusting B doesn't imply C trusts B

## Shared State Protocol

### State Sync

Federated nodes periodically sync their state. Each node exposes a state summary via its RPC endpoint, accessible only to trusted federation peers.

**Synced data**:
- Container/app statuses (installed, running, stopped, version)
- Node health (CPU, memory, disk, uptime)
- Available storage capacity
- Tor hidden service status
- Lightning Network status (channels, capacity)

**Not synced** (privacy):
- Credentials and secrets
- Private keys
- Session data
- User passwords

### Sync Protocol

```
Every 5 minutes (configurable):
  For each federated node:
    1. POST to peer's /rpc/ endpoint: federation.get-state
    2. Authenticate with signed challenge (DID key)
    3. Receive state snapshot
    4. Store in local federation cache
    5. Broadcast changes via WebSocket to local UI
```

### State Storage

```
/var/lib/archipelago/federation/
  ├── nodes.json           # List of federated nodes with trust levels
  ├── state-cache/
  │   ├── <node-did>.json  # Latest state snapshot from each peer
  │   └── ...
  └── invites/
      ├── pending.json     # Outgoing invites awaiting acceptance
      └── received.json    # Incoming invites awaiting approval
```

## RPC Endpoints

### Federation Management

| Method | Description | Auth |
|--------|-------------|------|
| `federation.invite` | Generate invite code for a new peer | Local |
| `federation.join` | Accept an invite and establish federation | Local |
| `federation.list-nodes` | List all federated nodes with status | Local |
| `federation.remove-node` | Remove a node from federation | Local |
| `federation.set-trust` | Change trust level for a federated node | Local |

### Federation Data Exchange

| Method | Description | Auth |
|--------|-------------|------|
| `federation.get-state` | Return node's state snapshot | Federation peer |
| `federation.deploy-app` | Request remote app installation | Trusted peer |
| `federation.sync-state` | Trigger manual state sync | Local |

### Authentication for Inter-Node RPC

Federation RPC calls between nodes use DID-based authentication:

1. Caller includes `X-Federation-DID` header with their DID
2. Caller includes `X-Federation-Sig` header with a signed timestamp
3. Receiver verifies the DID is in their trusted federation list
4. Receiver verifies the signature using the DID's public key
5. Timestamp must be within 5 minutes to prevent replay attacks

## Federated App Deployment

### Flow

```
Local Node                          Remote Node
     │                                   │
     │── federation.deploy-app ──────►   │
     │   {app_id, version, config}       │
     │                                   │
     │   [Remote verifies trust level]   │
     │   [Remote checks if app exists]   │
     │   [Remote pulls container image]  │
     │   [Remote starts container]       │
     │                                   │
     │   ◄── Status update via sync ──   │
     │   {app_id: "running"}             │
```

### Constraints

- Only `trusted` peers can deploy apps to each other
- Remote node can reject deployment (insufficient resources, policy)
- Container images are pulled from registry, not transferred between nodes
- App configuration is sent with the deploy command
- Remote node applies its own security policies (AppArmor, capabilities)

## UI: Federation Dashboard

**Route**: `/dashboard/server/federation`

**Components**:
1. **Node List**: Table of federated nodes showing:
   - Node name (DID-derived or custom alias)
   - Status: online/offline (based on last successful sync)
   - Trust level badge (trusted/observer)
   - App count, resource usage summary
   - Last seen timestamp

2. **Add Node**: Form with invite code input or QR code scanner

3. **Node Detail Modal**: Clicking a node shows:
   - Full DID and onion address
   - Container/app list with statuses
   - Resource usage (CPU, memory, disk)
   - Deploy app button (if trusted)
   - Change trust level / remove node

## Security Considerations

1. **All federation traffic over Tor**: Prevents IP address leakage between nodes
2. **DID-based auth**: No shared secrets; each node proves identity with its key
3. **Replay protection**: Signed timestamps prevent replay attacks
4. **Trust is bilateral**: Both nodes must agree to federate
5. **App deployment is opt-in**: Remote node can refuse deployment requests
6. **State snapshots are read-only**: A compromised peer cannot modify another node's state
7. **Invite codes are single-use**: Once accepted, the invite token is invalidated
