# Peering & Federation Trust — naming and semantics

Status: TERMINOLOGY SET — records what the code does today (#134).
Deferred: the "don't advertise my peers" opt-out (see §Open questions).

The code is the authority; this doc gives names to the four concepts that
issue #134 showed get conflated in conversation. Where a name changed in
user-facing discussion, the term below is the one to use everywhere
(UI copy, docs, issues, reviews).

## The four concepts

| Term (use this) | What it is | Where it lives |
|---|---|---|
| **Trusted peer** | A node THIS operator invited and verified: bilateral DID challenge over an out-of-band invite code (`federation::sync`, ADR-007). The only level that grants full access. | `TrustLevel::Trusted`, set via `TrustSource::Invite` or `Manual` |
| **Discovered peer** | A peer we learned about from a Trusted peer's advertised list — the transitive merge. Never better than **Observer**: `TRUST IS NOT TRANSITIVE` (sync.rs guard). | `TrustLevel::Observer`, `TrustSource::TransitiveMerge` |
| **Routing hint** | What a Discovered peer actually contributes: an address that lets us route directly over FIPS without a second invite hop. Reachability, not trust. | Observer-level sync + FIPS endpoint records |
| **Peer advertisement** | The act of a Trusted peer sharing its own peer list during sync. This is the *mechanism* #134 observed — a feature, not a leak. | sync.rs merge path |

## The two rules that make it sound

1. **Trust requires an operator decision, always traceable.** Every trust
   level carries a `TrustSource`. Only a minted invite (or an explicit
   operator change) can produce `Trusted`; uninvited joins and transitive
   merges are hard-capped at `Observer` — a peer can never expand our
   trusted set on its own authority.
2. **Discovery is transitive; trust is not.** Seeing more nodes through a
   Trusted peer is expected and useful (routing). Granting those nodes
   anything is an operator action, never automatic.

## Why a Trusted peer advertising its list is by design

Without advertisement, every new node needs a direct invite from every node
that wants to reach it — the invite graph becomes the routing bottleneck
AdDR-007 set out to remove. With it, one invite makes a node *reachable* to
the trusted set (routing hints), while *authorization* still requires each
operator's own invite. Reachability ≠ access.

## Open questions (deferred, tracked in #134)

- **"Don't advertise my peers"** — an operator privacy toggle suppressing
  peer advertisement during sync. Small code change, real design questions:
  it hides peers who may WANT discovery, and it degrades the routing benefit
  for every node trusting you. Needs a product decision, not just code.
- **Tier vocabulary in the UI** — whether to surface "Observer" as such or
  a friendlier term ("Connected"/"Visible") — part of the TODO.md peering
  trust-model item.
