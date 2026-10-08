FROM debian:bookworm-slim@sha256:7c7b2c966bc9ee8cedfeef67e0e279108992c77681fa595db4a9d65c06ccc587 AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates git build-essential cmake pkg-config libjansson-dev \
    libmicrohttpd-dev libsodium-dev libcurl4-openssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
# DATUM v0.4.1beta. Verify the commit as well as the tag.
RUN git init && git remote add origin https://github.com/OCEAN-xyz/datum_gateway.git \
    && git fetch --depth 1 origin refs/tags/v0.4.1beta \
    && git checkout --detach FETCH_HEAD \
    && test "$(git rev-parse HEAD)" = 5b061233a3d3323771b2be98e17f543e59346619 \
    && cmake -DCMAKE_BUILD_TYPE=Release . && make -j2

FROM debian:bookworm-slim@sha256:7c7b2c966bc9ee8cedfeef67e0e279108992c77681fa595db4a9d65c06ccc587
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libjansson4 libmicrohttpd12 libsodium23 libcurl4 jq curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 1000 --create-home datum
WORKDIR /app
COPY --from=build /src/datum_gateway /app/datum_gateway
COPY --from=build /src/www /app/www
COPY entrypoint.sh /app/entrypoint.sh
COPY configure.jq /app/configure.jq
USER 1000:1000
EXPOSE 7152 23334
ENTRYPOINT ["sh", "/app/entrypoint.sh"]
