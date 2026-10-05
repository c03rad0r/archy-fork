#!/usr/bin/env python3
"""Keep default dashboard vhosts private while allowing HTTP-01 challenges.

Apply only to Archipelago's default HTTP/HTTPS servers. Named NPM public
services remain separate. Never trust Host, XFF or rewritten client addresses
as evidence that a request came from a private network.
"""
from pathlib import Path
import argparse
import fcntl
import os
import re
import subprocess
import tempfile
import time

BEGIN = '# BEGIN ARCHIPELAGO MANAGEMENT SOURCE GUARD'
END = '# END ARCHIPELAGO MANAGEMENT SOURCE GUARD'
GUARD = '''# BEGIN ARCHIPELAGO MANAGEMENT SOURCE GUARD
# Use the original socket peer, before any real_ip / forwarded-header rewrite.
geo $realip_remote_addr $archy_management_private_source {
    default 0;
    127.0.0.0/8 1;
    169.254.0.0/16 1;
    10.0.0.0/8 1;
    172.16.0.0/12 1;
    192.168.0.0/16 1;
    100.64.0.0/10 1;
    ::1/128 1;
    fc00::/7 1;
    fe80::/10 1;
}
# A configured trusted proxy may have rewritten remote_addr. Require both
# the original peer and the validated effective client to be private.
geo $remote_addr $archy_management_private_client {
    default 0;
    127.0.0.0/8 1;
    169.254.0.0/16 1;
    10.0.0.0/8 1;
    172.16.0.0/12 1;
    192.168.0.0/16 1;
    100.64.0.0/10 1;
    ::1/128 1;
    fc00::/7 1;
    fe80::/10 1;
}
map $http_x_archipelago_public_ingress $archy_management_public_ingress {
    default 1;
    '' 0;
}
map "$archy_management_private_source:$archy_management_private_client:$archy_management_public_ingress:$uri" $archy_management_denied {
    default 1;
    ~^1:1:0: 0;
    "~^[01]:[01]:[01]:/\\.well-known/acme-challenge/[A-Za-z0-9_-]+$" 0;
}
# END ARCHIPELAGO MANAGEMENT SOURCE GUARD
'''
CHECK = '    if ($archy_management_denied) { return 404; }\n'


def server_blocks(text):
    """Find server blocks without interpreting braces in comments or strings."""
    masked = list(text)
    quote = None
    escaped = False
    comment = False
    for i, char in enumerate(text):
        if comment:
            if char == '\n':
                comment = False
            else:
                masked[i] = ' '
        elif escaped:
            masked[i] = ' '
            escaped = False
        elif quote:
            masked[i] = ' '
            if char == '\\':
                escaped = True
            elif char == quote:
                quote = None
        elif char == '#':
            comment = True
            masked[i] = ' '
        elif char in ('"', "'"):
            quote = char
            masked[i] = ' '
    plain = ''.join(masked)
    for found in re.finditer(r'\bserver\s*\{', plain):
        opening = found.end() - 1
        depth = 1
        end = opening + 1
        while end < len(plain) and depth:
            if plain[end] == '{':
                depth += 1
            elif plain[end] == '}':
                depth -= 1
            end += 1
        if depth:
            raise ValueError('Unbalanced nginx server block; configuration left unchanged')
        yield opening, end, plain[opening + 1:end - 1]


def guarded(text):
    original = text
    if text.count(BEGIN) != text.count(END) or text.count(BEGIN) > 1:
        raise ValueError('Ambiguous managed source guard; configuration left unchanged')
    if BEGIN in text and text.index(BEGIN) > text.index(END):
        raise ValueError('Reversed managed source guard markers; configuration left unchanged')
    text = re.sub(re.escape(BEGIN) + r'.*?' + re.escape(END) + r'\n?', '', text, flags=re.S)
    edits = []
    protected = set()
    for opening, end, body in server_blocks(text):
        ports = set()
        # Older node-CA setup used address-specific HTTPS listeners without
        # default_server. The dashboard's catch-all name still identifies it.
        management = any('_' in names.split() for names in
                         re.findall(r'\bserver_name\s+([^;]+);', body))
        for listen in re.findall(r'\blisten\s+([^;]+);', body):
            tokens = listen.split()
            if 'default_server' not in tokens and not management:
                continue
            match = re.search(r'(?:^|:)(80|443)$', tokens[0])
            if match:
                ports.add(int(match[1]))
        if not ports:
            continue
        protected.update(ports)
        actual = text[opening + 1:end - 1]
        if CHECK.strip() not in actual:
            edits.append(opening + 1)
    if protected != {80, 443}:
        raise ValueError('Expected both default HTTP and HTTPS dashboard servers; no partial guard installed')
    for at in reversed(edits):
        text = text[:at] + '\n' + CHECK + text[at:]
    text = GUARD + '\n' + text.lstrip('\n')
    return text if text != original else original


def atomic(path, data, mode):
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        os.fchmod(stream.fileno(), mode)
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    try:
        os.replace(temporary, path)
        sync_directory(path.parent)
    finally:
        temporary.unlink(missing_ok=True)


def sync_directory(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def active_dashboard(nginx_root=Path('/etc/nginx')):
    enabled = nginx_root / 'sites-enabled/archipelago'
    available = nginx_root / 'sites-available/archipelago'
    # Installed systems have both symlinks and standalone enabled copies.
    # Follow a symlink without replacing it; patch the copy when it is active.
    selected = enabled if enabled.exists() or enabled.is_symlink() else available
    return selected.resolve(strict=True)


def apply(path, command=subprocess.run, lock_path=Path('/run/lock/archy-nginx-config.lock'), source=None):
    # The NPM bridge uses this same lock for nginx configuration transactions.
    with lock_path.open('a+b') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        return apply_locked(path.resolve(strict=True), command, source)


def apply_locked(path, command, source=None):
    old = path.read_bytes()
    # A cached legacy OTA can predate the guard. Never install its unguarded
    # bytes and repair them afterwards: another startup task can reload nginx
    # in that gap. Validate/render before the atomic replacement, under the
    # same lock as the public-host bridge.
    new = guarded(source.read_text() if source is not None else old.decode()).encode()
    if new == old:
        return False
    backup_dir = (Path('/var/lib/archipelago/nginx-management-guard')
                  if path.is_relative_to('/etc/nginx') else path.parent)
    backup_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    backup = backup_dir / (path.name + '.before-management-guard-' + str(time.time_ns()))
    # Exclusive, synced backup is a prerequisite to modifying the live config.
    with backup.open('xb') as stream:
        os.fchmod(stream.fileno(), 0o600)
        stream.write(old)
        stream.flush()
        os.fsync(stream.fileno())
    sync_directory(backup.parent)
    mode = path.stat().st_mode & 0o777
    atomic(path, new, mode)
    try:
        for args in (['nginx', '-t'], ['systemctl', 'reload', 'nginx']):
            result = command(args, capture_output=True, timeout=30)
            if result.returncode:
                raise RuntimeError('Dashboard source guard validation/reload failed')
    except Exception as failure:
        atomic(path, old, mode)
        # Reload the known previous configuration if a failed reload changed state.
        for args in (['nginx', '-t'], ['systemctl', 'reload', 'nginx']):
            result = command(args, capture_output=True, timeout=30)
            if result.returncode:
                raise RuntimeError('Previous configuration restored on disk, but rollback validation/reload failed') from failure
        raise
    return True


def protect_template(path):
    """Keep rollback to an older binary from reinstalling an unguarded template.

    This updates an inactive runtime payload, without reloading nginx. The old
    binary will copy these already-guarded bytes using its legacy installer.
    """
    path = path.resolve(strict=True)
    old = path.read_bytes()
    new = guarded(old.decode()).encode()
    if new == old:
        return False
    atomic(path, new, path.stat().st_mode & 0o777)
    return True


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--render', action='store_true')
    parser.add_argument('--install', type=Path, metavar='SOURCE')
    parser.add_argument('--protect-template', type=Path, metavar='PATH')
    parser.add_argument('path', nargs='?')
    args = parser.parse_args()
    if sum(bool(value) for value in [args.render, args.install, args.protect_template]) > 1:
        parser.error('--render, --install and --protect-template cannot be combined')
    if args.protect_template:
        protect_template(args.protect_template)
        print('Rollback runtime template protected')
        return
    path = Path(args.path) if args.path else active_dashboard()
    if args.render:
        print(guarded(path.read_text()), end='')
    else:
        print('Dashboard public source guard installed' if apply(path, source=args.install) else 'Dashboard source guard unchanged')


if __name__ == '__main__':
    main()
