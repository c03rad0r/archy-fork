# TODO

Working backlog of forward-looking items not yet scoped into a dedicated plan
doc. See [`ROADMAP.md`](ROADMAP.md) for the curated, public-facing direction.

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

## Observability

- Capture error logs to troubleshoot customer issues.
- Stats & visualization for traffic, blocked attacks, VPNs, routing.
