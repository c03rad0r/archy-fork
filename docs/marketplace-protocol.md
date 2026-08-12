# Decentralized App Marketplace Protocol

**Status:** implemented (updated 2026-07-08). This started as a protocol
proposal; the described subsystem is now shipped end-to-end —
`core/archipelago/src/marketplace.rs` (discover/publish/trust scoring),
the `marketplace.*` RPC namespace, and `Marketplace.vue`. Beyond this doc,
the code also adds `marketplace.create-invoice` (Lightning BOLT11 app
purchases). What remains is maturation: publishing tooling and trust UX
(see `ROADMAP.md`). Note: the manifest schema below is the marketplace's
own flatter format, **not** the runtime `apps/*/manifest.yml` schema
(`app-manifest-spec.md`).

> **The DID signature layer is implemented** as of 2026-08-08. `publish` signs
> with the node's Ed25519 identity key, `discover` verifies every manifest
> before caching it, and a manifest whose signature is *present but wrong* is
> dropped rather than listed at a lower score. See
> [Signing Protocol](#signing-protocol) for the exact preimage rules — they are
> normative, and an implementation that canonicalises differently will produce
> signatures this node rejects.

## Overview

Archipelago's community marketplace enables developers to publish app manifests to Nostr relays, where nodes discover and install them without a central app store. Trust is established through DID-signed manifests and community reputation.

## Architecture

```
Developer Node                    Nostr Relays                    User Node
     │                                │                               │
     │── Publish signed manifest ──►  │                               │
     │   (NIP-78, kind 30078)         │                               │
     │                                │  ◄── Query app manifests ──   │
     │                                │      (filter by d-tag)        │
     │                                │                               │
     │                                │── Return signed manifests ──► │
     │                                │                               │
     │                                │      [Verify DID signature]   │
     │                                │      [Check trust score]      │
     │                                │      [Display in marketplace] │
     │                                │                               │
     │                                │      [User clicks Install]    │
     │                                │      [Pull container image]   │
     │                                │      [Start container]        │
```

## Manifest Schema

App manifests published to Nostr relays use the marketplace's own flatter JSON
schema — the `AppManifest` type in `marketplace.rs`, shown below — serialized
into the Nostr event's `content`. It is **not** the runtime
`apps/{app-id}/manifest.yml` schema in
[`app-manifest-spec.md`](app-manifest-spec.md); the two are separate types that
happen to share a name.

### Marketplace Manifest Fields

```json
{
  "app_id": "my-bitcoin-tool",
  "name": "My Bitcoin Tool",
  "version": "1.2.0",
  "description": {
    "short": "A useful Bitcoin utility",
    "long": "Detailed description of what this app does..."
  },
  "author": {
    "name": "Developer Name",
    "did": "did:key:z6Mkh...",
    "nostr_pubkey": "npub1..."
  },
  "container": {
    "image": "docker.io/developer/my-bitcoin-tool:1.2.0",
    "ports": [{ "container": 8080, "host": 8180, "protocol": "tcp" }],
    "volumes": [{ "name": "data", "path": "/data" }],
    "env": {
      "NETWORK": "mainnet"
    },
    "capabilities": [],
    "readonly_root": true,
    "no_new_privileges": true,
    "run_as_user": 1000
  },
  "category": "money",
  "icon_url": "https://example.com/icon.png",
  "repo_url": "https://github.com/developer/my-bitcoin-tool",
  "license": "MIT",
  "min_archipelago_version": "0.1.0",
  "dependencies": [],
  "signatures": {
    "manifest_hash": "sha256:abc123...",
    "did_signature": "base64-encoded-signature"
  }
}
```

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `app_id` | string | Unique identifier, lowercase kebab-case |
| `name` | string | Human-readable display name |
| `version` | string | Semantic version (major.minor.patch) |
| `description.short` | string | One-line description (max 120 chars) |
| `author.did` | string | Developer's DID (did:key method) |
| `container.image` | string | Full container image reference with tag (never `latest`) |
| `category` | string | One of: money, commerce, data, networking, home, community, other |

### Security-Required Fields

| Field | Default | Description |
|-------|---------|-------------|
| `container.readonly_root` | true | Container root filesystem is read-only |
| `container.no_new_privileges` | true | Prevent privilege escalation |
| `container.run_as_user` | 1000 | UID to run as (must be ≥ 1000) |
| `container.capabilities` | [] | Required Linux capabilities (drop all, add only needed) |

## Nostr Event Format

### Event Kind

App manifests use **NIP-78 application-specific data** with event kind **30078** (replaceable parameterized). This matches the existing node discovery pattern in `nostr_discovery.rs`.

### Event Structure

```json
{
  "kind": 30078,
  "tags": [
    ["d", "archipelago-app:<app_id>"],
    ["t", "archipelago-marketplace"],
    ["t", "category:<category>"],
    ["version", "<semver>"],
    ["image", "<container_image>"],
    ["L", "archipelago"],
    ["l", "app-manifest", "archipelago"]
  ],
  "content": "<JSON-serialized manifest>",
  "created_at": 1710000000,
  "pubkey": "<developer's secp256k1 pubkey hex>",
  "sig": "<schnorr signature>"
}
```

### Tag Semantics

| Tag | Purpose |
|-----|---------|
| `d` | Unique identifier for NIP-33 replaceable events. Format: `archipelago-app:<app_id>` |
| `t` | Searchable topic tags for relay filtering |
| `version` | Allows version-specific queries |
| `image` | Container image for quick display without parsing content |
| `L`/`l` | NIP-32 labeling namespace for structured queries |

### Publishing a Manifest

1. Developer creates/updates their app manifest
2. `author.did` is filled in with the node's own `did:key` if empty. If it is
   set to a **different** DID, publishing is refused — the node can only sign as
   itself, and broadcasting a manifest every verifier will reject helps nobody
3. Canonicalise the manifest without `signatures` and SHA-256 it (see
   [Signing Protocol](#signing-protocol))
4. Sign the digest with the node's Ed25519 identity key and attach `signatures`
5. Embed the signed manifest as the Nostr event content
6. Sign the Nostr event with the node's secp256k1 Nostr key
7. Publish to all configured Nostr relays

Note the two distinct keys: the **Ed25519 identity key** proves *authorship of
the manifest* and is what `author.did` names; the **secp256k1 Nostr key** proves
*who sent this event*. They are separate on purpose — relaying is not
authorship, and only the first survives being copied between relays.

### Discovering Manifests

1. Node queries configured relays with filter:
   ```json
   {
     "kinds": [30078],
     "limit": 100,
     "#t": ["archipelago-marketplace"]
   }
   ```
2. For each returned event:
   a. Verify Nostr event signature (standard NIP-01)
   b. Parse manifest JSON from content
   c. Verify DID signature on manifest hash
   d. Check manifest against security requirements
   e. Calculate trust score
3. Return manifests sorted by trust score

## Trust Model

### Trust Score Calculation

Each discovered app receives a trust score (0-100) based on:

This table is `calculate_trust_score()` in `marketplace.rs`. What each factor
actually checks:

| Factor | Max | What is checked |
|--------|-----|-----------------|
| **Identity proven** | 30 | The manifest carries a `valid` DID signature — the author demonstrated control of the key `author.did` encodes. Requires key material; cannot be faked by choosing a string |
| **Relay consensus** | 20 | Graduated, and never zero: 1 relay → 5, 2–3 → 12, 4+ → 20 |
| **Federation trust** | 20 | `author.did` is in the user's federated DID list **and** identity is proven. Both halves are required — see below |
| **Provenance** | 15 | 10 for a 3-part semver `version`, 5 for a non-empty `repo_url`. Nothing counts published versions |
| **Security compliance** | 15 | 15 when `validate_manifest()` returns no issues, 5 when it returns 1–2, 0 otherwise |

Both identity-derived factors hang off the signature, which is the point:

- Before, "DID present" was `did.starts_with("did:")`, so an unsigned manifest
  with a plausible-looking DID string and a pinned image scored 65 — *Community*
  tier — on no cryptography whatsoever. It now scores 35, *Unverified*.
- Federation trust is gated too. An unverified `author.did` is just a string the
  publisher chose, so an attacker could otherwise copy the DID of a peer the
  user federates with and collect 20 points for impersonating precisely the
  party the user trusts most.

An unsigned publisher is not punished beyond losing those points: `missing` is a
normal state, and such apps still appear.

### Trust Tiers

| Score | Tier | UI Treatment |
|-------|------|--------------|
| 80-100 | Verified | Green badge, install with one click |
| 50-79 | Community | Yellow badge, install with confirmation |
| 20-49 | Unverified | Orange badge, install with warning dialog |
| 0-19 | Untrusted | Red badge, requires explicit security override |

### Federation-Based Trust

When a developer's DID appears in the user's federation network (trusted peer), the app automatically receives +20 trust points. This creates organic trust propagation: if you trust a node operator, you're more likely to trust their published apps.

### ADR: Nostr Relays over Centralized Registry

**Decision**: Use Nostr relays as the app discovery layer instead of a centralized registry.

**Context**: A centralized app store contradicts Archipelago's sovereignty principles. Nostr relays provide censorship-resistant, decentralized event distribution.

**Consequences**:
- (+) No single point of failure for app discovery
- (+) Developers publish without permission or review gates
- (+) Multiple relay sources increase availability
- (+) Leverages existing Nostr infrastructure and key management
- (-) No global content moderation (each node decides trust locally)
- (-) Spam is possible (mitigated by DID verification and trust scoring)
- (-) Relay availability varies (mitigated by querying multiple relays)

## Signing Protocol

### Manifest Signing (DID Layer)

**Normative.** These rules define the signed preimage byte-for-byte. An
implementation that canonicalises differently will produce signatures this node
rejects, so they are worth following exactly.

```
1. Take the manifest with `signatures` REMOVED (a signature cannot cover the
   field that holds it; omit the key entirely rather than setting it null).
2. Canonicalise to JSON:
     - every object's keys sorted lexicographically, recursively;
     - no insignificant whitespace;
     - arrays keep their order.
3. manifest_hash = SHA-256(canonical_json_bytes)
4. did_signature = Ed25519_Sign(author_private_key, manifest_hash)
      ^ the signature covers the 32 RAW DIGEST BYTES, not the "sha256:..."
        string and not the JSON itself.
5. Attach:
   {
     "signatures": {
       "manifest_hash": "sha256:<64 lowercase hex chars>",
       "did_signature": "<standard base64, RFC 4648 §4, with padding>"
     }
   }
```

The signing key MUST be the Ed25519 key that `author.did` encodes — `author.did`
is a `did:key` whose multibase body is `0xed01 || <32-byte public key>`. A
publisher signing with any other key produces a manifest that verifies as
`invalid` and is dropped.

**Why canonicalisation is required and not cosmetic.** `container.env` is a map,
and map iteration order is not stable across processes or implementations. Sign
the serialiser's natural output and the same manifest hashes differently between
runs, so signatures fail at random rather than never — much harder to diagnose
than a clean rejection. Sorting keys removes the ambiguity.

`archipelago` implements this in `marketplace::canonical_signing_bytes` /
`sign_manifest` / `verify_manifest_signature`.

### Event Signing (Nostr Layer)

Standard NIP-01 Schnorr signature over the event ID (hash of serialized event fields). This is handled by the Nostr client library.

### Verification Flow

```
Receiving Node:
  1. Verify Nostr event signature (NIP-01)       → event authenticity   [IMPLEMENTED]
  2. Extract manifest JSON from event content                           [IMPLEMENTED]
  3. Canonicalise the manifest without `signatures`, SHA-256 it         [IMPLEMENTED]
  4. Compare with manifest.signatures.manifest_hash → content integrity [IMPLEMENTED]
  5. Resolve author.did (did:key) to its Ed25519 public key             [IMPLEMENTED]
  6. Verify did_signature over the digest         → author identity     [IMPLEMENTED]
  7. Check container.image tag is pinned (not :latest)                  [ADVISORY]
  8. Validate security fields meet minimums                             [ADVISORY]
```

Steps 3–6 are `verify_manifest_signature()`, which returns one of three verdicts
rather than a boolean:

| Verdict | Meaning | What discovery does |
|---|---|---|
| `valid` | Hash matches the content **and** the key named by `author.did` signed it | Listed; earns the identity-derived trust points |
| `missing` | No `signatures` block | Listed, but scores **zero** on identity and federation. An unsigned publisher is unproven, not hostile |
| `invalid` | A `signatures` block is present and wrong — tampered, corrupt, or signed by another key | **Dropped entirely**, with the reason logged. Never cached, never installable |

That `invalid` handling is deliberate: a broken signature is not a low-quality
manifest, it is a forged or corrupted one, so it fails closed rather than
appearing with a scary badge someone can click past.

Steps 7–8 still run, but `validate_manifest()` returns a list of *issues* that
feed the trust score — they do not block discovery or installation.

## RPC Endpoints

### Marketplace Discovery

| Method | Description | Auth |
|--------|-------------|------|
| `marketplace.discover` | Query relays for app manifests, verify, score, return sorted | Local |
| `marketplace.publish` | Sign the manifest with this node's identity key, then publish to configured relays | Local |
| `marketplace.get-manifest` | Get full manifest for a specific app by ID | Local |
| `marketplace.verify` | Check a manifest's DID signature and security compliance without publishing it | Local |

`marketplace.verify` returns the signature verdict separately from the advisory
policy issues, because they mean different things:

```json
{
  "signature": { "status": "invalid", "reason": "did_signature does not verify against author.did" },
  "signature_valid": false,
  "valid": true,          // ← policy compliance only; NOT authenticity
  "issues": [],
  "trust_score": 35,
  "trust_tier": "unverified"
}
```

`valid` has always meant "passes the advisory security checks". Read
`signature_valid` for authenticity. Discovered apps carry the same verdict in
their `signature` field.

### Manifest Management

| Method | Description | Auth |
|--------|-------------|------|
| `marketplace.list-published` | List manifests published by this node | Local |

### Purchases

| Method | Description | Auth |
|--------|-------------|------|
| `marketplace.create-invoice` | Create a Lightning BOLT11 invoice for a paid app | Local |
| `marketplace.check-payment` | Poll whether an invoice has settled | Local |

`marketplace.unpublish` was specified here but **never implemented** — the
string appears nowhere in the codebase, and there is no dispatcher entry. NIP-33
replaceable events mean an unpublish would have to be a tombstone/replacement
rather than a delete, which is presumably why it stalled.

## Security Requirements

### Container Security Enforcement

`validate_manifest()` checks the following and returns them as a list of issues.
**These are score inputs, not gates** — a manifest that fails all of them is
still discoverable and installable, it just scores 0 on the security factor:

1. **No `latest` tag**: Image must use a specific version tag
2. **Read-only root**: `readonly_root` should be true
3. **No root**: `run_as_user` must be **≥ 1000** (the code's bound; the example
   manifest above uses exactly `1000`)
4. **No new privileges**: `no_new_privileges` should be true

Items previously listed here — a capability allow-list, a host-networking ban,
and system-path mount restrictions — are **not** part of marketplace validation.
Those rules exist, but they live in the runtime manifest parser
(`core/container/src/manifest.rs`, see [`app-manifest-spec.md`](app-manifest-spec.md))
and apply to `apps/*/manifest.yml`, which is a different schema from the
marketplace manifest. Closing that gap is part of the pre-third-party-publishing
work.

### Image Verification

- Container images are pulled from registries, never transferred between nodes
- Future: Cosign signature verification for container images (leverages `core/security/`)
- Image digest pinning recommended for production apps

## UI: Community Marketplace Tab

### Route

Extends existing `/dashboard/marketplace` page.

### Layout

Two tabs at the top of Marketplace.vue:

1. **Curated** (existing): Built-in apps maintained by Archipelago team
2. **Community** (new): Apps discovered from Nostr relays

### Community Tab Components

1. **App Grid**: Same card layout as curated tab, with trust score badge
2. **Search & Filter**: Category filter + text search across community apps
3. **Trust Indicators**: Color-coded badges (Verified/Community/Unverified/Untrusted)
4. **App Detail**: Shows full manifest, developer DID, relay sources, version history
5. **Install Flow**: Trust-level-dependent confirmation (one-click for Verified, warning for Untrusted)

### Publishing UI

Accessible from Settings or a "Developer" section:
1. Select a local app container to publish
2. Fill in manifest metadata (description, category, icon)
3. Review security compliance
4. Sign and publish to relays
5. View published manifests and their discovery status

## Data Storage

```
/var/lib/archipelago/marketplace/
  ├── cache/
  │   └── manifests.json      # Cached discovered manifests, trust scores included
  └── published/
      └── <app-id>.json       # Manifests published by this node
```

The earlier version of this tree also listed `cache/trust-scores.json` and
`config.json`. Neither is written: scores live on the cached entries themselves
(`MarketplaceCache`), and there is no marketplace preferences file.

## Implementation Notes

### Relay Query Strategy

1. Query all enabled relays in parallel (from `nostr_relays.rs` config), with a
   10s connect timeout and a 20s fetch timeout per relay
2. Deduplicate manifests by `app_id` + `version`
3. If the same manifest is found on multiple relays, boost trust score
4. Write results to `cache/manifests.json`

Items 4–5 of the original design — a 15-minute cache TTL and a 30-minute
background refresh — are **not implemented**. The cache has no expiry and
nothing refreshes it on a timer; it is rewritten whenever
`marketplace.discover` runs.

### Version Comparison

- Use semantic versioning for all version comparisons
- When multiple versions exist for the same `app_id`, show the latest
- Keep version history available in app detail view
- Flag apps with versions older than 6 months as potentially unmaintained
