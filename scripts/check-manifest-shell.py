#!/usr/bin/env python3
"""Syntax-check the shell embedded in apps/*/manifest.yml.

A manifest can carry a whole startup script in `container.custom_args` /
`entrypoint`. Nothing validated it, so a broken one shipped through the
signed catalog and only failed on the node — as a container that exits
instantly and an app that vanishes from the UI.

Two checks, both learned from v1.7.124 (bitcoin-knots / bitcoin-core):

1. `sh -n` the snippet. The break was `sh: Syntax error: "fi" unexpected`,
   which no YAML parse and no Rust test could have caught.

2. Reject `#` inside the snippet. These are YAML **folded** scalars (`>-`),
   where `#` is NOT a comment — it is literal text that reaches the shell,
   and because folding joins lines with spaces it comments out the rest of
   the folded line. That is exactly how an `if ... then` was swallowed while
   its more-indented body survived, leaving an orphan `fi`. Put explanations
   above the `- >-` line, where YAML really does treat them as comments.
"""

from __future__ import annotations

import glob
import os
import subprocess
import sys
import tempfile

import yaml

# Long enough to be a script rather than a flag.
MIN_SCRIPT_LEN = 60


def snippets(path: str):
    with open(path, encoding="utf-8") as fh:
        data = yaml.safe_load(fh)
    container = ((data or {}).get("app") or {}).get("container") or {}
    for key in ("custom_args", "entrypoint"):
        value = container.get(key)
        if not isinstance(value, list):
            continue
        for i, part in enumerate(value):
            if isinstance(part, str) and len(part) >= MIN_SCRIPT_LEN:
                yield f"{key}[{i}]", part


def main() -> int:
    failures = []
    checked = 0
    for path in sorted(glob.glob("apps/*/manifest.yml")):
        app = os.path.basename(os.path.dirname(path))
        try:
            found = list(snippets(path))
        except Exception as exc:  # noqa: BLE001 — report, don't crash the gate
            failures.append(f"{app}: manifest does not parse: {exc}")
            continue
        for where, script in found:
            checked += 1
            if "#" in script:
                failures.append(
                    f"{app} {where}: contains '#'. In a folded YAML scalar that is not a "
                    f"comment — it reaches the shell and comments out the rest of the "
                    f"folded line. Move the explanation above the '- >-' line."
                )
            with tempfile.NamedTemporaryFile("w", suffix=".sh", delete=False) as tmp:
                tmp.write(script)
                tmp_path = tmp.name
            try:
                proc = subprocess.run(
                    ["sh", "-n", tmp_path], capture_output=True, text=True, check=False
                )
            finally:
                os.unlink(tmp_path)
            if proc.returncode != 0:
                failures.append(f"{app} {where}: {proc.stderr.strip()}")

    for f in failures:
        print(f"MANIFEST-SHELL {f}", file=sys.stderr)
    print(f'{{"snippets_checked": {checked}, "failures": {len(failures)}}}')
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
