# ADR-004: Tor Hidden Services for Peer Communication

**Status**: Accepted (2026-03) — **partially superseded in practice, see
Amendment below**
**Date**: 2026-03

## Context

Federated nodes need to communicate directly for state sync, app deployment, and peer verification. Options: direct IP, VPN tunnel, Tor hidden services, I2P.

## Decision

Use Tor hidden services (.onion addresses) for all inter-node communication.

## Consequences

### Positive
- **NAT traversal**: Works behind any firewall or NAT without port forwarding
- **IP privacy**: Nodes never expose their real IP addresses to each other
- **End-to-end encryption**: Tor provides encryption without additional TLS setup
- **Censorship resistance**: Onion routing makes traffic analysis difficult
- **Stable addressing**: .onion addresses persist across IP changes and network migrations
- **No central infrastructure**: No VPN server, STUN/TURN server, or relay needed

### Negative
- **Latency**: Tor adds 200-500ms per hop; 3 hops per direction = noticeable delay
- **Bandwidth**: Tor network has limited bandwidth; not suitable for bulk data transfer
- **Reliability**: Tor circuits can break; connections may need retry logic
- **Setup complexity**: Requires running a Tor daemon (`archy-tor` container)
- **Blocked networks**: Some networks block Tor; bridges can help but add complexity

### Mitigation
- Use Tor only for RPC/control plane; bulk data (container images) pulled from registries
- Implement retry with backoff for Tor connections
- Container `archy-tor` runs automatically with host networking for hidden service access
- Federation sync interval (5 min) tolerates occasional connection failures

## Amendment (recorded 2026-08)

Two things in this ADR no longer describe the system. Both changes happened
without their own ADR, which is itself worth noting.

**1. Tor is no longer used for *all* inter-node communication — it is the last
fallback.** The transport layer now tries, in order, mesh radio → LAN → FIPS
overlay → Tor (`transport::TransportKind`, priority 1–4). The latency and
bandwidth costs listed above are exactly why: FIPS was introduced to carry WAN
peering that Tor made too slow, and direct LAN peering skips the overlay
entirely for co-located nodes. Tor's NAT-traversal and IP-privacy properties are
still what make it the dependable floor when the others are unavailable.

**2. Tor does not run as the `archy-tor` container.** It is the host's Debian
`tor` package, running as `debian-tor` and driven by the
`archipelago-tor-helper` path unit (`scripts/tor-helper.sh`), which installs a
staged `/etc/tor/torrc` and restarts the service. The migration was deliberate
and is still enforced: `scripts/container-doctor.sh` removes an `archy-tor`
container if it finds one and switches the node to system Tor. There is no
`apps/tor` manifest.

The decision to use onion services for peer reachability stands; only its
exclusivity and its packaging changed.
