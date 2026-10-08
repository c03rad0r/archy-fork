FROM docker.io/denoland/deno:debian-2.9.7
WORKDIR /app
# Upstream MIT source is pinned independently from the application version.
ADD https://codeload.github.com/hzrd149/blossom-server/tar.gz/a492dc61c4a581bbd0992546b2aec6f9aa543f75 /tmp/upstream.tar.gz
RUN echo 'd4f4ab9cbbf1b6d72d8cdb68fbb0b7b55dbe0c7e4a7414819f5c96dafd0af3fd  /tmp/upstream.tar.gz' | sha256sum -c - && tar -xzf /tmp/upstream.tar.gz --strip-components=1 -C /app && rm /tmp/upstream.tar.gz
COPY patch.ts startup.ts ./
COPY ui/ ./archy-ui/
RUN deno run --allow-read=/app --allow-write=/app patch.ts && deno cache --frozen main.ts startup.ts && deno bundle --no-config --platform browser archy-ui/app.ts -o archy-ui/app.js
# Fetch native dependencies at build time, not on a user's first upload.
RUN deno eval 'await import("@libsql/client"); await import("sharp")'
ENV BLOSSOM_REQUIRE_CONFIG=1
EXPOSE 3000
ENTRYPOINT ["deno", "run", "--cached-only", "--frozen", "-A", "--deny-run", "/app/startup.ts"]
