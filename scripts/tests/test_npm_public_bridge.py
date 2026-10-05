import contextlib
import importlib.util
import json
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('bridge', Path(__file__).parents[1] / 'npm-public-bridge.py')
bridge = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bridge)


def database(path):
    path.mkdir(parents=True, exist_ok=True)
    with contextlib.closing(sqlite3.connect(path / 'database.sqlite')) as con:
        con.executescript('''
          CREATE TABLE proxy_host(id INTEGER, domain_names TEXT, certificate_id INTEGER,
                                  enabled INTEGER, is_deleted INTEGER);
          CREATE TABLE certificate(id INTEGER, provider TEXT, is_deleted INTEGER);
        ''')


def runtime(data, certs):
    return {'Mounts': [{'Destination': '/data', 'Source': str(data)},
                       {'Destination': '/etc/letsencrypt', 'Source': str(certs)}]}


class StorageTests(unittest.TestCase):
    def test_managed_realip_file_is_idempotent_and_preserves_operator_snippets(self):
        with tempfile.TemporaryDirectory() as tmp:
            data = Path(tmp)
            custom = data / 'nginx/custom/http_top.conf'
            custom.parent.mkdir(parents=True)
            custom.write_text('# Operator configuration\n')
            original_mode = custom.stat().st_mode & 0o777
            path = bridge.prepare_realip({'data': str(data)})
            self.assertEqual(path.stat().st_mode & 0o777, original_mode)
            self.assertEqual(path.read_text().count('set_real_ip_from'), 1)
            self.assertIn('set_real_ip_from 169.254.1.100;', path.read_text())
            before = path.stat().st_mtime_ns
            self.assertEqual(bridge.prepare_realip({'data': str(data)}), path)
            self.assertEqual(path.stat().st_mtime_ns, before)
            self.assertTrue(custom.read_text().startswith('# Operator configuration\n'))
            path.write_text('# BEGIN ARCHY HOST BRIDGE\n# Operator override\n# END ARCHY HOST BRIDGE\n')
            with self.assertRaisesRegex(ValueError, 'operator override'):
                bridge.prepare_realip({'data': str(data)})
            self.assertIn('# Operator override', path.read_text())
            original = data / 'operator.conf'
            original.write_text('# Preserved\n')
            path.unlink(); path.symlink_to(original)
            with self.assertRaisesRegex(ValueError, 'symlink'):
                bridge.prepare_realip({'data': str(data)})
            self.assertEqual(original.read_text(), '# Preserved\n')

    def test_recorded_custom_mount_survives_missing_container_record(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp) / 'npm'
            base.mkdir()
            data = Path(tmp) / 'custom-data'
            database(data)
            expected = {'data': str(data), 'certificates': str(Path(tmp) / 'custom-certs')}
            bridge.atomic(base / '.archy-storage.json', json.dumps(expected).encode())
            self.assertEqual(bridge.resolve_paths(base), expected)
            (data / 'database.sqlite').unlink()
            with self.assertRaisesRegex(ValueError, 'Previously initialized'):
                bridge.resolve_paths(base)

    def test_fresh_flat_nested_and_custom_mount_without_mutation(self):
        for layout in ['fresh', 'flat', 'nested', 'custom']:
            with self.subTest(layout=layout), tempfile.TemporaryDirectory() as tmp:
                base = Path(tmp) / 'npm'
                data = base / 'data' if layout == 'nested' else base
                if layout == 'custom':
                    data = Path(tmp) / 'operator-data'
                certs = Path(tmp) / 'operator-certs' if layout == 'custom' else base / 'letsencrypt'
                if layout != 'fresh':
                    database(data)
                before = {p: p.read_bytes() for p in Path(tmp).rglob('*') if p.is_file()}
                result = bridge.resolve_paths(base, runtime(data, certs))
                self.assertEqual(result, {'data': str(data), 'certificates': str(certs)})
                after = {p: p.read_bytes() for p in Path(tmp).rglob('*') if p.is_file()}
                self.assertEqual(before, after)
                if layout != 'custom':
                    self.assertEqual(bridge.resolve_paths(base)['data'], str(data))

    def test_ambiguity_and_wrong_active_mount_fail_without_replacing_data(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            database(base)
            original = (base / 'database.sqlite').read_bytes()
            with self.assertRaisesRegex(ValueError, 'active mount differs'):
                bridge.resolve_paths(base, runtime(base / 'empty', base / 'certs'))
            database(base / 'data')
            with self.assertRaisesRegex(ValueError, 'Multiple NPM databases'):
                bridge.resolve_paths(base)
            before = {path: path.read_bytes() for path in base.rglob('database.sqlite')}
            for selected in [base, base / 'data']:
                self.assertEqual(bridge.resolve_paths(base, runtime(selected, base / 'certs'))['data'], str(selected))
            bridge.atomic(base / '.archy-storage.json', json.dumps({
                'data': str(base), 'certificates': str(base / 'certs')}).encode())
            self.assertEqual(bridge.resolve_paths(base)['data'], str(base))
            self.assertEqual({path: path.read_bytes() for path in base.rglob('database.sqlite')}, before)
            self.assertEqual((base / 'database.sqlite').read_bytes(), original)

    def test_corrupt_or_uninitialized_database_is_not_recreated(self):
        for value in [b'not a sqlite database', b'']:
            with tempfile.TemporaryDirectory() as tmp:
                base = Path(tmp)
                db = base / 'database.sqlite'
                db.write_bytes(value)
                with self.assertRaises((sqlite3.DatabaseError, ValueError)):
                    bridge.resolve_paths(base)
                self.assertEqual(db.read_bytes(), value)

    def test_duplicate_and_missing_mounts_rejected(self):
        info = runtime(Path('/data/npm'), Path('/data/certs'))
        for mounts in [info['Mounts'][:1], info['Mounts'] + info['Mounts'][:1]]:
            with self.assertRaises(ValueError):
                bridge.resolve_paths(Path('/nonexistent-fixture'), {'Mounts': mounts})

    def test_deleted_and_disabled_hosts_are_excluded(self):
        with tempfile.TemporaryDirectory() as tmp:
            data = Path(tmp)
            database(data)
            with contextlib.closing(sqlite3.connect(data / 'database.sqlite')) as con:
                con.executemany('INSERT INTO proxy_host VALUES (?, ?, 0, ?, ?)', [
                    (1, '["active.example"]', 1, 0),
                    (2, '["disabled.example"]', 0, 0),
                    (3, '["deleted.example"]', 1, 1)])
                con.commit()
            self.assertEqual([row['id'] for row in bridge.hosts(data)], [1])

    def test_redirect_and_dead_hosts_are_also_routed_through_npm(self):
        with tempfile.TemporaryDirectory() as tmp:
            data = Path(tmp)
            database(data)
            with contextlib.closing(sqlite3.connect(data / 'database.sqlite')) as con:
                for table, domain in [('redirection_host', 'redirect.example'), ('dead_host', 'gone.example')]:
                    con.execute(f'CREATE TABLE {table} AS SELECT * FROM proxy_host')
                    con.execute(f'INSERT INTO {table} VALUES (1, ?, 0, 1, 0)', (json.dumps([domain]),))
                con.commit()
            self.assertEqual({r['domain_names'] for r in bridge.hosts(data)},
                             {'["redirect.example"]', '["gone.example"]'})


class RoutingTests(unittest.TestCase):
    def test_active_dashboard_uses_enabled_copy_or_symlink_target(self):
        for symlink in [False, True]:
            with self.subTest(symlink=symlink), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                enabled = root / 'sites-enabled/archipelago'
                available = root / 'sites-available/archipelago'
                enabled.parent.mkdir(); available.parent.mkdir()
                available.write_text('available')
                if symlink:
                    enabled.symlink_to(available)
                else:
                    enabled.write_text('active copy')
                self.assertEqual(bridge.active_dashboard(root), available if symlink else enabled)

    def test_acme_flat_nested_upgrade_and_custom_override_preservation(self):
        for legacy in [str(bridge.BASE), str(bridge.BASE / 'data')]:
            source = ('location ^~ /.well-known/acme-challenge/ {\n'
                      f' root {legacy}/letsencrypt-acme-challenge; try_files $uri =404;\n' + '}\n') * 2
            result = bridge.dashboard_acme_root(source, '/operator/npm-data')
            self.assertEqual(result.count('root "/operator/npm-data/letsencrypt-acme-challenge";'), 2)
            self.assertEqual(bridge.dashboard_acme_root(result, '/operator/npm-data'), result)
            with self.assertRaisesRegex(ValueError, 'Custom dashboard ACME'):
                bridge.dashboard_acme_root(source.replace(legacy, '/unrecognized'), '/operator/npm-data')
            with self.assertRaisesRegex(ValueError, 'HTTP and HTTPS'):
                bridge.dashboard_acme_root(source.split('}\n')[0] + '}\n', '/operator/npm-data')

    def test_shipped_template_and_legacy_missing_https_acme(self):
        template = (Path(__file__).parents[2] / 'image-recipe/configs/nginx-archipelago.conf').read_text()
        import re
        pattern = re.compile(r'location\s+\^~\s+/\.well-known/acme-challenge/\s*\{[^{}]*\}', re.S)
        locations = list(pattern.finditer(template))
        self.assertEqual(len(locations), 2)
        legacy = template[:locations[1].start()] + template[locations[1].end():]
        for source in (template, legacy):
            result = bridge.dashboard_acme_root(source, '/operator/npm-data')
            self.assertEqual(result.count('root "/operator/npm-data/letsencrypt-acme-challenge";'), 2)
            self.assertEqual(bridge.dashboard_acme_root(result, '/operator/npm-data'), result)
            self.assertEqual(result.count('try_files $uri =404;'), template.count('try_files $uri =404;'))
        with self.assertRaisesRegex(ValueError, 'HTTP and HTTPS'):
            bridge.dashboard_acme_root(legacy.replace('listen 443 ssl default_server;', 'listen 444 ssl;'), '/operator/npm-data')

    def test_custom_duplicate_routes_block_replacement(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp) / 'conf.d'
            directory.mkdir()
            config = directory / 'operator.conf'
            original = 'server { listen 80; server_name public.example; return 200 "operator"; }'
            config.write_text(original)
            rows = [{'domain_names': '["public.example"]'}]
            with self.assertRaisesRegex(ValueError, 'custom configuration preserved'):
                bridge.existing_route_changes(rows, {}, output=directory / 'generated.conf', directories=[directory])
            self.assertEqual(config.read_text(), original)
            config.write_text('server { listen 80; server_name other.example; return 200 "operator"; }')
            self.assertEqual(bridge.existing_route_changes(rows, {}, directories=[directory]), [])

    def test_emergency_routes_retire_transactionally_and_preserve_operator_edits(self):
        paths = {'data': str(bridge.BASE), 'certificates': str(bridge.BASE / 'letsencrypt')}
        fixtures = Path(__file__).parent / 'fixtures'
        for app in ['indexer', 'relay', 'shop']:
            with self.subTest(app=app), tempfile.TemporaryDirectory() as tmp:
                directory = Path(tmp) / 'conf.d'
                directory.mkdir()
                route = directory / ('shop-btcpay.conf' if app == 'shop' else f'angor-{app}-npm.conf')
                original = (fixtures / f'npm-emergency-{app}.conf').read_bytes()
                route.write_bytes(original)
                self.assertTrue(bridge.legacy_shop_route(original.decode(), paths) if app == 'shop'
                                else bridge.legacy_angor_route(original.decode(), paths, relay=app == 'relay'))
                names = ['fixture.example', 'www.fixture.example'] if app == 'shop' else ['fixture.example']
                rows = [{'domain_names': json.dumps(names), 'certificate_id': 12}]
                self.assertEqual(bridge.existing_route_changes([], paths, directories=[directory]), [])
                with self.assertRaisesRegex(ValueError, 'custom configuration preserved'):
                    bridge.existing_route_changes([{**rows[0], 'certificate_id': 0}], paths, directories=[directory])
                if app == 'shop':
                    with self.assertRaisesRegex(ValueError, 'custom configuration preserved'):
                        bridge.existing_route_changes([{**rows[0], 'domain_names': '["fixture.example"]'}], paths, directories=[directory])
                changes = bridge.existing_route_changes(rows, paths, directories=[directory])
                self.assertEqual(len(changes), 1)
                def fail(args, **kwargs):
                    return subprocess.CompletedProcess(args, 1 if route.read_bytes() != original else 0)
                with self.assertRaisesRegex(RuntimeError, 'validation/reload failed'):
                    bridge.apply_files(changes, Path(tmp) / 'state', fail, Path(tmp) / 'lock')
                self.assertEqual(route.read_bytes(), original)
                def succeed(args, **kwargs):
                    return subprocess.CompletedProcess(args, 0)
                self.assertTrue(bridge.apply_files(changes, Path(tmp) / 'state', succeed, Path(tmp) / 'lock'))
                self.assertEqual(bridge.existing_route_changes(rows, paths, directories=[directory]), [])
                modified = original.replace(b'proxy_http_version 1.1;', b'proxy_http_version 1.1; proxy_read_timeout 42s;')
                route.write_bytes(modified)
                with self.assertRaisesRegex(ValueError, 'custom configuration preserved'):
                    bridge.existing_route_changes(rows, paths, directories=[directory])
                self.assertEqual(route.read_bytes(), modified)

    def test_domain_injection_rejected(self):
        for name in ['_', 'x; return 200;', 'x\ninclude bad;', '$host', 'a/b', '.example', 'a..b', '-a.example']:
            with self.subTest(name=name), self.assertRaises(ValueError):
                bridge.domains(json.dumps([name]))
        self.assertEqual(bridge.domains('["EXAMPLE.COM.", "*.example.com"]'),
                         ['*.example.com', 'example.com'])

    def test_mixed_wildcard_and_loopback_publication_rejected(self):
        info = {'NetworkSettings': {'Ports': {'80/tcp': [
            {'HostIp': '127.0.0.1', 'HostPort': '8088'},
            {'HostIp': '0.0.0.0', 'HostPort': '8088'}]}}}
        with self.assertRaisesRegex(ValueError, 'non-loopback'):
            bridge.local_port(info, 80)
        info['NetworkSettings']['Ports']['80/tcp'].pop()
        self.assertEqual(bridge.local_port(info, 80), '127.0.0.1:8088')

    def test_wireguard_web_listener_preserved_but_loopback_still_required(self):
        tunnel = {'HostIp': '10.55.0.2', 'HostPort': '18081'}
        info = {'NetworkSettings': {'Ports': {'80/tcp': [
            tunnel, {'HostIp': '127.0.0.1', 'HostPort': '8088'}]}}}
        interface = [{'ifname': 'wg-web', 'linkinfo': {'info_kind': 'wireguard'},
                      'addr_info': [{'family': 'inet', 'local': '10.55.0.2'}]}]
        with patch.object(bridge, 'run', return_value=json.dumps(interface)):
            self.assertEqual(bridge.local_port(info, 80), '127.0.0.1:8088')
            info['NetworkSettings']['Ports']['80/tcp'].pop()
            with self.assertRaisesRegex(ValueError, 'needs a loopback'):
                bridge.local_port(info, 80)
            self.assertFalse(bridge.managed_tunnel_listener(tunnel, 81))
            self.assertFalse(bridge.managed_tunnel_listener(dict(tunnel, HostIp='0.0.0.0'), 80))
            self.assertFalse(bridge.managed_tunnel_listener(dict(tunnel, HostIp='203.0.113.1'), 80))
            self.assertFalse(bridge.managed_tunnel_listener(dict(tunnel, HostIp='10.55.0.3'), 80))
            self.assertFalse(bridge.managed_tunnel_listener(dict(tunnel, HostPort='18080'), 80))
            interface[0]['linkinfo']['info_kind'] = 'dummy'
        with patch.object(bridge, 'run', return_value=json.dumps(interface)):
            self.assertFalse(bridge.managed_tunnel_listener(tunnel, 80))
        with patch.object(bridge, 'run', side_effect=RuntimeError('interface missing')):
            self.assertFalse(bridge.managed_tunnel_listener(tunnel, 80))

    def test_bridge_routes_through_npm_without_copying_upstream(self):
        rows = [{'id': 1, 'domain_names': '["public.example"]', 'certificate_id': 0,
                 'certificate_deleted': 0, 'provider': None, 'forward_host': 'private-backend'}]
        config, trust, fingerprints = bridge.render(rows, {}, '127.0.0.1:8088', '127.0.0.1:8444',
                                                    '/acme', '/trust.pem')
        self.assertIn(b'proxy_pass http://127.0.0.1:8088;', config)
        self.assertNotIn(b'private-backend', config)
        self.assertNotIn(b'listen 443', config)
        self.assertIn(b'proxy_set_header X-Forwarded-For $remote_addr;', config)
        self.assertEqual(fingerprints, [])
        self.assertTrue(trust)
        with self.assertRaisesRegex(ValueError, 'Duplicate'):
            bridge.render(rows + rows, {}, '127.0.0.1:8088', '127.0.0.1:8444', '/acme', '/trust.pem')


class TransactionTests(unittest.TestCase):
    def test_idempotent_sync_and_certificate_renewal_reload(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            files = [(base / 'hosts.conf', b'new routes', 0o644), (base / 'trust.pem', b'chain', 0o600)]
            calls = []
            def command(args, **kwargs):
                calls.append(args)
                return subprocess.CompletedProcess(args, 0)
            args = (files, base / 'state', command, base / 'lock')
            self.assertTrue(bridge.apply_files(*args, renewal_fingerprint='first'))
            self.assertEqual(len(calls), 2)
            self.assertFalse(bridge.apply_files(*args, renewal_fingerprint='first'))
            self.assertEqual(len(calls), 2)
            self.assertTrue(bridge.apply_files(*args, renewal_fingerprint='renewed'))
            self.assertEqual(len(calls), 4)
            self.assertEqual((base / 'hosts.conf').stat().st_mode & 0o777, 0o644)
            self.assertEqual((base / 'trust.pem').stat().st_mode & 0o777, 0o600)

    def test_certificate_reload_failure_rolls_back_and_retry_keeps_obligation(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            output = base / 'hosts.conf'
            output.write_bytes(b'previous')
            def command(args, **kwargs):
                return subprocess.CompletedProcess(args, 0)
            def failure():
                raise RuntimeError('fixture NPM reload failure')
            args = ([(output, b'candidate', 0o644)], base / 'state', command, base / 'lock')
            with self.assertRaisesRegex(RuntimeError, 'fixture NPM reload failure'):
                bridge.apply_files(*args, renewal_fingerprint='new', certificate_reload=failure)
            self.assertEqual(output.read_bytes(), b'previous')
            self.assertFalse((base / 'state/applied.json').exists())
            reloaded = []
            callback = lambda: reloaded.append(True)
            self.assertTrue(bridge.apply_files(*args, renewal_fingerprint='new', certificate_reload=callback))
            self.assertFalse(bridge.apply_files(*args, renewal_fingerprint='new', certificate_reload=callback))
            self.assertEqual(reloaded, [True])

    def test_validation_or_reload_failure_restores_files_and_modes(self):
        for failure in ['nginx', 'systemctl']:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as tmp:
                base = Path(tmp)
                original = base / 'hosts.conf'
                original.write_bytes(b'operator previous routes')
                original.chmod(0o640)
                trust = base / 'trust.pem'
                calls = []
                def command(args, **kwargs):
                    calls.append(args)
                    fail = args[0] == failure and sum(c[0] == failure for c in calls) == 1
                    return subprocess.CompletedProcess(args, int(fail))
                with self.assertRaisesRegex(RuntimeError, 'validation/reload failed'):
                    bridge.apply_files([(original, b'candidate', 0o644), (trust, b'new trust', 0o600)],
                                       base / 'state', command, base / 'lock')
                self.assertEqual(original.read_bytes(), b'operator previous routes')
                self.assertEqual(original.stat().st_mode & 0o777, 0o640)
                self.assertFalse(trust.exists())
                self.assertFalse((base / 'state/pending.json').exists())
                backup = next((base / 'state').glob('backup-*/0'))
                self.assertEqual(backup.read_bytes(), original.read_bytes())
                self.assertEqual(backup.stat().st_mode & 0o777, 0o600)

    def test_failed_rollback_is_recovered_before_next_sync(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            original = base / 'hosts.conf'
            original.write_bytes(b'previous')
            files = [(original, b'candidate', 0o644)]
            def fail(args, **kwargs):
                return subprocess.CompletedProcess(args, 1)
            with self.assertRaisesRegex(RuntimeError, 'rollback incomplete'):
                bridge.apply_files(files, base / 'state', fail, base / 'lock')
            self.assertTrue((base / 'state/pending.json').exists())
            seen = []
            def succeed(args, **kwargs):
                seen.append(original.read_bytes())
                return subprocess.CompletedProcess(args, 0)
            self.assertTrue(bridge.apply_files(files, base / 'state', succeed, base / 'lock'))
            self.assertEqual(seen, [b'previous', b'previous', b'candidate', b'candidate'])
            self.assertFalse((base / 'state/pending.json').exists())

    def test_pending_recovery_restores_inputs_before_render_and_is_idempotent(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / 'active.conf'
            target.write_bytes(b'partially written candidate')
            backup = root / 'backup'
            backup.write_bytes(b'original active config')
            state = root / 'state'
            state.mkdir()
            journal = state / 'pending.json'
            journal.write_text(json.dumps({'files': [{'path': str(target), 'backup': str(backup), 'mode': 0o640}]}))
            calls = []
            def command(args, **kwargs):
                calls.append(args)
                self.assertEqual(target.read_bytes(), b'original active config')
                return subprocess.CompletedProcess(args, 0)
            self.assertTrue(bridge.recover_pending(state, command))
            self.assertFalse(bridge.recover_pending(state, command))
            self.assertEqual(len(calls), 2)
            self.assertEqual(target.stat().st_mode & 0o777, 0o640)

    def test_operator_symlink_not_replaced(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            custom = base / 'custom.conf'
            custom.write_bytes(b'operator')
            output = base / 'hosts.conf'
            output.symlink_to(custom)
            with self.assertRaisesRegex(ValueError, 'symlink'):
                bridge.apply_files([(output, b'new', 0o644)], base / 'state', lock_path=base / 'lock')
            self.assertTrue(output.is_symlink())
            self.assertEqual(custom.read_bytes(), b'operator')


if __name__ == '__main__':
    unittest.main()
