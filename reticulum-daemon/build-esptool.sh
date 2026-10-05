#!/usr/bin/env bash
# Self-contained flasher shipped identically in OTA and ISO payloads.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
[ -d .venv ] || python3 -m venv .venv
.venv/bin/python -m pip install -q 'esptool==4.8.1' -r requirements-build.txt
.venv/bin/python archy_esptool.py --archy-self-test
.venv/bin/pyinstaller --onefile --name archy-esptool --clean --noconfirm \
    --collect-all esptool --copy-metadata esptool archy_esptool.py
env -i PATH=/usr/bin:/bin dist/archy-esptool --archy-self-test
env -i PATH=/usr/bin:/bin dist/archy-esptool version
