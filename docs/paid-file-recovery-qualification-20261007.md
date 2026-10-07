# Paid-file recovery qualification — 2026-10-07

Status: **OPEN — safety fixes in source; final combined validation and actual-node initial-payment interruption acceptance remain required.**

## Findings repaired

The on-chain cross-rail admission helper had no callers. Both current Cashu
purchase and Lightning create/pay/retry/external exposure could bypass an
existing on-chain operation, despite sharing its admission lock. They now check
the durable on-chain journal while holding that lock, before any alternate
wallet operation or externally payable invoice can be created. Read-only
Lightning lookup remains available. Retired unallocated on-chain operations
retain the journal's existing release policy; unresolved and funded operations
must be recovered, not paid again.

The old `content.download-peer-paid` route still directly spent ecash before a
recoverable operation/receipt existed. Its `cache_only` flag controls response
format, not payment authorization. The old `content.request-invoice` and
`content.request-onchain` methods likewise bypassed the durable purchase flows.
Fresh legacy spending and invoice/address creation now return actionable errors.
Already-owned exact/alias cache reads, existing invoice/on-chain status and
original-payment download endpoints remain available. No automatic conversion
to another method, payment retry, or bypass of reviewed fee consent was added.

Compatibility impact: current PeerFiles used the legacy spender for Fedimint;
Cashu already uses `content.purchase`. New Fedimint file purchases are therefore
temporarily unavailable, explicitly shown in the payment UI and guarded against
stale callbacks. Existing purchased files remain accessible. Restore Fedimint
purchases only with durable dispatch, ambiguous-outcome recovery and receipt
handling. Hidden `Web5SharedContent.vue` also references the legacy spender;
its import is currently commented out in `Web5.vue`. No current UI callers of
the two legacy invoice/address creation methods were found.

## Verification

- Focused PeerFiles payment suite: **62 passed**, zero failed.
  `/tmp/archy-paid-file-ui-20261007.log`. TypeScript `vue-tsc --noEmit` also
  passed; `/tmp/archy-paid-file-typecheck-20261007.log`.
- New backend regression exercises actual Cashu and Lightning RPC entry points
  with a persisted on-chain attempt, including consent, retry and external
  invoice exposure; checks unchanged original journal, absent replacement
  records and no wallet creation. Separate real-handler regression checks
  rejection of all legacy ecash choices and invoice/address creation, then
  exact cached Fedimint bytes and zero-payment repeat access.
- Backend tests must run through `scripts/test-backend-isolated.sh`. Two
  compilation attempts were deliberately interrupted before test execution:
  the first after IndeeHub's final outer guards arrived during source capture,
  the second after prolonged host page-I/O starvation when the final IndeeHub
  lifecycle/health correction was ready. Neither is a test failure or a pass.
  Logs, input hashes and explicit incomplete status are retained. One combined
  run after the final source freeze remains required; the new backend tests are
  not yet claimed passed.
- Current live read-only recheck: dev and Yaya each retain the accepted 20 MiB
  fixture with exact original SHA256 and ownership despite the seller share
  having been removed. Only ownership lookup and cached HTTP GET were used;
  the cumulative payment ledger remained byte-identical. Initial harness
  rejected JSON-RPC `error:null`; corrected rerun passed both nodes. Both logs
  are retained; this is not initial-payment response-loss acceptance.
  `/tmp/archy-paid-cache-readonly-20261007-rerun.log`.
- No actual funds, wallet state, live services, files or peer policies were
  changed by this qualification. No deployment or publication has occurred.

## Reconciled prior evidence — do not repeat payments

Older summaries retain stale open subitems. Existing receipts establish:

- Framework's October 2 one-sat Lightning purchase: seller settlement,
  exact 121-byte cache, durable ownership and operator-confirmed free reopen.
  Framework/Shorty were on the documented older binaries; this is not current
  seller-journal acceptance.
- `/tmp/archy-190-framework-paid-files-readonly-final.log` records one durable
  ownership entry and exact accepted bytes in Files/Documents. Optional Files
  copy was subsequently verified, despite an earlier SSH failure in the ledger.
- October 6 dev↔Yaya purchases: two distinct 1-sat Cashu fixtures, seller credit,
  exact 20 MiB bytes, range reads and free cached reopen after temporary shares
  were removed. The existing private cumulative spend ledger must not be reset.
- `/tmp/archy-paid-cache-restart-qualification.log` records twenty cached
  interruptions across both nodes, management restart on each, preserved app
  containers/cache/ownership and zero additional sats.
- Existing isolated tests cover lost offer/acceptance/settlement replies,
  persistent native invoice dispatch recovery, damaged journals, corrupt/truncated
  delivery and one-wallet debit. These are fixtures, not actual-node fault
  injection during initial payment.

Still required: verify current corrected artifact, initial payment/settlement
response-loss recovery before successful delivery headers, and current seller
persistence across restart. Never send a new payment to recover the historical
sales. The original missing-file incident was individually accepted by the
operator; broader release acceptance remains separate. Timed IndeeHub rental
and producer payout acceptance belongs to the independent IndeeHub workstream.
