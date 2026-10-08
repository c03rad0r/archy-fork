"""Config upgrades must preserve payout policy and reject broken input."""
import json
import os
from pathlib import Path
import subprocess
import unittest

FILTER = Path(__file__).resolve().parents[1] / 'configure.jq'


def configure(value, host='bitcoin-core', password='new-rpc'):
    return subprocess.run(['jq', '-e', '-f', str(FILTER)], input=json.dumps(value),
                          text=True, capture_output=True,
                          env={**os.environ, 'BITCOIN_RPC_HOST': host,
                               'BITCOIN_RPC_PASSWORD': password,
                               'DATUM_ADMIN_PASSWORD': 'test-admin'})


class ConfigTests(unittest.TestCase):
    def test_first_run_has_no_borrowed_payout_address(self):
        result = configure({})
        self.assertEqual(result.returncode, 0, result.stderr)
        data = json.loads(result.stdout)
        self.assertEqual(data['mining']['pool_address'], '')
        self.assertTrue(data['datum']['pooled_mining_only'])
        self.assertTrue(data['api']['modify_conf'])

    def test_restart_preserves_payout_and_explicit_false_settings(self):
        original = {'mining': {'pool_address': 'operator-address'},
                    'datum': {'pool_pass_workers': False, 'pool_pass_full_users': False,
                              'pooled_mining_only': False},
                    'bitcoind': {'rpcurl': 'http://old-ip:8332', 'rpcpassword': 'old'}}
        result = configure(original, password='quotes"and\\slashes')
        self.assertEqual(result.returncode, 0, result.stderr)
        data = json.loads(result.stdout)
        self.assertEqual(data['mining']['pool_address'], 'operator-address')
        self.assertEqual(data['datum'], original['datum'])
        self.assertEqual(data['bitcoind']['rpcurl'], 'http://bitcoin-core:8332')
        self.assertEqual(data['bitcoind']['rpcpassword'], 'quotes"and\\slashes')

    def test_invalid_root_is_rejected(self):
        for value in [None, [], 'broken', 1]:
            self.assertNotEqual(configure(value).returncode, 0)


if __name__ == '__main__':
    unittest.main()
