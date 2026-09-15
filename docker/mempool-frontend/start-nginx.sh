#!/bin/sh
set -eu
/patch/repair-nginx.sh
nginx -t
exec "$@"
