# Bitcoin Core — minimal rootless image built from the OFFICIAL upstream release.
#
# The CANONICAL, verified build path is scripts/build-bitcoin-image.sh, which
# downloads the upstream tarball, verifies SHA-256 + the OpenPGP signature
# (fail-closed), and tags/pushes <registry>/bitcoin:<version>. This Dockerfile
# mirrors that image for a manual/local build and replaces the old stale
# community base (`FROM bitcoin/bitcoin:24.0`).
#
# Build (binaries must be pre-fetched + verified into ./bin — see the script):
#   scripts/build-bitcoin-image.sh core 31.0
FROM debian:bookworm-slim
ARG BITCOIN_VERSION=31.0
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates; \
    rm -rf /var/lib/apt/lists/*; \
    useradd -m -u 1000 -s /bin/bash bitcoin; \
    mkdir -p /home/bitcoin/.bitcoin; \
    chown -R bitcoin:bitcoin /home/bitcoin
# bin/ holds the SHA-256 + GPG-verified bitcoind / bitcoin-cli (Guix-built,
# x86_64-linux-gnu) extracted from the official release tarball.
COPY bin/bitcoind /usr/local/bin/bitcoind
COPY bin/bitcoin-cli /usr/local/bin/bitcoin-cli
RUN chmod 0755 /usr/local/bin/bitcoind /usr/local/bin/bitcoin-cli
# Run as (container) root, like the legacy hand-built :latest image. Rootless
# Podman maps container-root to the unprivileged host service user; the manifest
# grants CAP_DAC_OVERRIDE so bitcoind can read its data dir, which the
# orchestrator chowns to the data_uid (host 100101 / container uid 102), not to
# this image's `bitcoin` user. A non-root USER can't read existing chain data and
# bitcoind crash-loops with "Error initializing block database".
WORKDIR /home/bitcoin
VOLUME ["/home/bitcoin/.bitcoin"]
EXPOSE 8332 8333
ENTRYPOINT ["bitcoind"]
