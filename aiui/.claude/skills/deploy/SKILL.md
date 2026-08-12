---
name: deploy
description: Build and prepare AIUI for deployment to Archy node
allowed-tools: Bash(*), Read, Edit, Glob, Grep
---

Build AIUI for production deployment. Steps:

1. **Pre-flight checks**:
   - `pnpm typecheck` — must pass
   - `pnpm lint` — must pass
   - `pnpm --filter @aiui/app test -- --run` — report failures but continue

2. **Build**:
   - `pnpm build`
   - Verify `packages/app/dist/` exists and contains `index.html`

3. **Bundle analysis**:
   - Report total dist size and gzip estimate
   - List the 5 largest chunks
   - Check against 250KB gzipped budget (warn if over)

4. **Verify nginx config**:
   - Read `packages/app/server/nginx-archy.conf`
   - Verify SPA routing (`try_files $uri $uri/ /aiui/index.html`)
   - Verify proxy paths for Claude API

5. **Container build** (if Dockerfile exists):
   - `podman build -t aiui:latest packages/app/`
   - Report image size

6. **Report**: Build status, bundle size, any warnings.
