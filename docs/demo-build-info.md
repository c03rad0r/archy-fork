# Archipelago Public Demo — build info & status

**Status:** implemented & deployable (2026-07-14)
**Branch:** `main` — the demo machinery was merged from the old `demo-build`
branch and now lives on main, pushed to
`gitea-vps2` = `https://source.archipelago-foundation.org/lfg2025/archy.git`.

A public, click-to-play demo of the Archipelago UI, 100% mock-data driven,
multi-visitor, deployed via Portainer. See also `docs/archive/demo-deployment-design.md`
(original design) and `demo-deploy/` (thin prebuilt-image stack).

---

## Deploy (Portainer)

Build-from-repo (works today, no registry needed):

| Field | Value |
|-------|-------|
| Repository URL | `https://source.archipelago-foundation.org/lfg2025/archy.git` |
| Reference | `refs/heads/main` |
| Compose path | `docker-compose.demo.yml` |
| Auth | user `lfg2025`, password = Gitea token |
| UI port | **2100** · Login password: **`entertoexit`** |

Redeploy after each push. `docker-compose.demo.yml` builds two images
(`neode-ui/Dockerfile.backend` = mock server, `neode-ui/Dockerfile.web` = nginx+UI).
The thin `demo-deploy/docker-compose.yml` pulls prebuilt `:demo` images instead
(needs the CI image pipeline / registry wired — `.github/workflows/demo-images.yml`).

### Flags / env
- Backend: `DEMO=1` (compose sets it) → multi-session sandbox, no real runtime.
- Web build: `VITE_DEMO=1` (Dockerfile.web ARG, default 1) → inlined demo UI behaviour.
- Optional: `ANTHROPIC_API_KEY` (NOT needed — AIUI chat is canned in demo),
  `DEMO_SESSION_TTL_MS` (45m), `DEMO_MAX_SESSIONS` (500), `DEMO_FILE_QUOTA_BYTES` (50MB).

---

## Architecture

Everything is gated behind `DEMO` (off = classic single-user dev mock, unchanged).

- **`neode-ui/mock-backend.js`** — the entire fake backend (Node/Express, ~95+ RPCs).
  - **Per-session isolation:** `AsyncLocalStorage` + Proxy. Globals (`mockData`,
    `walletState`, `userState`, `mockState`, `bitcoinRelayMockState`) are Proxies
    that resolve to the current request's store, keyed by a `demo_sid` cookie.
    Deep-cloned from `SEED_*` on first hit; idle-reaped; per-session WS fan-out.
  - **Files:** per-session in-memory store + curated disk files (see below).
  - Forces simulation mode in DEMO (`docker=null`).
- **`neode-ui/src/composables/useDemoIntro.ts`** — the frontend demo switch
  (`IS_DEMO`), per-day intro gate, `DEMO_PASSWORD`, app demoability + launch URLs.
- **`neode-ui/docker/nginx-demo.conf`** — routes `/rpc`, `/ws`, `/app/*`,
  `/electrs-status`, `/proxy/`, `/lnd-connect-info`, the IndeeHub/Mempool
  reverse-proxies, and the SPA.
- **`docker/{bitcoin-ui,electrs-ui,lnd-ui,fedimint-ui}/`** — the REAL registry app
  UIs, served statically under `/app/<id>/` with mocked data endpoints.
- **`demo/aiui/`** — prebuilt AIUI dist (chat is canned; `?mockArchy&seed`).
- **`demo/files/`** — curated cloud files drop-in (see below).

## Demo features (all implemented)
Per-session sandbox · per-session file upload (Range streaming) · testnet/signet
flavor · per-day intro replay · `entertoexit` login (prefilled + hint) · version
`<real>-demo` · onboarding wizard skipped (intro kept) · "No demo" install gating ·
real app UIs (Bitcoin Core vs Knots by subversion, ElectrumX, LND, Fedimint;
Mempool/IndeeHub iframed) · 12 federation nodes / 5 peers · FIPS active · interactive
buy flow (testnet addresses, bolt11, 2s QR) · real testnet tx links (mempool.space) ·
networking profits 5,231,978 sats + labelled wallet txs · VPN · Nostr relays ·
node-visibility toggle · dummy Cashu mints + Fedimint federations · AIUI canned
reply + `?mockArchy` mock data + `?seed` pre-loaded "Content Showcase" chat.

---

## Curated cloud files (`demo/files/`)
Drop real files into `demo/files/<Folder>/<file>` and commit — they become the
cloud content for every visitor (read-only; git access = the "private login").
Loader **merges per top-level folder**: adding `Music/` swaps only Music and keeps
the sample Documents/Photos/Videos. Empty → built-in seeds. Text inlined; binaries
streamed from disk with HTTP Range (seek). Backend reads `/demo/files` —
**Dockerfile.backend COPYs it; `.dockerignore` must allow it.**

---

## Gotchas (READ before editing)
- **Sibling dirs need both the Dockerfile COPY and a `.dockerignore` allow.**
  `docker/bitcoin-ui`, `docker/electrs-ui`, `docker/lnd-ui`, `docker/fedimint-ui`,
  `demo/files` are outside `neode-ui/`; they're copied into the backend image and
  un-ignored in `.dockerignore` (`* ` + `!docker/` + `docker/*` + `!docker/<ui>/`).
  Forgetting either → Portainer build "not found" or runtime 500/404.
- **Real app UIs assume root-serving** — served via `express.static('/app/<id>')`
  + `/app/<id>/assets/*` → `/assets/*` redirect + per-path data endpoints
  (`bitcoin-status`, `rpc/v1`, `bitcoin-rpc/`, `/proxy/lnd/*`, `/electrs-status`).
- **Uploaded-via-UI files are ephemeral** (per-session, lost on redeploy/reap).
  Only `demo/files/` persists.
- **Mempool iframe is best-effort** (third-party CSP/websockets). **IndeeHub** is
  reverse-proxied with header-strip + `sub_filter` asset rewrite; if still black,
  it's indee's own `X-Frame-Options` (fix on that server).
- **AIUI `?seed` bootstrap hardcodes the current AIUI bundle hash**
  (`/aiui/assets/seedPrompts-CLWaUv28.js`) — re-paste if AIUI is rebuilt. Tiny
  first-load IndexedDB race (one refresh shows the chat).
- **Running mock-backend.js locally in the sandbox is flaky:** start backgrounded,
  `sleep 5+`, then curl; NEVER `pkill -f mock-backend` (it matches & kills the
  shell) — use `pkill -x node`.
- **Delete-405** seen pre-redeploy was nginx/stale; backend DELETE returns 200.

---

## Commit trail (demo-build, newest last)
`2715f2d8` sandbox → … → `7efebb4a` media merge + AIUI seed. ~14 commits, all
`feat(demo)/fix(demo)`.
