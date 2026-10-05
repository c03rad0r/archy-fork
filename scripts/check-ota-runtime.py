#!/usr/bin/env python3
"""Reject OTA archives that would overwrite the qualified first-boot repair."""
import argparse
import pathlib
import tarfile


def check(archive, source):
    member_name = 'archipelago-runtime/scripts/first-boot-containers.sh'
    with tarfile.open(archive, 'r:gz') as package:
        matches = [m for m in package.getmembers()
                   if m.name.removeprefix('./') == member_name]
        if len(matches) != 1 or not matches[0].isfile():
            raise ValueError('OTA must contain exactly one regular first-boot script')
        with package.extractfile(matches[0]) as stream:
            actual = stream.read()
    if actual != source.read_bytes():
        raise ValueError('OTA first-boot script differs from qualified source; repackage before signing')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=pathlib.Path)
    args = parser.parse_args()
    try:
        check(args.archive, pathlib.Path(__file__).resolve().parents[1] /
              'scripts/first-boot-containers.sh')
    except (OSError, ValueError, tarfile.TarError) as error:
        parser.exit(1, f'FAIL: {error}\n')
    print('PASS: OTA first-boot script matches qualified source')


if __name__ == '__main__':
    main()
