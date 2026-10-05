#!/usr/bin/env bash
# Requires separately staged binaries; never runs against real node wallets.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
BIN_DIR=${ARCHY_REGTEST_BIN_DIR:?Set the path to the disposable fixture binaries}
RESULT_DIR=${ARCHY_REGTEST_RESULT_DIR:?Set a fresh private result directory}
for binary in bitcoind bitcoin-cli lnd lncli archipelago; do test -x "$BIN_DIR/$binary"; done
install -d -m 700 "$RESULT_DIR"
test ! -e "$RESULT_DIR/result.json"
sudo -n systemd-run --unit="archy-fee-regtest-$(date +%s)-$$" --wait --pipe --collect \
  --property=PrivateNetwork=yes --property=PrivateTmp=yes --property=PrivateDevices=yes \
  --property=ProtectSystem=strict --property=ProtectHome=yes --property=NoNewPrivileges=yes \
  --property='TemporaryFileSystem=/run:rw /var/lib/archipelago:rw /var/lib/containers:rw /root:rw /etc/archipelago:rw /etc/nginx:rw /etc/systemd/system:rw /opt/archipelago:rw' \
  --property="BindReadOnlyPaths=$BIN_DIR:/opt/archy-regtest-bin $ROOT/tests/lifecycle/fee-bump-regtest.py:/opt/archy-fee-regtest.py" \
  --property="BindPaths=$RESULT_DIR:/opt/archy-regtest-results" \
  --setenv=ARCHY_FEE_REGTEST_ISOLATED=1 --setenv="ARCHY_HOST_NET_NS=$(readlink /proc/self/ns/net)" \
  /usr/bin/unshare --pid --fork --mount-proc --kill-child python3 -u /opt/archy-fee-regtest.py
