#!/usr/bin/env python3
"""
Production app catalog image smoke test.

Parses local app manifests, then probes images on a target production node via
SSH. This catches catalog/image mismatches before a user clicks Install.

Checks:
  - manifest YAML loads and required app/container fields exist
  - production node health endpoint responds
  - each non-local image can be pulled on the node
  - shell-entrypoint apps reference commands that exist inside the image

Usage:
  scripts/app-catalog-image-smoke-test.py \
    --target archipelago@192.0.2.11 \
    --ssh-key /home/archipelago/.ssh/id_ed25519
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shlex
import subprocess
import sys
from pathlib import Path

import yaml


INSECURE_REGISTRIES = ("source.archipelago-foundation.org", "23.182.128.160:3000")


def run(cmd: list[str], timeout: int = 120) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
    )


class Remote:
    def __init__(self, target: str, ssh_key: str | None, extra: list[str]) -> None:
        self.base = [
            "ssh",
            "-F",
            "/dev/null",
            "-o",
            "ConnectTimeout=8",
            "-o",
            "BatchMode=yes",
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "PasswordAuthentication=no",
            "-o",
            "StrictHostKeyChecking=no",
        ]
        if ssh_key:
            self.base.extend(["-i", ssh_key])
        self.base.extend(extra)
        self.target = target

    def sh(self, script: str, timeout: int = 120) -> subprocess.CompletedProcess[str]:
        return run(self.base + [self.target, script], timeout=timeout)


def load_manifests(apps_dir: Path) -> list[dict]:
    manifests = []
    for path in sorted(apps_dir.glob("*/manifest.yml")):
        with path.open("r", encoding="utf-8") as fh:
            data = yaml.safe_load(fh)
        if not isinstance(data, dict):
            app = None
            container = None
        elif isinstance(data.get("app"), dict):
            app = data["app"]
            container = app.get("container")
        else:
            app = data
            container = data.get("container") if isinstance(data.get("container"), dict) else data
        manifests.append({"path": path, "app": app, "container": container})
    return manifests


def insecure(image: str) -> bool:
    return image.startswith(INSECURE_REGISTRIES)


def shell_probe_for(app_id: str, command: str) -> str | None:
    if app_id in {"bitcoin-core", "bitcoin-knots"}:
        return "command -v bitcoind || find /opt -path '*/bin/bitcoind' -type f 2>/dev/null | sort | tail -n 1"

    match = re.search(r"\bexec\s+([\"']?)([A-Za-z0-9_./-]+)\1", command)
    if not match:
        return None

    binary = match.group(2)
    if binary.startswith("$"):
        return None
    if "/" in binary:
        return f"test -x {shlex.quote(binary)} && echo {shlex.quote(binary)}"
    return f"command -v {shlex.quote(binary)}"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True)
    parser.add_argument("--ssh-key", default=os.environ.get("ARCHIPELAGO_SSH_KEY"))
    parser.add_argument("--apps-dir", default="apps")
    parser.add_argument("--pull", action="store_true", help="pull missing images before probing")
    parser.add_argument("--ssh-option", action="append", default=[])
    args = parser.parse_args()

    apps_dir = Path(args.apps_dir)
    remote = Remote(args.target, args.ssh_key, sum((["-o", x] for x in args.ssh_option), []))

    failures: list[str] = []
    warnings: list[str] = []
    passes = 0

    health = remote.sh("curl -fsS --max-time 5 http://127.0.0.1:5678/health", timeout=15)
    if health.returncode != 0:
        failures.append(f"target health failed: {health.stderr.strip() or health.stdout.strip()}")
        print(json.dumps({"passes": passes, "warnings": 0, "failures": len(failures)}, sort_keys=True))
        for failure in failures:
            print(f"FAIL {failure}")
        return 1
    else:
        passes += 1
        print(f"PASS target health {health.stdout.strip()}")

    manifests = load_manifests(apps_dir)
    print(f"INFO loaded {len(manifests)} manifests from {apps_dir}")

    for item in manifests:
        path = item["path"]
        app = item["app"]
        container = item["container"]
        if not isinstance(app, dict) or not isinstance(container, dict):
            failures.append(f"{path}: missing app.container")
            continue

        app_id = str(app.get("id") or "")
        image = str(container.get("image") or app.get("image") or "")
        if not app_id:
            failures.append(f"{path}: missing app id")
            continue
        if not image and container.get("build"):
            warnings.append(f"{app_id}: skipped locally built image")
            continue
        if not image:
            failures.append(f"{path}: missing container image")
            continue
        passes += 1

        if image.startswith("localhost/") or image.startswith("archipelago/"):
            warnings.append(f"{app_id}: skipped local/unpublished image {image}")
            continue

        pull_args = ["pull"]
        if insecure(image):
            pull_args.append("--tls-verify=false")
        pull_args.append(image)

        if args.pull:
            pull_cmd = "timeout 300s podman " + " ".join(shlex.quote(x) for x in pull_args)
            pulled = remote.sh(pull_cmd, timeout=330)
            if pulled.returncode != 0:
                failures.append(f"{app_id}: pull failed for {image}: {(pulled.stderr or pulled.stdout).strip()[-500:]}")
                continue
            print(f"PASS {app_id}: pulled {image}")
            passes += 1
        else:
            exists = remote.sh(f"podman image exists {shlex.quote(image)}", timeout=30)
            if exists.returncode != 0:
                warnings.append(f"{app_id}: image not present on target, rerun with --pull: {image}")
                continue

        custom_args = container.get("custom_args") or []
        entrypoint = container.get("entrypoint") or []
        if entrypoint == ["sh", "-lc"] and custom_args:
            command = str(custom_args[0])
            probe = shell_probe_for(app_id, command)
            if probe:
                remote_script = (
                    "timeout 45s podman run --rm "
                    f"--entrypoint sh {shlex.quote(image)} -c {shlex.quote(probe)}"
                )
                checked = remote.sh(remote_script, timeout=60)
                found = checked.stdout.strip().splitlines()[-1:] or [""]
                if checked.returncode == 0 and found[0]:
                    print(f"PASS {app_id}: command probe found {found[0]}")
                    passes += 1
                else:
                    failures.append(
                        f"{app_id}: command probe failed in {image}: {(checked.stderr or checked.stdout).strip()[-500:]}"
                    )

    print(json.dumps({"passes": passes, "warnings": len(warnings), "failures": len(failures)}, sort_keys=True))
    for warning in warnings:
        print(f"WARN {warning}")
    for failure in failures:
        print(f"FAIL {failure}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
