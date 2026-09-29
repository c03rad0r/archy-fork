#!/usr/bin/env python3
"""Exercise actual manifest entrypoints with a fake bitcoind; no node data touched."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[2]

class PruningEntrypoint(unittest.TestCase):
    def test_auto_and_user_choice_for_both_bitcoin_implementations(self):
        for app in ('bitcoin-core', 'bitcoin-knots'):
            manifest = yaml.safe_load((ROOT / 'apps' / app / 'manifest.yml').read_text())
            command = manifest['app']['container']['custom_args'][0]
            for disk, choice, pruned in [(500,'0',True),(999,'0',True),(1000,'0',False),(2000,'0',False),(2000,'1',True),(500,'1',True)]:
                with self.subTest(app=app,disk=disk,choice=choice), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    binary = root / 'bitcoind'
                    binary.write_text('#!/usr/bin/env python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\n')
                    binary.chmod(0o755)
                    env = dict(os.environ, PATH=directory+':'+os.environ['PATH'], DISK_GB=str(disk), BITCOIN_PRUNE=choice,
                               BITCOIN_RPC_USER='test',BITCOIN_RPC_PASS='test')
                    # Isolate the ephemeral RPC config too.
                    script = command.replace('/tmp/rpc.conf',str(root/'rpc.conf'))
                    args = json.loads(subprocess.check_output(['sh','-c',script],env=env,text=True))
                    self.assertEqual('-prune=50000' in args,pruned)
                    self.assertEqual('-txindex=1' in args,not pruned)
                    self.assertIn('-server=1',args)

if __name__ == '__main__': unittest.main()
