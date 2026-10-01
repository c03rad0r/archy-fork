# Angor Relay

Optional standalone strfry relay for Angor's public project metadata. See
[Angor Indexer setup](../angor-indexer/README.md) for client URLs and HTTPS/WSS.

The gate exposes port 8091 for Nostr clients. strfry validates event signatures;
this is a public relay, not a private messaging archive. It mounts only
`/var/lib/archipelago/angor-relay` and its separate configuration directory.
It never opens, reconfigures or shares the node's internal strfry database.

For a public domain, proxy HTTPS to node port **8091**, enable WebSocket upgrade,
and add `wss://your-relay-domain/` in Angor. Test both NIP-11 metadata (send
`Accept: application/nostr+json`) and a real Nostr subscription over WSS. An
Archipelago login page at this domain is a routing failure, not relay readiness.

New relays start without project history. Keep existing discovery relays alongside
yours until the needed original signed announcements and metadata are available
locally. Relays do not automatically synchronize. Any history import must retain
the original event IDs and signatures; verify funded projects against their
on-chain commitments. A working WebSocket with zero stored events is not proof
that the client's project discovery works. See the indexer README's browser test.

The configuration is seeded only when absent, preserving operator changes.
Stop the service before making a consistent backup of its event database.
Ordinary start/restart/recreation preserves both mounts. Use the standard app
lifecycle; do not manually recreate a systemd-managed container.

## Image provenance

Mirrored from `docker.io/dockurr/strfry:1.1.2`, upstream manifest digest
`sha256:e81d238db13507f6ef24c49d47cd0b0ea58ff207961f10581fa2a7c901054df4`.
The public Angor policy is supplied by this app's own configuration; it does not
reuse the internal relay's event whitelist.
