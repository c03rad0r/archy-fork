import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('guard', Path(__file__).parents[1] / 'dashboard-public-guard.py')
guard = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(guard)
SOURCE = '''# quoted braces must not confuse the parser
server { listen 80 default_server; server_name _; location / { return 200 "{}"; } }
server { listen 443 ssl default_server; server_name _; location / { return 200 "a}"; } }
server { listen 80; server_name public.example; location / { return 200 "app"; } }
'''


class GuardTests(unittest.TestCase):
    def test_active_site_copy_and_symlink_target_are_resolved(self):
        for symlink in [False, True]:
            with self.subTest(symlink=symlink), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                available = root / 'sites-available/archipelago'
                enabled = root / 'sites-enabled/archipelago'
                available.parent.mkdir(); enabled.parent.mkdir()
                available.write_text(SOURCE)
                if symlink:
                    enabled.symlink_to(available)
                else:
                    enabled.write_text(SOURCE + '# active custom copy\n')
                active = guard.active_dashboard(root)
                self.assertEqual(active, available if symlink else enabled)
                before = available.read_bytes()
                def command(args, **kwargs):
                    return subprocess.CompletedProcess(args, 0)
                guard.apply(active, command, root / 'lock')
                self.assertIn(guard.BEGIN, enabled.read_text())
                self.assertEqual(enabled.is_symlink(), symlink)
                if not symlink:
                    self.assertEqual(available.read_bytes(), before)

    def test_all_defaults_guarded_named_apps_untouched_idempotent(self):
        updated = guard.guarded(SOURCE)
        self.assertEqual(updated.count(guard.CHECK), 2)
        self.assertIn(SOURCE.splitlines()[-1], updated)
        self.assertEqual(guard.guarded(updated), updated)
        self.assertIn('geo $realip_remote_addr', updated)

    def test_incomplete_and_ambiguous_config_rejected(self):
        for source in [SOURCE.replace('listen 443 ssl default_server;', 'listen 8443 ssl;'),
                       SOURCE + '\n' + guard.BEGIN, SOURCE + '\nserver {',
                       guard.END + '\n' + guard.BEGIN + '\n' + SOURCE]:
            with self.assertRaises(ValueError):
                guard.guarded(source)

    def test_legacy_address_specific_https_dashboard(self):
        source = SOURCE.replace('listen 443 ssl default_server;', 'listen 192.168.1.10:443 ssl;')
        updated = guard.guarded(source)
        self.assertEqual(updated.count(guard.CHECK), 2)
        self.assertEqual(guard.guarded(updated), updated)
        self.assertIn(SOURCE.splitlines()[-1], updated)

    def test_syntax_and_reload_failure_restore_exact_previous_bytes(self):
        for failure in ['nginx', 'systemctl']:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as tmp:
                path = Path(tmp) / 'archipelago'
                path.write_text(SOURCE)
                calls = []
                def command(args, **kwargs):
                    calls.append(args)
                    # Only the candidate's first matching command fails.
                    fail = args[0] == failure and sum(x[0] == failure for x in calls) == 1
                    return subprocess.CompletedProcess(args, int(fail))
                with self.assertRaises(RuntimeError):
                    guard.apply(path, command, Path(tmp) / 'nginx.lock')
                self.assertEqual(path.read_text(), SOURCE)
                backups = list(Path(tmp).glob('*.before-management-guard-*'))
                self.assertEqual(len(backups), 1)
                self.assertEqual(backups[0].read_text(), SOURCE)
                self.assertEqual(backups[0].stat().st_mode & 0o777, 0o600)
                self.assertEqual(calls[-1], ['systemctl', 'reload', 'nginx'])

    def test_second_application_does_not_reload(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'archipelago'
            path.write_text(SOURCE)
            calls = []
            def command(args, **kwargs):
                calls.append(args)
                return subprocess.CompletedProcess(args, 0)
            self.assertTrue(guard.apply(path, command, Path(tmp) / 'nginx.lock'))
            self.assertFalse(guard.apply(path, command, Path(tmp) / 'nginx.lock'))
            self.assertEqual(len(calls), 2)

    def test_legacy_runtime_install_is_guarded_before_any_validation_or_reload(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'active'
            source = Path(tmp) / 'legacy-runtime'
            path.write_text(guard.guarded(SOURCE))
            source.write_text(SOURCE + '# legacy runtime revision\n')
            calls = []
            def command(args, **kwargs):
                # Even the first nginx -t must see a complete guarded candidate.
                current = path.read_text()
                self.assertEqual(current.count(guard.CHECK), 2)
                self.assertEqual(guard.guarded(current), current)
                self.assertIn('# legacy runtime revision', current)
                calls.append(args)
                return subprocess.CompletedProcess(args, 0)
            self.assertTrue(guard.apply(path, command, Path(tmp) / 'lock', source))
            self.assertFalse(guard.apply(path, command, Path(tmp) / 'lock', source))
            self.assertEqual(len(calls), 2)
            self.assertEqual(source.read_text(), SOURCE + '# legacy runtime revision\n')

    def test_invalid_runtime_never_replaces_protected_site(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'active'
            source = Path(tmp) / 'legacy-runtime'
            previous = guard.guarded(SOURCE)
            path.write_text(previous)
            source.write_text('server { listen 80 default_server; server_name _; }')
            def command(*args, **kwargs):
                self.fail('invalid candidate must be rejected before any command')
            with self.assertRaises(ValueError):
                guard.apply(path, command, Path(tmp) / 'lock', source)
            self.assertEqual(path.read_text(), previous)

    def test_runtime_validation_or_reload_failure_preserves_previous_guard(self):
        for failure in ['nginx', 'systemctl']:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as tmp:
                path = Path(tmp) / 'active'
                source = Path(tmp) / 'legacy-runtime'
                previous = guard.guarded(SOURCE)
                path.write_text(previous)
                source.write_text(SOURCE + '# incoming revision\n')
                calls = []
                def command(args, **kwargs):
                    self.assertEqual(path.read_text().count(guard.CHECK), 2)
                    calls.append(args)
                    fail = args[0] == failure and sum(x[0] == failure for x in calls) == 1
                    return subprocess.CompletedProcess(args, int(fail))
                with self.assertRaises(RuntimeError):
                    guard.apply(path, command, Path(tmp) / 'lock', source)
                self.assertEqual(path.read_text(), previous)

    def test_runtime_bootstrap_uses_guarded_installer_instead_of_raw_copy(self):
        # Wiring matters: a safe helper does not help if bootstrap bypasses it.
        source = (Path(__file__).parents[2] / 'core/archipelago/src/bootstrap.rs').read_text()
        block = source.split('let nginx_src = configs.join("nginx-archipelago.conf");', 1)[1].split('// archipelago-host-secrets-audit', 1)[0]
        self.assertIn('scripts/dashboard-public-guard.py', block)
        self.assertIn('"--install"', block)
        self.assertNotIn('"install",', block)

    def test_rollback_payload_is_protected_idempotently_before_old_binary_can_copy_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            template = Path(tmp) / 'legacy.conf'
            template.write_text(SOURCE)
            template.chmod(0o640)
            self.assertTrue(guard.protect_template(template))
            self.assertEqual(template.read_text(), guard.guarded(SOURCE))
            self.assertEqual(template.stat().st_mode & 0o777, 0o640)
            self.assertFalse(guard.protect_template(template))
            invalid = 'server { listen 80 default_server; }'
            template.write_text(invalid)
            with self.assertRaises(ValueError):
                guard.protect_template(template)
            self.assertEqual(template.read_text(), invalid)
        source = (Path(__file__).parents[2] / 'core/archipelago/src/update.rs').read_text()
        rollback = source.split('pub async fn rollback_update', 1)[1]
        self.assertLess(rollback.index('"--protect-template"'), rollback.index('host_sudo(&["cp"'))
        self.assertIn('protected.success()', rollback)


if __name__ == '__main__':
    unittest.main()
