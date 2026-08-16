#!/usr/bin/env python3
"""Archipelago Reticulum daemon — host-supervised RNS + LXMF bridge.

archipelago spawns and supervises one of these per active Reticulum (RNode) radio.
It owns the serial port, runs the Reticulum stack + an LXMF router whose identity is
**derived deterministically from the Archy node key**, and exposes a tiny
line-delimited JSON-RPC over a Unix domain socket (0600) for the Rust side to drive.

Security posture (see the plan's "most secure way" section):
  * Runs as the same rootless archipelago user; no root, no network control plane.
  * RPC is a Unix socket only; the identity key never leaves the host and is never
    logged. The daemon executes ONLY the fixed verb set below.

RPC (one JSON object per line, both directions):
  in : {"cmd":"send","dest_hash":"<hex16>","content":"…","title":"…","method":"direct|opportunistic"}
       {"cmd":"announce"}
       {"cmd":"set_name","name":"…"}
       {"cmd":"status"}
       {"cmd":"send_resource","id":"<correlation>","dest_hash":"<hex16>","data_b64":"…"}
       {"cmd":"shutdown"}
  out: {"event":"ready","dest_hash":"<hex16>","display_name":"…"}
       {"event":"recv","source_hash":"<hex16>","content":"…","title":"…","fields":{…},"app_data":"<hex>","rssi":n,"snr":n,"stamp":t}
       {"event":"announce","dest_hash":"<hex16>","app_data":"<hex>","display_name":"…"|null,"archy_blob":"ARCHY:2:…"|null}
       {"event":"delivered","dest_hash":"<hex16>","state":"delivered|failed","id":"<hex>"}
       {"event":"status","connected":bool,"dest_hash":"<hex16>","interfaces":[…]}
       {"event":"resource_progress","id":"<correlation>","transferred":n,"total":n}
       {"event":"resource_sent","id":"<correlation>"}
       {"event":"resource_failed","id":"<correlation>","reason":"…"}
       {"event":"resource_recv","source_hash":"<hex16>","data_b64":"…"}

``send_resource`` is for large (>~2.3KB) binary payloads that don't fit the small
LXMF-message path — it uses RNS's native Resource transfer protocol over a `RNS.Link`
to the peer's *resource* destination (a separate aspect from the LXMF delivery
destination, so it doesn't disturb normal messaging). Built for sending compressed
photos/files/voice-messages directly over LoRa instead of always falling back to Tor
past the small-message size cap. ``resource_recv``'s `data_b64` is the same
CBOR-encoded payload format already used for the small-inline LoRa path, so the Rust
side decodes it identically regardless of which path delivered it.

This is the Phase-1 skeleton: the identity/LXMF wiring and RPC loop are real and
exercised by ``--check`` / ``--selftest`` (no radio). The live LoRa message path is
validated in the Phase-0 hardware gate on .116/.228.
"""

from __future__ import annotations

import argparse
import asyncio
import base64
import json
import os
import signal
import sys
from pathlib import Path

from archy_rns_identity import lxmf_destination_hash, load_identity

# Lazy heavy imports (RNS/LXMF) happen in run(); --check stays import-light.


# ─────────────────────────── RNS config generation ───────────────────────────

def _require_loopback(host: str) -> None:
    """Bind-scope guard: TCP server mode is dev/verification only for now. WAN/LAN
    exposure is a separate future decision needing its own security review — archy
    is Tor-first for inter-node traffic (see README/CLAUDE.md), and a plain-TCP
    Reticulum listener bound beyond loopback would bypass that entirely. Defense in
    depth: the Rust side enforces the same rule (reticulum.rs::is_loopback_host),
    but this daemon can also be invoked directly, so it re-checks here too."""
    if host not in ("127.0.0.1", "::1", "localhost"):
        raise SystemExit(
            f"--tcp-listen host must be loopback-only (got {host!r}); "
            "WAN/LAN bind is out of scope for this daemon build"
        )


def _write_rns_config(
    configdir: Path,
    *,
    serial_port: str | None,
    lora: dict,
    no_radio: bool,
    tcp_listen: str | None = None,
    tcp_connect: list[str] | None = None,
    enable_transport: bool = False,
) -> None:
    """Materialise an RNS config file. RNode interface for real radios; plain-TCP
    server/client interface(s) for radio-less dev/verification (e.g. Aurora
    interop testing); a loopback-only disabled config for --selftest so the stack
    comes up with zero interfaces at all."""
    configdir.mkdir(parents=True, exist_ok=True)
    cfg = configdir / "config"
    if no_radio:
        interfaces = (
            "  [[Default Interface]]\n"
            "    type = AutoInterface\n"
            "    enabled = no\n"
        )
    elif serial_port:
        interfaces = (
            "  [[RNode LoRa]]\n"
            "    type = RNodeInterface\n"
            "    enabled = yes\n"
            f"    port = {serial_port}\n"
            f"    frequency = {lora['frequency']}\n"
            f"    bandwidth = {lora['bandwidth']}\n"
            f"    txpower = {lora['txpower']}\n"
            f"    spreadingfactor = {lora['spreadingfactor']}\n"
            f"    codingrate = {lora['codingrate']}\n"
        )
        # Regulatory duty-cycle limits (percent). Only written when set —
        # absent keys keep RNS's own default (no software airtime lock),
        # matching every daemon built before these args existed.
        if lora.get("airtime_limit_short") is not None:
            interfaces += f"    airtime_limit_short = {lora['airtime_limit_short']}\n"
        if lora.get("airtime_limit_long") is not None:
            interfaces += f"    airtime_limit_long = {lora['airtime_limit_long']}\n"
    elif tcp_listen or tcp_connect:
        parts = []
        if tcp_listen:
            host, _, port = tcp_listen.rpartition(":")
            _require_loopback(host)
            parts.append(
                "  [[Reticulum TCP Server]]\n"
                "    type = TCPServerInterface\n"
                "    enabled = yes\n"
                f"    listen_ip = {host}\n"
                f"    listen_port = {port}\n"
            )
        for i, target in enumerate(tcp_connect or []):
            host, _, port = target.rpartition(":")
            parts.append(
                f"  [[Reticulum TCP Client {i}]]\n"
                "    type = TCPClientInterface\n"
                "    enabled = yes\n"
                f"    target_host = {host}\n"
                f"    target_port = {port}\n"
            )
        interfaces = "\n".join(parts)
    else:
        raise SystemExit(
            "no interface configured: need --serial-port, --tcp-listen/--tcp-connect, "
            "or --no-radio"
        )
    cfg.write_text(
        "[reticulum]\n"
        f"  enable_transport = {'yes' if enable_transport else 'no'}\n"
        "  share_instance = no\n"
        "  panic_on_interface_error = no\n\n"
        "[interfaces]\n" + interfaces
    )
    os.chmod(cfg, 0o600)


# ─────────────────────────────── the daemon ──────────────────────────────────

class ReticulumDaemon:
    def __init__(self, args):
        self.args = args
        self.seed = self._read_seed(Path(args.identity_key))
        self.loop = asyncio.new_event_loop()
        asyncio.set_event_loop(self.loop)
        self.clients: set[asyncio.StreamWriter] = set()
        self.reticulum = None
        self.router = None
        self.delivery_destination = None
        self.identity = None
        self.dest_hash_hex = lxmf_destination_hash(self.seed).hex()
        # Resource transfer (large binary payloads over a dedicated Link, separate
        # from LXMF delivery — see module docstring). Keyed by the peer's *resource*
        # destination hash (bytes), not their LXMF delivery hash.
        self.resource_destination = None
        self.links: dict[bytes, "RNS.Link"] = {}
        self.pending_resource_sends: dict[bytes, list[tuple[bytes, str]]] = {}

    @staticmethod
    def _read_seed(path: Path) -> bytes:
        seed = path.read_bytes()
        if len(seed) != 32:
            raise SystemExit(f"identity key {path} must be 32 bytes, got {len(seed)}")
        return seed

    # ---- RNS / LXMF bring-up ----
    def bring_up(self):
        import RNS
        import LXMF

        configdir = Path(self.args.rns_config)
        _write_rns_config(
            configdir,
            serial_port=self.args.serial_port,
            lora={
                "frequency": self.args.frequency,
                "bandwidth": self.args.bandwidth,
                "txpower": self.args.txpower,
                "spreadingfactor": self.args.spreadingfactor,
                "codingrate": self.args.codingrate,
                "airtime_limit_short": self.args.airtime_limit_short,
                "airtime_limit_long": self.args.airtime_limit_long,
            },
            no_radio=self.args.no_radio,
            tcp_listen=self.args.tcp_listen,
            tcp_connect=self.args.tcp_connect,
            enable_transport=self.args.enable_transport,
        )
        self.reticulum = RNS.Reticulum(configdir=str(configdir))
        self.identity = load_identity(self.seed)

        storagepath = str(configdir / "lxmf")
        Path(storagepath).mkdir(parents=True, exist_ok=True)
        self.router = LXMF.LXMRouter(identity=self.identity, storagepath=storagepath)
        self.delivery_destination = self.router.register_delivery_identity(
            self.identity, display_name=self.args.display_name
        )
        self.router.register_delivery_callback(self._on_lxmf_delivery)

        # Hear other LXMF nodes' announces so we can surface peers + bind contacts.
        RNS.Transport.register_announce_handler(_AnnounceHandler(self))

        assert self.delivery_destination.hash.hex() == self.dest_hash_hex, (
            "derived dest hash diverged from LXMF's — aspect/identity mismatch"
        )

        # Separate destination/aspect for inbound Resource transfers (large
        # binary payloads), so it never collides with LXMF delivery traffic.
        # Every peer running this same daemon code can derive our resource
        # destination hash from our (already-announced) identity via
        # RNS.Destination.hash(identity, "archy", "resource") — see
        # _resource_dest_hash_for, used on the sending side.
        self.resource_destination = RNS.Destination(
            self.identity, RNS.Destination.IN, RNS.Destination.SINGLE,
            "archy", "resource",
        )
        self.resource_destination.set_link_established_callback(
            self._on_resource_link_established
        )

    def announce(self):
        if self.delivery_destination is not None:
            self.delivery_destination.announce(app_data=self._announce_app_data())

    def _archy_identity_blob(self):
        """The ``ARCHY:2:{ed25519_hex}:{x25519_hex}`` identity string the Rust
        side parses (``protocol::parse_identity_broadcast``) and binds via
        ``handle_identity_received`` — so a Reticulum-carried identity merges
        into the SAME conversation as the meshcore/Meshtastic/federation twins
        of the same Archy node. The keys are the node's real Archipelago
        pubkeys (passed in by the Rust side) — NOT this daemon's
        internally-HKDF-derived RNS keys, which exist only to make the RNS
        destination hash deterministic. ``None`` when the pubkeys weren't
        supplied (dev/selftest run)."""
        if self.args.archy_ed_pubkey_hex and self.args.archy_x25519_pubkey_hex:
            return (
                f"ARCHY:2:{self.args.archy_ed_pubkey_hex}:"
                f"{self.args.archy_x25519_pubkey_hex}"
            ).encode("ascii")
        return None

    def _announce_app_data(self) -> bytes:
        """LXMF-standard announce app_data — msgpack ``[display_name,
        stamp_cost, supported_functionality]`` via the router, so Sideband/
        NomadNet/MeshChat (and upgraded archy nodes) all see our real display
        name — with the Archy identity blob appended as an EXTRA list element.
        Stock clients only read the elements they know ([0]/[1]), so the blob
        rides along invisibly instead of replacing the name the way the old
        blob-only app_data did (which left every archy node nameless on RNS).
        """
        import RNS.vendor.umsgpack as msgpack
        app_data = self.router.get_announce_app_data(self.delivery_destination.hash)
        blob = self._archy_identity_blob()
        if blob is None:
            return app_data
        try:
            peer_data = msgpack.unpackb(app_data)
            if not isinstance(peer_data, list):
                raise ValueError("unexpected announce app_data shape")
            peer_data.append(blob)
            return msgpack.packb(peer_data)
        except Exception:
            # Never let announce formatting kill announcing entirely — fall
            # back to the legacy blob-only format (identity binding > name).
            return blob

    # ---- RNS-thread callbacks → asyncio ----
    def _on_lxmf_delivery(self, message):
        import LXMF
        try:
            app_data = b""
            src = message.source_hash.hex() if message.source_hash else ""
            event = {
                "event": "recv",
                "source_hash": src,
                "content": message.content_as_string() if hasattr(message, "content_as_string")
                           else (message.content.decode("utf-8", "replace") if message.content else ""),
                "title": message.title_as_string() if hasattr(message, "title_as_string") else "",
                "app_data": app_data.hex(),
                "stamp": getattr(message, "timestamp", None),
                # LXMF populates these from the delivery packet's phy stats on
                # direct RNode hops; they are None over TCP or multi-hop relay.
                # That None-vs-number difference is the RF/TCP discriminator
                # the UI needs (a TCP-fed mesh used to show rssi=0 and look
                # like working RF, 2026-08-16).
                "rssi": getattr(message, "rssi", None),
                "snr": getattr(message, "snr", None),
                "q": getattr(message, "q", None),
            }
            # Native LXMF attachment fields (Sideband/NomadNet/stock clients use
            # these, NOT our own typed-envelope wire format) — a stock client's
            # photo/voice-memo/file arrives here, not in `content`, which is why
            # it was previously dropped silently (content was just blank/space).
            # See LXMF field format confirmed against Sideband's own source
            # (sbapp/sideband/core.py): FIELD_IMAGE = [format_str, bytes],
            # FIELD_AUDIO = [mode_byte, bytes], FIELD_FILE_ATTACHMENTS =
            # [[filename, bytes], ...].
            fields = getattr(message, "fields", None) or {}
            if LXMF.FIELD_IMAGE in fields:
                fmt, img_bytes = fields[LXMF.FIELD_IMAGE]
                event["image_format"] = str(fmt)
                event["image_b64"] = base64.b64encode(bytes(img_bytes)).decode("ascii")
            if LXMF.FIELD_FILE_ATTACHMENTS in fields:
                attachments = fields[LXMF.FIELD_FILE_ATTACHMENTS]
                if attachments:
                    filename, file_bytes = attachments[0]
                    event["attachment_filename"] = str(filename)
                    event["attachment_b64"] = base64.b64encode(bytes(file_bytes)).decode("ascii")
            self._emit_threadsafe(event)
        except Exception as e:  # never let a callback kill the RNS thread
            self._emit_threadsafe({"event": "error", "where": "delivery", "detail": str(e)})

    def _emit_threadsafe(self, obj: dict):
        self.loop.call_soon_threadsafe(self._broadcast, obj)

    def _broadcast(self, obj: dict):
        line = (json.dumps(obj) + "\n").encode("utf-8")
        for w in list(self.clients):
            try:
                w.write(line)
            except Exception:
                self.clients.discard(w)

    # ---- RPC server ----
    async def _handle_client(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter):
        self.clients.add(writer)
        writer.write((json.dumps({
            "event": "ready", "dest_hash": self.dest_hash_hex,
            "display_name": self.args.display_name,
        }) + "\n").encode())
        try:
            async for raw in reader:
                line = raw.decode("utf-8", "replace").strip()
                if not line:
                    continue
                try:
                    req = json.loads(line)
                except json.JSONDecodeError:
                    continue
                await self._dispatch(req, writer)
        finally:
            self.clients.discard(writer)

    async def _dispatch(self, req: dict, writer: asyncio.StreamWriter):
        cmd = req.get("cmd")
        if cmd == "send":
            self._send(req)
        elif cmd == "announce":
            self.announce()
        elif cmd == "set_name":
            # Live rename: update the LXMF delivery destination's display name
            # (what get_announce_app_data reads) and re-announce immediately so
            # peers learn the new name without waiting for the next advert tick.
            name = (req.get("name") or "").strip()
            if name and self.delivery_destination is not None:
                self.args.display_name = name
                self.delivery_destination.display_name = name
                self.announce()
        elif cmd == "status":
            self._broadcast(self._status())
        elif cmd == "radio_state":
            self._broadcast(self._radio_state())
        elif cmd == "send_resource":
            self._send_resource(req)
        elif cmd == "shutdown":
            self.loop.stop()
        # unknown verbs are ignored by contract

    def _status(self) -> dict:
        ifaces = []
        if self.reticulum is not None:
            try:
                ifaces = [str(i) for i in self.reticulum.get_interface_stats().get("interfaces", [])]
            except Exception:
                pass
        return {"event": "status", "connected": self.router is not None,
                "dest_hash": self.dest_hash_hex, "interfaces": ifaces}

    def _radio_state(self) -> dict:
        """Radio-confirmed RNode parameters, straight from the live
        RNodeInterface object. The r_* attributes are what the RADIO reported
        after detect/configure (RNS/Interfaces/RNodeInterface.py) — this is
        the read-back the settings panel shows as proof the device is
        actually using the applied values, as opposed to what the config
        asked for. Absent radio (TCP/no-radio builds) → configured=False."""
        state = {"event": "radio_state", "configured": False, "online": False}
        try:
            import RNS
            for iface in list(RNS.Transport.interfaces):
                if type(iface).__name__ != "RNodeInterface":
                    continue
                state.update({
                    "configured": True,
                    "online": bool(getattr(iface, "online", False)),
                    "port": getattr(iface, "port", None),
                    # Requested (config) values…
                    "frequency": getattr(iface, "frequency", None),
                    "bandwidth": getattr(iface, "bandwidth", None),
                    "txpower": getattr(iface, "txpower", None),
                    "spreadingfactor": getattr(iface, "sf", None),
                    "codingrate": getattr(iface, "cr", None),
                    "airtime_limit_short": getattr(iface, "st_alock", None),
                    "airtime_limit_long": getattr(iface, "lt_alock", None),
                    # …and what the radio itself confirmed it is running.
                    "r_frequency": getattr(iface, "r_frequency", None),
                    "r_bandwidth": getattr(iface, "r_bandwidth", None),
                    "r_txpower": getattr(iface, "r_txpower", None),
                    "r_spreadingfactor": getattr(iface, "r_sf", None),
                    "r_codingrate": getattr(iface, "r_cr", None),
                    "r_airtime_limit_short": getattr(iface, "r_st_alock", None),
                    "r_airtime_limit_long": getattr(iface, "r_lt_alock", None),
                    # Live utilisation, when the interface tracks it.
                    "airtime_short": getattr(iface, "airtime_short", None),
                    "airtime_long": getattr(iface, "airtime_long", None),
                })
                break
        except Exception:
            pass
        return state

    def _send(self, req: dict):
        import RNS
        import LXMF
        try:
            dest_hash = bytes.fromhex(req["dest_hash"])
        except (KeyError, ValueError):
            return
        recipient_identity = RNS.Identity.recall(dest_hash)
        if recipient_identity is None:
            # No path/identity yet — ask the network and drop this attempt; the Rust
            # side retries once the peer is reachable (mirrors LoRa "unreachable").
            RNS.Transport.request_path(dest_hash)
            self._broadcast({"event": "delivered", "dest_hash": req["dest_hash"],
                             "state": "failed", "id": "", "reason": "no_path"})
            return
        dest = RNS.Destination(recipient_identity, RNS.Destination.OUT,
                               RNS.Destination.SINGLE, "lxmf", "delivery")
        method = {"direct": LXMF.LXMessage.DIRECT,
                  "opportunistic": LXMF.LXMessage.OPPORTUNISTIC,
                  "propagated": LXMF.LXMessage.PROPAGATED}.get(
                      req.get("method", "direct"), LXMF.LXMessage.DIRECT)
        # Native LXMF FIELD_IMAGE — for a stock Sideband/NomadNet peer, which
        # has no idea how to decode our own typed-envelope wire format. Rust
        # only sets these two keys when the peer isn't an archy contact (see
        # `is_archy_peer` gating in typed_messages.rs); [format, bytes] is the
        # wire shape confirmed against Sideband's own source.
        fields = {}
        if req.get("image_b64") and req.get("image_format"):
            fields[LXMF.FIELD_IMAGE] = [req["image_format"], base64.b64decode(req["image_b64"])]
        msg = LXMF.LXMessage(dest, self.delivery_destination,
                             req.get("content", ""), req.get("title", ""),
                             desired_method=method, fields=fields or None)
        msg.register_delivery_callback(lambda m: self._emit_threadsafe(
            {"event": "delivered", "dest_hash": req["dest_hash"], "state": "delivered",
             "id": m.hash.hex() if m.hash else ""}))
        msg.register_failed_callback(lambda m: self._emit_threadsafe(
            {"event": "delivered", "dest_hash": req["dest_hash"], "state": "failed",
             "id": m.hash.hex() if m.hash else ""}))
        self.router.handle_outbound(msg)

    # ---- Resource transfer (large binary payloads over a dedicated Link) ----
    def _resource_dest_hash_for(self, lxmf_dest_hash: bytes):
        """Derive a peer's *resource* destination hash from their LXMF delivery
        hash. Requires having already recalled their Identity (e.g. via a prior
        `_send`/announce) — returns None if we haven't heard from them yet."""
        import RNS
        identity = RNS.Identity.recall(lxmf_dest_hash)
        if identity is None:
            return None
        return RNS.Destination.hash(identity, "archy", "resource")

    def _get_or_create_link(self, lxmf_dest_hash: bytes):
        import RNS
        identity = RNS.Identity.recall(lxmf_dest_hash)
        if identity is None:
            RNS.Transport.request_path(lxmf_dest_hash)
            return None, None
        resource_hash = RNS.Destination.hash(identity, "archy", "resource")
        link = self.links.get(resource_hash)
        if link is not None and link.status != RNS.Link.CLOSED:
            return link, resource_hash
        out_dest = RNS.Destination(
            identity, RNS.Destination.OUT, RNS.Destination.SINGLE,
            "archy", "resource",
        )
        # established_callback identifies us to the peer BEFORE any Resource
        # goes out: the receiving daemon attributes an inbound resource via
        # link.get_remote_identity() (see _on_resource_received), which is
        # None unless the initiator calls identify() — without this every
        # transfer arrives with an empty source_hash and the Rust side can't
        # route it to a contact (observed live 2026-07-28: 5KB image sent,
        # concluded COMPLETE, never surfaced on the receiving node).
        link = RNS.Link(
            out_dest,
            established_callback=lambda lk: self._on_out_link_established(
                resource_hash, lk
            ),
        )
        link.set_link_closed_callback(
            lambda lk: self.links.pop(resource_hash, None)
        )
        self.links[resource_hash] = link
        return link, resource_hash

    def _on_out_link_established(self, resource_hash: bytes, link):
        link.identify(self.identity)
        self._flush_pending_resource_sends(resource_hash, link)

    def _send_resource(self, req: dict):
        import RNS
        req_id = req.get("id", "")
        try:
            lxmf_dest_hash = bytes.fromhex(req["dest_hash"])
            data = base64.b64decode(req["data_b64"])
        except (KeyError, ValueError, TypeError):
            self._emit_threadsafe({"event": "resource_failed", "id": req_id,
                                   "reason": "bad_request"})
            return
        link, resource_hash = self._get_or_create_link(lxmf_dest_hash)
        if link is None:
            self._emit_threadsafe({"event": "resource_failed", "id": req_id,
                                   "reason": "no_path"})
            return
        self.pending_resource_sends.setdefault(resource_hash, [])
        if link.status == RNS.Link.ACTIVE:
            self._start_resource(link, data, req_id)
        else:
            # Link is establishing — queue; the creation-time established
            # callback (_on_out_link_established) identifies us then flushes
            # the queue. Re-check afterwards to close the race where the
            # link went ACTIVE between the status check and the append
            # (the callback fires once, on the RNS thread).
            self.pending_resource_sends[resource_hash].append((data, req_id))
            if link.status == RNS.Link.ACTIVE:
                self._flush_pending_resource_sends(resource_hash, link)

    def _flush_pending_resource_sends(self, resource_hash: bytes, link):
        import RNS
        pending = self.pending_resource_sends.pop(resource_hash, [])
        for data, req_id in pending:
            if link.status == RNS.Link.ACTIVE:
                self._start_resource(link, data, req_id)
            else:
                self._emit_threadsafe({"event": "resource_failed", "id": req_id,
                                       "reason": "link_failed"})

    def _start_resource(self, link, data: bytes, req_id: str):
        import RNS

        def on_progress(resource):
            total = resource.get_data_size() if hasattr(resource, "get_data_size") else len(data)
            transferred = int(total * resource.get_progress()) if hasattr(resource, "get_progress") else 0
            self._emit_threadsafe({"event": "resource_progress", "id": req_id,
                                   "transferred": transferred, "total": total})

        def on_concluded(resource):
            if resource.status == RNS.Resource.COMPLETE:
                self._emit_threadsafe({"event": "resource_sent", "id": req_id})
            else:
                self._emit_threadsafe({"event": "resource_failed", "id": req_id,
                                       "reason": "transfer_failed"})

        RNS.Resource(data, link, callback=on_concluded, progress_callback=on_progress)

    def _on_resource_link_established(self, link):
        import RNS
        link.set_resource_strategy(RNS.Link.ACCEPT_ALL)
        link.set_resource_concluded_callback(self._on_resource_received)

    def _on_resource_received(self, resource):
        import RNS
        try:
            if resource.status != RNS.Resource.COMPLETE:
                return
            identity = resource.link.get_remote_identity()
            # Report the peer's LXMF *delivery* hash (not the raw identity hash,
            # and not the resource-aspect hash) since that's what the Rust side's
            # contact table is keyed on for every other inbound event.
            source_hash = (
                RNS.Destination.hash(identity, "lxmf", "delivery").hex()
                if identity is not None else ""
            )
            # RNS hands a concluded Resource's data as a file-like object
            # (BufferedReader over the assembled stream), not bytes —
            # observed live 2026-07-28: b64encode raised "a bytes-like
            # object is required" and every received transfer was lost.
            data = resource.data
            if hasattr(data, "read"):
                data = data.read()
            self._emit_threadsafe({
                "event": "resource_recv",
                "source_hash": source_hash,
                "data_b64": base64.b64encode(data).decode("ascii"),
            })
        except Exception as e:  # never let a callback kill the RNS thread
            self._emit_threadsafe({"event": "error", "where": "resource_recv",
                                   "detail": str(e)})

    async def serve(self):
        sock_path = self.args.socket
        if os.path.exists(sock_path):
            os.unlink(sock_path)
        # limit: asyncio's default per-line StreamReader cap is 64KiB, but
        # send_resource requests carry the whole payload base64-encoded on one
        # JSON line — a ~48KB+ attachment overflows the default and the raised
        # LimitOverrunError tears down the RPC connection (the Rust side then
        # sees "reticulum-daemon is gone" and restarts the whole session).
        server = await asyncio.start_unix_server(
            self._handle_client, path=sock_path, limit=16 * 1024 * 1024
        )
        os.chmod(sock_path, 0o600)
        self.announce()
        async with server:
            await server.serve_forever()


class _AnnounceHandler:
    """Surfaces every heard LXMF delivery announce to the Rust side."""
    aspect_filter = "lxmf.delivery"

    def __init__(self, daemon: "ReticulumDaemon"):
        self.daemon = daemon
        self.receive_path_responses = True

    def received_announce(self, destination_hash, announced_identity, app_data,
                          announce_packet_hash=None):
        # The 4-arg signature makes RNS.Transport hand us the announce packet
        # hash (it dispatches by arity), which unlocks per-announce RSSI/SNR
        # via Reticulum's local packet-stat cache. The default keeps a
        # hypothetical 3-arg dispatch working.
        # Decode what we can here (both the LXMF-standard display name and our
        # appended ARCHY identity blob — see _announce_app_data) so the Rust
        # side gets clean typed fields instead of re-implementing msgpack.
        display_name = None
        archy_blob = None
        raw = app_data or b""
        try:
            import LXMF
            display_name = LXMF.display_name_from_app_data(raw)
            # A legacy blob-only announce is plain ascii, so LXMF's decoder
            # returns the whole ARCHY identity blob as a "name" — drop it.
            if display_name and display_name.startswith("ARCHY:"):
                display_name = None
        except Exception:
            display_name = None
        try:
            if raw[:1] and ((0x90 <= raw[0] <= 0x9F) or raw[0] == 0xDC):
                import RNS.vendor.umsgpack as msgpack
                peer_data = msgpack.unpackb(raw)
                if isinstance(peer_data, list):
                    for el in peer_data[3:]:
                        if isinstance(el, bytes) and el.startswith(b"ARCHY:"):
                            archy_blob = el.decode("ascii", "ignore")
                            break
            elif raw.startswith(b"ARCHY:"):
                # Legacy (pre-upgrade archy node): app_data IS the blob.
                archy_blob = raw.decode("ascii", "ignore")
        except Exception:
            archy_blob = None
        rssi = None
        snr = None
        try:
            if announce_packet_hash is not None and self.daemon.reticulum is not None:
                rssi = self.daemon.reticulum.get_packet_rssi(announce_packet_hash)
                snr = self.daemon.reticulum.get_packet_snr(announce_packet_hash)
        except Exception:
            rssi = None
            snr = None
        self.daemon._emit_threadsafe({
            "event": "announce",
            "dest_hash": destination_hash.hex(),
            "app_data": raw.hex(),
            "display_name": display_name,
            "archy_blob": archy_blob,
            # None over TCP interfaces / multi-hop; real numbers only on a
            # direct RNode reception — see the recv-event comment.
            "rssi": rssi,
            "snr": snr,
        })


# ─────────────────────────────── entrypoints ─────────────────────────────────

def _parse_args(argv):
    p = argparse.ArgumentParser(description="Archipelago Reticulum daemon")
    p.add_argument("--identity-key", required=True, help="path to Archy node_key (32-byte ed25519 seed)")
    p.add_argument("--socket", default="/tmp/archy-reticulum.sock", help="Unix RPC socket path")
    p.add_argument("--rns-config", default=str(Path.home() / ".archy-reticulum"), help="RNS config/storage dir")
    p.add_argument("--serial-port", help="RNode serial device, e.g. /dev/reticulum-radio")
    p.add_argument("--tcp-listen", default=None, metavar="HOST:PORT",
                    help="Bind a plain-TCP Reticulum server interface (dev/verification "
                         "only; loopback-only, e.g. 127.0.0.1:4242).")
    p.add_argument("--tcp-connect", action="append", default=None, metavar="HOST:PORT",
                    help="Dial a plain-TCP Reticulum client interface (repeatable).")
    p.add_argument("--display-name", default="Archy", help="LXMF display name")
    p.add_argument("--archy-ed-pubkey-hex", default=None,
                    help="Archy ed25519 pubkey hex (64 chars) — embedded in the announce "
                         "app_data as ARCHY:2:... so peers bind this RNS destination onto "
                         "the existing Archy contact. Omit for a plain display-name announce.")
    p.add_argument("--archy-x25519-pubkey-hex", default=None,
                    help="Archy x25519 pubkey hex (64 chars), paired with --archy-ed-pubkey-hex.")
    # LoRa profile (defaults are EU_868-ish; settled by the Phase-0 spike)
    p.add_argument("--frequency", type=int, default=869525000)
    p.add_argument("--bandwidth", type=int, default=125000)
    p.add_argument("--txpower", type=int, default=17)
    p.add_argument("--spreadingfactor", type=int, default=8)
    p.add_argument("--codingrate", type=int, default=5)
    # Regulatory duty-cycle locks (percent of airtime, e.g. EU868 short=25
    # long=10). None (the default) writes no config line, so RNS applies no
    # software airtime lock — identical to daemons built before these existed.
    p.add_argument("--airtime-limit-short", type=float, default=None)
    p.add_argument("--airtime-limit-long", type=float, default=None)
    p.add_argument("--enable-transport", action="store_true",
                   help="run as an RNS transport node: relay traffic and rebroadcast "
                        "announces so nodes beyond direct RF range discover each other "
                        "through this one")
    p.add_argument("--no-radio", action="store_true", help="bring up with no RNode (selftest)")
    p.add_argument("--check", action="store_true", help="print derived dest hash and exit (no RNS)")
    p.add_argument("--selftest", action="store_true", help="bring up RNS+LXMF with no radio, verify, exit")
    return p.parse_args(argv)


def _install_parent_death_signal() -> None:
    """Die when our parent process does.

    The daemon ships as a PyInstaller one-file binary: our direct parent is the
    bootloader, and the Rust supervisor (mesh/reticulum.rs) stops us by SIGKILL-
    ing that bootloader. SIGKILL can't be forwarded, so without this the Python
    child is orphaned and keeps holding the RNode serial port — which piles up
    stale daemons that jam the radio (observed: 9 instances on one node). Asking
    the kernel to send us SIGTERM on parent death lets our existing SIGTERM
    handler shut down cleanly and free the port. Linux-only; no-op elsewhere.
    """
    if sys.platform != "linux":
        return
    try:
        import ctypes

        PR_SET_PDEATHSIG = 1
        ctypes.CDLL("libc.so.6", use_errno=True).prctl(
            PR_SET_PDEATHSIG, signal.SIGTERM
        )
    except Exception:
        pass  # best-effort; never block startup on this


def main(argv=None) -> int:
    _install_parent_death_signal()
    args = _parse_args(argv if argv is not None else sys.argv[1:])

    if args.serial_port and (args.tcp_listen or args.tcp_connect):
        raise SystemExit(
            "--serial-port is mutually exclusive with --tcp-listen/--tcp-connect "
            "(one daemon process = one interface)"
        )

    if args.check:
        seed = ReticulumDaemon._read_seed(Path(args.identity_key))
        print(lxmf_destination_hash(seed).hex())
        return 0

    daemon = ReticulumDaemon(args)

    if args.selftest:
        args.no_radio = True
        daemon.bring_up()
        # Announce app_data round-trip: the LXMF-standard msgpack name must be
        # decodable by stock clients AND (with archy keys set) the appended
        # identity blob must survive as an extra list element — this is the
        # exact wire contract the Rust announce handler and Sideband both
        # depend on, so verify it here where there's a real router to build it.
        import LXMF as _LXMF
        import RNS.vendor.umsgpack as _msgpack
        args.archy_ed_pubkey_hex = args.archy_ed_pubkey_hex or "ab" * 32
        args.archy_x25519_pubkey_hex = args.archy_x25519_pubkey_hex or "cd" * 32
        app_data = daemon._announce_app_data()
        decoded_name = _LXMF.display_name_from_app_data(app_data)
        assert decoded_name == args.display_name, (
            f"announce name round-trip failed: {decoded_name!r} != {args.display_name!r}"
        )
        blob_elems = [e for e in _msgpack.unpackb(app_data)[3:]
                      if isinstance(e, bytes) and e.startswith(b"ARCHY:")]
        assert blob_elems, "identity blob missing from announce app_data"
        # Live rename: set_name must change what the next announce carries.
        daemon.delivery_destination.display_name = "selftest-renamed"
        renamed = _LXMF.display_name_from_app_data(daemon._announce_app_data())
        assert renamed == "selftest-renamed", f"rename round-trip failed: {renamed!r}"
        print(f"selftest ok — dest_hash={daemon.dest_hash_hex} "
              f"display_name={args.display_name!r} lxmf_router=up "
              f"announce_app_data=verified set_name=verified")
        return 0

    for sig in (signal.SIGINT, signal.SIGTERM):
        daemon.loop.add_signal_handler(sig, daemon.loop.stop)
    try:
        daemon.bring_up()
        daemon.loop.run_until_complete(daemon.serve())
    except RuntimeError:
        pass  # loop.stop() during serve_forever
    finally:
        if os.path.exists(args.socket):
            os.unlink(args.socket)
    return 0


if __name__ == "__main__":
    sys.exit(main())
