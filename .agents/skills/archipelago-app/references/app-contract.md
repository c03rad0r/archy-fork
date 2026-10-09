# App contract

Start with `docs/app-developer-guide.md` and `docs/app-manifest-spec.md`; those
files are authoritative for fields and launch behavior. Validate with:

```bash
./scripts/validate-app-manifest.sh apps/<id>/manifest.yml
python3 scripts/generate-app-catalog.py
python3 scripts/check-app-catalog-drift.py --release --strict
```

Use pinned image versions, read-only root, no-new-privileges, minimal
capabilities, rootless Podman, declared persistent data under
`/var/lib/archipelago/<id>`, health checks, generated/declared secrets, and
truthful interfaces. Test install/start/stop/restart/manager-restart/uninstall
with data preservation/reinstall on a disposable node. Do not replace the
signed catalog to make a local test app appear.
