import hashlib
import io
import platform
import tarfile
import urllib.request
from pathlib import Path

arch = {'x86_64': 'amd64', 'aarch64': 'arm64'}[platform.machine()]
pins = {
    'frp': ('0.71.0', {'amd64': '84f27e39f11169f7adcef8e8b70c9329de17747b1f14dad9fb95eef5682ea716', 'arm64': 'f33c293c275d8fc68c654b6fba8f10b2551d6463d09a9fc9cffb7227eae82266'}),
    'caddy': ('2.11.7', {'amd64': '727b91701a392de6ebc5027509f548bf39979e5216340d0faed8fa5e69c84f8b', 'arm64': 'd8fc6d179a5d283028a472a5618564f6ad8a86fed513e64f032b3b0b7cc45e42'}),
}
for name, (version, digests) in pins.items():
    repo = 'fatedier/frp' if name == 'frp' else 'caddyserver/caddy'
    url = f'https://github.com/{repo}/releases/download/v{version}/{name}_{version}_linux_{arch}.tar.gz'
    with urllib.request.urlopen(url, timeout=120) as response:
        data = response.read(64 * 1024 * 1024 + 1)
    if hashlib.sha256(data).hexdigest() != digests[arch]:
        raise ValueError(f'{name} archive checksum mismatch')
    binary = 'frpc' if name == 'frp' else 'caddy'
    member = f'frp_{version}_linux_{arch}/frpc' if name == 'frp' else 'caddy'
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        info = archive.getmember(member)
        if not info.isfile() or info.size > 128 * 1024 * 1024:
            raise ValueError('Invalid binary archive member')
        output = Path('/usr/local/bin') / binary
        output.write_bytes(archive.extractfile(info).read())
        output.chmod(0o755)
