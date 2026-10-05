#!/usr/bin/env python3
"""Resolve NPM's existing storage and bridge public requests through NPM itself.

Never move a database or reimplement NPM access lists/custom locations. The
host terminates public TLS, then forwards to NPM's loopback-only listeners.
"""
import argparse
import contextlib
import fcntl
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import pwd
import re
import shutil
import sqlite3
import subprocess
import tempfile
import time

BASE = Path('/var/lib/archipelago/nginx-proxy-manager')
STATE = Path('/var/lib/archipelago/npm-public-bridge')
OUTPUT = Path('/etc/nginx/conf.d/public-npm-proxy-hosts.conf')
def active_dashboard(nginx_root=Path('/etc/nginx')):
    enabled = nginx_root / 'sites-enabled/archipelago'
    selected = enabled if enabled.exists() or enabled.is_symlink() else nginx_root / 'sites-available/archipelago'
    return selected.resolve()


DASHBOARD = active_dashboard()


def run(args, **kwargs):
    result = subprocess.run(args, capture_output=True, timeout=45, **kwargs)
    if result.returncode:
        raise RuntimeError(f'{args[0]} failed (exit {result.returncode}); previous configuration retained')
    return result.stdout


def podman_args():
    if os.geteuid() == 0:
        account = pwd.getpwnam('archipelago')
        return ['sudo', '-n', '-u', account.pw_name, 'env',
                f'XDG_RUNTIME_DIR=/run/user/{account.pw_uid}', 'podman']
    return ['podman']


def inspect_runtime():
    command = podman_args()
    exists = subprocess.run(command + ['container', 'exists', 'nginx-proxy-manager'],
                            capture_output=True, timeout=20)
    if exists.returncode == 1:
        return None
    if exists.returncode:
        raise RuntimeError('Cannot inspect NPM runtime; refusing to guess its data directory')
    info = json.loads(run(command + ['inspect', 'nginx-proxy-manager']))
    if len(info) != 1:
        raise RuntimeError('Ambiguous NPM runtime')
    return info[0]


def checked_path(value):
    path = Path(value)
    if not path.is_absolute() or any(c in str(path) for c in '\r\n\x00'):
        raise ValueError('NPM mount must be an absolute path without control characters')
    return path


def resolve_paths(base=BASE, runtime=None):
    """Keep the active mount; without one, reuse the single existing database."""
    base = checked_path(base)
    remembered = {}
    layout_file = base / '.archy-storage.json'
    if layout_file.exists():
        saved = json.loads(layout_file.read_text())
        remembered = {name: checked_path(saved[name]) for name in ('data', 'certificates')}
    mounts = {}
    for mount in [] if runtime is None else runtime.get('Mounts', []):
        destination = mount.get('Destination')
        if destination not in ('/data', '/etc/letsencrypt'):
            continue
        if destination in mounts:
            raise ValueError('Duplicate NPM persistent mount; refusing ambiguous runtime configuration')
        mounts[destination] = checked_path(mount['Source'])
    if runtime is not None and set(mounts) != {'/data', '/etc/letsencrypt'}:
        raise ValueError('NPM must have explicit /data and /etc/letsencrypt mounts; review before recreation')
    candidates = {base, base / 'data'}
    if remembered:
        candidates.add(remembered['data'])
    if '/data' in mounts:
        candidates.add(mounts['/data'])
    databases = {p.resolve() for p in candidates if (p / 'database.sqlite').exists()}
    # A live, explicit mount (or our previously validated receipt after a
    # managed stop) identifies the database without guessing. Older installers
    # can leave an unused second database behind; preserve it, never merge it
    # or let its mere existence displace the database NPM actually uses.
    if len(databases) > 1 and '/data' not in mounts and not remembered:
        raise ValueError('Multiple NPM databases found; choose the active layout explicitly before upgrading')
    data = mounts.get('/data', remembered.get('data', next(iter(databases), base)))
    if databases and data.resolve() not in databases:
        raise ValueError('NPM active mount differs from the saved database; refusing an empty replacement')
    db = data / 'database.sqlite'
    if remembered and not db.exists():
        raise ValueError('Previously initialized NPM database is missing; refusing an empty replacement')
    if db.exists():
        # Read-only opening never creates an empty replacement database.
        con = sqlite3.connect(db.resolve().as_uri() + '?mode=ro', uri=True, timeout=5)
        try:
            if con.execute('PRAGMA quick_check').fetchone()[0] != 'ok':
                raise ValueError('NPM database integrity check failed')
            tables = {r[0] for r in con.execute("SELECT name FROM sqlite_master WHERE type='table'")}
            if not {'proxy_host', 'certificate'} <= tables:
                raise ValueError('NPM database schema is not initialized; retry after NPM startup')
        finally:
            con.close()
    certs = mounts.get('/etc/letsencrypt', remembered.get('certificates', base / 'letsencrypt'))
    return {'data': str(data), 'certificates': str(certs)}


def quote(value):
    value = str(value)
    if any(ord(c) < 32 for c in value):
        raise ValueError('Control character in nginx configuration value')
    return '"' + value.replace('\\', '\\\\').replace('"', '\\"').replace('$', '\\$') + '"'


def prepare_realip(paths):
    """Append one managed block through NPM's supported custom HTTP include.

    A read-only mount in /etc/nginx/conf.d breaks NPM's startup ownership pass.
    Preserve existing custom directives and a private copy before adding trust
    for rootlessport's single forwarding source on our legacy subnet.
    """
    data = Path(paths['data'])
    path = data / 'nginx/custom/http_top.conf'
    for candidate in [path, path.parent, path.parent.parent]:
        if candidate.is_symlink():
            raise ValueError('NPM custom configuration is a symlink; preserved for review')
    source = path.read_bytes() if path.exists() else b''
    begin = b'# BEGIN ARCHY HOST BRIDGE\n'
    end = b'# END ARCHY HOST BRIDGE\n'
    block = begin + b'set_real_ip_from 169.254.1.100;\n' + end
    if begin.strip() in source or end.strip() in source:
        if source.count(begin) != 1 or source.count(end) != 1 or block not in source:
            raise ValueError('NPM managed trust block has an operator override; preserved for review')
        return path
    if source:
        backups = data / '.archy-http-top-backups'
        backups.mkdir(exist_ok=True, mode=0o700)
        backup = backups / hashlib.sha256(source).hexdigest()
        if not backup.exists():
            atomic(backup, source, 0o600)
    content = source + (b'\n' if source and not source.endswith(b'\n') else b'') + block
    mode = path.stat().st_mode & 0o777 if path.exists() else 0o644
    atomic(path, content, mode)
    return path


def domains(value):
    names = json.loads(value)
    if not isinstance(names, list) or not names:
        raise ValueError('NPM host has no valid domain names')
    result = []
    for name in names:
        if not isinstance(name, str):
            raise ValueError('Invalid NPM domain name')
        name = name.lower().rstrip('.')
        plain = name[2:] if name.startswith('*.') else name
        if len(name) > 253 or not plain or any(
            not re.fullmatch(r'[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?', label)
            for label in plain.split('.')
        ):
            raise ValueError('Invalid NPM domain name; no nginx configuration was generated')
        result.append(name)
    return sorted(set(result))


def hosts(data):
    db = Path(data) / 'database.sqlite'
    con = sqlite3.connect(db.resolve().as_uri() + '?mode=ro', uri=True, timeout=5)
    con.row_factory = sqlite3.Row
    try:
        # Avoid reading credentials, access-list passwords or advanced snippets.
        tables = {r[0] for r in con.execute("SELECT name FROM sqlite_master WHERE type='table'")}
        rows = []
        for table in ('proxy_host', 'redirection_host', 'dead_host'):
            if table not in tables:
                continue
            # Table names come solely from this fixed allowlist, never DB data.
            rows.extend(dict(r) for r in con.execute(f'''
              SELECT p.id, p.domain_names, p.certificate_id, c.provider,
                     COALESCE(c.is_deleted, 0) AS certificate_deleted
              FROM {table} p LEFT JOIN certificate c ON c.id = p.certificate_id
              WHERE p.enabled = 1 AND p.is_deleted = 0 ORDER BY p.id
            '''))
        return rows
    finally:
        con.close()


def managed_tunnel_listener(binding, container_port):
    """Recognize the existing private WireGuard web ingress, never a LAN bind.

    This does not select the tunnel as an upstream: the host bridge still
    requires a separate loopback listener. NPM's admin port has no exception.
    """
    expected_port = {80: '18081', 443: '18443'}.get(container_port)
    if expected_port is None or str(binding.get('HostPort')) != expected_port:
        return False
    try:
        address = ipaddress.IPv4Address(binding.get('HostIp', ''))
        if not any(address in ipaddress.IPv4Network(network)
                   for network in ('10.0.0.0/8', '172.16.0.0/12', '192.168.0.0/16')):
            return False
        interfaces = json.loads(run(['ip', '-d', '-j', 'address', 'show', 'dev', 'wg-web']))
        return any(item.get('ifname') == 'wg-web' and
                   item.get('linkinfo', {}).get('info_kind') == 'wireguard' and
                   any(entry.get('family') == 'inet' and entry.get('local') == str(address)
                       for entry in item.get('addr_info', [])) for item in interfaces)
    except (ValueError, RuntimeError, OSError):
        return False


def local_port(runtime, container_port):
    bindings = runtime.get('NetworkSettings', {}).get('Ports', {}).get(f'{container_port}/tcp') or []
    # A loopback mapping alongside a wildcard mapping is still public exposure.
    if any(binding.get('HostIp') not in ('127.0.0.1', '::1') and
           not managed_tunnel_listener(binding, container_port) for binding in bindings):
        raise ValueError(f'NPM container port {container_port} has a non-loopback published listener')
    for binding in bindings:
        if binding.get('HostIp') in ('127.0.0.1', '::1'):
            port = int(binding['HostPort'])
            if 1 <= port <= 65535:
                host = '[::1]' if binding['HostIp'] == '::1' else '127.0.0.1'
                return f'{host}:{port}'
    raise ValueError(f'NPM container port {container_port} needs a loopback-only published listener')


def certificate_paths(paths, row):
    number = row['certificate_id']
    if not isinstance(number, int) or number < 0:
        raise ValueError('Invalid NPM certificate ID')
    if number == 0 or row['certificate_deleted']:
        return None
    parent = (Path(paths['certificates']) / 'live' if row['provider'] == 'letsencrypt'
              else Path(paths['data']) / 'custom_ssl') / f'npm-{number}'
    cert, key = parent / 'fullchain.pem', parent / 'privkey.pem'
    if not cert.is_file() or not key.is_file():
        raise ValueError(f'NPM certificate {number} files are missing; inspect certificate issuance')
    return cert, key


def render(rows, paths, http_address, https_address, acme_root, trust_file):
    """Only route through NPM; never bypass its authentication or custom routes."""
    for address in (http_address, https_address):
        host, port = address.rsplit(':', 1)
        if not ipaddress.ip_address(host.strip('[]')).is_loopback or not 1 <= int(port) <= 65535:
            raise ValueError('NPM upstream must be loopback')
    chunks = ['# Generated by npm-public-bridge.py; routes and access control remain owned by NPM.\n']
    fingerprints = []
    trust = Path('/etc/ssl/certs/ca-certificates.crt').read_bytes()
    seen = set()
    for row in rows:
        names = domains(row['domain_names'])
        if seen.intersection(names):
            raise ValueError('Duplicate public NPM domain; resolve conflicting hosts first')
        seen.update(names)
        cert = certificate_paths(paths, row)
        common = f'''
    location ^~ /.well-known/acme-challenge/ {{
        default_type text/plain;
        root {quote(acme_root)};
        try_files $uri =404;
    }}
'''
        def proxy(address, scheme):
            tls = '' if scheme == 'http' else f'''
        proxy_ssl_server_name on;
        proxy_ssl_name $host;
        # One NPM listener serves different certificates. A shared upstream
        # session cache can resume another hostname's session and fail SNI.
        proxy_ssl_session_reuse off;
        proxy_ssl_verify on;
        proxy_ssl_verify_depth 5;
        proxy_ssl_trusted_certificate {quote(trust_file)};'''
            return f'''
    location / {{
        proxy_pass {scheme}://{address};
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header X-Forwarded-Scheme $scheme;
        proxy_set_header X-Archipelago-Public-Ingress 1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $http_connection;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
        proxy_buffering off;
        proxy_request_buffering off;
        # NPM/app configuration owns upload limits; do not impose a second cap.
        client_max_body_size 0;{tls}
    }}
'''
        chunks.append(f'''server {{
    listen 80;
    listen [::]:80;
    server_name {' '.join(names)};
{common}{proxy(http_address, 'http')}
}}
''')
        if cert:
            chain, key = cert
            cert_bytes = chain.read_bytes()
            trust += b'\n' + cert_bytes
            fingerprints.append(hashlib.sha256(cert_bytes).hexdigest())
            # Including the key fingerprint detects replacement without logging keys.
            fingerprints.append(hashlib.sha256(key.read_bytes()).hexdigest())
            chunks.append(f'''server {{
    listen 443 ssl;
    listen [::]:443 ssl;
    server_name {' '.join(names)};
    ssl_certificate {quote(chain)};
    ssl_certificate_key {quote(key)};
{common}{proxy(https_address, 'https')}
}}
''')
    return ''.join(chunks).encode(), trust, fingerprints


def atomic(path, content, mode=0o600):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        os.fchmod(stream.fileno(), mode)
        stream.write(content)
        stream.flush()
        os.fsync(stream.fileno())
    try:
        os.replace(temporary, path)
        fd = os.open(path.parent, os.O_DIRECTORY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
    finally:
        temporary.unlink(missing_ok=True)


def recover_pending(state=STATE, command=subprocess.run):
    """Caller holds the nginx lock; recover BEFORE reading candidate input files."""
    journal = Path(state) / 'pending.json'
    if not journal.exists():
        return False
    saved = json.loads(journal.read_text())
    for entry in saved['files']:
        target = Path(entry['path'])
        if entry['backup'] is None:
            target.unlink(missing_ok=True)
        else:
            atomic(target, Path(entry['backup']).read_bytes(), entry['mode'])
    for args in (['nginx', '-t'], ['systemctl', 'reload', 'nginx']):
        result = command(args, capture_output=True, timeout=30)
        if result.returncode:
            raise RuntimeError('NPM bridge pending transaction recovery failed; journal retained')
    journal.unlink()
    return True


def apply_files(files, state=STATE, command=subprocess.run,
                lock_path=Path('/run/lock/archy-nginx-config.lock'), renewal_fingerprint='', locked=False,
                certificate_reload=None):
    """Commit managed files together, with durable rollback before nginx reload.

    The journal contains file paths and backups, never private key contents.
    A interrupted transaction is rolled back before attempting the next one.
    """
    state = Path(state)
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(state, 0o700)
    journal = state / 'pending.json'
    receipt = state / 'applied.json'

    def reload_nginx():
        for args in (['nginx', '-t'], ['systemctl', 'reload', 'nginx']):
            result = command(args, capture_output=True, timeout=30)
            if result.returncode:
                raise RuntimeError('NPM bridge nginx validation/reload failed')

    def restore(saved):
        for entry in saved['files']:
            target = Path(entry['path'])
            if entry['backup'] is None:
                target.unlink(missing_ok=True)
            else:
                atomic(target, Path(entry['backup']).read_bytes(), entry['mode'])
        reload_nginx()
        journal.unlink()

    with contextlib.nullcontext() if locked else Path(lock_path).open('a+b') as lock:
        if not locked:
            fcntl.flock(lock, fcntl.LOCK_EX)
        if journal.exists():
            restore(json.loads(journal.read_text()))
        current = {}
        if receipt.exists():
            current = json.loads(receipt.read_text())
        changed = [(Path(path), content, mode) for path, content, mode in files
                   if not Path(path).is_file() or Path(path).read_bytes() != content
                   or Path(path).stat().st_mode & 0o777 != mode]
        if not changed and current.get('renewal_fingerprint') == renewal_fingerprint:
            return False
        backup_dir = state / ('backup-' + str(time.time_ns()))
        backup_dir.mkdir(mode=0o700)
        entries = []
        for index, (target, _, _) in enumerate(changed):
            if target.is_symlink():
                raise ValueError('Managed nginx output is a symlink; review operator override before updating')
            backup = None
            mode = 0o600
            if target.exists():
                backup = backup_dir / str(index)
                mode = target.stat().st_mode & 0o777
                atomic(backup, target.read_bytes())
            entries.append({'path': str(target), 'backup': None if backup is None else str(backup), 'mode': mode})
        saved = {'files': entries}
        atomic(journal, json.dumps(saved).encode())
        try:
            for target, content, mode in changed:
                atomic(target, content, mode)
            if certificate_reload and current.get('renewal_fingerprint') != renewal_fingerprint:
                # Replacing a custom certificate through NPM's API can update
                # files without reloading its running TLS listener. Ensure both
                # TLS endpoints pick up the replacement, not just host nginx.
                certificate_reload()
            reload_nginx()
            atomic(receipt, json.dumps({'renewal_fingerprint': renewal_fingerprint}).encode())
            journal.unlink()
        except Exception as failure:
            try:
                restore(saved)
            except Exception as rollback:
                raise RuntimeError('NPM bridge failed; rollback incomplete, durable recovery journal retained') from rollback
            raise failure
        return True


def dashboard_acme_root(source, data):
    """Update only known managed ACME roots, without changing custom locations."""
    root = Path(data) / 'letsencrypt-acme-challenge'
    pattern = re.compile(r'(location\s+\^~\s+/\.well-known/acme-challenge/\s*\{)([^{}]*)(\})', re.S)
    count = 0
    def replace(found):
        nonlocal count
        body = found[2]
        existing = re.findall(r'\broot\s+([^;]+);', body)
        allowed = {str(BASE / 'letsencrypt-acme-challenge'),
                   str(BASE / 'data/letsencrypt-acme-challenge'), str(root)}
        if len(existing) != 1 or existing[0].strip().strip('"') not in allowed:
            raise ValueError('Custom dashboard ACME location requires review; configuration preserved')
        count += 1
        body = re.sub(r'\broot\s+[^;]+;', lambda _: 'root ' + quote(root) + ';', body)
        return found[1] + body + found[3]
    result = pattern.sub(replace, source)
    if count == 1:
        # Older shipped dashboard templates omitted HTTPS ACME entirely. A
        # public challenge exception must never fall through to the SPA there.
        # Recognize only our canonical default servers; preserve custom layouts.
        prefix = r'server\s*\{\s*(?:if\s*\(\$archy_management_denied\)\s*\{\s*return 404;\s*\}\s*)?'
        http = list(re.finditer(prefix + r'listen 80 default_server;', result))
        https = list(re.finditer(prefix + r'listen 443 ssl default_server;', result))
        challenge = list(pattern.finditer(result))
        if (len(http) == len(https) == 1
                and http[0].end() < challenge[0].start() < https[0].start()
                and result.count('/.well-known/acme-challenge/') == 1):
            at = https[0].end()
            location = ('\n\n    location ^~ /.well-known/acme-challenge/ {\n'
                        '        default_type text/plain;\n'
                        '        root ' + quote(root) + ';\n'
                        '        try_files $uri =404;\n    }')
            result = result[:at] + location + result[at:]
            count += 1
    if count != 2:
        raise ValueError('Expected HTTP and HTTPS dashboard ACME locations; refusing partial migration')
    return result


def legacy_angor_route(source, paths, relay=False):
    """Recognize only the exact temporary routes recorded in the live handoff.

    Operator edits must fail recognition, not be overwritten by migration.
    Domain and certificate IDs are extracted, then the entire configuration is
    compared to the known template; no deployment hostname is hardcoded.
    """
    marker = ('# Live repair: NPM relay host, certificate11. Retire through release migration.'
              if relay else '# Shorty repair 2026-10-01: NPM host 2, certificate 8.')
    if not source.startswith(marker + '\n'):
        return False
    names = re.findall(r'\bserver_name\s+([^;]+);', source)
    if len(names) != 2 or names[0] != names[1]:
        return False
    try:
        name = domains(json.dumps([names[0].strip()]))[0]
    except ValueError:
        return False
    certificate_id = 11 if relay else 8
    parent = Path(paths['certificates']) / 'live' / f'npm-{certificate_id}'
    extra = '''proxy_set_header Upgrade $http_upgrade;
               proxy_set_header Connection "upgrade";
               proxy_read_timeout 3600s;
               proxy_send_timeout 3600s;''' if relay else ''
    limit = '' if relay else 'client_max_body_size 4m;'
    expected = f'''
server {{
 listen 80; listen [::]:80; server_name {name};
 location ^~ /.well-known/acme-challenge/ {{
  default_type text/plain; root {Path(paths['data']) / 'letsencrypt-acme-challenge'}; try_files $uri =404;
 }}
 location / {{ return 301 https://$host$request_uri; }}
}}
server {{
 listen 443 ssl; listen [::]:443 ssl; server_name {name};
 ssl_certificate {parent / 'fullchain.pem'};
 ssl_certificate_key {parent / 'privkey.pem'};
 ssl_protocols TLSv1.2 TLSv1.3; {limit}
 location / {{
  proxy_pass http://127.0.0.1:{8091 if relay else 8998};
  proxy_http_version 1.1;
  proxy_set_header Host $host;
  proxy_set_header X-Real-IP $remote_addr;
  proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
  proxy_set_header X-Forwarded-Proto $scheme;
  {extra}
 }}
}}
'''
    def normalized(text):
        return ''.join(re.sub(r'#[^\n]*', '', text).split())
    return normalized(source) == normalized(expected)


def legacy_shop_route(source, paths):
    """Recognize the exact BTCPay emergency route; preserve any operator edits."""
    marker = re.match(r'# ([a-z0-9.-]+) — BTCPay Server\. LetsEncrypt cert \(npm-([0-9]+)\) obtained via NPM webroot\.\n', source)
    if not marker:
        return False
    try:
        name = domains(json.dumps([marker[1]]))[0]
    except ValueError:
        return False
    parent = Path(paths['certificates']) / 'live' / f'npm-{marker[2]}'
    expected = f'''
server {{
 listen 80; listen [::]:80;
 server_name {name} www.{name};
 location ^~ /.well-known/acme-challenge/ {{
  default_type text/plain; root {Path(paths['data']) / 'letsencrypt-acme-challenge'}; try_files $uri =404;
 }}
 location / {{ return 301 https://$host$request_uri; }}
}}
server {{
 listen 443 ssl; listen [::]:443 ssl;
 server_name {name} www.{name};
 ssl_certificate {parent / 'fullchain.pem'};
 ssl_certificate_key {parent / 'privkey.pem'};
 ssl_protocols TLSv1.2 TLSv1.3;
 location / {{
  proxy_pass http://127.0.0.1:23000;
  proxy_http_version 1.1;
  proxy_set_header Host $host;
  proxy_set_header X-Real-IP $remote_addr;
  proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
  proxy_set_header X-Forwarded-Proto https;
  proxy_set_header X-Forwarded-Scheme https;
  proxy_set_header Upgrade $http_upgrade;
  proxy_set_header Connection "upgrade";
  proxy_read_timeout 300s;
 }}
}}
'''
    def normalized(text):
        return ''.join(re.sub(r'#[^\n]*', '', text).split())
    return normalized(source) == normalized(expected)


def existing_route_changes(rows, paths, output=OUTPUT, directories=None):
    """Retire exact managed emergency routes and refuse other duplicate hosts."""
    if directories is None:
        directories = [Path('/etc/nginx/conf.d'), Path('/etc/nginx/sites-enabled')]
    claimed = {name for row in rows for name in domains(row['domain_names'])}
    tls_claimed = {name for row in rows if row.get('certificate_id') and not row.get('certificate_deleted')
                   for name in domains(row['domain_names'])}
    changes = []
    for directory in directories:
        if not directory.exists():
            continue
        for path in directory.iterdir():
            if not path.is_file() or path.resolve() == Path(output).resolve():
                continue
            if directory.name == 'conf.d' and path.suffix != '.conf':
                continue
            source = path.read_text()
            uncommented = re.sub(r'#[^\n]*', '', source)
            existing = {name for group in re.findall(r'\bserver_name\s+([^;]+);', uncommented)
                        for name in group.split()}
            recognized = (path.name in ('angor-indexer-npm.conf', 'angor-relay-npm.conf') and legacy_angor_route(
                source, paths, relay=path.name == 'angor-relay-npm.conf'
            )) or (path.name == 'shop-btcpay.conf' and legacy_shop_route(source, paths))
            # Never retire a working emergency endpoint without a complete
            # replacement, including every alias and its HTTPS listener.
            if recognized and existing and existing.issubset(tls_claimed):
                changes.append((path, b'# Temporary managed route retired; NPM owns routing through public-npm-proxy-hosts.conf.\n', 0o644))
                continue
            if claimed.intersection(existing):
                raise ValueError(f'Existing public nginx route conflicts with NPM in {path.name}; custom configuration preserved for review')
    return changes


def build_files(runtime, paths, dashboard=DASHBOARD, output=OUTPUT, state=STATE):
    """Create a reviewable candidate without altering database, keys or services."""
    trust_file = Path(state) / 'upstream-trust.pem'
    rows = hosts(paths['data'])
    configuration, trust, fingerprints = render(
        rows, paths, local_port(runtime, 80), local_port(runtime, 443),
        Path(paths['data']) / 'letsencrypt-acme-challenge', trust_file)
    dashboard = Path(dashboard)
    default_site = dashboard_acme_root(dashboard.read_text(), paths['data'])
    files = [(Path(output), configuration, 0o644), (trust_file, trust, 0o644),
             (dashboard, default_site.encode(), dashboard.stat().st_mode & 0o777)]
    files.extend(existing_route_changes(rows, paths, output))
    digest = hashlib.sha256(json.dumps(fingerprints).encode()).hexdigest()
    return files, digest


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--resolve', action='store_true')
    parser.add_argument('--remember', action='store_true')
    parser.add_argument('--prepare-realip', action='store_true')
    parser.add_argument('--acme-only', action='store_true')
    args = parser.parse_args()
    runtime = inspect_runtime()
    if args.resolve:
        paths = resolve_paths(runtime=runtime)
        if args.prepare_realip:
            prepare_realip(paths)
        if args.remember and runtime is not None and (Path(paths['data']) / 'database.sqlite').is_file():
            # Capture authoritative mounts BEFORE lifecycle code removes the
            # inspect record. Subsequent recreation must reuse custom storage.
            atomic(BASE / '.archy-storage.json', json.dumps(paths).encode())
        print(json.dumps(paths))
        return
    if args.acme_only:
        with Path('/run/lock/archy-nginx-config.lock').open('a+b') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            recover_pending(STATE / 'acme')
            recover_pending(STATE)
            paths = resolve_paths(runtime=runtime)
            candidate = dashboard_acme_root(DASHBOARD.read_text(), paths['data']).encode()
            apply_files([(DASHBOARD, candidate, DASHBOARD.stat().st_mode & 0o777)],
                        state=STATE / 'acme', locked=True)
        print('NPM default ACME root verified')
        return
    with Path('/run/lock/archy-nginx-config.lock').open('a+b') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        recover_pending(STATE / 'acme')
        recover_pending(STATE)
        if runtime is None:
            # A saved DB does not authorize resurrecting an uninstalled app's routes.
            files = existing_route_changes([], resolve_paths(runtime=None))
            if OUTPUT.exists():
                files.append((OUTPUT, b'# NPM is not installed; public bridge disabled.\n', 0o644))
            if files:
                apply_files(files, locked=True)
            print('NPM absent; no public routes installed')
            return
        paths = resolve_paths(runtime=runtime)
        # Certificate issuance must not wait for the optional HTTP/TLS bridge
        # listeners to be migrated or become ready.
        acme_candidate = dashboard_acme_root(DASHBOARD.read_text(), paths['data']).encode()
        apply_files([(DASHBOARD, acme_candidate, DASHBOARD.stat().st_mode & 0o777)],
                    state=STATE / 'acme', locked=True)
        files, fingerprint = build_files(runtime, paths)
        def reload_certificate():
            for args in (['nginx', '-t'], ['nginx', '-s', 'reload']):
                run(podman_args() + ['exec', 'nginx-proxy-manager'] + args)
        changed = apply_files(files, renewal_fingerprint=fingerprint, locked=True,
                              certificate_reload=reload_certificate)
    print('NPM public bridge updated' if changed else 'NPM public bridge unchanged')


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        # Do not expose DB values, private keys, command output or account data.
        print(f'NPM public bridge: {type(error).__name__}: {error}', file=__import__('sys').stderr)
        raise SystemExit(1)
