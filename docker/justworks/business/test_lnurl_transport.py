"""No-network checks of the LNURL transport trust boundary."""
import io
import unittest
from email.message import Message
from unittest.mock import patch
from urllib.error import HTTPError
from urllib.request import HTTPSHandler, build_opener
from urllib.response import addinfourl
import server

class FixtureHttps(HTTPSHandler):
    def __init__(self, body=b'{}', redirect=None, status=302):
        super().__init__()
        self.body, self.redirect, self.status, self.calls = body, redirect, status, []
    def https_open(self, request):
        self.calls.append(request.full_url)
        headers = Message()
        if self.redirect: headers['Location'] = self.redirect
        response = addinfourl(io.BytesIO(self.body), headers, request.full_url, self.status if self.redirect else 200)
        response.msg = 'Found' if self.redirect else 'OK'
        return response

class LnurlTransportTests(unittest.TestCase):
    def transport(self, fixture):
        with patch('server.assert_public_https', side_effect=lambda value: value), patch('server.build_opener', side_effect=lambda *handlers: build_opener(*handlers, fixture)):
            return server.fetch_lnurl_json('https://provider.example/lnurl')
    def test_actual_redirect_chain_refuses_internal_and_public_destinations(self):
        for destination in ['http://127.0.0.1/admin', 'https://10.0.0.1/private', 'https://other.example/callback']:
            for status in [301, 302, 303, 307, 308]:
                fixture = FixtureHttps(redirect=destination, status=status)
                with self.subTest(destination=destination, status=status):
                    with self.assertRaisesRegex(ValueError, 'redirects are not permitted'): self.transport(fixture)
                    self.assertEqual(fixture.calls, ['https://provider.example/lnurl'])
    def test_response_cap_precedes_json_parsing(self):
        with self.assertRaisesRegex(ValueError, 'response too large'): self.transport(FixtureHttps(body=b' ' * 65537))
    def test_valid_bounded_json(self):
        self.assertEqual(self.transport(FixtureHttps(body=b'{"tag":"payRequest"}')), {'tag':'payRequest'})
    def test_unsafe_initial_url_never_opens_transport(self):
        with patch('server.assert_public_https', side_effect=ValueError('unsafe')), patch('server.build_opener') as opener:
            with self.assertRaises(ValueError): server.fetch_lnurl_json('https://127.0.0.1/private')
            opener.assert_not_called()
    def test_metadata_and_invoice_use_restricted_transport(self):
        pay = {'tag':'payRequest','callback':'https://provider.example/invoice','minSendable':1,'maxSendable':5000}
        with patch('server.assert_public_https', side_effect=lambda value:value), patch('server.fetch_lnurl_json', side_effect=[pay,{'pr':'fixture-invoice'}]) as fetch, patch('server.fetch_json') as unrestricted:
            result=server.lightning_invoice('merchant@provider.example',1000)
        self.assertEqual(result['bolt11'],'fixture-invoice');self.assertEqual(fetch.call_count,2);unrestricted.assert_not_called()
    def test_zap_fallback_uses_restricted_transport(self):
        pay={'tag':'payRequest','callback':'https://provider.example/invoice','allowsNostr':True,'nostrPubkey':'a'*64}
        with patch('server.assert_public_https',side_effect=lambda value:value), patch('server.helper',return_value={'event':{},'pubkey':'b'*64,'lnurl':'fixture-lnurl'}), patch('server.fetch_lnurl_json',side_effect=[pay,HTTPError('',400,'fixture',{},None),{'pr':'fixture-invoice'}]) as fetch, patch('server.fetch_json') as unrestricted:
            result=server.lightning_invoice('merchant@provider.example',1000)
        self.assertFalse(result['verifiable']);self.assertEqual(fetch.call_count,3);unrestricted.assert_not_called()

if __name__=='__main__': unittest.main()
