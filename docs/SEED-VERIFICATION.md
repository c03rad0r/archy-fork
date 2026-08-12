# Archipelago Seed Verification

Independently verify that your 24-word BIP-39 mnemonic produces the correct
Nostr keys and DID identifiers — using only standard cryptographic primitives,
no Archipelago code.

```
24-word mnemonic
      |
      v
PBKDF2-HMAC-SHA512 (2048 rounds, salt = "mnemonic")
      |
      v
64-byte master seed
      |
      +-- HKDF-SHA256 (info="archipelago/node/ed25519/v1")
      |       --> Node Ed25519 keypair --> did:key:z...
      |
      +-- HKDF-SHA256 (info="archipelago/nostr-node/secp256k1/v1")
      |       --> Node Nostr key --> npub1...
      |
      +-- HKDF-SHA256 (info="archipelago/fips/secp256k1/v1")
      |       --> FIPS mesh transport key --> npub1...
      |
      +-- HKDF-SHA256 (info="archipelago/identity/{i}/ed25519/v1")
      |       --> Identity[i] Ed25519 --> did:key:z...
      |
      +-- BIP-32 m/44'/1237'/0'/0/{i}  (NIP-06)
      |       --> Identity[i] Nostr key --> npub1...
      |
      +-- BIP-32 m/84'/0'/0'
      |       --> Bitcoin HD wallet
      |
      +-- HKDF-SHA256 (info="archipelago/lnd/entropy/v1")
              --> 16 bytes LND aezeed entropy
```

Source: [`core/archipelago/src/seed.rs`](../core/archipelago/src/seed.rs) and
[`core/archipelago/src/identity.rs`](../core/archipelago/src/identity.rs)

---

## Setup

```bash
pip3 install cryptography ecdsa
```

Two packages, both pure crypto, no network calls. Python 3.9+.

---

## The Verification Script

Save as `verify-seed.py` and run with your mnemonic:

```bash
MNEMONIC="word1 word2 ... word24" python3 verify-seed.py
```

```python
#!/usr/bin/env python3
"""
Archipelago seed derivation verifier.

Re-derives every key from a BIP-39 mnemonic using the exact same algorithms
as the Rust backend (seed.rs), so you can compare outputs independently.

Dependencies: cryptography, ecdsa  (pip3 install cryptography ecdsa)
No network calls. No file writes. Safe to run air-gapped.
"""

import hashlib, hmac, os, sys

# ── BIP-39: mnemonic --> 64-byte master seed ─────────────────────────────

def mnemonic_to_seed(mnemonic: str) -> bytes:
    """PBKDF2-HMAC-SHA512, 2048 rounds, salt = 'mnemonic', no passphrase."""
    return hashlib.pbkdf2_hmac(
        "sha512",
        mnemonic.encode("utf-8"),
        b"mnemonic",  # BIP-39 salt prefix + empty passphrase
        2048,
    )

# ── HKDF-SHA256 (RFC 5869) ──────────────────────────────────────────────

def hkdf_sha256(ikm: bytes, info: bytes, length: int = 32) -> bytes:
    """
    HKDF-Extract(salt=None, ikm) then HKDF-Expand(PRK, info, L).
    Salt=None means 32 zero bytes per RFC 5869 section 2.2.
    Matches: hkdf::Hkdf::<Sha256>::new(None, ikm).expand(info, &mut okm)
    """
    # Extract
    prk = hmac.new(b"\x00" * 32, ikm, hashlib.sha256).digest()
    # Expand (32 bytes = 1 block, only T(1) needed)
    t1 = hmac.new(prk, info + b"\x01", hashlib.sha256).digest()
    return t1[:length]

# ── Ed25519 ──────────────────────────────────────────────────────────────

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

def ed25519_keypair(secret_32: bytes) -> tuple[bytes, bytes]:
    """Returns (private_32, public_32) from a 32-byte seed."""
    sk = Ed25519PrivateKey.from_private_bytes(secret_32)
    pk = sk.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
    return secret_32, pk

# ── secp256k1 ────────────────────────────────────────────────────────────

from ecdsa import SECP256k1, SigningKey as ECDSASigningKey

def secp256k1_xonly(secret_32: bytes) -> bytes:
    """32-byte x-only pubkey (Schnorr/Nostr format) from private key bytes."""
    sk = ECDSASigningKey.from_string(secret_32, curve=SECP256k1)
    point = sk.get_verifying_key().pubkey.point
    return point.x().to_bytes(32, "big")

# ── BIP-32 HD derivation (secp256k1) ────────────────────────────────────

import struct

SECP256K1_N = SECP256k1.order

def _bip32_master(seed: bytes) -> tuple[bytes, bytes]:
    """BIP-32 master key: HMAC-SHA512(key='Bitcoin seed', data=seed)."""
    I = hmac.new(b"Bitcoin seed", seed, hashlib.sha512).digest()
    return I[:32], I[32:]  # (secret, chain_code)

def _bip32_ckd(key: bytes, chain: bytes, index: int) -> tuple[bytes, bytes]:
    """Child key derivation (private -> private)."""
    if index >= 0x80000000:
        data = b"\x00" + key + struct.pack(">I", index)
    else:
        # Compressed pubkey for non-hardened
        sk = ECDSASigningKey.from_string(key, curve=SECP256k1)
        pt = sk.get_verifying_key().pubkey.point
        prefix = b"\x02" if pt.y() % 2 == 0 else b"\x03"
        data = prefix + pt.x().to_bytes(32, "big") + struct.pack(">I", index)

    I = hmac.new(chain, data, hashlib.sha512).digest()
    child = (int.from_bytes(I[:32], "big") + int.from_bytes(key, "big")) % SECP256K1_N
    return child.to_bytes(32, "big"), I[32:]

def bip32_derive(seed: bytes, path: str) -> bytes:
    """
    Derive private key for a BIP-32 path like 'm/44h/1237h/0h/0/0'.
    Matches: bitcoin::bip32::Xpriv::new_master + derive_priv
    """
    key, chain = _bip32_master(seed)
    for part in path.lstrip("m/").split("/"):
        hardened = part.endswith("'") or part.endswith("h")
        idx = int(part.rstrip("'h"))
        if hardened:
            idx += 0x80000000
        key, chain = _bip32_ckd(key, chain, idx)
    return key

# ── Bech32 encoding (NIP-19: npub / nsec) ───────────────────────────────

_BECH32 = "qpzry9x8gf2tvdw0s3jn54khce6mua7l"

def _bech32_polymod(values):
    GEN = [0x3B6A57B2, 0x26508E6D, 0x1EA119FA, 0x3D4233DD, 0x2A1462B3]
    chk = 1
    for v in values:
        b = chk >> 25
        chk = ((chk & 0x1FFFFFF) << 5) ^ v
        for i in range(5):
            chk ^= GEN[i] if ((b >> i) & 1) else 0
    return chk

def bech32_encode(hrp: str, data: bytes) -> str:
    """Bech32 encode (NIP-19 for npub1.../nsec1...)."""
    # Convert 8-bit to 5-bit
    acc, bits, vals = 0, 0, []
    for byte in data:
        acc = (acc << 8) | byte
        bits += 8
        while bits >= 5:
            bits -= 5
            vals.append((acc >> bits) & 31)
    if bits:
        vals.append((acc << (5 - bits)) & 31)
    # Checksum
    hrp_exp = [ord(c) >> 5 for c in hrp] + [0] + [ord(c) & 31 for c in hrp]
    polymod = _bech32_polymod(hrp_exp + vals + [0]*6) ^ 1
    checksum = [(polymod >> 5*(5-i)) & 31 for i in range(6)]
    return hrp + "1" + "".join(_BECH32[d] for d in vals + checksum)

# ── did:key encoding ────────────────────────────────────────────────────

_B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

def base58_encode(data: bytes) -> str:
    n = int.from_bytes(data, "big")
    result = ""
    while n > 0:
        n, r = divmod(n, 58)
        result = _B58[r] + result
    for b in data:
        if b == 0:
            result = "1" + result
        else:
            break
    return result

def to_did_key(ed25519_pub_32: bytes) -> str:
    """did:key:z<base58btc(0xed01 + pubkey)>  — W3C did:key method, Ed25519."""
    return "did:key:z" + base58_encode(b"\xed\x01" + ed25519_pub_32)

# ── Main ─────────────────────────────────────────────────────────────────

def main():
    mnemonic = os.environ.get("MNEMONIC", "").strip()
    if not mnemonic:
        print("Enter your 24-word mnemonic (space-separated):")
        mnemonic = input("> ").strip()

    words = mnemonic.split()
    if len(words) != 24:
        print(f"Error: expected 24 words, got {len(words)}", file=sys.stderr)
        sys.exit(1)

    seed = mnemonic_to_seed(mnemonic)

    W = 72
    print()
    print("=" * W)
    print("  ARCHIPELAGO SEED DERIVATION VERIFICATION")
    print("=" * W)
    print()
    print(f"  Seed fingerprint (SHA-256):  {hashlib.sha256(seed).hexdigest()[:16]}...")
    print(f"  Seed length:                 {len(seed)} bytes")

    # ── 1. Node Ed25519 + DID ────────────────────────────────────────────

    print()
    print("-" * W)
    print("  1. NODE ED25519 KEY")
    print(f"     HKDF-SHA256(seed, info='archipelago/node/ed25519/v1')")
    print("-" * W)

    node_ed_priv, node_ed_pub = ed25519_keypair(
        hkdf_sha256(seed, b"archipelago/node/ed25519/v1")
    )
    node_did = to_did_key(node_ed_pub)

    print(f"  Private:  {node_ed_priv.hex()}")
    print(f"  Public:   {node_ed_pub.hex()}")
    print(f"  did:key:  {node_did}")

    # ── 2. Node Nostr key ────────────────────────────────────────────────

    print()
    print("-" * W)
    print("  2. NODE NOSTR KEY")
    print(f"     HKDF-SHA256(seed, info='archipelago/nostr-node/secp256k1/v1')")
    print("-" * W)

    node_nostr_priv = hkdf_sha256(seed, b"archipelago/nostr-node/secp256k1/v1")
    node_nostr_pub = secp256k1_xonly(node_nostr_priv)

    print(f"  Private:  {node_nostr_priv.hex()}")
    print(f"  X-only:   {node_nostr_pub.hex()}")
    print(f"  nsec:     {bech32_encode('nsec', node_nostr_priv)}")
    print(f"  npub:     {bech32_encode('npub', node_nostr_pub)}")

    # ── 2b. FIPS mesh transport key ─────────────────────────────────────

    print()
    print("-" * W)
    print("  2b. FIPS MESH TRANSPORT KEY")
    print(f"     HKDF-SHA256(seed, info='archipelago/fips/secp256k1/v1')")
    print("-" * W)

    fips_priv = hkdf_sha256(seed, b"archipelago/fips/secp256k1/v1")
    fips_pub = secp256k1_xonly(fips_priv)

    print(f"  X-only:   {fips_pub.hex()}")
    print(f"  npub:     {bech32_encode('npub', fips_pub)}")

    # ── 3. Identity[0..2] Ed25519 + DID ─────────────────────────────────

    print()
    print("-" * W)
    print("  3. IDENTITY ED25519 KEYS + DID")
    print(f"     HKDF-SHA256(seed, info='archipelago/identity/{{i}}/ed25519/v1')")
    print("-" * W)

    for i in range(3):
        info = f"archipelago/identity/{i}/ed25519/v1".encode()
        priv, pub = ed25519_keypair(hkdf_sha256(seed, info))
        did = to_did_key(pub)
        print(f"  [{i}] Public:   {pub.hex()}")
        print(f"      did:key:  {did}")

    # ── 4. Identity[0..2] Nostr (NIP-06 BIP-32) ────────────────────────

    print()
    print("-" * W)
    print("  4. IDENTITY NOSTR KEYS (NIP-06)")
    print(f"     BIP-32  m/44'/1237'/0'/0/{{i}}")
    print("-" * W)

    for i in range(3):
        priv = bip32_derive(seed, f"m/44'/1237'/0'/0/{i}")
        pub = secp256k1_xonly(priv)
        print(f"  [{i}] X-only:   {pub.hex()}")
        print(f"      nsec:     {bech32_encode('nsec', priv)}")
        print(f"      npub:     {bech32_encode('npub', pub)}")

    # ── 5. Bitcoin BIP-84 ───────────────────────────────────────────────

    print()
    print("-" * W)
    print("  5. BITCOIN WALLET (BIP-84)")
    print(f"     BIP-32  m/84'/0'/0'")
    print("-" * W)

    btc_acct = bip32_derive(seed, "m/84'/0'/0'")
    btc_pub = secp256k1_xonly(btc_acct)
    print(f"  Account key:  {btc_acct.hex()}")
    print(f"  Account pub:  {btc_pub.hex()}")

    # ── 6. LND Entropy ──────────────────────────────────────────────────

    print()
    print("-" * W)
    print("  6. LND AEZEED ENTROPY")
    print(f"     HKDF-SHA256(seed, info='archipelago/lnd/entropy/v1')  [16 bytes]")
    print("-" * W)

    lnd = hkdf_sha256(seed, b"archipelago/lnd/entropy/v1", 16)
    print(f"  Entropy:  {lnd.hex()}")

    # ── Done ─────────────────────────────────────────────────────────────

    print()
    print("=" * W)
    print("  Compare these values with your Archipelago node:")
    print("    SSH: xxd -p /var/lib/archipelago/identity/node_key.pub   (section 1)")
    print("         cat /var/lib/archipelago/identity/nostr_pubkey      (section 2)")
    print("    RPC: curl -s -b jar.txt http://<ip>/rpc/v1 \\")
    print("           -H 'Content-Type: application/json' \\")
    print("           -d '{\"method\":\"node.did\"}' | jq .")
    print("         ...and {\"method\":\"node.nostr-pubkey\"} for the npub")
    print("=" * W)
    print()

if __name__ == "__main__":
    main()
```

---

## How to Run

```bash
# Install (two packages, pure crypto, no telemetry)
pip3 install cryptography ecdsa

# Option A: environment variable (doesn't persist in shell history)
read -rs MNEMONIC && export MNEMONIC
# (type or paste your 24 words, press Enter)
python3 verify-seed.py
unset MNEMONIC

# Option B: interactive prompt
python3 verify-seed.py
# Enter your 24-word mnemonic (space-separated):
# > abandon abandon ... art
```

---

## What to Compare

| Output field | Where to find on your node |
|---|---|
| Node Ed25519 public | `xxd -p /var/lib/archipelago/identity/node_key.pub` |
| Node did:key | Settings > Identity > Node DID |
| Node npub | Settings > Identity > Nostr Public Key |
| Identity[0] did:key | Settings > Identity > first identity DID |
| Identity[0] npub | Settings > Identity > first identity Nostr key |

RPC alternative (from any machine on the LAN):

```bash
# Node identity
curl -s http://archipelago.local/api/rpc \
  -H 'Content-Type: application/json' \
  -d '{"method":"identity.get-node"}' | jq .

# All identities
curl -s http://archipelago.local/api/rpc \
  -H 'Content-Type: application/json' \
  -d '{"method":"identity.list"}' | jq .
```

---

## Cryptographic Reference

### HKDF-SHA256 (RFC 5869)

Used for Ed25519 and node-level Nostr keys. Domain separation via unique `info` strings
prevents key reuse across contexts.

```
Extract:  PRK = HMAC-SHA256(salt=0x00*32, ikm=64_byte_seed)
Expand:   OKM = HMAC-SHA256(PRK, info || 0x01)  [first 32 bytes]
```

The Rust backend uses `hkdf::Hkdf::<Sha256>::new(None, ikm)` where `None` salt = 32 zero bytes.

### BIP-32 (secp256k1 HD derivation)

Used for per-identity Nostr keys (NIP-06) and Bitcoin wallet.

```
Master:   HMAC-SHA512(key="Bitcoin seed", data=64_byte_seed)
Child:    HMAC-SHA512(key=chain_code, data=0x00||key||index)  [hardened]
          HMAC-SHA512(key=chain_code, data=pubkey||index)     [normal]
```

The Rust backend uses the `bitcoin` crate: `Xpriv::new_master()` + `derive_priv()`.

### did:key (W3C)

```
did:key:z  +  base58btc( 0xED 0x01 || 32_byte_ed25519_pubkey )
```

Multicodec prefix `0xED 0x01` identifies Ed25519 public keys.
The Rust backend uses `bs58::encode()` over a 34-byte buffer.

### NIP-19 Bech32 (npub/nsec)

```
npub1...  =  bech32(hrp="npub", data=32_byte_x_only_pubkey)
nsec1...  =  bech32(hrp="nsec", data=32_byte_private_key)
```

X-only pubkey = just the x-coordinate of the secp256k1 point (Schnorr format).

---

## Security

- Run on an air-gapped machine or at minimum a private terminal session
- The script makes zero network calls and writes zero files
- After verification, clean up:
  ```bash
  rm verify-seed.py
  unset MNEMONIC
  history -c  # bash
  # or: fc -W /dev/null  # zsh
  ```
- Never paste your mnemonic into a web tool, online REPL, or shared terminal
