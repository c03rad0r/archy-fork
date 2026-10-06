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
