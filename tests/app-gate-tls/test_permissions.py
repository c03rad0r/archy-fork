import importlib.util
import os
from pathlib import Path
import stat
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('repair', Path(__file__).resolve().parents[2] / 'scripts/repair-app-gate-tls-permissions.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class Permissions(unittest.TestCase):
    def test_existing_key_bytes_survive_and_repeat_is_noop(self):
        with tempfile.TemporaryDirectory() as d:
            key = Path(d) / 'key'
            key.write_bytes(b'private fixture bytes')
            key.chmod(0o600)
            self.assertTrue(module.repair(key, os.getuid(), os.getgid()))
            self.assertEqual(key.read_bytes(), b'private fixture bytes')
            self.assertEqual(stat.S_IMODE(key.stat().st_mode), 0o640)
            self.assertFalse(module.repair(key, os.getuid(), os.getgid()))

    def test_preserves_read_only_owner_and_removes_other_access(self):
        with tempfile.TemporaryDirectory() as d:
            key = Path(d) / 'key'
            key.write_bytes(b'fixture')
            key.chmod(0o444)
            module.repair(key, os.getuid(), os.getgid())
            self.assertEqual(stat.S_IMODE(key.stat().st_mode), 0o440)

    def test_missing_key_does_not_generate_identity(self):
        with tempfile.TemporaryDirectory() as d:
            key = Path(d) / 'missing'
            self.assertFalse(module.repair(key, os.getuid(), os.getgid()))
            self.assertFalse(key.exists())

    def test_symlink_target_is_never_modified(self):
        with tempfile.TemporaryDirectory() as d:
            target = Path(d) / 'target'
            target.write_bytes(b'preserve')
            target.chmod(0o600)
            link = Path(d) / 'key'
            link.symlink_to(target)
            with self.assertRaises(OSError): module.repair(link, os.getuid(), os.getgid())
            self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o600)
            self.assertEqual(target.read_bytes(), b'preserve')

    @unittest.skipIf(os.getuid() == 0, 'Root is an explicitly accepted owner')
    def test_unexpected_owner_is_rejected_without_permission_change(self):
        with tempfile.TemporaryDirectory() as d:
            key = Path(d) / 'key'
            key.write_bytes(b'fixture')
            key.chmod(0o600)
            with self.assertRaises(RuntimeError): module.repair(key, os.getuid() + 1, os.getgid())
            self.assertEqual(stat.S_IMODE(key.stat().st_mode), 0o600)

    def test_non_regular_file_is_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(RuntimeError): module.repair(Path(d), os.getuid(), os.getgid())

if __name__ == '__main__': unittest.main()
