# reticulum-daemon

Host-supervised **Reticulum (RNS) + LXMF** bridge for Archipelago's Mesh tab. This is
the Python side of the Reticulum transport work:
archipelago spawns one of these per active Reticulum (RNode) radio, it owns the serial
port, and the Rust mesh subsystem drives it over a Unix-socket JSON-RPC.

Why a daemon (not the Rust `reticulum-rs` crate): the canonical Python `rns`/`lxmf`
guarantees interop with Sideband / NomadNet / MeshChat, and lets us derive the RNS
identity from the existing Archy key (proven in `spike_identity.py`).

## Layout
- `archy_rns_identity.py` — derive a **deterministic** RNS `Identity` from the 32-byte
  Archy Ed25519 seed (`identity_dir/node_key`) via domain-separated HKDF. The node's
  LXMF destination hash is a stable function of the Archy identity.
- `spike_identity.py` — **Phase-0 gate #1** (no radio): proves that determinism.
- `reticulum_daemon.py` — the daemon: RNS bring-up, LXMF router, announce handler, and
  the Unix-socket RPC. See its module docstring for the wire protocol.
- `requirements.txt` — pinned `rns==1.3.5`, `lxmf==1.0.1` (validated on Python 3.13).

## Dev setup
```sh
python3 -m venv .venv
.venv/bin/pip install -r requirements.txt
```

## Run the spike / smoke tests (no hardware)
```sh
.venv/bin/python spike_identity.py                     # gate #1: identity determinism
.venv/bin/python reticulum_daemon.py --check \
    --identity-key /path/to/node_key                   # print this node's dest hash
.venv/bin/python reticulum_daemon.py --selftest \
    --identity-key /path/to/node_key                   # bring up RNS+LXMF, no radio
```

## Run against a real RNode (Phase-0 hardware gate, on .116 / .228)
```sh
.venv/bin/python reticulum_daemon.py \
    --identity-key /var/lib/archipelago/identity/node_key \
    --serial-port /dev/reticulum-radio \
    --socket /run/archy/reticulum.sock \
    --display-name "archy-228"
```
Then verify a two-node LXMF DM over LoRa and interop with a stock Sideband/MeshChat
client (Phase-0 gates #2 and #3).

## Packaging (Phase 1)
Ship as a **PyInstaller single binary** in the OTA next to `/usr/local/bin/archipelago`
(no provision-time `pip install`). archipelago supervises it: start on RNode detect,
kill on unplug/disable. The RPC socket and RNS config dir are archipelago-owned, 0600.

```sh
./build.sh   # → dist/archy-reticulum-daemon (~16M, fully standalone)
```
`-d noarchive` is required, not optional — see the comment in `build.sh`: RNS computes
`RNS.Interfaces.__all__` via a `glob()` against its own `__file__` directory at import
time, which only works when PyInstaller keeps modules as loose files instead of zipping
them into the binary.

## Status
Phase-0 gate #1 (identity determinism) **passes**, verified in both the dev venv and the
packaged binary (same dest hash). The signed-identity announce (`ARCHY:2:{ed}:{x25519}` in
`_announce_app_data`, via `--archy-ed-pubkey-hex`/`--archy-x25519-pubkey-hex`) is wired and
the Rust side (`reticulum.rs`) already passes the node's real keys through. Packaging is
done and verified standalone. What's left is entirely hardware-dependent: the live LoRa
message path (Phase-0 gates #2/#3) needs a real RNode-flashed board.
