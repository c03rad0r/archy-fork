import hashlib
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('gateway_policy', Path(__file__).resolve().parents[2] / 'scripts/public-web-gateway/policy.py')
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)


class PolicyTests(unittest.TestCase):
    def setUp(self):
        self.token = 'synthetic-test-token-' * 3
        self.user = {'user': 'framework', 'metas': {'enrollment_token': self.token}}
        self.entries = {'framework': {'enabled': True, 'token_sha256': hashlib.sha256(self.token.encode()).hexdigest(), 'domains': ['free.archipelago.builders']}}
        self.proxy = {'user': self.user, 'proxy_name': 'framework.website', 'proxy_type': 'https', 'custom_domains': ['free.archipelago.builders']}

    def test_allow_assigned_https_only(self):
        self.assertTrue(policy.authorize('Login', self.user, self.entries))
        for op in ('NewProxy', 'NewUserConn', 'Ping', 'NewWorkConn'):
            self.assertTrue(policy.authorize(op, self.proxy, self.entries))

    def test_revoke_and_rotate_apply_to_all_operations(self):
        for op in policy.OPS:
            content = self.user if op == 'Login' else self.proxy
            self.entries['framework']['enabled'] = False
            self.assertFalse(policy.authorize(op, content, self.entries))
            self.entries['framework']['enabled'] = True
            self.entries['framework']['token_sha256'] = '0' * 64
            self.assertFalse(policy.authorize(op, content, self.entries))

    def test_no_other_domains_protocols_or_shared_groups(self):
        for key, value in [('custom_domains', ['other.example']), ('custom_domains', ['free.archipelago.builders', 'other.example']), ('proxy_type', 'tcp'), ('proxy_type', 'http'), ('remote_port', 22), ('subdomain', 'admin'), ('group', 'shared'), ('locations', ['/']), ('proxy_name', 'another.website')]:
            with self.subTest(key=key, value=value):
                self.assertFalse(policy.authorize('NewProxy', {**self.proxy, key: value}, self.entries))

    def test_missing_or_malformed_auth_is_denied(self):
        for user in ({}, {'user': 'framework'}, {'user': 'framework', 'metas': []}, {'user': 'framework', 'metas': {'enrollment_token': 'wrong'}}):
            self.assertFalse(policy.authorize('Login', user, self.entries))
        self.assertFalse(policy.authorize('Unknown', self.proxy, self.entries))
        self.assertFalse(policy.authorize('Login', self.user, {}))


if __name__ == '__main__':
    unittest.main()
