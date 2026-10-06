# Paid content: recovery before response headers

Status: OPEN, identified during IndeeHub rental integration on 2026-10-06.
This is a source-confirmed gap. No new real-money failure was induced.

## Confirmed boundaries

`api/rpc/content.rs::handle_content_download_peer_paid` checks the existing
purchase index and FIPS route before calling a wallet. It persists ownership
through `content_owned::record_purchase_stream` only after successful response
headers. Thus the previously qualified interrupted cached-body/seek cases do
not establish recovery during wallet preparation or after seller settlement but
before headers reach the buyer.

Cashu `send_token_at` may swap inputs remotely before saving the local wallet
and returning the prepared token. Its deterministic output derivation supports
wallet restoration, but is not a correlated purchase operation journal.
Concurrent mutations also need an operation-wide wallet reservation/commit
boundary, beyond the existing atomic file writer. Fedimint operation IDs and
out-of-band note recovery must be handled using that backend's actual semantics.

The seller currently redeems a presented token in `content_server::serve_content`
after preparing readable media. It does not persist a Cashu purchase receipt
that can authorize subsequent delivery without attempting redemption again.
A token hash alone is not evidence that redemption settled.

## Immediate correction being qualified

Choose the ecash backend before either wallet operation begins. An explicit
choice remains pinned; automatic selection reads the home-mint spendable Cashu
balance and selects Fedimint only when that balance is insufficient. Never fall
through to another wallet after an attempted operation returns an error: the
remote mint may already have consumed inputs. Reject unknown method names.
This prevents a second backend operation in the same RPC; it does **not** solve
crash recovery or make a fresh user retry safe.

## Required implementation and acceptance

1. Persist an unpredictable purchase ID and immutable seller/buyer/content/hash,
   terms, amount, method and protocol capability before any mint/spend. A damaged
   or ambiguous journal must block a second payment, preserving original data.
2. Journal wallet input reservations and recoverable output derivation/operation
   IDs before a remote mutation. Commit wallet change, prepared token and operation
   result durably. Serialize all competing wallet mutations, including receives,
   melts and restores, without deadlocking nested operations.
3. Persist the seller's receipt/settlement transition. Recover ambiguous receipt
   writes through correlated wallet operation results, not balance changes or
   acceptance of the client's claimed outcome. Repeated delivery requests for
   the same settled purchase must not redeem/pay again.
4. Bind delivery authorization to authenticated buyer and immutable content/terms,
   with an unpredictable capability. Never turn a public on-chain address into
   a bearer authorization credential. Negotiate updated peer capability; do not
   assume old nodes implement this receipt protocol.
5. Retain prepared tokens privately until delivery or confirmed refund settles.
   Record actual refunded amounts/fees. A failed refund is not proof of payment
   failure and must not clear an ambiguous purchase for another charge.
6. Exercise interruption at every write/send/receipt boundary, duplicate requests,
   process reconstruction, full restart, corrupt journal, disk-full, changed
   offers, wrong buyer, incompatible peer, and mint/federation rejection. Prove
   wallet conservation and one settlement with disposable deterministic fixtures
   before bounded live payments. Keep rented cache authorization separate from
   permanent paid-file ownership.

The existing two-node 1-sat purchases and cached-delivery tests remain valid for
their documented scope. They must not be relabeled as acceptance of these open
initial-payment/recovery requirements. No additional real payment is needed to
prove the immediate backend-selection regression.

## Backend-selection qualification

The immediate correction passes **12 focused content RPC tests**, including
injected ambiguous Cashu/Fedimint failures proving the unselected wallet future
is never polled, explicit-choice preservation and unknown-method rejection.
The full isolated suite passes **1,749 tests, zero failures, five existing
ignored tests**. Logs: `/tmp/archy-peer-payment-selection-tests.log` and
`/tmp/archy-peer-payment-selection-full-tests.log`. Only the required isolated
runner was used. No live wallet data or real payments were involved. This is
source/test qualification; production build and deployment are separate.

## Recovery-metadata prerequisite qualified (2026-10-06)

Seed reads now distinguish genuine absence from I/O failure. Loading a damaged
recovery source returns an error instead of silently enabling random outputs.
A configured recovery source must reserve/derive outputs successfully before a
mint request; derivation/counter failures no longer fall back to random secrets.
Legacy wallets which genuinely have no seed retain their existing output path.

Counter reservations reject empty/corrupt/unreadable existing files rather than
resetting to zero. Updates use an owner-only sibling temporary file, file flush,
atomic replacement and directory flush before returning a usable reservation.
Invalid derivation is rejected before reserving counters. Existing counter
serialization within the management process is preserved; this is not a claim
that a new cross-process wallet lock or the purchase journal has been implemented.

Six new regressions cover damaged seed/counter reads, preserved damaged files,
concurrent reservations/reload/private permissions/temp cleanup, invalid keysets,
no silent random-output fallback and explicit legacy behavior. Wallet-focused
isolated result:150passed,0failed,1existing ignored. Full isolated result:
**1,755passed,0failed,5existing ignored**. Evidence:
`/tmp/archy-wallet-recovery-prerequisites-tests.log` and
`/tmp/archy-wallet-recovery-prerequisites-full-tests.log`.

Read-only checks found well-shaped seed/counter JSON on dev and Yaya; no secret
values were printed and no wallet files were changed by those checks. Production
build/deployment of this prerequisite remains pending. Durable initial purchase
intent, mint-operation recovery, seller receipt and refund recovery remain open.

Strict-schema follow-up: an existing counter document must actually contain its
counter map. Empty JSON objects and null maps are rejected without modification,
not deserialized as a fresh zero state. The full isolated suite again passes
1,755tests,0failures,5existing ignored in
`/tmp/archy-wallet-recovery-strict-schema-full-tests.log`. No live deployment or
claim of complete initial-payment recovery is implied.

## Additional wallet boundaries found during review (6 October)

Source review of `ecash::melt_tokens`, `swap_between_mints` and
`MintClient::melt_tokens` found further work required before full recovery can
be accepted. The melt path does not currently submit or retain fee-change
outputs, and the caller does not require a PAID response before proceeding.
Cross-mint recovery records are written after the remote operation, leaving an
interruption window. These are source findings, not newly induced live losses.

The implementation must account for NUT-05 quote states and NUT-08 change using
the mint's advertised support, preserve uncertain operations, and verify amount
conservation. Reference specifications:
https://github.com/cashubtc/nuts/blob/main/05.md and
https://github.com/cashubtc/nuts/blob/main/08.md .

Wallet-wide serialization must also include network changes and streaming
revenue writes; a load/save outside the operation lock can overwrite another
mutation. Current empty-wallet-file handling, file permissions and directory
fsync require review as part of durable storage. Counter fail-closed tests do
not establish that this larger transaction journal has been implemented.

### Wallet storage durability follow-up

Existing empty/whitespace wallet files now fail closed instead of reporting zero;
only a missing file creates fresh state. Atomic saves use unique0600 temporary
files, file and directory fsync, and cleanup on failure. Concurrency tests prove
whole-file replacement (not read-modify-write serialization), private permissions
and retained targets on rename failure. Wallet tests:57passed. Full backend
qualification passes in `/tmp/archy-wallet-storage-tls-full-tests.log`. No live
wallet was altered. Operation serialization and recovery journal remain open.

### Serialized wallet mutations and seed durability

The wallet now serializes public mutations by canonical node data directory,
including network changes and seed establishment/import. Independent node
fixtures retain separate locks; nested send/swap paths use private implementations
under the outer lock. Streaming revenue records use the same boundary, and direct
wallet saves are restricted to the wallet module. This is in-process serialization,
not a cross-process transaction journal or a claim that an entire payment RPC is
recoverable.

Damaged network configuration now errors instead of silently choosing mainnet;
only an absent configuration retains the historical mainnet default. The previous
seed is durably copied to a unique private backup before atomic replacement,
rather than moved away before the replacement write succeeds.

Final isolated suite: **1,764 tests pass, zero failures, five existing skips**.
Log: /tmp/archy-wallet-mutation-seed-final-tests.log. Regressions include eight
simultaneous real HTTP/curve-signed fixture receipts plus sixteen history writes,
preserving255sats and24entries; canonical/symlink lock identity; damaged network
configuration; and simultaneous seed establishment/unique retained backups.
No live wallet data or additional real payments were used.

Still required: correlated purchase and mint-operation journal, recoverable
prepared outputs, quote/change handling and seller receipts, interrupted-operation
recovery and complete timed-playback integration. Higher-level Minibits claim and
purchase flows must pin their network/terms across their entire business operation;
serializing individual wallet calls alone does not provide that contract.

### Recoverable prepared Cashu swaps

MintClient now separates preparation from execution. Prepared requests contain
the exact outputs and private unblinding material, can survive serialization, and
validate their mint, commitments, values and keyset before execution. Debug output
omits bearer secrets. Recovery uses the original outputs, matches returned curve
points independently of response ordering/hex case, and rejects partial,
duplicate, unknown or mismatched signatures. An empty restore response remains
uncertain; it is never interpreted as permission to spend again.

Real HTTP/curve fixtures simulate a mint consuming inputs and returning500instead
of its successful response. A reconstructed client recovers the original valid
proofs without a second swap. Negative tests cover changed preparation and
malformed recovery. The first run failed an overly strict test comparing JSON
bytes with unordered map keys; the corrected test compares semantic contents.
Original log: /tmp/archy-prepared-swap-tests.log. Final complete isolated backend
suite: **1,767 pass, zero failures, five existing skips**, in
/tmp/archy-prepared-swap-final-full-tests.log.

This introduces the request/recovery primitive. Existing swap callers still
execute immediately; durable wallet reservations and the correlated purchase
journal are not wired yet. No live money, wallet state or app deployment changed.

### Operation journal qualification in progress

A separate private write-ahead store now records immutable operation ID, network,
mint, amount and purchase-context hash alongside the exact request material.
Records advance from prepared to saved result to committed; they cannot skip the
saved-result boundary. Retries retain the original request, changed terms are
rejected, and damaged/unsupported/oversized records block a fresh operation.
Files use0600, the journal directory0700, atomic replacement and file/directory
flushes. A wallet mutation guard scopes writes to the canonical node directory.
The checksum detects accidental corruption; it is not authorization against a
process able to edit the node's private state.

The initial full isolated run passed1,773tests with zero failures and five existing
skips (`/tmp/archy-send-journal-full-tests.log`). Review subsequently added explicit
sat-unit validation and a negative regression. The final full isolated run also
passed1,773tests, zero failures and five existing skips
(`/tmp/archy-send-journal-final-tests.log`).
This storage module is not yet connected to wallet reservation/commit or paid-file
purchase/receipt handling and has not been deployed. Do not infer complete
payment recovery from the storage tests.

The next integration must reserve selected inputs with an operation owner before
any remote request, recover the exact prepared outputs, and commit change/history
once. A restored result must match all outputs; absence is not proof of failure.
An input-state response must account for every requested proof without foreign or
duplicate entries. Pending/spent/unknown states must never authorize a new payment.
See the current [NUT-07](https://github.com/cashubtc/nuts/blob/main/07.md) and
[NUT-09](https://github.com/cashubtc/nuts/blob/main/09.md) specifications. Wallet
integration still needs crash-boundary fixtures, followed by purchase-context and
seller-receipt integration before any new paid-content acceptance claim.

### Additional restore checks retained for the next batch

Review of seed restore found that the current NUT-07 consumer verifies response
length but not each returned proof identifier. Match every response to the exact
requested curve point and reject unknown state values before crediting proofs.
Do not treat a same-length response as sufficient. Also prevent a seed scan from
making reserved outgoing operation outputs available before that operation's
result/commit is recovered. These are source findings; no live restore was run.

### Wallet reservation/commit boundaries qualified

The journal can now durably reserve its exact inputs with an operation owner,
refuse another operation's reservation, and commit its saved token/change/history
once. Recovery after purse-save but before journal-phase-save uses the same stable
transaction ID. Changing the selected network cannot redirect that commit into
the other purse. Outgoing swap proofs are retained as locally spent, alongside
spendable change, so they are not immediately rediscovered as wallet funds.

Four additional regressions cover local restart boundaries, competing operations,
network switching and a real HTTP/curve-signature lost-response scenario. The
latter reconstructs the journal, restores the original swap outputs, commits twice,
and verifies one mint swap, one history entry,4sats sent and4sats change from8.
Full isolated qualification:1,777passed, zero failures, five existing skips,
`/tmp/archy-journal-wallet-commit-tests.log`. No real sats or live wallet data used.

These methods are not yet wired into the purchase RPC or a remote-operation
executor. Seller receipts, delivery capabilities, ambiguous refunds, melt/change
and seed-restore interaction remain open. A passing commit primitive is not full
paid-content recovery acceptance.

### Seed-restore interaction qualified

NUT-07 responses now require one correctly identified state per requested proof
in protocol order, with only defined states accepted. Duplicate requests, foreign
or duplicated response identifiers, omissions, reordered entries and unknown
states fail closed. Hexadecimal case differences remain accepted.

Seed restore inspects the private send journal before contacting the mint: it
blocks unresolved operations for the selected network/mint and excludes committed
outgoing token secrets even after legacy purse pruning/removal. Damaged journal
records block restoration rather than disappearing from the decision. Other
network/mint operations do not block an unrelated valid restore. Prepared swap
requests also must cover the bound payment amount before being recorded.

Full isolated qualification:1,779passed, zero failures, five existing skips,
`/tmp/archy-journal-restore-boundary-tests.log`. These source changes remain
undeployed. The higher-level purchase/receipt executor remains open.

### Recoverable send executor qualified locally

The caller-owned operation now reserves inputs before remote spending, restores
the original saved swap after an ambiguous response, and commits its original
token/change/history once. Changed payment terms reject reuse. A spent input with
no recoverable output blocks another swap. Exact sends support compact v2 wire
keyset IDs while retaining the full wallet ID. Seed restoration canonicalizes
curve points, rejects foreign or duplicate metadata, and requires durable counter
advancement before crediting recovered funds.

The first test run exposed compact-ID comparison and uppercase restore-point
matching defects; both were fixed. The final isolated run passed1,783tests with
zero failures and five existing skips:
`/tmp/archy-recoverable-send-executor-final-tests.log`.
No real payment or deployment was performed. Purchase RPC integration, durable
seller settlement/receipt recovery and the end-to-end acceptance remain open.

Recovery preflight now rejects malformed or unsupported restore responses before
reserving or spending inputs, and refuses already-issued newly derived outputs.
Required restore fields cannot silently default to empty. The malformed-response
regression verifies unchanged spendable balance/no swap, then successful retry
when the mint responds correctly. Full isolated1,784passed initially; combined
catalog-route qualification1,785passed, zero failures/five existing skips.

### Recoverable incoming settlement — implementation under qualification

A separate private receive journal now binds a caller-owned UUID to its original
network, canonical mint, token hash, agreed minimum net price and purchase-context
hash. Incoming proofs are claimed outside the spendable purse. Claims use their
mint and secrets, so another operation cannot redeem a differently encoded token
or an overlapping subset. The legacy receive entry point also checks these claims.

Preparation verifies recovery support and mint fees before any swap. The exact
prepared outputs are then saved before POST. An ambiguous response resumes the
same outputs with NUT-09; empty restoration requires every original input to be
strictly UNSPENT before the identical request can be retried. No automatic refund
is inferred from a missing response. Mixed-mint and non-sat tokens are outside this
primitive's deliberately narrow contract.

Commit order is Result → Committing → atomic purse save → Committed. Committing
stores a hash of the exact pre-save purse bytes (absence differs from an empty
file). The purse contains an independent `received:<UUID>` commitment marker,
separate from prunable history; it contains no bearer proofs. A retry either finds
that marker or requires the exact unchanged pre-save purse. A changed purse whose
marker is missing causes a manual-recovery hold. In particular, an older wallet
writer dropping markers is not treated as permission to re-credit the receipt.
Markers must not be compacted with ordinary history. A completed journal receipt
returns its original net amount even if the wallet/history was subsequently
pruned; it never reconstructs funds merely because its caller retries.

Pending settlement blocks seed restoration for the bound network/mint. Committed
incoming outputs remain ordinary wallet funds, subject to the existing mint-state
checks during seed restoration; they are not outgoing-token exclusions.

This is a source implementation under test, not a deployed seller receipt or
purchase protocol. No real payments were made. Purchase authorization, delivery
capabilities, transport correlation, refunds, and Fedimint/melt remain separate
open requirements. Test results will be recorded after the isolated runner exits.

### Resumed settlement qualification — 6 October

The interrupted session's receive journal, executor and four regression tests
were recovered intact from the existing worktree. The old focused log stopped
at compilation; it does not establish a test pass. No wallet data was restored,
replaced or modified to resume this source work.

Review also found that `verify_and_receive_payment`, used by the legacy content
server, needed the same incoming-proof claim check as ordinary `receive_token`.
It now refuses to redeem another settlement's claimed proofs, including when the
mint still considers those proofs unspent. An additional HTTP/curve regression
covers a rejected first POST, legacy redemption refusal, a malformed state reply
blocking a second POST, and then verified-unspent retry using the exact original
request. The new regression must pass before qualification is claimed.

Next integration boundary remains buyer purchase intent → recoverable sender →
correlated seller settlement → durable delivery receipt/capability. The current
`handle_content_download_peer_paid` still calls the legacy sender and records
ownership after headers; `content_server::verify_payment_token` still calls the
legacy payment verifier. These call sites are not yet the new purchase protocol.
Do not deploy these primitives as a claim that pre-header paid-file recovery is
complete. Keep the original payment/receipt context for retries and do not send
another payment to recover delivery.

Resumption compile checkpoint: `/tmp/archy-resumed-receive-tests.log` finished
compilation successfully in 10m20s, but the requested `wallet::payment_tests`
filter matched zero tests (1,794 filtered out). This is compilation evidence only,
not a focused test pass. The module is `wallet::ecash::payment_tests`. Final-source
qualification must include the later legacy-claim guard and fifth receive test;
its build is queued behind coordinated IndeeHub/browser/image checks to avoid
competing heavy jobs on the development node.

#### Next implementation batch: purchase/receipt contract

The next source batch should introduce `content_purchase` journals and protocol
fixtures before switching the live RPC entry points. Bind a versioned UUID,
verified buyer/seller DIDs, content SHA256/size, immutable terms hash, Cashu
network/mint, gross token amount and minimum seller net amount. Account for mint
fees explicitly; sending the displayed net price is not sufficient when the
seller pays an input fee. `ContentItem` currently has no content hash, so obtain
a stable readable content snapshot and authenticated offer before spending.

The existing `content_auth` v1 signature covers GET/path/range/time, not payment
headers or request bodies. Add a separately domain-separated signed-body proof
for purchase requests covering method/path/audience/body hash; do not treat plain
`X-Federation-DID` or appended unsigned purchase headers as authentication.
`PeerRequest::send_json` already provides the transport operation.

Persist the seller contract before calling recoverable receive with its UUID and
context hash; persist receipt/capability after settlement. Authenticated status
lookup by the same buyer must recover a lost receipt without another redemption.
Buyer intent precedes recoverable send and retains its original token privately;
retries resume that purchase and retrieve the receipt, rather than refunding or
creating another payment after an ambiguous response. Reject unsupported protocol
versions before spending. Cover changed content/terms, wrong buyer, duplicate
requests, expired offers, interrupted writes and lost settlement/receipt replies
with deterministic fixtures before any live purchase acceptance.

The next batch now exists as the initially unreferenced
`core/archipelago/src/content_purchase.rs`: strict versioned contracts/context
hashes, private original buyer tokens, seller settlement and durable random
receipts, buyer receipt/delivery states, and cross-process journal locking with
flushed atomic private records. Six temporary-directory tests cover restart,
exact replay, changed terms/results, expired new offers versus existing recovery,
foreign receipts, corrupt/nonregular records and invalid state transitions.
It performs no wallet or network operations. Root integration will declare the
module for combined qualification; passing tests are still pending. Callers must
authenticate offer/receipt provenance and discover/reuse an existing purchase
UUID before generating another one; this primitive does not yet supply that
business-level lookup or the transport/serving integration.


Review caught a cancellation ordering issue in the asynchronous rename commit
point. Purchase journals and the existing purse/send/receive writers now perform
rename plus directory flush synchronously while their guard is held, so cancelled
async work cannot later overwrite a newer writer. A deterministic purchase test
pauses after flushing the temporary file but before commit, cancels the writer,
then verifies that a subsequent token commit survives and the stale temporary
file is removed. This is a source correction awaiting the combined isolated run.

Next RPC/UI batch acceptance must also include node-side discovery of an existing
pending purchase by authenticated buyer/seller/content before minting a fresh
UUID. Caller-only UUID retention is insufficient after lost browser/client state.
The current journal's UUID binding does not yet implement this lookup. The UI
must show reserved/pending funds separately from spendable funds; a spendable
balance of zero must not be presented as zero total owned funds while an operation
still holds them. Neither requirement is satisfied by the wallet primitive tests.

Transport integration detail: the new v2 peer proof hashes the exact request body.
`PeerRequest::send_json` currently serializes independently inside its FIPS/Tor
helpers. Add a serialize-once POST-bytes transport before wiring purchase routes;
sign and send those exact bytes, rather than signing a separately serialized JSON
value then allowing another `.json()` call to choose the wire bytes. Existing
content dispatch currently routes GET reads only, so POST purchase routes remain
a separate integration step. Existing v1 GET authentication must remain compatible.

### Combined resumed qualification passed — 6 October

The full isolated runner passed **1,808 tests, zero failures, five existing
ignored tests**, with no filter. Compilation took 9m07s; isolated execution took
15.01s. This includes all five recoverable-receive regressions, six purchase
journal tests (including cancellation ordering), eleven media-registration tests,
and the independently coordinated v2 request-authentication coverage. Only
`scripts/test-backend-isolated.sh` executed backend tests.

Evidence: `/tmp/archy-resumed-combined-backend-tests.log`.
The exact tested coordinated tree was HEAD
`1c6daab92a4e7c9fa70b81489c355a35163369b5` plus `git diff HEAD --binary`, SHA256
`e9a8d75a81a966904d0929a1a1869adb892d266ef1b6b59580b7543a6b0ca7a4`, saved privately
at `/tmp/archy-resumed-combined-tested-source.patch`. New source SHA256 values:

- `content_purchase.rs`: `5d597f48c24ab96d4a7ee348ef641cc1f2d98c411574c46f14c579bea9930e84`
- `wallet/receive_journal.rs`: `43333ec21a6b1f7613078083d8114ef934bd44af69c9e93138e58a419cb919ac`
- `media_registration.rs`: `45450e99b50eff3d1115c2b9787e7235e9772209151a06fd09d62c47d0bd93a5`
- Its `fixtures/v1.json`: `8fbfcbc3beb0b4758fadf677c39c688d55a89ed200d8a7cd8741de0da569feb3`

All captured source hashes were rechecked unchanged at test completion before
this evidence update. Machine-readable provenance is in
`/tmp/archy-resumed-combined-source-provenance.json`. The test suite's isolated
subprocess case also prints a one-test result; it is not a second full run.

No real payment, wallet replacement, deployment or release occurred. The current
purchase module is a qualified local journal primitive, not a wired purchase RPC,
authenticated seller offer/receipt transport, or serving capability. Scratch
acceptance/deadline/executor work in `/tmp/archy-purchase-executor-next` is separate
and is NOT included in these test results. Explicit seller acceptance before
buyer spending and retained late-settlement eligibility are the next batch.
