# Peer content request authentication

Candidate implementation; isolated tests and actual-node qualification remain
required. No live deployment or migration acceptance is implied.

The content routes previously treated `X-Federation-DID` as an authenticated
identity. Knowing another node's public DID could therefore satisfy restricted
sharing checks. A claimed identifier is no longer used for authorization.

New requests use `X-Archipelago-Content-Auth`: bounded base64 JSON signed with the
existing node Ed25519 key. The versioned signature preimage binds the sender DID,
recipient DID, GET method, exact path, exact Range header and timestamp. The
receiver uses strict signature verification and a 60-second clock window. This
is an Archipelago request-authentication extension, not a new Nostr NIP. It is
a short-lived authorization proof for one read scope, not proof of settlement
and not a claim of single-use replay protection within that scope/time window.
FIPS transport and existing sharing/payment checks remain required.

Catalog visibility, invoice issuance and file serving use the same sharing gate.
Anonymous public content remains available; specific-recipient and peer-only
shares require verified identity. Delisted items are never advertised or offered
for a new invoice. The local authenticated operator retains the existing owner
access rules. Outbound reads never create or repair a missing signing identity.

Older nodes can still access public shares. Restricted sharing requires both
nodes to support the proof; failure must not silently downgrade to a claimed
DID or broaden availability. Test signature mutation, recipient/path/range/time
changes, missing proofs, private catalog filtering, invoice refusal, legitimate
peer streaming and clock skew before deployment. No private live file was used
to demonstrate the source finding.

Separate open review: existing on-chain addresses serve as delivery gate tokens,
and bearer ecash needs durable end-to-end receipt/recovery across a lost response
or process failure during settlement. These are not resolved by authenticating
a request and must not be marked passed by these signature tests.
