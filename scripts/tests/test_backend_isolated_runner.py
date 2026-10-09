#!/usr/bin/env python3
import os
import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
RUNNER = ROOT / "scripts" / "test-backend-isolated.sh"


class BackendIsolationRunnerTests(unittest.TestCase):
    def test_podman_mode_separates_build_and_networkless_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            temp = pathlib.Path(temporary)
            fake_bin = temp / "bin"
            fake_bin.mkdir()
            log = temp / "podman.log"
            podman = fake_bin / "podman"
            podman.write_text(
                "#!/usr/bin/env bash\n"
                "printf '%s\\0' \"$@\" >> \"$PODMAN_TEST_LOG\"\n"
                "printf '\\n' >> \"$PODMAN_TEST_LOG\"\n"
                "if [[ \" $* \" == *' cargo test '* ]]; then\n"
                "  printf '%s\\n' '{\"reason\":\"compiler-artifact\",\"profile\":{\"test\":true},\"executable\":\"/workspace/core/target/debug/archy-test\"}'\n"
                "fi\n"
            )
            podman.chmod(0o700)
            cargo_home = temp / "cargo"
            env = os.environ.copy()
            env.update(
                {
                    "PATH": f"{fake_bin}:{env['PATH']}",
                    "PODMAN_TEST_LOG": str(log),
                    "ARCHY_TEST_ISOLATOR": "podman",
                    "ARCHY_TEST_REPO": str(ROOT),
                    "ARCHY_TEST_IMAGE": "example.invalid/ci@sha256:test",
                    "ARCHY_TEST_CARGO_HOME": str(cargo_home),
                }
            )

            subprocess.run([str(RUNNER)], env=env, check=True)
            invocations = [
                line.replace("\0", " ") for line in log.read_text().splitlines()
            ]
            self.assertEqual(len(invocations), 2)
            self.assertIn("--network=pasta", invocations[0])
            self.assertIn(f"{ROOT}:/workspace:rw,Z", invocations[0])
            self.assertIn("--network=none", invocations[1])
            self.assertIn(f"{ROOT}:/workspace:ro,Z", invocations[1])
            self.assertIn("ARCHY_TEST_ISOLATED=1", invocations[1])

    def test_unknown_isolator_fails_closed(self):
        result = subprocess.run(
            [str(RUNNER)],
            env={**os.environ, "ARCHY_TEST_ISOLATOR": "unknown"},
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("must be systemd or podman", result.stderr)


if __name__ == "__main__":
    unittest.main()
