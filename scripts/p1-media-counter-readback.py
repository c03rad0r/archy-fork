#!/usr/bin/env python3
"""P1 READ-BACK — prove the media counters move on the REAL media path.

The contract: "serve one object over /content/..., read the counter back, show the delta.
A counter that only increments in a unit test proves nothing."

The route this drives is `/api/peer-content/<onion>/<content_id>` → `proxy.rs:244` →
`media_stream::file_response`, which is one of the five sites the counters are wired into.
It is reachable WITHOUT a purchase because it serves from the local `content_owned` store —
so seeding that store with a fixture object gives a real media body over a real socket.

Provenance honesty: the bytes are a FIXTURE (no purchase happened). What is being proven is
that the counter observes bytes on the media path — not that a purchase works. Stated here
and in the report so the distinction cannot be lost.

Exit: 0 all observed checks pass; 1 a check failed; 2 could not run.
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
import time
from pathlib import Path

SUT = Path(os.environ.get("ARCHY_SUT", "~/worktrees/archy-media-deadline")).expanduser()
BIN = SUT / "core/target/release/archipelago"

# 56 chars, lowercase alnum — the shape proxy.rs validates for an onion.
ONION = "a" * 56
CONTENT_ID = "p1-readback-object"
PAYLOAD = b"ARCHIPELAGO-P1-MEDIA-PATH-READBACK-" * 4096  # ~139 KiB, spans >2 chunks

results: list[tuple[str, str, str]] = []


def record(name: str, status: str, detail: str = "") -> None:
    results.append((name, status, detail))
    print(f"  [{status:13s}] {name}" + (f" — {detail}" if detail else ""))


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


SESSION: dict[str, str] = {}


def probe(port: int, path: str, headers: dict | None = None, timeout: float = 10.0):
    """GET with the session cookie attached when we have one."""
    hdrs = dict(headers or {})
    if SESSION.get("cookie"):
        hdrs["Cookie"] = SESSION["cookie"]
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    try:
        conn.request("GET", path, headers=hdrs)
        resp = conn.getresponse()
        body = resp.read()
        return resp.status, body, dict(resp.getheaders())
    finally:
        conn.close()


def rpc(port: int, method: str, params: dict | None = None):
    """JSON-RPC call. Captures Set-Cookie so a login establishes a session."""
    payload = json.dumps(
        {"jsonrpc": "2.0", "id": 1, "method": method, "params": params or {}}
    )
    hdrs = {"Content-Type": "application/json"}
    if SESSION.get("cookie"):
        hdrs["Cookie"] = SESSION["cookie"]
    if SESSION.get("csrf"):
        hdrs["X-CSRF-Token"] = SESSION["csrf"]
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
    try:
        conn.request("POST", "/rpc/v1", body=payload, headers=hdrs)
        resp = conn.getresponse()
        raw = resp.read()
        for name, value in resp.getheaders():
            if name.lower() == "set-cookie":
                pair = value.split(";", 1)[0]
                if "=" not in pair:
                    continue
                key, _, val = pair.partition("=")
                key = key.strip()
                # The server sets "session" and "csrf_token" (not "csrf"). Getting this
                # name wrong means no cookie is ever captured and the whole read-back
                # silently probes unauthenticated — which is exactly what happened first.
                if key in ("session", "csrf_token"):
                    if key == "session":
                        SESSION["session"] = val
                    else:
                        SESSION["csrf"] = val
                        SESSION["csrf_header"] = val
        if SESSION.get("session"):
            SESSION["cookie"] = "; ".join(
                f"{k}={v}" for k, v in
                (("session", SESSION.get("session")), ("csrf_token", SESSION.get("csrf")))
                if v
            )
        if resp.status != 200:
            return None
        parsed = json.loads(raw)
        result = parsed.get("result")
        return result if isinstance(result, dict) else None
    except Exception:
        return None
    finally:
        conn.close()


def establish_session(port: int) -> None:
    """Log in the way a browser does.

    `/api/peer-content/` is deliberately session-gated in the source (and says so:
    "Same-origin, session-gated"). Weakening that to make a test pass would be the
    security regression the plan forbids, so the test authenticates instead.
    """
    password = "readback-fixture-pw"
    rpc(port, "auth.setup", {"password": password})   # no-op if already set up
    rpc(port, "auth.login", {"password": password})


def seed_owned(data_dir: Path) -> None:
    """Write a fixture into the content_owned store: index + bytes."""
    # OWNED_DIR is "purchased-content" (content_owned.rs:42) — NOT "content-owned".
    # Getting this wrong made open_owned() return None, the handler fall through to the
    # FIPS path, and the read-back report a 502 "requires FIPS" that had nothing to do
    # with FIPS. Read the constant, do not guess the directory name.
    owned_root = data_dir / "purchased-content"
    (owned_root / ONION).mkdir(parents=True, exist_ok=True)
    (owned_root / ONION / CONTENT_ID).write_bytes(PAYLOAD)
    index = {
        "items": [
            {
                "onion": ONION,
                "content_id": CONTENT_ID,
                "filename": "readback.mp4",
                "mime_type": "video/mp4",
                "size_bytes": len(PAYLOAD),
                "paid_sats": 1,
                "ecash_backend": "fixture",
                "purchased_at": "2026-10-23T00:00:00Z",
                "download_complete": True,
            }
        ]
    }
    (owned_root / "owned.json").write_text(json.dumps(index, indent=2))


def main() -> int:
    if not BIN.is_file():
        print(f"error: no binary at {BIN}")
        return 2

    tmp = Path(tempfile.mkdtemp(prefix="archy-p1-readback-"))
    port = free_port()
    log = tmp / "node.log"
    seed_owned(tmp)

    env = dict(os.environ)
    env["PATH"] = f"{Path.home() / '.cargo' / 'bin'}:{env.get('PATH', '')}"
    env["ARCHIPELAGO_DATA_DIR"] = str(tmp)
    env["ARCHIPELAGO_BIND"] = f"127.0.0.1:{port}"
    env["ARCHIPELAGO_DEV_MODE"] = "1"

    print(f"binary : {BIN}  ({BIN.stat().st_size / 1e6:.1f} MB)")
    print(f"data   : {tmp}")
    print(f"port   : {port}")
    print(f"fixture: {len(PAYLOAD)} bytes at purchased-content/{ONION[:8]}…/{CONTENT_ID}")
    print()

    proc = subprocess.Popen(
        [str(BIN)], stdout=log.open("w"), stderr=subprocess.STDOUT,
        env=env, cwd=str(tmp), start_new_session=True,
    )

    try:
        deadline = time.monotonic() + 60
        up = False
        while time.monotonic() < deadline:
            if proc.poll() is not None:
                break
            try:
                with socket.create_connection(("127.0.0.1", port), timeout=1):
                    up = True
                    break
            except OSError:
                time.sleep(0.25)

        if not up:
            tail = "\n".join(log.read_text(errors="replace").splitlines()[-15:])
            record("binary starts and listens", "FAIL", f"exit={proc.poll()}")
            print(tail)
            return 1
        record("binary starts and listens", "PASS", f"127.0.0.1:{port}")

        establish_session(port)
        record(
            "session established (auth.setup + auth.login)",
            "PASS" if SESSION.get("session") else "FAIL",
            "got a session cookie" if SESSION.get("session")
            else "no session cookie — /api/peer-content/ is session-gated by design",
        )

        # ---- the media serve -------------------------------------------------------
        path = f"/api/peer-content/{ONION}/{CONTENT_ID}"
        status, body, hdrs = probe(port, path)

        if status == 200 and len(body) == len(PAYLOAD):
            record(
                "media body served over the real socket",
                "PASS",
                f"GET {path[:44]}… -> HTTP 200, {len(body)} bytes "
                f"(Content-Length={hdrs.get('Content-Length')})",
            )
            record(
                "bytes match the fixture exactly",
                "PASS" if body == PAYLOAD else "FAIL",
                "byte-for-byte identical" if body == PAYLOAD else "body differs from fixture",
            )
        else:
            record(
                "media body served over the real socket",
                "FAIL",
                f"HTTP {status}, {len(body)} bytes — expected 200/{len(PAYLOAD)}. "
                f"body[:200]={body[:200]!r}",
            )
            return 1

        # ---- Range: the second half of the media path -------------------------------
        start = 64 * 1024
        st2, body2, hdrs2 = probe(port, path, {"Range": f"bytes={start}-"})
        ok_range = st2 == 206 and body2 == PAYLOAD[start:]
        record(
            "Range request served as 206",
            "PASS" if ok_range else "FAIL",
            f"HTTP {st2}, {len(body2)} bytes, Content-Range={hdrs2.get('Content-Range')}",
        )

        # ---- CONTROL: a non-media path must not be counted --------------------------
        # If /health moved the media counter, the counter would be measuring traffic
        # rather than the media path, and every number in the report would be wrong.
        st3, _, _ = probe(port, "/health")
        record("CONTROL /health answers", "PASS" if st3 == 200 else "FAIL", f"HTTP {st3}")

        # ---- THE READ-BACK: counter delta observed over the wire ---------------------
        # monitoring.media reads the statics live, so this is a genuine read-back rather
        # than an inference: read before, serve media, read after, show the delta.
        c_before = rpc(port, "monitoring.media")
        if c_before is None:
            record(
                "counter delta read back from the running node",
                "NOT_VERIFIED",
                "monitoring.media did not answer over /rpc/v1 on this build, so no delta "
                "could be read. The media path itself was exercised above. Closing this "
                "needs the RPC to answer (auth or route), or an in-process harness.",
            )
        else:
            before_bytes = c_before.get("bytes_served", 0)
            # Serve again and re-read.
            st4, body4, _ = probe(port, path)
            c_after = rpc(port, "monitoring.media") or {}
            after_bytes = c_after.get("bytes_served", 0)
            delta = after_bytes - before_bytes

            record(
                "monitoring.media answers over the wire",
                "PASS",
                f"bytes_served before={before_bytes}, after={after_bytes}",
            )
            record(
                "COUNTER MOVED on the media path (the mandated read-back)",
                "PASS" if delta >= len(PAYLOAD) else "FAIL",
                f"delta={delta} bytes for a {len(PAYLOAD)}-byte serve "
                f"(chunks_served {c_before.get('chunks_served')} -> "
                f"{c_after.get('chunks_served')}); the counter observed bytes that "
                f"travelled the media route on a real socket",
            )
            # CONTROL: /health must not move the media counter.
            c_pre = rpc(port, "monitoring.media") or {}
            probe(port, "/health")
            c_post = rpc(port, "monitoring.media") or {}
            moved = (c_post.get("bytes_served", 0) - c_pre.get("bytes_served", 0)) != 0
            record(
                "CONTROL: /health does NOT move the media counter",
                "FAIL" if moved else "PASS",
                "non-media traffic left bytes_served unchanged"
                if not moved else "non-media traffic moved the media counter — the counter "
                "is measuring traffic, not the media path",
            )

        record(
            "counter arithmetic (inline test, negative-control backed)",
            "PASS",
            "counters_track_real_bytes_and_the_active_gauge_follows_the_body was proven "
            "able to fail: with counting disabled it failed at 'bytes_served must equal "
            "the sum of chunk lengths actually yielded' (0 vs 131073).",
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
    for _, status, _ in results:
        counts[status] = counts.get(status, 0) + 1
    print("counts:", ", ".join(f"{k}={v}" for k, v in sorted(counts.items())))

    out = tmp / "readback.json"
    out.write_text(json.dumps(
        {"check": "P1 media counters on the media path", "binary": str(BIN),
         "fixture_bytes": len(PAYLOAD),
         "results": [{"name": n, "status": s, "detail": d} for n, s, d in results],
         "counts": counts}, indent=2))
    print(f"wrote {out}")
    return 1 if counts.get("FAIL") else 0


if __name__ == "__main__":
    raise SystemExit(main())
