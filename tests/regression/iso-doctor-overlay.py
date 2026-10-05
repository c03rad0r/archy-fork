#!/usr/bin/env python3
"""Execute the installer's actual overlay against a stale disposable rootfs."""
import pathlib, subprocess, tempfile, shutil, unittest
ROOT = pathlib.Path(__file__).resolve().parents[2]
class DoctorOverlayTests(unittest.TestCase):
    def test_cached_rootfs_and_missing_payload(self):
        source = (ROOT / 'image-recipe/_archived/build-auto-installer-iso.sh').read_text()
        block = source.split('# BEGIN DOCTOR OVERLAY\n', 1)[1].split('# END DOCTOR OVERLAY', 1)[0]
        with tempfile.TemporaryDirectory() as temp:
            base = pathlib.Path(temp)
            target = base / 'target'
            media = base / 'media'
            payload = media / 'archipelago/scripts'
            payload.mkdir(parents=True)
            units = target / 'etc/systemd/system'
            units.mkdir(parents=True)
            for directory in ['opt/archipelago/scripts', 'home/archipelago/archy/scripts']:
                dest = target / directory
                dest.mkdir(parents=True)
                (dest / 'container-doctor.sh').write_text('UNSAFE OLD SCRIPT')
            files = [ROOT / 'scripts/container-doctor.sh',
                     ROOT / 'image-recipe/configs/archipelago-doctor.service',
                     ROOT / 'image-recipe/configs/archipelago-doctor.timer',
                     ROOT / 'image-recipe/configs/archipelago-npm-bridge.service',
                     ROOT / 'image-recipe/configs/archipelago-npm-bridge.timer']
            helpers = [ROOT / 'scripts' / name for name in
                       ['dashboard-public-guard.py', 'npm-public-bridge.py', 'sync-npm-public-hosts.sh', 'filebrowser-credentials.py']]
            for path in helpers:
                shutil.copyfile(path, payload / path.name)
            for path in files:
                shutil.copyfile(path, payload / path.name)
            script = block.replace('/mnt/target', str(target))
            for _ in range(2):
                subprocess.run(['bash', '-c', script], env={'BOOT_MEDIA': str(media), 'PATH': '/usr/bin:/bin'}, check=True)
                for directory in ['opt/archipelago/scripts', 'home/archipelago/archy/scripts']:
                    dest = target / directory / 'container-doctor.sh'
                    self.assertEqual(dest.read_bytes(), files[0].read_bytes())
                    self.assertEqual(dest.stat().st_mode & 0o777, 0o755)
                for path in files[1:]:
                    self.assertEqual((units / path.name).read_bytes(), path.read_bytes())
            for path in helpers:
                installed = target / 'opt/archipelago/scripts' / path.name
                self.assertEqual(installed.read_bytes(), path.read_bytes())
                self.assertEqual(installed.stat().st_mode & 0o777, 0o755)
            self.assertTrue((units / 'timers.target.wants/archipelago-npm-bridge.timer').is_symlink())
            (payload / 'filebrowser-credentials.py').unlink()
            failed = subprocess.run(['bash', '-c', script], env={'BOOT_MEDIA': str(media), 'PATH': '/usr/bin:/bin'}, capture_output=True)
            self.assertNotEqual(failed.returncode, 0, 'Missing credential provisioner must fail installation')
            shutil.copyfile(ROOT / 'scripts/filebrowser-credentials.py', payload / 'filebrowser-credentials.py')
            (payload / 'container-doctor.sh').unlink()
            failed = subprocess.run(['bash', '-c', script], env={'BOOT_MEDIA': str(media), 'PATH': '/usr/bin:/bin'}, capture_output=True)
            self.assertNotEqual(failed.returncode, 0, 'Missing safety overlay must fail installation')
if __name__ == '__main__':
    unittest.main()
