import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[2] / 'scripts/public-web-gateway/enroll.py'
class EnrollmentTests(unittest.TestCase):
    def test_private_export_duplicate_domain_and_explicit_rotation(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            subprocess.run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(root/'key'), '-out', str(root/'ca'), '-days', '1', '-subj', '/CN=gateway.example'], check=True, capture_output=True)
            config = {'bindPort': 7400, 'auth': {'method': 'token', 'token': 't'*64}, 'transport': {'tls': {'force': True}}, 'httpPlugins': [{'ops': ['Login', 'NewProxy', 'Ping', 'NewWorkConn', 'NewUserConn']}]}
            path = root/'frps.json'; path.write_text(json.dumps(config)); path.chmod(0o600)
            base = [sys.executable, str(SCRIPT), '--frps-config', str(path), '--policy', str(root/'policy.json'), '--ca', str(root/'ca'), '--host', 'gateway.example', '--tls-server-name', 'gateway.example', '--domain', 'site.example']
            result = subprocess.run(base + ['--name', 'node-a', '--output', str(root/'node-a.json')], capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            private = json.loads((root/'node-a.json').read_text())
            self.assertNotIn(private['enrollment_token'].encode(), result.stdout + result.stderr)
            self.assertEqual((root/'node-a.json').stat().st_mode & 0o777, 0o600)
            first = (root/'policy.json').read_bytes()
            result = subprocess.run(base + ['--name', 'node-b', '--output', str(root/'node-b.json')], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(first, (root/'policy.json').read_bytes())
            self.assertFalse((root/'node-b.json').exists())
            result = subprocess.run(base + ['--name', 'node-a', '--rotate', '--output', str(root/'rotated.json')], capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            self.assertNotEqual(private['enrollment_token'], json.loads((root/'rotated.json').read_text())['enrollment_token'])
            self.assertNotEqual(first, (root/'policy.json').read_bytes())

if __name__ == '__main__': unittest.main()
