import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('catalog_drift', ROOT / 'scripts/check-app-catalog-drift.py')
drift = importlib.util.module_from_spec(spec)
spec.loader.exec_module(drift)

class CatalogCapabilities(unittest.TestCase):
    def test_generated_npm_migration_requires_gateway_support_and_preserves_old_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'catalog.json'
            subprocess.run(['bash', str(ROOT / 'scripts/generate-app-catalog.sh'), str(target)], check=True, capture_output=True)
            catalog = json.loads(target.read_text())
            baseline = json.loads((ROOT / 'releases/app-catalog.json').read_text())
            entry = catalog['apps']['nginx-proxy-manager']
            self.assertEqual(entry['manifest'], baseline['apps']['nginx-proxy-manager']['manifest'])
            self.assertEqual(set(entry['manifest_variants'][0]['requires']), {'runtime-migration-backup-v1', 'npm-legacy-host-gateway-v1'})
            selected = drift.load_catalog(target)['nginx-proxy-manager']
            self.assertEqual(selected['container']['network'], 'slirp4netns:allow_host_loopback=true,cidr=169.254.1.0/24')
            entry['manifest_variants'][0]['requires'].append('future-unknown-capability')
            target.write_text(json.dumps(catalog))
            self.assertEqual(drift.load_catalog(target)['nginx-proxy-manager']['container'], entry['manifest']['app']['container'])

if __name__ == '__main__': unittest.main()
