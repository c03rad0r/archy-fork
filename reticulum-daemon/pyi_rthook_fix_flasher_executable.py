# PyInstaller runtime hook — see build.sh.
#
# rnodeconf's own board-flashing code shells out to a bundled esptool.py as
# `[sys.executable, flasher_path, "--chip", ..., "write_flash", ...]` (RNS's
# rnodeconf.py, ~line 2794 as of RNS 1.3.5). That's correct for a normal
# `python rnodeconf.py` invocation, but under a frozen PyInstaller binary
# `sys.executable` is the frozen binary itself, not a real interpreter — so
# the "subprocess" just re-invokes archy-rnodeconf's OWN argparse CLI with
# esptool-shaped flags, which it doesn't recognize, and the flash step fails
# immediately with "unrecognized arguments: --chip ...". Confirmed live
# against a real Heltec V4 (2026-07-23): device selection, band selection,
# and firmware download all worked; only the final `write_flash` subprocess
# call broke this way.
#
# Fix: point sys.executable at a real Python interpreter that has rnodeconf's
# own runtime deps available (esptool.py only needs pyserial, which RNS
# already depends on) before any of rnodeconf's code runs. Prefer the build
# venv this exact binary was frozen from — see build.sh — falling back to a
# bare `python3` on PATH if that venv isn't present on this node.
import os
import sys

if getattr(sys, "frozen", False):
    _candidates = [
        os.environ.get("ARCHY_RNODECONF_PYTHON", ""),
        os.path.join(os.path.dirname(os.path.abspath(sys.argv[0])), "..", "reticulum-daemon", ".venv", "bin", "python3"),
        os.path.expanduser("~/archy/reticulum-daemon/.venv/bin/python3"),
    ]
    for _candidate in _candidates:
        if _candidate and os.path.isfile(_candidate):
            sys.executable = _candidate
            break
    else:
        sys.executable = "python3"
