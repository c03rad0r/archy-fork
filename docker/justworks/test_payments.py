import unittest
import http.client
import threading
from http.server import ThreadingHTTPServer
import server
from unittest.mock import patch
from urllib.error import HTTPError
import payments

BODY = {'siteId':'bakery', 'address':'mywallet@minibits.cash', 'credentials':{'ownerIdentifier':'owner', 'ownerPassword':'fake-test-password'}}
def record(address='mywallet@minibits.cash'):
    return {'id':'bakery', 'nsite':{'deploymentStatus':'published'}, 'identity':{'profile':{'lud16':address}}}

class PaymentTests(unittest.TestCase):
    def test_saves_only_address_and_checks_published_destination(self):
        replies = [{'nsite':{'id':'bakery'}}, {}, {'artifact':{'verified':True}, 'nsite':record()}, {'nsite':record()}]
        with patch('payments.request_core', side_effect=replies) as call:
            result = payments.connect_payment(BODY)
            self.assertTrue(result['verified'])
            update = call.call_args_list[1]
            self.assertEqual(update.args[:2], ('PATCH','/api/nsites/bakery'))
            self.assertEqual(set(update.args[2]), {'payment','ownerIdentifier','ownerPassword'})
            self.assertEqual(update.args[2]['payment'], BODY['address'])
            self.assertEqual(call.call_args_list[-1].args, ('GET','/api/nsites/bakery'))
            self.assertNotIn('password', str(result))

    def test_rejected_owner_never_changes_or_publishes(self):
        with patch('payments.request_core', side_effect=HTTPError('https://justworks.cash',401,'unauthorized',{},None)) as call:
            with self.assertRaises(HTTPError): payments.connect_payment(BODY)
            self.assertEqual(call.call_count,1)

    def test_wrong_website_owner_never_changes(self):
        with patch('payments.request_core', return_value={'nsite':{'id':'other'}}) as call:
            with self.assertRaises(ValueError): payments.connect_payment(BODY)
            self.assertEqual(call.call_count,1)

    def test_partial_publish_and_wrong_destination_never_succeed(self):
        for deploy, public in [({}, {'nsite':record()}),
            ({'artifact':{'verified':True},'nsite':record('other@minibits.cash')},{'nsite':record()}),
            ({'artifact':{'verified':True},'nsite':record()},{'nsite':record('other@minibits.cash')}),
            (OSError('timeout'), {})]:
            with self.subTest(deploy=deploy), patch('payments.request_core', side_effect=[{'nsite':{'id':'bakery'}},{},deploy,public]):
                with self.assertRaises(payments.PublishUnverified): payments.connect_payment(BODY)

    def test_invalid_input_never_reaches_upstream(self):
        for body in [None, {}, {**BODY,'siteId':'../admin'}, {**BODY,'address':'someone@evil.test'}, {**BODY,'credentials':{}}, {**BODY,'credentials':{'ownerKey':['invalid']}}]:
            with self.subTest(body=body), patch('payments.request_core') as call:
                with self.assertRaises(ValueError): payments.connect_payment(body)
                call.assert_not_called()

    def test_concurrent_updates_are_rejected_without_writes(self):
        with payments.LOCK, patch('payments.request_core') as call:
            with self.assertRaises(ValueError): payments.connect_payment(BODY)
            call.assert_not_called()

class PaymentHttpTests(unittest.TestCase):
    def test_post_requires_exact_origin_json_and_bounded_body(self):
        httpd = ThreadingHTTPServer(('127.0.0.1', 0), server.Handler)
        worker = threading.Thread(target=httpd.serve_forever, daemon=True); worker.start()
        host = f'127.0.0.1:{httpd.server_port}'
        try:
            with patch('server.connect_payment', return_value={'verified':True}) as connect:
                for origin, content_type, body, expected in [
                    ('https://evil.test','application/json','{}',403),
                    ('','application/json','{}',403),
                    ('http://'+host,'text/plain','{}',415),
                    ('http://'+host,'application/json','x'*8193,413),
                    ('http://'+host,'application/json','{}',200)]:
                    client = http.client.HTTPConnection(host)
                    client.request('POST','/api/payment-connect',body,{'Origin':origin,'Content-Type':content_type})
                    response=client.getresponse(); self.assertEqual(response.status,expected); response.read(); client.close()
                self.assertEqual(connect.call_count,1)
        finally:
            httpd.shutdown(); httpd.server_close(); worker.join()

if __name__ == '__main__': unittest.main()
