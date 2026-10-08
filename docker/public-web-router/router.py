#!/usr/bin/env python3
"""Supervise node-owned frpc and Caddy. Configuration is supplied by Setup.

No local management listener or arbitrary TCP forwarding. Invalid or removed
configuration stops the owned children. Certificates persist in /data.
"""
import ipaddress
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

DOMAIN = re.compile(r'(?=.{1,253}\Z)(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}\Z')
NAME = re.compile(r'[a-z0-9][a-z0-9-]{0,47}\Z')


def render(config):
    if config.get('schema') != 1:
        raise ValueError('Unsupported configuration')
    gateway = config['gateway']
    host = gateway['host']
    try:
        ipaddress.ip_address(host)
    except ValueError:
        if not DOMAIN.fullmatch(host):
            raise ValueError('Invalid gateway hostname')
    port = gateway['port']
    if type(port) is not int or not 1024 <= port <= 65535:
        raise ValueError('Invalid gateway control port')
    node = gateway['node_id']
    if not NAME.fullmatch(node):
        raise ValueError('Invalid enrollment name')
    for key in ('transport_token', 'enrollment_token'):
        if not isinstance(gateway[key], str) or not 32 <= len(gateway[key]) <= 256:
            raise ValueError('Invalid enrollment credential')
    pem = gateway['ca_pem']
    if len(pem) > 16384 or not pem.startswith('-----BEGIN CERTIFICATE-----') or 'PRIVATE KEY' in pem:
        raise ValueError('A gateway CA certificate is required')
    server_name = gateway['tls_server_name']
    try:
        ipaddress.ip_address(server_name)
    except ValueError:
        if not DOMAIN.fullmatch(server_name):
            raise ValueError('Invalid gateway TLS name')
    mode = config.get('certificate_mode', 'public')
    if mode not in ('public', 'test'):
        raise ValueError('Invalid certificate mode')
    routes = config['routes']
    if not isinstance(routes, list) or len(routes) > 32:
        raise ValueError('Too many routes')
    caddy = '{\n admin off\n auto_https disable_redirects\n skip_install_trust\n}\n'
    proxies = []
    domains, names = set(), set()
    for route in routes:
        name, domain = route['id'], route['domain']
        if not NAME.fullmatch(name) or not DOMAIN.fullmatch(domain) or name in names or domain in domains:
            raise ValueError('Invalid or duplicate route')
        if domain not in gateway.get('domains', []):
            raise ValueError('Domain is not assigned by enrollment')
        names.add(name); domains.add(domain)
        address = ipaddress.IPv6Address(route['fips_address'])
        if address not in ipaddress.IPv6Network('fd00::/8'):
            raise ValueError('A FIPS ULA address is required')
        upstream = route['port']
        app_id = route.get('app_id')
        if app_id is not None:
            if not isinstance(app_id, str) or not NAME.fullmatch(app_id) or name != 'app-' + app_id or type(upstream) is not int or not 1024 <= upstream <= 65535:
                raise ValueError('Invalid catalogue app route')
            identity_header = f'X-Archipelago-App {app_id}'
        else:
            if type(upstream) is not int or not 32000 <= upstream < 32032:
                raise ValueError('Only published website listeners are supported')
            identity_header = f'X-Archipelago-Website {name}'
        tls = 'tls internal' if mode == 'test' else 'tls {\n issuer acme {\n disable_http_challenge\n }\n }'
        caddy += f'https://{domain}:8443 {{\n bind 127.0.0.1\n {tls}\n reverse_proxy http://[{address}]:{upstream} {{\n header_up {identity_header}\n }}\n}}\n'
        proxies.append({'name': name, 'type': 'https', 'localIP': '127.0.0.1', 'localPort': 8443, 'customDomains': [domain]})
    frpc = {'serverAddr': host, 'serverPort': port, 'user': node,
            'metadatas': {'enrollment_token': gateway['enrollment_token']},
            'auth': {'method': 'token', 'token': gateway['transport_token'], 'additionalScopes': ['HeartBeats', 'NewWorkConns']},
            'transport': {'tls': {'enable': True, 'trustedCaFile': '/tmp/router/gateway.crt', 'serverName': server_name}},
            'loginFailExit': False, 'proxies': proxies, 'log': {'to': 'console', 'level': 'error'}}
    return caddy, frpc, pem


def main():
    os.umask(0o077)
    root = Path('/tmp/router'); root.mkdir(exist_ok=True)
    source = Path('/config/router.json')
    children = []
    stopping = False
    previous = None

    def stop_children():
        for child in children:
            if child.poll() is None:
                child.terminate()
        for child in children:
            try: child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill(); child.wait()
        children.clear()

    def shutdown(*_):
        nonlocal stopping
        stopping = True

    signal.signal(signal.SIGTERM, shutdown)
    signal.signal(signal.SIGINT, shutdown)
    try:
        while not stopping:
            try:
                if source.stat().st_size > 131072:
                    raise ValueError('Oversized config')
                raw = source.read_bytes()
                caddy, frpc, pem = render(json.loads(raw))
                if previous != raw or any(child.poll() is not None for child in children):
                    stop_children()
                    (root/'gateway.crt').write_text(pem)
                    (root/'frpc.json').write_text(json.dumps(frpc))
                    (root/'Caddyfile').write_text(caddy)
                    if frpc['proxies']:
                        for command in [ ['/usr/local/bin/caddy', 'validate', '--config', str(root/'Caddyfile'), '--adapter', 'caddyfile'], ['/usr/local/bin/frpc', 'verify', '-c', str(root/'frpc.json')] ]:
                            subprocess.run(command, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15)
                        for command in [['/usr/local/bin/caddy', 'run', '--config', str(root/'Caddyfile'), '--adapter', 'caddyfile'], ['/usr/local/bin/frpc', '-c', str(root/'frpc.json')]]:
                            children.append(subprocess.Popen(command))
                    previous = raw
                (root/'status.json').write_text(json.dumps({'configured': True, 'routes': len(frpc['proxies']), 'certificate_mode': json.loads(raw).get('certificate_mode', 'public'), 'externally_verified': False}))
            except (OSError, ValueError, KeyError, TypeError, AttributeError, subprocess.SubprocessError):
                stop_children(); previous = None
                for name in ('frpc.json', 'Caddyfile', 'gateway.crt'):
                    (root/name).unlink(missing_ok=True)
                (root/'status.json').write_text(json.dumps({'configured': False, 'externally_verified': False}))
            (root/'heartbeat').touch()
            time.sleep(2)
    finally:
        stop_children()


if __name__ == '__main__':
    main()
