#!/usr/bin/env python3
"""Disposable real-image credential migration acceptance. No live DB mounts."""
import hashlib
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile
import time
import urllib.request
import urllib.error

REPO = Path(__file__).resolve().parents[2]
IMAGE = os.environ.get('FILEBROWSER_TEST_IMAGE', 'source.archipelago-foundation.org/lfg2025/filebrowser:v2.63.23')


def command(*args, **kwargs):
    return subprocess.run(list(args), check=True, capture_output=True, **kwargs)


def request(port, path, method='GET', data=None, token=None):
    headers = {'Content-Type': 'application/json'}
    if token:
        headers['X-Auth'] = token
    req = urllib.request.Request(f'http://127.0.0.1:{port}{path}', data=json.dumps(data).encode() if data is not None else None, method=method, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=5) as response:
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        return error.code, b''


def test(case):
    root = Path(tempfile.mkdtemp(prefix='archy-fb-acceptance-'))
    name = 'credential_' + ''.join(secrets.choice('abcdefghijklmnopqrstuvwxyz') for _ in range(20))
    for folder in ['data', 'srv', 'secrets']:
        (root / folder).mkdir(mode=0o700 if folder == 'secrets' else 0o755)
    command('podman', 'unshare', 'chown', '1000:1000', str(root/'data'), str(root/'srv'))
    mount = ['-v', str(root/'data')+':/data', '-v', str(root/'srv')+':/srv']
    def cli(*args):
        return command('podman', 'run', '--rm', '--network', 'none', *mount, '--entrypoint', 'filebrowser', IMAGE, *args, '--database', '/data/database.db')
    def start():
        command('podman', 'run', '-d', '--name', name, '--cap-drop', 'ALL', '--cap-add', 'NET_BIND_SERVICE', '--cap-add', 'DAC_OVERRIDE', '-p', '127.0.0.1::80', *mount, IMAGE, '--config', '/data/.filebrowser.json', '--port', '80')
        inspect = json.loads(command('podman', 'inspect', name).stdout)[0]
        port = inspect['NetworkSettings']['Ports']['80/tcp'][0]['HostPort']
        for _ in range(60):
            try:
                if request(port, '/health')[0] == 200:
                    return port
            except OSError:
                pass
            time.sleep(.2)
        info = json.loads(command('podman', 'inspect', name).stdout)[0]
        print('Fixture state:', {k:info['State'].get(k) for k in ['Status', 'ExitCode', 'Error']})
        logs = command('podman', 'logs', name)
        print((logs.stdout + logs.stderr).decode()[-1400:])
        raise AssertionError('Fixture server did not become ready')
    def stop():
        subprocess.run(['podman', 'rm', '-f', name], capture_output=True)
    try:
        prior_users = None
        if case != 'fresh':
            cli('config', 'init', '--root', '/srv', '--minimum-password-length', '3', '--auth.method', 'json')
            admin_password = 'admin' if case in ['legacy-default', 'legacy-noauth'] else secrets.token_hex(16)
            cli('users', 'add', 'admin', 'initial-fixture-password' if case in ['legacy-default', 'legacy-noauth'] else admin_password, '--perm.admin')
            if case in ['legacy-default', 'legacy-noauth']:
                # Current File Browser refuses creating weak passwords; import
                # a real bcrypt hash to reproduce an older admin/admin DB.
                legacy_hash = cli('hash', 'admin').stdout.decode().strip()
                assert legacy_hash.startswith('$2')
                cli('users', 'export', '/data/legacy-fixture.json')
                command('podman', 'unshare', 'python3', '-c', 'import json,pathlib,sys;p=pathlib.Path(sys.argv[1]);u=json.loads(p.read_text());u[0]["password"]=sys.argv[2];p.write_text(json.dumps(u))', str(root/'data'/'legacy-fixture.json'), legacy_hash)
                cli('users', 'import', '/data/legacy-fixture.json', '--overwrite')
            cli('users', 'add', 'existing-owner', 'fixture-owner-password', '--perm.admin=false', '--perm.delete=false')
            config = {'root':'/srv','database':'/data/database.db','address':'0.0.0.0','port':80}
            command('podman', 'unshare', 'python3', '-c', 'import pathlib,sys;pathlib.Path(sys.argv[1]).write_text(sys.argv[2]);pathlib.Path(sys.argv[1]).chmod(0o644)', str(root/'data'/'.filebrowser.json'), json.dumps(config))
            port = start()
            code, token = request(port, '/api/login', 'POST', {'username':'admin','password':admin_password})
            assert code == 200
            token = token.decode().strip('"')
            code, users = request(port, '/api/users', token=token)
            assert code == 200
            prior_users = json.loads(users)
            (root/'secrets'/'password').write_text(admin_password)
            stop()
        command('podman', 'unshare', 'python3', '-c', 'import pathlib,sys;pathlib.Path(sys.argv[1]).write_bytes(b"preserve original file bytes")', str(root/'srv'/'preserve.txt'))
        prepare = ['python3', str(REPO/'scripts/filebrowser-credentials.py'), '--image', IMAGE, '--container', name, '--data-dir', str(root/'data'), '--srv-root', str(root/'srv'), '--secrets-dir', str(root/'secrets')]
        if case == 'legacy-noauth':
            cli('config', 'set', '--auth.method=noauth')
        if case == 'legacy-manifest-owner':
            command('podman', 'unshare', 'chown', '-R', '1:1', str(root/'data'), str(root/'srv'))
            owner_before = (root/'data').stat().st_uid
        if case == 'rollback':
            # Force failure AFTER noauth has been migrated to json: creating the
            # managed account must respect a stricter operator password policy.
            cli('config', 'set', '--minimum-password-length', '128', '--auth.method=noauth')
            before = hashlib.sha256(command('podman', 'unshare', 'cat', str(root/'data'/'database.db')).stdout).digest()
            result = subprocess.run(prepare, capture_output=True)
            assert result.returncode != 0 and not (root/'secrets'/'credentials.json').exists()
            after = hashlib.sha256(command('podman', 'unshare', 'cat', str(root/'data'/'database.db')).stdout).digest()
            assert before == after, 'Failed migration did not restore original database'
            print('PASS forced account-policy failure after auth migration restores exact DB and does not publish credentials')
            return
        command(*prepare)
        if case == 'legacy-manifest-owner':
            assert (root/'data').stat().st_uid == owner_before
            assert (root/'srv'/'preserve.txt').stat().st_uid == owner_before
        record = json.loads((root/'secrets'/'credentials.json').read_text())
        assert record['username'] != 'admin' and len(record['password']) == 64
        assert (root/'secrets'/'credentials.json').stat().st_mode & 0o777 == 0o600
        if case in ['legacy-default', 'legacy-noauth']:
            assert (root/'secrets'/'password').read_text() == record['legacy_admin_password']
            assert (root/'secrets'/'password.before-secure-cloud').read_text() == 'admin'
        for cycle in range(2):
            port = start()
            assert request(port, '/api/login', 'POST', {'username':'admin','password':'admin'})[0] == 403
            assert request(port, '/api/resources/')[0] == 401
            code, token = request(port, '/api/login', 'POST', {key:record[key] for key in ['username','password']})
            assert code == 200
            token = token.decode().strip('"')
            assert request(port, '/api/raw/preserve.txt', token=token) == (200, b'preserve original file bytes')
            code, users = request(port, '/api/users', token=token)
            assert code == 200
            users = json.loads(users)
            assert len([u for u in users if u['username'] == record['username']]) == 1
            if prior_users:
                for prior in prior_users:
                    current = next(u for u in users if u['id'] == prior['id'])
                    assert current == prior, 'Existing user attributes changed'
                assert request(port, '/api/login', 'POST', {'username':'existing-owner','password':'fixture-owner-password'})[0] == 200
                if case == 'legacy-custom':
                    assert request(port, '/api/login', 'POST', {'username':'admin','password':admin_password})[0] == 200
            stop()
            if cycle == 0:
                command(*prepare)
                assert json.loads((root/'secrets'/'credentials.json').read_text()) == record
        print('PASS', case, 'unique credentials, rejected admin/admin, private access, exact files, accounts/permissions preserved, repeat/restart stable')
    finally:
        stop()
        assert root.name.startswith('archy-fb-acceptance-') and root.parent == Path('/tmp')
        command('podman', 'unshare', 'rm', '-rf', str(root))


if __name__ == '__main__':
    for case in ['fresh', 'legacy-default', 'legacy-custom', 'legacy-noauth', 'legacy-manifest-owner', 'rollback']:
        test(case)
