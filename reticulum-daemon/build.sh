#!/usr/bin/env bash
# Build the PyInstaller single-binaries for the OTA (plan Phase 1 packaging).
# Outputs: dist/archy-reticulum-daemon, dist/archy-rnodeconf — drop both next
# to /usr/local/bin/archipelago.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

if [ ! -d .venv ]; then
    python3 -m venv .venv
fi
.venv/bin/pip install -q -r requirements.txt -r requirements-build.txt

rm -rf build dist archy-reticulum-daemon.spec archy-rnodeconf.spec

# --collect-submodules: RNS/LXMF load most of their own internals dynamically
# (interface drivers, transport backends), which PyInstaller's static import
# analysis can't see from a plain `import RNS`.
#
# -d noarchive is NOT optional: RNS.Interfaces/__init__.py builds its
# `__all__` by glob-ing *.py/*.pyc next to its own `__file__` at import time
# (`from RNS.Interfaces import *` in Reticulum.py relies on that). PyInstaller
# normally zips pure-Python modules into an in-binary PYZ archive, so
# `__file__` doesn't point at a real directory and the glob comes back empty
# -> `NameError: name 'Interface' is not defined` at RNS.Reticulum() bring-up.
# noarchive keeps modules as loose .pyc files on disk so the glob still works.
.venv/bin/pyinstaller --onefile --name archy-reticulum-daemon --clean --noconfirm \
    --collect-submodules RNS \
    --collect-submodules LXMF \
    --collect-data RNS \
    -d noarchive \
    reticulum_daemon.py

echo "Built dist/archy-reticulum-daemon ($(du -h dist/archy-reticulum-daemon | cut -f1))"

# archy-rnodeconf: RNS's own official RNode config/diagnostic tool
# (RNS.Utilities.rnodeconf — reads/sets frequency, bandwidth, spreading
# factor, coding rate, TX power on an attached RNode; also verifies firmware
# signatures and can flash/bootstrap a board). Shipped alongside the daemon
# so every node can inspect and reconfigure its own radio without needing a
# full RNS/Python dev environment set up by hand — see the checkpoint in
# docs/RETICULUM-TRANSPORT-PROGRESS.md for the incident (two nodes silently
# running at different spreading factors, invisible without this tool) that
# motivated shipping it as a first-class OS tool rather than an ad hoc script.
RNODECONF_SRC="$(find .venv/lib -maxdepth 5 -path '*/site-packages/RNS/Utilities/rnodeconf.py' -print -quit)"
if [ -n "$RNODECONF_SRC" ] && [ -f "$RNODECONF_SRC" ]; then
    # --runtime-hook: rnodeconf's own graceful_exit() calls the bare
    # exit()/quit() builtins, which only exist in interactive Python (site.py
    # injects them) — a frozen app hits NameError right as it tries to quit
    # cleanly, after all the real work already succeeded. See
    # pyi_rthook_exit_builtins.py. A second hook fixes rnodeconf's board-flash
    # step, which shells out to a bundled esptool.py via `sys.executable` —
    # under a frozen binary that's the binary itself, not a real interpreter,
    # so the flash subprocess call breaks. See
    # pyi_rthook_fix_flasher_executable.py.
    .venv/bin/pyinstaller --onefile --name archy-rnodeconf --clean --noconfirm \
        --collect-submodules RNS \
        --collect-data RNS \
        --runtime-hook pyi_rthook_exit_builtins.py \
        --runtime-hook pyi_rthook_fix_flasher_executable.py \
        -d noarchive \
        "$RNODECONF_SRC"
    echo "Built dist/archy-rnodeconf ($(du -h dist/archy-rnodeconf | cut -f1))"
else
    echo "WARNING: rnodeconf.py not found at $RNODECONF_SRC (RNS version mismatch?) — skipping archy-rnodeconf build" >&2
fi
