"""Local Pebble ACME authority for real NPM issuance/renewal tests only.

No production CA requests, DNS changes, or system trust-store modifications.
See https://github.com/letsencrypt/pebble for the test server's protocol/limits.
"""
import hashlib
import http.client
import json
import socket
import ssl
import subprocess
import time
import urllib.request
import uuid


class AcmeFixture:
    def __init__(self, root, http_port, run, port):
        self.root = root / 'acme'
        self.root.mkdir(mode=0o700)
        self.http_port, self.run = http_port, run
        self.api_port, self.management_port, self.dns_management = port(), port(), port()
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as probe:
            probe.bind(('127.0.0.1', 0))
            self.dns_port = probe.getsockname()[1]
        suffix = ''.join(chr(ord('a') + int(char, 16)) for char in uuid.uuid4().hex)
        self.ca_name, self.dns_name = 'credential_acme' + suffix, 'credential_dns' + suffix
        self.root_pem = b''

    def start(self):
        self.run('openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '2',
                 '-subj', '/CN=acme-fixture.test',
                 '-addext', 'subjectAltName=DNS:acme-fixture.test,IP:127.0.0.1',
                 '-addext', 'basicConstraints=critical,CA:TRUE',
                 '-keyout', str(self.root/'api-key.pem'), '-out', str(self.root/'api-cert.pem'))
        config = {'pebble': {
            'listenAddress': f'127.0.0.1:{self.api_port}',
            'managementListenAddress': f'127.0.0.1:{self.management_port}',
            'certificate': '/fixture/api-cert.pem', 'privateKey': '/fixture/api-key.pem',
            'httpPort': self.http_port, 'tlsPort': 5001,
            'externalAccountBindingRequired': False,
            'retryAfter': {'authz': 1, 'order': 1}}}
        (self.root/'pebble.json').write_text(json.dumps(config))
        self.run('podman', 'run', '-d', '--name', self.dns_name, '--network', 'host', '--memory', '128m',
                 'ghcr.io/letsencrypt/pebble-challtestsrv:latest',
                 '-dnsserver', f'127.0.0.1:{self.dns_port}', '-management', f'127.0.0.1:{self.dns_management}',
                 '-http01', '', '-https01', '', '-tlsalpn01', '', '-doh', '',
                 '-defaultIPv4', '127.0.0.1', '-defaultIPv6', '')
        self.run('podman', 'run', '-d', '--name', self.ca_name, '--network', 'host', '--memory', '192m',
                 '-e', 'PEBBLE_VA_NOSLEEP=1', '-e', 'PEBBLE_AUTHZREUSE=0',
                 '-e', 'PEBBLE_WFE_NONCEREJECT=0',
                 '-v', str(self.root)+':/fixture:ro', 'ghcr.io/letsencrypt/pebble:latest',
                 '-config', '/fixture/pebble.json', '-dnsserver', f'127.0.0.1:{self.dns_port}', '-strict=false')
        context = ssl.create_default_context(cafile=str(self.root/'api-cert.pem'))
        deadline = time.monotonic() + 30
        while True:
            try:
                with urllib.request.urlopen(f'https://127.0.0.1:{self.management_port}/roots/0',
                                            context=context, timeout=5) as response:
                    self.root_pem = response.read()
                assert b'BEGIN CERTIFICATE' in self.root_pem
                (self.root/'root-ca.pem').write_bytes(self.root_pem)
                break
            except OSError:
                if time.monotonic() >= deadline:
                    raise RuntimeError('Local ACME authority did not become ready') from None
                time.sleep(.2)

    def npm_args(self):
        # Explicit slirp host-loopback gateway: Podman's automatic host alias
        # can resolve to a LAN address where the loopback-only CA does not listen.
        return ['--add-host', 'acme-fixture.test:169.254.1.2',
                '-e', f'LE_SERVER=https://acme-fixture.test:{self.api_port}/dir',
                '-e', 'REQUESTS_CA_BUNDLE=/acme-fixture/api-cert.pem',
                '-v', str(self.root)+':/acme-fixture:ro']

    def verify(self, api, sync, public, payload, tls_port, log):
        # Issue before creating this NPM host: challenge must use the default
        # ACME location, without making the management vhost publicly available.
        domain = 'prehost.example'
        certificate = api('/nginx/certificates', {
            'provider': 'letsencrypt', 'domain_names': [domain],
            'meta': {'dns_challenge': False}}, 'POST')
        assert certificate['provider'] == 'letsencrypt'
        host = api('/nginx/proxy-hosts', {
            **payload, 'domain_names': [domain], 'certificate_id': certificate['id'], 'ssl_forced': True}, 'POST')
        assert sync()
        time.sleep(.4)
        context = ssl.create_default_context(cafile=str(self.root/'root-ca.pem'))
        def served():
            stream = context.wrap_socket(socket.create_connection(('127.0.0.1', tls_port), timeout=15),
                                         server_hostname=domain)
            fingerprint = hashlib.sha256(stream.getpeercert(binary_form=True)).hexdigest()
            connection = http.client.HTTPConnection(domain, tls_port, timeout=15)
            connection.sock = stream
            try:
                connection.request('GET', '/', headers={'Host': domain})
                response = connection.getresponse()
                assert response.status == 200, 'Issued certificate route did not reach the app'
                response.read()
                return fingerprint
            finally:
                connection.close()
        before = served()
        assert public(host=domain)[0] == 301
        initial_challenges = log.read_text().count('/.well-known/acme-challenge/')
        assert initial_challenges > 0, 'No actual ACME HTTP validation reached the bridge'
        api(f'/nginx/certificates/{certificate["id"]}/renew', {}, 'POST')
        assert sync(), 'Renewed certificate did not trigger bridge reload'
        time.sleep(.4)
        assert served() != before, 'Public TLS still serves the pre-renewal certificate'
        assert log.read_text().count('/.well-known/acme-challenge/') > initial_challenges
        assert public('/rpc/v1', host='unknown.example')[0] == 404
        print('PASS actual local Pebble ACME: pre-host issuance, HTTP validation, forced-HTTPS renewal, new served certificate, unknown management route404', flush=True)
        api(f'/nginx/proxy-hosts/{host["id"]}', method='DELETE')
        assert sync()

    def close(self):
        for name in [self.ca_name, self.dns_name]:
            subprocess.run(['podman', 'rm', '-f', '--ignore', '--time', '3', name],
                           check=True, capture_output=True, timeout=90)
