#!/usr/bin/env python3
"""Create a private node enrollment for an existing operator-owned frps gateway.

Reads frps token configuration without printing credentials. The admission policy
is replaced atomically; an existing node requires --rotate to replace its token.
The exported enrollment is for Setup's file picker, never Nostr or public storage.
"""
import argparse
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import secrets
import ssl
import fcntl
import tempfile
from policy import DOMAIN, NAME


def atomic(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, stage = tempfile.mkstemp(prefix='.' + path.name, dir=path.parent)
    try:
        with os.fdopen(fd, 'w') as f:
            os.fchmod(f.fileno(), 0o600)
            json.dump(value, f); f.flush(); os.fsync(f.fileno())
        os.replace(stage, path)
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try: os.fsync(directory)
        finally: os.close(directory)
    finally:
        Path(stage).unlink(missing_ok=True)


def main():
    os.umask(0o077)
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--frps-config', type=Path, required=True)
    p.add_argument('--policy', type=Path, required=True)
    p.add_argument('--ca', type=Path, required=True)
    p.add_argument('--host', required=True)
    p.add_argument('--tls-server-name', required=True)
    p.add_argument('--name', required=True)
    p.add_argument('--domain', action='append', required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--rotate', action='store_true')
    args = p.parse_args()
    if not NAME.fullmatch(args.name) or len(args.domain) > 32 or any(not DOMAIN.fullmatch(d) for d in args.domain):
        p.error('Use a lowercase node name and exact lowercase domains')
    for host in (args.host, args.tls_server_name):
        try: ipaddress.ip_address(host)
        except ValueError:
            if not DOMAIN.fullmatch(host): p.error('Invalid gateway host or TLS name')
    if args.output.exists(): p.error('Enrollment output already exists; choose a new private file')
    if args.frps_config.stat().st_mode & 0o077: p.error('frps configuration must be private (0600)')
    server = json.loads(args.frps_config.read_text())
    auth = server.get('auth', {})
    if auth.get('method') != 'token' or not isinstance(auth.get('token'), str) or len(auth['token']) < 32:
        p.error('Gateway requires a strong frps transport token')
    if server.get('transport', {}).get('tls', {}).get('force') is not True:
        p.error('Gateway must require TLS')
    required = {'Login', 'NewProxy', 'Ping', 'NewWorkConn', 'NewUserConn'}
    if not any(required.issubset(plugin.get('ops', [])) for plugin in server.get('httpPlugins', [])):
        p.error('Configure the admission plugin for every required operation first')
    args.policy.parent.mkdir(parents=True, exist_ok=True)
    policy_lock = args.policy.with_suffix(args.policy.suffix + ".lock").open("a")
    os.chmod(policy_lock.name, 0o600)
    fcntl.flock(policy_lock, fcntl.LOCK_EX)
    existing = {}
    if args.policy.exists():
        if args.policy.stat().st_mode & 0o077: p.error('Admission policy must be private (0600)')
        existing = json.loads(args.policy.read_text())
    if not isinstance(existing, dict): p.error('Invalid admission policy')
    if args.name in existing and not args.rotate: p.error('Node already enrolled; use --rotate explicitly')
    for name, entry in existing.items():
        if name != args.name and set(entry.get('domains', [])) & set(args.domain):
            p.error('A domain is already assigned to another enrollment')
    ca = args.ca.read_text()
    if len(ca) > 16384 or not ca.startswith('-----BEGIN CERTIFICATE-----') or 'PRIVATE KEY' in ca:
        p.error('Supply only the public gateway CA certificate')
    try: ssl.create_default_context().load_verify_locations(cadata=ca)
    except ssl.SSLError: p.error("Invalid gateway CA certificate")
    token = secrets.token_hex(32)
    enrollment = {'host': args.host, 'port': server['bindPort'], 'node_id': args.name,
                  'tls_server_name': args.tls_server_name, 'transport_token': auth['token'],
                  'enrollment_token': token, 'ca_pem': ca, 'domains': args.domain}
    # Save the recoverable private output before changing admission. A failed
    # policy write leaves a file that is not yet enrolled, never a lost token.
    atomic(args.output, enrollment)
    existing[args.name] = {'enabled': True, 'token_sha256': hashlib.sha256(token.encode()).hexdigest(), 'domains': args.domain}
    atomic(args.policy, existing)
    print('Private enrollment written. Import it in Setup; do not publish it.')


if __name__ == '__main__': main()
