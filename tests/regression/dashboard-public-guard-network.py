#!/usr/bin/env python3
"""Actual nginx HTTP/TLS source-boundary tests in a disposable network namespace."""
import importlib.util
import os
from pathlib import Path
import socket
import ssl
import subprocess
import tempfile
import time

assert os.geteuid() == 0, 'Run through the isolated systemd test unit'
assert os.readlink('/proc/self/ns/net') != os.readlink('/proc/1/ns/net'), 'Refusing host network namespace'
root = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('guard', root / 'scripts/dashboard-public-guard.py')
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)
bridge_spec = importlib.util.spec_from_file_location('bridge', root / 'scripts/npm-public-bridge.py')
bridge = importlib.util.module_from_spec(bridge_spec)
bridge_spec.loader.exec_module(bridge)
subprocess.run(['ip', 'link', 'set', 'lo', 'up'], check=True)
for address in ['198.18.0.1/32', '198.18.0.2/32', '192.168.10.2/32', '100.64.123.2/32',
                '2001:db8:1::1/128', '2001:db8:1::2/128', 'fd00:1::2/128']:
    subprocess.run(['ip', 'addr', 'add', address, 'dev', 'lo'], check=True)
with tempfile.TemporaryDirectory(prefix='archy-guard-network-') as tmp:
    tmp = Path(tmp)
    tmp.chmod(0o755)
    challenge = tmp / 'letsencrypt-acme-challenge/.well-known/acme-challenge'
    challenge.mkdir(parents=True)
    (challenge / 'test-token').write_text('exact-acme-token')
    cert, key = tmp / 'cert.pem', tmp / 'key.pem'
    subprocess.run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1',
                    '-subj', '/CN=fixture.example', '-keyout', str(key), '-out', str(cert)],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    source = f'''pid {tmp}/nginx.pid;
error_log {tmp}/error.log;
events {{ worker_connections 128; }}
http {{
 access_log off;
 set_real_ip_from 127.0.0.1;
 set_real_ip_from ::1;
 real_ip_header X-Real-IP;
 server {{
  listen 80 default_server;
  listen [::]:80 default_server;
  server_name _;
  location ^~ /.well-known/acme-challenge/ {{ root {bridge.BASE}/data/letsencrypt-acme-challenge; try_files $uri =404; }}
  location / {{ return 200 "dashboard-or-rpc"; }}
 }}
 server {{
  listen 443 ssl default_server;
  listen [::]:443 ssl default_server;
  ssl_certificate {cert}; ssl_certificate_key {key};
  server_name _;
  # Legacy shipped HTTPS template omitted the ACME location.
  location / {{ return 200 "dashboard-or-rpc"; }}
 }}
 server {{ listen 80; listen [::]:80; server_name public.example;
  location / {{ return 200 "public-app"; }}
 }}
}}
'''
    # Use the same http-context site include as a real appliance, so the
    # guarded runtime installer is exercised against actual nginx reloads.
    start = source.index('http {') + len('http {')
    legacy = tmp / 'legacy-runtime.conf'
    legacy.write_text(bridge.dashboard_acme_root(source[start:source.rfind('}')], tmp))
    site = tmp / 'site.conf'
    site.write_text(guard.guarded(legacy.read_text()))
    config = tmp / 'nginx.conf'
    config.write_text(source[:start] + f'\ninclude {site};\n}}\n')
    command = ['nginx', '-p', str(tmp), '-c', str(config)]
    subprocess.run(command + ['-t'], check=True, capture_output=True)
    subprocess.run(command, check=True, capture_output=True)
    context = ssl.create_default_context(cafile=str(cert))
    context.check_hostname = False  # Unknown-SNI routing probe; certificate chain still verified.
    def request(src, path='/', tls=False, host='unknown.example', sni='unknown.example', extra='', method='GET', websocket=False):
        family = socket.AF_INET6 if ':' in src else socket.AF_INET
        target = '2001:db8:1::1' if family == socket.AF_INET6 else '198.18.0.1'
        stream = socket.socket(family)
        stream.settimeout(4)
        stream.bind((src, 0))
        stream.connect((target, 443 if tls else 80))
        if tls:
            stream = context.wrap_socket(stream, server_hostname=sni)
        with stream:
            connection = 'Upgrade' if websocket else 'close'
            if websocket:
                extra += 'Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n'
            stream.sendall(f'{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: {connection}\r\nContent-Length: 0\r\n{extra}\r\n'.encode())
            body = b''
            while data := stream.recv(65536):
                body += data
                # Upgrade requests can leave a rejected connection persistent.
                if websocket and b'\r\n\r\n' in body:
                    headers, payload = body.split(b'\r\n\r\n', 1)
                    lengths = [int(line.split(b':', 1)[1]) for line in headers.split(b'\r\n')
                               if line.lower().startswith(b'content-length:')]
                    if lengths and len(payload) >= lengths[0]:
                        break
        return int(body.split(b' ', 2)[1]), body
    try:
        count = 0
        for src in ['198.18.0.2', '2001:db8:1::2']:
            for tls in [False, True]:
                for host in ['unknown.example', '127.0.0.1', 'archipelago.local', '198.18.0.1', '[2001:db8:1::1]']:
                    for path in ['/', '/login', '/assets/dashboard.js', '/rpc/v1', '/ws', '/.well-known/acme-challenge/../rpc/v1']:
                        status, body = request(src, path, tls, host, None if host in ['198.18.0.1', '[2001:db8:1::1]'] else host,
                            'X-Forwarded-For: 127.0.0.1\r\nX-Real-IP: 192.168.1.2\r\n',
                            method='POST' if path == '/rpc/v1' else 'GET', websocket=path == '/ws')
                        assert status == 404 and b'dashboard-or-rpc' not in body, (src, tls, host, path, status)
                        count += 1
                status, body = request(src, '/.well-known/acme-challenge/test-token', tls)
                assert status == 200 and body.endswith(b'exact-acme-token'), (status, body)
            assert request(src, host='public.example')[1].endswith(b'public-app')
        for src in ['127.0.0.1', '192.168.10.2', '100.64.123.2', '::1', 'fd00:1::2']:
            for tls in [False, True]:
                assert request(src, '/rpc/v1', tls)[0] == 200
        for src in ['127.0.0.1', '::1']:
            for tls in [False, True]:
                for client in ['198.18.0.2', '2001:db8:1::2']:
                    assert request(src, '/rpc/v1', tls, extra=f'X-Real-IP: {client}\r\n', method='POST')[0] == 404
                assert request(src, '/rpc/v1', tls, extra='X-Real-IP: 192.168.10.2\r\n')[0] == 200
                assert request(src, '/rpc/v1', tls, extra='X-Archipelago-Public-Ingress: 1\r\n', method='POST')[0] == 404
                status, body = request(src, '/.well-known/acme-challenge/test-token', tls,
                    extra='X-Real-IP: 198.18.0.2\r\nX-Archipelago-Public-Ingress: 1\r\n')
                assert status == 200 and body.endswith(b'exact-acme-token')
        subprocess.run(command + ['-s', 'reload'], check=True, capture_output=True)
        assert request('198.18.0.2')[0] == 404
        assert request('fd00:1::2', tls=True)[0] == 200
        def fixture_command(args, **kwargs):
            assert site.read_text().count(guard.CHECK) == 2
            actual = command + (['-t'] if args[0] == 'nginx' else ['-s', 'reload'])
            return subprocess.run(actual, **kwargs)
        assert guard.apply(site, fixture_command, tmp / 'lock', legacy) is False
        legacy.write_text(legacy.read_text() + '# old OTA payload with no guard\n')
        assert guard.apply(site, fixture_command, tmp / 'lock', legacy)
        for src in ['198.18.0.2', '2001:db8:1::2']:
            for tls in [False, True]:
                for endpoint in ['/', '/assets/main.js', '/rpc/v1', '/ws']:
                    assert request(src, endpoint, tls, extra='X-Forwarded-For: 127.0.0.1\r\nX-Real-IP: 192.168.1.2\r\n')[0] == 404
                assert request(src, '/.well-known/acme-challenge/test-token', tls)[0] == 200
        assert request('fd00:1::2', tls=True)[0] == 200
        # Emulate the actual legacy rollback: its old binary copies the runtime
        # template verbatim. Protect the payload before that old code can run.
        assert guard.protect_template(legacy)
        site.write_bytes(legacy.read_bytes())
        subprocess.run(command + ['-t'], check=True, capture_output=True)
        subprocess.run(command + ['-s', 'reload'], check=True, capture_output=True)
        assert request('198.18.0.2', '/rpc/v1')[0] == 404
        assert request('2001:db8:1::2', '/rpc/v1', tls=True)[0] == 404
        previous = site.read_bytes()
        legacy.write_text(legacy.read_text() + 'invalid_nginx_directive;\n')
        try:
            guard.apply(site, fixture_command, tmp / 'lock', legacy)
            raise AssertionError('Invalid runtime configuration was accepted')
        except RuntimeError:
            pass
        assert site.read_bytes() == previous
        assert request('198.18.0.2')[0] == 404
        assert request('fd00:1::2', tls=True)[0] == 200
        print('PASS real legacy runtime installation and invalid-template rollback: guarded before reload, public IPv4/IPv6 blocked, ACME/private access preserved')
        print(f'PASS {count} public HTTP/TLS negative cases: IPv4/IPv6, unknown/raw/forged Host/SNI, forwarded headers, UI/assets/RPC/WS; ACME/private access and reload')
    finally:
        subprocess.run(command + ['-s', 'quit'], check=True, capture_output=True)
        for _ in range(40):
            if not (tmp / 'nginx.pid').exists():
                break
            time.sleep(.05)
