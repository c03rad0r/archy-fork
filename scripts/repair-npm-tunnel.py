#!/usr/bin/env python3
"""Migrate the known NPM/LND tunnel collision, without touching wallet services.

Runs as the rootless app owner before orchestrator startup. Only the narrow
legacy web-tunnel profile is accepted. Unknown custom routing fails closed.
"""
import ipaddress
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import tempfile


def command(*args, input=None):
    result = subprocess.run(args, input=input, text=True, capture_output=True, timeout=45)
    if result.returncode:
        # Commands may read private files. Never print their captured output.
        raise RuntimeError(f'{args[0]} operation failed (exit {result.returncode})')
    return result.stdout


def plan(drop, rules):
    """Return a conservative migration, or None for absent/already fixed mapping."""
    matches = re.findall(r'^PublishPort=([0-9.]+):18080:80/tcp$', drop, re.M)
    if not matches:
        return None
    if len(matches) != 1:
        raise ValueError('ambiguous NPM tunnel mapping')
    destination = str(ipaddress.IPv4Address(matches[0]))
    peer_match = re.search(r'ip saddr ([0-9.]+) ip daddr ' + re.escape(destination)
                           + r' tcp dport \{ 18080, 18443 \} accept', rules)
    if not peer_match:
        raise ValueError('unrecognized NPM tunnel firewall; manual review required')
    peer = str(ipaddress.IPv4Address(peer_match[1]))
    # Match the entire old profile, not just a substring in an arbitrary firewall.
    old = f'''table inet web_tunnel {{
    chain input {{
        type filter hook input priority -10; policy accept;
        iifname != "wg-web" return
        ct state established,related accept
        ip saddr {peer} icmp type echo-request accept
        ip saddr {peer} ip daddr {destination} tcp dport {{ 18080, 18443 }} accept
        counter drop
    }}
    chain forward {{
        type filter hook forward priority -10; policy accept;
        iifname "wg-web" counter drop
        oifname "wg-web" counter drop
    }}
}}'''
    if rules.split() != old.split():
        raise ValueError('custom NPM tunnel firewall differs; manual review required')
    if 'PublishPort='+destination+':18081:' in drop:
        raise ValueError('replacement port already configured')
    new_rules = rules.replace('table inet web_tunnel {', f'''table inet web_tunnel {{
    # Preserve incoming HTTP while keeping LND REST's port free.
    chain prerouting {{
        type nat hook prerouting priority dstnat; policy accept;
        iifname "wg-web" ip saddr {peer} ip daddr {destination} tcp dport 18080 redirect to :18081
    }}''', 1).replace('tcp dport { 18080, 18443 } accept',
                     'tcp dport { 18081, 18443 } accept')
    return (drop.replace(f'PublishPort={destination}:18080:80/tcp',
                         f'PublishPort={destination}:18081:80/tcp'), new_rules, destination)


def atomic_user(path, content):
    with tempfile.NamedTemporaryFile(mode='w', dir=path.parent, delete=False) as f:
        tmp = Path(f.name)
        os.fchmod(f.fileno(), 0o600)
        f.write(content)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


def root_write(path, content):
    # Stage next to the destination; rename makes the config update atomic.
    staged = str(path)+'.archy-npm-migration'
    command('sudo', '-n', 'tee', staged, input=content)
    command('sudo', '-n', 'chmod', '600', staged)
    command('sudo', '-n', 'mv', '--', staged, str(path))


def main():
    drop = Path.home()/'.config/containers/systemd/nginx-proxy-manager.container.d/web-tunnel.conf'
    rules_path = Path('/etc/wireguard/wg-web.nft')
    state = Path.home()/'.local/state/archipelago/npm-tunnel-migration'
    journal = state/'pending.json'
    recovered_active = None
    # Interrupted migrations are completed/rolled back before normal startup.
    if journal.exists():
        saved = json.loads(journal.read_text())
        command('systemctl', '--user', 'stop', 'nginx-proxy-manager.service')
        atomic_user(drop, saved['drop'])
        root_write(rules_path, saved['rules'])
        command('sudo', '-n', 'nft', '-f', '-', input='delete table inet web_tunnel\n'+saved['rules'])
        command('systemctl', '--user', 'daemon-reload')
        # Do not restart the colliding old configuration before reapplying.
        recovered_active = saved.get('was_active')
        journal.unlink()
    if not drop.exists():
        return
    old_drop = drop.read_text()
    if not re.search(r'^PublishPort=[0-9.]+:18080:80/tcp$', old_drop, re.M):
        return
    old_rules = command('sudo', '-n', 'cat', str(rules_path))
    new_drop, new_rules, destination = plan(old_drop, old_rules)
    # A free, assigned replacement is required; do not guess another port.
    with socket.socket() as probe:
        probe.bind((destination, 18081))
    # The route must be persistent and loaded by the tunnel's startup contract.
    wg = command('sudo', '-n', 'grep', '-E', r'^(PreUp|PostDown)\s*=', '/etc/wireguard/wg-web.conf')
    if 'PreUp = nft -f /etc/wireguard/wg-web.nft' not in wg or 'PostDown = nft delete table inet web_tunnel' not in wg:
        raise ValueError('unrecognized tunnel lifecycle; manual review required')
    active = command('sudo', '-n', 'nft', 'list', 'table', 'inet', 'web_tunnel')
    # Reject live-only rule changes instead of silently discarding them. nft
    # canonicalizes priority names and adds counter values when listing rules.
    def normalized(text):
        text = re.sub(r'counter packets \d+ bytes \d+', 'counter', text)
        return text.replace('priority filter - 10', 'priority -10').split()
    if normalized(active) != normalized(old_rules):
        raise ValueError('live tunnel rules differ from persistent config; review required')
    transaction = 'delete table inet web_tunnel\n'+new_rules
    command('sudo', '-n', 'nft', '--check', '-f', '-', input=transaction)
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(state, 0o700)
    was_active = recovered_active or command('systemctl', '--user', 'show', 'nginx-proxy-manager.service', '--property=ActiveState', '--value').strip()
    saved = json.dumps({'drop': old_drop, 'rules': old_rules, 'was_active': was_active})
    atomic_user(state/'before.json', saved)
    atomic_user(journal, saved)
    try:
        command('systemctl', '--user', 'stop', 'nginx-proxy-manager.service')
        atomic_user(drop, new_drop)
        root_write(rules_path, new_rules)
        command('sudo', '-n', 'nft', '-f', '-', input=transaction)
        command('systemctl', '--user', 'daemon-reload')
        if was_active in ('active', 'activating', 'reloading', 'failed'):
            command('systemctl', '--user', 'restart', 'nginx-proxy-manager.service')
        journal.unlink()
    except Exception:
        atomic_user(drop, old_drop)
        root_write(rules_path, old_rules)
        command('sudo', '-n', 'nft', '-f', '-', input='delete table inet web_tunnel\n'+old_rules)
        command('systemctl', '--user', 'daemon-reload')
        # Keep the journal if rollback fails so the next startup retries it.
        journal.unlink()
        raise
    print('NPM tunnel port repaired; original configuration backed up; native services unchanged')


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        raise SystemExit('NPM tunnel migration requires attention: '+str(error)) from None
