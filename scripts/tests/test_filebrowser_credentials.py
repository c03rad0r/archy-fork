import importlib.util
from pathlib import Path
import json
import os
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('filebrowser_credentials', Path(__file__).resolve().parents[1] / 'filebrowser-credentials.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class CredentialTests(unittest.TestCase):
    def test_record_validation_rejects_defaults_and_malformed_credentials(self):
        valid = {'schema': 1, 'username': 'archy-' + 'a' * 32, 'password': 'b' * 64}
        self.assertEqual(module.credentials(valid), valid)
        for values in [{**valid, 'username': 'admin'}, {**valid, 'password': 'admin'}, {**valid, 'schema': 2}, {**valid, 'password': 'x' * 64}, None]:
            with self.assertRaises(module.ProvisionError):
                module.credentials(values)

    def test_atomic_secret_is_private_and_refuses_symlinks(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'credential.json'
            module.atomic_json(path, {'value': 'test'})
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            self.assertEqual(json.loads(path.read_text()), {'value': 'test'})
            target = Path(tmp) / 'target'
            target.write_text('preserved')
            path.unlink()
            path.symlink_to(target)
            with self.assertRaises(module.ProvisionError):
                module.atomic_json(path, {})
            self.assertEqual(target.read_text(), 'preserved')

    def test_database_selection_preserves_legacy_and_configured_locations(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.assertEqual(module.database_path(root), '/data/filebrowser.db')
            (root / 'database.db').touch()
            self.assertEqual(module.database_path(root), '/data/database.db')
            (root / 'filebrowser.db').touch()
            with self.assertRaises(module.ProvisionError):
                module.database_path(root)
            (root / '.filebrowser.json').write_text(json.dumps({'database': '/data/database.db'}))
            self.assertEqual(module.database_path(root), '/data/database.db')

    def test_database_paths_fail_closed_on_traversal_or_symlinks(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for database in ['/data/../outside.db', '/outside.db', None, '/data/a.db\n--flag']:
                (root / '.filebrowser.json').write_text(json.dumps({'database': database}))
                with self.assertRaises(module.ProvisionError):
                    module.database_path(root)
            (root / '.filebrowser.json').write_text(json.dumps({'database': '/data/filebrowser.db'}))
            (root / 'filebrowser.db').symlink_to(root / 'elsewhere')
            with self.assertRaises(module.ProvisionError):
                module.database_path(root)


if __name__ == '__main__':
    unittest.main()
