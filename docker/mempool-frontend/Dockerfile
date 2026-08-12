# Archipelago mempool frontend — adds a resilient nginx backend proxy.
#
# The only delta vs the upstream image is /patch/entrypoint.sh, which rewrites
# the generated nginx-mempool.conf to use `resolver` + a variable proxy_pass so
# the frontend re-resolves the backend (mempool-api) via DNS on every request.
# Without this, nginx pins the backend IP at startup and serves 502 / "offline"
# after any backend restart (podman reassigns the IP). See the script header.
ARG BASE=source.archipelago-foundation.org/lfg2025/mempool-frontend:v3.0.0
FROM ${BASE}

# --chmod keeps the exec bit (build runs as USER 1000, plain COPY lands root:0644
# → "not executable"). Base USER/ENTRYPOINT/CMD (1000 / /patch/entrypoint.sh /
# nginx -g "daemon off;") are inherited unchanged.
COPY --chmod=0755 entrypoint.sh /patch/entrypoint.sh
