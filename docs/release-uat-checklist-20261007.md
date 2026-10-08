# Release UAT checklist — 2026-10-07

This is a read-only acceptance checklist for the post-1.9.0 candidate. It
does not authorize deployment, payment, catalog activation, wallet mutation or
service restart. Run backend tests only through the isolated runner.

## Before UAT

- Record the candidate commit, backend binary digest, dashboard index digest,
  catalog digest and installed app image digests.
- Verify Framework, Yaya and development-node identity before collecting any
  live evidence. Do not substitute the development box for Framework.
- Capture service/container state, boot ID, restart counters and app desired
  state. Do not print environments, credentials, macaroons, seeds, invoices or
  wallet databases.
- Confirm that private rollback copies and IndeeHub data backups exist and are
  readable without changing them.

## IndeeHub publishing and paid viewing

Source checks already recorded: backend 151 tests, frontend 94 tests,
production builds, and mobile/desktop browser checks. The following live flow
is still required and must use disposable test identities and a regtest or
explicitly approved test mint:

1. Publish one media item in Backstage and verify its immutable media hash,
   owner, expiry and node registration.
2. Discover the item from a second authorized node.
3. Attempt the payment once. Record the operation ID and verify that timeout,
   navigation, refresh and reconnect return the same operation rather than
   creating another payment.
4. Verify seller settlement, buyer entitlement, byte delivery and timed FIPS
   playback. Stop and resume playback without another charge.
5. Verify cached ownership and free repeat playback; test an interrupted
   download and a partial transfer.
6. Verify expiry, rejection, insufficient funds and unavailable peer states
   produce actionable errors and do not leave a stuck purchase.

Required evidence: buyer/seller operation records, one payment attempt,
matching media hash, entitlement receipt, playback authorization and a
rollback result. Current blocker: the complete distributed publish → discover
→ pay → timed playback flow has not been accepted on live nodes.

## V4V Yaya demo

The browser qualification already covers native Nostr login, Browse launch,
artwork, previous/next, shuffle and close-to-bottom-player behavior at mobile
and desktop widths. Remaining acceptance is physical companion behavior:

- Launch from the Yaya App Store and confirm `/browse` is the first route.
- Authenticate through the native Nostr provider; confirm no alpha password
  gate or private development fixture is used.
- Background playback and notification presence on APK0.5.37 are operator
  accepted. Verify any remaining individual controls (previous, next, play/pause
  and shuffle) without treating the general playback report as their acceptance.
- Notification artwork still failed on the phone after the later dashboard
  delivery. The operator explicitly deferred investigation to task23, after all
  other tasks. Do not make it an immediate playback retest or deployment gate.
- Reopen the app and verify the same queue, position and authorization remain.
- Stop playback, log out and expire the session; confirm playback and controls
  stop without a duplicate stream or payment.
- Confirm the Yaya-only catalog remains private and anonymous catalog access
  remains rejected.

Remaining: queue/authorization/session-expiry checks and any individual controls
without explicit acceptance; background playback itself is accepted. Artwork is
deferred to the end of the backlog. The V4V image must remain private until its
registry visibility and catalog policy are independently rechecked.

## Payment and wallet safety

Run locally or against regtest only:

```sh
scripts/test-backend-isolated.sh
bash scripts/test-fee-bump-regtest.sh
```

Verify these cases:

- Lightning success, timeout, rejection and reconnect.
- Lightning failure followed by an ecash fallback: no dead-end state, no
  second payment and no blocked retry.
- Ecash preparation, ambiguous mint response, recovery and idempotent retry.
- On-chain quote expiry, insufficient change, stale quote, duplicate submit,
  CPFP/RBF distinction and confirmation reconciliation.
- Cooperative-close fee choices and explicit slower/custom choices.
- Wallet balance and transaction history never convert an unavailable response
  into zero.

Current blockers: integrated payment qualification and live regtest fault-path
evidence must be recorded together; no real payment should be used as a test.

## Federated Nodes, firewall and tunnel

- Start sync and verify a visible spinner, node count and current phase.
- Verify bounded progress when one peer is slow or offline; the remaining
  peers must complete and the UI must show partial failure truthfully.
- Retry after reconnect; stale or duplicate responses must not resurrect old
  state or issue duplicate requests.
- Verify Connected Nodes shows peer and trusted-node counts without double
  counting overlapping identities.
- Verify the Network map button is bottom anchored and usable at 320px, 360px,
  390px and desktop widths.
- Verify peer removal and federation removal are clearly distinct. For a node
  with both relationships, remove only the selected relationship.
- Verify removal has a bottom-aligned action, spinner, duplicate-submit guard,
  completion/error toast and accurate post-removal state.
- Verify an available authenticated FIPS path is preferred, with bounded
  fallback and preserved identity checks.
- Open the firewall/tunnel settings screen. Confirm read-only diagnostics,
  refresh progress, nullable addresses and partial failures render safely.
- Verify persistence and rollback using a disposable configuration; do not
  change live firewall rules during this checklist.

Current blockers: wider reciprocal-sync/reconnect qualification and live
firewall/tunnel persistence/reboot acceptance remain open.

## Release evidence commands

```sh
git status --short
git log -1 --oneline
git diff --check HEAD^ HEAD
python3 scripts/check-git-mirrors.py --local
```

Before any OTA, ISO, catalog or app activation, also require reviewed matching
main/tag refs on both ngit and Gitea, a recorded rollback result, and separate
source-test versus actual-node acceptance records. A passing local suite alone
does not close a release gate.

## Known release blockers

1. IndeeHub distributed paid playback and supervised cutover rollback.
2. Companion native background audio/media; operator accepted delivered Cloud
   PiP on 2026-10-07. Detailed interruption/expiry cases remain distinct.
3. Framework Monitoring owner-browser acceptance. The earlier LND startup,
   Receive and false-zero incident is closed with operator acceptance; it is
   not a release blocker. Paid-file settlement/recovery is a separate open gate.
4. Fleet-wide offline/reconnect and FIPS acceptance.
5. Firewall/tunnel persistence and reboot qualification.
6. HTTPS hostname/trust and companion acceptance.
7. MeshCore two-radio qualification.
8. Full mirror parity and signed OTA/ISO/catalog acceptance.
