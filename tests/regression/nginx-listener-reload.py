#!/usr/bin/env python3
"""Exercise the exact bootstrap reload shell with isolated fake service commands.

No actual nginx/systemd command is invoked. A successful reload signal must not
count as acceptance if the master leaves its old workers running.
"""
from pathlib import Path
import os
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'core/archipelago/src/bootstrap.rs'


class ReloadAcceptance(unittest.TestCase):
    def run_reload(self, mode):
        source = SOURCE.read_text()
        function = source.split('async fn reload_nginx_checked()', 1)[1]
        script = function.split('r#"', 1)[1].split('"#;', 1)[0]
        with tempfile.TemporaryDirectory(prefix='nginx-reload-fixture-') as directory:
            root = Path(directory)
            commands = {
                'systemctl': '''#!/bin/sh
if [ "$1" = show ]; then echo 23; exit 0; fi
[ "$1" = reload ] || exit 90
[ "$RELOAD_CASE" != signal_failure ] || exit 1
touch "$FIXTURE_ROOT/reloaded"
''',
                'nginx': '#!/bin/sh\n[ "$RELOAD_CASE" != invalid_config ]\n',
                'ps': '''#!/bin/sh
[ "$1" = --ppid ] && [ "$2" = 23 ] || exit 91
if [ "$RELOAD_CASE" = accepted ] && [ -f "$FIXTURE_ROOT/reloaded" ]; then
    printf '102 nginx: worker process\n103 nginx: worker process\n'
elif [ "$RELOAD_CASE" = shutting_down ] && [ -f "$FIXTURE_ROOT/reloaded" ]; then
    printf '100 nginx: worker process\n101 nginx: worker process\n102 nginx: worker process is shutting down\n'
else
    printf '100 nginx: worker process\n101 nginx: worker process\n'
fi
''',
                'sleep': '#!/bin/sh\nexit 0\n',
            }
            for name, body in commands.items():
                path = root / name
                path.write_text(body)
                path.chmod(0o700)
            env = {'PATH': f'{root}:/usr/bin:/bin', 'FIXTURE_ROOT': str(root),
                   'RELOAD_CASE': mode, 'LC_ALL': 'C'}
            result = subprocess.run(['/bin/sh', '-c', script], env=env,
                                    capture_output=True, text=True, timeout=5)
            signalled = (root / 'reloaded').exists()
            return result.returncode, signalled

    def test_new_workers_accept_reload(self):
        self.assertEqual(self.run_reload('accepted'), (0, True))

    def test_successful_signal_without_new_workers_is_failure(self):
        code, signalled = self.run_reload('bind_failure')
        self.assertNotEqual(code, 0)
        self.assertTrue(signalled)

    def test_shutting_down_workers_do_not_prove_acceptance(self):
        self.assertNotEqual(self.run_reload('shutting_down')[0], 0)

    def test_invalid_config_never_signals(self):
        code, signalled = self.run_reload('invalid_config')
        self.assertNotEqual(code, 0)
        self.assertFalse(signalled)

    def test_failed_signal_is_failure(self):
        self.assertNotEqual(self.run_reload('signal_failure')[0], 0)


if __name__ == '__main__':
    unittest.main()
