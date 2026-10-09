## Unit tests on a live node

Run backend unit tests through `scripts/test-backend-isolated.sh`. Do not run
unrestricted `cargo test` on a node with installed apps: older mocked-runtime
tests still reached real service commands. The runner isolates wallet data,
service buses, container storage, networking, and process IDs. Compilation with
`cargo test --no-run` is safe. Keep separately authorized live checks explicit.

## Active release regression checklist

Before resuming release work, read
`docs/post-1.8.22-regressions-20261001.md` and retain its unfinished tasks.
The operator requested that every reported issue be tracked, fixed and tested
before another OTA/ISO. Keep source/unit-test results separate from actual-node
acceptance. In particular, paid-file recovery must not send another payment,
and app cleanup must preserve wallets, persistent data and uninstall decisions.
Do not mark the new paid-file incident resolved merely because the earlier
Framework LND startup incident was closed.

## Gitea and ngit mirror parity

Nostr Git (`ngit`) is the canonical contribution and review platform. Gitea
(`origin`) mirrors accepted code on `main` and release tags. Both are required
publication mirrors; duplicate PRs and proposal branches on Gitea are not required.
For every change, including fixes and release preparation:

- Review and merge once. Push the exact same resulting commits to both mirrors;
  never independently squash, rebase or merge the same change on each platform.
- Open new contributions and PRs on ngit; review and merge there, then mirror the
  exact accepted main commits to Gitea. Record the ngit proposal and resulting
  merge commit in the release ledger. Existing Gitea PRs must be reviewed and
  explicitly linked to their ngit replacement or accepted result before closing;
  do not abandon contributions or mark unmerged changes as merged. PR numbers,
  reviews and discussions remain platform-specific; matching Git refs does not
  prove their synchronization.
- Push main and release tags to both mirrors. Preserve commit history
  and annotated tag objects/signatures. Do not resolve drift by force pushing,
  deleting remote refs, or rewriting published history without explicit approval.
- After publishing source, run `python3 scripts/check-git-mirrors.py --local`.
  Include each additional shared branch or release tag with repeated `--ref`
  arguments (full `refs/heads/...` or `refs/tags/...` names).
- Before OTA, catalog or ISO publication, require matching reviewed local and
  remote main and release tag refs, and record ngit PR dispositions in the
  release acceptance ledger. A failed push, unavailable mirror, missing ref or
  mismatch blocks publication; never describe a partial push as synchronized.
  Run `--all` for a complete advertised branch/tag audit; a main-only pass must
  never be described as full historical mirror parity. Proposal-only branches
  may intentionally differ. Existing unrelated drift
  must be inventoried explicitly rather than silently overwritten.
