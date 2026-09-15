# Archipelago mempool frontend — adds a resilient nginx backend proxy.
#
# Keep the upstream startup logic; repair its rendered proxy configuration.
# Publish this derived image under an Archipelago-specific tag, never the
# upstream version tag that the registry mirror can overwrite.
ARG BASE=source.archipelago-foundation.org/lfg2025/mempool-frontend@sha256:d63498a109622475c913db4e3199d893f2440a451450e542923d2e55a38407a0
FROM ${BASE}

# --chmod keeps the exec bit (build runs as USER 1000, plain COPY lands root:0644
# → "not executable"). Base USER/ENTRYPOINT/CMD (1000 / /patch/entrypoint.sh /
# nginx -g "daemon off;") are inherited unchanged.
RUN cp /patch/entrypoint.sh /patch/upstream-entrypoint.sh
COPY --chmod=0755 entrypoint.sh start-nginx.sh repair-nginx.sh /patch/
