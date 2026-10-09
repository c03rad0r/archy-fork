# Post-1.8.22 regressions and retained release checklist

Release target: **1.9.0**, as requested by the operator. Supersedes the provisional 1.8.23-alpha target.

Status: OPEN. New regressions reported after publication on 2026-10-01.
Do not mark complete from source changes alone. Preserve wallets, app state and
operator uninstall decisions. Never send a second payment to recover delivery.
The earlier Framework startup incident remains separately closed with operator
acceptance; this is a new paid-file incident.

## Latest deployment checkpoint

- Framework physical radio investigation is **operator-deferred**, not passed.
- Unpublished candidate backend/UI deployed on dev and yaya with backups;
  management health passed and native Bitcoin/LND stayed running.
- Actual yaya startup exposed missing HTTPS ACME in the shipped template.
  Template and narrowly recognized legacy migration are fixed; 28 Python checks,
  120 isolated public-security cases, and the ISO overlay check passed.
  Live yaya exact-token and missing-token checks passed over HTTP/HTTPS and
  public HTTP; existing public-site TLS/authentication remain intact.
- Latest embedded helper/template rebuilt and deployed on dev/yaya; full isolated
  backend suite passed 1,651 tests, zero failed, four explicit ignores. Helper
  bytes match source after management restart and live ACME/security probes
  pass. Full-machine reboot and signed catalog migration remain unverified.
- Operator signed the candidate catalog; exact reviewed contents and release-root
  signature verified on dev/yaya. Only NPM and Angor indexer changed. An explicit
  signed local-candidate selector is implemented because mirror ordering always
  prioritizes the public origin. Full isolated suite now passes 1,653 tests; its
  optimized build completed and the signed candidate is active on dev/yaya.
  Live NPM migration found a further compatibility failure: existing saved
  upstreams use the former host gateway, which default slirp cannot reach.
  A disposable namespace verified preservation using an explicit slirp subnet;
  live qualification repair now passes saved-upstream, database/certificate
  preservation, bridge, ACME and public HTTPS checks. Durable source/catalog
  correction and removal of the temporary qualification network override remain
  mandatory before release. See the acceptance document for details.
- Shorty's containment is untouched. Its manual shop HTTPS route versus NPM
  certificate ownership remains a blocker. Paid-file regression acceptance,
  physical companion uploads, Angor's 34 missing announcements/full-chain
  acceptance, and exact OTA/raw-ISO acceptance remain open.
- No new catalog, OTA or ISO has been published.

## Current tasks

- 2026-10-09 live repair checkpoint (supersedes the pending deployment notes
  immediately below): Yaya now runs management backend SHA256
  `938a90c7d9ecbdb179b1cf2886b5ed03e4d78c0d454f2cec42c0d9de3d447eb9`,
  built from the registration correction in `424070d2`. Optimized build passed.
  Management health, unchanged running container IDs, unchanged node identity key
  and unchanged registration pin/marker checks passed. The old binary is retained
  under `/var/lib/archipelago/support/indee-registration-424070d2/`.
  The first context probe during inventory startup still failed; after inventory
  recovery the authenticated `media.registration.context` probe succeeds and
  returns all installation bindings. No catalog or app-data mutation was used.
  The probe initially treated JSON-RPC `error: null` as failure; corrected it and
  repeated the successful live request. Isolated backend regression compilation
  remains pending after severe build-host swapping; do not claim its test passed.

  A real native Home Serve login then exposed another defect: `UserDTO` omitted
  `nostrPubkey`, so the strict frontend profile check correctly refused the
  otherwise matching signed session. IndeeHub `08586e3` adds this public field;
  two DTO regressions and backend production build pass. Deployed only the built
  DTO module with its previous copy saved under the private
  `indee-profile-08586e3` artifact directory, then restarted only `indeedhub-api`.
  API health passes. A clean real browser selected native **Home Serve**, approved
  only its kind-27235 authentication request, and verified that the native choice,
  session subject and API profile key match. The signer closes normally. No
  identity was created/replaced and no payment or publication was submitted.
  Frontend `7b18912` and API DTO remain running-container patches; durable image/
  managed deployment, complete video upload/publication and cached-account-switch
  acceptance remain open. Preserve the operator's native identities throughout.

- 2026-10-09 UI checkpoint: IndeeHub commit `40b3798` is served on Yaya
  (index SHA256 `d921a88448f89674809dbe623ec045849012b1fb0330e3abf1d2d6836e9b3c96`).
  Publishing now appears only in its own editor tab; Assets remains mounted
  across tab switches. Copy distinguishes computer upload from Cloud-backed
  catalog publication; saved-registration recovery is under a details control.
  Production build and 26 focused frontend tests pass. Browser checks verify
  native signer availability, no automatic login, removal of local account
  controls and clearing of legacy app-local accounts. Real selected-identity
  sign-in and video upload/publication remain unverified. This is a running
  container frontend patch, not a durable pinned-image/catalog deployment.
  Backend correction `424070d2` has ngit proposal
  `9d6de783f5022b3b859e0b1f3e881e389da4482dd5dd15ff29a40b4e25dafcb8`;
  isolated regression and optimized build are still running, not passed or
  deployed. Preserve this distinction when resuming the backend work.

- 2026-10-09 follow-up: operator reports missing autosign and the same generic
  registration error. `40b3798` disabled provider auto-authentication without
  replacing it with app authentication on native selection; this was a regression.
  IndeeHub `7b18912` adds a single shared automatic/manual login path driven by
  the trusted provider's `onIdentitySelected`. It rejects mismatching signer keys
  and changed selections, checks the backend profile, and never falls back to a
  cached app identity. Twenty-one focused tests and the production build pass.
  Yaya serves index SHA256
  `1c7fee64430b0d993288408dbe493d0f4f55a49fe2422a3507c7204c12343632`;
  the native provider bytes are unchanged. A disposable served-app browser test
  verifies selection triggers exactly one kind-27235 signature request without
  clicking login; it intentionally stops before signing with a stub. This is not
  real-identity login acceptance. The first browser run raced deployment and
  failed against the previous build; the post-deployment rerun passes.
  Authenticated read-only registration RPC still reproduces the generic failure,
  with server cause `IndeeHub is not installed`; backend build/test remain pending.

- [ ] **2026-10-09 IndeeHub UAT remains failed:** operator still reports wrong
      Nostr identity, upload failure and confusing always-visible publishing UI.
      Preserve native identities; remove only app-local generated/imported accounts.
      Require explicit Archipelago selection and matching API profile. On Yaya,
      `media.registration.context` logs `IndeeHub is not installed` while all seven
      containers run. Authenticated state includes only the parent `indeedhub`
      package; the registration code incorrectly requires a separate
      `indeedhub-api` package record. Candidate correction retains parent running/
      installed checks and the existing API installation pin; it checks a legacy
      API package when present. Source tests/build and live registration acceptance
      must complete before closing. Move publishing to its own editor tab, preserve
      computer-upload state and existing pending registration requests.

- [x] Yaya App Store component/alias regression (reported 2026-10-02): one
      Cuprate entry (hide Cuprate UI companion), one BTCPay Server in Commerce
      with its icon (merge legacy btcpay pins), one NetBird entry (hide server
      and dashboard components). Apply to fresh signed/community catalogs,
      persisted caches and installed Apps/Services; preserve actual components,
      data, dependencies and legacy-only installations. Source fix and 34
      focused tests pass; production build and yaya live browser checks at
      mobile/desktop widths, including hard reload, pass. Actual new installs
      of these three products were not performed during this UI acceptance.

- [ ] **2026-10-02 operator requirement for the next update: Fast by default for
      on-chain transactions**, including sends and cooperative channel closes;
      users can explicitly select slower or custom fees. This supersedes the
      earlier instruction to leave defaults unchanged. Implement dynamic
      next-block targeting, not a fixed sat/vB default. Cover initial form state,
      reset/reopen, preview, submission and backend omitted-fee defaults; preserve
      explicit user selections. Audit channel opens and other on-chain entry
      points for the same policy. Force-close commitment fees and subsequent
      sweeps require separate supported LND handling, not cooperative-close
      parameters or a promise of immediate spendability. Implementation and
      actual-node acceptance remain pending; no default was changed by the
      manual acceleration below.
- [ ] **Speed up interface:** implement the plan in the fee-acceleration section
      below, with explicit additional/total fee preview, budget, live eligibility,
      RBF/CPFP distinction and durable operation tracking. Include in next-update
      scope alongside Fast defaults; do not infer completion from this plan.
- [ ] **Bump-quote wallet-change failure UX:** Framework's read-only check on
      2026-10-07 returned `Not enough wallet change for this fee; choose a lower
      rate` three times and did not submit a transaction. Preflight must calculate
      spendable change against the requested rate and budget, explain the shortfall,
      suggest a viable rate/budget, and stop duplicate quote retries without
      altering the wallet or original transaction.

- [x] Resolve the tester's missing-file recovery request: operator confirmed on
      2026-10-02 that the tester received the file from Amish Paradise and accepts
      closure of this individual recovery. No seller access or further payment
      is required. This is operator-confirmed receipt, not independent byte
      verification or proof that the candidate fix delivered it. The durable
      payment/delivery fixes and regression acceptance below remain required.
- [x] Correct seller settlement verification when local-node payment skips polling.
- [x] Durable seller entitlements and safe buyer retry after navigation/restart;
      do not issue another payment on an uncertain or successful attempt.
- [ ] Cache Lightning purchases, preserve ownership, optional Files copy, free repeat.
      Exact bytes, ownership and free repeat passed; optional Files-copy acceptance
      must be located or repeated before closing this combined checkbox.
- [ ] Diagnose mobile companion uploads on the affected route/device.
- [x] Real progress in the existing compact upload bar; no increased height.
      Actual browser uploads verified a 44px bar; physical companion remains separate.
- [x] Preserve uploads/progress across screens and original batch destination.
      Actual browser batch destination and cross-screen persistence verified.
- [x] Cancel active transfer and queued files; truthful partial/error/server-save status.
- [x] Transparent transaction-filter container; single horizontal scrolling mobile row.
      Actual served desktop/mobile styles and geometry verified; retain in final artifact checks.
- [ ] Immich displayed as one app, internal components hidden; diagnose restarting services.
- [x] Diagnose unwanted CryptPad after upgrade, failed uninstall, and persistent removal.
- [x] Identify the other removed unexpected service: Core Lightning, confirmed
      in the original node investigation and its retained uninstall markers.
- [ ] Upgrade regression matrix: installed/stopped/restarting/removed/legacy apps,
      aliases, dependencies, inventory, desired-state reconciliation and data preservation.

- [x] Portainer duplicate-network migration: retire the redundant managed repair
      override, preserve operator settings/state, verify generated command,
      actual request namespace, dashboard readiness and repeated reconciliation.

- [ ] Lightning cooperative-close fees: Standard/Medium/Fast/Custom selection,
      explicit default target, strict backend validation and forwarding, error
      handling, mobile layout and no real channel closure during tests.

- [x] Apps search clear control: My Apps, Services and App Store, desktop/mobile,
      existing design tokens, right-aligned icon, no size change, keyboard focus.

## Fee acceleration and Fast-default handoff — 2026-10-02

Operator explicitly authorized accelerating the latest outgoing Bitcoin payment
and subsequently requested Fast defaults for all on-chain transactions in the
next update, with slower options. The later instruction supersedes the earlier
no-default-change restriction. This is independent of paid-file recovery and
does not authorize another purchase payment.

Live Framework evidence, checked through the existing SSH connection with
hostname verification (not the development node):

- Original transaction:
  `ad3c2ffd14f35e0508045f538b0046876cfeddc732ed8c1f49771c219f2f2b42`.
  Recipient 161,650 sats; original fee 144 sats; vsize 142; no unconfirmed
  ancestors or descendants before submission. Wallet-owned output 0 was
  unspent and worth 21,126 sats. It did not signal BIP125 replacement.
- LND next-block estimate: 2 sat/vB. Bitcoin conservative estimator returned
  2.255 sat/vB (effective target 2 blocks). Mempool/relay floor: 1 sat/vB.
- Submitted exactly one `wallet bumpfee` for original output 0, using
  `--sat_per_vbyte 3 --budget 650 --deadline_delta 1 --immediate`.
  This registers a CPFP child, not a replacement of the recipient payment.
  The budget was explicit; no implicit 50%-of-change budget was used.
- Actual child broadcast and accepted in the node's mempool:
  `e04aec1dfbc16043611411de83201a32f7ffdcb9e8bb362c281cf967ee4bdd69`.
  Its only input is the specified change output; output 20,476 sats;
  fee 650 sats; vsize 111. Parent plus child: 794 sats / 253 vB =
  approximately 3.138 sat/vB. Recipient output remains unchanged.
- At the post-submit check, node tip was 969564, both transactions remained
  unconfirmed, and child `unbroadcast=false`. LND recorded one broadcast
  attempt, budget 650 and deadline height 969565. RPC success is not
  confirmation; next-block inclusion is not guaranteed. Do not repeat the
  bump blindly if resuming: inspect original, child, sweeper and chain first.
- Bitcoin CLI works with `-conf=/tmp/rpc.conf` inside `bitcoin-knots`.
  Do not print/copy that configuration or its credentials. No default settings,
  wallets, channels or services were changed/restarted by this acceleration.

Final confirmation check: **both original and CPFP child confirmed in block
969565**, the next block after submission. LND reports one confirmation for
each, with fees still 144 and 650 sats respectively. The authorized acceleration
is complete. This outcome does not guarantee next-block results for future sends.

Implementation plan for the next agent:

1. **Fast default:** update initial/reset states in `SendBitcoinModal.vue` and
   `LightningChannelsPanel.vue`, plus their fallback targets and backend
   omitted-fee behavior in `lnd/wallet.rs` and `lnd/channels.rs`. Existing Fast
   presets already target 1 block; current initial/reset states select Standard
   and cooperative-close backend defaults to 6. Preserve explicit slower/custom
   settings. Quote a fresh fee and show total cost before confirmation; never
   silently substitute a stale/cheap estimate after estimator failure. Explain
   that next block is a target, not a guarantee. Audit channel opening and other
   on-chain call sites, distinguishing force-close and sweep semantics.
2. **Flow:** transaction details → Speed up → Next block / Custom → review
   additional fee, resulting total fee, maximum additional-fee budget and
   recipient amount → Confirm. Custom specifies a target package rate in
   sat/vB for CPFP, or replacement rate for RBF. Clearly identify the method:
   RBF replaces a transaction; CPFP spends wallet-owned change to accelerate
   the original. Only expose methods supported for that specific transaction
   by the installed wallet; do not infer direct RBF capability from a flag.
3. **Read-only quote RPC:** return eligibility/reason, chosen method, original
   txid/outpoint, live chain/mempool status, rate, transaction/package sizes,
   existing fee, estimated additional fee, resulting total fee, explicit hard
   budget, expiry and a server-bound quote ID. Verify ownership, spendability,
   leases, dust, ancestors/descendants, sweep membership and relay/replacement
   rules. CPFP fee calculation must cover the ancestor package, not just the
   child's vsize. Distinguish estimated spend from maximum approved spend;
   LND can consume its entire budget at the deadline (as happened here).
4. **Submit RPC:** require wallet authorization and quote ID/idempotency key;
   serialize by affected transaction/outpoint and persist operation state before
   calling LND. Revalidate confirmation, conflict, output availability, topology,
   fee estimate and policy at Confirm. Invalidate materially changed/expired
   quotes for another review; never raise the approved budget silently. A
   timeout is an unknown outcome to reconcile, not permission to pay again.
   Repeated/restarted submissions return the existing operation.
5. **Track results:** distinguish requested, registered, broadcast, mempool
   accepted, confirmed, rejected and unknown. Link original/replacement/child
   txids and subsequent child replacements durably. Preserve recipient amount
   and original identity; present one payment with fee history, not a second
   outgoing payment. Current `lnd.gettransactions` drops output ownership and
   input relationships and uses absolute wallet delta as amount; extend the
   normalized model deliberately to avoid counting the CPFP fee as a new send.
   Home and its transaction-detail component need live refresh and reorg handling.
6. **Acceptance:** default/reset/explicit-slower UI and omitted-fee backend
   tests; package math, budget and dust boundaries; stale/confirmed/conflicted
   quotes; concurrent submits, timeout and restart reconciliation; unsupported
   RBF, CPFP child replacement and history grouping; estimator outage; mobile
   review. Run backend tests only through `scripts/test-backend-isolated.sh`.
   Use regtest for broadcast/confirmation scenarios; do not close real channels
   or send real payments merely to test defaults. Record source results separately
   from deployed UI and live-node acceptance before OTA/ISO publication.

Upstream semantics: [LND BumpFee API](https://lightning.engineering/api-docs/api/lnd/wallet-kit/bump-fee/)
and [LND unconfirmed transactions guide](https://docs.lightning.engineering/lightning-network-tools/lnd/unconfirmed-bitcoin-transactions).
No fee-bump interface or Fast-default source change is implemented by this
handoff. Both remain required next-update work.

### Bump interface implementation and operator UI review — 2026-10-02

This checkpoint supersedes the preceding statement that the interface is only
planned. The operator requested a small **Bump** button on pending on-chain
transactions, asked for a Framework/yaya preview before release handover, and
approved the layout. They then requested the existing glowing green transaction
success animation and approved that addition as well.

- Implemented `BumpFeeModal.vue` and a small Bump action in `TransactionsModal.vue`;
  Home refreshes wallet history after a verified update. Next block is selected
  initially; Custom invalidates the previous quote. The review shows recipient
  amount, current fee, additional fee cap, resulting total cap and package rate.
- Implemented authenticated/CSRF-protected `lnd.bump-quote`, `lnd.bump-submit`
  and `lnd.bump-status` in `lnd/fee_bump.rs`, with submit/quote rate limits.
  Quotes expire after 60 seconds and are revalidated before submission. A
  synced write-ahead receipt under `wallet/fee-bumps/<txid>.json`, a submission
  lock and create-new semantics prevent blindly retrying a possibly accepted
  mutation after timeout/restart. Unknown outcomes remain blocked for recovery.
- Method is selected from live wallet evidence, not chosen by the user:
  CPFP uses spendable/unleased native wallet change on a simple pending outgoing
  payment. RBF is limited to LND's existing single-input wallet CPFP sweeps,
  with a verified pending input, wallet-owned output and simple parent package.
  Arbitrary payment replacement, anchor/HTLC/batched sweeps and complex ancestor
  chains are explicitly unsupported. LND 0.21+ is required for the exact
  budget/deadline API used. No default wallet fee setting is changed.
- Every mutation supplies an explicit budget below the selected input value
  and a one-block deadline; the review explains that the full budget may be
  consumed. Node mempool acceptance and LND confirmation evidence drive status.
  The shared `PaymentSuccessPane` displays its existing green glow/check only
  after acceptance: **BUMP BROADCAST / Awaiting confirmation**, then **CONFIRMED**.
  RPC registration alone never triggers the success animation.
- Standalone preview deployed to Framework at `/fee-bump-preview/index.html`.
  Its CPFP/RBF buttons are sample scenarios for review, not product method
  choices. It compiles the actual components with an isolated mock RPC module;
  browser checks verified zero wallet RPC calls. No test payment was sent.
  Source: `neode-ui/previews/fee-bump/` and `vite.bump-preview.config.ts`.
- Initial UI verification: eight focused Bump tests, 12 existing Home wallet
  freshness tests, TypeScript check and production build passed. Playwright
  exercised 390px and 1440px preview, Custom, Confirm, success animation and
  Done with no browser errors or wallet RPC requests. An initial stacking
  defect was found visually and fixed with explicit dialog z-order; mobile
  transaction amounts now stay on one line, with wrapping badges.
- Latest backend validation and actual wallet deployment are still in progress
  at this checkpoint. Do not claim regtest or real-wallet RBF/CPFP acceptance
  from unit tests or the static preview. The earlier manual CPFP and confirmation
  above remain separate evidence.

**Retain for the next release:** Fast defaults are still an unfinished requirement
from the operator; this interface work does not implement those defaults. Preserve
all other checklist tasks. Broader RBF support and grouping fee-only child rows
with their original payment remain limitations to assess explicitly. No fleet
OTA/catalog/ISO publication is authorized by this preview deployment alone.

### Release-agent handover: approved Bump UI, production deployment blocked

Operator approved the design and the added shared glowing green success animation.
The last request was to finish and provide the next-release agent a handover.

**Verified complete:**

- The interactive sample preview is deployed on the actual Framework:
  `http://100.65.115.109/fee-bump-preview/index.html`. Select a sample scenario,
  then Bump → Next block / Custom → preview → Confirm. It cannot send funds.
  CPFP/RBF scenario buttons exist only in this preview; the actual wallet chooses
  the supported method. The production dialog is not a method-selection menu.
- Eight focused frontend tests passed, including the real shared success badge
  appearing only after mempool acceptance/confirmation; 12 existing Home wallet
  freshness tests passed. TypeScript check and production UI build passed.
- Seven focused backend tests passed; the full isolated backend run subsequently
  passed **1,660 tests, zero failed, four explicit ignores**. No additional live
  payment, bump, channel closure or wallet setting change was made for testing.
- Desktop 1440px and mobile 390px browser tests exercised the deployed sample
  review, custom fees, Confirm, green animation and Done with zero browser errors
  and zero wallet RPC requests. The user approved both design and animation.

**Deployment boundary and blocker (2026-10-02 10:23 UTC):**

- Only the standalone sample preview is deployed. The actual dashboard/backend
  fee-bump feature is NOT deployed yet. No management/native service was restarted
  by this feature work. Framework's existing backend SHA256 was
  `8fb6249d1869bb8c9aea26d0f846de5b3eec328113a5c7f573628306e26e652e`.
- The session changed to a restricted sandbox while the release backend was
  compiling. The original build process/session is gone. The existing
  `core/target/release/archipelago` still had the pre-feature 04:42 local timestamp
  at the checkpoint; **do not deploy that stale artifact as this feature**.
- SSH now fails with `Control socket connect(...): Operation not permitted` and
  `socket: Operation not permitted`. This is an execution permission blocker,
  not another Framework password failure. Approval mode is never, so this session
  cannot request an elevated network operation. Resume deployment from a session
  with authorized network access; do not alter node credentials to solve it.
- A local offline release-build retry was started after the interruption; its
  completion must be checked before using any backend artifact.

**Exact source scope (uncommitted, mixed workspace; preserve unrelated edits):**

- New backend: `core/archipelago/src/api/rpc/lnd/fee_bump.rs`.
- Wiring: `api/rpc/lnd/mod.rs`, `api/rpc/dispatcher.rs`,
  `api/rpc/bitcoin.rs` (shared read-only Bitcoin RPC helper visibility), and
  `core/archipelago/src/rate_limit.rs`.
- UI: `neode-ui/src/components/BumpFeeModal.vue`, `TransactionsModal.vue`,
  `neode-ui/src/views/Home.vue`, and
  `neode-ui/src/components/__tests__/BumpFeeModal.test.ts`.
- Preview-only files: `neode-ui/previews/fee-bump/` and
  `neode-ui/vite.bump-preview.config.ts`; keep its mock RPC alias out of the
  production build. The regular production build uses the real RPC client.

**Staged artifacts and next steps:**

1. Finish the release backend build and record its SHA256. Production UI archive
   `/tmp/archy-bump-ui.tar.gz` SHA256 is
   `d0e253aba09ad257136e3feb5b63176c388f3bb968f560702eefb768d29e494d`;
   its `index.html` SHA256 is
   `9fef18c8c1c9bc21f43c3635b747dfcfbea965b096867b454bbf608121715438`.
   This UI contains the approved green animation.
2. With access restored, verify hostname `framework-pt`. The UI archive and
   `/tmp/archy-deploy-bump-framework.py` were staged on Framework; the same script
   exists locally. Review it, stage the correct backend as `/tmp/archy-bump-backend`,
   then supply the exact backend/archive hashes as its two arguments. It prepares
   a rollback under `/var/lib/archipelago/support/framework-fee-bump-20261002`,
   checks manager health, and compares native container identity/start times,
   wallet identity, channels and balances. It has NOT been executed yet; the
   rollback directory is therefore planned, not a verified backup.
3. Verify served production assets and authenticated quote/status behavior after
   deployment. The previously saved dashboard session returned 401 during
   preflight; use a normal fresh login. Do not fabricate authenticated acceptance
   from the sample preview. No funded UI transaction test has been authorized.
4. Keep the documented support limits: simple outgoing native-change CPFP and
   RBF of LND's single-input CPFP sweeps; no general payment replacement, incoming
   payment bumps, batched/channel sweeps or complex unconfirmed ancestry. Quote
   expiry, persisted ambiguous outcomes and fee budgets must remain intact.
   Full regtest mutation/confirmation/reorg acceptance is still outstanding.
5. Implement the separately authorized **Fast default** for sends/cooperative
   closes and audit other on-chain entry points; preserve slower/custom choices
   and distinguish force-close semantics. That change remains required for the
   next release and is not implemented by this Bump work. Preserve all other
   regression/release blockers and the separately closed startup incident.

## Retained release work (previous acceptance is not new-regression acceptance)

- Mempool patched image/catalog version agreement, update-button clearing, one card.
- Minibits PR160, Lightning address availability, concise single-column backup copy.
- Framework LND startup/Receive and unknown-vs-zero balance behavior.
- Friendly Bitcoin warmup; LND waiting for install/sync; Bitcoin UI during IBD;
  headless Phoenixd without self-waiting or bogus launch action.
- Cashu same-mint paid files, exact amounts/change/refund, errors, stored bytes,
  Files copy and repeat access without re-payment.
- mempool.space public explorer fallback, preserving local/custom configuration.
- Optional install pruning and consistent automatic-pruning policy.
- X250 kiosk version picker layering/contrast and pruning layout.
- AIUI single desktop/mobile background, transparent embedded layers,
  preserved standalone wallpaper.
- PR review/fixes/tests and normal merge/closure (160 previously shipped;
  161/162 merged and included in 1.8.22).
- Installed inventory retained during app restart/hard-refresh.
- Correct iframe/browser launch readiness, useful errors and delayed startup.
- GitWorkshop payload/build contexts, progress and persistence after refresh.
- Gitea/Portainer same-server Git from actual request namespace; URLs, auth,
  fresh installation in either order, migration/rollback, restart/reboot,
  Git/SSH/LFS/registry/browser compatibility and data/stack preservation.
- NPM correct admin port/URL, malformed URL behavior, bind-aware readiness,
  persistent backed-up tunnel/LND port-conflict repair on OTA and ISO.
- Angor headless indexer on DEV BOX only, full unpruned Bitcoin/Mempool/ElectrumX
  prerequisites, optional separate relay, official icon with green white areas.
- Compact named readiness messages and bottom-aligned app-card actions.
- Remove unused integration/build fixtures from Apps/Services, preserve app data.
- Safe network doctor, no all-app stop/reset on failed egress probe.
- No orphan companion resurrection; retain existing companion security repairs.
- Current companion image registry, build contexts, runtime asset promotion order,
  generated-service argument quoting and graceful Bitcoin/LND shutdown.
- OTA + RAW ISO, root signatures/catalog compatibility/checksums, independently
  verified public files, Git/ngit source/releases and fleet discovery.
- Correct LAN SCP command for the new ISO.

## Explicit boundaries/follow-ups

- Full-chain Angor indexing awaits development Bitcoin IBD.
- Primal automatic comment exceeding Minibits metadata limit: previously accepted
  upstream limitation, no unsupported local identity/metadata rewrite.
- Lost-response ecash seller receipt redesign is a separately accepted follow-up;
  do not claim an uncertain refund completed or automatically pay twice.
- Optional external-provider/hardware tests must be labelled if not exercised.

## Initial source evidence

`PeerFiles.vue::payWithLightning` immediately downloaded after buyer payment,
while only seller `handle_content_invoice_status` marked a pending invoice paid.
Seller download checked only that cached flag. This matches the reported error
and is supported by source inspection. An earlier diagnostic's HTTP 404 is not
valid confirmation: it incorrectly base64-decoded lncli's already-hex payment
hash. The corrected live diagnostic recognizes the settled invoice on the
candidate; do not cite the earlier 404 as proof of the original failure sequence.
`content_invoice.rs` stored all entitlements only in process memory with a
one-hour TTL, losing both pending and paid access on restart/expiry.
Lightning download returned transient base64 without the Cashu ownership cache.
CloudFolder's view-local spinner had no byte progress/cancel; batch upload read
`currentPath` independently for each file, allowing navigation to move destinations.
Immich's underscore dependencies are scanner-excluded; hyphen manifest IDs are
not. Live inventory confirmed both hyphenated synthetic entries while the
actual underscore-named containers had remained running for nine days.

## Access / acceptance

Operator provided updated Framework SSH authentication privately in chat.
Do not put credentials or deployment addresses in this public document.
Framework was reached over SSH. Native Bitcoin, LND and all three Immich
container identities/start times were recorded before candidate deployment.
The kiosk is at its login page. Dashboard password authentication succeeds but
requires the operator's second factor; normal uninstall acceptance remains pending.

Confirmed live evidence:

- A 10,000-sat peer-file invoice settled at 12:19:29 UTC. The original status
  diagnostic used an incorrectly decoded hash; see the correction above. Buyer
  identity and confirmation that this is the reported sale remain pending.
- The matching item currently allows free access; preserve that operator setting.
- CryptPad has no container but remains in installed-apps metadata. Uninstall
  repeatedly aborts because the removed catalog ID has no manifest.
- Immich server/database/cache are running; synthetic hyphenated dependencies
  appear stopped and the recovery overlay briefly advertises restarting.
- The other removed service was Core Lightning; uninstall tombstones exist.
- No Android resource-upload POST appears in the inspected recent nginx log.
  This does not establish why the affected companion failed.

## Candidate implementation and validation

Source changes persist seller entitlements with atomic writes, verify settlement
at delivery, recover older Lightning entitlements from the seller's LND invoice,
perform the status handshake for older sellers, and cache delivered Lightning
files. Buyer purchase bytes and the shared ownership index now use atomic,
synced writes and a serialized read/modify/write transaction; a corrupt index
fails the write instead of silently replacing existing ownership. The browser saves the invoice before payment and retries delivery without
another payment. Browser receipts are not yet a node-wide recovery store.

The upload queue now belongs to the shared Cloud store, captures its original
folder, reports actual sent bytes and server completion, and cancels its active
XHR and remaining queue. The fixed-height bar remains available across routes.
Transaction filters use a transparent container and one scrollable row. Immich
aliases normalize to their real component names and internal cards are hidden.
Unknown catalog entries no longer prevent the regular uninstall flow.

Validation so far (additional acceptance still pending):

- Final isolated backend suite: **1,631 passed**, zero failed, four optional
  tests ignored. This includes invoice settlement/amount boundaries, durable
  seller records, concurrent buyer ownership, damaged-index preservation,
  Portainer override retirement/idempotence/customization/backup failures,
  recovery overlays and channel-close fee forwarding/validation.
- Final frontend suite: **1,157 passed** across 142 files. Production build
  passed. Six payment-recovery and twelve channel-close tests are included.
- Real FileBrowser uploads at 1440px and 390px: exact bytes and original folder
  verified after navigation, 44px bar, cancellation and queue stop passed.
- Mobile viewport acceptance is not physical Android companion acceptance.
- Final release backend build passed. Candidate backend and dashboard are now
  deployed on dev and Framework. Another OTA/ISO remains pending; published
  1.8.22 artifacts remain unchanged.

Release gates still include actual-node payment recovery/delivery, durable
CryptPad removal through normal controls, Immich inventory after refresh/restart,
physical companion diagnosis, and remaining upgrade regression acceptance.
No new payments, native-service restarts or wallet changes were used in testing.

## Additional live Portainer regression

The X250 user service exited 125 because the generated command supplied
`--network slirp4netns` twice. The manifest already supplies the network, while
an older Archipelago-created `archy-same-node-network.conf` drop-in adds it
again. Quadlet's Network directives accumulate; they do not override each other.
This repair artifact should have been retired when the declarative fix shipped.

The live repair backed up the override and Portainer state, removed only the
exact redundant override, reloaded user systemd and restarted Portainer. API
status returned HTTP 200 with version 2.45.0; the actual kiosk's package state
reported running and UI-ready. Bitcoin/LND and the production site's container
identities/start times remained unchanged. The source migration now detects
this exact managed override before preparing the persistent restart obligation,
backs up app state, retires the redundant file with a retained copy, and reloads
and restarts through normal reconciliation. Custom overrides are preserved.
Automated migration coverage passed; candidate is now deployed on dev and
Framework. The X250 retains its verified live repair pending the next OTA.

## Channel-close fee selection

The existing close UI sent only the channel point, and the backend forwarded
only `force=false`. LND therefore used its lax default confirmation target.
The candidate reuses the channel-opening fee choices (six/three/one block target,
or custom target/rate), explicitly sends six blocks for legacy clients that omit
fees, and validates query parameters before accessing the wallet. Cooperative
fees are never silently applied to force closes. Close RPC retries are disabled
so a timeout cannot silently repeat this mutation.

Protocol reference: [LND CloseChannel](https://lightning.engineering/api-docs/api/lnd/lightning/close-channel/).
Fee targets are estimates, not guaranteed confirmation times. Tests use mocked
requests; no production channel is closed to verify the feature.

Additional browser acceptance:

- Transaction filters at 390px: computed transparent background, one row and
  horizontal overflow verified.
- Close-channel selector at 1440px and 390px: preset/custom controls visible,
  no overflow, custom 25 sat/vB forwarded. The close request was intercepted;
  no real channel closure or wallet mutation occurred.
- All three Apps search screens at both widths: clear icon stays inside the
  field; click/Escape clear; input retains focus; desktop 40px/mobile 52px heights
  stay unchanged. Shared design-system search-field classes are retained.
- Portainer remained active with zero service restarts and no pending marker.
  Its real network namespace read smart HTTP Git refs and the Compose file.
  Original persistent mounts were unchanged. The old integration test containers
  are absent from dev, Framework and X250. One leftover upload-test folder was
  removed after checking it contained only this task's test files.

## Build resource observation

The final optimized compile coincided with heavy memory/disk pressure and local
Bitcoin/LND RPC timeouts on the development node. After pausing the compiler,
both authenticated RPCs responded again; Bitcoin reported height 506400 and
19.6% verification progress, with LND waiting for chain sync. No native service
was restarted. Compilation resumed in a separate user scope limited to one CPU,
with nice 19 and idle I/O priority. This is evidence of resource contention,
not proof of a new wallet or startup defect. Verify native RPC health again
before candidate deployment.

## Candidate deployment and live acceptance — 2026-10-01

Source: `f4d34554` (later commits update this checklist only).
Backend SHA-256:
`8fb6249d1869bb8c9aea26d0f846de5b3eec328113a5c7f573628306e26e652e`.

- Backed up backend, dashboard and app metadata on both nodes under the root-only
  support directory `post1822-regressions-20261001`. Deployed assets before
  promoting the dashboard entry point; manager health passed first.
- Only the Archipelago manager restarted. Bitcoin/LND IDs and start times stayed
  unchanged; Framework's three Immich containers also stayed unchanged.
- Dev authenticated Bitcoin/LND RPCs responded after deployment. Bitcoin IBD
  continued above height 513000; LND correctly reported not yet chain-synced.
- Both nodes serve dashboard entry bytes identical to the production build.
  All six search-clear cases passed against the live dev dashboard (three
  screens, desktop/mobile), including focus, Escape and unchanged field size.
- Framework's stale CryptPad installed claim was removed while the manager was
  stopped; both legacy IDs were recorded as user-uninstalled. Data was preserved.
  Removal remained after another management restart. Dashboard uninstall-flow
  acceptance still awaits authentication; this repair was performed over SSH.
- Corrected invoice diagnostic recovered the settled seller entitlement, returned
  HTTP 200 with `paid: true`, and saved a mode-0600 record. A different item was
  rejected. Paid status survived another manager restart without native restarts
  or a second payment.
- Delivery acceptance remains OPEN: the catalog's file is absent from both its
  dedicated content path and FileBrowser path; a privileged filename search of
  those trees found no copy. Its free-access setting was preserved. Buyer and
  reported-purchase identity still need confirmation; do not claim the actual
  buyer received the file.
- The malformed-hash diagnostic also exposed that invoice-status currently
  propagates validation errors as a closed connection. Before release, return a
  structured 400 for malformed hashes and an explicit retryable response for
  settlement-service errors, with endpoint coverage.

Physical companion upload diagnosis and remaining release acceptance stay OPEN.
This is a candidate deployment, not a newly signed OTA or ISO.

## Release authorization — 2026-10-01

The operator authorized preparing the next OTA and raw ISO after completing all
checks available here. Keep the missing original file/buyer confirmation,
physical companion upload and full-chain Angor indexing as explicit follow-ups;
this authorization does not establish that those scenarios passed. Complete
the known invoice-status HTTP error fix and available automated release gates
before requesting the offline signatures. Angor Indexer and the optional relay
are already present in the current signed catalog and remain included.

Shorty's Mempool report was inspected read-only: frontend/API had been running
for over two weeks, systemd reported zero restarts, and the API was processing
current blocks. The operator said it looked normal after applying the latest
update. No Mempool repair or native-service restart was performed in this check.

## Mandatory release control — operator reiterated 2026-10-01

Every task in Current tasks and Retained release work above remains in scope.
Use this document as the master coverage map, with detailed evidence in
`release-1.8.23-acceptance.md` and `npm-certificate-handoff-20261001.md`.
Distinguish source implementation, automated tests, live acceptance, packaged
artifact tests and publication verification. An implementation or passing unit
suite alone must not check off end-to-end acceptance. Keep earlier accepted
limitations explicit; never relabel an unavailable check as passed.

### Newly required NPM/security gates — publication blocked

- [ ] One authoritative active NPM data/certificate path; fresh, legacy nested
      and current flat layouts, ambiguous/corrupt DB and permission errors.
- [ ] Preserve hosts, accounts, certificates/private keys, renewal files,
      custom settings, permissions and uninstall decisions; backups/rollback
      and repeated migration tested.
- [ ] Default and named HTTP challenge routes follow the active mount, including
      first issuance and renewal under forced HTTPS.
- [ ] Automatic host/certificate create/edit/delete/disable/replacement routing
      and renewal reload; no manual repair required for each host.
- [ ] NPM access lists, custom locations, multiple domains and WebSocket routes
      remain enforced; host bridge must never bypass NPM security settings.
- [ ] Injection/invalid DB data, concurrent sync, failed syntax/reload, delayed
      startup and certificate failures retain safe service and clear diagnostics.
- [ ] Public unknown HTTP Host, TLS SNI, raw public IP and forged Host/XFF cannot
      serve management login/UI/assets/RPC/WebSockets, for IPv4 and IPv6.
- [ ] Private LAN/tailnet access works; public ACME issuance/renewal works without
      opening management; trusted proxy/tunnel paths and spoofed headers tested.
- [ ] Named indexer and relay route to their actual services with verified TLS;
      relay WebSocket upgrade and Nostr REQ/EOSE verified separately from certs.
- [ ] Manager/NPM/nginx restart, reconciliation, disposable VM reboot, legacy
      upgrade, flat upgrade, fresh ISO and rollback preserve these protections.
- [ ] Exact OTA and raw ISO payloads contain the same correction; required
      candidate tests pass before signatures, feed promotion or publication.

The release owner acknowledged both handoffs in
`/tmp/npm-release-handoff-ack.txt`. Investigator-reported Shorty containment is
recorded separately from release tests. Do not modify Shorty nginx concurrently
or overwrite its containment until an equivalent fix is tested. Known failures
and unverified required security gates block publication.

### Latest completed release harness

The pre-NPM candidate's complete release harness passed: 1,633 backend tests
(zero failed, four optional exclusions), 1,157 frontend tests, Rust checks,
formatting, type checking, catalog/trust, runtime build contexts, doctor safety,
pruning, readiness, tunnel migration and ISO overlay regressions. This does not
cover the new NPM bridge/security implementation, which requires its own tests
and relevant repeat gates after changes. No new release has been published.

### Final handoff additions — all retained

- [ ] Investigate the existing public website TLS hostname mismatch independently;
      preserve unrelated sites and establish a baseline before attributing cause.
- [ ] Verify actual Angor client/version discovery with our indexer and relay
      settings end-to-end. New relay kind3030 zero-events versus five on the
      original Angor relay is a data/discovery difference, not API health proof.
- [ ] Resolve/document the client's actual project-query API contract, including
      the observed legacy `/api/v1/query/Angor/projects` 404.
- [ ] Retain original discovery relays alongside an empty self-hosted relay;
      do not imply that running a new relay automatically replicates projects.

Final investigator security evidence is now available in the NPM handoff. It
confirms the reported live containment but does not replace IPv6, reboot, fleet
or packaged-release tests. All those required gates remain open.

### Angor ownership and evidence correction

The operator retained the original relays alongside the new relay. Do not
attribute missing projects to the empty new relay, and do not treat the legacy
specialized-query 404 as a proven cause: current browser logs do not call it,
and sampled transaction IDs/status match the reference indexer. The Angor
investigator owns `apps/angor-*` and scoped tests/docs and will provide verified
project-flow results. This release session owns NPM, the management guard and
packaging; Angor acceptance stays open until that handoff passes.

### Transactions rail correction — operator report

The earlier computed-background check missed `backdrop-blur-md`: the rail still
painted a blurred surface even with a transparent background. Remove that effect;
only filter buttons should have visual styling. Verify background color/image,
backdrop filter, border and shadow at mobile and desktop widths, retain the
single scrolling row, then promote the corrected dashboard on the dev box.
This correction is required in the next OTA and ISO.

### Final Angor handoff retained

Read `angor-client-acceptance-20261001.md` and the 35-project recovery inventory;
receipt saved to `/tmp/angor-final-handoff-ack.txt`. Investigator verified one
complete Explore/detail/statistics flow and all 35 chain commitments. Recovery
of 34 original signed announcements remains open; queried sources did not yield
them, which does not prove global loss. Retain the upstream discovery defect
and full-recovery gate; repeat the scoped browser test after NPM migration and
OTA, preserve relay data, and include the app README corrections.

Transactions correction is now deployed on the dev dashboard (static assets
only; no service restart). Production build/type check passed. Both source and
served production browser checks at 390px and 1440px report transparent color,
no background image, no backdrop filter, no shadow and zero borders. Mobile
rail: 324px visible, 419px scroll width, one row. Testing used a disposable
browser profile with layout-only cached transactions; no wallet state changed.
A root-only dashboard backup was taken before promotion. Served index SHA-256:
`670c89a0ceb9d1e64e7460c554c642353a7e1f5bdaff1ec7d2a524135bd82f7f`.

### Reconfirmed NPM handoff and Framework deferral

The operator explicitly requested and received another receipt acknowledgement
in `/tmp/npm-release-handoff-ack.txt`. NPM certificate issuance remains a required
release gate: resolve the active flat/nested/custom data mount, preserve the
existing database/hosts/certificates, and verify fresh and legacy installation,
initial issuance, staging renewal, restart/reboot and OTA/raw ISO persistence.
The legacy shell bridge's nested-path assumptions are included in this repair.
Framework work is explicitly deferred for this task. Do not concurrently alter
Shorty's NPM/nginx or overwrite its live containment. Retain later evidence and
security gates; this repeated earlier handoff does not reset their status.

### Added requirement: Mempool UI on the Angor indexer domain

- [ ] Serve the existing Mempool explorer UI at the Angor indexer's public
      hostname root, with working assets, deep links and live WebSocket updates.
- [ ] Preserve Angor API aliases, CORS, transaction broadcast, readiness and
      verified project flow at the same origin.
- [ ] Reuse the existing Mempool stack; do not create a duplicate explorer,
      database or node, or expose dashboard/Bitcoin RPC credentials.
- [ ] Update app documentation, interface metadata, dependencies and appropriate
      versioned image/catalog entries when the implementation changes.
- [ ] Verify public HTTPS desktop/mobile UI, API, WebSockets, upgrade/restart,
      NPM routing and exact OTA/ISO contents before marking this complete.

Confirmed against the pinned official deployment guide linked in the Angor app
README: its public indexer endpoint includes Mempool frontend plus API; standard
Mempool images are supported without a custom Angor fork or ANGOR_ENABLED flag.
Our current 1.0.1 adapter instead returns service JSON at `/` and 404 for frontend
paths. Therefore UI exposure is a real missing feature, not currently implemented.
This supersedes the earlier API-only/headless requirement for the public endpoint.

### Deployment order reconfirmed

Operator requires completing fixes and available acceptance, then deployment and
verification on the dev box and yaya before release. Framework is only a fallback
when a required check cannot be established on those nodes. Yaya address and an
unused public staging-ACME hostname have been requested; dependent checks remain
pending, while source and disposable integration work continues. Release includes
OTA, catalog/app updates and raw ISO only after required gates pass.

Angor candidate 1.0.2 now proxies the existing Mempool UI and WebSocket feed.
Disposable image checks passed root/assets/deep links, actual WebSocket upgrade
and frame, API aliases/query/body, CORS/credential stripping, method/size limits,
frontend outage with API preserved, and DNS recovery after frontend/backend
recreation. This is candidate implementation, not deployed fleet acceptance.

### Further NPM qualification findings

Real same-node routing failed inside the existing pasta namespace even when the
host could reach the LAN upstream. Disposable probes using the proposed explicit
slirp network succeeded through both LAN and host-alias addresses. The manifest,
Quadlet, Podman API and first-boot paths now express that mode, with legacy drift
checks. First initialization retains a 180-second readiness budget independent of
the network driver. The full disposable NPM proxy test passed with an actual LAN
upstream, including TLS/WSS, ACLs, certificate replacement and restart. Production
NPM and its emergency routes remain unchanged pending migration acceptance.

The dev node uses a copied enabled nginx site. The initial guard edit reached
only sites-available; a live public-ingress-marker test exposed the omission.
Both helper scripts now resolve the active copy or symlink target. After repair,
dev private HTTP returns 200 and public-proxy-marked management requests 404.
Backups stay outside active nginx include directories. Do not claim the earlier
inactive-file edit as successful protection. Focused Python coverage now includes
this layout and pre-render crash recovery, with 26 tests passing.

### Added release requirement: LoRa flasher and UK MeshCore acceptance

Operator reports `esptool write_flash failed`, with both attempts failing to start
the subprocess (`No such file or directory`, OS error 2). This is an additional
release requirement; it does not replace the existing tasks or publication gates.

- [ ] Diagnose executable discovery, installation/runtime packaging, service PATH,
      missing interpreter and permissions; fail before changing the device when
      required tooling is unavailable. Do not retry a missing executable as though
      it were a transient serial fault.
- [ ] Verify board/chip identity and select the correct official MeshCore image;
      validate downloads and offsets and preserve readable device configuration.
- [ ] Test missing tool/module, incompatible version, permission denied, busy or
      disconnected serial device, timeout, flash failure and recovery reporting.
- [ ] User explicitly authorizes flashing radios attached to the dev box and
      Framework with MeshCore UK settings. Identify each radio before writing;
      retain backups where supported and verify flash, reboot, serial handshake,
      firmware identity and actual UK radio parameters on both devices.
- [ ] Verify application reconnect and communication, and ship required tools and
      fixes consistently in fresh ISO and OTA upgrades. Record physical tests
      separately from mocked coverage; no universal-success claim.

Framework deferral is lifted specifically for this requested radio diagnosis and
flash acceptance; wallet/native service work remains outside this new action.

Operator additionally reports that both Framework and dev have trouble connecting
to their radios. Add connection/reconnection diagnosis and acceptance on both
nodes: USB enumeration, udev permissions/stable paths, exclusive serial ownership,
protocol detection, listener recovery after failures/unplug/replug, and correct
visible connection state. Successful firmware writing alone does not close this
task; verify subsequent serial handshake and communication on each physical radio.

LoRa investigation update: dev's existing firmware responds as Reticulum/RNode.
After confirming no flash was active, an explicit mesh-listener disable/enable
restored connection without a firmware write or native-service restart. Candidate
code fixes unchanged-settings reconnect after a stopped/finished listener, checks
tooling before disconnection, serializes probes/configuration with flash jobs,
and retains writer ownership through timeout and post-flash cleanup. Downloads
now complete before listener shutdown. MeshCore release size/SHA-256 checks apply
to fresh and cached images; the existing V3 cache matches the official release.

The packaged flasher passes its self-check and version command on both dev and
Framework without a system esptool module. Its OTA and ISO inclusion is mandatory.
Two UI board-selection/preflight tests and type checking passed; final backend
and release-suite results are still pending. Neither radio has been flashed.

### Latest acceptance checkpoint: radio connection and release status

The complete frontend suite passed: 143 files, 1,159 tests. Type checking and
both new radio setup tests also passed. The final isolated backend build/test
run is still pending; the previous run exposed an NPM/Router port collision,
which was corrected by assigning NPM's local HTTP listener port 8088. Do not
report the previous run as fully passing or the final run as completed.

Yaya's identity has been confirmed. Its existing managed web-tunnel drop-in
clears manifest port publications and supplies private tunnel HTTP/HTTPS ports
plus the local admin port. This would suppress the candidate bridge's new
loopback HTTP/TLS listeners. Migration must preserve the working tunnel/site,
add the required local listeners, validate the managed firewall/lifecycle,
retain custom overrides, and cover repeat upgrade and rollback. The current
bridge rejects non-loopback listeners, so the supported tunnel topology also
needs explicit qualification. No tunnel or NPM runtime change was applied to
yaya during this diagnosis. Its management source guard was applied and tested:
private dashboard HTTP 200, public-ingress-marked request 404.

Dev radio connection has been restored on its existing Reticulum firmware.
Framework still does not enumerate a USB serial radio. Both nodes now have the
self-tested packaged flasher, but physical MeshCore UK flashing, post-flash
handshake and reconnect acceptance remain open pending board identification and
Framework USB detection. No device firmware has been written.

OTA, app catalog and raw ISO publication remain held. Outstanding acceptance
includes NPM migration/renewal/reboot and exact artifacts, end-to-end delivery
of the reported paid file, physical companion uploads, full Angor discovery
(the 34 missing original announcements), radio hardware tests, and the final
dev/yaya candidate deployment checks. Retained tasks above remain in scope.

### Radio models confirmed and additional serial ownership fault

Operator confirmed dev Heltec V3 and Framework Heltec V4, authorizing the already
requested MeshCore UK flashing on those models. Framework is physically connected
according to the operator but Linux still enumerates no USB serial radio; manual
USER/BOOT plus RST bootloader entry has been requested. Do not assume a missing
serial node is an application-only failure.

Dev chip query confirms ESP32-S3 with 8 MB flash. Initial read-only backup attempts
failed while a surviving Reticulum sidecar held the serial port despite disabled
mesh status. The exact owning sidecar process group was identified and terminated
gracefully; the subsequent exclusive backup progresses normally. Source review
found that cancelling the asynchronous sidecar startup before its ready event
bypassed ordinary error cleanup. A new owning startup guard terminates the group
on cancellation as well as error, with an isolated real-child regression. Final
validation of this additional fix is running; it was not in the prior passing run.

The preceding full backend run passed 1,647 tests with zero failures and four
ignored. Complete frontend suite passed 1,159 tests. Added an explicitly named
EU/UK Narrow preset (869.618 MHz, BW62.5, SF8, CR4/8), retaining existing plans;
these parameters match the MeshCore app maintainer's presets in upstream
MeshCore discussion 1650. Neither physical flashing nor reconnect acceptance
is complete at this checkpoint.

### Dev Heltec V3 physical flashing result

Full 8 MiB pre-flash backup saved privately with mode 0600 and checksum. After
removing the confirmed stale radio sidecar, the exclusive read completed.
The normal mesh.flash-device RPC then flashed the official Heltec V3 Companion
USB v1.17.1-d929643 merged image successfully (100%, no error). The image's
size and SHA-256 were checked against the official GitHub release metadata.

Independent serial protocol queries confirmed model Heltec V3, firmware
v1.17.1-d929643 and actual RF readback: 869618 kHz, 62500 Hz bandwidth, SF8,
CR8 (EU/UK Narrow). This is device readback, not merely saved host settings.
The listener was then re-enabled and reported connected as meshcore. No wallet
or native Bitcoin/LND service restart was used. MeshCore remote reboot is not
supported by the current API; that attempted check returned an explicit error.
Physical unplug/replug and communication to Framework remain pending.

Live qualification additionally found incorrect binary DEVICE_INFO/SELF_INFO
parsing and a stale host-side RF-applied marker after full-chip flashing.
Candidate changes decode the current official binary layout, query the actual
firmware version during initialization, and invalidate the RF marker after a
successful flash. The dev marker was backed up and cleared before provisioning;
the independent readback above confirms settings applied. New parser, startup
cancellation and marker lifecycle regressions are queued/running; the prior
1,647 passing tests do not cover these later changes.

Framework's V4 official USB image has been downloaded and verified. Despite the
operator confirming it is plugged in, repeated sysfs/device checks show no
ESP32 USB or serial port. USER/BOOT plus RST bootloader entry was requested;
Framework has NOT been flashed and the two-device acceptance remains open.

### Operator deferral and latest qualification

Operator explicitly deferred Framework radio/hardware work and instructed us to
continue all other release tasks. Framework V4 flashing, USB reconnect and
radio-to-radio acceptance remain UNVERIFIED / OPERATOR-DEFERRED; this is not a
pass. Do not request further Framework radio operations unless needed and the
operator resumes that work. Dev V3 acceptance and durable fleet fixes remain.

The final radio backend suite passed 1,650 tests, zero failed, four ignored,
including sidecar-startup cancellation, current MeshCore metadata parsing, and
post-flash RF-marker invalidation. Frontend baseline remains 1,159 passed.

Correction to the earlier yaya tunnel concern: direct inspection of the active
Quadlet and all drop-ins confirms web-tunnel.conf ADDS private tunnel ports; it
does not contain an empty PublishPort reset. The earlier statement that it
cleared manifest listeners was incorrect. The new loopback publications therefore
coexist declaratively without editing that working tunnel drop-in. The bridge
validator now permits only the known HTTP/TLS tunnel ports bound to a currently
assigned RFC1918 address on an actual WireGuard interface named wg-web; it still
requires a separate loopback upstream, and rejects wildcard/public/unassigned
bindings and any admin-port exception. Python bridge/guard tests: 27 passed.
Actual yaya candidate upgrade and public route acceptance remain pending.

### NPM public certificate and restart qualification checkpoint

- Main backend: 1,651 passed, zero failed, four explicitly ignored hardware /
  external integration tests. Container runtime library: 80 passed, zero failed.
- Bridge/management guard Python suite: 27 passed. Shell syntax and actual ISO
  overlay-content test passed.
- Candidate validator inspected yaya's real runtime/WireGuard interface and
  accepted its existing web tunnel publications while selecting proposed
  loopback HTTP/TLS upstreams. This was read-only, not a runtime upgrade.
- Existing yaya public HTTP ACME route returned the exact random token body
  written inside NPM. Lets Encrypt STAGING initial issuance and renewal dry run
  succeeded using separate temporary account/config/work/log storage. Current
  production certificate and host records were not replaced. Public site HTTP
  and trusted HTTPS retain their authentication requirement (401).
- First renewal harness timed out while Certbot used a 292.7-second randomized
  delay; the remote log confirmed successful simulated renewal. A deterministic
  rerun with --no-random-sleep-on-renew returned exit 0 and success confirmation.
  Only the temporary staging directory was removed afterward.
- Real disposable NPM nested-layout test completed: namespace LAN upstream,
  API host creation, custom locations, client-IP spoof rejection, exact ACME
  token, trusted TLS/WSS, certificate replacement, password/network ACLs,
  enable/disable/delete, and restart with retained DB. Latest restart took 7.6s.
  Earlier rerun exceeded the fixture's 90-second restart window; the test now
  uses the manifest's 180-second budget and records both route/admin statuses
  and container state on failure. Do not erase that earlier observed failure.
- Cleanup was hardened to continue cleaning other fixtures after a timeout.
  Two early test runs passed functional assertions but failed cleanup; their
  leftover disposable containers/networks were explicitly removed. The latest
  full run exited successfully.

Legacy non-Quadlet repair now retains the old container for rollback, restores
it after replacement failure, preserves its exact image and environment values,
and waits for HTTP API readiness. Environment values use an exclusive mode0600
file cleaned on drop, not argv or the host process environment. Invalid/multiline
entries fail before stopping the original. An occupied rollback slot is preserved
for review rather than deleting an unknown container. Live interrupted-migration,
additional operator-override and rollback acceptance remain OPEN; unit success
is not proof of those deployment paths.

The deployment backend and frontend build are in progress. Full candidate dev/
yaya upgrade acceptance, reboot, exact signed OTA/catalog and booted raw ISO
remain open. Framework radio is operator-deferred, not passed. The original paid
file bytes, physical companion upload and 34 missing Angor announcements remain
separately tracked.

### Further completed acceptance

The complete disposable NPM test now includes a deliberately failed replacement
with an occupied host port. Restoring the retained container preserved its exact
container ID, database, configured hosts and authenticated API access; the test
exited successfully and cleaned its fixtures. This verifies the Podman rollback
mechanism, not yet the full installed backend's legacy-repair entry point.

Dev frontend build completed and was deployed with a separate rollback backup.
Served production Transactions layout passed at widths 390 and 1440: transparent
background, no image/blur/shadow/border, nowrap and exactly one row. Mobile rail
is 324px wide with 419px scroll content. Backend candidate compilation is still
in progress, so the latest backend changes are not yet deployed.

Dev Bitcoin remains unpruned and in IBD (observed block543676/header969495,
verification progress0.2284). This is not full-chain Angor acceptance. The operator
has been asked which seller and filename identify the failed Lightning purchase;
the earlier recovered Framework-seller invoice is not confirmed as that purchase.

### Read-only Shorty migration preflight: remaining ownership conflict

Preflight found both flat and nested NPM databases. The live container's explicit
/data mount identifies the active one, so the candidate resolver now uses that
verified mount (or its saved validated receipt after a managed stop), preserves
both databases, and still refuses multiple databases without an authoritative
selection. Python coverage verifies both explicit choices and unchanged bytes.
This helper update occurred after the deployment binary build started; a final
release rebuild must include it. Do not claim the in-progress binary contains it.

After resolving storage, the two Angor emergency routes match the exact known
handoff templates. Another existing file, shop-btcpay.conf, conflicts with an
enabled NPM record: the manual route supplies HTTPS using certificate10, while
the NPM host currently has certificate_id0 and SSL forcing disabled. The manual
route and NPM record cover the same two public names and backend web port. Blindly
retiring the route would break its HTTPS. No database, host, certificate, route
or runtime was changed on Shorty. The bridge correctly refuses this ownership
conflict and now names its configuration file in the diagnostic. Align TLS/route
ownership and verify the public shop before retiring that manual configuration;
Shorty's full migration acceptance remains OPEN. Preserve live containment.

### Candidate deployment and additional real-upgrade ACME regression

The operator explicitly deferred Framework's physical radio investigation and
requested continuation of all other work. Framework radio remains unverified.
Dev and yaya now run the backed-up unpublished backend candidate SHA256
4c47269b3480ca0362df18dae160c073a19ea33507e04cacdad5b96837990ca6 and production
frontend index 3099c4ba44528a4a4c524f9a26159414558efa16abdd12cc7bfd64376cbb6089.
Both management health checks passed; native Bitcoin/LND container identities
and start times were unchanged. Yaya public site retains trusted TLS and its
401 authentication requirement; NPM admin API200, private dashboard200 and
public-ingress-marked dashboard404. The first yaya staging attempt stopped
before binary replacement because rsync was absent; deployment now uses Python
copying without that dependency and completed successfully.

Actual startup exposed an additional regression: the canonical nginx template
had only HTTP ACME, while the bridge demanded two locations. Startup rewrote the
previously repaired config and the bridge rejected it before fixing the nested
root. Source now adds HTTPS ACME to the shipped template and migrates the exact
recognized legacy default-server layout; custom/ambiguous layouts still fail
closed. The missing-token route must return404 rather than the dashboard SPA.
28 Python checks passed. The real isolated nginx suite now starts from the
legacy missing-HTTPS layout, applies the migration, and passes all120 public
negative cases plus HTTP/TLS exact-token, private-access and reload checks.

Applied the latest helper and its atomic ACME-only repair to yaya: actual token
written inside NPM returned exact200 over host HTTP, host HTTPS and public HTTP;
missing tokens returned404 for allthree. Public site trustedTLS/auth preserved.
Local self-signed host HTTPS was tested with certificate verification disabled;
public site HTTPS used normal certificate verification. Shorty was not modified.
The running backend still embeds the older helper/template; final rebuild is
REQUIRED before restart/reboot/persistent upgrade acceptance can pass.

The prepared unsigned catalog validates with zero metadata drift and trusted
registry hosts. Its only changed entries are NPM and Angor indexer1.0.2. It has
not been signed, installed or published. Yaya's current signed catalog retains
NPM's old pasta network/tunnel-only HTTP+TLS listeners; full NPM runtime/bridge
migration acceptance awaits the reviewed signed catalog. Do not mistake the
backend/UI deployment or ACME-only fix for completed catalog migration.

### 2026-10-02: rebuilt candidate deployed; private catalog qualification prepared

Release-profile candidate build completed successfully. SHA256:
af0648ad4ef8b6183d3c1ff485721fa40b12bb730c6e0322bc5f209ed06fce39.
Focused bootstrap tests:10 passed. Full isolated backend rerun:1651 passed,
zero failed, four explicit hardware/external ignores. Python guard/bridge28
passed again. No new source changes occurred between these checks and deployment.

Deployed this rebuilt backend on dev and yaya, with private previous-binary,
nginx and native-container baselines. Both manager health checks passed. A later
post-startup comparison confirmed Bitcoin/LND identities/start times unchanged.
The installed bridge helper now matches latest source bytes after restart.
Private UI200, marked-public HTTP/HTTPS404 and missing HTTPS challenge404 passed.
Dev HTTPS intentionally binds its LAN/WireGuard addresses, not127.0.0.1; an
initial loopback probe got connection refused, corrected to the actual listener.
This was a test-address error, not a product outage. Local self-signed HTTPS
checks skip certificate verification; public-site TLS checks use normal trust.
Full-machine reboot qualification is still pending.

Prepared a fresh candidate catalog with only NPM and Angor indexer entries changed.
Metadata drift0; registry trust check passed. Unsigned SHA256:
5801309bf21d3f6fd03ce5702d68b383a518f734092bf265f5e0ad243095a25e.
NPM's new manifest is capability-gated by runtime-migration-backup-v1; older
nodes retain the original manifest. This candidate is for private qualification,
not fleet publication. An operator-only hidden-input signer validates the exact
catalog and binary hashes, checks the pinned release root and restores the
unsigned original on failure. Its noninteractive refusal was tested. Signature
is required before testing through the nodes' normal trusted-catalog path.
No catalog, app image, OTA or ISO was published. Framework remains deferred;
all other open requirements retain their previous status.

### Signed catalog and private qualification selector

The operator signed the qualification catalog. Cryptographic release-root
verification passed locally and on yaya; after removing signature envelope fields,
its contents exactly match the reviewed unsigned candidate. No publication.
The catalog is staged under /var/lib/archipelago/qualification on both test nodes,
with previous catalog/app metadata and container identities privately backed up.

Normal mirror loading deliberately forces the public origin first, so simply
prepending a private mirror cannot reliably test an unpublished candidate.
Implemented ARCHY_APP_CATALOG_CANDIDATE as an explicit absolute-file selection:
requires an anchored release-root signature, validates before cache replacement,
retains exact signed bytes, does not alter mirrors/trust, and fails without
public fallback when the selected file is invalid. Added unsigned, tampered,
wrong-key, malformed, missing, oversized, relative-path, valid and idempotent
coverage. Full isolated backend suite:1653 passed,0 failed,4 explicit ignores.
The optimized selector build is still in progress; selection is not enabled yet.
See docs/candidate-catalog-qualification.md for activation and mandatory removal
once the tested public catalog is available. Do not leave test nodes pinned.

Yaya NPM preflight:8088/8444 are free, active public host has no conflicting
host-nginx ownership, database tables and certificate-file hashes saved privately.
No Shorty mutation. Dev public Angor reference acceptance passed TLS/WSS, exact
funding commitment, official Explore and detail/statistics again; this still
checks only the known original project, not all35. Dev Bitcoin continues syncing
(reported sync_progress approximately0.307); full-chain acceptance remains open.
Current tested hardware product codes:dev20CLS7S900 and yaya20CLS6BH00, both Podman
5.4.2; kernels6.12.74+deb13+1-amd64 and6.12.107+deb13-amd64 respectively. Framework
remains deferred. No Docker or new ISO-boot acceptance is implied.


## Live Lightning purchase: Framework to Shorty — 2026-10-02

Operator explicitly authorized a very small new test purchase, then performed
it from Framework. This is separate from recovering the Amish Paradise sale.
Fixture: `archy-lightning-delivery-test-20261002.txt`, price 1 sat, Lightning only.
Read-only checks confirmed one matching seller invoice, SETTLED for exactly
1 sat, and Framework payment SUCCEEDED for 1 sat with 1 sat routing fee
(1,000 msat). Total spent was 2 sats. The assistant sent no payment.

Framework has exactly one durable purchased-content ownership entry for the
fixture, with backend `lightning`, paid_sats=1 and size_bytes=121. Its cached
file SHA256 equals the original seller fixture:
`d55f7a6acd77bdc3c35c65ecac6d1492096e99252d07d433e6286544540cde7e`.
**PASS: actual node-wallet payment, seller settlement, delivered bytes and
persisted buyer ownership/cache.** Framework runs the candidate backend
`8fb6249d1869bb8c9aea26d0f846de5b3eec328113a5c7f573628306e26e652e`;
Shorty runs `e108b78bbbd21cb7d5d47c8d0b7b9b19b63fb0c44678773603202440ec7d6f5b`.
This also exercises the candidate buyer against the existing seller version.

Live reopen without payment and restart acceptance are not established by
this check. Shorty had no matching durable entitlement JSON in the inspected
location; do not attribute the candidate seller persistence implementation to
this older seller binary. No node or wallet was restarted. The tiny fixture
remains available for a free cached reopen check; remove only its catalog
entry/source file afterwards, preserving buyer ownership and payment records.


### Operator-confirmed free reopen — 2026-10-02

After the verified one-sat purchase, the operator reopened the file on Framework
and confirmed it worked without another payment. **PASS: live paid delivery,
durable buyer ownership/cache, and free repeat access**, with independent
settlement/byte checks above and operator confirmation of the reopen UI.
This closes that specific live acceptance check; it does not establish an
untested restart, outage, or seller-upgrade scenario.

The temporary seller catalog entry and source fixture were removed after
acceptance. Buyer purchased bytes/ownership and all payment records were preserved.


## Release-agent continuation: Bump production deployment — 2026-10-02

Authorized network access was restored without changing credentials. The
previously running optimized build completed; its Bump implementation was
verified present and artifact frozen separately from subsequent Fast-default
and NPM source edits. Backend SHA256:
`af0cf7bd8bdc60c7b29976c11cbc002c46979be5120909f169856f8bd6e71058`.
The approved production archive retained SHA256
`d0e253aba09ad257136e3feb5b63176c388f3bb968f560702eefb768d29e494d`.

The reviewed staged rollback deployment ran successfully on Framework. Manager
health200, native Bitcoin/LND container IDs and start times unchanged, LND
wallet identity/channels/balance unchanged. Actual rollback files now exist
under `support/framework-fee-bump-20261002`. Served UI index SHA256:
`9fef18c8c1c9bc21f43c3635b747dfcfbea965b096867b454bbf608121715438`.
The approved Bump layout and shared green animation were preserved. No bump,
channel operation or payment was submitted. Normal authenticated quote/status
acceptance needs a fresh dashboard TOTP login; requested from the operator.

Later source work, NOT in that deployed artifact: Fast defaults for send,
channel open/cooperative close, reset/reopen and hardware PSBT estimates.
97 focused frontend/client tests and three isolated fee-policy tests passed.
Full integration/release validation remains pending. NPM legacy gateway support
was added across runtime paths with a separate catalog capability, but the
real integration test exposed lost client-IP preservation on that subnet;
this remains an explicit blocker until corrected and retested. Neither the
new catalog nor these later backend changes has been deployed or published.


### Combined continuation validation checkpoint

Fast-default, Bump and App Store UI changes pass the complete frontend suite:
1,176 tests in146 files, zero failures. Production TypeScript/Vite build passed.
The App Store change filters Cuprate/NetBird implementation parts from signed,
community and persisted catalogs, collapses BTCPay aliases onto the canonical
Commerce app, and keeps installed legacy-only BTCPay visible. Thirty-four
focused grouping/launch checks passed; actual yaya browser acceptance is running.

NPM legacy host gateway correction now passes the complete disposable integration:
LAN/host alias/old gateway requests from its namespace; forwarded client IP;
spoofed headers; custom paths; real HTTP/TLS/WSS; ACME exact file; password/network
ACLs; certificate replacement; restart; forced-failure rollback; disable/delete.
The trusted rootless forwarding source is169.254.1.100, added as one managed block
through NPM's supported custom http_top.conf include. Existing custom directives
are preserved with a private pre-change copy; symlink/modified managed blocks
fail for review. A read-only bind under conf.d was rejected during qualification
because NPM's startup mutates that directory; it is absent from the final source.
Twenty-three Python bridge checks pass. Older gateways and manifests still need
the new signed capability variant and actual upgrade acceptance; no fleet release.

### Yaya App Store grouping deployed — 2026-10-02

The dashboard fix is now deployed on the affected yaya-server. Actual served
index SHA256 is `076290e992a18fe418ea5ad411007cffe3280608c63576da2587d1cc371a50e4`.
The previous dashboard is backed up under
`/var/lib/archipelago/support/app-grouping-ui-20261002`. Every container ID and
start time matched before/after; this UI deployment did not recreate apps.

Authenticated live Chromium checks at 390px and 1440px show exactly one Cuprate,
one NetBird and one BTCPay Server, with loaded icons. Repeated checks after a
hard reload pass at both widths. Acceptance used the actual served dashboard,
backend and signed catalog. The earlier local asset-overlay attempt caused a
Chromium private-network classification error and is not counted as acceptance.
Installed-component hiding and legacy-only BTCPay preservation have automated
coverage; these browser checks did not install these products on yaya.

The latest strict custom-fee validation adds eleven invalid-input cases;
95 focused SendBitcoinModal/RPC tests pass after that change. The subsequent
TypeScript/Vite production build passes. Prior full frontend suite: 1,176 passed.
Combined optimized backend and final-source isolated backend rerun remain in
progress. No public release, catalog update or ISO has been published.

Follow-up audit: Marketplace.vue still uses plain desktop/mobile search inputs;
extend the existing shared clear-search control to this category view before
claiming the earlier all-App-Store search requirement complete.

Additional live category checks pass: BTCPay Server appears in Commerce and is
absent from Money. The final browser run passed all listing, icon, hard-reload
and category assertions; its trailing optional screenshot timed out after font
loading. This capture failure is recorded separately, not reported as a fully
successful harness exit. Earlier actual-node desktop/mobile screenshots exist.

### 1.9.0 continuation checkpoint

See [the current 1.9.0 acceptance summary](release-1.9.0-acceptance.md).
The category-view clear-search gap is fixed and verified on yaya at 390/1440px:
click and Escape clear, focus is retained, button remains inside the field,
heights 52/40px. Dev mobile passed; a companion introduction and then resource
alerts obscured the desktop test, and a later navigation timed out under build
load. Dev desktop must be repeated after the build; these attempts are not passes.

Read-only Framework recheck: CryptPad/Core Lightning containers remain absent,
uninstall markers retained, all three real Immich containers running continuously
since 2026-09-21. The duplicate Immich entries were synthetic inventory, not actual
restarting databases. Rendered Framework inventory still requires normal dashboard
authentication. No Framework native service was changed by this check.

### Verified ledger reconciliation — 2026-10-02

Completed checkboxes above now reflect the retained source, automated, live-node
and operator evidence rather than leaving those accepted tasks apparently open.
Seller settlement/persistence and buyer exact-byte one-sat purchase/free reopen
are recorded separately; no additional payment was sent. CryptPad removal and
uninstall markers survive the recorded manager restart. Actual Portainer namespace
smart HTTP and Compose access passed after the current combined deployment.
The installed/stopped/removed upgrade matrix, physical companion route, Framework
rendered Immich inventory, final Fast-default acceptance, NPM signed migration and
final artifacts remain open. No artifact-wide reboot acceptance is inferred.

Dev category search now passes 390px and1440px, including click/Escape, retained
focus and40/52px field heights. The earlier build-load timeouts remain recorded.

The final audit found the fee-only child grouping requirement still outstanding.
A source correction now groups only a recorded simple CPFP child verified against
wallet ownership, input relationship, fee-only delta and current chain/mempool
state. It preserves recipient amount, excludes replaced fees from the total,
exposes linked fee history and targets the current child for another Bump.
Focused UI tests pass; new backend and real-regtest history checks are pending.

### Signed qualification — 2026-10-05

See the current1.9.0 acceptance record for exact hashes/evidence. Signed catalog
and final payment/history corrections are deployed on dev/yaya; actual regtest
with grouped history passes. Yaya NPM's managed gateway works without its
temporary override, with preserved DB/certificates, actual upstream access and
manager-restart/reconciliation stability. ACME/public application and Portainer
checks pass. Full-machine reboot, final artifacts and remaining operator/Angor
gates are still open; nothing published.

## Operator checks accepted; upload UX amendment — 2026-10-05

Operator reports the requested human checks worked perfectly: Shorty shop SSL,
physical upload flow and Framework dashboard/purchased-file checks. This is
operator acceptance, not a claim of newly independent device testing. Read-only
Shorty verification confirms shop certificate_id12 and Force SSL enabled.
Remaining migration/artifact/security and Angor requirements still apply.

Operator supersedes the globally persistent upload-bar requirement: keep the
bar only on the screen where the batch originated, continue transfers across
navigation, show explicit Complete on successful server save, and use a
completion notification elsewhere. Source now retains the originating route,
removes the global floating bar, keeps the original44px inline bar, and reports
success/error/cancellation distinctly.25 focused store/component/notification
tests pass. The subsequent full frontend suite passed1,193 tests; production
build and actual served desktop/mobile real-upload checks passed. Deployed to
dev/yaya with index SHA256
`86bb728017b118d8e98f032419e7fbfd6ecd78b7e464c982a2075cc38c582814`;
no apps or backend services restarted. Retain this as the preceding UI evidence,
not evidence for the later resumable-upload implementation. No new payment was requested or performed.

### Resumable upload addition — 2026-10-05

Operator requests recovery after a background pause or connection loss. The
installed File Browser identifies as2.63.23/e8a388f8 and supports TUS. Cloud now
has a candidate chunked upload implementation: random same-folder staging path,
server-offset reconciliation, transient retry/online/visibility recovery,
cancellation, final SHA256 verification and rename. Lost final chunk/rename
responses are reconciled without restarting or accepting a same-size old file.
The original-screen-only44px bar, Complete label and off-screen notification
remain. Fifteen focused protocol tests pass; full build/deployed fault injection
are in progress. This is not yet live acceptance.

Recovery requires the selected File to remain available in the running page.
An OS-killed app or expired server upload session may require reselecting the
file. Do not promise uninterrupted background execution or restart persistence.
This gate is additional to the already accepted physical upload flow.

## ngit PR integration — 2026-10-05

Both requested proposals are merged and pushed to Gitea and ngit main at
`2c1bcacf`; ngit independently reports both as `applied`.

- `494d2483`: opt-in NODE_IDENTITY_PUBKEYS for app owner allow-lists. Review
  corrected ECMAScript/Rust whitespace differences and added strict public-key
  validation. Appliance identity excluded; no private keys or signing capability
  given to apps. Existing manifests and the native signing flow are unchanged.
  Documentation explicitly describes linking all offered user identities.
- `c18ebd7f`: nostr0.44.7 and nostr-relay-pool0.44.3. The standalone relay pool's
  maintenance advisory remains; SDK0.45 migration is a separate follow-up.
- Combined isolated backend suite:1,678 passed, zero failed,4 explicit ignores.
  Real loopback hostile-relay test rejects altered content, author and signature
  reusing a known DB event ID while accepting a valid event. NIP04/NIP44 normal
  encryption and hostile/oversized payload tests pass. The initial relay harness
  returned before connection establishment; corrected to wait for an actual
  connection before fetch, and the complete rerun passes.
- Evidence: /tmp/archy-190-ngit-complete-tests.log, origin/ngit push logs and
  /tmp/archy-190-ngit-postmerge.json. This is source publication, not OTA/ISO or
  catalog publication. Later File Browser credential changes need a new suite.

## File Browser secure automatic login — NEW REQUIRED GATE

Operator requests unique per-node credentials, working Cloud from first launch,
no admin/admin and fleet-wide testing. Framework's reported authentication issue
recovered, which is not proof that this gate is fixed. Yaya rejects the saved
password with403 despite healthy File Browser2.63.23. Never count that as a
passed upload test.

Confirmed source issues: first-boot paths still try noauth/admin defaults; the
post-install hook assumes admin/admin and uses an incompatible password-change
request shape; the generated ISO path updates a running DB and uses a different
DB filename; Cloud hardcodes admin and invents admin/admin on missing secrets.

Candidate scripts/filebrowser-credentials.py now provisions a random username
and256-bit password offline with the pinned app image, backs up the selected
DB/config, preserves custom accounts, tests automatic login plus folder access
in a network-isolated container, rejects unauthenticated access, rotates only a
proven admin/admin login, and atomically publishes a0600 credential record.
Fresh real-image acceptance passes. Legacy/default/custom/restart/rollback,
first-boot/Quadlet/runtime wiring, live yaya/dev/Framework qualification and final
artifacts remain OPEN. No live File Browser account or DB has been modified.

Upload resume: source/build/full frontend1,208 tests passed; subsequent48 focused
protocol/client tests passed after filename escaping correction. UI deployed on
dev/yaya index SHA256
`31ac7bcc704c18f88a8b9800fb46bc7941651983e1a99b97d036a8d9e95a58b5`.
Actual dev1440/390px real-server fault injection passed partial offset123456,
offline reconnect, lost final PATCH and rename replies, exactSHA256, encoded
filenames, original-screen-only44px bar, notification, cancel and empty files.
Yaya is blocked at the credential gate above. Physical suspended/killed-app
acceptance is not inferred from these viewport tests.

## 2026-10-05 resumed release qualification

- Latest full frontend: 1,210 tests passed across 148 files. Production Cloud UI
  and AIUI builds passed. Dev and yaya serve index SHA256
  `3e10a25db75e4712310eb34c98bf7595ad5db3a444f9126a73715b1d40493a33`;
  UI archive SHA256 `586d1864c5c4027086194f6b5951a9b770c4e7ce9ba9ec3128e2ac0c1bde55e7`.
  Private UI backups: `/var/lib/archipelago/support/cloud-auth-20261005`.
- Real dev browser upload tests pass at 1440/390px, including interrupted JWT
  refresh, partial write, offline recovery, lost final PATCH/rename responses,
  exact SHA256, encoded filenames, cancellation, empty file, origin-only 44px
  bar and completion notification. Initial run overlapped UI deployment and
  failed navigation/bar timing; kept as failed evidence. Clean rerun explicitly
  verifies successful navigation and passes both viewports. Physical OS suspension
  and yaya authentication/upload acceptance remain separate gates.
- Real File Browser image matrix passed fresh, legacy-default, legacy-custom,
  legacy-noauth and forced-failure exact DB rollback. Existing file bytes and
  user IDs/permissions preserved; custom credentials preserved; admin/admin and
  anonymous access rejected. Actual disposable Quadlet pre-start and restart
  also pass, with stable managed credentials. Four Python unit tests pass.
- File Browser startup integration now covers the direct runtime, Quadlet,
  first boot and ISO script. Binary bootstrap installs its matching helper before
  reconciliation. Fixed bundled first-boot missing NET_BIND_SERVICE and duplicate
  creation attempt for a stopped File Browser. Live credential migration is still
  pending the optimized backend build; no production DB/account modified yet.
- Final combined isolated backend suite: 1,681 passed, zero failed, four ignored.
  An earlier run failed the Nostr relay fixture after a normal ping closed its
  text-only receive loop. Fixed the fixture to answer pings; the complete rerun
  passes. No failed run is counted as acceptance.
- NPM: 23 Python tests pass, including exact emergency BTCPay route recognition,
  operator edit preservation, missing certificate/alias refusal and transactional
  rollback. Existing emergency Angor routes now also require complete TLS
  replacements before retirement. Actual disposable flat-layout NPM integration
  passed namespace reachability, legacy gateway, ACME exact bytes, forced HTTPS,
  WSS, certificate replacement, password/network ACLs and forged-header rejection,
  restart, disable/delete, and forced bind-failure restoration. This is not a
  staging-CA issuance/renewal or ISO/reboot pass.
- Shorty read-only inspection confirms shop certificate12 and Force SSL with both
  hostname aliases; old manual shop route still uses certificate10. No live
  Shorty routing change in this qualification. Migration remains pending.
- Evidence logs: `/tmp/archy-190-final-combined-backend.log`,
  `/tmp/archy-190-cloud-auth-ui-dev-live-2.log`,
  `/tmp/archy-190-filebrowser-final-integration.log`,
  `/tmp/archy-190-filebrowser-quadlet.log`,
  `/tmp/archy-190-npm-final-integration.log`.
- OTA/catalog/raw ISO publication remains held. Framework radio deferred;
  Angor 34 unrecovered original announcements and dev full-chain acceptance
  remain open. README alpha/funds notice is separately published to both remotes.

## Live File Browser ownership regression — publication hold

2026-10-05 dev candidate backend SHA256
`3e01da72fcea0a61852f3d9038e67630e328c65d6433da749671d60b91c37ffa`
built successfully, then failed live credential migration before DB mutation.
The helper could not create its private backup under the legacy data-directory
owner (host UID100000). The original real-image fixtures aligned data ownership
to the image UID and therefore missed the shipped manifest's different mapping.
The managed File Browser has DAC_OVERRIDE for that layout; the helper did not.

Restored prior backend SHA256
`cfddec834a53609f8bef924f3905da76df45f22426cac2628c4c2cb06bea5d09`
and original File Browser Quadlet with the staged rollback script. Both services
are active. Yaya backend was not changed. No candidate credential record was
published; failed setup stopped at backup-directory creation before DB changes.

Source helper now includes the managed server's DAC_OVERRIDE storage capability;
new real-image legacy-owner and actual-Quadlet fixtures reproduce that mapping.
Rollback fixture now forces an account-policy failure after noauth migration so
it still verifies restoration after a real DB mutation. These revised tests and
combined backend validation are in progress. A corrected embedded-helper build
and new live qualification remain required. Do not reuse the failed binary as
final release or mark the credential gate passed from earlier fixture results.

Release-note drift corrected to1.9.0/current upload behavior and File Browser/
Nostr additions. The checker now rejects stale descriptions/dates for an existing
version; its regression passes. Latest notes UI built and deployed dev/yaya index
SHA256 `97aab07e67eccc1bb3d215534b537b83e1372bf5c36b505b6127a24a2b629e23`.
Phone background/reconnect acceptance question is pending, not passed.

## Mobile Cloud media viewer — 2026-10-05 release addition

Operator screenshot shows the filename behind the companion status bar and a
cramped video viewer. Confirmed mobile CSS positioned every toolbar button at the
same coordinates; fullscreen was only attached to a video double-click, and the
viewer ignored the companion's `--safe-area-top/bottom` values. Legacy global
lightbox maximum dimensions also constrained the component unexpectedly.

The viewer now reserves separate safe-area-aware title, media and action rows on
phones, keeps each action at least44px without shrinking, and places previous/next
beside the action row rather than over the media. Videos retain their intrinsic
picture ratio with native playback controls. Photos and videos have a labeled
fullscreen action: standard fullscreen where supported, native Safari video
fallback, and an expandable in-page viewer when embedded browsers deny it.
Escape/exit and keyboard focus remain usable. Decode failures offer retry, and
late media requests cannot replace a newly selected file or leak their blob URL.

Validation:11component tests pass, including existing PiP handoff, fullscreen
success/denial/Safari fallback, decode retry, fetch ordering and focus restoration.
Real Chromium checks at320x568,390x844,844x390 and1440x900 pass safe-area/control
geometry, fullscreen and exit, generated video playback and close. Screenshots
were inspected. This does not claim physical companion/Safari acceptance.
Evidence: `/tmp/archy-190-lightbox-focused.log`,
`/tmp/archy-190-lightbox-browser.log`; repeatable browser fixture
`tests/lifecycle/media-lightbox-browser.cjs`.

The operator separately accepted both physical phone upload background/reconnect
and Framework Cloud folder/upload/open checks. Those upload manual gates are
closed; this newly reported media viewer gate is separate. The ongoing candidate
ISO and staged OTA predate this addition and must not be published as final;
regenerate final artifacts and hashes after this fix is qualified.

### Permanent file menus and companion fullscreen follow-up

The operator additionally reported that touch users cannot reach hover-only file
actions without opening the file. Grid and list cards now have a permanent44px
translucent ellipsis action, and owned-file lightboxes expose the same menu.
Share/download/delete are available without opening the underlying file; delete
requires a separate explicit confirmation. Unsupported actions are omitted for
non-owned callers. The menu stays within the viewport, participates in native
fullscreen, traps keyboard focus, dismisses on Escape/outside tap and restores
focus. Cloud-folder delete failures now remain visible instead of becoming an
unhandled rejected promise.

Sixteen focused component tests pass. Real Chromium grid/list touch checks pass
at320,390and1440px, covering permanent visibility, unclipped menus, share and
cancelled delete with no preview triggered. Lightbox action access also passes
inside fullscreen at all four prior viewport sizes. The actual deployed dev
Cloud screenshot and3840x2160video decode successfully (75.633seconds, no media
error); native Chromium fullscreen/exit/close pass. No operator files changed.

The Android companion has no existing `onShowCustomView` implementation. Added a
shared fullscreen host to both dashboard and app WebViews: retains the current
WebView, accepts Chromium's custom view, hides system bars with swipe escape,
handles Back and Chromium exit, restores prior bars, releases the view on screen
disposal and rejects duplicate/reparented requests. Kotlin compilation passes.
Companion version0.5.33/build53 is reserved for this change. Lifecycle tests,
clean signed APK build and physical companion acceptance remain required; the
web fallback is not evidence of native Android fullscreen acceptance.

Updated qualification: all **1,222 frontend tests / 150 files** pass, production
build passes, and the permanent menus are deployed on the dev box. Live browser
checks pass for real Cloud photo/video decode, card-menu access without preview,
cancelled deletion, and the viewer's action menu during fullscreen. No files were
deleted or shared by the tests. Android's three lifecycle tests pass (zero errors
or failures). The clean APK build initially failed because the expected signing
keystore was absent. Recovered the existing local key after matching its public
certificate exactly to the currently served APK; a clean packaging retry is in
progress. No replacement signing identity was generated, and no private signing
material is included in this change.

Companion packaging exposed an additional release-script defect: noisy successful
`apksigner` output caused `printf | grep -q` under `pipefail` to return141/SIGPIPE,
rejecting an APK whose v1/v2/v3 verification results were all true. The publisher
now uses input redirection for these checks and additionally pins the existing
companion certificate, protecting in-place update compatibility. Four executable
regressions exercise the actual verification block: large valid output, missing
signature schemes, wrong signer and verifier failure. All pass; the test is
included in `tests/release/run.sh`. A fresh canonical clean/package/sign run is
required after this script fix; no failed packaging attempt is marked published.

### Companion download regression — operator report after build53

The operator accepted the improved viewer, then reported Download did nothing,
confirmed companion-only. Real Chromium downloaded the exact Cloud screenshot
bytes (202,648 bytes; matching SHA256), so this is separate from the web menu.
Both companion WebViews lack a general DownloadListener. Added a shared native
Save dialog/download handler, with bounded streaming, existing WebView cookies,
progress, cancellation and deletion of the newly created incomplete destination
on failure. Redirects retain cookies only for the starting origin, HTTPS
downgrades are rejected, and authentication/login-page failures do not save an
error page as the user's file. TLS verification stays enabled. No broad storage
permission is introduced. Blob/data URLs currently report unsupported instead of
silently doing nothing; own Cloud files use authenticated HTTP(S) raw URLs.

Companion0.5.34/build54 is reserved for this repair. Compile, network regression
suite, canonical clean signing, dev deployment and a real phone download remain
required. Build53 must not be described as having working companion downloads.
The existing fullscreen suite also passed its Android28+35 matrix: six cases,
zero failures/errors. No release or APK fleet publication has occurred.

Download validation update: the sequential clean Android build and all12tests
pass (six network-download cases plus six fullscreen lifecycle cases across
Android28/35). Tests cover exact authenticated bytes/progress, same-origin versus
cross-origin redirects, bounded redirects, unsupported URLs, auth failure,
cancellation, destination failure, TLS downgrade refusal and login HTML rejection.
XML evidence was preserved before packaging in
`/tmp/archy-190-companion-download-test-results/`. The canonical clean0.5.34/build54
APK package passes v1/v2/v3 verification and the existing signing-certificate pin;
the APK contains the new download handler. Fleet publication remains held pending
physical save/open acceptance and the existing release gates.

2026-10-05 operator acceptance: companion0.5.34/build54 phone download check
(save/open/cancel) accepted: “works, we can proceed”. Viewer and physical upload
acceptance retained. Close this manual gate; other release/security/Angor gates
remain open. No fleet OTA, ISO or public demo publication inferred.

### Operator accepts Angor discovery limitation — 2026-10-05

The operator explicitly accepted releasing with incomplete historical project
discovery documented as a known limitation ("that's fine for now"). Funding
commitments for all35 reference projects were verified;34 original signed
announcements remain unrecovered from the sources checked. This releases the
all-project-discovery publication hold, not a claim that recovery passed. Track
recovery separately and retain the limitation in release notes. Other artifact,
upgrade, security and publication checks remain required.

## 2026-10-08: final IndeeHub update restored at health-check boundary

Native operation `851483a2` automatically restored during target startup, before
the final frontend was started. This is **not successful final delivery**.
The MinIO target started at 18:24:31.489 UTC. Its immediate probe was still
`starting`; MinIO reported API readiness at 18:24:32, and the next scheduled
health check reported `healthy` at 18:25:02.851. The updater's previous 30 samples,
one second apart with no final sample after sleeping, could expire before that
healthy result. The actual health configuration uses a 30-second interval,
five-second timeout and five retries. PostgreSQL, Redis and MinIO were the only
target members started; relay, API, worker and final frontend were not started.

Independent post-restoration verification passed: all 31 running containers,
24 unrelated runtime identities preserved, the exact current 110-migration
history and original database commitments preserved, and restoration-image
proof matched. Retain the operation journal and backup history; do not treat a
restored transaction as a successful update or erase the first failed attempt.

The shared native provider (`b77…`) is deployed. The final frontend image
(`8db…`) remains pending; paired cached-account-A to chosen-identity-B acceptance
has not run. Prior clean native login success does not prove this reported
cached-account transition is fixed on the deployed frontend.

Source correction `7fe63355` replaces the sample-count deadline with a bounded
monotonic readiness deadline derived once from the first inspected health
configuration. It includes the configured start period, intervals, timeouts and
retries, with a 30-second minimum and five-minute ceiling; every inspect is
bounded too. `starting` never qualifies as healthy, configured-but-missing health
status stays pending, and unhealthy or exited containers fail immediately.
Running-only readiness remains allowed only for containers without a configured
health check. The isolated runner checkout bind is separately committed as
`1b0b119a` and preserves all existing isolation properties.

Validation at this checkpoint: source formatting and independent source review
passed; seven deterministic paused-time regressions are compiling through the
isolated runner. The full isolated suite, production backend build, another
explicit native update, post-update runtime/data proofs, and paired browser
acceptance remain pending. No wallet, payment, personal-media or profile changes
are part of this readiness correction.
