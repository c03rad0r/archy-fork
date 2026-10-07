#!/usr/bin/env python3
"""Run real provider hooks only against disposable nginx/index fixtures."""
from pathlib import Path
import re
import subprocess
import tempfile
import yaml

root = Path(__file__).resolve().parents[2]
hooks = yaml.safe_load((root / 'apps/indeedhub/manifest.yml').read_text())['app']['hooks']['post_install']
for mode in ('fresh', 'literal', 'legacy-filter'):
    with tempfile.TemporaryDirectory(prefix='indeehub-provider-hook-') as directory:
        fixture = Path(directory)
        nginx, index = fixture / 'default.conf', fixture / 'index.html'
        old_script = '<script src="/nostr-provider.js?v=tab-signer-v2"></script>'
        index.write_text('<html><head>' + (old_script if mode == 'literal' else '') + '</head><body></body></html>')
        nginx.write_text('server {\n    location = /sw.js {\n    }\n' + (
            "    sub_filter '</head>' '" + old_script + "</head>';\n" if mode == 'legacy-filter' else '') + '}\n')
        first = None
        for repeat in range(2):
            for hook in hooks:
                command = hook.get('exec')
                if not command or command[0] == 'nginx':
                    continue
                command = [argument.replace('/etc/nginx/conf.d/default.conf', str(nginx)).replace('/usr/share/nginx/html/index.html', str(index)) for argument in command]
                subprocess.run(command, check=True, capture_output=True, text=True)
            rendered = index.read_text() + nginx.read_text()
            assert len(re.findall(r'<script[^>]*nostr-provider', rendered)) == 1, mode
            assert rendered.count('location = /nostr-provider.js {') == 1, mode
            assert 'tab-signer-v2' not in rendered and 'tab-signer-v4' in rendered, mode
            assert 'no-cache, no-store, must-revalidate' in rendered, mode
            if first is not None:
                assert rendered == first, mode
            first = rendered
print('PASS: fresh, literal and legacy sub_filter provider hooks are idempotent; fixtures only')
