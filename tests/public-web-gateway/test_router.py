import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('node_router', Path(__file__).resolve().parents[2] / 'docker/public-web-router/router.py')
router = importlib.util.module_from_spec(spec)
spec.loader.exec_module(router)

class RouterTests(unittest.TestCase):
    def setUp(self):
        self.config = {'schema': 1, 'gateway': {'host': '192.0.2.1', 'port': 7400, 'node_id': 'node-a', 'transport_token': 'a'*64, 'enrollment_token': 'b'*64, 'ca_pem': '-----BEGIN CERTIFICATE-----\nexample\n-----END CERTIFICATE-----', 'tls_server_name': 'gateway.example', 'domains': ['site.example']}, 'routes': [{'id': 'site-a', 'domain': 'site.example', 'fips_address': 'fd00::1', 'port': 32000}]}

    def test_tls_ends_on_node_with_pinned_control_channel(self):
        caddy, frpc, pem = router.render(self.config)
        self.assertIn('disable_http_challenge', caddy)
        self.assertNotIn('tls internal', caddy)
        self.assertIn('bind 127.0.0.1', caddy)
        self.assertEqual(frpc['proxies'][0]['type'], 'https')
        self.assertEqual(frpc['transport']['tls']['serverName'], 'gateway.example')
        self.assertIn('trustedCaFile', frpc['transport']['tls'])

    def test_refuses_management_and_arbitrary_upstreams(self):
        for port in (22, 443, 7474, 8191, 31999, 32032, True):
            self.config['routes'][0]['port'] = port
            with self.assertRaises(ValueError): router.render(self.config)
        self.config['routes'][0]['port'] = 32000
        for address in ('127.0.0.1', '::1', '2001:db8::1'):
            self.config['routes'][0]['fips_address'] = address
            with self.assertRaises(ValueError): router.render(self.config)

    def test_refuses_config_injection_and_duplicate_domains(self):
        for key in ('domain', 'id'):
            old = self.config['routes'][0][key]
            self.config['routes'][0][key] = 'site.example\n import /secret'
            with self.assertRaises(ValueError): router.render(self.config)
            self.config['routes'][0][key] = old
        self.config['routes'].append(dict(self.config['routes'][0]))
        with self.assertRaises(ValueError): router.render(self.config)

    def test_app_routes_keep_the_expected_app_gate_identity(self):
        self.config['routes'][0].update(id='app-photoprism', app_id='photoprism', port=2342)
        caddy, _, _ = router.render(self.config)
        self.assertIn('header_up X-Archipelago-App photoprism', caddy)
        self.config['routes'][0]['id'] = 'app-another'
        with self.assertRaises(ValueError): router.render(self.config)

    def test_test_certificates_require_explicit_mode(self):
        self.config['certificate_mode'] = 'test'
        self.assertIn('tls internal', router.render(self.config)[0])
        self.config['certificate_mode'] = 'insecure'
        with self.assertRaises(ValueError): router.render(self.config)

if __name__ == '__main__': unittest.main()
