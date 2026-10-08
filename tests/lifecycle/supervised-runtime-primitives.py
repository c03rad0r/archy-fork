#!/usr/bin/env python3
"""Qualify real AutoRemove/Quadlet recovery primitives using owned fixtures only.

This does not replace full app-specific drain or production updater acceptance.
No app volume, port, wallet, catalog, or installed unit is used.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import time
import uuid


def run(*args, check=True, timeout=90):
    result = subprocess.run(args, check=False, timeout=timeout, capture_output=True, text=True)
    if check and result.returncode:
        raise RuntimeError(f'{args[0]} failed ({result.returncode}): {result.stderr.strip()}')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', required=True, help='Already imported local image containing /bin/sh')
    args = parser.parse_args()
    image = run('podman', 'image', 'inspect', '--format', '{{.Id}}', args.image).stdout.strip()
    operation = str(uuid.uuid4())
    name = 'archy-recovery-fixture-' + operation
    tag = 'localhost/archy-update-recovery:' + operation + '-0'
    root = Path(tempfile.mkdtemp(prefix=name + '-'))
    data = root / 'data'
    data.mkdir()
    units = Path.home() / '.config/containers/systemd'
    units.mkdir(parents=True, exist_ok=True)
    unit = units / (name + '.container')
    assert not unit.exists()
    service = name + '.service'
    snapshot = None
    original = f'''[Container]
Image=sha256:{image.removeprefix('sha256:')}
ContainerName={name}
Network=none
Pull=never
Volume={data}:/state
Environment=MODE=original
Entrypoint=/bin/sh
Exec=-c "trap 'exit 0' TERM; while sleep 1; do :; done"

[Service]
Restart=no
TimeoutStartSec=60
TimeoutStopSec=15
'''
    def write(body):
        temporary = unit.with_suffix('.next')
        temporary.write_text(body)
        temporary.chmod(0o600)
        temporary.replace(unit)
        run('systemctl', '--user', 'daemon-reload')
    def inspect():
        rows = json.loads(run('podman', 'inspect', name).stdout)
        assert len(rows) == 1 and rows[0]['Name'] == name
        return rows[0]
    try:
        write(original)
        run('systemctl', '--user', 'start', service)
        old = inspect()
        assert old['State']['Running'] and old['HostConfig']['AutoRemove']
        run('podman', 'exec', name, '/bin/sh', '-c',
            'printf original-layer > /original-layer; printf persistent-bytes > /state/value')
        volume_hash = hashlib.sha256((data / 'value').read_bytes()).hexdigest()
        run('podman', 'commit', '--pause=true', '--include-volumes=false',
            '--change', 'LABEL io.archipelago.recovery.operation=' + operation,
            '--change', 'LABEL io.archipelago.recovery.container=' + old['Id'], old['Id'], tag)
        captured = json.loads(run('podman', 'image', 'inspect', tag).stdout)[0]
        snapshot = captured['Id']
        assert captured['Config']['Labels']['io.archipelago.recovery.operation'] == operation
        assert captured['Config']['Labels']['io.archipelago.recovery.container'] == old['Id']
        run('podman', 'run', '--rm', '--network=none', '--pull=never', '--entrypoint=/bin/sh', snapshot,
            '-c', 'test "$(cat /original-layer)" = original-layer; test ! -f /state/value')
        run('systemctl', '--user', 'stop', service)
        assert run('podman', 'container', 'exists', old['Id'], check=False).returncode == 1
        failed = original.replace('Environment=MODE=original', 'Environment=MODE=candidate').replace(
            'Exec=-c "trap \'exit 0\' TERM; while sleep 1; do :; done"', 'Exec=-c "exit 77"')
        assert failed != original
        write(failed)
        run('systemctl', '--user', 'start', service, check=False)
        deadline = time.monotonic() + 15
        while run('systemctl', '--user', 'is-active', service, check=False).returncode == 0:
            assert time.monotonic() < deadline, 'Fault injection unexpectedly remained active'
            time.sleep(0.2)
        assert run('systemctl', '--user', 'show', service, '--property=Result', '--value').stdout.strip() != 'success'
        run('systemctl', '--user', 'stop', service, check=False)
        restored = original.replace('Image=sha256:' + image.removeprefix('sha256:'),
                                    'Image=sha256:' + snapshot.removeprefix('sha256:'))
        write(restored)
        run('systemctl', '--user', 'reset-failed', service, check=False)
        run('systemctl', '--user', 'start', service)
        current = inspect()
        assert current['State']['Running'] and current['Id'] != old['Id']
        assert current['Image'].removeprefix('sha256:') == snapshot.removeprefix('sha256:')
        assert 'MODE=original' in current['Config']['Env']
        assert unit.read_text() == restored and unit.stat().st_mode & 0o777 == 0o600
        assert run('podman', 'exec', name, 'cat', '/original-layer').stdout == 'original-layer'
        assert hashlib.sha256((data / 'value').read_bytes()).hexdigest() == volume_hash
        print(json.dumps({'auto_remove_recovery': 'passed', 'writable_layer_preserved': True,
                          'volume_bytes_preserved': True, 'original_configuration_restored': True,
                          'injected_target_failure': True, 'network': 'none',
                          'full_supervised_application_cutover': 'not tested'}))
    finally:
        run('systemctl', '--user', 'stop', service, check=False)
        unit.unlink(missing_ok=True)
        unit.with_suffix('.next').unlink(missing_ok=True)
        run('systemctl', '--user', 'daemon-reload')
        run('systemctl', '--user', 'reset-failed', service, check=False)
        run('podman', 'rm', '-f', name, check=False)
        if snapshot:
            run('podman', 'rmi', tag)
        assert root.parent == Path('/tmp') and root.name.startswith(name + '-')
        run('podman', 'unshare', 'rm', '-rf', str(root))


if __name__ == '__main__':
    main()
