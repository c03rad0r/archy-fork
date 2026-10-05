#!/usr/bin/env python3
"""Exercise the credential pre-start hook through a disposable real Quadlet."""
import importlib.util
import json
from pathlib import Path
import secrets
import socket
import subprocess
import tempfile
import time

spec = importlib.util.spec_from_file_location('fixture', Path(__file__).with_name('filebrowser-credentials.py'))
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
run = fixture.command
name = 'credential_' + ''.join(secrets.choice('abcdefghijklmnopqrstuvwxyz') for _ in range(20))
root = Path(tempfile.mkdtemp(prefix='archy-fb-quadlet-'))
units = Path.home() / '.config/containers/systemd'
units.mkdir(parents=True, exist_ok=True)
unit = units / (name + '.container')
assert not unit.exists()
with socket.socket() as probe:
    probe.bind(('127.0.0.1', 0))
    port = probe.getsockname()[1]
try:
    for folder in ['data', 'srv', 'secrets']:
        (root / folder).mkdir(mode=0o700 if folder == 'secrets' else 0o755)
    # Reproduce the shipped manifest's legacy storage owner, rather than
    # pre-aligning fixture ownership to the image user and hiding migration bugs.
    run('podman', 'unshare', 'chown', '1:1', str(root/'data'), str(root/'srv'))
    helper = fixture.REPO / 'scripts/filebrowser-credentials.py'
    unit.write_text(f'''[Container]
Image={fixture.IMAGE}
ContainerName={name}
PublishPort=127.0.0.1:{port}:80
Volume={root}/data:/data
Volume={root}/srv:/srv
DropCapability=all
AddCapability=NET_BIND_SERVICE
AddCapability=DAC_OVERRIDE
NoNewPrivileges=true
Exec=--config /data/.filebrowser.json

[Service]
ExecStartPre=/usr/bin/python3 {helper} --image {fixture.IMAGE} --container {name} --data-dir {root}/data --srv-root {root}/srv --secrets-dir {root}/secrets
TimeoutStartSec=150
Restart=no
''')
    run('systemctl', '--user', 'daemon-reload')
    previous = None
    for cycle in range(2):
        run('systemctl', '--user', 'start' if cycle == 0 else 'restart', name + '.service')
        record = json.loads((root/'secrets/credentials.json').read_text())
        if previous is not None:
            assert record == previous, 'Service restart changed the managed account'
        previous = record
        deadline = time.monotonic() + 30
        while True:
            try:
                code, token = fixture.request(port, '/api/login', 'POST', {key:record[key] for key in ['username', 'password']})
                if code == 200:
                    break
            except OSError:
                pass
            assert time.monotonic() < deadline, 'Service did not provide Cloud login'
            time.sleep(.2)
        assert fixture.request(port, '/api/resources/', token=token.decode().strip('"'))[0] == 200
        assert fixture.request(port, '/api/resources/')[0] == 401
        assert fixture.request(port, '/api/login', 'POST', {'username':'admin', 'password':'admin'})[0] == 403
    print('PASS actual Quadlet pre-start, fresh secure login, managed restart, stable account, rejected anonymous/default access')
finally:
    subprocess.run(['systemctl', '--user', 'stop', name + '.service'], capture_output=True)
    unit.unlink(missing_ok=True)
    subprocess.run(['systemctl', '--user', 'daemon-reload'], capture_output=True)
    subprocess.run(['systemctl', '--user', 'reset-failed', name + '.service'], capture_output=True)
    subprocess.run(['podman', 'rm', '-f', name], capture_output=True)
    assert root.name.startswith('archy-fb-quadlet-') and root.parent == Path('/tmp')
    run('podman', 'unshare', 'rm', '-rf', str(root))
