# Next OTA and raw ISO after 1.8.21

**Status: implementation and acceptance in progress; NOT ready to release.**

This is the consolidated execution checklist for the operator's chat requests.
A targeted node repair is not completion of the release. Finish the remaining
acceptance gates, preserve live wallets and app data, and publish both artifacts
through git and ngit. No universal absence of future failures is claimed.

## Changes already shipped in 1.8.21 or earlier

Keep these fixes in the next build and include relevant regressions:

- Mempool image/catalog version agreement and update-button behavior.
- Minibits integration; Framework automatic LND startup and safe unavailable
  balances. Framework incident closed with operator acceptance.
- Shorter, single-column ecash backup messaging.
- AIUI transparent background on desktop/mobile.
- Cashu paid-file keyset/mint/error/refund corrections, with live purchases.
- mempool.space explorer fallback, preserving local/custom explorer settings.
- Bitcoin install pruning choice and matching automatic-pruning behavior.
- Friendly Bitcoin warmup and LND install/start/sync waiting states.
- Raw ISO publishing and upload support.

The Primal automatic LNURL comment problem was traced to sender behavior and
Minibits metadata. The user accepted clearing the sender's automatic comment;
no unsupported local metadata rewrite or wallet-identity replacement is planned.
See the Framework incident and 1.8.21 execution records for evidence/limits.

## New release scope and gates

| Task | Implemented/verified | Remaining before release |
| --- | --- | --- |
| X250 Bitcoin picker | Inline choices; actual Chromium kiosk selection, readability and pruning layout passed | Include in final UI/build checks |
| App disappearance/readiness | Durable inventory and safe lifecycle repair; delayed HTTP and desktop/mobile hard-refresh checks passed | Final lifecycle/reboot gate on candidate |
| X250 GitWorkshop/Nginx | Missing build contexts restored, dependency/build checks and live UI passed; Nginx slow pull diagnosed; truthful progress label | Verify both artifact payloads contain all build contexts |
| PRs 161/162 | Reviewed, repaired, merged/closed normally; combined regression suite passed | Candidate funded Tor-only purchase, change and Files acceptance |
| Gitea/Portainer | Root cause confirmed; source network/backup/retry/catalog changes; real X250 routing repair and restart verified; private Git, SSH, LFS, registry and browser fixture checks passed | Automatic migration/rollback, failure retry, reverse install order, reboot convergence, production Source API/UI, signed delivery |
| Angor headless store service | Current official guide reviewed: standard Mempool with optional strfry relay | Implement using app-development docs; safe dependency/relay integration; official logo; API and lifecycle acceptance |

Durable payment receipts after a lost seller response remain a separately
recorded design follow-up. Preserve the truthful unconfirmed-refund warning and
prevent duplicate automatic payment; do not describe an unconfirmed refund as
completed. See PR review for the accepted scope and coverage limits.

## Final release checklist

- [ ] Finish all new-scope implementation and specific acceptance above.
- [ ] Remove disposable fixtures and temporary test overrides; verify native
      Bitcoin/LND identity and start-state baselines remain protected.
- [ ] Commit and push completed source changes to git and ngit.
- [ ] Run final backend/UI/regression/release gates on the final source; inspect
      skipped tests and report actual hardware/runtime coverage.
- [ ] Prepare compatible signed app catalog; old runtimes must not apply a
      migration before they have backup/recovery support.
- [ ] Version/changelog and OTA payload prepared, validated and signed by user.
- [ ] Raw ISO built; payload hashes/content verified; installer boot tested.
- [ ] User signs ISO checksums; publish OTA and ISO plus verification files on
      git and ngit; independently read back hashes and update discovery.
- [ ] Provide LAN scp command for the new raw ISO.

Latest backend source verification: 1,605 passed, zero failed, four existing
ignored tests. This is one layer of evidence, not a substitute for live gates.
