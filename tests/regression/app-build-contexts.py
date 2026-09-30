#!/usr/bin/env python3
"""Exercise release payload checks with complete, incomplete and escaping contexts."""
import importlib.util
import shutil
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('contexts', REPO / 'scripts/check-app-build-contexts.py')
contexts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contexts)


class BuildPayloadTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        shutil.copytree(REPO / 'apps', self.root / 'apps')
        shutil.copytree(REPO / 'docker', self.root / 'docker')

    def test_complete_payload(self):
        self.assertGreaterEqual(contexts.check(self.root), 6)

    def test_iso_old_allowlist_rejected(self):
        shutil.rmtree(self.root / 'docker/archipelago-source')
        with self.assertRaisesRegex(ValueError, 'archipelago-source.*missing'):
            contexts.check(self.root)

    def test_missing_dockerfile_rejected(self):
        (self.root / 'docker/archipelago-source/Dockerfile').unlink()
        with self.assertRaisesRegex(ValueError, 'archipelago-source.*Dockerfile'):
            contexts.check(self.root)

    def test_context_symlink_cannot_escape_payload(self):
        target = self.root / 'docker/archipelago-source'
        shutil.rmtree(target)
        target.symlink_to(REPO / 'docker/archipelago-source', target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'out-of-payload'):
            contexts.check(self.root)

    def test_empty_payload_rejected(self):
        shutil.rmtree(self.root / 'apps')
        with self.assertRaisesRegex(ValueError, 'No app manifests'):
            contexts.check(self.root)


if __name__ == '__main__':
    unittest.main()
