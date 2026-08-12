# Public Demo Deployment — Design

**Status:** design (2026-06-22)
**Goal:** a public, click-to-play demo of the Archipelago UI that **auto-tracks
the real code** yet stays **separated** from the private monorepo and its
secrets/backend. Deployed via **Portainer**, mock-data driven, with working file
storage and a testnet-flavored Bitcoin sandbox so visitors can play freely.

See also: `neode-ui/mock-backend.js` (existing mock), `docker-compose.demo.yml`
(existing demo stack).

---

## 1. What already exists (the 70%)

The demo is mostly built. Inventory:

| Asset | Path | State |
|-------|------|-------|
| Mock backend (Node/Express + ws) | `neode-ui/mock-backend.js` (~3,862 lines) | 95+ JSON-RPC methods: auth, package lifecycle, Bitcoin/LND wallet, mesh, federation, identity, monitoring, mock filebrowser |
| Mock data | `mockData` / `walletState` / `MOCK_FILES` in `mock-backend.js` | rich; 10 pre-installed apps, 30+ marketplace apps, wallet balances, seeded files (Music/Documents/Photos/Videos) |
| Demo compose | `docker-compose.demo.yml` | `neode-backend` (mock, `:5959`) + `neode-web` (nginx, `:4848`); header already says "Deploy via Portainer" |
| Backend image | `neode-ui/Dockerfile.backend` | Node 22 Alpine → `node mock-backend.js` |
| Web image | `neode-ui/Dockerfile.web` | multi-stage `vite build` → nginx |
| Demo nginx | `neode-ui/docker/nginx-demo.conf` | proxies `/rpc/v1`, `/ws`, `/app/*` to the mock backend |
| Precedent | `indee-demo` Portainer stack | separate stack referencing a **pre-built image** — the pattern we extend |

**Gaps for a *public* (not dev) demo:** state is global (visitors collide),
uploads are no-ops, Bitcoin block height is hardcoded, no CI image pipeline, no
separated public deploy repo.

---

## 2. Architecture: source in monorepo, demo ships as images, public repo is thin

The tension — "must update as I update the real code" **and** "sort of
separated" — is resolved by separating at the **deploy layer, not the source
layer**.

```
  monorepo (private — single source of truth)
    neode-ui/ + mock-backend.js
            │  push to main
            ▼
  CI: build archy-demo-web + archy-demo-backend
            │  push :demo / :latest
            ▼
  registry (source.archipelago-foundation.org / vps2)
            │  Portainer webhook / re-pull
            ▼
  archy-demo (public repo — tiny)
    docker-compose.yml  ──referencing pre-built images──▶  Portainer ▶ demo.<host>
    .env.example
```

- **Single source of truth = the monorepo.** `neode-ui/` and `mock-backend.js`
  stay where they are, so the demo tracks real code automatically — no fork to
  sync, no drift.
- **Separation = the public repo never holds source.** `archy-demo` contains only
  a `docker-compose.yml` (image refs) + `.env.example` + README. No Rust backend,
  no secrets, no UI source. Safe to make public.
- **Auto-update flow:** edit code → push → CI rebuilds demo images → Portainer
  redeploys. The public compose file is touched rarely (only when service shape
  changes).

**Why not a true fork / `git subtree split`?** It works but needs a sync job
*and* re-exposes UI source publicly. The image pipeline gives stronger
separation (zero source leak) **and** zero manual sync. (Decided 2026-06-22.)

---

## 3. Work items

### 3.1 CI image pipeline
- On push to `main` (path filter: `neode-ui/**`), build:
  - `archy-demo-backend` from `neode-ui/Dockerfile.backend`
  - `archy-demo-web` from `neode-ui/Dockerfile.web` (`build:docker`)
- Tag `:demo` + `:<git-sha>`, push to the registry.
- Trigger Portainer redeploy (stack webhook) on success.

### 3.2 Public `archy-demo` repo
- `docker-compose.yml` mirroring `docker-compose.demo.yml` but **`image:`
  references instead of `build:`** (pull `:demo`, no build context).
- `.env.example` (`ANTHROPIC_API_KEY`, `VITE_DEV_MODE=existing`, session TTL,
  upload quota).
- README: one-paragraph "deploy in Portainer → web editor paste / deploy from
  repo," access on `:4848`.
- No source. This is the only public surface.

### 3.3 Multi-user: per-session sandbox (reset on idle)  ⟵ *decided*
The biggest code change. Today `mockData` / `walletState` / `MOCK_FILES` are
**global singletons** → visitors corrupt each other's view.
- Issue a `demo-session` cookie on first hit (the mock already sets a session on
  login; extend it to anonymous visitors).
- Key state by session id: `sessions[sid] = { mockData, walletState, files }`,
  each **deep-cloned from a pristine seed** on creation.
- Reap on idle (e.g. 30 min no activity) + hard cap concurrent sessions; on reap,
  free memory + temp dir.
- RPC dispatch + WS patches resolve the per-session state instead of the global.
- Keeps the demo a true playground: install/uninstall/spend freely, reset by
  reconnecting.

### 3.4 File storage: persisted per session  ⟵ *decided*
Today filebrowser upload/delete/rename are 200-OK no-ops.
- Back each session with a temp dir (e.g. `/tmp/demo/<sid>/`), seeded from
  `MOCK_FILES`.
- Make `POST/DELETE/PATCH /app/filebrowser/api/resources/*` and `GET …/raw/*`
  read/write that dir. Enforce a per-session quota (e.g. 50 MB) and reject
  oversize/odd MIME.
- Cleaned when the session is reaped — no standing public writable volume, no real
  filebrowser container to harden.

### 3.5 Bitcoin: testnet-flavored mock  ⟵ *decided*
- Relabel wallet/chain as **testnet/signet**: `tb1q…` addresses, "testnet" chain
  in `bitcoin.getinfo`, scripted-but-plausible block height + confirmations.
- Keep `dev.faucet` as the in-UI "get test sats" button (instant, free).
- No real `bitcoind` → no sync, no disk, no public RPC attack surface.
- *Future upgrade path:* swap to a real signet node + LND in the stack if we ever
  want movable real test sats (out of scope now).

### 3.6 Mock containers / app lifecycle
- The mock already simulates `package.install/uninstall/start/stop/restart`
  asynchronously. For the demo, **force simulation mode** (never touch a real
  Docker socket — rootless/safe and host-independent). Confirm no path in
  `mock-backend.js` reaches for a real runtime when `DEMO=1`.

### 3.7 Mock-data refresh
- Update `mockData` static apps + marketplace to current app set/versions, refresh
  wallet figures, seeded mesh messages, and files so the demo feels current. This
  is ongoing and rides the same image pipeline.

---

## 4. Invariants / guardrails (public exposure)

- **No real secrets, no real backend, no real Docker socket** in the demo image or
  public repo. Mock password stays a known demo credential, clearly labeled.
- **Per-session isolation** is a hard requirement before going public — without it
  the demo is unusable for strangers.
- **Resource caps:** session count, per-session memory + upload quota, idle reap;
  the box can't be DoS'd into OOM by upload spam or session churn.
- **`ANTHROPIC_API_KEY`** (chat) is injected via Portainer env, never committed;
  rate-limit / budget-cap demo chat usage.
- **Read-only registry creds** for the Portainer host to pull `:demo`.

---

## 5. Files / seams

| Concern | Where |
|---------|-------|
| Per-session state, file persistence, testnet labels, sim-mode | `neode-ui/mock-backend.js` |
| Build contexts (reused as-is) | `neode-ui/Dockerfile.backend`, `neode-ui/Dockerfile.web`, `neode-ui/docker/nginx-demo.conf` |
| Demo stack (in-repo, dev) | `docker-compose.demo.yml` (keep `build:`) |
| Public stack (new repo) | `archy-demo/docker-compose.yml` (`image:` refs), `.env.example`, README |
| CI pipeline | new workflow (path filter `neode-ui/**` → build + push `:demo` → Portainer webhook) |

---

## 6. Open questions

1. **Demo host** — which Portainer instance (OVH `.168`? a dedicated VPS)? Public
   DNS + TLS for `demo.<domain>`?
2. **Registry for `:demo` images** — `source.archipelago-foundation.org` vs vps2; public-pull or
   creds baked into Portainer?
3. **Session TTL + concurrency cap** — concrete numbers (30 min / N sessions / 50 MB)?
4. **Chat in the demo** — enable Claude chat (needs key + budget cap) or stub it?
5. **Sync cadence** — rebuild `:demo` on every `neode-ui/**` push, or nightly?
