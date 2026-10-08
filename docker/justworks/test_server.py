import io
import json
import unittest
from unittest.mock import patch
import server

NPUB = 'npub1' + 'q' * 58


class SiteTests(unittest.TestCase):
    def test_only_canonical_core_page_or_slug_is_accepted(self):
        self.assertEqual(server.site_id(' https://justworks.cash/test-bakery/ '), 'test-bakery')
        self.assertEqual(server.site_id('test-bakery'), 'test-bakery')
        for invalid in ['http://127.0.0.1/', 'https://justworks.cash.evil/test',
                        'https://justworks.cash@evil/test', 'https://justworks.cash:444/test',
                        'https://justworks.cash/test?ownerKey=secret', '../api', 'a/b',
                        'https://justworks.cash/%2e%2e/secret', 'x' * 161, 'nsec1secret', 'ncryptsec1secret']:
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                server.site_id(invalid)

    def test_returns_only_public_links_without_owner_material(self):
        record = {'id': 'bakery', 'npub': NPUB, 'name': '<script>alert(1)</script>',
                  'nsite': {'deploymentStatus': 'published'}, 'ownerKey': 'secret',
                  'qrUrl': 'https://evil.test', 'editorUrl': 'javascript:alert(1)'}
        with patch('server.build_opener') as opener:
            opener.return_value.open.side_effect = [io.BytesIO(json.dumps({'nsite': record}).encode()),
                io.BytesIO(json.dumps({'places': [{'npub': NPUB, 'slug': 'bakery'}]}).encode())]
            result = server.read_site('bakery')
            request = opener.return_value.open.call_args_list[0].args[0]
            self.assertEqual(request.full_url, 'https://justworks.cash/api/nsites/bakery')
        self.assertEqual(result['url'], 'https://justworks.cash/bakery')
        self.assertEqual(result['editorUrl'], 'https://justworks.cash/cms?id=bakery')
        self.assertNotIn('secret', json.dumps(result))
        self.assertNotIn('evil.test', json.dumps(result))
        self.assertTrue(result['businessAvailable'])

    def test_business_lookup_does_not_require_regional_directory(self):
        record = {'id': 'bakery', 'npub': NPUB, 'nsite': {'deploymentStatus': 'published'}}
        with patch('server.read_core', return_value={'nsite':record}) as read:
            result = server.read_site('bakery')
            self.assertTrue(result['businessAvailable'])
            self.assertEqual(result['businessUrl'], '/business-demo/?npub='+NPUB)
            read.assert_called_once()

    def test_incomplete_failed_and_mismatched_sites_never_complete_setup(self):
        for record in [None, {}, {'id': 'bakery', 'npub': NPUB, 'status': 'active'},
                       {'id': 'bakery', 'npub': NPUB, 'nsite': {'deploymentStatus': 'relay-publish-failed'}},
                       {'id': 'another-page', 'npub': NPUB, 'nsite': {'deploymentStatus': 'published'}}]:
            with self.subTest(record=record), patch('server.build_opener') as opener:
                opener.return_value.open.return_value = io.BytesIO(json.dumps({'nsite': record}).encode())
                with self.assertRaises((ValueError, AttributeError)):
                    server.read_site('bakery')

    def test_npub_resolves_to_its_canonical_site_without_accepting_mismatch(self):
        record = {'id':'bakery','npub':NPUB,'nsite':{'deploymentStatus':'published'}}
        with patch('server.read_core', side_effect=[{'nsite':record},{'places':[]}]):
            self.assertEqual(server.read_site(NPUB)['id'],'bakery')

    def test_no_remote_redirects_and_bounded_response(self):
        self.assertIsNone(server.NoRedirect().redirect_request(None, None, 302, '', {}, 'http://127.0.0.1'))
        with patch('server.build_opener') as opener:
            opener.return_value.open.return_value = io.BytesIO(b' ' * (server.MAX_RESPONSE + 1))
            with self.assertRaises(ValueError): server.read_site('bakery')


if __name__ == '__main__': unittest.main()
