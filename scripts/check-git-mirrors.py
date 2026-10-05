#!/usr/bin/env python3
"""Read-only check of advertised Git refs; does not inspect PR metadata."""
import argparse
import re
import subprocess
import sys


def git(*args):
    result = subprocess.run(['git', *args], capture_output=True, text=True,
                            timeout=120)
    if result.returncode:
        # Transport errors can contain credential-bearing remote URLs.
        raise ValueError('Git lookup failed; check mirror access privately')
    return result.stdout


def parse_refs(output):
    return {ref: sha for sha, ref in (line.split() for line in output.splitlines())
            if ref.startswith(('refs/heads/', 'refs/tags/'))}


def compare(left, right, refs):
    failures = []
    for ref in sorted(refs):
        if ref not in left or ref not in right:
            failures.append(f'{ref}: missing from at least one side')
        elif left[ref] != right[ref]:
            failures.append(f'{ref}: different object IDs')
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--remotes', nargs=2, default=['origin', 'ngit'])
    parser.add_argument('--ref', action='append', default=[],
                        help='additional full branch/tag ref; main is always checked')
    parser.add_argument('--all', action='store_true', help='audit all advertised branches/tags')
    parser.add_argument('--local', action='store_true', help='also require local refs to match')
    args = parser.parse_args()
    try:
        configured = set(git('remote').splitlines())
        if any(remote not in configured for remote in args.remotes):
            raise ValueError('Both arguments must name configured remotes')
        refs = {'refs/heads/main', *args.ref}
        for ref in refs:
            if not re.match(r'^refs/(heads|tags)/', ref):
                raise ValueError('Use full refs/heads/... or refs/tags/... names')
            git('check-ref-format', ref)
        left, right = [parse_refs(git('ls-remote', remote)) for remote in args.remotes]
        if args.all:
            refs.update(left)
            refs.update(right)
        # Compare both annotated tag objects and their peeled target commits.
        refs.update(ref + '^{}' for ref in list(refs)
                    if ref + '^{}' in left or ref + '^{}' in right)
        failures = compare(left, right, refs)
        if args.local:
            local = parse_refs(git('show-ref', '--dereference'))
            failures += ['local: ' + error for error in compare(left, local, refs)]
        if failures:
            print('\n'.join(failures), file=sys.stderr)
            return 1
        print(f'PASS: {len(refs)} refs match on both mirrors'
              + (' and locally' if args.local else '')
              + '; PR metadata not checked.')
        return 0
    except (ValueError, subprocess.TimeoutExpired, OSError):
        print('FAIL: unable to validate refs; check arguments and mirror access privately.',
              file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
