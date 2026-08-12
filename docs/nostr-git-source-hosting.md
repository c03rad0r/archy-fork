# Nostr Git Source Hosting Plan

This plan describes how Archipelago can publish and accept contributions to its
source code through `ngit`, NIP-34, and GRASP while keeping the developer
experience inside Archipelago.

## Goals

- Publish Archipelago source from a sanitized, fresh-history repository.
- Make the in-app registry the primary onboarding path for contributors.
- Let contributors clone, branch, push PR branches, open PRs, and discuss issues
  with a Nostr identity from their Archipelago node.
- Follow the Bitcoin Core development model: broad public review and easy forks,
  with canonical merge authority held by a small maintainer set.
- Give contributors full read, fork, and proposal rights, but no direct merge
  rights on the canonical repository.
- Keep the official maintainer identity and merge authority separate from user
  node identities.

## Current Building Blocks

Archipelago already has most of the primitives needed for this:

- App manifests and the app registry already install developer tooling as
  rootless Podman apps.
- The `gitea` app provides a conventional fallback Git UI and package registry.
- The app launcher already exposes a consent-gated NIP-07 bridge for launched
  apps using `getPublicKey`, `signEvent`, NIP-04, and NIP-44 requests.
- The backend exposes node and identity Nostr signing RPC methods.
- FIPS gives nodes a stable mesh identity and private transport path, but repo
  announcements and PRs should remain NIP-34 compatible on normal Nostr relays.
- DWN protocol registration exists and can be used later for local contribution
  metadata/cache, but should not be required for the first public workflow.

## Protocol Basis

Use existing Nostr Git conventions rather than inventing an Archipelago-only
protocol:

- NIP-34 repository announcement events identify repositories with kind `30617`.
- NIP-34 repository state events publish branch/tag refs with kind `30618`.
- NIP-34 patches, pull requests, PR updates, issues, and status events use kinds
  `1617`, `1618`, `1619`, `1621`, and `1630`-`1633`.
- `ngit` provides the `git-remote-nostr` helper for `nostr://` clone URLs and PR
  branches.
- GRASP servers provide Git Smart HTTP storage while Nostr events remain the
  authority for repository identity, refs, PRs, issues, and maintainer state.

Primary references:

- https://nips.nostr.com/34
- https://docs.rs/crate/ngit/latest/source/README.md
- https://ngit.dev/grasp/

## Recommended Architecture

### Apps

Create two first-party apps:

- `ngit`: CLI/runtime package containing `ngit` and `git-remote-nostr`.
- `archipelago-source`: web UI for cloning Archipelago source, viewing NIP-34
  issues/PRs, opening branches, and submitting PR events.

The `archipelago-source` app should depend on `ngit`. It can also recommend
Gitea for users who want a conventional local web Git UI, but Gitea should not
be the source of truth for public contribution permissions.

### Contributor Onboarding

When the user installs `archipelago-source` from the registry:

1. Show a modal before first launch: "Contribute to Archipelago".
2. Explain that the app will use their Archipelago Nostr identity to clone and
   sign contribution events.
3. Display the maintainer repository announcement, clone URL, maintainer npub,
   and relay/GRASP endpoints.
4. Ask for consent to:
   - fetch repository metadata from configured relays,
   - clone source through `nostr://`,
   - create local branches,
   - sign NIP-34 issue/PR/comment events,
   - push PR branches to approved GRASP servers.
5. Store approval per app origin, identity id, repository id, and relay set.

This should build on the existing NIP-07 app-launcher bridge, but use a more
specific permission scope than the generic sign-event approval.

### Identity And Permissions

Use four identity classes:

- `archipelago-maintainer`: an offline or tightly controlled Nostr key that
  signs the canonical kind `30617` repo announcement and status/merge events.
- `archipelago-merge-maintainer`: one of the small set of maintainer npubs
  allowed to advance canonical refs and publish valid merged/applied status.
- `archipelago-build`: release automation key for signed release artifacts and
  CI status events. It must not have merge authority.
- `contributor`: user node or app-specific identity used for PRs, issues, and
  comments.

Contributor rights:

- Clone the repository.
- Open issues.
- Push proposal branches using `pr/<npub>/<short-topic>` or `pr/<event-id>`.
- Publish NIP-34 PR/update/comment events.
- Rebase and update their own PR branch.
- Run local validation and attach status evidence.

Contributor restrictions:

- Cannot update `refs/heads/main` or release branches in canonical state.
- Cannot publish maintainer-valid merge/applied status.
- Cannot alter the canonical repository announcement.
- Cannot publish release catalog signatures.

Maintainer rights:

- Publish/update the canonical repo announcement.
- Publish canonical `refs/heads/main` state.
- Mark PRs merged/closed/draft via NIP-34 status events.
- Sign release tags and catalog updates.

Fork rights:

- Any contributor can create their own NIP-34 kind `30617` repository
  announcement for a fork.
- Fork announcements should use the NIP-34 `u` tag to point back to the
  canonical `archy` repository.
- The source app should make forking a first-class path: "Fork on Nostr", clone
  the fork locally, push branches to the contributor's GRASP list, and open PRs
  back to canonical Archipelago when they want review.
- Forks can have their own maintainer npubs, relays, policies, and release
  cadence, but the app should clearly label them as forks unless signed by the
  canonical maintainer set.

The GRASP server policy should enforce this by accepting pushes to maintainer
refs only when backed by signed maintainer state, while allowing contributor PR
refs from their own npubs.

## Repository Layout

Canonical repo announcement:

- repo id: `archy`
- display name: `Archipelago`
- clone URLs:
  - `nostr://<maintainer-npub>/<relay-hint>/archy`
  - `https://<grasp-host>/<maintainer-npub>/archy.git`
- relays:
  - Archipelago-operated relay
  - at least two public Nostr relays that support the event load
- GRASP servers:
  - Archipelago-operated GRASP instance
  - one public GRASP-compatible mirror

Keep the existing HTTP Git remote as a mirror during launch. The docs can
present `nostr://` as the preferred contribution path once the workflow is
proven.

## UI Requirements

The source app should provide:

- A first-run contribution modal with a real Archipelago source graphic, not a
  generic text-only dialog.
- Current clone status and local path.
- Branch list, changed files, commit form, and push/open-PR flow.
- PR inbox, issue list, maintainer status, and relay health.
- Explicit identity indicator showing which npub will sign events.
- A merge rights indicator that clearly says contributors can propose changes
  but cannot merge them.
- A fork flow that creates a user-owned NIP-34 repo announcement and remote,
  then offers "Open PR to Archipelago" from any fork branch.
- Maintainer badges based only on pinned canonical maintainer npubs, not relay
  metadata or server-side account names.
- Links to container docs, deployment docs, manifest spec, and open-source
  readiness tasks.

## Backend Work

Add an RPC module for source contribution workflow:

- `source.repo-info`: returns canonical announcement, clone URL, relay set,
  maintainer npubs, and local clone state.
- `source.ensure-ngit`: verifies the `ngit` app/runtime is installed.
- `source.clone`: clones or updates the local source checkout.
- `source.status`: returns branch, dirty files, ahead/behind, and PR state.
- `source.commit`: creates a local commit from selected files.
- `source.fork`: creates a contributor-owned NIP-34 fork announcement and local
  remote.
- `source.open-pr`: pushes a PR branch and publishes a kind `1618` event.
- `source.update-pr`: updates the branch and publishes kind `1619`.
- `source.issue`: publishes a kind `1621` event.

Backend must shell out through a narrow command wrapper, never arbitrary user
commands. The wrapper should set an isolated working tree under
`/var/lib/archipelago/source/archy`, run as the Archipelago service user, and
deny operations outside that path.

## Security Model

- Never expose maintainer private keys to an Archipelago node.
- Prefer app-specific contributor identities over the node's default identity.
- Require per-action consent for first PR push, issue creation, and signing any
  event that tags the canonical repository.
- Pin the canonical maintainer npub in the app manifest and backend config.
- Keep the canonical merge-maintainer allow list signed by the
  `archipelago-maintainer` key; never infer merge rights from GRASP server
  accounts.
- Verify the canonical kind `30617` event signature before displaying clone
  instructions.
- Treat GRASP servers as untrusted storage; verify Git refs against signed
  Nostr state.
- Do not use destructive git operations from the UI without an explicit modal.
- Store local clones and generated patches outside app container writable roots
  unless the user exports them.

## MVP

1. Package `ngit` as a first-party app.
2. Stand up one Archipelago-operated GRASP server and one Nostr relay.
3. Publish sanitized fresh-history `archy` through `ngit init`.
4. Add a simple `archipelago-source` app that clones source and links out to the
   preferred Nostr Git browser.
5. Add app-launcher consent scopes for repository-specific NIP-34 signing.
6. Allow issues and PR branch submission from contributor npubs.
7. Add a one-click fork flow that publishes a contributor-owned fork
   announcement referencing canonical Archipelago.
8. Keep maintainer merge/status publication manual.

## Later

- Native PR review UI with file diffs and inline comments.
- CI status events signed by the build identity.
- FIPS-first source sync between trusted Archipelago nodes.
- Private prerelease repositories using NIP-42 allow lists and/or protected
  events if the ecosystem support is mature enough.
- Multi-maintainer policy with threshold signatures or explicit maintainer-list
  rotation events.

## Open Questions

- Which maintainer npub should become canonical for `archy`?
- Should contributor identities be node-default or app-specific by default?
- Which GRASP implementation should be deployed first: `ngit-grasp` or another
  NIP-34/GRASP-compatible relay?
- Should the source app include a full web Git UI in v1, or launch Gitea/ngit
  browser links for review while keeping signing/submission native?
- What exact license and contribution certificate should contributors accept
  before submitting PR events?
