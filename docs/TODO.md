# TODO

Working backlog of forward-looking items not yet scoped into a dedicated plan
doc. See [`ROADMAP.md`](ROADMAP.md) for the curated, public-facing direction.

## NPM certificate release blocker — reported 2026-10-01

- [ ] **NPM first certificate and renewal must work without manual repair.**
  Shorty's HTTP-01 404 exposed inconsistent active data/ACME paths between the
  manifest, legacy creation paths and host nginx bridge. Independent reviewer
  acknowledged the handoff; separate release owner must acknowledge receipt,
  implement data-safe migration and complete fresh-install, legacy-upgrade,
  issuance/renewal, restart/reboot and OTA/ISO acceptance. See
  [repair evidence and required release tests](npm-certificate-handoff-20261001.md).
  Do not equate the live challenge-route repair with a tested release fix.

## Framework incident — closed with operator acceptance

- **CLOSED WITH OPERATOR ACCEPTANCE (2026-09-30): Framework LND startup /
  missing Receive address / false zero balance.** Startup, native balances,
  Cashu address and source integration were verified; the operator accepted the
  remaining display check and authorized release. See the incident record for evidence.
  See [incident evidence and closure criteria](incident-framework-lnd-startup.md)
  and the repository `AGENTS.md` session-start instructions.

## Next release after 1.8.21 — reported 2026-09-30

Release status and acceptance gates: [execution checklist](next-release-20260930.md).

- [ ] **Release blocker: Gitea → Portainer repository integration.** Diagnose
  smart-HTTP reachability from Portainer's actual request namespace, then provide
  one declarative topology and idempotent migration for fresh installs and
  existing nodes. Preserve gate/auth boundaries, operator configuration,
  repository/key/database mounts and Portainer stacks. Cover install order,
  lifecycle/reboot/update convergence, clone/push and source-branch/Compose-file
  acceptance with a disposable integration setup. Ship in both OTA and ISO;
  a healthy Gitea root page is insufficient. Operator supplied a private handover;
  deployment addresses and credentials must not be committed.

- [ ] **New X250: GitWorkshop failed at 70%; slow Nginx installation.** Missing
  ISO build contexts restored on-node; package staging/smoke checks added.
  GitWorkshop dependency audit refreshed and build/HTTP recovery verified;
  Nginx was a slow successful image pull. Aggregate progress label corrected.
  Include the validated repair in the next OTA/ISO. See lifecycle evidence.

- [ ] **Angor indexer service in the app store**, requested after the other
  current repair/review work (2026-09-30). Follow the repository's app-development
  and packaging documentation; treat it as a headless service unless upstream
  documentation establishes a UI. Verify Bitcoin/Mempool requirements, decide
  whether an existing first-class relay meets Angor's requirements or a relay
  must be packaged with the indexer, and use the Angor logo from angor.io for its
  service icon. Official current deployment documentation located and reviewed: stock Mempool
  plus an optional strfry relay. Both headless services and the dependency guard
  are implemented; API outage/recovery and five relay lifecycle cycles passed.
  Final candidate install/lifecycle checks and signed delivery remain pending.
  Install on the development box; Bitcoin must finish syncing for indexed queries.

- [ ] **App lifecycle: keep installed apps visible through restart and hard
  refresh; gate embedded/browser launches on actual web and listener readiness.**
  Source repair and scoped live acceptance passed; full release gate pending.
  Includes durable inventory reconstruction,
  concurrent inventory writes, stale scan/lifecycle updates, delayed HTTP startup,
  and the app gate's post-install listener delay. See
  [app lifecycle repair evidence](app-lifecycle-repair-20260930.md).

- [x] Review and repair open paid-download PRs #161 and #162, refresh both
  branches from main, run independent and combined isolated suites, and verify
  rootless file permissions in disposable scratch storage. Combined result:
  1,585 passed, zero failed, four existing tests ignored. See the
  [review evidence and remaining acceptance work](pr-review-20260930.md).
- [x] Integrate the reviewed PR branches into the next release and run funded
  candidate acceptance, including Tor-only transport and payments with change.
  Operator authorized completing the normal merge/closure workflow on
  2026-09-30. Both PRs are now merged and closed through Gitea; integrate
  local repair commits and sync git/ngit before release. The combined candidate
  is deployed on both test endpoints. Funded Tor-only purchase with change,
  confirmed refund, exact Files bytes and zero-cost repeat delivery passed.
  The updated source still needs inclusion in signed OTA/ISO artifacts.
- [ ] Design durable recovery for an accepted payment whose response is lost.
  Preserve the truthful unconfirmed-refund warning and prevent automatic
  duplicate payment while that recovery work is outstanding.
- [x] **ThinkPad X250 kiosk: Bitcoin version choices readable above pruning.**
  Replaced the native popup with inline radio choices. Actual Chromium 152 kiosk
  assertions and screenshot verify white-on-dark choices, selection changes and
  layout above pruning controls. Focused component tests pass. Included in the
  next-release source; published 1.8.21 artifacts remain unchanged.

## 1.8.21 repair and release tasks — completed 2026-09-30

See the [execution record](repair-release-20260929.md) for evidence and limits.

- [x] Fix Cashu paid-file redemption between dev and Shorty; test keyset IDs,
  mint errors, fees, and refund reporting before live validation.
- [x] Record Framework verification and the operator's acceptance of the
  remaining display check before release.
- [x] Replace the unavailable tx1138.com explorer default with mempool.space;
  migrate the old default with fresh consent and preserve custom/local explorers.
- [x] Offer pruning in the Bitcoin installation version modal, using the same
  pruning settings as automatic pruning even on large disks.
- [x] Explain Bitcoin warmup without raw RPC errors; gate LND unlock on Bitcoin
  RPC readiness and show install/start/sync waiting states with automatic recovery.
- [x] Test the completed changes on this development box, then publish a new
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
