#!/usr/bin/env python3
"""Debug: what does auth.setup / auth.login actually return?"""
import http.client, json, os, signal, socket, subprocess, sys, tempfile, time
from pathlib import Path

SUT = Path(os.environ.get("ARCHY_SUT", "~/worktrees/archy-media-deadline")).expanduser()
BIN = SUT / "core/target/release/archipelago"
tmp = Path(tempfile.mkdtemp(prefix="archy-authdbg-"))
with socket.socket() as s:
    s.bind(("127.0.0.1", 0)); port = s.getsockname()[1]

env = dict(os.environ)
env.update(ARCHIPELAGO_DATA_DIR=str(tmp), ARCHIPELAGO_BIND=f"127.0.0.1:{port}",
           ARCHIPELAGO_DEV_MODE="1")
env["PATH"] = f"{Path.home()/'.cargo'/'bin'}:{env.get('PATH','')}"
log = tmp/"node.log"
proc = subprocess.Popen([str(BIN)], stdout=log.open("w"), stderr=subprocess.STDOUT,
                        env=env, cwd=str(tmp), start_new_session=True)
try:
    for _ in range(240):
        try:
            socket.create_connection(("127.0.0.1", port), timeout=1).close(); break
        except OSError: time.sleep(0.25)
    else:
        print("never listened"); print(log.read_text()[-2000:]); sys.exit(1)

    def call(method, params=None):
        body = json.dumps({"jsonrpc":"2.0","id":1,"method":method,"params":params or {}})
        c = http.client.HTTPConnection("127.0.0.1", port, timeout=15)
        c.request("POST","/rpc/v1",body=body,headers={"Content-Type":"application/json"})
        r = c.getresponse(); raw = r.read()
        cookies = [v for (n,v) in r.getheaders() if n.lower()=="set-cookie"]
        c.close()
        print(f"\n== {method} -> HTTP {r.status}")
        print("   set-cookie:", cookies if cookies else "NONE")
        try:
            print("   body:", json.dumps(json.loads(raw))[:600])
        except Exception:
            print("   raw:", raw[:300])
        return cookies

    call("health")
    call("monitoring.media")
    call("auth.isSetup")
    call("auth.setup", {"password": "readback-fixture-pw"})
    call("auth.login", {"password": "readback-fixture-pw"})
    call("auth.isSetup")
finally:
    if proc.poll() is None:
        proc.send_signal(signal.SIGTERM)
        try: proc.wait(timeout=10)
        except subprocess.TimeoutExpired: proc.kill()
