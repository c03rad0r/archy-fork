# Multi-stage Dockerfile for Indeehub Frontend (Next.js)
# Build: podman build -t localhost/indeedhub:latest -f apps/indeedhub/Dockerfile /path/to/indeehub-frontend
# Run:   podman run -d --name indeedhub -p 8190:3000 localhost/indeedhub:latest

# ── Stage 1: Dependencies ──
FROM node:20-alpine AS deps
WORKDIR /app
COPY package.json package-lock.json* ./
RUN npm ci --ignore-scripts

# ── Stage 2: Build ──
FROM node:20-alpine AS builder
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY . .

# Inject standalone output mode for containerized deployment
RUN sed -i 's/reactStrictMode: true,/reactStrictMode: true, output: "standalone",/' next.config.js

# Build-time environment — connects to Indeehub production services
ENV NEXT_PUBLIC_APP_ENVIRONMENT=production
ENV NEXT_PUBLIC_APP_URL=http://localhost:8190
ENV NEXT_PUBLIC_API_URL=https://staging-api.indeehub.studio
ENV NEXT_PUBLIC_S3_PRIVATE_BUCKET=indeehub-private
ENV NEXT_PUBLIC_S3_PUBLIC_BUCKET=indeehub-public
ENV NEXT_PUBLIC_ENABLE_APPROVAL_FLOW=false
ENV NEXT_TELEMETRY_DISABLED=1

# Remove shaka-player .d.ts files that break the build (per package.json build script)
RUN rm -f ./node_modules/shaka-player/dist/*.d.ts

# Patch: replace home page with error-resilient version that doesn't crash
# when the Webflow landing page URL is unreachable from the container.
RUN printf '%s\n' \
  "import axios from 'axios';" \
  "import { HomeClient } from './page.client';" \
  "" \
  "export const dynamic = 'force-dynamic';" \
  "" \
  "export default async function Home() {" \
  "  try {" \
  "    const response = await axios('https://indeehub-30479a.webflow.io/', { timeout: 8000 });" \
  "    if (response.status !== 200) throw new Error('Bad status');" \
  "    const html = String(response.data)" \
  "      .replace('https://cdn.prod.website-files.com/img/favicon.ico', '/favicon.ico')" \
  "      .replace('https://cdn.prod.website-files.com/img/webclip.png', '/favicon.ico');" \
  "    return <HomeClient html={html} />;" \
  "  } catch {" \
  "    return <HomeClient html=\"<html><head><title>IndeeHub</title></head><body style='background:#000;color:#fff;font-family:sans-serif;display:flex;justify-content:center;align-items:center;height:100vh;margin:0'><div style='text-align:center'><h1>IndeeHub</h1><p>Loading content...</p><script>setTimeout(function(){location.reload()},5000)</script></div></body></html>\" />;" \
  "  }" \
  "}" > src/app/page.tsx

RUN npm run build

# ── Stage 3: Runner ──
FROM node:20-alpine AS runner
WORKDIR /app

ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1
ENV PORT=3000
ENV HOSTNAME=0.0.0.0

RUN addgroup --system --gid 1001 nodejs && \
    adduser --system --uid 1001 nextjs

# Copy standalone build output
COPY --from=builder /app/public ./public
COPY --from=builder --chown=nextjs:nodejs /app/.next/standalone ./
COPY --from=builder --chown=nextjs:nodejs /app/.next/static ./.next/static

USER nextjs
EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=10s --retries=3 --start-period=40s \
  CMD wget --no-verbose --tries=1 --spider http://localhost:3000/ || exit 1

CMD ["node", "server.js"]
