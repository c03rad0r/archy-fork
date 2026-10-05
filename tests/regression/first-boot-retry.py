#!/usr/bin/env python3
"""Execute unbundled first-boot retry with temporary paths and fake system commands."""
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
source = (root / 'scripts/first-boot-containers.sh').read_text()
# Exercise the real early-exit path, excluding the unrelated bundled installer.
source = source.split('TARGET_IP=$(hostname -I', 1)[0]
with tempfile.TemporaryDirectory(prefix='archy-firstboot-retry-') as directory:
    tmp = Path(directory)
    for original, replacement in [('/var/lib/archipelago', tmp / 'data'),
                                   ('/opt/archipelago', tmp / 'opt'),
                                   ('/home/archipelago', tmp / 'home'),
                                   ('/var/log', tmp / 'logs')]:
        source = source.replace(original, str(replacement))
    for folder in ['data/secrets', 'data/wireguard', 'opt', 'logs']:
        (tmp / folder).mkdir(parents=True, exist_ok=True)
    (tmp / 'opt/.unbundled').touch()
    for name in ['bitcoin-rpc-password', 'mempool-db-password',
                 'btcpay-db-password', 'mysql-root-db-password']:
        (tmp / 'data/secrets' / name).write_text('fixture-preserved')
    (tmp / 'data/wireguard/private.key').write_text('fixture-preserved')
    candidate = tmp / 'firstboot.sh'
    candidate.write_text(source)
    # Functions take precedence over host commands. Unexpected privileged work
    # fails, and no real Podman/systemd command can run through these stubs.
    harness = r'''
id() { if [ "$*" = '-u' ]; then echo 0; else echo 1000; fi; }
chown() { :; }
loginctl() { :; }
modprobe() { :; }
systemctl() { :; }
ufw() { echo 'Status: inactive'; }
openssl() { echo 'unexpected credential rotation' >&2; return 99; }
runuser() {
  printf '%s\n' "$*" >> "$TRACE"
  case "$*" in
    *'podman system migrate'*) return 99 ;;
    *'podman container exists filebrowser'*) return 0 ;;
    *'podman ps -a'*) echo fedimint-clientd; return 0 ;;
    *'podman network create archy-net'*) return 0 ;;
    *) echo 'unexpected system command' >&2; return 99 ;;
  esac
}
export -f id chown loginctl modprobe systemctl ufw openssl runuser
bash "$CANDIDATE"
'''
    before = {str(p): p.read_bytes() for p in (tmp / 'data').rglob('*') if p.is_file()}
    for attempt in range(2):
        trace = tmp / f'trace-{attempt}'
        result = subprocess.run(['bash', '-c', harness], capture_output=True,
                                text=True, timeout=15, env=os.environ | {
                                    'CANDIDATE': str(candidate), 'TRACE': str(trace),
                                    'ARCHY_REGISTRY': 'fixture.invalid',
                                    'BITCOIN_KNOTS_IMAGE': 'fixture.invalid/bitcoin',
                                })
        assert result.returncode == 0, result.stderr
        assert not result.stderr, result.stderr
        assert 'Unbundled first-boot complete' in result.stdout, result.stdout
        assert 'system migrate' not in trace.read_text(), trace.read_text()
        assert all(Path(p).read_bytes() == content for p, content in before.items())
    print('PASS repeated unbundled setup logs correctly without stopping apps or rotating stored secrets')
