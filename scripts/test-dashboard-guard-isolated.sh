#!/usr/bin/env bash
# Exercise actual nginx without using the node's network or writable configuration.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
sudo -n true
sudo -n systemd-run --unit="archy-guard-test-$(date +%s)-$$" --wait --pipe --collect \
  --property="WorkingDirectory=$ROOT" \
  --property=PrivateNetwork=yes --property=PrivateTmp=yes \
  --property=ProtectSystem=strict --property=ProtectHome=read-only \
  --property=NoNewPrivileges=yes \
  --property='TemporaryFileSystem=/var/log/nginx:rw /var/lib/nginx:rw' \
  python3 "$ROOT/tests/regression/dashboard-public-guard-network.py"
