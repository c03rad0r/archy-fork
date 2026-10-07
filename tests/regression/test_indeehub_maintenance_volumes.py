#!/usr/bin/env python3
"""Restore fixture-only volume archives with the production maintenance gate.

Requires rootless Podman. Creates no containers and never opens live app volumes.
"""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import uuid

MODULE = Path(__file__).resolve().parents[2] / 'scripts/indeehub-maintenance-controller.py'
spec = importlib.util.spec_from_file_location('maintenance', MODULE)
maintenance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(maintenance)


def main():
    with tempfile.TemporaryDirectory(prefix='archy-volume-restore-') as temporary:
        root = Path(temporary)
        source = root/'source'
        source.mkdir()
        (source/'.hidden').write_bytes(b'retained hidden object\x00')
        (source/'nested').mkdir()
        original = source/'nested'/'media'
        original.write_bytes(bytes(range(256))*4096)
        original.chmod(0o640)
        os.link(original, source/'hardlink')
        (source/'symlink').symlink_to('nested/media')
        os.setxattr(original, 'user.archy-fixture', b'retained metadata')
        # Exercise numeric ownership which the calling host user cannot reproduce
        # without the rootless user namespace used by production backup/restore.
        subprocess.run(['podman','unshare','chown','101:102',str(original)],check=True)
        subprocess.run(['podman','unshare','setfacl','-m','u:103:r--',str(original)],check=True)
        operation = str(uuid.uuid4())
        controller = maintenance.Controller(root/'data',operation,0)
        controller.record = {'operation_id':operation,'artifacts':{}}
        holds = controller.data/'update-transactions'/'holds'
        holds.mkdir(parents=True)
        for name in maintenance.NAMES:(holds/name).write_text(operation)
        controller.fence.parent.mkdir(parents=True)
        controller.fence.write_text(operation)
        backup = controller.root/'backup'
        backup.mkdir(parents=True)
        for name in ('database.dump',*(v+'.tar' for v in maintenance.VOLUMES)):
            path = backup/name
            if name=='database.dump':path.write_bytes(b'database checked by separate fixture')
            else:
                subprocess.run(['podman','unshare','tar','--xattrs','--acls','--numeric-owner',
                    '-C',str(source),'-cpf',str(path),'.'],check=True)
            controller.record['artifacts'][name]={'bytes':path.stat().st_size,'sha256':maintenance.sha(path)}
        controller.save()
        try:
            controller.verify_volume_backups()
            assert controller.record['volume_restore_verified']==controller.volume_restore_terms()
            assert 'volume_restore_fixture' not in controller.record
            controller.verify_volume_backups() # durable proof is reusable
            controller.record.pop('volume_restore_verified')
            actual_run = controller.run
            def corrupt_restored(argv,**kwargs):
                if '-df' in argv:
                    payload = Path(argv[argv.index('-C')+1])
                    subprocess.run(['podman','unshare','sh','-c','printf changed > "$1/.hidden"','fixture',str(payload)],check=True)
                return actual_run(argv,**kwargs)
            controller.run=corrupt_restored
            try:controller.verify_volume_backups()
            except subprocess.CalledProcessError:pass
            else:raise AssertionError('Changed restoration accepted')
            assert 'volume_restore_verified' not in controller.record
            assert 'volume_restore_fixture' not in controller.record
            assert controller.fence.read_text()==operation
            controller.run=actual_run
            def corrupt_metadata(argv,**kwargs):
                result=actual_run(argv,**kwargs)
                if '-xpf' in argv:
                    payload=Path(argv[argv.index('-C')+1])
                    subprocess.run(['podman','unshare','python3','-c',
                        "import os,sys;os.setxattr(sys.argv[1],'user.archy-fixture',b'changed')",
                        str(payload/'nested'/'media')],check=True)
                return result
            controller.run=corrupt_metadata
            try:controller.verify_volume_backups()
            except RuntimeError as error:assert 'metadata differs' in str(error)
            else:raise AssertionError('Changed xattr restoration accepted')
            assert 'volume_restore_verified' not in controller.record
            assert 'volume_restore_fixture' not in controller.record
            controller.run=actual_run
            path=backup/(maintenance.VOLUMES[0]+'.tar')
            path.write_bytes(b'not a tar archive')
            controller.record['artifacts'][path.name]={'bytes':path.stat().st_size,'sha256':maintenance.sha(path)}
            try:controller.verify_volume_backups()
            except maintenance.tarfile.ReadError:pass
            else:raise AssertionError('Unreadable archive accepted')
            assert 'volume_restore_verified' not in controller.record
            assert controller.fence.read_text()==operation
            print(json.dumps({'production_volume_restore_barrier':'passed','volumes':4,
                'hidden_files':True,'hardlinks':True,'symlinks':True,'numeric_ownership':True,
                'xattrs':True,'acls':True,'corrupt_xattr_rejected':True,'corrupt_restore_rejected':True,'unreadable_archive_rejected':True,
                'live_volumes_opened':False}))
        finally:
            subprocess.run(['podman','unshare','rm','-rf','--',str(source)],check=True)

if __name__=='__main__':main()
