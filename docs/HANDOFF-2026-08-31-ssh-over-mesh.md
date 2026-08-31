# HANDOFF — SSH over the FIPS mesh (node-side toggle), 2026-08-31

**For: the node OS agent.** From the companion agent, mid-0.5.28 testing. The
user wants to SSH their node from Termux over the phone's FIPS mesh instead
of keeping Tailscale around for it — the phone side is done and verified; the
remaining work is all node-side, and it wants to be a **first-class settings
toggle**, not a hand-edited firewall rule.

## What already works (do not rebuild this)

- The companion's embedded mesh is a **device-wide split tunnel**
  (`ArchyVpnService` routes `fd00::/8` for the whole phone, no per-app
  filter, `allowBypass`). Termux — or any app — reaches mesh addresses with
  zero setup while the tunnel is up, on-LAN and away (anchor path).
- The hub's Nodes page now **displays and copies each FIPS node's `fips0`
  ULA** (committed on `companion/0.5.28`).
- Verified live today: `ssh user@<node-ULA>` from Termux answers **RST** —
  the path works end-to-end; something on the node is doing the refusing.

## The diagnosis (from today's field test + code read)

1. **`fips0` is default-deny inbound.** The hardening baseline
   (`/etc/fips/fips.nft`, provisioned out-of-band) rejects un-allowlisted
   ports with RST — the exact symptom the web-UI drop-in's comment documents
   on :80 (`core/archipelago/src/fips/config.rs` ~L237). The daemon's own
   drop-ins (`/etc/fips/fips.d/80-web-ui.nft`: 80/8443/5679,
   `85-app-ports.nft`: app launch ports) **do not include 22**.
2. **sshd IPv6 listening is unverified.** `fips0` is IPv6-only; a sshd pinned
   to `ListenAddress 0.0.0.0` RSTs on the ULA identically. The image installs
   and enables openssh-server (`image-recipe/archipelago-scripts/install-to-disk.sh`
   L177/L210) with default config (binds `::`), but a preflight in the toggle
   should confirm rather than assume.

**Interim manual unblock (what the user can do today, keep valid):**
`/etc/fips/fips.d/90-ssh.nft` containing `ip6 saddr <phone-ULA> tcp dport 22
accept`, then `sudo nft -f /etc/fips/fips.nft`. A daemon-owned toggle must
**own that file name/lifecycle** so a hand-added rule and the feature don't
fight over the same slot.

## The ask: a "SSH over mesh" toggle

The user's instinct (seconded here): **a setting in the FIPS/network area of
the node UI**, default **off**. Sketch:

- **UI**: a small settings card in the pattern of
  `neode-ui/src/views/settings/` (see `TransportPrefsCard.vue` for a
  segmented-pref card + vitest). Toggle + a source-scope selector +
  preflight status rows.
- **RPC**: `fips.ssh-over-mesh.get` / `fips.ssh-over-mesh.set` (dispatch arm
  in `core/archipelago/src/api/rpc/dispatcher.rs` alongside the existing
  `fips.*` arms at ~L544; handler in `api/rpc/fips.rs`). Persisted with the
  other fips daemon-config state.
- **Enforcement**: mirror the existing drop-in lifecycle in
  `core/archipelago/src/fips/config.rs` (~L243–320): when the toggle is on,
  write `/etc/fips/fips.d/90-ssh.nft` on every daemon config install and on
  toggle change; when off, remove it. Reload stays
  `sudo nft -f /etc/fips/fips.nft`. Never touch `80-web-ui.nft` /
  `85-app-ports.nft`.
- **Source scope** (the design decision worth an issue thread):
  - *Paired phones only* — restricts to the phone ULAs/npubs the node has
    actually paired with. Open question: does the node durably know which
    inbound peers are "its" phones? FIPS accepts inbound peers without prior
    registration, so this may need a small persisted "trusted peers" list
    (seeded when `fips.pair-info` is issued, or on first successful dial).
    Recommended default if the data can be made reliable.
  - *Custom source list* — raw ULA list, per-rule `ip6 saddr <ula> …`
    entries. Escape hatch; fine to ship alongside.
  - *Any mesh peer* — what the user literally asked for, but flag it
    honestly in the UI: with no registration requirement, this faces port 22
    at every peer that can route to the node over the mesh. If offered at
    all, gate it behind the same "I understand" confirmation pattern as
    other danger-zone settings.
- **Preflights, surfaced in the card**: sshd enabled + listening on IPv6
  (`[::]:22` or `*:22` via `ss -tln`), and whether
  `PasswordAuthentication` is on — if it is, show a keys-only recommendation
  (the firewall restriction is the belt; this is the suspenders).

## Acceptance (on-device)

- [ ] Toggle on, phone on LAN: `ssh user@<node-ULA>` from Termux connects.
- [ ] Phone away from LAN (anchor path): same result.
- [ ] Toggle off: connection refused again; `90-ssh.nft` gone.
- [ ] Daemon config install (upgrade/restart) preserves the on-state and
      the rule; nothing duplicated.
- [ ] Non-default source scope actually restricts (try from a second mesh
      peer, or a wrong ULA).
- [ ] Settings UI survives a page reload; RPC has a vitest like
      `TransportPrefsCard.test.ts`.

## Addendum (2026-08-31, same day): the npub IS the address

While wiring this up we confirmed the mesh ULA is a **pure function of the
public key** — `fd ‖ sha256(x-only pubkey)[0..15]` (`fips/src/identity/node_addr.rs`
`from_pubkey` → `identity/address.rs` `from_node_addr`,
`FIPS_ADDRESS_PREFIX = 0xfd`). The daemon's DNS resolver (`fips/dial.rs`) just
answers what anyone can compute. Consequences for the node side:

- Docs/UI can advertise `ssh <user>@npub1…`-style addressing: Termux's
  `Android/tools/fipssh` (shipped with the companion work) derives the ULA
  from the npub with zero infrastructure, verified byte-identical against
  the fips crate (`archy-fips-core` test
  `npub_derives_the_same_mesh_ula_as_the_fips_identity`).
- If the settings toggle from this handover ever grows a "copy command"
  affordance, `fipssh <user>@<npub>` is the natural shape (npub, not ULA —
  it is the durable identity; the ULA follows from it).
- No node-side DNS surface is required for the SSH case; the resolver stays
  what it is today (the node's own peer dials).

## Working rules

Same as the queue handoffs: small commits, tracker issue for this feature
(`ssh-over-mesh`), and the companion agent is downstream-only here — no
companion changes are required (the phone already routes and displays the
ULA). Optional nicety later, NOT part of this issue: the companion's FIPS
hub page could one day surface the toggle state — only worth it if the
`fips.ssh-over-mesh.get` RPC is trivial to add to the existing status call.
