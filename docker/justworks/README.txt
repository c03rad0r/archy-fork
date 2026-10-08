Just Works for Archipelago — app and demo

Scope
  One optional Archipelago app. First launch embeds the original Just Works home.
  After a published website is connected, show Website and Business tools.
  Dashboard > Setup > Instant business site contains the native OS guided flow.
  The split launcher appears after the first connected website. A small Business
  tools switch is always available at bottom-left, with a Website return path.
  It uses the shared ring-button style, building/person icons, and icon-only
  layout on phones to leave room for the upstream menu at bottom-right.
  No changes to, publication in, or deployment of the existing Just Works repos.
  Native wallet keys never leave the node. Website owner keys can be imported
  into a separate native business identity. Reuse requires the node password and
  explicit key-sharing approval; only its nsec is forwarded to Just Works for
  the selected login/payment action. No secrets are saved in browser storage.

Real-service mode
  The original hosted creator/editor and an isolated Business copy load inside
  the app iframe. Business data persists in /var/lib/archipelago/justworks.
  The same wrapper is used when Archipelago opens the app in its own tab.
  Owner authorization is verified by Just Works. Connecting a public link is not
  ownership proof.
  The app's GET /api/site checks the existing Core public API for published status.
  It returns only a bounded display name, public identity and fixed-origin links.
  Outbound reads reject redirects and are bounded in size/time; there is no generic
  proxy or client-controlled upstream host. Only the page slug is saved in browser
  storage. Another browser must connect its own link. Removing the app/link does
  not delete the hosted business or its data. Internet access is required.
  Isolated Business discovery resolves published sites directly through Core.
  It uses the connected npub automatically and preserves the upstream UI.
  No demo recipient fallback is enabled; legacy hardcoded order checkout is
  blocked. The website payment page and merchant-address quick-pay are separate.
  The local proxy has a fixed loopback destination and requires same-origin writes.

Demo mode
  The dashboard demo shows the native guide and embeds the original hosted site.
  It does not simulate a replacement website editor or Business interface.
  Connecting a published page requires the installed adapter on a real node.

Setup completion
  The native guide checks the published page through a hidden app-origin bridge.
  Requests are correlated by random IDs; replies require the exact iframe window
  and origin. Merely opening the app never completes the guide. The bridge saves
  only the public slug; storage failure does not mark setup complete.
  The launcher rechecks publication before revealing Business tools.

Native wallet connection
  The third OS guide step calls existing wallet.ecash-lnaddress; no LND/Bitcoin
  dependency, additional wallet app or custom mint is installed. The native
  @minibits.cash address is displayed automatically, not typed by the merchant.
  Owner login/key authorization is entered for Connect wallet & publish. Core
  verifies it; the adapter changes only the payment field using existing APIs.
  POST /api/payment-connect requires exact same-origin JSON and a bounded body;
  fixed-origin requests refuse redirects. No credentials are logged or stored.
  Success requires Core's verified artifact, published status and a fresh public
  record whose lud16 exactly matches the native address. Partial save/publish
  failures remain incomplete and offer retry. No payment is sent by setup.
  Hosted website tip flow reads this public record. Legacy Business order flows
  with hardcoded destinations are not changed or certified by this integration.
  Wallet Receive > Ecash uses the existing claim/retry flow. Node ecash still
  depends on its existing hosted address/mint service; this is not self-hosting
  the mint or receiving into LND. Existing Business merchant discovery applies.

Runtime
  Python adapter and isolated Business service, non-root, read-only root, no
  capabilities, 256 MiB limit.
  Port 8340 is loopback-bound behind Archipelago's app gate. The manifest carries
  the icon, UI classification, build context and host-frame integration.
  Run locally: JW_HOST=127.0.0.1 JW_PORT=18340 python3 server.py
  Build: docker build -t localhost/archipelago-justworks:0.1.0 docker/justworks

Design-system provenance
  Vendored from bencoin21/justworks-business at
  0aec5c44fb90f8642afab8f2e9d4c13364af1ae0 (2026-09-20), MIT.
  The original tokens, design-system.css imports, logo renderer, base/components,
  layout presets and mobile chrome are retained. The only vendor adaptation is
  ds/frame.js configureNavigation(): local launcher destinations replace
  the upstream customer menu. launcher.css owns the new page composition.
  The dashboard SVG uses the official portal paths in white,
  normalized with scripts/normalize-app-icon.py (12% margin). Archipelago adds
  its tile plate. No replacement brand glyph or external artwork is used.
  Just Works MIT notice: UPSTREAM-LICENSE and public/licenses/JustWorks-LICENSE.
  Syne and Geist font licenses: public/fonts/*-LICENSE.

Verification
  python3 -m unittest discover -s docker/justworks -p 'test_*.py'
  From neode-ui: npm test -- src/views/goals/__tests__/justworksCompletion.test.ts
    src/views/goals/__tests__/goalStepActions.test.ts src/stores/__tests__/goals.test.ts
  From neode-ui: npm run build
  bash scripts/validate-app-manifest.sh apps/justworks/manifest.yml
  python3 scripts/check-app-catalog-drift.py --release --strict
  Local shared browser scenarios/reports live outside the repository, under
  ~/.local/share/browser-check/projects/archy-justworks/ and /tmp/justworks-*.
  Browser fixture tests perform no production writes. A container read-only check
  used the existing public barreirinha-bar-cafe-3 site; no credentials were used.

Deployment boundary
  Deployed and verified as an Archipelago demo on yaya. Not an OTA, catalog
  publication or change to the hosted Just Works deployment. Shared-platform release/integration remains with the
  existing platform owner; preserve their concurrent work when integrating.

Native key rollout
  Backend additions: identity.capabilities and password-protected identity.import-nostr.
  Import verifies the expected npub, creates a separate business identity, reuses
  an existing matching business identity, and never replaces another identity.
  The native guide checks capabilities before collecting an import key. Nodes
  without the new RPC retain direct owner-key/login access and a clear update note.
  Export uses existing identity.export-keys after node-password verification;
  other exported secret fields are never forwarded to the app.
  The original hosted CMS has no session injection mechanism. Its own sign-in
  remains separate. Saved credentials support payment setup and local Business.
  Source: isolated justworks-business-archipelago worktree, upstream 0aec5c4.
  Business LICENSE and bundled upstream assets retained in business/.
