#!/usr/bin/env python3
"""
Cryptographically verify that a node's on-disk keys are deterministically
derived from its onboarding seed, exactly as documented in core/archipelago/
src/seed.rs:

    BIP-39 mnemonic (24 words)
      -> PBKDF2-HMAC-SHA512(2048, salt="mnemonic")           = 64-byte seed
        -> HKDF-SHA256(salt=None, IKM=seed, info=<domain>)    = each 32-byte key
             "archipelago/node/ed25519/v1"          -> node_key   (=> Node DID)
             "archipelago/nostr-node/secp256k1/v1"  -> nostr_secret (=> npub)
             "archipelago/fips/secp256k1/v1"        -> fips_key    (FIPS transport)

It compares each freshly-derived key against the bytes actually on disk under
/var/lib/archipelago/identity/. A MATCH proves the on-disk key was derived from
the seed (and nothing else). Also prints the resulting did:key for cross-check
against Settings -> Node DID.

Usage (run on the node):
    sudo python3 verify-seed-derivation.py
    # paste the 24-word mnemonic when prompted (input is hidden, never logged)

Pure standard library — no third-party crypto packages required.
"""
import sys, os, hmac, hashlib, getpass, unicodedata

IDENT = "/var/lib/archipelago/identity"

DOMAINS = {
    "node_key (=> Node DID)":      (b"archipelago/node/ed25519/v1",        f"{IDENT}/node_key",     "raw"),
    "nostr_secret (=> node npub)": (b"archipelago/nostr-node/secp256k1/v1", f"{IDENT}/nostr_secret", "nsec"),
    "fips_key (FIPS transport)":   (b"archipelago/fips/secp256k1/v1",       f"{IDENT}/fips_key",     "nsec"),
}


def hkdf_sha256(ikm: bytes, info: bytes, length: int = 32) -> bytes:
    """RFC 5869 HKDF-SHA256 with salt=None (== HashLen zero bytes)."""
    salt = b"\x00" * hashlib.sha256().digest_size
    prk = hmac.new(salt, ikm, hashlib.sha256).digest()
    okm, t, i = b"", b"", 1
    while len(okm) < length:
        t = hmac.new(prk, t + info + bytes([i]), hashlib.sha256).digest()
        okm += t
        i += 1
    return okm[:length]


# --- minimal bech32 decode (BIP-173) to recover the 32-byte secret from nsec ---
_B32 = "qpzry9x8gf2tvdw0s3jn54khce6mua7l"


def _bech32_decode_data(s: str) -> bytes:
    s = s.strip().lower()
    pos = s.rfind("1")
    data = [_B32.index(c) for c in s[pos + 1:]]
    data = data[:-6]  # drop 6-char checksum
    # convert 5-bit groups -> 8-bit bytes
    acc = bits = 0
    out = bytearray()
    for v in data:
        acc = (acc << 5) | v
        bits += 5
        if bits >= 8:
            bits -= 8
            out.append((acc >> bits) & 0xFF)
    return bytes(out)


# --- minimal base58btc + multicodec to render did:key from the ed25519 pubkey ---
_B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


def _b58(b: bytes) -> str:
    n = int.from_bytes(b, "big")
    s = ""
    while n:
        n, r = divmod(n, 58)
        s = _B58[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\x00"))) + s


def did_key_from_ed25519_pub(pub: bytes) -> str:
    return "did:key:z" + _b58(b"\xed\x01" + pub)  # 0xed01 = ed25519-pub multicodec


def main() -> int:
    if not os.path.isdir(IDENT):
        print(f"!! {IDENT} not found — run this on a node.")
        return 2
    mnemonic = getpass.getpass("Paste the node's 24-word mnemonic (hidden): ").strip()
    words = mnemonic.split()
    if len(words) != 24:
        print(f"!! expected 24 words, got {len(words)}")
        return 2
    # BIP-39: seed = PBKDF2-HMAC-SHA512(NFKD(mnemonic), "mnemonic"+passphrase, 2048, 64)
    norm = unicodedata.normalize("NFKD", " ".join(words)).encode("utf-8")
    seed = hashlib.pbkdf2_hmac("sha512", norm, b"mnemonic", 2048, 64)

    all_ok = True
    for name, (info, path, fmt) in DOMAINS.items():
        derived = hkdf_sha256(seed, info, 32)
        try:
            raw = open(path, "rb").read()
            disk = raw if fmt == "raw" else _bech32_decode_data(raw.decode().strip())
            disk = disk[:32]
        except Exception as e:
            print(f"[{name}] could not read {path}: {e}")
            all_ok = False
            continue
        ok = disk == derived
        all_ok &= ok
        print(f"[{'MATCH ✅' if ok else 'MISMATCH ❌'}] {name}")
        print(f"    derived(seed): {derived.hex()}")
        print(f"    on-disk      : {disk.hex()}")

    # Render the Node DID from node_key.pub for a visual cross-check vs the UI.
    try:
        pub = open(f"{IDENT}/node_key.pub", "rb").read()[:32]
        print(f"\nNode DID (from node_key.pub): {did_key_from_ed25519_pub(pub)}")
        print("  ^ should equal Settings -> Node DID")
    except Exception:
        pass

    print("\n==> ALL KEYS SEED-DERIVED ✅" if all_ok else "\n==> SOME KEYS DID NOT MATCH ❌")
    return 0 if all_ok else 1


if __name__ == "__main__":
    sys.exit(main())
