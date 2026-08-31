#!/usr/bin/env python3
"""
NIP-46 test client for the Archipelago companion's Remote Signer (#139).

Plays the role the node's login flow will play (rust-nostr nostr-connect
client): generates a nostrconnect:// pairing QR, connects to a relay, waits
for the phone's bunker `connect` (secret echo), acks it, then exercises
get_public_key + sign_event and VERIFIES the returned schnorr signature with
independent pure-Python BIP-340 code (no shared code with the phone's Rust).

Run it on your computer next to the phone:

    python3 -m venv /tmp/nip46env
    /tmp/nip46env/bin/pip install websockets qrcode
    /tmp/nip46env/bin/python Android/tools/nip46-test-client.py [--relay wss://relay.damus.io]

…then on the phone: hub menu (three-finger hold) → Remote Signer →
Generate key (once) → Scan pairing QR → point at the terminal QR → Approve.

Pure Python (no deps for the crypto; websockets + qrcode for transport/QR).
"""
import argparse
import asyncio
import base64
import hashlib
import hmac
import json
import os
import secrets
import struct
import sys
import time
import urllib.parse

import websockets  # pip install websockets

# ── secp256k1 / BIP-340 (independent of the phone's Rust code) ──────────────

P = 2**256 - 2**32 - 977
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
GX = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
GY = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8
G = (GX, GY)


def _add(pt1, pt2):
    if pt1 is None:
        return pt2
    if pt2 is None:
        return pt1
    x1, y1 = pt1
    x2, y2 = pt2
    if x1 == x2 and (y1 + y2) % P == 0:
        return None
    if pt1 == pt2:
        lam = (3 * x1 * x1) * pow(2 * y1, -1, P) % P
    else:
        lam = (y2 - y1) * pow(x2 - x1, -1, P) % P
    x3 = (lam * lam - x1 - x2) % P
    return (x3, (lam * (x1 - x3) - y1) % P)


def _mul(k, pt):
    r = None
    while k:
        if k & 1:
            r = _add(r, pt)
        pt = _add(pt, pt)
        k >>= 1
    return r


def lift_x(x):
    if x >= P:
        return None
    y_sq = (pow(x, 3, P) + 7) % P
    y = pow(y_sq, (P + 1) // 4, P)
    if y * y % P != y_sq:
        return None
    return (x, y if y % 2 == 0 else P - y)


def tagged(tag: bytes, data: bytes) -> bytes:
    """BIP-340 tagged hash: sha256(hash(tag) || hash(tag) || data)."""
    th = hashlib.sha256(tag).digest()
    return hashlib.sha256(th + th + data).digest()


def bip340_sign(msg: bytes, seckey: int, aux: bytes) -> bytes:
    d = seckey if seckey <= N - 1 else seckey - N
    pub = _mul(d, G)
    if pub[1] % 2 != 0:
        d = N - d
    t = bytes(a ^ b for a, b in zip(d.to_bytes(32, "big"), tagged(b"BIP0340/aux", aux)))
    rand = tagged(b"BIP0340/nonce", t + pub[0].to_bytes(32, "big") + msg)
    k = int.from_bytes(rand, "big") % N
    assert k > 0
    R = _mul(k, G)
    if R[1] % 2 != 0:
        k = N - k
    e = int.from_bytes(tagged(b"BIP0340/challenge", R[0].to_bytes(32, "big") + pub[0].to_bytes(32, "big") + msg), "big") % N
    return R[0].to_bytes(32, "big") + ((k + e * d) % N).to_bytes(32, "big")


def bip340_verify(msg: bytes, pubkey_x: bytes, sig: bytes) -> bool:
    """Check s·G − e·P == R with even-y R and x(R) == r (BIP-340)."""
    if len(sig) != 64 or len(pubkey_x) != 32:
        return False
    pub = lift_x(int.from_bytes(pubkey_x, "big"))
    if pub is None:
        return False
    r = int.from_bytes(sig[:32], "big")
    s = int.from_bytes(sig[32:], "big")
    if r >= P or s >= N:
        return False
    e = int.from_bytes(tagged(b"BIP0340/challenge", sig[:32] + pubkey_x + msg), "big") % N
    sg = _mul(s, G)
    ep = _mul(e, pub)
    neg_ep = (ep[0], (P - ep[1]) % P)
    rp = _add(sg, neg_ep)
    return rp is not None and rp[0] == r and rp[1] % 2 == 0


def ecdh_x(secret_hex: str, peer_x_hex: str) -> bytes:
    """Raw ECDH x-coordinate against an x-only peer key (even-y lift)."""
    peer = lift_x(int(peer_x_hex, 16))
    assert peer is not None, "peer pubkey not on curve"
    pt = _mul(int(secret_hex, 16) % N, peer)
    return pt[0].to_bytes(32, "big")


# ── NIP-44 v2 (pure python, spec-literal) ────────────────────────────────────

def hkdf_extract(salt: bytes, ikm: bytes) -> bytes:
    return hmac.new(salt, ikm, hashlib.sha256).digest()


def hkdf_expand(prk: bytes, info: bytes, length: int) -> bytes:
    t = b""
    out = b""
    i = 1
    while len(out) < length:
        t = hmac.new(prk, t + info + bytes([i]), hashlib.sha256).digest()
        out += t
        i += 1
    return out[:length]


def _rotl(x: int, n: int) -> int:
    return ((x << n) | (x >> (32 - n))) & 0xFFFFFFFF


def _qr(s, a, b, c, d):
    s[a] = (s[a] + s[b]) & 0xFFFFFFFF; s[d] ^= s[a]; s[d] = _rotl(s[d], 16)
    s[c] = (s[c] + s[d]) & 0xFFFFFFFF; s[b] ^= s[c]; s[b] = _rotl(s[b], 12)
    s[a] = (s[a] + s[b]) & 0xFFFFFFFF; s[d] ^= s[a]; s[d] = _rotl(s[d], 8)
    s[c] = (s[c] + s[d]) & 0xFFFFFFFF; s[b] ^= s[c]; s[b] = _rotl(s[b], 7)


def chacha20_block(key: bytes, counter: int, nonce: bytes) -> bytes:
    consts = [0x61707865, 0x3320646E, 0x79622D32, 0x6B206574]
    state = consts + list(struct.unpack("<8I", key)) + [counter] + list(struct.unpack("<3I", nonce))
    working = list(state)
    for _ in range(10):
        _qr(working, 0, 4, 8, 12); _qr(working, 1, 5, 9, 13)
        _qr(working, 2, 6, 10, 14); _qr(working, 3, 7, 11, 15)
        _qr(working, 0, 5, 10, 15); _qr(working, 1, 6, 11, 12)
        _qr(working, 2, 7, 8, 13); _qr(working, 3, 4, 9, 14)
    return struct.pack("<16I", *[(x + y) & 0xFFFFFFFF for x, y in zip(working, state)])


def chacha20(key: bytes, nonce: bytes, data: bytes) -> bytes:
    counter = 0  # NIP-44: "ChaCha20 (RFC 8439) with starting counter set to 0"
    out = bytearray()
    for i in range(0, len(data), 64):
        ks = chacha20_block(key, counter, nonce)
        chunk = data[i:i + 64]
        out += bytes(a ^ b for a, b in zip(chunk, ks))
        counter += 1
    return bytes(out)


def calc_padded_len(n: int) -> int:
    if n <= 32:
        return 32
    power = 1 << ((n - 1).bit_length())
    chunk = 32 if power <= 256 else power // 8
    return chunk * ((n - 1) // chunk + 1)


def nip44_encrypt(secret_hex: str, peer_hex: str, plaintext: str) -> str:
    ck = hkdf_extract(b"nip44-v2", ecdh_x(secret_hex, peer_hex))
    nonce = secrets.token_bytes(32)
    okm = hkdf_expand(ck, nonce, 76)
    key, iv, mac_key = okm[:32], okm[32:44], okm[44:76]
    pt = plaintext.encode()
    padded = (len(pt).to_bytes(2, "big") if len(pt) < 65536 else b"\x00\x00" + len(pt).to_bytes(4, "big")) + pt
    padded += b"\x00" * (calc_padded_len(len(pt)) - len(pt))
    ct = chacha20(key, iv, padded)
    mac = hmac.new(mac_key, nonce + ct, hashlib.sha256).digest()
    return base64.b64encode(bytes([2]) + nonce + ct + mac).decode()


def nip44_decrypt(secret_hex: str, peer_hex: str, payload: str) -> str:
    data = base64.b64decode(payload)
    assert data[0] == 2, "only NIP-44 v2 supported"
    nonce, ct, mac = data[1:33], data[33:-32], data[-32:]
    ck = hkdf_extract(b"nip44-v2", ecdh_x(secret_hex, peer_hex))
    okm = hkdf_expand(ck, nonce, 76)
    key, iv, mac_key = okm[:32], okm[32:44], okm[44:76]
    assert hmac.compare_digest(hmac.new(mac_key, nonce + ct, hashlib.sha256).digest(), mac), "bad MAC"
    padded = chacha20(key, iv, ct)
    ln = int.from_bytes(padded[:2], "big")
    body = padded[2:2 + ln] if ln else padded[6:6 + int.from_bytes(padded[2:6], "big")]
    return body.decode()


# ── nostr events ─────────────────────────────────────────────────────────────

def event_id(pubkey_hex: str, created_at: int, kind: int, tags, content: str) -> str:
    serialized = json.dumps([0, pubkey_hex, created_at, kind, tags, content], separators=(",", ":"))
    return hashlib.sha256(serialized.encode()).hexdigest()


def sign_event(secret_hex: str, event: dict) -> dict:
    eid = event_id(event["pubkey"], event["created_at"], event["kind"], event["tags"], event["content"])
    ev = dict(event)
    ev["id"] = eid
    ev["sig"] = bip340_sign(bytes.fromhex(eid), int(secret_hex, 16), os.urandom(32)).hex()
    return ev


# ── the client session ────────────────────────────────────────────────────────

def compact(d) -> str:
    return json.dumps(d, separators=(",", ":"))


async def run(relay: str):
    client_secret = os.urandom(32).hex()
    client_secret_int = int(client_secret, 16) % N
    client_pub_hex = _mul(client_secret_int, G)[0].to_bytes(32, "big").hex()
    pair_secret = secrets.token_hex(16)
    nonce = secrets.token_hex(8)

    uri = (
        f"nostrconnect://{client_pub_hex}"
        f"?relay={urllib.parse.quote(relay, safe='')}"
        f"&secret={pair_secret}"
        f"&name=Archipelago+Test+Client"
    )

    print(f"· client key  : {client_pub_hex}")
    print(f"· relay       : {relay}")
    print()
    print("Scan this QR with: Companion → hub (3-finger) → Remote Signer → Scan pairing QR")
    print()

    try:
        import qrcode
        qr = qrcode.QRCode(border=1)
        qr.add_data(uri)
        qr.make(fit=True)
        qr.print_ascii(invert=True)
    except ImportError:
        print(uri)

    print()
    print("Waiting for the phone to pair (connect, ack, get_public_key, sign_event)…")

    async with websockets.connect(relay, max_size=2**22) as ws:
        await ws.send(compact(["REQ", "test", {"kinds": [24133], "#p": [client_pub_hex], "since": int(time.time()) - 60}]))

        signer_pub = None
        acked = False
        requests = []

        def send_frame(content: dict):
            assert signer_pub is not None
            ev = {
                "pubkey": client_pub_hex,
                "created_at": int(time.time()),
                "kind": 24133,
                "tags": [["p", signer_pub]],
                "content": nip44_encrypt(client_secret, signer_pub, compact(content)),
            }
            return asyncio.ensure_future(ws.send(compact(["EVENT", sign_event(client_secret, ev)])))

        async def request(method, params, rid):
            send_frame({"id": rid, "method": method, "params": params})

        timeout = time.time() + 120
        got_pubkey = None
        signed_event = None

        while time.time() < timeout:
            try:
                raw = await asyncio.wait_for(ws.recv(), timeout=timeout - time.time())
            except (asyncio.TimeoutError, TimeoutError):
                break
            arr = json.loads(raw)
            if not isinstance(arr, list) or len(arr) < 3 or arr[0] != "EVENT":
                continue
            ev = arr[2]
            if ev.get("kind") != 24133 or ev.get("pubkey") == client_pub_hex:
                continue
            author = ev["pubkey"]
            try:
                msg = json.loads(nip44_decrypt(client_secret, author, ev["content"]))
            except Exception:
                continue

            if "method" in msg and msg["method"] == "connect":
                params = msg.get("params", [])
                if params and params[0] == author and (len(params) < 2 or params[1] == pair_secret):
                    signer_pub = author
                    print(f"✓ phone paired  — signer pubkey {author[:16]}…")
                    send_frame({"id": msg["id"], "result": "ack"})
                    acked = True
                    await asyncio.sleep(0.5)
                    await request("get_public_key", [], nonce + "-gpk")
                else:
                    print("✗ phone sent connect but the secret didn't match")
                    return 1
                continue

            if "result" in msg or "error" in msg:
                rid = msg.get("id", "")
                if "error" in msg:
                    print(f"✗ error for {rid}: {msg['error']}")
                    if rid.endswith("-sign"):
                        return 1
                    continue
                result = msg.get("result", "")
                if rid.endswith("-gpk"):
                    got_pubkey = result
                    print(f"✓ get_public_key → {result}")
                    await request(
                        "sign_event",
                        [compact({
                            "kind": 1,
                            "content": "Hello from the Archipelago NIP-46 test client — approved by hand.",
                            "tags": [],
                            "created_at": int(time.time()),
                        })],
                        nonce + "-sign",
                    )
                elif rid.endswith("-sign"):
                    signed_event = json.loads(result)
                    print(f"✓ sign_event → signed event {signed_event.get('id', '')[:16]}…")
                    break

        if not acked:
            print("✗ the phone never connected (2-minute timeout)")
            return 1
        if got_pubkey is None or got_pubkey != signer_pub:
            print("✗ get_public_key missing or mismatched")
            return 1
        if signed_event is None:
            return 1

        ev = signed_event
        expected_id = event_id(ev["pubkey"], ev["created_at"], ev["kind"], ev["tags"], ev["content"])
        ok_id = expected_id == ev["id"]
        ok_sig = bip340_verify(bytes.fromhex(expected_id), bytes.fromhex(ev["pubkey"]), bytes.fromhex(ev["sig"]))
        print(f"· event id correct : {ok_id}")
        print(f"· schnorr signature: {'VERIFIED ✓' if ok_sig else 'INVALID ✗'}")
        if ok_id and ok_sig:
            print()
            print("END-TO-END PASS — the companion signed as the identity the phone holds,")
            print("and the signature verifies under an independent BIP-340 implementation.")
            return 0
        return 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--relay", default="wss://relay.damus.io", help="any nostr relay both devices can reach")
    args = ap.parse_args()
    sys.exit(asyncio.run(run(args.relay)))


if __name__ == "__main__":
    main()
