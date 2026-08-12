# Archived ISO build recipes

These scripts built the Archipelago auto-installer ISO (bundled and
unbundled variants). As of v1.7.43-alpha, ISOs are no longer part of the
release deliverable. Releases ship as tarballs consumed by
`scripts/self-update.sh` on existing nodes.

Archived here rather than deleted so they can be resurrected if ISO
distribution is reintroduced.

## Contents

- `build-auto-installer-iso.sh` — orchestrator, bundles container images into squashfs
- `build-unbundled-iso.sh` — thin wrapper that sets BUNDLE_IMAGES=0 and delegates
- `test-iso-qemu.sh` — smoke-tests a built ISO under QEMU
- `scripts/convert-iso-to-disk.sh` — converts an ISO to a raw disk image
- `BUILD-ISO-STATUS.md`, `ISO-BUILD-CHECKLIST.md` — contributor guides
- `branding/isohdpfx.bin` — isolinux MBR hybrid image
- `.gitea-workflows/build-iso-dev.yml` — CI workflow that ran the build+smoke-test

## To resurrect

1. `git mv image-recipe/_archived/* image-recipe/` (adjust paths back)
2. Restore `.gitea/workflows/build-iso-dev.yml`
3. Re-add release-process references (see `scripts/create-release.sh`,
   `docs/BETA-RELEASE-CHECKLIST.md`, `docs/hotfix-process.md`, `README.md`).

## Why archived

The release flow is simpler and faster as tarball-only:
- `releases/vX.Y.Z-alpha/archipelago` (backend binary)
- `releases/vX.Y.Z-alpha/archipelago-frontend-X.Y.Z-alpha.tar.gz` (frontend + AIUI + filebrowser UI assets)
- `releases/manifest.json` (pointers + changelog)

Nodes pull these via `scripts/self-update.sh` from either Gitea mirror.
Filebrowser and AIUI remain bundled inside the frontend tarball and deployed
atomically by `self-update.sh`.
