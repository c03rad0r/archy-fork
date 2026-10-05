"""QEMU runner argument/error handling; this is not an actual ISO boot test."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

RUNNER = Path(__file__).resolve().parents[2] / 'image-recipe/_archived/test-iso-qemu.sh'


class QemuRunnerTests(unittest.TestCase):
    def test_command_survives_timeout_and_stale_logs_cannot_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            iso = root / 'candidate with spaces.iso'
            iso.touch()
            executable = root / 'qemu-system-x86_64'
            executable.write_text('''#!/usr/bin/python3
import os,pathlib,sys,time
args=sys.argv[1:]
assert args[args.index('-cdrom')+1]==os.environ['FIXTURE_ISO']
assert args[args.index('-machine')+1]=='pc'
assert 'hostfwd=tcp:127.0.0.1:2222-:22,hostfwd=tcp:127.0.0.1:8100-:80' in args[args.index('-serial')-1]
log=pathlib.Path(args[args.index('-serial')+1].removeprefix('file:'))
mode=os.environ['FIXTURE_MODE']
if mode=='login':log.write_text('Debian GNU/Linux 13 archipelago-installer ttyS0\\n')
elif mode!='silent':log.write_text('systemd[1]: boot fixture marker\\n')
if mode=='crash':sys.exit(5)
if mode=='timeout':time.sleep(10)
''')
            executable.chmod(0o755)
            # A dedicated empty fixture disk avoids requiring qemu-img here.
            (root / 'archipelago-test-disk.qcow2').touch()
            env = dict(os.environ, PATH=str(root)+':'+os.environ['PATH'],
                       TMPDIR=str(root), FIXTURE_ISO=str(iso))
            for mode, expected in [('timeout', 0), ('crash', 5), ('silent', 1), ('login', 0)]:
                with self.subTest(mode=mode):
                    (root / 'archipelago-qemu-serial.log').write_text('systemd[1]: stale pass\n')
                    result = subprocess.run(['bash', str(RUNNER), str(iso), '--bios', '1'],
                                            env=dict(env, FIXTURE_MODE=mode),
                                            capture_output=True, text=True, timeout=15)
                    self.assertEqual(result.returncode, expected, result.stdout+result.stderr)


if __name__ == '__main__':
    unittest.main()
