#!/usr/bin/env python3
"""Validate build-source apps against an OTA/ISO runtime payload before shipping."""
import sys
from pathlib import Path
import yaml


def check(root: Path) -> int:
    root = root.resolve()
    manifests = sorted((root / 'apps').glob('*/manifest.y*ml'))
    if not manifests:
        raise ValueError(f'No app manifests in {root / "apps"}')
    count = 0
    for manifest in manifests:
        app = yaml.safe_load(manifest.read_text())['app']
        build = app.get('container', {}).get('build')
        if not build:
            continue
        context = Path(build['context'])
        if context.is_absolute():
            context = root / context.relative_to('/opt/archipelago')
        else:
            context = manifest.parent / context
        context = context.resolve()
        if not context.is_relative_to(root) or not context.is_dir():
            raise ValueError(f'{app["id"]}: missing or out-of-payload build context: {context}')
        dockerfile = (context / build.get('dockerfile', 'Dockerfile')).resolve()
        if not dockerfile.is_relative_to(context) or not dockerfile.is_file():
            raise ValueError(f'{app["id"]}: missing or out-of-context Dockerfile: {dockerfile}')
        if 'git.tx1138.com/' in dockerfile.read_text():
            raise ValueError(f'{app["id"]}: Dockerfile references retired registry git.tx1138.com')
        count += 1
    return count


if __name__ == '__main__':
    try:
        count = check(Path(sys.argv[1] if len(sys.argv) > 1 else '.'))
    except (ValueError, KeyError, OSError, yaml.YAMLError) as error:
        sys.exit(f'Invalid app build payload: {error}')
    print(f'Validated {count} app build contexts and Dockerfiles.')
