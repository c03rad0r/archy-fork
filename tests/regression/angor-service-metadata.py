#!/usr/bin/env python3
"""Headless store apps must be discoverable without acquiring a UI launcher."""
import importlib.util
import pathlib
import unittest
import yaml

ROOT = pathlib.Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('catalog_generator', ROOT / 'scripts/generate-app-catalog.py')
generator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generator)

class ServiceMetadata(unittest.TestCase):
    def test_headless_services_have_mesh_ports_but_no_browser_launcher(self):
        for name, port in [('angor-indexer', 8998), ('angor-relay', 8091)]:
            app = yaml.safe_load((ROOT / 'apps' / name / 'manifest.yml').read_text())['app']
            self.assertIsNone(generator.manifest_launch_port(app))
            self.assertEqual(generator.manifest_service_ports(app), [port])
            self.assertEqual(app['ports'][0]['bind'], '127.0.0.1')
            self.assertEqual(app['security']['capabilities'], [])

    def test_indexer_reuses_both_existing_mempool_components(self):
        app = yaml.safe_load((ROOT / 'apps/angor-indexer/manifest.yml').read_text())['app']
        self.assertEqual(set(app['install_prerequisites']), {'mempool', 'mempool-api'})
        self.assertTrue({'mempool', 'mempool-api'} <= {
            dep['app_id'] for dep in app['dependencies'] if 'app_id' in dep})
        self.assertEqual(app.get('volumes', []), [])

    def test_indexer_health_targets_ipv4_listener(self):
        app = yaml.safe_load((ROOT / 'apps/angor-indexer/manifest.yml').read_text())['app']
        self.assertEqual(app['health_check']['endpoint'], 'http://127.0.0.1:8080')
        self.assertEqual(app['health_check']['path'], '/health')

    def test_host_local_api_never_opens_mesh_port(self):
        app = {'interfaces': {'main': {'type': 'api', 'port': 8999}},
               'ports': [{'host': 8999, 'auth': 'local'}],
               'health_check': {'type': 'http'}}
        self.assertIsNone(generator.manifest_launch_port(app))
        self.assertEqual(generator.manifest_service_ports(app), [])

    def test_legacy_ui_fallback_retained(self):
        self.assertEqual(generator.manifest_launch_port({'ports': [{'host': 8080}],
            'health_check': {'type': 'http'}}), 8080)

    def test_relay_storage_cannot_share_node_identity_or_database(self):
        node = yaml.safe_load((ROOT / 'apps/strfry/manifest.yml').read_text())['app']
        angor = yaml.safe_load((ROOT / 'apps/angor-relay/manifest.yml').read_text())['app']
        node_paths = {v['source'] for v in node['volumes']}
        self.assertTrue(node_paths.isdisjoint(v['source'] for v in angor['volumes']))
        self.assertTrue(all(not f['overwrite'] for f in angor['files']))
        self.assertEqual(angor['interfaces']['main']['type'], 'api')

if __name__ == '__main__':
    unittest.main()
