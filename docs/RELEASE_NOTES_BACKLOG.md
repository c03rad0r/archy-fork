# Release Notes Backlog

## Next Release Required Work

- Backfill missing or thin historical release notes before cutting the next release.
- Audit every `CHANGELOG.md` section from `v1.7.44-alpha` through the current release.
- Replace raw commit-hash entries with user/operator-facing bullets that explain behavior changes, operational impact, validation, and known limitations.
- Ensure `releases/manifest.json` changelog entries come from curated `CHANGELOG.md` notes only.

## Release Note Policy

- Every release must have at least three curated bullets.
- Raw `git log --oneline` output is not acceptable release documentation.
- Notes should answer what changed, why it matters, what operators should expect, and any known limitations.
- `scripts/check-release-manifest.sh` is the enforcement gate before publishing artifacts.
