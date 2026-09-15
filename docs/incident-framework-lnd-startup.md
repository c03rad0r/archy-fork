# Framework: LND startup, missing Receive address, false zero balance

**Status: OPEN — Framework inspected live; candidate fix under test; not fixed yet.**

Reported: 2026-09-15. Source inspected: main at `3b9b74da` (v1.8.17-alpha publication).
The Framework's installed version and exact incident time have not been verified.

## Mandatory priority across sessions

The user explicitly requested that this be investigated and fixed on the node
before resuming unrelated work in later sessions. `AGENTS.md` in the repository
and `/home/archipelago/.codex/AGENTS.md` carry this session-start priority.
Only live verification below, or an explicit user change of priority, clears it.

## Reported observations

- Framework stopped showing its Lightning address in Receive.
- After a restart, LND did not initialize and the UI displayed a balance of zero.
- Manually restarting LND restored operation.
- Node access will be supplied later. No Framework connection, restart, wallet
  operation, or deployment was performed during this offline investigation.
- Still clarify whether the restart was a full reboot or management-service
  restart, and which Receive item vanished: a Lightning invoice, an on-chain
  address, or the Cashu tab's `@minibits.cash` address.

A successful manual restart is a workaround, not a root cause or durable fix.
The zero display does not establish that any funds were lost. Its relation to
v1.8.17-alpha is unknown; do not infer a release regression from timing alone.

## Confirmed source findings

### 1. LND errors can be presented as successful zero balances

`core/archipelago/src/api/rpc/lnd/info.rs`, `handle_lnd_getinfo`:

- `/v1/getinfo` is decoded without checking HTTP success. Its response fields are
  optional, so an error object such as `{"code":14,"message":"wallet not ready"}`
  can deserialize with every expected field absent instead of rejecting the call.
- Channel and blockchain balance requests suppress connection/JSON failures and
  substitute responses with absent balances. HTTP status is not checked here either.
- Missing or unparsable balances become `0` through `unwrap_or(0)`.
- `neode-ui/src/views/Home.vue`, `loadWeb5Status`, treats this RPC response as
  success, sets the wallet connected flag, overwrites prior balances, and can
  persist the false zero in the wallet snapshot. Its existing failure handling
  preserves prior balances only when the RPC actually rejects.

This is a confirmed code defect and a plausible explanation for the reported
display. It is not proof of the Framework's failure sequence.

Required fix: reject unsuccessful/incomplete LND balance responses or model
availability explicitly end to end. Never translate unavailable data into a
verified zero. Preserve known balances with a clear unavailable/stale indication;
show an unknown state when no valid balance is known. Genuine successful zeros
must still render as zero. Cover outage, partial failure, cold load, and recovery.

### 2. Startup readiness and wallet unlock need live evidence

- `main.rs` runs crash/container boot recovery before starting the reconciler.
- `crash_recovery.rs` can start existing containers directly.
- `container/prod_orchestrator.rs` runs LND post-start hooks on explicit restart
  and on normal reconciliation of already-running containers. Therefore it is
  incorrect to conclude that running containers categorically skip unlock.
- `container/lnd.rs::ensure_wallet_initialized` checks wallet existence and
  `/v1/getinfo`, then attempts unlock. Its unlock wait budget is approximately ten
  minutes; per-request timeouts can extend elapsed time. Historical comments
  describe slow database startup and restart loops, but that is not Framework evidence.
- `health_monitor.rs` models LND's Bitcoin dependency. Container-running state
  alone is not proof of wallet readiness, Bitcoin connectivity, or invoice readiness.

Investigate boot ordering, Bitcoin readiness, listener/port mapping, wallet unlock,
mount availability, stopped markers, restart counters, and actual reconcile logs.

### 3. Destructive automatic recovery exists; exclude it from diagnosis

`container/lnd.rs::ensure_wallet_initialized` calls
`recreate_wallet_destructively` when all candidate passwords are rejected. That
function can delete the LND chain and graph data directories. Its comment assumes
alpha wallets hold no real funds; that assumption must not guide this investigation.

No evidence establishes that it ran on Framework. Preserve the original wallet
and channels; rejected passwords must lead to a recoverable error, not automatic
wallet deletion. Review and disable this destructive fallback before using a
modified initialization path as a repair. The existing
`unlock_existing_wallet_no_wipe` demonstrates the non-destructive error behavior.

### 4. The missing address must be identified precisely

`ReceiveBitcoinModal.vue` generates Lightning invoices using `lnd.createinvoice`
after a readiness check, and Bitcoin addresses using `lnd.newaddress`. Its Cashu
Lightning address uses `wallet.ecash-lnaddress` and the Minibits service separately.
Do not assume the Minibits address disappears because LND is down. Trace the actual
tab and response once the user clarifies and the node can be inspected.

## Next session: live investigation order

1. Request Framework access and verify node identity without publishing its hostname,
   address, credentials, or wallet identifiers. Do not substitute the development box.
2. Record installed backend/image versions, boot and incident timestamps, and exact
   restart/action sequence. Capture current and previous-boot management/LND logs
   before another restart can obscure evidence. Keep raw logs private and redact
   secrets, invoices, wallet identifiers, and personally identifying data in summaries.
3. Read container/service state, restart counters, mounts, stopped markers, listener
   mappings, Bitcoin readiness, LND wallet state, and authenticated API results.
   Never dump container environments, macaroons, passwords, seeds, or wallet databases.
4. Compare HTTP status and data from LND getinfo/balance endpoints with the RPC and
   visible Receive/balance state. Distinguish unavailable data, locked wallet,
   syncing wallet, and genuine zero. Preserve last-known balance evidence privately.
5. Establish whether the manual restart ran a missing/failed hook, waited out a
   dependency, refreshed networking/credentials, or masked another failure.
6. Implement the evidenced startup repair and unavailable-balance handling with
   regressions. Preserve wallet/channel state and arrange recovery access before
   deploying or deliberately rebooting the node.

## Acceptance criteria — all required to close

- [ ] Root cause of Framework startup failure supported by node evidence.
- [ ] Fix implemented and focused regression tests pass.
- [ ] Failed, locked, delayed, and partial LND responses never masquerade as a
      fresh zero balance; genuine zero remains correct.
- [ ] Existing wallet identity and channel state preserved through the repair.
- [ ] Framework starts LND and reaches usable wallet readiness after a controlled
      full reboot, without manually restarting LND.
- [ ] The originally affected Receive flow works after boot and after recovery;
      outages show an actionable state and recover without requiring a page reload.
- [ ] Balance display verified against authenticated LND data on the node.
- [ ] Logs show no restart loop, repeated unlock failure, or wallet-recreation path.
- [ ] Evidence, tested versions, deployment, and limitations recorded here; user
      informed of live results. Only then set status RESOLVED and clear the blockers.

## Work completed so far

2026-09-15: source investigation and persistent session-start instructions only.
No code fix, release, node deployment, or live reproduction for this incident yet.

## Live evidence captured 2026-09-15

Access was provided during the same session. Read-only inspection confirmed:

- Framework runs `1.8.17-alpha-dev`; the current full boot began at 18:40:09 UTC.
- LND opened its databases in 6.7 seconds and requested its wallet password at
  18:40:20. It then rejected GetInfo/ChannelBalance/WalletBalance as wallet locked.
- The management service's first sequential reconcile pass was occupied by
  unrelated image recovery, including a missing voice image from 18:40:24 and
  later a missing Core Lightning image. Manifests are iterated from a HashMap;
  wallet readiness has no initial priority. Boot recovery itself completed at
  18:40:18; the first full app-reconcile report appeared at 18:44:34.
- The user's manual LND restart was recorded at 18:42:33. The replacement LND
  process started at 18:42:40, requested its password at 18:43:05, and unlocked
  at 18:43:07 through the explicit restart hook. This supports delayed unlock
  behind unrelated recovery, rather than a missing wallet or bad password.
- At inspection, `/v1/state` reports SERVER_ACTIVE; getinfo reports chain and
  graph sync and two active channels. Both authenticated balance endpoints
  report nonzero balances. No wallet-recreation event was found in captured logs.
- The Minibits RPC separately fails with “The ecash wallet has no seed yet”.
  `wallet/cashu_seed.json` and `wallet/minibits.json` are absent. The existing
  ecash wallet is present with proofs and an August modification timestamp.
  Do not overwrite it or generate an unrelated recovery identity. Still identify
  which Receive item the user meant before declaring this part repaired.

Private raw evidence: `/home/archipelago/.local/state/archy-incidents/framework-lnd-20260915/`.
Files have mode 0600 and the directory 0700. Do not commit or publish raw logs.

Candidate changes on `investigate/framework-lnd-startup`:

- Run Bitcoin and LND reconciliation before unrelated image pulls/builds.
- Reject failed/incomplete LND balance responses instead of manufacturing zeros.
- Preserve known Home balances on invalid responses, visibly label unavailable
  balances, and clear the warning after a successful refresh.
- Remove automatic destructive wallet recreation; failed unlock preserves data.
- Add backend outage/zero/ordering regressions and UI failure/recovery coverage.

These changes are not yet deployed or verified through a Framework reboot.
