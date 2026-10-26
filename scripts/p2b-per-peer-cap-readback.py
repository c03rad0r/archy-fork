#!/usr/bin/env python3
"""P2b READ-BACK — prove the PER-PEER connection cap on the real peer listener's wire.

The contract this closes: `PEER_CONNECTION_BUDGET` fixed "media starves the UI" but it is a
*shared* pool, so one peer could still take every permit and starve every other peer. The
unit tests prove the limiter's accounting; they cannot prove that a real accepted socket is
refused. This script drives the real release binary's PEER listener (`fips0` ULA:5679,
bound by `peer_late_bind_loop`) and counts what the kernel actually hands back.

WHAT IT PROVES, one observation per assertion:
  1. the peer listener binds on the fips0 ULA (the interface exists on this host);
  2. `CAP` concurrent connections from this peer are admitted (the cap is not zero/short);
  3. the connection AFTER the cap is answered **429** — and 429 specifically, not the 503
     that means the shared listener budget is exhausted. The two are different failures and
     a test that accepted either would not distinguish them;
  4. the UI listener is unaffected while the peer cap is saturated (the whole point of
     keying the cap by peer rather than shrinking the budget);
  5. closing one peer connection returns its slot — a new connection is admitted again, so a
     peer that bursts is never permanently locked out.

WHAT IT CANNOT PROVE, stated rather than implied:
  * that TWO DIFFERENT peers are isolated from each other. Every connection here originates
    from this host, so they all share one peer key. The isolation claim rests on the limiter
    being keyed by address (behavioural unit test `one_peer_cannot_take_every_slot`), not on
    this script.
  * that a real FIPS mesh peer is refused. This exercises the real listener and the real
    accept path on the real binary, but not a second machine.

The cap is read from the source constant so this cannot silently drift from the code.

Exit: 0 all observed checks pass; 1 a check failed; 2 could not run.
"""

from __future__ import annotations

import json
import os
import re
import signal
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SUT = Path(os.environ.get("ARCHY_SUT", "~/worktrees/archy-media-deadline")).expanduser()
BIN = SUT / "core/target/release/archipelago"

PEER_PORT = 5679
FIPS_IFACE = "fips0"

results: list[tuple[str, str, str]] = []


def record(name: str, status: str, detail: str = "") -> None:
    results.append((name, status, detail))
    print(f"  [{status:13s}] {name}" + (f" — {detail}" if detail else ""))


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def cap_from_source() -> int | None:
    """Read PEER_PER_PEER_CONNECTION_CAP out of server.rs so the script cannot drift."""
    src = SUT / "core" / "archipelago" / "src" / "server.rs"
    try:
        text = src.read_text()
    except OSError:
        return None
    m = re.search(r"const PEER_PER_PEER_CONNECTION_CAP: usize = (\d+);", text)
    return int(m.group(1)) if m else None


def fips0_ula() -> str | None:
    """ULA (fd00::/8) on fips0, read from /proc the way the binary itself does."""
    try:
        contents = Path("/proc/net/if_inet6").read_text()
    except OSError:
        return None
    for line in contents.splitlines():
        parts = line.split()
        if len(parts) != 6 or parts[5] != FIPS_IFACE:
            continue
        hexaddr = parts[0]
        if len(hexaddr) != 32:
            continue
        octets = bytes.fromhex(hexaddr)
        if (octets[0] & 0xFE) != 0xFC:
            continue
        return socket.inet_ntop(socket.AF_INET6, octets)
    return None


def open_peer_conn(ula: str, timeout: float = 5.0) -> socket.socket:
    s = socket.socket(socket.AF_INET6, socket.SOCK_STREAM)
    s.settimeout(timeout)
    s.connect((ula, PEER_PORT, 0, 0))
    return s


def probe(port: int, path: str, timeout: float = 10.0) -> tuple[int, bytes]:
    conn = socket.create_connection(("127.0.0.1", port), timeout=timeout)
    try:
        conn.sendall(f"GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n".encode())
        chunks = []
        while True:
            try:
                b = conn.recv(65536)
            except socket.timeout:
                break
            if not b:
                break
            chunks.append(b)
        raw = b"".join(chunks)
        status = 0
        if raw.startswith(b"HTTP/1."):
            try:
                status = int(raw.split(b" ", 2)[1])
            except (IndexError, ValueError):
                status = 0
        return status, raw
    finally:
        conn.close()


def peer_port_owner(port: int) -> int | None:
    """PID owning a LISTEN socket on `port`, via `ss -ltnp`.

    WHY THIS EXISTS: this read-back originally reported "peer accepting=True" and then measured
    whatever answered on 5679. A FOREIGN process already held the port, our own child logged
    `bind ... Address already in use` and never bound, and the probe happily reported the other
    process's behaviour (200 instead of 429) as if it were ours. An availability probe is not an
    identity probe: a port that answers proves nothing about WHICH binary answers.
    """
    try:
        out = subprocess.run(
            ["ss", "-ltnp"], capture_output=True, text=True, timeout=10
        ).stdout
    except (OSError, subprocess.SubprocessError):
        return None
    for line in out.splitlines():
        if f"]:{port} " not in line and f"]:{port}\t" not in line:
            continue
        m = re.search(r"pid=(\d+)", line)
        if m:
            return int(m.group(1))
    return None


def status_of_refusal(sock: socket.socket, timeout: float = 5.0) -> int:
    """Return the HTTP status this accepted socket carries.

    ORDER MATTERS. The over-cap branch in `accept_loop` writes `429` and closes WITHOUT
    waiting for a request, so the response is already waiting and sending first can raise
    EPIPE on a half-closed socket. An ADMITTED connection instead waits for headers, so it
    needs a nudge. Try to read first; only send if nothing is there.
    """
    sock.settimeout(timeout)
    try:
        head = sock.recv(256)
    except (socket.timeout, OSError):
        head = b""
    if not head:
        try:
            sock.sendall(b"GET /health HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
            head = sock.recv(256)
        except (socket.timeout, OSError):
            return 0
    m = re.match(rb"HTTP/1\.[01] (\d{3})", head)
    return int(m.group(1)) if m else 0


def main() -> int:
    if not BIN.is_file():
        print(f"error: no binary at {BIN}")
        return 2

    cap = cap_from_source()
    if cap is None:
        record("cap is readable from source", "FAIL",
               "PEER_PER_PEER_CONNECTION_CAP not found in server.rs — cannot pin the expectation")
        print("\ncounts: FAIL=1")
        return 2
    record("cap is readable from source", "PASS", f"PEER_PER_PEER_CONNECTION_CAP={cap}")

    ula = fips0_ula()
    if ula is None:
        record(f"{FIPS_IFACE} carries a ULA", "NOT_VERIFIED",
               f"no fd00::/8 address on {FIPS_IFACE} in /proc/net/if_inet6 — the peer listener "
               f"binds on that ULA, so the wire check cannot run on this host")
        print("\ncounts: " + ", ".join(f"{k}={v}" for k, v in _counts().items()))
        return 0
    record(f"{FIPS_IFACE} carries a ULA", "PASS", f"{ula} (peer listener binds [{ula}]:{PEER_PORT})")

    tmp = Path(tempfile.mkdtemp(prefix="archy-p2b-readback-"))
    port = free_port()
    log = tmp / "node.log"

    env = dict(os.environ)
    env["PATH"] = f"{Path.home() / '.cargo' / 'bin'}:{env.get('PATH', '')}"
    env["ARCHIPELAGO_DATA_DIR"] = str(tmp)
    env["ARCHIPELAGO_BIND"] = f"127.0.0.1:{port}"
    env["ARCHIPELAGO_DEV_MODE"] = "1"

    print(f"binary : {BIN}  ({BIN.stat().st_size / 1e6:.1f} MB)")
    print(f"data   : {tmp}")
    print(f"ui port: {port}   peer: [{ula}]:{PEER_PORT}")
    print()

    proc = subprocess.Popen(
        [str(BIN)],
        stdout=log.open("w"),
        stderr=subprocess.STDOUT,
        env=env,
        cwd=str(tmp),
        start_new_session=True,
    )
    held: list[socket.socket] = []
    try:
        # ---- 1. wait for BOTH listeners ------------------------------------------------
        # The peer listener is late-bound: `peer_late_bind_loop` polls every 30 s, so this
        # must wait past one tick or it will report a bind failure that is really patience.
        deadline = time.time() + 60
        ui_ok = peer_ok = False
        while time.time() < deadline:
            if proc.poll() is not None:
                break
            if not ui_ok:
                try:
                    ui_ok = probe(port, "/health", timeout=2)[0] in (200, 404)
                except OSError:
                    pass
            if ui_ok and not peer_ok:
                try:
                    s = open_peer_conn(ula, timeout=2)
                    s.close()
                    peer_ok = True
                except OSError:
                    pass
            if ui_ok and peer_ok:
                break
            time.sleep(1.0)

        if proc.poll() is not None:
            tail = "\n".join(log.read_text(errors="replace").splitlines()[-12:])
            record("binary starts and serves both listeners", "FAIL",
                   f"process exited early; last lines:\n{tail}")
            print("\ncounts: " + ", ".join(f"{k}={v}" for k, v in _counts().items()))
            return 1
        record("binary starts and serves both listeners", "PASS" if (ui_ok and peer_ok) else "FAIL",
               f"UI 127.0.0.1:{port} healthy={ui_ok}; peer [{ula}]:{PEER_PORT} accepting={peer_ok}")
        if not (ui_ok and peer_ok):
            print("\ncounts: " + ", ".join(f"{k}={v}" for k, v in _counts().items()))
            return 1

        # ---- 1b. IDENTITY: is the listener on the peer port OURS? ----------------------
        # A port that answers is not proof that our binary answers. Measured failure: another
        # `archy-node` held 5679, our child logged "Address already in use", and the probe went
        # on to measure the OTHER process — reporting its 200 (no cap) as if it were a
        # regression. Refuse to measure a listener we do not own.
        owner = peer_port_owner(PEER_PORT)
        ours = owner == proc.pid
        if not ours:
            log_tail = "\n".join(log.read_text(errors="replace").splitlines()[-8:])
            record("the peer listener on the wire is OUR binary", "FAIL",
                   f"port {PEER_PORT} is held by pid {owner}, our child is pid {proc.pid}. "
                   f"Whatever answers there is a different build, so nothing measured below "
                   f"would be attributable to this binary. Node log tail:\n{log_tail}")
            print("\ncounts: " + ", ".join(f"{k}={v}" for k, v in _counts().items()))
            return 1
        record("the peer listener on the wire is OUR binary", "PASS",
               f"port {PEER_PORT} owned by our pid {proc.pid}")

        # ---- 2. fill the peer to exactly its cap ---------------------------------------
        # These are held open WITHOUT sending a request: the slot is taken at accept(), which
        # is precisely the thing under test. Sending a request would also work but would put
        # the header-read timeout in the way.
        for i in range(cap):
            try:
                held.append(open_peer_conn(ula, timeout=5))
            except OSError as e:
                record(f"peer admits {cap} concurrent connections", "FAIL",
                       f"connection {i + 1} of {cap} was refused before the cap: {e}")
                break
        if len(held) == cap:
            record(f"peer admits {cap} concurrent connections", "PASS",
                   f"{cap}/{cap} established and held open (cap is not shorter than declared)")

        # ---- 3. the connection AFTER the cap must be 429 --------------------------------
        try:
            over = open_peer_conn(ula, timeout=5)
        except OSError as e:
            record("the connection past the cap is refused", "FAIL",
                   f"expected an accepted socket carrying a 429, got a connect error: {e}")
            over = None
        if over is not None:
            code = status_of_refusal(over)
            over.close()
            if code == 429:
                record("the connection past the cap is refused", "PASS",
                       f"connection {cap + 1} answered HTTP 429 (Too Many Requests)")
            else:
                record("the connection past the cap is refused", "FAIL",
                       f"connection {cap + 1} answered {code or 'no response'} instead of 429 — "
                       f"a 503 here would mean the SHARED budget was hit, not the per-peer cap")
            # The refusal must be distinguishable from budget exhaustion.
            record("refusal is 429, not the 503 used for budget exhaustion",
                   "PASS" if code == 429 else "FAIL",
                   f"observed {code or 'none'}")

        # ---- 4. the UI listener is unaffected by a saturated peer cap ------------------
        ui_status, _ = probe(port, "/health")
        record("the UI listener still answers while the peer cap is saturated",
               "PASS" if ui_status == 200 else "FAIL",
               f"GET /health -> {ui_status} with {len(held)} peer connections at the cap")

        # ---- 5. a released slot is re-admitted -----------------------------------------
        if held:
            held.pop().close()
            time.sleep(0.5)  # let the connection task observe EOF and drop its slot
            try:
                again = open_peer_conn(ula, timeout=5)
                code_again = status_of_refusal(again)
                again.close()
                admitted = code_again != 429
                record("closing a peer connection returns its slot", "PASS" if admitted else "FAIL",
                       f"after releasing one slot, a new connection answered {code_again or 'no response'}"
                       + ("" if admitted else " — still 429, so the slot was never returned"))
            except OSError as e:
                record("closing a peer connection returns its slot", "FAIL",
                       f"new connection could not even be established: {e}")

        record("two DIFFERENT peers are isolated from each other", "NOT_VERIFIED",
               "every connection here originates from this host, so they share one peer key. "
               "The isolation claim rests on address-keyed accounting "
               "(unit test one_peer_cannot_take_every_slot), not on this script")

        # ---- 6. shallow control: the refusal is keyed to the COUNT, not a latch ---------
        # Without this, "the 65th connection got 429" is also consistent with a latch that
        # refuses everything, or with a machine-level limit. Hold a deliberately small number
        # and require the next connection to be ADMITTED.
        for s in list(held):
            try:
                s.close()
            except OSError:
                pass
        held.clear()
        time.sleep(1.0)  # let the connection tasks observe EOF and return their slots

        shallow = 3
        shallow_held: list[socket.socket] = []
        try:
            for _ in range(shallow):
                shallow_held.append(open_peer_conn(ula, timeout=5))
            probe_sock = open_peer_conn(ula, timeout=5)
            code_shallow = status_of_refusal(probe_sock)
            probe_sock.close()
            record("below the cap the next connection is admitted (429 is count-keyed)",
                   "PASS" if code_shallow != 429 else "FAIL",
                   f"with {shallow} held, connection {shallow + 1} answered "
                   f"{code_shallow or 'no response'}"
                   + ("" if code_shallow != 429 else
                      " — a 429 this shallow means the refusal is a latch, not a cap"))
        except OSError as e:
            record("below the cap the next connection is admitted (429 is count-keyed)",
                   "FAIL", f"could not establish the shallow probe: {e}")
        finally:
            for s in shallow_held:
                try:
                    s.close()
                except OSError:
                    pass

    finally:
        for s in held:
            try:
                s.close()
            except OSError:
                pass
        if proc.poll() is None:
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=15)
            except subprocess.TimeoutExpired:
                proc.kill()

    print()
    counts = _counts()
    print("counts:", ", ".join(f"{k}={v}" for k, v in sorted(counts.items())))

    out = Path(os.environ.get("ARCHY_READBACK_OUT", tmp / "readback.json"))
    out.write_text(json.dumps(
        {
            "check": "P2b per-peer connection cap",
            "binary": str(BIN),
            "binary_sha256": subprocess.run(
                ["sha256sum", str(BIN)], capture_output=True, text=True
            ).stdout.split(" ")[0],
            "cap": cap,
            "peer_ula": ula,
            "results": [{"name": n, "status": s, "detail": d} for n, s, d in results],
            "counts": counts,
        },
        indent=2,
    ))
    print(f"wrote {out}")

    return 1 if counts.get("FAIL") else 0


def _counts() -> dict[str, int]:
    counts: dict[str, int] = {}
    for _, status, _ in results:
        counts[status] = counts.get(status, 0) + 1
    return counts


if __name__ == "__main__":
    raise SystemExit(main())
