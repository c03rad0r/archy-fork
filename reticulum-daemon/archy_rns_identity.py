"""Derive a stable Reticulum (RNS) Identity from the Archipelago node identity.

Archy's root identity is a single 32-byte Ed25519 seed (``identity_dir/node_key``,
0600 — see core/archipelago/src/identity.rs). A Reticulum ``Identity`` is a *pair*
of keypairs: an X25519 (encryption) key and an Ed25519 (signing) key, whose
concatenated 64-byte private blob is ``x25519_priv(32) || ed25519_priv(32)``.

We derive both halves deterministically from the Archy seed with domain-separated
HKDF-SHA256. Properties this gives us:

  * **Reproducible** — the same Archy node always produces the same RNS destination
    hash, so a contact's Reticulum address is stable across reboots / reinstalls and
    peers can bind it to the existing Archy contact (no manual re-pairing).
  * **Domain-separated** — we never reuse the raw Archy signing key inside RNS; each
    derived key has its own HKDF ``info`` label. Reusing one private key across two
    cryptographic schemes is a footgun we deliberately avoid.

Binding to the Archy DID/Npub is NOT done by key reuse — it is carried in the signed
announce app-data (see ``build_announce_app_data``), which peers verify against the
Archy ed25519 identity and then bind onto the contact's stable ``arch_pubkey_hex``.
"""

from __future__ import annotations

import hashlib
import hmac

# HKDF info labels — changing these changes every node's RNS address, so they are
# part of the wire contract. Do not edit without a migration.
_INFO_X25519 = b"archipelago/reticulum/x25519/v1"
_INFO_ED25519 = b"archipelago/reticulum/ed25519/v1"
_HKDF_SALT = b"archipelago-reticulum-identity-v1"


def _hkdf_sha256(ikm: bytes, info: bytes, length: int = 32) -> bytes:
    """RFC 5869 HKDF-SHA256 (extract + expand) for one output block (length <= 32)."""
    if length > 32:
        raise ValueError("this helper only emits up to one SHA-256 block")
    prk = hmac.new(_HKDF_SALT, ikm, hashlib.sha256).digest()  # extract
    okm = hmac.new(prk, info + b"\x01", hashlib.sha256).digest()  # expand (T(1))
    return okm[:length]


def rns_private_blob(archy_ed25519_seed: bytes) -> bytes:
    """Return the 64-byte RNS private blob (x25519_priv || ed25519_priv).

    ``archy_ed25519_seed`` is the raw 32 bytes of ``identity_dir/node_key``.
    """
    if len(archy_ed25519_seed) != 32:
        raise ValueError(f"expected a 32-byte Archy ed25519 seed, got {len(archy_ed25519_seed)}")
    x25519_priv = _hkdf_sha256(archy_ed25519_seed, _INFO_X25519, 32)
    ed25519_priv = _hkdf_sha256(archy_ed25519_seed, _INFO_ED25519, 32)
    return x25519_priv + ed25519_priv


def load_identity(archy_ed25519_seed: bytes):
    """Build an ``RNS.Identity`` deterministically from the Archy seed.

    Imported lazily so this module (and the determinism unit test) can be reasoned
    about without RNS installed; the daemon imports it after the venv is present.
    """
    import RNS  # noqa: PLC0415 — lazy by design

    blob = rns_private_blob(archy_ed25519_seed)
    identity = RNS.Identity(create_keys=False)
    identity.load_private_key(blob)
    return identity


def lxmf_destination_hash(archy_ed25519_seed: bytes) -> bytes:
    """The 16-byte LXMF *delivery* destination hash for this node's identity.

    Uses the static ``Destination.hash`` so we can derive the address without a
    running Reticulum/Transport instance (the daemon computes this at startup,
    before bringing interfaces up).
    """
    import RNS  # noqa: PLC0415

    identity = load_identity(archy_ed25519_seed)
    return RNS.Destination.hash(identity, "lxmf", "delivery")
