import contextlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('release_notes', Path(__file__).resolve().parents[1] / 'sync-whats-new.py')
notes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notes)


class ReleaseNotesTests(unittest.TestCase):
    def test_check_rejects_stale_content_without_modifying_the_modal(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            changelog = root / 'CHANGELOG.md'
            modal = root / 'notes.vue'
            changelog.write_text('## v1.9.0 (2026-10-05)\n\n- Keep uploads on their original screen.\n')
            stale = notes.render_block({'ver': 'v1.9.0', 'date': 'October 2, 2026', 'bullets': ['Keep uploads across screens.']})
            modal.write_text('<template>\n' + stale + '</template>\n')
            with patch.object(notes, 'CHANGELOG', changelog), patch.object(notes, 'MODAL', modal):
                original = modal.read_bytes()
                with patch('sys.argv', ['sync-whats-new.py', '--check']), contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(notes.main(), 1)
                self.assertEqual(modal.read_bytes(), original)
                with patch('sys.argv', ['sync-whats-new.py']), contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(notes.main(), 0)
                self.assertIn('original screen', modal.read_text())
                self.assertIn('October 5, 2026', modal.read_text())
                with patch('sys.argv', ['sync-whats-new.py', '--check']), contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(notes.main(), 0)


if __name__ == '__main__':
    unittest.main()
