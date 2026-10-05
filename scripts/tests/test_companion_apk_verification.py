"""Run the publisher's actual verifier block, including noisy apksigner output."""
import os
import pathlib
import subprocess
import tempfile
import unittest
ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = (ROOT / 'scripts/publish-companion-apk.sh').read_text()
BLOCK = SCRIPT[SCRIPT.index('# 4. Verify'):SCRIPT.index('# 5. Publish')]
CERT = 'd622e07e7f474246e8e2bcee828ca9bf6c6cdf9434c065c58fe89bb4eec2664d'
GOOD = '\n'.join(f'Verified using {v} scheme (test): true' for v in ['v1', 'v2', 'v3']) + '\nSigner #1 certificate SHA-256 digest: ' + CERT

class VerificationTests(unittest.TestCase):
    def check(self, output, code=0):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            verifier = root / 'apksigner'
            verifier.write_text('#!/bin/sh\ncat "$TEST_VERIFY_OUTPUT"\nexit "$TEST_VERIFY_CODE"\n')
            verifier.chmod(0o700)
            (root / 'output').write_text(output)
            env = dict(os.environ, APKSIGNER=str(verifier), SIGNED=str(root / 'test.apk'),
                       TEST_VERIFY_OUTPUT=str(root / 'output'), TEST_VERIFY_CODE=str(code))
            return subprocess.run(['bash', '-c', 'set -euo pipefail\n' + BLOCK], env=env, capture_output=True, text=True).returncode

    def test_noisy_valid_apk_does_not_fail_from_grep_sigpipe(self):
        # Reproduces pipefail+grep -q exit141 after an otherwise valid APK.
        self.assertEqual(self.check(GOOD + '\n' + 'WARNING: compatible signature algorithm\n' * 4000), 0)

    def test_all_three_schemes_are_required(self):
        for version in ['v1', 'v2', 'v3']:
            with self.subTest(version=version):
                self.assertNotEqual(self.check(GOOD.replace(f'{version} scheme (test): true', f'{version} scheme (test): false')), 0)

    def test_wrong_signing_identity_fails(self):
        self.assertNotEqual(self.check(GOOD.replace(CERT, '0' * 64)), 0)

    def test_verifier_failure_is_never_accepted(self):
        self.assertNotEqual(self.check(GOOD, 1), 0)

if __name__ == '__main__':
    unittest.main()
