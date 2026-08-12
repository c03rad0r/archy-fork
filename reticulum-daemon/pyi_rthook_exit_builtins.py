# PyInstaller runtime hook — see build.sh.
#
# `exit()`/`quit()` aren't part of the language; they're `site.Quitter`
# instances the interactive interpreter injects into builtins at startup
# (site.py). A frozen PyInstaller app never runs that interactive-mode
# init, so any bundled script that calls bare `exit()` (RNS's own
# rnodeconf.py does, in its graceful_exit() cleanup path) hits
# `NameError: name 'exit' is not defined` right as it tries to quit
# cleanly — the real work above it already completed, but the process
# still exits 1, which is a foot-gun for anything scripting off the exit
# code. Pre-define both as sys.exit so that path is a no-op crash-wise.
import builtins
import sys

if not hasattr(builtins, "exit"):
    builtins.exit = sys.exit
if not hasattr(builtins, "quit"):
    builtins.quit = sys.exit
