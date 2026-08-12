# Companion app pairing QR — integration handoff

**Status:** web-UI side SHIPPED (CompanionIntroOverlay.vue, 2026-07-16). This doc is
the contract + requirements for the companion-app side (worked on separately, on
the Mac).

## What the web UI now does

The "Remote Companion" intro modal (shown once after first dashboard login, and in
the public demo) gained a second screen:

1. **Screen 1 (existing):** APK download QR (desktop) / download button, plus a new
   **"I've installed it"** button to the right of the download button.
2. **Screen 2 (new, slide transition):** a **pairing QR** the companion app scans to
   auto-fill the server entry, with a **Back** button returning to screen 1. On
   small screens (where you can't scan your own display) an
   **"Open in companion app"** deep-link button is shown above Back, using the same
   URI as the QR.

## The QR payload / deep link (the contract)

A single URI, also usable as an OS deep link:

```
archipelago://pair?v=1&url=<origin>&name=<display name>[&tok=<device token>][&pw=<password>][&fnpub=…&fip=…&fhost=…&fudp=…&ftcp=…]
```

Query parameters:

| param | required | meaning |
|-------|----------|---------|
| `v`   | yes      | Payload version, currently `1`. Reject/ignore unknown majors gracefully — show "please update the app". |
| `url` | yes      | Full origin the app should connect to, scheme included: `https://demo.archipelago-foundation.org`, `http://archipelago.local`, `http://192.0.2.10`, etc. No trailing slash guaranteed either way — normalize. |
| `name`| no       | Display name for the server entry. Real nodes send the configured server name, or `My Archipelago` when it's still the factory default. |
| `tok` | no       | **Device token** minted via `auth.createDeviceToken` when the QR is rendered. The app logs in with `{"method":"auth.login","params":{"token":"…"}}` — same endpoint, same rate limiter, skips TOTP (the token was minted from an authenticated session). Long-lived until re-minted (re-showing the pair screen replaces the `companion` token) or revoked (`auth.revokeDeviceToken`). Scan → instantly connected, no typing. |
| `pw`  | no       | Login password. **Only present in the public demo** (shared demo password `entertoexit`). Real nodes never embed a password — the frontend doesn't have it. |
| `fnpub` | no     | Node's FIPS mesh identity (bech32 npub of the daemon's seed-derived key). Presence of this param means "this node speaks FIPS — mesh with it". |
| `fip` | no       | Node's `fips0` ULA (IPv6). Once the phone is meshed, the node's UI stays reachable at `http://[<fip>]` from anywhere — this is the remote-access address (replaces the old WireGuard 10.44.0.1 flow). |
| `fhost` | no     | Host the phone's embedded FIPS dials (same host `url` resolved to). |
| `fudp` / `ftcp` | no | Mesh transport ports on `fhost` (currently 2121/udp and 8443/tcp). |
| `fanchors` | no | Comma-joined `npub@host:port/transport` rendezvous anchors, capped at 4. **The FIRST entry is the paired node itself** (`fnpub` at its current LAN host — the addr is a dial *hint*, the npub is the identity). The remaining entries are the node's seed anchors, and the node guarantees the Archipelago public anchor (vps2, `146.59.87.168:8444/tcp`) is present even if the operator trimmed their own list — so the phone can always rendezvous through the public mesh when the LAN endpoint is unreachable (away from home / NAT). |

Examples the web UI actually emits:

- Demo: `archipelago://pair?v=1&url=https%3A%2F%2Fdemo.archipelago-foundation.org&pw=entertoexit`
- Real node, browsed via LAN IP: `archipelago://pair?v=1&url=http%3A%2F%2F192.0.2.10`
- Real node kiosk (UI runs on localhost, so it advertises the mDNS name from
  `system.get-hostname`): `archipelago://pair?v=1&url=http%3A%2F%2Farchipelago.local`

## Companion app requirements

1. **Scan entry point:** the app's action is labeled **"Scan Node's QR"**
   (implemented 2026-07-17; the modal copy in CompanionIntroOverlay.vue was
   updated to match).
2. **Parse the URI** (from camera scan AND from an OS deep-link intent —
   register the `archipelago://` scheme so the "Open in companion app" button on
   phones works).
3. On success, **create/update a saved server entry**:
   - Server address = `url` exactly as given (respect the scheme — the demo is
     https, LAN nodes are typically http, `.local` mDNS names must work).
   - If `pw` present, prefill the password and attempt auto-login; otherwise land
     on the password prompt for that server.
   - If an entry with the same origin already exists, update it rather than
     duplicating.
4. **Demo flow (the showcase):** scanning the demo QR should take a fresh install
   to a logged-in demo session in one step — url `https://demo.archipelago-foundation.org`,
   password `entertoexit`, no manual typing.
5. **Robustness:**
   - Tolerate unknown extra query params (forward compat — we may add `name`,
     `cert` fingerprint, etc. under `v=1`).
   - Self-signed HTTPS on `.local`/LAN addresses may appear later; don't hard-fail
     the parse on scheme.
   - Bad/foreign QR → clear error, stay on the scan screen.

## Landed extensions (2026-07-22)

- **Device token** (`tok`) — real nodes now pair instantly; see the param table.
  Minting replaces the previous `companion` token, so merely re-opening the pair
  screen invalidates a previously issued token (the phone's session/remember
  cookies keep working; a re-scan re-pairs).
- **`name`** — the app labels the entry with the node's server name
  ("My Archipelago" when unset).
- **FIPS mesh params** (`fnpub`/`fip`/`fhost`/`fudp`/`ftcp`/`fanchors`) — the
  companion app embeds a leaf-only FIPS node (Android/rust/archy-fips-core)
  behind a split-tunnel VpnService and dials the node + rendezvous anchors on
  scan. This replaces the WireGuard install/tunnel onboarding screens entirely;
  remote access = the node's fips0 ULA (`fip`), which the WebView falls back to
  automatically when the LAN address stops answering.

## npub-first connectivity (2026-07-23 — REQUIRED app-side changes)

The QR used to be effectively IP-first: the app connected to `url`/`fhost` and
broke as soon as the LAN renumbered or the phone left home. FIPS peers on
**npubs**; IPs are only dial hints. The app must treat them that way:

1. **Identity = `fnpub`.** The saved server entry is keyed by the node's npub
   (fall back to origin only when the QR has no FIPS params). Re-scanning the
   same npub updates the entry even if every address changed.
2. **Peer with the node itself as the first anchor** (`fanchors[0]`): on LAN
   this is direct p2p over FIPS (mDNS/known-endpoint dial via archy-fips-core
   v0.4+, which discovers LAN peers without any IP pinning), so it keeps
   working after DHCP renumbering.
3. **Peer with the public anchors too** (remaining `fanchors` entries — the
   Archipelago vps2 anchor is always included by the node): away from LAN the
   phone routes to the node's npub via the public mesh and reaches the UI at
   `http://[<fip>]` (the fips0 ULA).
4. **Address selection order** for the WebView: LAN `url` when it answers →
   ULA `fip` over the mesh otherwise. Never hard-fail because the scanned LAN
   IP stopped existing.

(Node-side counterpart shipped 2026-07-23: `fips.pair-info` always includes
the vps2 public anchor, and the web UI prepends the node's self-anchor to
`fanchors`.)

App-side status: items 2/3 were already covered by `FipsPreferences.
upsertNodePeer` (npub-matched peers, self-anchor dedup) and item 4 by the
WebView's `meshFallbackUrl` retry. Item 1 shipped 2026-07-23: `ServerEntry`
carries `npub` (trailing serialization field, legacy entries still parse) and
`ServerPreferences` matches saved/active entries via `sameNode` — npub first,
address/port/scheme only as the LAN-only fallback.

## Testing checklist (app side)

- [ ] Scan demo QR from https://demo.archipelago-foundation.org → auto-connected demo session.
- [ ] Scan a real node's QR (LAN IP origin) → entry created, password prompt shown.
- [ ] Scan a kiosk node's QR (`http://<name>.local`) → mDNS resolution works on the phone.
- [ ] Tap "Open in companion app" on a phone browser → deep link opens the app with the same behavior.
- [ ] Re-scan same node → no duplicate entry.
