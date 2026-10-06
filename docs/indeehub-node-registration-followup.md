# IndeeHub node media registration

Status: node preparation primitive compiled and qualified locally; RPC/serving
integration, deployment and actual-node acceptance remain open. App-side v1 intent/receipt implementation is qualified in the
separate IndeeHub checkout at `c4b920f` (190 backend tests, production build).
This document records the integration contract, not a completed publishing flow.

## Implemented boundary

`core/archipelago/src/media_registration.rs` accepts an existing `NodeIdentity`,
an installer-owned node DID/app-instance audience pin, an authenticated v1 app
intent, the operator-approved relative Cloud selection and receiving methods,
trusted current UTC seconds, a byte limit, cancellation flag and progress callback.
It does not create/recover keys, select files for the operator, expose an RPC,
change existing content policies, publish a catalog entry or grant playback.

Call `prepare` on a blocking worker: it performs filesystem I/O and waits for a
cross-process per-request filesystem lock. Lock acquisition polls cancellation
and stops at the earlier of 30 seconds or an unexpired intent's deadline. An
exact completed retry after expiry may still wait at most 30 seconds; caller
cancellation can end that wait earlier. Root has declared the module in `main.rs` for the queued combined isolated
backend qualification. RPC/UI and serving integration remain unwired. The public Rust
selection structure is a caller assertion, not an authorization token.

The implementation validates canonical v4 request IDs, lowercase nonce/producer
keys, ASCII project/audience IDs, identity and installed audience equality,
JavaScript-safe numbers, ten-minute maximum intent lifetime, bounded viewing
windows and sorted distinct supported methods. It checks the existing node key's
DID against the installation pin, never a browser-supplied replacement key.

Linux `openat2` with `RESOLVE_BENEATH|RESOLVE_NO_SYMLINKS` resolves the approved
relative path from a held Cloud-root descriptor. An `O_PATH` descriptor is checked
for regular-file type before reopening that same held file for reading; devices
and FIFOs are not opened for I/O. Unsupported kernels fail explicitly. Source
inode/device, size and modification/change times must remain stable through the
streaming copy. The original Cloud file is never altered by preparation.

Each request gets its own private 0700 directory under
`<node-data>/media-registration/<request-id>/`. Files are not placed under a
web-served Cloud or content directory. The durable records are:

- `operation.json`: original intent, exact selection/methods/root and original
  file identity/metadata, plus original receipt issue time.
- `snapshot.json`: SHA256 and uint64 byte size committed before snapshot publication.
- `media`: private 0400 snapshot retaining the approved original bytes.
- `receipt.json`: exact fixed-domain Ed25519 receipt after all preceding commits.

Records and snapshots are fsynced; no-replace hard-link publication and directory
fsync establish durable names. Intermediate files are named `pending-<uuid>` in
the private operation directory. Interrupted staging is retained for controlled
recovery/cleanup, never served. Future cleanup must distinguish these private
partials from a completed snapshot or outstanding receipt and must not remove
original Cloud data or published/rented bytes.

An identical completed retry verifies the saved signature, snapshot byte commitment
and all original terms, then returns the same receipt even after its initial
expiry. A pending operation cannot issue its first receipt after expiry. Changed
terms, selection, audience, node identity or receiving methods reject reuse.
Pending snapshot recovery requires either the committed immutable snapshot or
unchanged source metadata/bytes. Damaged records or missing completed bytes fail
without recreating the operation or rewriting its commitment.

The wire schema and fixed ordered JSON array match
`/home/archipelago/Projects/indeehub-followup/docs/archipelago-registration-wire-v1.md`.
`sizeBytes` is a decimal string; the receipt contains exactly the 16 v1 fields.
The signature uses the already-loaded node's existing Ed25519 identity.

## Caller work required before enabling this feature

1. Obtain the intent through the authenticated installed IndeeHub bridge and its
   owner-authenticated API. A browser merely presenting matching-looking JSON does
   not prove that the app persisted an intent or owns the project. Bind app origin,
   installation identity, native dashboard session and CSRF protections.
2. Show project/producer identity, the actual Cloud selection, price, viewing time
   and supported receiving methods for explicit operator consent. Pass only the
   configured Cloud root and the exact selected relative path. Neither filesystem
   root nor installation pins may come from arbitrary request fields.
3. Supply a configured per-file quota, cancellation/progress and actual UTC time;
   preserve backpressure and track cumulative private storage use. File size is
   streamed and checked rather than read into memory. Private staging retention
   needs a separate reviewed quota/cleanup policy.
4. Persist the opaque `contentId` → immutable snapshot mapping and complete content
   policy under the same original request ID. Authenticate all content access and
   keep the pending item private throughout setup. Do not use the mutable Cloud
   filename as the serving identity or temporarily make it free/public.
5. Only after durable serving and entitlement linkage succeeds may the caller
   return the prepared receipt to the app for intent consumption. If this step
   fails, retry the same prepared request/receipt; do not create another content
   ID, silently change rental terms or return a success notification.
6. Complete purchase recovery and seller receipt settlement, timed FIPS playback,
   full immutable-byte/expiry enforcement and real node UAT before enabling the
   app's registration/publication flags. A registration receipt is neither payment
   proof nor a playback entitlement. Nothing in this primitive advertises serving
   availability or receives funds.

The returned `snapshot` descriptor is positioned at zero and read-only. The
`snapshot_path` is private node storage for the caller's persistent mapping;
it must never be sent to the app or used as a browser-controlled path. Any future
serving reopen must preserve descriptor containment/integrity and authorization.

## Local qualification

Eleven isolated tests cover original-key signing and exact wire preimage,
private durable snapshot/retry after source removal and expiry, changed bindings,
traversal/symlink/directory/FIFO rejection, cancellation/retry, source modification
during copying, concurrent callers, recovery at the snapshot/receipt boundary,
corrupt snapshot/missing commitment, invalid terms/expiry/quota and damaged-record
preservation. Identity fixtures use the existing node identity test helper only.

`media_registration/fixtures/v1.json` is a deterministic golden wire artifact
created with Node.js crypto from the public fixed test seed `07` repeated 32 times.
It contains independent canonical array bytes, expected public key/DID, signature,
receipt and media. A byte-identical copy lives in the IndeeHub app's test fixtures;
Rust preparation and signature verification and the app receipt parser both check
it. Fixture SHA256:
`8fbfcbc3beb0b4758fadf677c39c688d55a89ed200d8a7cd8741de0da569feb3`.
No live node identity or user key is included. A separate test checks expired lock
deadline and pre-cancelled acquisition without waiting.

The combined `scripts/test-backend-isolated.sh` run passed 1,808 tests with
zero failures and five existing skips, including all eleven media-registration
tests and the independent Node.js golden wire/signature fixture. Compilation
took 9m07s and execution 15.01s. Source hashes matched the frozen build inputs.
Log: `/tmp/archy-resumed-combined-backend-tests.log`; source provenance:
`/tmp/archy-resumed-combined-source-provenance.json`.

Later integration tests must also exercise caller origin/CSRF/consent, bridge interruption,
serving linkage failure/retry, receiving-method availability and real paid FIPS
playback. No live file, wallet, public relay, service or app was changed here.

### Existing app parser golden check

The lightweight Node check passed using the existing production-compiled IndeeHub
receipt parser, with no rebuild, Jest worker, database or network. It checks fixed
node DID, exact UTF-8 signature preimage, acceptance of the golden pinned-key
signature and rejection after changing `sizeBytes`.

Log: `/tmp/indeehub-registration-golden-dist-check.log`.

- Fixture SHA256:
  `8fbfcbc3beb0b4758fadf677c39c688d55a89ed200d8a7cd8741de0da569feb3`.
- Existing `backend/dist/archipelago/media-registration.protocol.js` SHA256:
  `8e4a9f17649381c0b6bd8b9e187bd266b94743f40c63f5bc384ade204114b98b`,
  built at 2026-10-06T22:23:09.597Z by the successful resumed backend build.
- Its unchanged source protocol SHA256:
  `ec5b614516dcbd9918ff8fdecfc980f4430ff20362aced4ab348197e284337d7`,
  source modification time 2026-10-06T22:07:50.141Z.

This verifies the actual compiled app parser against the shared artifact. The
subsequent combined isolated Rust run also passed its independent golden fixture
test, establishing agreement across both implementations. Node caller/serving
integration and live acceptance remain open. No fixture or Rust source changed
between the frozen compilation inputs and the successful run.
