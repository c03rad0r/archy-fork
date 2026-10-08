"""No-network checks using real HTTP request/response parsing over fake sockets."""
import io
import socket
import unittest
from unittest.mock import Mock, patch
from urllib.error import HTTPError
import server

PUBLIC = [(socket.AF_INET, socket.SOCK_STREAM, socket.IPPROTO_TCP, '', ('93.184.216.34', 443))]
PRIVATE = [(socket.AF_INET, socket.SOCK_STREAM, socket.IPPROTO_TCP, '', ('127.0.0.1', 443))]

class FixtureSocket:
    def __init__(self, body=b'{}', redirect=None, status=302):
        headers = f'Location: {redirect}\r\n' if redirect else ''
        self.response = (f'HTTP/1.1 {status if redirect else 200} Fixture\r\n{headers}Content-Length: {len(body)}\r\nConnection: close\r\n\r\n').encode() + body
        self.connected, self.sent, self.closed, self.timeout = [], b'', False, None
    def settimeout(self, value): self.timeout = value
    def connect(self, address): self.connected.append(address)
    def sendall(self, body): self.sent += body
    def makefile(self, *args): return io.BytesIO(self.response)
    def close(self): self.closed = True

class LnurlTransportTests(unittest.TestCase):
    def transport(self, fixture, url='https://provider.example/lnurl', timeout=10):
        context = Mock()
        context.wrap_socket.return_value = fixture
        with patch('server.socket.getaddrinfo', side_effect=[PUBLIC, PRIVATE]) as lookup, patch('server.socket.socket', return_value=fixture), patch('server.ssl.create_default_context', return_value=context), patch.dict('os.environ', {'HTTPS_PROXY':'http://127.0.0.1:9999'}):
            try: return server.fetch_lnurl_json(url, timeout=timeout)
            finally:
                self.assertEqual(lookup.call_count, 1)
                self.assertEqual(fixture.connected, [('93.184.216.34', 443)])
                context.wrap_socket.assert_called_once_with(fixture, server_hostname='provider.example')
                self.assertIn(b'Host: provider.example\r\n', fixture.sent)
                self.assertTrue(fixture.closed)
    def test_redirects_never_connect_to_another_destination(self):
        for destination in ['http://127.0.0.1/admin', 'https://10.0.0.1/private', 'https://other.example/callback']:
            for status in [301, 302, 303, 307, 308]:
                with self.subTest(destination=destination, status=status), self.assertRaisesRegex(ValueError, 'redirects are not permitted'):
                    self.transport(FixtureSocket(redirect=destination, status=status))
    def test_dns_rebinding_cannot_trigger_second_lookup_and_tls_keeps_hostname(self):
        fixture = FixtureSocket(body=b'{"tag":"payRequest"}')
        self.assertEqual(self.transport(fixture), {'tag':'payRequest'})
        self.assertEqual(fixture.timeout, 10)
    def test_response_cap_precedes_json_parsing(self):
        with self.assertRaisesRegex(ValueError, 'response too large'): self.transport(FixtureSocket(body=b' ' * 65537))
    def test_private_initial_resolution_never_opens_socket(self):
        with patch('server.socket.getaddrinfo', return_value=PRIVATE), patch('server.socket.socket') as connect:
            with self.assertRaisesRegex(ValueError, 'public host'): server.fetch_lnurl_json('https://provider.example/lnurl')
            connect.assert_not_called()
    def test_non_https_and_userinfo_never_resolve(self):
        for url in ['http://provider.example', 'https://owner:secret@provider.example', 'https://provider.example/#fragment']:
            with self.subTest(url=url), patch('server.socket.getaddrinfo') as lookup:
                with self.assertRaises(ValueError): server.fetch_lnurl_json(url)
                lookup.assert_not_called()
    def test_screen_metadata_retains_four_second_timeout_and_shared_transport(self):
        with patch('server.fetch_lnurl_json', return_value={'tag':'payRequest','allowsNostr':True,'nostrPubkey':'a'*64}) as fetch, patch('server.helper', return_value={'tips':[]}):
            self.assertEqual(server.screen_tips('fixture-npub','merchant@provider.example'), [])
            fetch.assert_called_once_with('https://provider.example/.well-known/lnurlp/merchant', timeout=4)
        fixture=FixtureSocket();self.transport(fixture,timeout=4);self.assertEqual(fixture.timeout,4)
    def test_metadata_and_invoice_use_restricted_transport(self):
        pay={'tag':'payRequest','callback':'https://provider.example/invoice','minSendable':1,'maxSendable':5000}
        with patch('server.fetch_lnurl_json', side_effect=[pay,{'pr':'fixture-invoice'}]) as fetch, patch('server.fetch_json') as unrestricted:
            result=server.lightning_invoice('merchant@provider.example',1000)
        self.assertEqual(result['bolt11'],'fixture-invoice');self.assertEqual(fetch.call_count,2);unrestricted.assert_not_called()
    def test_zap_fallback_uses_restricted_transport(self):
        pay={'tag':'payRequest','callback':'https://provider.example/invoice','allowsNostr':True,'nostrPubkey':'a'*64}
        with patch('server.helper',return_value={'event':{},'pubkey':'b'*64,'lnurl':'fixture-lnurl'}), patch('server.fetch_lnurl_json',side_effect=[pay,HTTPError('',400,'fixture',{},None),{'pr':'fixture-invoice'}]) as fetch, patch('server.fetch_json') as unrestricted:
            result=server.lightning_invoice('merchant@provider.example',1000)
        self.assertFalse(result['verifiable']);self.assertEqual(fetch.call_count,3);unrestricted.assert_not_called()

if __name__=='__main__': unittest.main()
