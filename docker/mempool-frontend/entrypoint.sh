#!/bin/sh
set -eu
# Preserve the pinned upstream entrypoint (including new runtime options).
# Apply our DNS repair only after it has rendered the nginx configuration.
exec /patch/upstream-entrypoint.sh /patch/start-nginx.sh "$@"
