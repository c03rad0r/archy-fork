#!/usr/bin/env python3
"""Keep the node TLS leaf private while allowing the management daemon to read it."""
import argparse
import os
from pathlib import Path
import pwd
import stat


def repair(key: Path, service_uid: int, service_gid: int) -> bool:
    try:
        fd = os.open(key, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    except FileNotFoundError:
        return False  # A node without TLS is not provisioned by this repair.
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_uid not in (0, service_uid):
            raise RuntimeError('Unexpected node TLS key type or owner; left unchanged')
        # The daemon's primary group is the sole additional reader. Never make
        # the key world-readable or grant group write/execute permissions.
        mode = 0o640 if before.st_mode & stat.S_IWUSR else 0o440
        if before.st_gid == service_gid and stat.S_IMODE(before.st_mode) == mode:
            return False
        os.fchown(fd, -1, service_gid)
        os.fchmod(fd, mode)
        os.fsync(fd)
        return True
    finally:
        os.close(fd)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--key-name', choices=['archipelago.key', 'archipelago.key.new', 'archipelago.key.rotnew'], default='archipelago.key')
    args = parser.parse_args()
    account = pwd.getpwnam('archipelago')
    changed = repair(Path('/etc/archipelago/ssl') / args.key_name, account.pw_uid, account.pw_gid)
    print('Node app TLS permissions repaired' if changed else 'Node app TLS permissions unchanged')
