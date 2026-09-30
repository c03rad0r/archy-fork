#!/usr/bin/env python3
import importlib.util
import io
import json
import pathlib
import unittest
import urllib.error

ROOT = pathlib.Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('diagnostic', ROOT / 'scripts/check-portainer-git-source.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class Response(io.BytesIO):
    pass


class FakeAPI:
    def __init__(self, result=None, error=None):
        self.result, self.error, self.request = result, error, None
    def open(self, request, timeout):
        self.request = request
        if self.error:
            raise self.error
        return Response(json.dumps(self.result).encode())


class Diagnostics(unittest.TestCase):
    def test_server_context_credentials_not_in_url_and_tls_stays_enabled(self):
        api = FakeAPI({'success': True})
        result = m.check('http://localhost:9000', 'http://node:3001/user/repo',
                         {'jwt': 'test-jwt', 'git': {'username': 'test-user', 'password': 'test-secret'}}, api)
        self.assertTrue(result['success'])
        self.assertEqual(api.request.full_url, 'http://localhost:9000/api/gitops/sources/test')
        payload = json.loads(api.request.data)
        self.assertFalse(payload['tlsSkipVerify'])
        self.assertEqual(payload['authentication']['password'], 'test-secret')
        self.assertNotIn('test-secret', json.dumps(result))

    def test_failure_categories_from_source_api(self):
        cases = [('dial tcp: connection refused', 'connection-refused'),
                 ('lookup node: no such host', 'dns-failure'),
                 ('context deadline exceeded', 'timeout'),
                 ('unexpected content-type text/html', 'proxy-or-login-interception'),
                 ('authentication required', 'repository-authentication'),
                 ('x509: certificate signed by unknown authority', 'tls-failure'),
                 ('repository not found', 'repository-not-found-or-private')]
        for error, expected in cases:
            with self.subTest(error=error):
                result = m.check('http://localhost:9000', 'http://node/repo', {'jwt': 'test'}, FakeAPI({'success': False, 'error': error}))
                self.assertEqual(result['category'], expected)
                self.assertFalse(result['success'])

    def test_portainer_auth_is_distinct_from_repository_auth(self):
        api = FakeAPI(error=urllib.error.HTTPError('http://localhost', 401, 'Unauthorized', {}, None))
        self.assertEqual(m.check('http://localhost', 'http://node/repo', {'jwt': 'bad'}, api)['category'], 'portainer-authentication')

    def test_html_or_malformed_api_response_never_proves_git_success(self):
        for value in ({'status': 1}, {'success': 'true'}, [], '<html>login</html>'):
            self.assertFalse(m.check('http://localhost', 'http://node/repo', {'jwt': 'test'}, FakeAPI(value))['success'])

    def test_credential_urls_rejected_before_request(self):
        for value in ('http://user:secret@node/repo', 'http://node/repo?token=secret', 'file:///data/repo'):
            with self.assertRaises(ValueError):
                m.check('http://localhost', value, {'jwt': 'test'}, FakeAPI())


if __name__ == '__main__':
    unittest.main()
