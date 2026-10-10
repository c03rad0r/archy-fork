#!/usr/bin/env python3
"""Keep the browser terminal's runtime and exact-authority proxy in releases."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
builder = (ROOT / "image-recipe/_archived/build-auto-installer-iso.sh").read_text()
installer = (ROOT / "image-recipe/archipelago-scripts/install-to-disk.sh").read_text()
nginx = (ROOT / "image-recipe/configs/nginx-archipelago.conf").read_text()

rootfs_packages = builder.split("cat >> \"$WORK_DIR/Dockerfile.rootfs\"", 1)[1].split(
    "# Create archipelago systemd service", 1
)[0]
assert "    tmux \\\n" in rootfs_packages, "fresh ISO rootfs omits tmux"
assert "    tmux \\\n" in installer, "install-to-disk path omits tmux"

assert 'map "$request_method|$http_sec_fetch_site|$http_origin" $archy_terminal_origin' in nginx
terminal_locations = nginx.split("location ^~ /api/terminal/ {")[1:]
assert len(terminal_locations) == 2, "expected HTTP and HTTPS terminal proxies"
for location in terminal_locations:
    block = location.split("}", 1)[0]
    assert "proxy_set_header Host $http_host;" in block, "terminal proxy drops authority port"
    assert "proxy_set_header Origin $archy_terminal_origin;" in block

assert nginx.count(
    "location ~ ^/api/(peer|public)-indeehub-(stream|media|key)/ {"
) == 2, "HTTP and HTTPS must proxy IndeeHub playback before the SPA fallback"

print("PASS terminal runtime packaging and proxy authority")
