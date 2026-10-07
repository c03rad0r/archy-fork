#!/usr/bin/env python3
"""Exercise rollback commitments on an owned, network-isolated PostgreSQL.

Requires an already imported image: --image IMAGE. Never mounts node volumes,
publishes ports, or invokes the maintenance entrypoint against installed apps.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import time
import uuid

MODULE = Path(__file__).resolve().parents[2] / 'scripts/indeehub-maintenance-controller.py'
spec = importlib.util.spec_from_file_location('maintenance', MODULE)
maintenance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(maintenance)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', required=True)
    args = parser.parse_args()
    image = subprocess.check_output(
        ['podman', 'image', 'inspect', '--format', '{{.Id}}', args.image], text=True,
    ).strip()
    name = 'archy-maintenance-sql-' + uuid.uuid4().hex
    container = None
    try:
        container = subprocess.check_output([
            'podman', 'run', '-d', '--pull=never', '--network=none', '--name', name,
            '--tmpfs', '/var/lib/postgresql/data:rw',
            '-e', 'POSTGRES_HOST_AUTH_METHOD=trust', '-e', 'POSTGRES_USER=indeedhub',
            '-e', 'POSTGRES_DB=indeedhub', image,
        ], text=True).strip()
        deadline = time.monotonic() + 60
        while subprocess.run(['podman', 'exec', container, 'pg_isready', '-U', 'indeedhub'],
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
            if time.monotonic() > deadline:
                raise RuntimeError('Disposable PostgreSQL did not become ready')
            time.sleep(0.5)

        def sql(statement, database='indeedhub'):
            return subprocess.check_output([
                'podman', 'exec', '-i', container, 'psql', '-XqAt',
                '--set=ON_ERROR_STOP=1', '-U', 'indeedhub', '-d', database,
            ], input=statement.encode(), timeout=60)

        sql('CREATE TABLE migrations(id serial PRIMARY KEY, timestamp bigint NOT NULL, name text NOT NULL);'
            "INSERT INTO migrations(timestamp,name) VALUES(1,'Original1');"
            'CREATE TABLE contents(id int PRIMARY KEY, title text NOT NULL);'
            "INSERT INTO contents VALUES(1,'retained original');")
        with tempfile.TemporaryDirectory(prefix=name) as root:
            controller = maintenance.Controller(root, str(uuid.uuid4()), 0)

            database = 'indeedhub'

            def fixture_run(argv, timeout=30, output=None, input_bytes=None):
                assert argv[:4] == ['podman', 'exec', '-i', 'indeedhub-postgres']
                assert output is None and input_bytes is not None
                return sql(input_bytes.decode(), database)

            controller.run = fixture_run
            before = controller.database_commitments()
            dump = subprocess.check_output([
                'podman', 'exec', container, 'pg_dump', '-U', 'indeedhub',
                '-d', 'indeedhub', '--format=custom', '--no-owner', '--no-acl',
            ], timeout=60)
            sql('CREATE DATABASE restore_check')
            restore_command = ['podman', 'exec', '-i', container, 'pg_restore',
                               '-U', 'indeedhub', '-d', 'restore_check',
                               '--exit-on-error', '--no-owner', '--no-acl']
            subprocess.run(restore_command, input=dump, check=True, timeout=60)
            database = 'restore_check'
            maintenance.verify_database_compatibility(before, controller.database_commitments())
            database = 'indeedhub'
            rejected_dump = subprocess.run(restore_command, input=dump[:32], timeout=60,
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            assert rejected_dump.returncode != 0, 'Truncated dump incorrectly accepted'
            maintenance.verify_database_compatibility(before, controller.database_commitments())
            for table in sorted(maintenance.ADDITIVE_TABLES):
                sql(f'CREATE TABLE {table}(id int PRIMARY KEY);')
            for migration, timestamp in maintenance.ADDITIVE_MIGRATIONS.items():
                sql(f"INSERT INTO migrations(timestamp,name) VALUES({timestamp},'{migration}');")
            proof = maintenance.verify_database_compatibility(before, controller.database_commitments())
            assert len(proof['new_empty_tables']) == 5
            rejected = 0
            for mutation, undo in [
                ("UPDATE contents SET title='changed'", "UPDATE contents SET title='retained original'"),
                ('ALTER TABLE contents ADD COLUMN unexpected text', 'ALTER TABLE contents DROP COLUMN unexpected'),
                ('INSERT INTO archipelago_publications VALUES(1)', 'DELETE FROM archipelago_publications'),
                ("UPDATE migrations SET name='changed' WHERE id=1", "UPDATE migrations SET name='Original1' WHERE id=1"),
            ]:
                sql(mutation)
                try:
                    maintenance.verify_database_compatibility(before, controller.database_commitments())
                except RuntimeError:
                    rejected += 1
                else:
                    raise AssertionError('Changed database incorrectly accepted')
                sql(undo)
            maintenance.verify_database_compatibility(before, controller.database_commitments())
            print(json.dumps({'postgres_commitments': 'passed', 'rejected_mutations': rejected,
                              'network': 'none', 'live_volumes_mounted': False,
                              'custom_dump_restored': True, 'truncated_dump_rejected': True}))
    finally:
        if container:
            subprocess.run(['podman', 'rm', '-f', container], check=True, stdout=subprocess.DEVNULL)


if __name__ == '__main__':
    main()
