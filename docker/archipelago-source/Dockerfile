FROM docker.io/library/node:24-alpine AS build

# GitWorkshop has no release artifacts, so pin the audited source revision.
# The fetch verifies that the exact requested commit was checked out before any
# dependency or build command runs.
ARG GITWORKSHOP_COMMIT=dc36db64f6a2cca29d109829eabaf0a49d4bf4da
RUN apk add --no-cache git
WORKDIR /src
RUN git init \
    && git remote add origin https://github.com/DanConwayDev/gitworkshop.git \
    && git fetch --depth=1 origin "${GITWORKSHOP_COMMIT}" \
    && git checkout --detach FETCH_HEAD \
    && test "$(git rev-parse HEAD)" = "${GITWORKSHOP_COMMIT}"

COPY gitworkshop-archipelago.patch /tmp/gitworkshop-archipelago.patch
RUN git apply --check /tmp/gitworkshop-archipelago.patch \
    && git apply /tmp/gitworkshop-archipelago.patch
COPY gitworkshop-dependencies.patch /tmp/gitworkshop-dependencies.patch
RUN git apply --check /tmp/gitworkshop-dependencies.patch \
    && git apply /tmp/gitworkshop-dependencies.patch
RUN npm ci \
    && npm audit --audit-level=moderate
RUN APP_BASE_PATH=/app/archipelago-source/ \
    APP_RELEASE_VERSION="archipelago-${GITWORKSHOP_COMMIT}" \
    npm run build

FROM docker.io/library/nginx:1.27.4-alpine

COPY --from=build /src/dist/ /usr/share/nginx/html/
COPY nginx-main.conf /etc/nginx/nginx.conf
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY UPSTREAM.md /usr/share/doc/archipelago-source/UPSTREAM.md

# Run both nginx master and workers as the packaged unprivileged user. Writable
# runtime paths live on the manifest's small mode-1777 `/tmp` tmpfs, so the
# container needs neither Linux capabilities nor a writable root filesystem.
EXPOSE 8337
ENTRYPOINT []
USER nginx
CMD ["nginx", "-g", "daemon off;"]
