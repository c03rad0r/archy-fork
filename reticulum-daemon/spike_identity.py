#!/usr/bin/env python3
"""Phase 0 gate #1 — deterministic RNS identity from the Archy seed (NO radio needed).

Proves the load-bearing assumption behind the whole "derive RNS identity from Archy
keys" decision: a node's Reticulum/LXMF destination hash is a stable, reproducible
function of its 32-byte Archy Ed25519 seed.

Run:
    reticulum-daemon/.venv/bin/python reticulum-daemon/spike_identity.py

Exits non-zero (and prints FAIL) if any invariant breaks.
"""

from __future__ import annotations

import sys

from archy_rns_identity import lxmf_destination_hash, rns_private_blob

# Two fixed, non-secret test seeds (32 bytes each). Real seeds come from node_key.
SEED_A = bytes(range(32))
SEED_B = bytes((i * 7 + 3) & 0xFF for i in range(32))


def _hex(b: bytes) -> str:
    return b.hex()


def main() -> int:
    ok = True

    # 1. The 64-byte private blob is deterministic and well-formed.
    blob1 = rns_private_blob(SEED_A)
    blob2 = rns_private_blob(SEED_A)
    if blob1 != blob2 or len(blob1) != 64:
        print(f"FAIL: private blob not deterministic/64B (len={len(blob1)})")
        ok = False
    else:
        print(f"ok  : private blob deterministic, 64B  ({_hex(blob1)[:16]}…)")

    # 2. Same seed -> same LXMF destination hash, across two independent builds.
    h1 = lxmf_destination_hash(SEED_A)
    h2 = lxmf_destination_hash(SEED_A)
    if h1 != h2 or len(h1) != 16:
        print(f"FAIL: destination hash not stable/16B: {_hex(h1)} vs {_hex(h2)}")
        ok = False
    else:
        print(f"ok  : destination hash stable, 16B   <{_hex(h1)}>")

    # 3. Different seed -> different destination (no accidental collision/constant).
    h3 = lxmf_destination_hash(SEED_B)
    if h3 == h1:
        print(f"FAIL: distinct seeds produced the same destination <{_hex(h3)}>")
        ok = False
    else:
        print(f"ok  : distinct seed -> distinct dest  <{_hex(h3)}>")

    print("\nPASS — RNS identity is deterministic from the Archy seed."
          if ok else "\nFAILED — see above.")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
