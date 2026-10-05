#!/usr/bin/env python3
"""Opt-in real NPM API/host-nginx integration with disposable state and listeners."""
import importlib.util
import base64
import http.client
import ipaddress
import json
import os
from pathlib import Path
import secrets
import socket
import ssl
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid
import yaml

if os.environ.get('ARCHY_ALLOW_DISPOSABLE_CONTAINERS') != '1':
    raise SystemExit('Set ARCHY_ALLOW_DISPOSABLE_CONTAINERS=1 for isolated NPM integration')
ROOT = Path(__file__).resolve().parents[2]
NPM_IMAGE = yaml.safe_load((ROOT / 'apps/nginx-proxy-manager/manifest.yml').read_text())['app']['container']['image']
spec = importlib.util.spec_from_file_location('bridge', ROOT / 'scripts/npm-public-bridge.py')
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)
# Opt-in reproduction of the previous generated configuration. Normal acceptance
# never enables this; a legacy run must fail the alternating-certificate check.
if os.environ.get('ARCHY_NPM_TEST_LEGACY_TLS_REUSE') == '1':
    fixed_render = bridge.render
    def legacy_render(*args, **kwargs):
        config, trust, fingerprints = fixed_render(*args, **kwargs)
        return config.replace(b'proxy_ssl_session_reuse off;', b'proxy_ssl_session_reuse on;'), trust, fingerprints
    bridge.render = legacy_render


def run(*args):
    result = subprocess.run(args, capture_output=True, timeout=120)
    if result.returncode:
        # NPM logs/API setup may contain credentials. Never dump them on failure.
        raise RuntimeError(f'{args[0]} fixture operation failed ({result.returncode})')
    return result.stdout


def available_port():
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
        return probe.getsockname()[1]


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def request(url, data=None, method=None, headers=None, timeout=15):
    req = urllib.request.Request(url, data=data, method=method, headers=headers or {})
    try:
        with urllib.request.build_opener(NoRedirect).open(req, timeout=timeout) as response:
            return response.status, response.headers, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.headers, error.read()


name = 'archy-npm-test-' + uuid.uuid4().hex[:10]
backend = name + '-upstream'
network = name + '-net'
npm_network = os.environ.get('ARCHY_NPM_NETWORK', 'slirp4netns:allow_host_loopback=true,cidr=169.254.1.0/24')
upstream_port = available_port()
lan_address = json.loads(run('ip', '-j', 'route', 'get', '1.1.1.1'))[0]['prefsrc']
assert ipaddress.ip_address(lan_address).is_private, 'Fixture requires a private LAN address'
upstream_host = os.environ.get('ARCHY_NPM_UPSTREAM', lan_address)
nginx_started = False
network_created = False
acme = None
with tempfile.TemporaryDirectory(prefix='archy-npm-bridge-test-') as directory:
    root = Path(directory)
    layout = os.environ.get('ARCHY_NPM_LAYOUT', 'flat')
    assert layout in ('flat', 'nested')
    base = root / 'npm'
    data = base if layout == 'flat' else base / 'data'
    certs = base / 'letsencrypt'
    data.mkdir(parents=True); certs.mkdir(parents=True)
    bridge.prepare_realip({'data': str(data)})
    nginx = ['/usr/sbin/nginx', '-p', str(root), '-c', str(root / 'nginx.conf')]
    try:
        http_port, tls_port = available_port(), available_port()
        if os.environ.get('ARCHY_NPM_ACME') == '1':
            acme_spec = importlib.util.spec_from_file_location('acme_fixture', Path(__file__).with_name('npm-acme-fixture.py'))
            acme_module = importlib.util.module_from_spec(acme_spec)
            acme_spec.loader.exec_module(acme_module)
            acme = acme_module.AcmeFixture(root, http_port, run, available_port)
            acme.start()
        run('podman', 'network', 'create', network)
        network_created = True
        fixture = r"""const server=require('http').createServer((q,r)=>{r.setHeader('Content-Type','application/json');r.end(JSON.stringify({route:q.url,client:q.headers['x-real-ip'],xff:q.headers['x-forwarded-for'],publicIngress:q.headers['x-archipelago-public-ingress']}))});server.on('upgrade',(q,s)=>{const accept=require('crypto').createHash('sha1').update(q.headers['sec-websocket-key']+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');const frame='["EOSE","fixture"]';s.end('HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: '+accept+'\r\n\r\n'+String.fromCharCode(129,frame.length)+frame,'latin1')});server.listen(8080,'0.0.0.0')"""
        run('podman', 'run', '-d', '--name', backend, '--network', network,
            '--network-alias', 'fixture-upstream', '--memory', '128m',
            '-p', f'127.0.0.1:{upstream_port}:8080',
            '-p', f'{lan_address}:{upstream_port}:8080',
            'docker.io/library/node:24-alpine', 'node', '-e', fixture)
        run('podman', 'run', '-d', '--name', name, '--network', npm_network,
            '--memory', '512m', '--pids-limit', '512',
            '-p', '127.0.0.1::81', '-p', '127.0.0.1::80', '-p', '127.0.0.1::443',
            '-v', f'{data}:/data', '-v', f'{certs}:/etc/letsencrypt',
            *(acme.npm_args() if acme else []),
            NPM_IMAGE)
        runtime = json.loads(run('podman', 'inspect', name))[0]
        admin = 'http://' + bridge.local_port(runtime, 81)
        deadline = time.monotonic() + 150
        while time.monotonic() < deadline:
            try:
                if request(admin + '/api/')[0] == 200:
                    break
            except OSError:
                pass
            time.sleep(2)
        else:
            raise RuntimeError('Disposable NPM admin did not become ready')
        token = None
        def api(path, payload=None, method=None):
            headers = {'Content-Type': 'application/json'}
            if token:
                headers['Authorization'] = 'Bearer ' + token
            status, _, body = request(admin + '/api' + path,
                                      None if payload is None else json.dumps(payload).encode(), method, headers,
                                      timeout=180 if acme else 15)
            if status not in (200, 201, 204):
                # Only non-secret schema diagnostics, never raw request/response.
                if acme and path.startswith('/nginx/certificates'):
                    fd, diagnostic = tempfile.mkstemp(prefix='archy-npm-acme-error-', suffix='.json')
                    with os.fdopen(fd, 'wb') as stream:
                        stream.write(body)
                    print('Private ACME failure diagnostic: ' + diagnostic, flush=True)
                raise RuntimeError(f'NPM API {method or "GET"} {path} returned {status}')
            return json.loads(body) if body else None
        password = secrets.token_urlsafe(32)
        api('/users', {'name': 'Disposable Fixture', 'nickname': 'Fixture', 'email': 'fixture@example.test',
                       'auth': {'type': 'password', 'secret': password}}, 'POST')
        token = api('/tokens', {'identity': 'fixture@example.test', 'secret': password}, 'POST')['token']
        deadline = time.monotonic() + 30
        while True:
            try:
                assert request(f'http://127.0.0.1:{upstream_port}/fixture-ready')[0] == 200
                probe = run('podman', 'exec', name, 'curl', '--silent', '--show-error',
                            '--max-time', '5', '--fail', f'http://{upstream_host}:{upstream_port}/fixture-ready')
                assert json.loads(probe)['route'] == '/fixture-ready'
                break
            except (OSError, RuntimeError, AssertionError):
                if time.monotonic() >= deadline:
                    raise RuntimeError('NPM same-node fixture upstream is not reachable') from None
                time.sleep(1)
        print('PASS NPM actual namespace reaches same-node upstream', flush=True)
        if 'cidr=169.254.1.0/24' in npm_network:
            for address in [lan_address, 'host.containers.internal', '169.254.1.2']:
                probe = run('podman', 'exec', name, 'curl', '--silent', '--show-error',
                            '--max-time', '5', '--fail', f'http://{address}:{upstream_port}/fixture-ready')
                assert json.loads(probe)['route'] == '/fixture-ready'
            print('PASS LAN, stable host alias and legacy gateway from NPM namespace', flush=True)
        payload = {'domain_names': ['fixture.example', 'second.example'], 'forward_scheme': 'http',
                   'forward_host': upstream_host, 'forward_port': upstream_port,
                   'allow_websocket_upgrade': True,
                   'advanced_config': 'location = /custom { return 200 "custom-route"; }'}
        host = api('/nginx/proxy-hosts', payload, 'POST')
        assert host['meta'].get('nginx_online', True), 'NPM rejected fixture config'
        paths = bridge.resolve_paths(base, runtime)
        assert paths == {'data': str(data), 'certificates': str(certs)}
        output, trust_file = root / 'public.conf', root / 'trust.pem'
        def sync():
            config, trust, fingerprints = bridge.render(bridge.hosts(data), paths,
                bridge.local_port(runtime, 80), bridge.local_port(runtime, 443),
                data / 'letsencrypt-acme-challenge', trust_file)
            if acme:
                # Trust the local test CA only inside this disposable nginx.
                trust += b'\n' + acme.root_pem
            config = config.replace(b'listen 80;', f'listen 127.0.0.1:{http_port};'.encode())
            config = config.replace(b'listen [::]:80;', b'')
            config = config.replace(b'listen 443 ssl;', f'listen 127.0.0.1:{tls_port} ssl;'.encode())
            config = config.replace(b'listen [::]:443 ssl;', b'')
            def command(args, **kwargs):
                translated = nginx + (['-t'] if args[0] == 'nginx' else ['-s', 'reload'])
                return subprocess.run(translated, **kwargs)
            def reload_certificate():
                run('podman', 'exec', name, 'nginx', '-t')
                run('podman', 'exec', name, 'nginx', '-s', 'reload')
            return bridge.apply_files([(output, config, 0o644), (trust_file, trust, 0o600)],
                root / 'state', command, root / 'lock', json.dumps(fingerprints),
                certificate_reload=reload_certificate)
        output.write_text('# initial\n')
        (root / 'nginx.conf').write_text(f'''pid {root}/nginx.pid;
error_log {root}/nginx-error.log;
events {{ worker_connections 128; }}
http {{ access_log {root}/access.log;
server {{ listen 127.0.0.1:{http_port} default_server;
 location ^~ /.well-known/acme-challenge/ {{ root {data}/letsencrypt-acme-challenge; try_files $uri =404; }}
 location / {{ return 404; }}
}} include {output}; }}
''')
        run(*nginx, '-t'); run(*nginx); nginx_started = True
        assert sync()
        time.sleep(.3)
        def public(path='/', host='fixture.example'):
            return request(f'http://127.0.0.1:{http_port}' + path,
                           headers={'Host': host, 'X-Real-IP': '192.168.99.99', 'X-Forwarded-For': '192.168.99.99'})
        status, _, body = public()
        assert status == 200, f'Public bridge status {status}'
        observed = json.loads(body)
        assert observed['client'] == '127.0.0.1', 'NPM did not preserve actual bridge client IP: ' + str(observed['client'])
        assert '192.168.99.99' not in observed['xff'], 'Forged forwarding header survived bridge'
        assert observed['publicIngress'] == '1', 'Public ingress marker missing at upstream'
        assert public('/custom')[2] == b'custom-route'
        assert public(host='second.example')[0] == 200
        assert public(host='unknown.example')[0] == 404
        assert not sync(), 'Unchanged configuration reloaded nginx'
        print('PASS real NPM fresh setup/API host creation, shared domains, custom route, client IP and spoof rejection', flush=True)
        if acme:
            acme.verify(api, sync, public, payload, tls_port, root/'access.log')
        certificate = api('/nginx/certificates', {'provider': 'other', 'nice_name': 'Disposable TLS fixture'}, 'POST')
        cert_path, key_path = root / 'leaf.pem', root / 'key.pem'
        def upload_certificate(record=certificate, leaf=cert_path, key=key_path, names=('fixture.example', 'second.example')):
            run('openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '2',
                '-subj', '/CN=' + names[0], '-addext', 'subjectAltName=' + ','.join('DNS:' + domain for domain in names),
                '-keyout', str(key), '-out', str(leaf))
            boundary = 'archy-' + uuid.uuid4().hex
            parts = []
            for field, path in [('certificate', leaf), ('certificate_key', key)]:
                parts.append((f'--{boundary}\r\nContent-Disposition: form-data; name="{field}"; filename="{path.name}"\r\n'
                              'Content-Type: application/octet-stream\r\n\r\n').encode() + path.read_bytes() + b'\r\n')
            body = b''.join(parts) + f'--{boundary}--\r\n'.encode()
            status, _, _ = request(admin + '/api/nginx/certificates/' + str(record['id']) + '/upload',
                                    body, 'POST', {'Authorization': 'Bearer ' + token,
                                                  'Content-Type': 'multipart/form-data; boundary=' + boundary})
            assert status == 200, f'Fixture certificate upload failed ({status})'
        upload_certificate()
        secure_payload = {**payload, 'certificate_id': certificate['id'], 'ssl_forced': True}
        api('/nginx/proxy-hosts/' + str(host['id']), secure_payload, 'PUT')
        assert sync(); time.sleep(.3)
        def secure(headers=None, hostname='fixture.example', cafile=cert_path):
            context = ssl.create_default_context(cafile=str(cafile))
            stream = context.wrap_socket(socket.create_connection(('127.0.0.1', tls_port), timeout=15),
                                         server_hostname=hostname)
            connection = http.client.HTTPConnection(hostname, tls_port, timeout=15)
            connection.sock = stream
            try:
                connection.request('GET', '/', headers={'Host': hostname, **(headers or {})})
                response = connection.getresponse()
                return response.status, response.read()
            finally:
                connection.close()
        # Distinct certificate on the SAME upstream listener. Sharing a TLS
        # session across SNI names causes intermittent certificate mismatch502s.
        other_certificate = api('/nginx/certificates', {'provider': 'other', 'nice_name': 'Different SNI certificate'}, 'POST')
        other_leaf, other_key = root / 'other-leaf.pem', root / 'other-key.pem'
        upload_certificate(other_certificate, other_leaf, other_key, ('other.example',))
        other_host = api('/nginx/proxy-hosts', {**payload, 'domain_names': ['other.example'],
            'certificate_id': other_certificate['id'], 'ssl_forced': True}, 'POST')
        assert sync(); time.sleep(.3)
        for _ in range(20):
            assert secure()[0] == 200, 'First hostname inherited another TLS session'
            assert secure(hostname='other.example', cafile=other_leaf)[0] == 200, 'Second hostname inherited another TLS session'
        print('PASS alternating40 trusted TLS requests across distinct SNI certificates on one NPM listener', flush=True)
        assert public()[0] == 301, 'NPM forced HTTPS was not preserved'
        assert secure()[0] == 200, 'Verified TLS bridge failed or redirected in a loop'
        run('podman', 'exec', name, 'sh', '-c',
            'mkdir -p /data/letsencrypt-acme-challenge/.well-known/acme-challenge && '
            'printf exact-challenge > /data/letsencrypt-acme-challenge/.well-known/acme-challenge/fixture-token')
        challenge = public('/.well-known/acme-challenge/fixture-token')
        assert challenge[0] == 200 and challenge[2] == b'exact-challenge', 'Forced HTTPS intercepted NPM challenge file'
        assert public('/.well-known/acme-challenge/missing-token')[0] == 404
        context = ssl.create_default_context(cafile=str(cert_path))
        with context.wrap_socket(socket.create_connection(('127.0.0.1', tls_port), timeout=15),
                                 server_hostname='fixture.example') as stream:
            stream.sendall(b'GET /relay HTTP/1.1\r\nHost: fixture.example\r\nConnection: Upgrade\r\n'
                           b'Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\n'
                           b'Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n')
            response = b''
            while b'EOSE' not in response:
                chunk = stream.recv(4096)
                if not chunk:
                    break
                response += chunk
            assert b'101 Switching Protocols' in response and b'EOSE' in response, 'WSS upgrade/frame failed through NPM'
        print(f'PASS {layout} active-mount ACME file under forced HTTPS and trusted WSS upgrade/frame through NPM', flush=True)
        upload_certificate()
        assert sync(), 'Certificate replacement did not trigger a host nginx reload'
        time.sleep(.3)
        renewed_status = secure()[0]
        assert renewed_status == 200, f'Certificate replacement TLS returned {renewed_status}'
        print('PASS trusted TLS to/from NPM, forced HTTPS without redirect loop, certificate replacement/reload', flush=True)
        acl_secret = secrets.token_urlsafe(24)
        acl = api('/nginx/access-lists', {'name': 'Fixture ACL', 'satisfy_any': False, 'pass_auth': False,
                  'items': [{'username': 'fixture', 'password': acl_secret}],
                  'clients': [{'directive': 'allow', 'address': '127.0.0.1'},
                              {'directive': 'deny', 'address': 'all'}]}, 'POST')
        api('/nginx/proxy-hosts/' + str(host['id']), {**secure_payload, 'access_list_id': acl['id']}, 'PUT')
        authorization = 'Basic ' + base64.b64encode(('fixture:' + acl_secret).encode()).decode()
        time.sleep(.3)
        assert secure()[0] == 401, 'NPM access-list authentication was bypassed'
        assert secure({'Authorization': authorization})[0] == 200
        api('/nginx/access-lists/' + str(acl['id']), {'name': 'Fixture ACL', 'satisfy_any': False, 'pass_auth': False,
            'items': [{'username': 'fixture', 'password': acl_secret}],
            'clients': [{'directive': 'allow', 'address': '192.168.0.0/16'},
                        {'directive': 'deny', 'address': 'all'}]}, 'PUT')
        time.sleep(.3)
        assert secure({'Authorization': authorization, 'X-Real-IP': '192.168.99.99',
                       'X-Forwarded-For': '192.168.99.99'})[0] == 403, 'Forged source bypassed NPM network ACL'
        print('PASS NPM password and network ACL enforcement through bridge; forged client address rejected', flush=True)
        api('/nginx/proxy-hosts/' + str(host['id']), {**payload, 'certificate_id': 0, 'ssl_forced': False, 'access_list_id': 0}, 'PUT')
        assert sync(); time.sleep(.3)
        api('/nginx/proxy-hosts/' + str(host['id']), {**payload, 'enabled': False}, 'PUT')
        assert sync(); time.sleep(.3)
        assert public()[0] == 404, 'Disabled NPM host remains routed'
        api('/nginx/proxy-hosts/' + str(host['id']), {**payload, 'enabled': True}, 'PUT')
        assert sync(); time.sleep(.3)
        assert public()[0] == 200
        run('podman', 'restart', name)
        restarted_at = time.monotonic()
        # Match the app's declared first-start/restart readiness budget.
        deadline = restarted_at + 180
        observed = {}
        while time.monotonic() < deadline:
            try:
                observed['public_http'] = public()[0]
                observed['admin_http'] = request(admin + '/api/users/me',
                    headers={'Authorization': 'Bearer ' + token})[0]
                if observed['public_http'] == 200 and observed['admin_http'] == 200:
                    break
            except OSError as error:
                observed['transport_error'] = type(error).__name__
            time.sleep(2)
        else:
            state = json.loads(run('podman', 'inspect', name))[0]['State']
            observed.update({key: state.get(key) for key in ['Status', 'ExitCode', 'OOMKilled']})
            raise RuntimeError('NPM restart exceeded the 180-second app budget: ' + json.dumps(observed))
        print(f'PASS NPM restart became ready in {time.monotonic() - restarted_at:.1f}s', flush=True)
        # Exercise the actual Podman failure/rollback primitive with retained
        # NPM state. The fixture upstream owns this port, forcing replacement
        # startup to fail before the old definition may safely be discarded.
        original_id = json.loads(run('podman', 'inspect', name))[0]['Id']
        previous = name + '-previous'
        run('podman', 'stop', '--time', '10', name)
        run('podman', 'rename', name, previous)
        failed = subprocess.run([
            'podman', 'run', '-d', '--name', name, '--pull', 'never',
            '--network', npm_network, '-p', f'127.0.0.1:{upstream_port}:81',
            '--memory', '512m', '-v', f'{data}:/data', '-v', f'{certs}:/etc/letsencrypt',
            NPM_IMAGE,
        ], capture_output=True, timeout=120)
        assert failed.returncode != 0, 'Occupied fixture port did not reject replacement'
        run('podman', 'rm', '-f', '--ignore', name)
        run('podman', 'rename', previous, name)
        run('podman', 'start', name)
        assert json.loads(run('podman', 'inspect', name))[0]['Id'] == original_id
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            try:
                if public()[0] == 200 and request(admin + '/api/users/me',
                    headers={'Authorization': 'Bearer ' + token})[0] == 200:
                    break
            except OSError:
                pass
            time.sleep(2)
        else:
            raise RuntimeError('Original NPM did not recover after rejected replacement')
        print('PASS forced replacement bind failure: original container ID, hosts, DB and authenticated API restored', flush=True)
        api('/nginx/proxy-hosts/' + str(host['id']), method='DELETE')
        assert sync(); time.sleep(.3)
        assert public()[0] == 404, 'Deleted NPM host remains routed'
        print('PASS NPM disable/enable/delete propagation and restart with preserved DB', flush=True)
    finally:
        # An overloaded node can time out removing one fixture. Always attempt
        # the others, then retry failed cleanup instead of leaving them running.
        cleanup = []
        if nginx_started:
            cleanup.append((nginx + ['-s', 'quit'], 15))
        cleanup.extend((['podman', 'rm', '-f', '--ignore', '--time', '3', container], 90)
                       for container in [name, name + '-previous', backend])
        if acme:
            cleanup.extend((['podman', 'rm', '-f', '--ignore', '--time', '3', container], 90)
                           for container in [acme.ca_name, acme.dns_name])
        if network_created:
            cleanup.append((['podman', 'network', 'rm', network], 30))
        # The fixture may contain rootless-mapped nginx ownership.
        cleanup.append((['podman', 'unshare', 'rm', '-rf', str(data), str(certs)], 30))
        failed = []
        for args, limit in cleanup:
            try:
                if subprocess.run(args, capture_output=True, timeout=limit).returncode:
                    failed.append((args, limit))
            except subprocess.TimeoutExpired:
                failed.append((args, limit))
        for args, limit in failed:
            subprocess.run(args, capture_output=True, timeout=limit, check=True)
