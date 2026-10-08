# Paid-file recovery qualification — 2026-10-07

Status: **OPEN — safety fixes and combined isolated validation pass; corrected-artifact deployment and actual-node initial-payment interruption acceptance remain required.**

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
original-payment download endpoints remain available.
This does not reconstruct missing historical Cashu/Fedimint delivery receipts:
a legacy spend without a saved file binding/receipt still needs investigation,
not another purchase. Generic wallet history alone cannot prove which file a
lost legacy request bought. No automatic conversion
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
  Logs, input hashes and explicit incomplete status are retained.
- Final combined isolated backend suite: **2,006 passed, zero failed, five
  ignored**. Both new actual-RPC regressions pass. All **532 captured backend,
  helper and catalog inputs** remained unchanged. This includes payment fix
  `18b08720` and complete IndeeHub lifecycle/health fix `b0b95810`. Compilation
  took 36m19s under host disk contention; isolated execution took 23.74s with
  325.1 MiB peak memory and no swap. No source or wallet pass is inferred from
  the earlier interrupted attempts.
  `/tmp/archy-paid-indee-lifecycle-backend-20261007.log` and
  `/tmp/archy-paid-indee-lifecycle-provenance-20261007.log`.
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

## Durable evidence

Logs, the final input manifest and earlier interrupted-attempt evidence are
retained under
`~/.local/state/archipelago/release-qualification/paid-file-recovery-20261007/`.
The combined run qualifies source/isolated behavior. It does not claim a new
production payment, initial-payment outage/restart acceptance or deployment of
this backend to either node.

## Later source checkpoints (before the final combined pass)

The 2,006-test stable-input pass above predates Indee scope-launch correction
`01a55d2d` and rental admission fix `4dde14bf`; it remains valid historical
qualification, not a full-suite pass for current source. The corrected Indee
fixture executable SHA256
`3cbe5a5c3a74e6463ab0874c74e23dfca68f0bfc3fe54883f0edf69abb37ac0d`
contains the scope-launch correction but predates the rental guard. Its VM
runtime acceptance must not imply rental guard deployment.

Rental quote, consent and recovery now take the same buyer/seller/content lock
and on-chain/Lightning journal guards as permanent purchases. Two new actual
RPC regressions cover concurrent alternate-rail commit, subsequent quote and
consent retries, exposed Lightning refusal and preserved original records.
Formatting/syntax checks pass; isolated execution waits for the VM-free compiler
slot. No real or repeated payment was sent.

## Final combined qualification including rental, Fleet and controller fixes

The full isolated backend suite now passes **2,012 tests, zero failures, five
existing ignores** against source frozen at `9964b5c1`. All **532 tracked
backend/helper/catalog inputs** were captured before compilation and verified
unchanged afterward. Both new rental RPC regressions, all four Fleet provenance
regressions and eleven selected paid-file recovery/admission cases explicitly
passed. Tests ran in the required isolated runner: 24.77 seconds, 272.3 MiB peak,
no swap, process exit zero. No real payments or live service changes occurred.

This supersedes the pending current-source validation above. It includes the
Indee scope/helper corrections, rental guard `4dde14bf`, and Fleet provenance
`9268930c`. The older normal executable still predates these final changes; a
matching corrected executable and actual VM/full transaction acceptance remain
required before deployment. Actual-node initial-payment response-loss acceptance
also remains open; synthetic test success does not close it.

Durable evidence (backend log, input manifest, receipt and runner):
`~/.local/state/archipelago/release-qualification/paid-file-recovery-20261007/final-combined-9964b5c1/`.
Temporary original: `/tmp/archy-paid-final-combined-fjrs3hIE/`.
The compiler slot was released to Indee immediately after successful provenance
verification; no additional backend compilation was started by this agent.

## Subsequent pre-target rollback safety correction (qualification pending)

Independent source review found that a failed initial drain could enter native
rollback and stop intact original writers, even though no target had started.
Commit `07c7eb0f` fixes this in `supervised_update.rs`: recovery durably chooses
which exact originals to preserve, restores only stopped/missing members, adopts
an operation-owned recreation after a lost start acknowledgement, and refuses
unexpected replacements or violations of an original stopped state. Preserved
members keep their original service recipes. Journal schema 2 prevents an older
backend from ignoring these preservation decisions; existing schema 1 journals
remain readable and migrate before restoration.

Six new regressions cover intact busy originals, partial frontend stop, lost
start acknowledgement, changed preserved identity, old-journal migration, and
unexpected startup of an originally stopped member. Formatting and diff checks
pass; these tests have **not yet executed**. The previous 2,012-test result and
normal executable `913c6572bf689f8c88d25ac6e7436e978fc88a3c1e0cecf26db159967285aa5b`
predate this fix. The next single combined isolated suite waits for the Indee
helper's actual-original restart-policy correction and VM shutdown. No live
node service or payment was changed for this work.

## Combined qualification after pre-target preservation and launch v2

The subsequent full isolated suite passed **2,021 tests, zero failures, five
existing ignores** against source frozen at `32317236`, including native
preservation `07c7eb0f` and helper restart-policy ownership `f42f3298`.
All **532 tracked backend/helper/catalog inputs** remained unchanged across the
build and execution. The receipt explicitly requires all six new pre-target
recovery tests, three launch-identity tests, both rental tests, four Fleet tests,
and eleven selected payment recovery/admission tests. Isolated execution took
26.17 seconds, peaked at 315.3 MiB, used no swap and exited successfully.

Launch v2 records normalize only unique environment ordering and the generated
container hostname, while retaining explicit hostname/environment, command,
mount and other launch semantics. Old raw hashes remain unchanged: no legacy
journal is promoted or accepted through a relaxed comparison. Invalid or
ambiguous environment data prevents fresh supervised capture.

Durable verified evidence:
`~/.local/state/archipelago/release-qualification/paid-file-recovery-20261007/final-combined-32317236/`.
Original temporary directory: `/tmp/archy-paid-final-combined-3fFXJD1v/`.
The slot was released immediately to Indee for the matching normal executable
and fresh v2 synthetic transaction. The older held synthetic transaction remains
unaccepted and preserved in its parent VM lineage; it is not silently rewritten.
Actual-node initial-payment response-loss acceptance remains open. No real or
repeat payment and no live-node lifecycle mutation occurred in this suite.

## 2026-10-08: combined qualification including owned restart overrides

The latest full isolated backend suite passed **2,025 tests, zero failures, five
existing ignores** against source frozen at `dc84a8b6`. All **532 tracked
backend/helper/catalog inputs** remained unchanged. The required receipt checks
now also explicitly include all four native restart-override regression groups,
alongside the existing payment, rental, Fleet, pre-target preservation and launch
v2 regressions. Isolated execution used 320.5 MiB peak memory, no swap, and exited
successfully in approximately 16 seconds.

Native commits `884ea492` and `f78ee252` recognize only the current held
operation's exact API/relay restart override. They verify distinct role records,
matching native/controller identities, admission fence, source ownership,
canonical operation path, exact bytes/hash, private file mode and effective
restart policy. Arbitrary overrides, cross-role borrowing, changed/released
ownership, symlinks and writable ownership paths remain refused. The source also
includes the helper's actual TypeORM history-table correction and qualified
relay shutdown path; full runtime transaction acceptance remains separate.

The isolated VM was retained and then paused through QMP during compilation,
rather than rebooted, to preserve its original PostgreSQL identity and held
recovery state. No VM transactions ran concurrently with the compiler. The
compiler slot was released immediately for the matching normal executable,
followed by resuming that exact guest for recovery acceptance.

Durable SHA-verified evidence:
`~/.local/state/archipelago/release-qualification/paid-file-recovery-20261007/final-combined-dc84a8b6/`.
Temporary original: `/tmp/archy-paid-final-combined-9lHLvVsA/`.
No real/repeated payment or live-node lifecycle change occurred. Actual-node
initial-payment response-loss acceptance remains open.

## 2026-10-08: update retry ownership and current combined qualification

Actual synthetic IndeeHub recovery exposed a stuck Installing overlay after a
failed update. Image byte progress overwrote Updating, while failure cleanup
correctly cleared only its Updating owner. Commit `2b2fd2b5` makes byte progress
preserve Updating and its existing readiness, matching phase/message updates.
The scanner and failure finalizer were not broadened to clear unrelated installs.

The new regression exercises the actual StateManager/RpcHandler, local-image
and streamed progress, phase/message changes, failure cleanup and normal install
behavior. Its first run caught an incorrect test assumption about wrapper
readiness (one passed, one failed); `105454bd` captures the wrapper's actual
readiness instead. The corrected focused run passed both tests.

The full isolated suite at `105454bd` then passed **2,026 tests, zero failures,
five existing ignores**, in 18.39 seconds, with 365 MB peak and no swap. All
**532 tracked backend/helper/catalog inputs** remained unchanged. The receipt
explicitly checks payment/rental/Fleet, pre-target preservation, v2 identity,
owned restart overrides and the new progress regression. This receipt includes
the current helper through `260e1327`; older suite receipts remain historical.

SHA-verified logs/manifests/receipts, including the initial fixture failure, are
retained at
`~/.local/state/archipelago/release-qualification/paid-file-recovery-20261007/final-combined-105454bd/`.
Temporary full evidence: `/tmp/archy-paid-final-combined-jsKnboX5/`. The compiler
slot was handed to IndeeHub for its matching executable and actual transaction
acceptance. No real/repeat payment or live-node lifecycle mutation occurred.
Full IndeeHub target-start/rollback acceptance and actual-node initial-payment
response-loss acceptance remain open.
