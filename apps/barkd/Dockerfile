# barkd — Ark protocol wallet daemon (https://gitlab.com/ark-bitcoin/bark).
# No official upstream image exists (their GitLab registry is empty), so we
# package the pinned, checksum-verified release binary ourselves and push to
# the node registry — same approach as fmcd. Keep the version in lockstep with
# the REST shapes coded in core/archipelago/src/wallet/ark_client.rs (0.3.0).
FROM debian:bookworm-slim

ARG BARKD_VERSION=0.3.0
ARG BARKD_SHA256=8562fa27386bae666ed62fa95c92d40f7bdb20d22525f75799adfc16adaaedb3

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl && \
    curl -fsSL "https://gitlab.com/api/v4/projects/ark-bitcoin%2Fbark/packages/generic/release-assets/bark-${BARKD_VERSION}/barkd-${BARKD_VERSION}-linux-x86_64" \
        -o /usr/local/bin/barkd && \
    echo "${BARKD_SHA256}  /usr/local/bin/barkd" | sha256sum -c - && \
    chmod a+x /usr/local/bin/barkd && \
    apt-get purge -y curl && apt-get autoremove -y && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

COPY entrypoint.sh /entrypoint.sh
RUN chmod a+x /entrypoint.sh

# The wallet itself is created over REST by the node's Ark bridge
# (wallet.ark-* RPCs) — the container just runs the daemon.
ENV BARKD_DATADIR=/data \
    BARKD_BIND_HOST=0.0.0.0 \
    BARKD_BIND_PORT=3535

EXPOSE 3535
VOLUME /data

ENTRYPOINT ["/entrypoint.sh"]
