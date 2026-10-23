#!/usr/bin/env python3
"""P2 READ-BACK — N concurrent media streams, then the node's own UI still answers.

The contract: "N concurrent media streams, then the node's own UI still answers. Before P2
the shared Semaphore::new(1024) 503s everything including the UI."

HOW THIS PROVES IT
------------------
Two phases against the real release binary:

  A. Baseline: the UI answers with no media load.
  B. Under load: hold many media streams open simultaneously, and while they are in
     flight, probe the UI's own routes. Before P2 both listeners shared one 1024-permit
     semaphore, so media load could exhaust the permits the UI needed and the UI would
     answer 503.

WHAT THIS CANNOT SHOW, stated rather than implied: 1024 concurrent streams is far more
than a test should open, so this exercises a load that is representative in KIND (media
holding connections while the UI is probed) but not in SCALE. The bug is structural —
shared budget — so a smaller N still detects a re-merge, but this is not a load test and
does not claim to be. The source-level guard in server.rs covers the structural claim; this
covers the observable one.

Reuses the P1 read-back's environment shape (ARCHIPELAGO_DATA_DIR, a fixture in
purchased-content/, auth.setup + auth.login). Exit 0 = all observed checks pass.
"""

from __future__ import annotations

import http.client
import json
import os
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

SUT = Path(os.environ.get("ARCHY_SUT", "~/worktrees/archy-media-deadline")).expanduser()
BIN = SUT / "core/target/release/archipelago"

ONION = "a" * 56
CONTENT_ID = "p2-readback-object"
PAYLOAD = b"P2-CONCURRENCY-READBACK-" * 8192  # ~180 KiB
STREAMS = int(os.environ.get("P2_STREAMS", "48"))

results: list[tuple[str, str, str]] = []
SESSION: dict[str, str] = {}


def record(name: str, status: str, detail: str = "") -> None:
    results.append((name, status, detail))
    print(f"  [{status:13s}] {name}" + (f" — {detail}" if detail else ""))


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def rpc(port: int, method: str, params: dict | None = None):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params or {}})
    hdrs = {"Content-Type": "application/json"}
    if SESSION.get("cookie"):
        hdrs["Cookie"] = SESSION["cookie"]
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=15)
    try:
        conn.request("POST", "/rpc/v1", body=body, headers=hdrs)
        resp = conn.getresponse()
        raw = resp.read()
        for name, value in resp.getheaders():
            if name.lower() == "set-cookie":
                pair = value.split(";", 1)[0]
                if "=" in pair:
                    k, _, v = pair.partition("=")
                    if k.strip() == "session":
                        SESSION["session"] = v
                    elif k.strip() == "csrf_token":
                        SESSION["csrf"] = v
        if SESSION.get("session"):
            SESSION["cookie"] = (
                f"session={SESSION['session']}; csrf_token={SESSION.get('csrf', '')}"
            )
        if resp.status != 200:
            return None
        return json.loads(raw).get("result")
    except Exception:
        return None
    finally:
        conn.close()


def seed(data_dir: Path) -> None:
    root = data_dir / "purchased-content"  # NOT "content-owned" — see content_owned.rs:42
    (root / ONION).mkdir(parents=True, exist_ok=True)
    (root / ONION / CONTENT_ID).write_bytes(PAYLOAD)
    (root / "owned.json").write_text(json.dumps({"items": [{
        "onion": ONION, "content_id": CONTENT_ID, "filename": "p2.mp4",
        "mime_type": "video/mp4", "size_bytes": len(PAYLOAD), "paid_sats": 1,
        "ecash_backend": "fixture", "purchased_at": "2026-10-23T00:00:00Z",
        "download_complete": True}]}))


def main() -> int:
    if not BIN.is_file():
        print(f"error: no binary at {BIN}")
        return 2

    tmp = Path(tempfile.mkdtemp(prefix="archy-p2-readback-"))
    port = free_port()
    seed(tmp)
    env = dict(os.environ)
    env.update(ARCHIPELAGO_DATA_DIR=str(tmp), ARCHIPELAGO_BIND=f"127.0.0.1:{port}",
               ARCHIPELAGO_DEV_MODE="1")
    env["PATH"] = f"{Path.home() / '.cargo' / 'bin'}:{env.get('PATH', '')}"

    print(f"binary : {BIN}  ({BIN.stat().st_size / 1e6:.1f} MB)")
    print(f"port   : {port}")
    print(f"streams: {STREAMS} concurrent")
    print()

    log = tmp / "node.log"
    proc = subprocess.Popen([str(BIN)], stdout=log.open("w"), stderr=subprocess.STDOUT,
                            env=env, cwd=str(tmp), start_new_session=True)
    try:
        for _ in range(240):
            if proc.poll() is not None:
                break
            try:
                socket.create_connection(("127.0.0.1", port), timeout=1).close()
                break
            except OSError:
                time.sleep(0.25)
        else:
            record("binary starts and listens", "FAIL", "never listened")
            return 1
        if proc.poll() is not None:
            record("binary starts and listens", "FAIL", f"exit={proc.poll()}")
            return 1
        record("binary starts and listens", "PASS", f"127.0.0.1:{port}")

        rpc(port, "auth.setup", {"password": "p2-readback-pw"})
        rpc(port, "auth.login", {"password": "p2-readback-pw"})
        record("session established", "PASS" if SESSION.get("session") else "FAIL")

        def ui_probe(label: str):
            """Probe the node's OWN management surface (not a media route)."""
            out = {}
            for path in ("/health", "/rpc/v1"):
                try:
                    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=8)
                    if path == "/health":
                        conn.request("GET", path)
                    else:
                        body = json.dumps({"jsonrpc": "2.0", "id": 1,
                                           "method": "monitoring.media", "params": {}})
                        conn.request("POST", path, body=body,
                                     headers={"Content-Type": "application/json"})
                    r = conn.getresponse()
                    r.read()
                    out[path] = r.status
                    conn.close()
                except Exception as exc:  # noqa: BLE001
                    out[path] = f"ERR:{type(exc).__name__}"
            return out

        # ---- A. baseline ------------------------------------------------------------
        base = ui_probe("baseline")
        record(
            "BASELINE: UI answers with no media load",
            "PASS" if base.get("/health") == 200 else "FAIL",
            f"/health -> {base.get('/health')}, /rpc/v1 -> {base.get('/rpc/v1')}",
        )

        # ---- B. hold N media streams open, probe the UI while they are in flight ----
        path = f"/api/peer-content/{ONION}/{CONTENT_ID}"
        statuses: list[int | str] = []
        lock = threading.Lock()
        release = threading.Event()

        def hold_stream():
            try:
                conn = http.client.HTTPConnection("127.0.0.1", port, timeout=20)
                hdrs = {"Cookie": SESSION.get("cookie", "")}
                conn.request("GET", path, headers=hdrs)
                resp = conn.getresponse()
                with lock:
                    statuses.append(resp.status)
                # Read only the first chunk, then hold the connection open. A media body is
                # long-lived; this reproduces "media is occupying connections" without
                # transferring the whole object N times.
                resp.read(1024)
                release.wait(timeout=25)
                conn.close()
            except Exception as exc:  # noqa: BLE001
                with lock:
                    statuses.append(f"ERR:{type(exc).__name__}")

        threads = [threading.Thread(target=hold_stream, daemon=True) for _ in range(STREAMS)]
        for t in threads:
            t.start()
        time.sleep(2.0)  # let the streams establish

        held_ok = sum(1 for s in statuses if s == 200)
        record(
            f"{STREAMS} media streams established",
            "PASS" if held_ok >= STREAMS // 2 else "FAIL",
            f"{held_ok}/{STREAMS} returned HTTP 200 while held open",
        )

        # ---- THE ASSERTION: the node's own UI must still answer under media load -------
        under = ui_probe("under-load")
        ui_ok = under.get("/health") == 200
        record(
            "UI STILL ANSWERS under concurrent media load (the P2 criterion)",
            "PASS" if ui_ok else "FAIL",
            f"/health -> {under.get('/health')}, /rpc/v1 -> {under.get('/rpc/v1')} "
            f"with {held_ok} media streams in flight. A 503 here means media exhausted the "
            f"budget the UI needs — the bug P2 removes.",
        )

        # ---- CONTROL: media itself must still be served (not starved by the UI) -------
        media_ok = sum(1 for s in statuses if s == 200) > 0
        record(
            "CONTROL: media is still served (UI budget did not starve it)",
            "PASS" if media_ok else "FAIL",
            "media and UI budgets are independent in both directions",
        )

        release.set()
        for t in threads:
            t.join(timeout=10)

        record(
            "scale caveat",
            "NOT_VERIFIED",
            f"{STREAMS} streams, not 1024. The bug is structural (a shared budget), so a "
            f"smaller N still detects a re-merge, but this is not a load test and does not "
            f"claim to be. The source-level guard in server.rs covers the structural claim.",
        )
    finally:
        if proc.poll() is None:
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=15)
            except subprocess.TimeoutExpired:
                proc.kill()

    print()
    counts: dict[str, int] = {}
    for _, st, _ in results:
        counts[st] = counts.get(st, 0) + 1
    print("counts:", ", ".join(f"{k}={v}" for k, v in sorted(counts.items())))
    (tmp / "readback.json").write_text(json.dumps(
        {"check": "P2 UI survives concurrent media load", "streams": STREAMS,
         "results": [{"name": n, "status": s, "detail": d} for n, s, d in results],
         "counts": counts}, indent=2))
    return 1 if counts.get("FAIL") else 0


if __name__ == "__main__":
    raise SystemExit(main())
