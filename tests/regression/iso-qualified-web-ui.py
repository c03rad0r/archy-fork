#!/usr/bin/env python3
"""Exercise the ISO builder's actual payload-selection branches, without a build."""
import os
import pathlib
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
source = (ROOT / 'image-recipe/_archived/build-auto-installer-iso.sh').read_text()
capture = source.split('WEBUI_CAPTURED=0', 1)[1].split('# Include AIUI web app', 1)[0]
select = source.split('AIUI_SRC=""', 1)[1].split('if [ -n "$AIUI_SRC" ]; then', 1)[0]
program = 'set -eu\nWEBUI_CAPTURED=0\n' + capture + '\nAIUI_SRC=""\n' + select + '\nprintf "%s\\n" "$AIUI_SRC"\n'
with tempfile.TemporaryDirectory(prefix='archy-iso-qualified-ui-') as tmp:
    base = pathlib.Path(tmp)
    qualified = base / 'qualified'
    (qualified / 'aiui').mkdir(parents=True)
    (qualified / 'index.html').write_text('qualified-dashboard')
    (qualified / 'aiui/index.html').write_text('qualified-aiui')
    (qualified / 'aiui/BUILD-INFO').write_text('commit=fixture')
    for label, path, success in [('qualified', qualified, True), ('missing', base / 'absent', False), ('partial', base / 'partial', False)]:
        out = base / 'outputs' / label / 'installer'
        (out / 'web-ui').mkdir(parents=True)
        if label == 'partial':
            path.mkdir(exist_ok=True)
            (path / 'index.html').write_text('incomplete')
        result = subprocess.run(['bash', '-c', program], env=os.environ | {
            'ARCHIPELAGO_WEB_UI': str(path), 'ARCH_DIR': str(out),
            'BUILD_FROM_SOURCE': '0', 'DEV_SERVER': 'localhost', 'SCRIPT_DIR': str(base),
        }, capture_output=True, text=True)
        assert (result.returncode == 0) == success, (label, result.stdout, result.stderr)
        if success:
            assert (out / 'web-ui/index.html').read_text() == 'qualified-dashboard'
            assert (out / 'web-ui/aiui/index.html').read_text() == 'qualified-aiui'
            assert result.stdout.strip().splitlines()[-1] == str(qualified / 'aiui')
            assert not (out / 'web-ui/archipelago-runtime').exists()
        else:
            assert not list((out / 'web-ui').iterdir()), 'invalid payload silently fell back to a live node'
        print('PASS ISO payload selection:', label)
