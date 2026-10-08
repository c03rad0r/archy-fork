# Blossom on Archipelago

Candidate package, not a published catalogue release. Follow
[`docs/app-developer-guide.md`](../../docs/app-developer-guide.md) and
[`docs/candidate-catalog-qualification.md`](../../docs/candidate-catalog-qualification.md)
for lifecycle and catalogue acceptance.

## Package contract

- MIT upstream `hzrd149/blossom-server` 6.4.1, source commit
  `a492dc61c4a581bbd0992546b2aec6f9aa543f75`. The Dockerfile verifies the source
  archive SHA-256 and uses upstream's frozen dependency lock for the server.
- Manifest-owned local build; the runtime payload must include `docker/blossom`.
  No unpublished registry image is advertised. Initial installation needs access
  to the open-source build dependencies; normal startup uses cached dependencies.
- Rootless container, read-only root, no capabilities, no new privileges,
  explicit `slirp4netns`. Host port 8191 binds IPv4 loopback behind AppGate.
  Keep that private backend binding: any FIPS/IPv6 ingress belongs at the gate.
- Persistent data and SQLite under `/var/lib/archipelago/blossom/data`.
  Preserve this directory on uninstall. No automatic expiry/pruning, automatic
  mirroring, media conversion, or upstream administration dashboard.
- Uploads require BUD-11 signatures from the profile identities supplied by
  `{{NODE_IDENTITY_PUBKEYS}}`. No profile means startup fails closed. Changes to
  that allowlist take effect on restart, including revocation of removed profiles.
  The appliance identity is excluded. Listing requires the owner's signature.
- The custom local UI loads the canonical, host-managed `nostr-provider.js`
  through the documented lifecycle hook. A missing provider fails verification.
  The UI uses the platform identity chooser and ordinary NIP-07 signing; there
  is no generated browser key, nsec input, or second consent modal.
- Upload authorization is scoped to the file hash, actual server hostname and
  five-minute expiry. Local upload does not send a public Nostr announcement.
- Uploaded files are returned as sandboxed attachments. Untrusted HTML/SVG must
  not acquire this app's origin or signer access. Published website rendering
  needs the separate website origin, not a relaxation of this policy.

AppGate protects reads as well as the UI. Blossom itself is content-addressed,
not an encrypted per-user vault: other authorized node users who know a hash can
retrieve its bytes. Do not open the whole app gate to publish one website. Public
asset serving must authorize exact selected hashes; external replication requires
its own explicit content/destination review. An inaccessible local URL is not a
working public Blossom endpoint.

## Qualification evidence — 2026-10-08

- Manifest preflight: 16 passed, no warnings. Generated catalogue drift: zero.
- Candidate built and started on Framework with read-only root and the declared
  resource/security constraints. All protocol tests use synthetic identities and
  files; no public relay or external Blossom server is contacted.
- `tests/apps/blossom/protocol.ts` passed against the candidate: authenticated
  upload/readback, exact hash/size/bytes, wrong identity/server/expired/anonymous
  upload rejection, owner-only listing, disabled mirror, canonical provider and
  health endpoint. HTML response has sandbox CSP and attachment headers.
- Fixture survived container recreation with the same data directory and restart
  with explicit slirp4netns. An earlier test using Podman's default pasta hit a
  transient port teardown conflict; that is not the package's configured network.
- Canonical Rust parser: all shipped manifests parse in the isolated test runner.
- Setup/source tests: 13 passed, dashboard typecheck passed including the final
  receipt-review presentation changes.
- Packaged UI passed a real Chromium test at mobile width: explicit identity
  chooser, consent before upload, signer refusal blocks upload, hash/host-scoped
  upload, consent reset and no external requests. Signer and upload transport
  were mocked for this UI test; live protocol checks above are separate.
- Framework's normal installer succeeded after the operator temporarily disabled
  dashboard 2FA. Candidate manifest and build context are staged in the runtime
  payload. The app is healthy, with its canonical bridge installed by the hook,
  read-only root, slirp4netns and a loopback backend behind AppGate. Anonymous
  HTTPS access on port 8191 returns the gate's 401 sign-in page.
- Real HTTPS tab signer acceptance passed: profile chooser, refusal prevents any
  upload, and an approved BUD-11 authorization stores a synthetic local file.
  No real identity key was exported or public Nostr event sent. Existing native
  Bitcoin/LND processes retained their original start times during installation.
- No signed catalogue, source proposal, public Nostr event, OTA or ISO published.

- Real HTTP tab signing also passed. Normal app stop/start, restart with a new
  container, uninstall with `preserve_data:true`, reinstall, and management restart
  all preserved the uploaded synthetic file, verified by hash. Native Bitcoin/LND
  processes retained their original start times.
- Local website archive integration is implemented in source: an explicit action
  signs a hash/server-scoped upload, stores the saved draft through the local
  manifest-owned Blossom backend, verifies exact readback and records a receipt.
  It does not announce or replicate anything. Its backend RPC is not yet deployed.

Still required before release: cross-profile identity switch (only one profile
was available), HTTP/HTTPS iframe and physical companion validation, arranged
reboot, integrated website archive acceptance, then reviewed source/mirror parity
and signed catalogue gates. Selective public asset routes remain separate work;
the authenticated app address must never be advertised as a public Blossom URL.
Restore dashboard 2FA with the operator after live testing.

### Companion follow-up — 2026-10-08

Blossom now requests the canonical identity chooser once when opened, identifies
itself explicitly to the tab signer, and disables the provider's unrelated
NIP-98 web-app login. Cancelled selection leaves a retry button; uploads still
require file review and signer approval. A host signer bug sent a Vue reactive
Proxy through postMessage after selection, closing the picker but stranding the
app behind an empty signer. The host now copies only public identity fields.
The reactive-object regression test and a real direct-app mobile-width browser
check pass: automatic chooser, closed signer, visible app, denied upload and
approved local upload. Physical companion confirmation remains pending.
The corrected image is a private rebuild of the existing candidate tag; assign
an updated package/image version before reviewed catalogue publication.
