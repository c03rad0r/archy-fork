# Archipelago — Public Demo deploy

A click-to-play demo of the Archipelago UI, backed entirely by a mock backend.
Every visitor gets an **isolated, ephemeral sandbox** (own apps, wallet, files),
real container runtimes are never touched, and Bitcoin runs on **signet** test
coins. **Login password: `entertoexit`** (shown on the login screen).

This directory is the full contents of the public `archy-demo` repo. It holds no
source — only this compose file that pulls prebuilt `:demo` images.

## Deploy in Portainer

1. **Stacks → Add stack → Repository** (or paste `docker-compose.yml` into the web editor).
2. Set environment variables (see `.env.example`) — at minimum `REGISTRY`, and
   `ANTHROPIC_API_KEY` if you want the AI chat panel.
3. Deploy. The UI is served on `:2100` (override with `DEMO_WEB_PORT`).

To pick up a new build, redeploy the stack (or wire the CI Portainer webhook).

## How it stays current

The images are built from the Archipelago monorepo by
`.github/workflows/demo-images.yml` on every change to `neode-ui/`, tagged `:demo`
and `:<git-sha>`, and pushed to `REGISTRY`. Editing the real UI → CI rebuilds →
redeploy here. No source lives in this repo.

## What's mocked

- **Per-visitor isolation** — state keyed by a `demo_sid` cookie, idle-reaped.
- **Apps** — install/uninstall/start/stop are simulated (no real Docker).
- **Wallet/Bitcoin** — signet-flavored; use the in-UI faucet for test sats.
- **Files** — real per-session upload/rename/delete, 50 MB quota, wiped on reap.
- **Intro** — replays once per calendar day per browser.

## Building a reviewed demo revision

The web image builds both the dashboard and AIUI from the checked-out source;
it does not use the historical `demo/aiui` bundle. CI supplies the full commit
as `SOURCE_REVISION`. For a local source build, run:

```sh
SOURCE_REVISION=$(git rev-parse HEAD) docker compose -f docker-compose.demo.yml build
```

AIUI uses its frozen dependency lockfile, type checking and the canonical
`/aiui/` build verifier. Its `BUILD-INFO` identifies that exact source revision.
Prepare and test the images before changing the public demo stack; retain the
previous image IDs for rollback.
