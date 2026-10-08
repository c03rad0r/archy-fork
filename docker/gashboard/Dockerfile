# TODO(security): pin this base by SHA256 before shipping to production.
# Resolve with:
#   docker pull node:22.12.0-alpine
#   docker inspect --format='{{index .RepoDigests 0}}' node:22.12.0-alpine
# then replace `node:22.12.0-alpine` below with `node@sha256:<digest>`.
ARG NODE_IMAGE=node:22.12.0-alpine@sha256:51eff88af6dff26f59316b6e356188ffa2c422bd3c3b76f2556a2e7e89d080bd

FROM ${NODE_IMAGE} AS deps
WORKDIR /app
# Avoid Corepack — Node 22 ships a Corepack that strict-validates pnpm
# signatures and breaks behind builders that can't reach the signing host.
RUN npm install -g pnpm@9.12.3 --no-fund --no-audit && npm cache clean --force
COPY pnpm-workspace.yaml package.json pnpm-lock.yaml tsconfig.base.json ./
COPY apps/api/package.json apps/api/
COPY apps/web/package.json apps/web/
RUN pnpm install --frozen-lockfile

FROM deps AS build-api
WORKDIR /app
COPY apps/api apps/api
RUN pnpm --filter @gashboard/api build

FROM deps AS build-web
WORKDIR /app
COPY apps/web apps/web
RUN pnpm --filter @gashboard/web build

FROM ${NODE_IMAGE} AS runtime
WORKDIR /app
ENV NODE_ENV=production
RUN apk add --no-cache wget tini \
 && npm install -g pnpm@9.12.3 --no-fund --no-audit \
 && npm cache clean --force

COPY pnpm-workspace.yaml package.json pnpm-lock.yaml ./
COPY apps/api/package.json apps/api/
RUN pnpm install --filter @gashboard/api --prod --frozen-lockfile

COPY --from=build-api /app/apps/api/dist apps/api/dist
COPY --from=build-web /app/apps/web/dist apps/web/dist
COPY nostr-provider.js /app/nostr-provider.js

USER node
EXPOSE 1337
ENTRYPOINT ["/sbin/tini", "--"]
CMD ["node", "apps/api/dist/index.js"]
