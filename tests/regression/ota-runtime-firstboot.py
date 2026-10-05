#!/usr/bin/env python3
"""Exercise the release payload gate with real archives, including stale bytes."""
import importlib.util
import io
import pathlib
import tarfile
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('check_ota_runtime', ROOT / 'scripts/check-ota-runtime.py')
validator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(validator)


class RuntimePayloadTests(unittest.TestCase):
    def test_actual_archives(self):
        with tempfile.TemporaryDirectory() as directory:
            base = pathlib.Path(directory)
            source = base / 'first-boot-containers.sh'
            source.write_bytes(b'#!/bin/sh\necho qualified\n')
            for case, contents, valid in [
                ('qualified', [source.read_bytes()], True),
                ('stale', [b'#!/bin/sh\necho obsolete\n'], False),
                ('missing', [], False),
                ('duplicate', [source.read_bytes(), source.read_bytes()], False),
            ]:
                with self.subTest(case=case):
                    archive = base / (case + '.tar.gz')
                    with tarfile.open(archive, 'w:gz') as output:
                        for content in contents:
                            member = tarfile.TarInfo('./archipelago-runtime/scripts/first-boot-containers.sh')
                            member.size = len(content)
                            output.addfile(member, io.BytesIO(content))
                    if valid:
                        validator.check(archive, source)
                    else:
                        with self.assertRaises(ValueError):
                            validator.check(archive, source)


if __name__ == '__main__':
    unittest.main()
