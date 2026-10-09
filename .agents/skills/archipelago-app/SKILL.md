---
name: archipelago-app
description: Build, package, test, and update a manifest-driven Archipelago app using the real app developer contract, rootless runtime, and lifecycle acceptance flow.
metadata:
  short-description: Build a real Archipelago app
---

# Archipelago app development

Use this skill when creating or changing an Archipelago app, manifest, container
build, app integration, credentials, signer flow, or app preview. Always load
`archipelago-design` for newly authored UI.

Read [app-contract.md](references/app-contract.md),
`docs/app-developer-guide.md`, and `docs/app-manifest-spec.md` before coding.

## Required loop

1. Create a dedicated worktree or project directory and choose the smallest
   useful app scope. Prefer the maintained starter and its pinned dependencies.
2. Implement the app as a manifest, rootless container, persistent data path,
   truthful health/readiness check, declared interface, and tests. Never add a
   per-app Rust installer or rootful/Docker-socket shortcut.
3. Use generated secrets or declared secret files; never put credentials in a
   manifest, image, logs, URL, or frontend bundle. Keep production wallets and
   app databases outside development fixtures.
4. Validate the manifest and build context, then run the app through install,
   start, stop, restart, manager restart, uninstall with data preservation, and
   reinstall. Verify the real My Apps/Services launch path, not only a direct
   port.
5. For UI, exercise standalone and embedded modes at 320, 390, 768, and 1440px
   plus phone landscape, keyboard navigation, loading/empty/error/retry/success,
   safe areas, and reduced motion.
6. Report source, disposable-node, actual-node, and release-artifact evidence
   separately. Catalog publication is a separate request.

Important runtime facts:

- `/opt/archipelago/apps` is rebuilt from the runtime payload at backend start;
  staging only there will be lost. Follow the developer guide's payload path.
- Signed catalog entries take precedence over disk manifests for catalog apps.
- Installed does not mean ready or launchable; use health and readiness evidence.
- App interfaces describe the service behind the gate. Do not hard-code a node
  IP, scheme, app path, or host frame URL into app code.

## References

- [app contract and acceptance](references/app-contract.md)
- [design routing](../archipelago-design/SKILL.md)
