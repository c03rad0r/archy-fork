# Release OTA and ISO from latest `main`

This is the operator handoff for producing a release from whatever commit is
the reviewed tip of `main` at the time. A release is not ready merely because
the artifacts build. Every active regression, live-node acceptance, mirror and
publication gate below must pass.

## 1. Freeze an exact reviewed source revision

Use a fresh clone or clean worktree. Do not build from an UAT node or a dirty
tree.

```bash
git switch main
git pull --ff-only ngit main
git status --short
git rev-parse HEAD
python3 scripts/check-git-mirrors.py --local --all
```

`git status --short` must be empty. The full mirror audit must show matching
advertised branches/tags on ngit and Gitea. Inventory and resolve any unrelated
drift; never force-push, delete refs, or rewrite published history merely to
make the check green. Record the accepted ngit proposal and merge commit in the
release acceptance ledger.

Read and retain every unchecked item in:

- `docs/post-1.8.22-regressions-20261001.md`
- the current release acceptance ledger and UAT checklist

Run the source gates and the documented live-node matrix. On a node with
installed apps, backend tests must run only through the isolation wrapper:

```bash
scripts/test-backend-isolated.sh
tests/release/run.sh
python3 scripts/check-app-catalog-drift.py --release --strict
```

Keep source/unit results separate from actual-node acceptance. In particular,
prove that paid-file recovery never makes a second payment and that app cleanup
preserves wallets, persistent data, and uninstall decisions.

## 2. Prepare and sign the OTA release

Choose a new SemVer that has never been published. Do not move or replace an
existing release tag. Curate the new top entry in `CHANGELOG.md`, then preview:

```bash
release_version=X.Y.Z-alpha
scripts/create-release.sh "$release_version" --dry-run
```

When all gates and review are complete, run the real preparation from clean
`main` in an interactive terminal:

```bash
scripts/create-release.sh "$release_version"
```

The script builds the backend, frontend, AIUI and radio tools, creates a signed
pending manifest and staged OTA artifacts, commits the release preparation, and
creates annotated tag `v$release_version`. During the signing prompt, paste the
24-word release master mnemonic **once**, press Enter, then Ctrl-D on the next
line. Never put the mnemonic in a command, file, chat, log, or Git. Do not use
`RELEASE_MASTER_MNEMONIC` for the normal operator ceremony.

If an already-prepared pending manifest needs signing on the offline/operator
terminal, first ensure the release binary was built from the exact frozen
commit, then run:

```bash
bash scripts/sign-manifest.sh \
  "releases/pending/v${release_version}/manifest.json"
```

The script cryptographically verifies the result against the public release
root pinned in the binary. A failed verification is a hard stop.

Review the preparation commit, tag target, staged artifacts and signature. Push
the exact reviewed commit/tag to both publication mirrors according to the
ngit-first workflow; do not create independent merges on each platform.

## 3. Build and sign the installer ISO

The ISO builder requires clean `main`, matching versions, the annotated tag and
the signed live manifest. After OTA publication has safely promoted the live
manifest (next section), return to the exact tagged clean tree and run:

```bash
scripts/build-iso-release.sh
```

For a normal release, do **not** pass `--skip-gates` or `--no-qemu`. The command
runs the release harness, strict catalog check, artifact checks, ISO build,
mount-level smoke test and headless QEMU boot. Keep its final PASS summary as
release evidence.

Sign the generated ISO checksum document in the operator ceremony:

```bash
iso_path=/absolute/path/to/archipelago-${release_version}.iso
bash scripts/sign-iso-checksums.sh "$iso_path"
core/target/release/archipelago ceremony verify \
  "$iso_path.sha256.json"
sha256sum -c "$iso_path.sha256"
```

The signing script again expects one mnemonic paste, Enter, then Ctrl-D. It
does not place the mnemonic on disk.

## 4. Publish without exposing incomplete updates

Configure the Gitea remote with a public HTTPS URL and keep its credentials in
Git's credential helper, never in the remote URL. Then run:

```bash
scripts/publish-release-assets.sh "$release_version" gitea-vps2
```

This script deliberately pushes the tag first, creates the release, uploads and
byte-verifies the OTA assets, and pushes the manifest-bearing `main` last. When
the signed ISO files exist, the same command attaches and verifies them. Never
manually publish `releases/manifest.json` before its referenced assets are
downloadable and verified.

After publishing, mirror the exact accepted `main` commit and annotated tag to
ngit and Gitea, then verify both:

```bash
python3 scripts/check-git-mirrors.py --local \
  --ref "refs/tags/v${release_version}"
python3 scripts/check-git-mirrors.py --local --all
scripts/check-release-assets.sh releases/manifest.json
```

A failed push, missing ref, tag-object mismatch, unavailable mirror, incomplete
asset, or mismatched byte hash blocks publication. A main-only check is not full
historical mirror parity.

## 5. Canary and final acceptance

Before announcing the release:

1. Apply OTA to one non-critical canary from the signed manifest.
2. Reboot it and verify management health, signer/native identity, installed and
   deliberately removed apps, wallet state, persistent app data, terminal,
   networking/FIPS, and the active regression checklist.
3. Exercise rollback/recovery without changing wallets or app data.
4. Boot the ISO in QEMU and on representative physical hardware; perform a clean
   install and an upgrade-path check, then repeat the same acceptance matrix.
5. Verify public release downloads and both Git mirrors once more. Record exact
   commit, annotated tag object, artifact hashes, signer verification, node
   evidence, ngit proposal/merge, known deferrals and operator acceptance in the
   release ledger.

Do not call the release complete if any required live acceptance, signature,
mirror, asset, OTA reboot/rollback, or ISO boot/install result is missing.
