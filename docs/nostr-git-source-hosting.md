# Nostr Git Source Hosting Plan

**Reviewed:** 2026-09-08

**Status:** GitWorkshop integration is deployed and engineering-tested on the
development node, ready for owner UAT. Canonical repository publication and
release work remain separate gates. No app-registry, OTA, ISO, or production
artifact may be published until the owner accepts the node deployment.

The Android companion opens Source as a top-level page in its native in-app
WebView. GitWorkshop's injected NIP-07 provider creates a small authenticated
dashboard-origin signer broker within that page, so the app itself is never
kept in a dashboard iframe. The App Store carries the upstream GitWorkshop
icon, source-focused copy, and a dedicated contribution banner. Popular ordering and promotional
placement are registry-owned `storefront` metadata rather than node-OS UI
policy; these are also part of owner UAT.

## Goal

Archipelago users can install a Source app from the app registry, obtain the
Archipelago source, browse it, and contribute through the established Nostr Git
ecosystem. Git remains the version-control engine, Nostr NIP-34 carries
repository identity and collaboration events, and GRASP transports Git objects.

The app must make public contribution easy without giving contributors direct
merge or release authority. Canonical refs, merge status, release tags, and
catalog signatures remain controlled by explicitly configured Archipelago
maintainers.

## Product Decision

Archipelago will package the upstream GitWorkshop web client instead of
building another NIP-34 repository interface.

GitWorkshop already provides repository discovery, a sparse Git explorer,
issues, pull requests, and review workflows. Archipelago owns only the node
integration around it:

- installable app metadata and a pinned upstream build;
- a same-origin `/app/archipelago-source/` launch path that works through the
  dashboard address the user already opened, whether that is LAN, Tailscale,
  FIPS, DNS, IPv4, or IPv6;
- authenticated routing through the existing app gate;
- an injected, consent-gated NIP-07 provider so GitWorkshop can use a selected
  node identity without receiving its private key;
- source provenance, security validation, upgrades, and rollback.

Archipelago will not duplicate GitWorkshop's repository browser, issue/PR,
fork, diff, relay, or GRASP behavior in private `source.*` RPC methods.

Primary references:

- https://ngit.dev/how-it-works
- https://github.com/DanConwayDev/gitworkshop
- https://gitworkshop.dev/
- https://nips.nostr.com/34

## Trust And Permissions

- GitWorkshop runs as a static, read-only container behind the app gate.
- The iframe may request NIP-07 operations through `postMessage`; only the
  exact launched frame and expected origin are accepted.
- `getPublicKey`, event signing, encryption, and decryption require explicit
  dashboard consent. A remembered decision is scoped to node origin, app,
  selected identity, and method.
- Contributor private keys never enter the GitWorkshop container.
- Browser-origin signing calls from direct high-port app origins are rejected;
  they must pass through the dashboard consent bridge.
- Maintainer and release keys must not be placed on ordinary user nodes.
- Relay and GRASP data is untrusted. Canonical status is derived only from the
  signed repository announcement and configured maintainer identities.

## Upstream Pin And Redistribution Gate

The development image currently pins GitWorkshop commit
`dc36db64f6a2cca29d109829eabaf0a49d4bf4da` (2026-07-28). The integration patch
only adds base-path support and the Archipelago NIP-07 provider.

The pinned revision and current upstream `main` have no license file, the npm
package metadata declares no license, and GitHub reports no detected license.
An earlier project-site description of “MIT” is not a license grant bundled
with the code. Local engineering and owner evaluation may continue, but the
compiled image must not be published to the production app registry until its
redistribution terms are unambiguous.

Preferred resolution: ask upstream to add an SPDX-recognized license file
(MIT if that remains their intent), then re-pin at or after that commit and add
GitWorkshop plus its copyright/license notice to Archipelago's `NOTICE` and
generated image inventory. A written grant that explicitly permits compiling,
modifying, and redistributing this app is an alternative, but is harder for
downstream users to audit. A public GitHub repository or permission to fork is
not sufficient redistribution permission. Production dependency-audit findings
must also be resolved or explicitly accepted before release.

The release-preparation audit on 2026-09-09 ran `npm audit --omit=dev` against
the exact pinned commit and reported 4 high and 6 moderate advisories, with
fixes available for every affected package. The same commit remains upstream
`main`, so repinning alone does not resolve them. The final runtime image is
static nginx rather than Node, which makes the Hono server findings unlikely to
be runtime-reachable, but browser/runtime dependencies such as `fflate` and
React Router still require an upstream dependency update or an explicit,
written risk acceptance before registry publication.

## Canonical Archipelago Repository

The canonical announcement maintainer is
`npub1w3sqdkrhn0gyuvsex32effzgnfpyde6qrrc4u467flg5e9txh4wsfn5vjg`.
The repository already contains a root MIT `LICENSE` and `CONTRIBUTING.md`;
contributors agree to license their contributions under that MIT License.

The user-facing Source app can ship for local evaluation before the canonical
Archipelago Nostr repository exists, but it must not pretend a placeholder is
canonical. Canonical launch requires:

1. A sanitized public `archy` source repository.
2. An offline or tightly controlled maintainer identity.
3. A signed NIP-34 kind `30617` repository announcement.
4. At least one Archipelago-operated relay/GRASP endpoint and one independent
   compatible mirror.
5. Tested `nostr://` clone, proposal, update, review, merge-status, server-loss,
   and recovery flows.
6. A GitWorkshop link/configuration that opens the verified `archy` repository.

The existing HTTP Git remote remains a fallback until those drills pass.

## Delivery Milestones

### 1. Plan And Protocol Review — complete

- Confirmed NIP-34/ngit/GRASP as the interoperability layer.
- Defined contributor, maintainer, build, and release trust boundaries.
- Confirmed that installation must ultimately come from the Archipelago app
  registry and include a path to the upstream/source code.

### 2. Runtime Feasibility — complete

- Validated pinned `ngit` and `git-remote-nostr` binaries on supported node
  architectures.
- Exercised public `nostr://` discovery/clone behavior.
- Established that app lifecycle dependencies do not share executables or
  filesystems, avoiding an invalid two-container CLI design.

These CLI checks remain useful for canonical repository operations and release
validation; they are not a reason to build a second browser client.

### 3. Node Integration Foundation — complete

- Added the installable app manifest, catalog metadata, icon, and port
  reservation.
- Added identity selection and a generic consent-gated NIP-07 bridge.
- Kept signing secrets out of the app container.

### 4. GitWorkshop Pivot — complete on the development node

- Replace the prototype Source UI and all private `source.*` APIs with the
  pinned upstream GitWorkshop build.
- Mount it below `/app/archipelago-source/` and proxy to the authenticated app
  gate, eliminating hard-coded address and high-port launch behavior.
- Validate upstream base-path routing, static assets, browser refresh/deep
  links, NIP-07 requests, container hardening, and install/restart behavior.
- Deploy the resulting daemon, dashboard, and app only on this development
  node, then hand it to the owner for UAT.

The pinned integration patch applies cleanly to a fresh upstream checkout. The
upstream unit suite passes 152 tests, Archipelago's full frontend suite passes
1,091 tests across 137 files, the production dashboard build and Android UAT
lint/build pass, and the manifest passes all 16 validators. The read-only,
capability-free container passes health, asset, manifest, and base-path checks.
The live same-origin route reaches the authenticated app gate through the
node's loopback, LAN, Tailscale, and FIPS addresses. A rollback snapshot is at
`/var/backups/archipelago/pre-uat-fixes-20260908-1140` on the development node.

### 5. Owner UAT — pending owner action

The owner validates install, launch, navigation, repository discovery, identity
selection, consent prompts, source browsing, and available contribution flows.
Engineering fixes UAT findings on this node and repeats the gate. Owner UAT is
not inferred from automated tests.

For companion testing, the node hosts a local-only Archipelago Companion
`0.5.32-uat` at `/packages/archipelago-companion-0.5.32-uat.apk`. It uses the
separate package ID `com.archipelago.app.uat`, installs beside the existing
companion, and includes the native WebView launch plus Android's native node-CA
installer. Its SHA-256 is
`8924d7ba3a013e0db09a5f1e72c21de7886e5e21ed1b2e31d585495183191fe7`.
The production companion download remains unchanged.

Owner UAT should cover:

1. Install/reinstall GitWorkshop from the local App Store and open it from the
   App Store, Apps screen, and Source banner. Confirm Discover shows Popular
   Apps first, the banner after two desktop rows, and the remaining catalog
   under All Apps; confirm the GitWorkshop mark is no longer the old icon.
2. Confirm it opens as a top-level page in Companion's native in-app browser,
   not a dashboard iframe, and loads without a blank or "webpage unavailable"
   screen. Confirm Back and Close return through the Companion UI correctly.
3. Select a node identity, exercise `getPublicKey` and signing prompts, verify
   the contained consent surface, short identity-circle loader, success/error,
   allow/deny/remember behavior, then change identity and confirm consent is
   requested again. Repeat this flow inside the Companion WebView.
4. Edit a Nostr identity and confirm the identity-specific success screen shows
   the saved identity, relay coverage, event ID, copy action, and honest partial
   publish warning when a relay does not accept the update.
5. Browse a known NIP-34 repository and exercise the contribution actions that
   GitWorkshop exposes without granting direct merge or release authority.
6. From Companion settings, choose **Download this node's certificate** and
   confirm Android opens the system CA-install prompt for this node. Confirm
   the ordinary browser link still downloads the `.crt` file.
7. Repeat launch through whichever of LAN, Tailscale, FIPS, DNS, IPv4, or IPv6
   is available; the app must follow the dashboard origin rather than a stored
   address. A raw numeric address works over HTTP; for HTTPS over Tailscale use
   the node's MagicDNS hostname because the certificate is issued to that name,
   not to the numeric Tailscale address.

UAT follow-up on 2026-09-08 found three integration defects: the mounted gate
used root-relative form/assets and returned nginx 405 in a fresh mobile
browser; silent signer requests flashed the full-screen broker frame in the
Companion WebView; and IndeeHub reloaded while the signer's success surface was
still closing, leaving Android WebView blank. The fixes are implemented with a
validated forwarded mount, consent-driven broker visibility, and a coordinated
post-auth reload plus native page-commit fallback. These items remain pending
owner retest on the development node; their implementation is not UAT
acceptance. The fixes were deployed locally on 2026-09-08. Live engineering
checks confirm that mounted gate pages and assets retain the app prefix, gate
POSTs return the application's 401 response instead of nginx 405 over HTTP and
LAN HTTPS, both apps are healthy, and the served provider and UAT APK match
their build hashes.

A further Companion retest showed a black surface immediately after the first
identity selection even though authentication, reload, application data, and
`/api/auth/me` all completed successfully. The common cause was the Android
Chromium compositor retaining the hidden broker iframe's last full-screen black
canvas. The broker route now has a genuinely transparent document, and hidden
brokers stay loaded as a non-interactive 1px surface parked off-screen so the
identity choice and immediately following sign request share one broker.
Companion covers an expected authentication navigation with its branded loader
until the app commits a new frame. This is deployed in `0.5.32-uat` and
remains pending owner visual retest.

### 6. Canonical Nostr Launch — pending

- Publish and configure the signed `archy` kind `30617` announcement.
- Bring up and test the chosen relays and GRASP servers.
- Deep-link/configure GitWorkshop to the verified repository.
- Run the real-node proposal and recovery drills listed above.

### 7. Release — explicitly blocked pending prior gates

Only after engineering tests, owner UAT acceptance, canonical launch tests,
license confirmation, and dependency review may the team:

- build and publish a production multi-architecture app image;
- sign/update the production app-registry entry;
- include the integration in an OTA or ISO;
- add release notes and migration/rollback instructions.

The production companion signing path also needs an explicit release decision.
The current branch omits the shared debug keystore expected by
`scripts/publish-companion-apk.sh` (an older repository revision contains it),
while the local UAT key is intentionally unsuitable for public artifacts.
Before publishing, verify upgrade compatibility against the already-distributed
companion's signing certificate and stage only the intended production-signed
APK.

## Completed Next-OTA Follow-ups

- The container doctor detects a missing rootless Podman `pasta` listener and
  restarts only the affected container, including the intermittent Nginx Proxy
  Manager port 8081 case. TCP and UDP bindings are checked independently.
- The node-certificate UI contains the approved macOS, iOS/iPadOS, Windows,
  Android, Linux, browser restart, DNS, and symptom/cause guidance, while the
  Companion hands the downloaded node CA to Android's system installer.

## Open Decisions Before Canonical Launch

- Which Archipelago-operated and independent relay/GRASP endpoints are used?
- Will upstream add an explicit GitWorkshop license file, or provide another
  written redistribution grant suitable for registry publication?
