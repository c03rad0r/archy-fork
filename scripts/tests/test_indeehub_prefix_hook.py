"""Offline behavior checks for the native post-install proxy prefix hook."""
import pathlib
import subprocess
import tempfile
import unittest
import yaml

ROOT = pathlib.Path(__file__).resolve().parents[2]


def prefix_hook():
    hooks = yaml.safe_load((ROOT / "apps/indeedhub/manifest.yml").read_text())["app"]["hooks"]["post_install"]
    matches = [index for index, hook in enumerate(hooks) if "X-Forwarded-Prefix" in " ".join(hook.get("exec", []))]
    assert len(matches) == 1
    index = matches[0]
    assert hooks[index + 1] == {"exec": ["nginx", "-s", "reload"]}
    return hooks[index]["exec"]


class IndeeHubPrefixHook(unittest.TestCase):
    def test_composes_prefix_before_reload_and_is_idempotent(self):
        original = "server {\n    proxy_set_header Host $http_host;\n    proxy_set_header X-Forwarded-Prefix /api;\n}\n"
        expected = original.replace("X-Forwarded-Prefix /api;", "X-Forwarded-Prefix $http_x_forwarded_prefix/api;")
        with tempfile.TemporaryDirectory() as directory:
            config = pathlib.Path(directory) / "default.conf"
            config.write_text(original)
            args = prefix_hook()
            self.assertEqual(args[-1], "/etc/nginx/conf.d/default.conf")
            for _ in range(2):
                subprocess.run([*args[:-1], str(config)], check=True)
                self.assertEqual(config.read_text(), expected)

    def test_existing_composed_prefix_is_preserved(self):
        original = "proxy_set_header X-Forwarded-Prefix $http_x_forwarded_prefix/api;\n"
        with tempfile.TemporaryDirectory() as directory:
            config = pathlib.Path(directory) / "default.conf"
            config.write_text(original)
            subprocess.run([*prefix_hook()[:-1], str(config)], check=True)
            self.assertEqual(config.read_text(), original)


if __name__ == "__main__":
    unittest.main()
