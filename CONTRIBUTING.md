# Contributing to Archipelago

This project is preparing for public developer contribution. The highest-value
contributions are focused fixes, tests, app manifests, documentation
improvements, and clear bug reports with reproducible evidence.

## Development setup

### Frontend

```bash
cd neode-ui
npm install
npm start
npm run type-check
npm test
```

### Backend

```bash
cd core
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Linux is required for host integration work involving Podman, systemd,
networking, or image builds. Frontend development works locally with the mock
backend.

## App manifests

App packages live under `apps/<app-id>/manifest.yml` and use the schema
documented in [docs/app-manifest-spec.md](docs/app-manifest-spec.md). Validate
before submitting:

```bash
./scripts/validate-app-manifest.sh apps/<app-id>/manifest.yml
python3 scripts/generate-app-catalog.py
python3 scripts/check-app-catalog-drift.py --release --strict
```

App submissions must:

- pin container image versions;
- avoid hardcoded secrets;
- use `security.no_new_privileges: true`;
- use `security.readonly_root: true` unless the manifest explains why writable
  root is required;
- request only necessary Linux capabilities;
- store durable data under `/var/lib/archipelago/<app-id>/`;
- define truthful health checks and launch interfaces for user-facing UIs.

## Code style

- Rust: prefer `?` over `unwrap()`/`expect()` in production paths.
- Rust: use `tracing` for structured logs.
- TypeScript: avoid `any`; use explicit types or `unknown`.
- Vue: prefer `<script setup lang="ts">`.
- Keep changes scoped; do not mix drive-by refactors with behavioral changes.
- Remove dead code rather than commenting it out.
- Add tests for new behavior and regression tests for bug fixes.

## Pull requests

Contributions, PRs and reviews live on **ngit**. Clone the canonical repository:

```text
nostr://npub1w3sqdkrhn0gyuvsex32effzgnfpyde6qrrc4u467flg5e9txh4wsfn5vjg/relay.ngit.dev/archy
```

Gitea is a conventional Git mirror of accepted `main` commits and release tags.
You do not need to open a duplicate Gitea PR. Existing Gitea contributions will
be reviewed and linked to their ngit replacement or accepted result before
closure.

1. Open one focused ngit PR per behavior or documentation change.
2. Explain what changed, why it changed, and how it was verified.
3. Include screenshots for UI changes.
4. Link relevant issues or docs.
5. Keep generated catalog changes in sync with manifest changes.

### Maintainer publication gate

Merge once through the ngit contribution workflow, then push the exact same
accepted commits to Gitea. Do not independently merge or squash on each mirror.
Publish identical release tag objects, including annotations and signatures.
After pushing main, verify:

```bash
python3 scripts/check-git-mirrors.py --local
```

Before publishing release artifacts, also check the actual release tag:

```bash
python3 scripts/check-git-mirrors.py --local --ref refs/tags/v1.9.0-alpha
```

Use the release's actual tag name. Missing refs, inaccessible mirrors or differing
object IDs block publication. Record the ngit PR disposition and resulting merge
commit in the release acceptance ledger. Resolve drift deliberately; do not
force-push or delete published history without explicit approval.

The checker is read-only. `--all` audits every advertised branch and tag; ngit
proposal branches may intentionally differ from Gitea. A main-only pass proves
only main parity, and no Git ref check verifies PR discussions or review state.

Suggested commit format:

```text
feat: add backup scheduling
fix: reject unsafe manifest volume
docs: clarify app deployment flow
test: cover catalog drift check
```

## Reporting bugs

Include:

- exact version or commit;
- host platform and architecture;
- steps to reproduce;
- expected and actual behavior;
- logs from the relevant component;
- screenshots for UI issues.

## Security

Do not report vulnerabilities in public issues. Follow [SECURITY.md](SECURITY.md).

## License

By contributing, you agree that your contribution is licensed under the
project's MIT License.
