# TODO

Working backlog of forward-looking items not yet scoped into a dedicated plan
doc. See [`ROADMAP.md`](ROADMAP.md) for the curated, public-facing direction.

## Blocking incident — before unrelated work

- **OPEN: Framework LND startup / missing Receive address / false zero balance.**
  User requires investigation and a verified fix on the actual node before later
  unrelated work. Startup and native balances were verified on the actual node;
  final display confirmation is pending. See the incident record for evidence.
  See [incident evidence and closure criteria](incident-framework-lnd-startup.md)
  and the repository `AGENTS.md` session-start instructions.

## Current repair and release tasks — 2026-09-29

Release is blocked until these pass; see [execution record](repair-release-20260929.md).

- [ ] Fix Cashu paid-file redemption between dev and Shorty; test keyset IDs,
  mint errors, fees, and refund reporting before live validation.
- [ ] Complete the remaining Framework incident verification and evidence.
- [ ] Replace the unavailable tx1138.com explorer default with mempool.space;
  migrate the old default with fresh consent and preserve custom/local explorers.
- [ ] Offer pruning in the Bitcoin installation version modal, using the same
  pruning settings as automatic pruning even on large disks.
- [ ] Explain Bitcoin warmup without raw RPC errors; gate LND unlock on Bitcoin
  RPC readiness and show install/start/sync waiting states with automatic recovery.
- [ ] Test the completed changes on this development box, then publish a new
  signed OTA and raw ISO release. Record any remaining verification gaps.

## Dev & build process (priority)

- Formalize the contributor workflow: releases, CI, maintainers, automated
  builds, PR/issue flow, branch naming, and reproducible builds.

## Federation & peering

- Peering trust model — define tiers (trusted / public / private / peered)
  on top of the existing federation DID trust levels.
- Federation architecture built on the above peering model.

## Distributed git & OTA

- Nostr-hosted git for the alpha (see
  [`nostr-git-source-hosting.md`](nostr-git-source-hosting.md)).
- Distributed git beyond the nostr-hosting case.
- Distributed OTA / app delivery.

## Nostr integration

- Nostr signer integration.

## Platform / OS

- Source-availability ISO — define the build/distribution story.
- HW/OS update pipeline.
- Deeper OpenWRT integration.
- GrapheneOS integration — backups, attestation, profiles.

## App ecosystem

- Full pass testing every app in the catalog; expect issues across the board.
- App update strategy — finalize the update policy referenced in
  [`app-developer-guide.md`](app-developer-guide.md) (pinned vs. mutable
  tags, catalog-vs-disk precedence, rollout/rollback).
- App wishlist — candidates not yet packaged: Cashu wallet, phoenixd.
  (CLN is already shipped as `apps/core-lightning`.)

## Access & security

- SSH access strategy — define the access model (keys, rotation, recovery
  path, remote-support access).

## Observability

- Capture error logs to troubleshoot customer issues.
- Stats & visualization for traffic, blocked attacks, VPNs, routing.
