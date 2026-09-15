#!/usr/bin/env python3
"""Exercise the built frontend against a backend that disappears and changes IP.

Uses an isolated Podman network and disposable containers, never the node stack.
Usage: python3 scripts/test-mempool-dns-recovery.py [frontend-image]
"""
import ipaddress
import json
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid

IMAGE = sys.argv[1] if len(sys.argv) > 1 else (
    "source.archipelago-foundation.org/lfg2025/mempool-frontend:v3.3.1-archy1"
)
BACKEND = "source.archipelago-foundation.org/lfg2025/mempool-backend:v3.3.1"
prefix = "mempool-dns-test-" + uuid.uuid4().hex[:8]
network, frontend, backend = prefix, prefix + "-web", prefix + "-api"


def podman(*args, check=True):
    return subprocess.run(["podman", *args], capture_output=True, text=True,
                          check=check, timeout=60).stdout.strip()


def eventually(check, timeout=25):
    deadline = time.monotonic() + timeout
    while True:
        try:
            return check()
        except (AssertionError, OSError, urllib.error.URLError):
            if time.monotonic() >= deadline:
                raise
            time.sleep(1)


server = r"""
const http = require('http'), crypto = require('crypto');
const server = http.createServer((req, res) => {
  res.setHeader('Content-Type', 'application/json');
  res.end(JSON.stringify({url: req.url, instance: process.env.INSTANCE}));
});
server.on('upgrade', (req, socket) => {
  const key = crypto.createHash('sha1')
    .update(req.headers['sec-websocket-key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11')
    .digest('base64');
  socket.end('HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n' +
    'Connection: Upgrade\r\nSec-WebSocket-Accept: ' + key + '\r\n' +
    'X-Upstream-Url: ' + req.url + '\r\nX-Instance: ' + process.env.INSTANCE + '\r\n\r\n');
});
server.listen(8999, '0.0.0.0');
"""

try:
    podman("network", "create", network)
    subnet = ipaddress.ip_network(json.loads(podman("network", "inspect", network))[0]["subnets"][0]["subnet"])
    podman("run", "-d", "--name", frontend, "--network", network,
           "-p", "127.0.0.1::8080", "-e", "BACKEND_MAINNET_HTTP_HOST=mempool-api",
           "-e", "FRONTEND_HTTP_PORT=8080", IMAGE)
    port = int(podman("port", frontend, "8080/tcp").rsplit(":", 1)[1])
    url = f"http://127.0.0.1:{port}"

    def static_ready():
        assert urllib.request.urlopen(url, timeout=4).status == 200

    eventually(static_ready)
    started = podman("inspect", frontend, "--format", "{{.State.StartedAt}}")
    try:
        urllib.request.urlopen(url + "/api/v1/backend-info", timeout=6)
        raise AssertionError("An absent backend must not appear healthy")
    except urllib.error.HTTPError as error:
        assert error.code == 502
    print("PASS: frontend starts while backend DNS is absent", flush=True)

    for instance, offset in [("first", 10), ("replacement", 11)]:
        if instance == "replacement":
            podman("rm", "-f", backend)
            # Ensure the cached address has expired while the backend is absent.
            time.sleep(6)
        podman("run", "-d", "--name", backend, "--network", network,
               "--network-alias", "mempool-api", "--ip", str(subnet[offset]),
               "-e", "INSTANCE=" + instance, "--entrypoint", "node", BACKEND,
               "-e", server)

        for path, expected in [
            ("/api/blocks/tip/height?probe=one", "/api/v1/blocks/tip/height?probe=one"),
            ("/api/v1/fees/recommended?probe=two", "/api/v1/fees/recommended?probe=two"),
        ]:
            def check_http():
                with urllib.request.urlopen(url + path, timeout=4) as response:
                    result = json.load(response)
                assert result == {"url": expected, "instance": instance}, result
            eventually(check_http)

        for path in ["/api/v1/ws?probe=ws", "/ws?probe=ws"]:
            def check_ws():
                with socket.create_connection(("127.0.0.1", port), timeout=4) as sock:
                    sock.sendall((f"GET {path} HTTP/1.1\r\nHost: localhost\r\n"
                                  "Upgrade: websocket\r\nConnection: Upgrade\r\n"
                                  "Sec-WebSocket-Version: 13\r\n"
                                  "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n").encode())
                    response = b""
                    while b"\r\n\r\n" not in response:
                        part = sock.recv(4096)
                        assert part, response
                        response += part
                assert b"101 Switching Protocols" in response, response
                assert b"X-Upstream-Url: /?probe=ws" in response, response
                assert ("X-Instance: " + instance).encode() in response, response
            eventually(check_ws)
        assert podman("inspect", frontend, "--format", "{{.State.StartedAt}}") == started
        print(f"PASS: {instance} backend at {subnet[offset]}: HTTP paths, query strings, both WebSocket routes; frontend never restarted", flush=True)

    before = podman("exec", frontend, "cat", "/etc/nginx/conf.d/nginx-mempool.conf")
    podman("exec", frontend, "/patch/repair-nginx.sh")
    assert podman("exec", frontend, "cat", "/etc/nginx/conf.d/nginx-mempool.conf") == before
    podman("exec", frontend, "nginx", "-t")
    print("PASS: repeated repair is idempotent and nginx configuration is valid", flush=True)
    podman("restart", frontend)
    eventually(static_ready)

    def after_restart():
        with urllib.request.urlopen(url + "/api/blocks/tip/height?restart=1", timeout=4) as response:
            assert json.load(response) == {
                "url": "/api/v1/blocks/tip/height?restart=1", "instance": "replacement"
            }
    eventually(after_restart)
    eventually(check_ws)
    print("PASS: frontend restart preserves DNS recovery and HTTP/WebSocket routing", flush=True)
finally:
    podman("rm", "-f", frontend, backend, check=False)
    podman("network", "rm", network, check=False)
