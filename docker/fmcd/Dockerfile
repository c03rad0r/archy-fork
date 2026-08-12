# fmcd (Fedimint client daemon) runtime image.
#
# The fmcd binary is built from source (github.com/minmoto/fmcd v0.8.0,
# fedimint-client 0.8.2 — iroh-capable) with Rust 1.86.0, then copied in here.
# Base must match the build host's glibc (Debian trixie / glibc 2.41).
# Binary is dynamically linked against libstdc++ (statically-bundled rocksdb)
# and uses rustls (no openssl). Build context must contain the `fmcd` binary.
FROM debian:trixie-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

COPY fmcd /usr/local/bin/fmcd
COPY fmcd-run /usr/local/bin/fmcd-run
RUN chmod +x /usr/local/bin/fmcd /usr/local/bin/fmcd-run

EXPOSE 8080
# Resilient launcher (retries on join failure instead of crash-looping).
# All config is read from FMCD_* env vars.
ENTRYPOINT ["fmcd-run"]
