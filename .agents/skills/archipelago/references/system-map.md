# System map and safe recipes

The native backend is `core/archipelago`; the Vue dashboard is `neode-ui`; ISO
and first-boot material lives under `image-recipe` and `scripts`; app manifests
are under `apps/<id>/manifest.yml`. Read `CLAUDE.md`, `AGENTS.md`, and the current
release checklist before release work.

Use the existing typed RPC and orchestration layers for app lifecycle, network,
wallet, identity, and service operations. Inspect a matching handler before
adding an endpoint. Preserve the existing session cookie, CSRF, Origin, and
app-gate protections.

For source tests, run frontend checks from `neode-ui`. Backend unit tests run
only through `scripts/test-backend-isolated.sh`; live host checks must be named
and explicit. Do not touch real wallet/payment/channel state for a test fixture.

Developer changes belong in a dedicated worktree. Keep generated catalog,
runtime payload, release artifacts, and local node state separate until the
request explicitly includes packaging or deployment.
