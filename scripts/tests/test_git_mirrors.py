"""Exercise mirror gate against disposable local Git remotes."""
import pathlib
import subprocess
import sys
import tempfile
import unittest

CHECK = pathlib.Path(__file__).resolve().parents[1] / 'check-git-mirrors.py'


class MirrorTests(unittest.TestCase):
    def test_publication_drift_and_annotated_tags(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            work = root / 'work'
            work.mkdir()

            def git(*args):
                return subprocess.run(['git', *args], cwd=work, check=True,
                                      capture_output=True, text=True).stdout

            def check(*args, passes=True):
                result = subprocess.run([sys.executable, str(CHECK), *args],
                                        cwd=work, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0 if passes else 1,
                                 result.stdout + result.stderr)

            git('init', '-b', 'main')
            git('config', 'user.name', 'Mirror test')
            git('config', 'user.email', 'fixture@example.invalid')
            git('config', 'commit.gpgsign', 'false')
            git('config', 'tag.gpgsign', 'false')
            for remote in ('origin', 'ngit'):
                git('init', '--bare', str(root / remote))
                git('remote', 'add', remote, str(root / remote))
            git('commit', '--allow-empty', '-m', 'first')
            check(passes=False)  # Empty mirrors cannot pass.
            git('push', 'origin', 'main')
            check(passes=False)  # Partial publication.
            git('push', 'ngit', 'main')
            check('--local')
            git('commit', '--allow-empty', '-m', 'second')
            check()  # Remote equality alone does not imply local publication.
            check('--local', passes=False)
            git('push', 'origin', 'main')
            check(passes=False)
            git('push', 'ngit', 'main')
            git('tag', '-a', 'v-test', '-m', 'original')
            git('push', 'origin', 'refs/tags/v-test')
            check('--ref', 'refs/tags/v-test', passes=False)
            git('push', 'ngit', 'refs/tags/v-test')
            check('--local', '--ref', 'refs/tags/v-test')
            # Same commit, different annotation must fail too.
            git('tag', '-f', '-a', 'v-test', '-m', 'different annotation')
            check('--local', '--ref', 'refs/tags/v-test', passes=False)
            git('push', '--force', 'ngit', 'refs/tags/v-test')
            check('--all', passes=False)
            git('tag', '-d', 'v-test')
            git('fetch', 'origin', 'refs/tags/v-test:refs/tags/v-test')
            git('push', '--force', 'ngit', 'refs/tags/v-test')
            git('push', 'origin', 'HEAD:refs/heads/extra')
            check('--local')
            check('--all', passes=False)
            git('push', 'ngit', 'HEAD:refs/heads/extra')
            check('--all')
            check('--ref', 'main', passes=False)
            check('--remotes', 'origin', 'missing', passes=False)
            git('remote', 'set-url', 'ngit', str(root / 'does-not-exist'))
            check(passes=False)


if __name__ == '__main__':
    unittest.main()
