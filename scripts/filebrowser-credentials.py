#!/usr/bin/env python3
"""Provision File Browser's private Cloud account before the server starts.

Runs only with File Browser stopped. Uses its pinned image/CLI, backs up its
actual database, verifies login in a network-isolated container, then atomically
publishes the credential record. Never prints credentials or raw CLI output.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import sys
import tempfile


class ProvisionError(Exception):
    pass


def credentials(value):
    if not isinstance(value, dict) or value.get('schema') != 1:
        raise ProvisionError('Unsupported Cloud credential record')
    if not re.fullmatch(r'archy-[0-9a-f]{32}', value.get('username', '')):
        raise ProvisionError('Invalid managed Cloud username')
    if not re.fullmatch(r'[0-9a-f]{64}', value.get('password', '')):
        raise ProvisionError('Invalid managed Cloud password')
    legacy = value.get('legacy_admin_password')
    if legacy is not None and not re.fullmatch(r'[0-9a-f]{64}', legacy):
        raise ProvisionError('Invalid legacy recovery password')
    return value


def atomic_text(path, text):
    if path.is_symlink():
        raise ProvisionError('Refusing symlink credential file')
    fd, tmp = tempfile.mkstemp(prefix='.credential-', dir=path.parent)
    try:
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, 'w') as stream:
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(tmp, path)
        directory = os.open(path.parent, os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def atomic_json(path, value):
    atomic_text(path, json.dumps(value))


def database_path(data):
    config = data / '.filebrowser.json'
    if config.is_symlink():
        raise ProvisionError('Refusing symlink File Browser config')
    if config.exists():
        database = json.loads(config.read_text()).get('database')
        if not isinstance(database, str) or not re.fullmatch(r'/data/[A-Za-z0-9_.-]+\.db', database):
            raise ProvisionError('Unsupported File Browser database path; preserve it for manual review')
    else:
        existing = [name for name in ['filebrowser.db', 'database.db'] if (data / name).exists()]
        if len(existing) > 1:
            raise ProvisionError('Multiple File Browser databases without a selected config')
        database = '/data/' + (existing[0] if existing else 'filebrowser.db')
    if (data / database.removeprefix('/data/')).is_symlink():
        raise ProvisionError('Refusing symlink File Browser database')
    return database


# All variable input is passed in argv/environment, never interpolated into shell
# code. CLI output may contain sensitive state, so it stays inside the helper.
BOOTSTRAP = r'''
set -eu
umask 077
db="$1"
backup="/data/.archy-auth-backup-$2"
server_pid=""
had_db=0
had_config=0
success=0
changed=0
stop_server() {
    if [ -n "$server_pid" ]; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
        server_pid=""
    fi
}
finish() {
    stop_server
    if [ "$success" != 1 ]; then
        if [ "$had_db" = 1 ]; then cp -p "$backup/database" "$db"; else rm -f "$db"; fi
        if [ "$had_config" = 1 ]; then cp -p "$backup/config" /data/.filebrowser.json; else rm -f /data/.filebrowser.json; fi
    elif [ "$changed" = 0 ]; then
        rm -f "$backup/database" "$backup/config"
        rmdir "$backup"
    fi
}
mkdir -m 700 "$backup"
if [ -f "$db" ]; then cp -p "$db" "$backup/database"; had_db=1; fi
if [ -f /data/.filebrowser.json ]; then cp -p /data/.filebrowser.json "$backup/config"; had_config=1; fi
trap finish EXIT
trap 'exit 1' INT TERM
if [ ! -f /data/.filebrowser.json ]; then
    printf '{"port":80,"baseURL":"","address":"0.0.0.0","database":"%s","root":"/srv","log":"stdout"}\n' "$db" > /data/.filebrowser.json
    chmod 644 /data/.filebrowser.json
fi
cli() { filebrowser "$@" --database "$db" >/tmp/setup.out 2>&1; }
if [ "$had_db" = 0 ]; then
    changed=1
    mkdir -p /srv/Documents /srv/Photos /srv/Music /srv/Downloads /srv/Builds
    cli config init --root /srv --auth.method=json
    cli users add "$FB_CLOUD_USERNAME" "$FB_CLOUD_PASSWORD" --perm.admin --perm.execute=false --scope . --lockPassword
fi
cli config export /tmp/settings.json
if grep -Eq '"authMethod"[[:space:]]*:[[:space:]]*"noauth"' /tmp/settings.json; then
    changed=1
    cli config set --auth.method=json
elif ! grep -Eq '"authMethod"[[:space:]]*:[[:space:]]*"json"' /tmp/settings.json; then
    echo 'Unsupported custom File Browser authentication method' >&2
    exit 1
fi
printf '{"username":"%s","password":"%s"}' "$FB_CLOUD_USERNAME" "$FB_CLOUD_PASSWORD" >/tmp/cloud-login.json
printf '{"username":"admin","password":"admin"}' >/tmp/default-login.json
printf '{"username":"admin","password":"%s"}' "$FB_LEGACY_PASSWORD" >/tmp/recovery-login.json
start_server() {
    filebrowser --database "$db" --root /srv --address 127.0.0.1 --port 18080 >/tmp/server.out 2>&1 &
    server_pid=$!
    attempts=0
    until wget -q -O /dev/null http://127.0.0.1:18080/health; do
        attempts=$((attempts + 1))
        [ "$attempts" -lt 30 ] && kill -0 "$server_pid" || return 1
        sleep 0.2
    done
}
login() { wget -q -O /tmp/login-token --header='Content-Type: application/json' --post-file="$1" http://127.0.0.1:18080/api/login 2>/dev/null && [ -s /tmp/login-token ]; }
cloud_root() {
    token=$(tr -d '\"' </tmp/login-token)
    wget -q -O /tmp/cloud-root --header="X-Auth: $token" http://127.0.0.1:18080/api/resources/ 2>/dev/null
}
start_server
if ! { login /tmp/cloud-login.json && cloud_root; }; then
    changed=1
    stop_server
    if cli users find "$FB_CLOUD_USERNAME"; then
        cli users update "$FB_CLOUD_USERNAME" --password "$FB_CLOUD_PASSWORD" --scope . --perm.admin --perm.execute=false --lockPassword
    else
        cli users add "$FB_CLOUD_USERNAME" "$FB_CLOUD_PASSWORD" --perm.admin --perm.execute=false --scope . --lockPassword
    fi
    start_server
    login /tmp/cloud-login.json
fi
# Only rotate a proven default login. Preserve custom administrator credentials,
# all user IDs, scopes, permissions and shared links.
if login /tmp/default-login.json; then
    changed=1
    stop_server
    cli users update admin --password "$FB_LEGACY_PASSWORD"
    start_server
    login /tmp/cloud-login.json
    if login /tmp/default-login.json; then exit 1; fi
fi
# noauth must not masquerade as a successful private Cloud login.
if wget -q -O /dev/null http://127.0.0.1:18080/api/resources/ 2>/dev/null; then exit 1; fi
if login /tmp/recovery-login.json; then echo DEFAULT_ADMIN_ROTATED; fi
login /tmp/cloud-login.json
cloud_root
stop_server
success=1
'''


def prepare(args):
    data = Path(args.data_dir).absolute()
    secret_dir = Path(args.secrets_dir).absolute()
    if data.is_symlink() or secret_dir.is_symlink():
        raise ProvisionError('Refusing symlink provisioning directories')
    if not data.is_dir():
        raise ProvisionError('Create File Browser storage with the runtime UID before provisioning')
    secret_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    record = secret_dir / 'credentials.json'
    pending = secret_dir / 'credentials.pending.json'
    with (secret_dir / '.provision.lock').open('a') as lock:
        os.chmod(lock.name, 0o600)
        fcntl.flock(lock, fcntl.LOCK_EX)
        result = subprocess.run([args.runtime, 'inspect', args.container], capture_output=True, text=True)
        if result.returncode == 0 and json.loads(result.stdout)[0]['State']['Running']:
            raise ProvisionError('File Browser must be stopped through its managed service before provisioning')
        database = database_path(data)
        if record.exists() or pending.exists():
            path = record if record.exists() else pending
            if path.is_symlink():
                raise ProvisionError('Refusing symlink credential file')
            value = credentials(json.loads(path.read_text()))
        else:
            value = {'schema': 1, 'username': 'archy-' + secrets.token_hex(16), 'password': secrets.token_hex(32)}
            atomic_json(pending, value)
        # Keep the rotated default admin password private for recovery. The Cloud
        # account is separate; administrator identity and existing shares survive.
        value.setdefault('legacy_admin_password', secrets.token_hex(32))
        atomic_json(pending, value)
        nonce = secrets.token_hex(8)
        env = {**os.environ, 'FB_CLOUD_USERNAME': value['username'], 'FB_CLOUD_PASSWORD': value['password'], 'FB_LEGACY_PASSWORD': value['legacy_admin_password']}
        command = [args.runtime, 'run', '--rm', '--network', 'none', '--pull', 'never',
                   '--name', 'credential_' + ''.join(secrets.choice('abcdefghijklmnopqrstuvwxyz') for _ in range(20)),
                   '--security-opt', 'no-new-privileges:true', '--cap-drop', 'ALL',
                   # Match the managed server's storage access. Legacy manifests
                   # use host UID100000, which need not map to the image's UID.
                   # Preserve ownership instead of recursively chowning user files.
                   '--cap-add', 'DAC_OVERRIDE',
                   '--tmpfs', '/tmp:rw,noexec,nosuid,size=32m',
                   '-v', str(data) + ':/data:rw', '-v', str(Path(args.srv_root).absolute()) + ':/srv:rw',
                   '--env', 'FB_CLOUD_USERNAME', '--env', 'FB_CLOUD_PASSWORD', '--env', 'FB_LEGACY_PASSWORD',
                   '--entrypoint', '/bin/sh', args.image, '-ec', BOOTSTRAP, 'setup', database, nonce]
        try:
            result = subprocess.run(command, env=env, capture_output=True, timeout=90)
        except subprocess.TimeoutExpired:
            # Do not print subprocess arguments/environment: they may contain
            # private data. SIGTERM lets the helper restore the DB before exit.
            subprocess.run([args.runtime, 'stop', '--time', '15', command[command.index('--name') + 1]], capture_output=True)
            raise ProvisionError('File Browser credential setup timed out; inspect private backup before retrying') from None
        if result.returncode:
            raise ProvisionError('File Browser credential setup failed; prior DB/config restored when possible, backup retained')
        if b'DEFAULT_ADMIN_ROTATED' in result.stdout:
            value['legacy_admin_rotated'] = True
            atomic_json(pending, value)
            legacy = secret_dir / 'password'
            previous = secret_dir / 'password.before-secure-cloud'
            if legacy.is_symlink() or previous.is_symlink():
                raise ProvisionError('Refusing symlink legacy credential')
            if legacy.exists() and not previous.exists():
                atomic_text(previous, legacy.read_text())
            # Preserve old-backend Cloud access if an OTA is rolled back.
            atomic_text(legacy, value['legacy_admin_password'])
        atomic_json(record, value)
        pending.unlink(missing_ok=True)
        print('File Browser Cloud credentials verified; existing files and users preserved.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', required=True)
    parser.add_argument('--runtime', choices=['podman', 'docker'], default='podman')
    parser.add_argument('--container', default='filebrowser')
    parser.add_argument('--data-dir', default='/var/lib/archipelago/filebrowser-data')
    parser.add_argument('--srv-root', default='/var/lib/archipelago/filebrowser')
    parser.add_argument('--secrets-dir', default='/var/lib/archipelago/secrets/filebrowser')
    args = parser.parse_args()
    try:
        prepare(args)
    except (ProvisionError, OSError, ValueError) as error:
        print('File Browser credential setup: ' + str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
