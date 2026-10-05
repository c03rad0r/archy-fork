// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Reticulum (RNS + LXMF) bridge.
//!
//! Unlike Meshcore/Meshtastic — simple framed-serial protocols driven entirely
//! in-process — Reticulum is a full network stack (identity, announce, multi-hop
//! routing, LXMF store-and-forward) that we run as a **host-supervised Python
//! daemon** (`reticulum-daemon/`, canonical `rns`+`lxmf`, chosen over the sub-1.0
//! Rust port for interop with Sideband/NomadNet/MeshChat — see the plan). This
//! module is the Rust-side half of that bridge: it owns the child process and
//! speaks the daemon's Unix-socket JSON-RPC, while presenting the same method
//! surface `MeshRadioDevice` (listener/session.rs) already calls on
//! `MeshcoreDevice`/`MeshtasticDevice`.
//!
//! Two contract details that are easy to get wrong (see the plan §2b/§2d):
//!   1. The wrapper's `send_text_msg` is handed only a 6-byte prefix, but an RNS
//!      destination is 16 bytes — `prefix_to_hash` below is the mandatory
//!      resolver, populated from announces/contacts.
//!   2. Inbound LXMF deliveries are translated into the exact same synthetic
//!      `InboundFrame` byte layout Meshtastic already produces
//!      (`RESP_CONTACT_MSG_V3[_E2E]`), so `frames::handle_frame` needs zero
//!      changes to route them.

use super::message_types::{self, ContentInlinePayload, MeshMessageType, TypedEnvelope};
use super::protocol::{self, InboundFrame, ParsedContact};
use super::types::DeviceInfo;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tracing::{debug, info, warn};

/// RNode KISS protocol bytes, verified against the canonical Reticulum source
/// (`RNS/Interfaces/RNodeInterface.py`, `detect()`/`readLoop()`) — NOT guessed.
/// See `docs/RETICULUM-TRANSPORT-PROGRESS.md` for the citation.
const KISS_FEND: u8 = 0xC0;
const KISS_CMD_DETECT: u8 = 0x08;
const KISS_DETECT_REQ: u8 = 0x73;
const KISS_DETECT_RESP: u8 = 0x46;
const KISS_CMD_FW_VERSION: u8 = 0x50;
const KISS_CMD_PLATFORM: u8 = 0x48;
const KISS_CMD_MCU: u8 = 0x49;

const PROBE_BAUD: u32 = 115200;
// 800ms was too tight for real hardware: confirmed on a genuine Heltec V4
// RNode (firmware 1.86, verified via `rnodeconf --info`) that prints extra
// boot/status chatter on the same serial line before answering KISS
// commands — DETECT_RESP measured arriving ~1.05s after the probe write in
// that case. 2.5s leaves comfortable margin without meaningfully slowing
// down detection of non-RNode devices (Meshcore/Meshtastic already budget
// ~5s each).
const PROBE_READ_TIMEOUT: Duration = Duration::from_millis(2500);

/// Prefix marking an LXMF `content` string as base64 of a raw binary
/// typed-envelope payload rather than literal text. LXMF `content` travels
/// as a JSON string over the daemon RPC bridge, so a binary CBOR envelope
/// sent through `send_text_msg` as-is (single-frame path — anything chunked
/// is already base64'd before it gets here) must be lossily reinterpreted as
/// UTF-8 or it corrupts the wire and surfaces as garbage text on receipt.
/// Real plain-text chat never starts with this (its first byte would have
/// to be the `TypedEnvelope` marker 0x02), so the check is unambiguous.
const RETICULUM_BINARY_CONTENT_MARKER: &str = "\u{0}b64:";

/// Path to the supervised daemon binary. In production this is the bundled
/// PyInstaller artifact shipped beside `/usr/local/bin/archipelago` (Phase 1
/// packaging); during development it can point at the venv's interpreter
/// invoking `reticulum_daemon.py` directly. Overridable for testing/packaging.
///
/// Which Reticulum interface the daemon should bring up. `Serial` is the
/// original (and only, until now) path — a physical RNode over a serial
/// port, gated behind `probe_rnode`'s KISS-detect handshake in `open()`.
/// `TcpServer`/`TcpClient` are the radio-less, dev/verification-only
/// addition (see `types::ReticulumTcpConfig`'s doc comment): they let the
/// daemon speak plain Reticulum TCP — the same transport Aurora's
/// `RnsTcpInterface`/`RnsTcpServerInterface` use by default — without any
/// physical hardware. `TcpServer` is hard-gated to loopback by
/// `open_tcp_server`; `TcpClient` (outbound dial) is unrestricted, the same
/// risk class as Aurora's own hub uplinks.
pub enum ReticulumInterface<'a> {
    Serial(&'a str),
    TcpServer(&'a str),
    TcpClient(&'a [String]),
}

impl ReticulumInterface<'_> {
    /// Human-readable label used for both the `device_path` status field and
    /// (sanitized) the per-instance RPC socket filename. For `Serial` this is
    /// exactly the old bare path — zero behavior change for the existing
    /// hardware flow.
    fn label(&self) -> String {
        match self {
            Self::Serial(path) => path.to_string(),
            Self::TcpServer(bind) => format!("tcp-server:{bind}"),
            Self::TcpClient(targets) => format!("tcp-client:{}", targets.join(",")),
        }
    }
}

/// Bind-scope guard for TCP server mode — see `ReticulumInterface` doc
/// comment. Mirrors `reticulum_daemon.py`'s `_require_loopback`; enforced
/// here too (defense in depth) since this is the gate an operator/caller
/// actually goes through in Rust.
fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "::1" | "localhost")
}

/// `archy_ed_pubkey_hex`/`archy_x25519_pubkey_hex` (when known) are embedded
/// by the daemon in its announce app_data as `ARCHY:2:{ed}:{x25519}` — the
/// SAME wire format meshcore/Meshtastic identity adverts use — so a
/// Reticulum-carried identity binds onto the existing Archy contact via the
/// existing `parse_identity_broadcast`/`handle_identity_received` path,
/// satisfying cross-protocol DM convergence with zero new Rust dispatch code.
/// Resolve the daemon invocation: packaged binary, else the dev venv script.
/// `(program, Some(script_arg))` for the venv fallback.
fn daemon_program() -> (String, Option<String>) {
    let bin = std::env::var("ARCHY_RETICULUM_DAEMON_BIN")
        .unwrap_or_else(|_| "/usr/local/bin/archy-reticulum-daemon".to_string());
    if Path::new(&bin).exists() {
        return (bin, None);
    }
    let py = std::env::var("ARCHY_RETICULUM_DAEMON_PY")
        .unwrap_or_else(|_| "reticulum-daemon/.venv/bin/python".to_string());
    let script = std::env::var("ARCHY_RETICULUM_DAEMON_SCRIPT")
        .unwrap_or_else(|_| "reticulum-daemon/reticulum_daemon.py".to_string());
    (py, Some(script))
}

/// Whether the installed daemon understands `--enable-transport`. OTA updates
/// ship the archipelago binary ahead of the packaged daemon tools, so a fleet
/// node can run a new binary against an old daemon — whose argparse EXITS on
/// an unknown flag, killing the mesh session on every spawn (live regression,
/// a test node on v1.7.117). Probe `--help` and only pass the flag when the
/// daemon advertises it; an old daemon then runs edge-only exactly as before.
async fn daemon_supports_enable_transport() -> bool {
    let (program, script) = daemon_program();
    let mut cmd = Command::new(&program);
    if let Some(script) = &script {
        cmd.arg(script);
    }
    cmd.arg("--help")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let out = tokio::time::timeout(Duration::from_secs(10), async { cmd.output().await }).await;
    match out {
        Ok(Ok(out)) => {
            let supported = String::from_utf8_lossy(&out.stdout).contains("--enable-transport");
            if !supported {
                warn!(
                    program,
                    "installed reticulum-daemon predates --enable-transport — running edge-only until the daemon tools update"
                );
            }
            supported
        }
        _ => {
            warn!(
                program,
                "reticulum-daemon --help probe failed — not passing --enable-transport"
            );
            false
        }
    }
}

fn daemon_command(
    socket_path: &Path,
    iface: &ReticulumInterface<'_>,
    identity_key: &Path,
    archy_ed_pubkey_hex: Option<&str>,
    archy_x25519_pubkey_hex: Option<&str>,
    display_name: Option<&str>,
    enable_transport: bool,
    rf: Option<&super::rnode_settings::RNodeRfSettings>,
) -> Command {
    let (program, script) = daemon_program();
    let mut cmd = Command::new(program);
    if let Some(script) = script {
        cmd.arg(script);
    }
    cmd.arg("--identity-key")
        .arg(identity_key)
        .arg("--socket")
        .arg(socket_path);
    match iface {
        ReticulumInterface::Serial(path) => {
            cmd.arg("--serial-port").arg(path);
            // Operator-editable RF parameters (.126 LoRa panel). Passed
            // explicitly on every spawn so the sidecar's argparse defaults
            // stop being the silent source of truth. `rf` is None only for
            // non-serial interfaces, where these have no meaning.
            if let Some(rf) = rf {
                cmd.arg("--frequency").arg(rf.frequency.to_string());
                cmd.arg("--bandwidth").arg(rf.bandwidth.to_string());
                cmd.arg("--txpower").arg(rf.txpower.to_string());
                cmd.arg("--spreadingfactor")
                    .arg(rf.spreading_factor.to_string());
                cmd.arg("--codingrate").arg(rf.coding_rate.to_string());
                if let Some(pct) = rf.airtime_limit_short {
                    cmd.arg("--airtime-limit-short").arg(pct.to_string());
                }
                if let Some(pct) = rf.airtime_limit_long {
                    cmd.arg("--airtime-limit-long").arg(pct.to_string());
                }
            }
        }
        ReticulumInterface::TcpServer(bind) => {
            cmd.arg("--tcp-listen").arg(bind);
        }
        ReticulumInterface::TcpClient(targets) => {
            for target in *targets {
                cmd.arg("--tcp-connect").arg(target);
            }
        }
    }
    // Archy nodes are RNS transport nodes: they relay traffic and rebroadcast
    // announces, so archy nodes (and Sideband/NomadNet peers) beyond direct RF
    // range discover and reach each other through any archy node in between.
    // Edge-only operation (`enable_transport = no`) left every node an island
    // limited to its own radio horizon. Gated on the installed daemon actually
    // supporting the flag — see `daemon_supports_enable_transport`.
    if enable_transport {
        cmd.arg("--enable-transport");
    }
    if let (Some(ed), Some(x)) = (archy_ed_pubkey_hex, archy_x25519_pubkey_hex) {
        cmd.arg("--archy-ed-pubkey-hex")
            .arg(ed)
            .arg("--archy-x25519-pubkey-hex")
            .arg(x);
    }
    // The RNS-visible display name (what Sideband/NomadNet/other archy nodes
    // show for us). Without this the daemon falls back to its argparse
    // default and every archy node announces the same anonymous name.
    if let Some(name) = display_name {
        let name = name.trim();
        if !name.is_empty() {
            cmd.arg("--display-name").arg(name);
        }
    }
    // Run the daemon as its own process-group leader. The packaged binary is
    // a PyInstaller one-file bootloader that forks the real Python process;
    // making it a group leader lets shutdown signal the WHOLE group so the
    // forked child can't be orphaned and keep holding the serial port.
    // Deliberately NOT `kill_on_drop`: that SIGKILLs the bootloader the instant
    // the `Child` drops, before it can delete its `_MEI*` extraction dir (48M
    // leaked into TMPDIR per daemon restart — filled .116's 12G /tmp tmpfs).
    // Shutdown goes through `terminate_group` instead, on every path.
    cmd.process_group(0)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd
}

/// Gracefully stop a daemon process group: SIGTERM now (the daemon's handler
/// releases the RNode + socket, and the PyInstaller bootloader gets to delete
/// its `_MEI*` extraction dir once the Python child exits), SIGKILL from a
/// detached backstop thread a few seconds later so a wedged daemon still can't
/// survive or keep holding the serial port.
fn terminate_group(child: &Child) {
    if let Some(pid) = child.id() {
        let pgid = -(pid as i32);
        unsafe {
            libc::kill(pgid, libc::SIGTERM);
        }
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(5));
            unsafe {
                libc::kill(pgid, libc::SIGKILL);
            }
        });
    }
}

/// Own the daemon during its asynchronous handshake too. Cancellation drops
/// the future without executing an error branch; a bare Child would survive
/// and keep the serial port open even though the listener had stopped.
struct StartingDaemon(Option<Child>);

impl Drop for StartingDaemon {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_ref() {
            terminate_group(child);
        }
    }
}

/// One peer learned via an RNS announce (LXMF delivery destination).
#[derive(Clone)]
struct ReticulumPeer {
    dest_hash: [u8; 16],
    display_name: String,
    /// Archy ed25519 identity hex, once carried in this peer's own announce
    /// app-data blob (`ARCHY:n:...`) — see `handle_event`'s "announce" arm.
    /// Unlike Meshcore/Meshtastic, this arrives in-band with the same event
    /// as the destination hash, so it can be bound directly onto this
    /// RNS-hash-keyed peer with no name-matching ambiguity (contrast
    /// `bind_federation_twins`, which those two transports rely on instead).
    arch_pubkey_hex: Option<String>,
    reachable: bool,
    /// Unix time of the last announce heard from this peer over the air.
    /// In-memory only (a persisted value would be stale by definition) —
    /// `0` after a restart until the peer re-announces.
    last_advert_at: u64,
    /// Signal stats of the last announce/message heard from this peer.
    /// `Some` only for direct RNode (RF) receptions — the sidecar reports
    /// `null` for TCP interfaces and multi-hop relays, which is exactly the
    /// RF-vs-internet discriminator the UI needs (a TCP-fed mesh used to
    /// surface every peer as rssi=0 and look like working RF, 2026-08-16).
    last_rssi: Option<i16>,
    last_snr: Option<f32>,
}

/// On-disk shape of `ReticulumPeer` — `[u8; 16]` can't be a JSON object key,
/// so this stores the dest hash as hex instead. Persisted so a peer's
/// prefix->dest-hash mapping and display name survive a restart instead of
/// requiring a fresh announce every time (both `prefix_to_hash` and `peers`
/// are otherwise pure in-memory state rebuilt empty on every reconnect).
#[derive(Serialize, Deserialize)]
struct PersistedReticulumPeer {
    dest_hash_hex: String,
    display_name: String,
    arch_pubkey_hex: Option<String>,
}

/// Bridge handle to one supervised `reticulum-daemon` instance, one per active
/// Reticulum (RNode) radio. Implements the same method shapes
/// `MeshRadioDevice` calls on `MeshcoreDevice`/`MeshtasticDevice`.
pub struct ReticulumLink {
    device_path: String,
    socket_path: std::path::PathBuf,
    child: Child,
    writer: tokio::net::unix::OwnedWriteHalf,
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    dest_hash: [u8; 16],
    display_name: Option<String>,
    /// Mandatory: the wrapper's `send_text_msg`/inbound frames only carry a
    /// 6-byte prefix, but an RNS destination is 16 bytes. Populated from
    /// announces and `get_contacts`.
    prefix_to_hash: HashMap<[u8; 6], [u8; 16]>,
    peers: HashMap<[u8; 16], ReticulumPeer>,
    /// Where `peers.json` lives (`{data_dir}/reticulum/peers.json`) — set once
    /// at spawn, used by `persist_peers`/`load_persisted_peers`.
    peers_file: std::path::PathBuf,
    inbound: std::collections::VecDeque<InboundFrame>,
    /// Monotonic correlation id for `send_resource` RPC calls — purely for
    /// matching `resource_progress`/`resource_sent`/`resource_failed` events
    /// back to a log line; sends are fire-and-forget (see `send_resource`).
    resource_id_counter: u64,
    /// Set when the daemon's RPC socket closes or its process exits. Once
    /// true, `try_recv_frame` returns an error so the session loop tears
    /// down and the outer reconnect loop respawns the daemon — without this
    /// a dead daemon was invisible until the 30-minute RX-stall watchdog.
    daemon_gone: bool,
    /// Latest `radio_state` event from the sidecar (the live RNodeInterface
    /// values, radio-confirmed `r_*` included). Refreshed by
    /// [`Self::query_radio_state`]; the .126 LoRa panel's read-back source.
    last_radio_state: Option<Value>,
}

impl ReticulumLink {
    /// Cheap probe: send the verified RNode KISS detect sequence over the raw
    /// serial port and look for `DETECT_RESP`, WITHOUT spawning the (heavy)
    /// daemon. Mirrors the open()/initialize() split Meshcore/Meshtastic use,
    /// so a non-RNode port is rejected fast and the daemon is only ever
    /// started against a port we've confirmed is an RNode.
    ///
    /// `data_dir` is the same archipelago data directory `NodeIdentity` was
    /// loaded from (`{data_dir}/identity/node_key`) — the daemon reads that
    /// key file directly to derive its RNS identity (we pass the path, not
    /// the key bytes, so it never travels through more hops than necessary).
    ///
    /// `our_ed_pubkey_hex`/`our_x25519_pubkey_hex` are this node's real Archy
    /// identity pubkeys (already computed by the caller — same values passed
    /// to `run_mesh_session`); forwarded to the daemon so its announces carry
    /// a peer-bindable `ARCHY:2:...` identity. Pass `None` to announce with
    /// just the plain display name (e.g. a non-archy/dev run).
    pub async fn open(
        path: &str,
        data_dir: &Path,
        our_ed_pubkey_hex: Option<&str>,
        our_x25519_pubkey_hex: Option<&str>,
        display_name: Option<&str>,
    ) -> Result<Self> {
        let rf = super::rnode_settings::RNodeRfSettings::load(data_dir).await;
        if !rf.enabled {
            anyhow::bail!(
                "RNode interface is disabled in the LoRa settings — enable it to connect"
            );
        }
        // Operator port override wins over the auto-detected path (.126 LoRa
        // panel). The probe below still gates: a wrong override fails with
        // the detect error instead of a silent dead transport.
        let path = rf.port.as_deref().unwrap_or(path);
        probe_rnode(path)
            .await
            .context("RNode KISS detect failed")?;
        Self::spawn(
            ReticulumInterface::Serial(path),
            data_dir,
            our_ed_pubkey_hex,
            our_x25519_pubkey_hex,
            display_name,
        )
        .await
    }

    /// Bring up a daemon in plain-TCP server mode — no physical RNode, no
    /// `probe_rnode` gate. `bind` (`host:port`) must be loopback; see
    /// `ReticulumInterface`/`is_loopback_host`.
    pub async fn open_tcp_server(
        bind: &str,
        data_dir: &Path,
        our_ed_pubkey_hex: Option<&str>,
        our_x25519_pubkey_hex: Option<&str>,
        display_name: Option<&str>,
    ) -> Result<Self> {
        let host = bind.rsplit_once(':').map(|(h, _)| h).unwrap_or(bind);
        anyhow::ensure!(
            is_loopback_host(host),
            "reticulum TCP server bind must be loopback-only (127.0.0.1/::1/localhost) — \
             got {bind}; WAN/LAN exposure needs its own security review"
        );
        Self::spawn(
            ReticulumInterface::TcpServer(bind),
            data_dir,
            our_ed_pubkey_hex,
            our_x25519_pubkey_hex,
            display_name,
        )
        .await
    }

    /// Bring up a daemon in plain-TCP client mode, dialing one or more
    /// `host:port` targets — no physical RNode, no `probe_rnode` gate.
    pub async fn open_tcp_client(
        targets: &[String],
        data_dir: &Path,
        our_ed_pubkey_hex: Option<&str>,
        our_x25519_pubkey_hex: Option<&str>,
        display_name: Option<&str>,
    ) -> Result<Self> {
        anyhow::ensure!(
            !targets.is_empty(),
            "reticulum TCP client mode needs at least one target"
        );
        Self::spawn(
            ReticulumInterface::TcpClient(targets),
            data_dir,
            our_ed_pubkey_hex,
            our_x25519_pubkey_hex,
            display_name,
        )
        .await
    }

    async fn spawn(
        iface: ReticulumInterface<'_>,
        data_dir: &Path,
        our_ed_pubkey_hex: Option<&str>,
        our_x25519_pubkey_hex: Option<&str>,
        display_name: Option<&str>,
    ) -> Result<Self> {
        // Keep the RPC socket under the archipelago-owned data dir (not the
        // shared system temp dir) so its access is bounded by the same
        // permissions as the rest of our state — consistent with the
        // "archipelago-owned runtime dir, 0600" security posture.
        let runtime_dir = data_dir.join("reticulum");
        tokio::fs::create_dir_all(&runtime_dir)
            .await
            .context("Failed to create reticulum runtime dir")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                tokio::fs::set_permissions(&runtime_dir, std::fs::Permissions::from_mode(0o700))
                    .await;
        }
        let label = iface.label();
        let iface_key = label.replace(['/', ' ', ':', ','], "_");
        let socket_path = runtime_dir.join(format!("{iface_key}.sock"));
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }
        // Private TMPDIR for the PyInstaller one-file bootloader, wiped on every
        // (re)spawn: even a hard-killed or power-lost daemon can never accumulate
        // stale 48M `_MEI*` extraction dirs, and they land under the data dir
        // instead of the small tmpfs /tmp. Per-interface so the live daemons of
        // two radios never share a dir (the previous daemon for the SAME
        // interface is guaranteed dead before we respawn it).
        let tmp_dir = runtime_dir.join("tmp").join(&iface_key);
        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
        tokio::fs::create_dir_all(&tmp_dir)
            .await
            .context("Failed to create reticulum daemon tmp dir")?;
        let identity_key = data_dir.join("identity").join("node_key");
        if !identity_key.exists() {
            anyhow::bail!(
                "Archy identity key not found at {} — cannot derive a Reticulum identity",
                identity_key.display()
            );
        }

        let enable_transport = daemon_supports_enable_transport().await;
        // Operator RF settings ride every serial spawn; loaded here (not by
        // callers) so a settings apply only needs a transport restart to take
        // effect. Non-serial interfaces carry no RF.
        let rf = match iface {
            ReticulumInterface::Serial(_) => {
                Some(super::rnode_settings::RNodeRfSettings::load(data_dir).await)
            }
            _ => None,
        };
        let mut cmd = daemon_command(
            &socket_path,
            &iface,
            &identity_key,
            our_ed_pubkey_hex,
            our_x25519_pubkey_hex,
            display_name,
            enable_transport,
            rf.as_ref(),
        );
        cmd.env("TMPDIR", &tmp_dir);
        let child = cmd
            .spawn()
            .context("Failed to spawn reticulum-daemon — is it installed/packaged?")?;
        let mut starting = StartingDaemon(Some(child));

        // Wait for the socket to appear, then for the daemon's "ready" event.
        // StartingDaemon tears the group down on errors AND cancellation.
        let init = async {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
            let stream = loop {
                if tokio::time::Instant::now() > deadline {
                    anyhow::bail!("reticulum-daemon did not create its RPC socket in time");
                }
                match UnixStream::connect(&socket_path).await {
                    Ok(s) => break s,
                    Err(_) => tokio::time::sleep(Duration::from_millis(150)).await,
                }
            };
            let (read_half, write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);

            let mut line = String::new();
            tokio::time::timeout(Duration::from_secs(10), reader.read_line(&mut line))
                .await
                .context("Timed out waiting for reticulum-daemon ready event")?
                .context("reticulum-daemon RPC connection closed before ready")?;
            let ready: Value = serde_json::from_str(line.trim())
                .context("reticulum-daemon sent a non-JSON ready line")?;
            if ready.get("event").and_then(Value::as_str) != Some("ready") {
                anyhow::bail!("reticulum-daemon's first message was not 'ready': {ready}");
            }
            let dest_hash_hex = ready
                .get("dest_hash")
                .and_then(Value::as_str)
                .context("ready event missing dest_hash")?;
            let dest_hash = parse_hash16(dest_hash_hex)?;
            let display_name = ready
                .get("display_name")
                .and_then(Value::as_str)
                .map(str::to_string);

            info!(
                iface = %label,
                dest_hash = %dest_hash_hex,
                "Reticulum daemon ready"
            );
            Ok::<_, anyhow::Error>((write_half, reader, dest_hash, display_name))
        };
        let (write_half, reader, dest_hash, display_name) = init.await?;

        let mut link = Self {
            device_path: label,
            socket_path,
            child: starting
                .0
                .take()
                .expect("starting daemon is owned until ready"),
            writer: write_half,
            reader,
            dest_hash,
            display_name,
            prefix_to_hash: HashMap::new(),
            peers: HashMap::new(),
            peers_file: runtime_dir.join("peers.json"),
            inbound: std::collections::VecDeque::new(),
            resource_id_counter: 0,
            daemon_gone: false,
            last_radio_state: None,
        };
        link.load_persisted_peers();
        Ok(link)
    }

    /// Repopulate `peers`/`prefix_to_hash` from disk so a restart doesn't
    /// force every peer back to "Anonymous Peer" / unreachable until they
    /// happen to re-announce or message us again.
    fn load_persisted_peers(&mut self) {
        let Ok(bytes) = std::fs::read(&self.peers_file) else {
            return;
        };
        let Ok(persisted) = serde_json::from_slice::<Vec<PersistedReticulumPeer>>(&bytes) else {
            warn!(path = %self.peers_file.display(), "Failed to parse persisted Reticulum peers");
            return;
        };
        for p in persisted {
            let Ok(hash) = parse_hash16(&p.dest_hash_hex) else {
                continue;
            };
            let prefix: [u8; 6] = hash[..6].try_into().unwrap();
            self.prefix_to_hash.insert(prefix, hash);
            // Heal names persisted by pre-2026-07-28 builds, which could
            // store a raw `ARCHY:…` identity blob as the display name (seen
            // live on a test node). Blob-only announces assert no name, so
            // nothing would ever overwrite it — swap in the placeholder.
            let display_name = if p.display_name.starts_with("ARCHY:") {
                format!("Reticulum {}", hex::encode(&hash[..4]))
            } else {
                p.display_name
            };
            self.peers.insert(
                hash,
                ReticulumPeer {
                    dest_hash: hash,
                    display_name,
                    arch_pubkey_hex: p.arch_pubkey_hex,
                    // Reachability is a live property, not a persisted fact —
                    // start conservative and let the first real event refresh it.
                    reachable: false,
                    last_advert_at: 0,
                    last_rssi: None,
                    last_snr: None,
                },
            );
        }
        info!(count = self.peers.len(), "Loaded persisted Reticulum peers");
    }

    /// Resolve a caller-supplied 6-byte routing prefix to a full 16-byte RNS
    /// destination hash. Two shapes arrive here depending on how the contact
    /// record was born (observed live 2026-07-28, image-to-merged-contact):
    ///   1. the peer's RNS dest-hash prefix (contacts created from an RNS
    ///      announce) — direct `prefix_to_hash` hit;
    ///   2. the peer's Archipelago ed25519 pubkey prefix (radio twins bound
    ///      via the ARCHY announce blob store the arch key as `pubkey_hex`) —
    ///      no dest-hash match possible, so fall back to scanning peers whose
    ///      announce-bound `arch_pubkey_hex` starts with the prefix.
    fn resolve_dest_hash(&self, prefix: &[u8; 6]) -> Option<[u8; 16]> {
        if let Some(hash) = self.prefix_to_hash.get(prefix) {
            return Some(*hash);
        }
        let hex_prefix = hex::encode(prefix);
        self.peers.values().find_map(|p| {
            p.arch_pubkey_hex
                .as_deref()
                .filter(|arch| arch.starts_with(&hex_prefix))
                .map(|_| p.dest_hash)
        })
    }

    /// Best-effort sync write of the current peer table — called after any
    /// insert that adds/renames a peer. Infrequent (announces/first-contact,
    /// not per-message) so a blocking write here is a fine trade for keeping
    /// this out of the async runtime's plumbing.
    fn persist_peers(&self) {
        let persisted: Vec<PersistedReticulumPeer> = self
            .peers
            .values()
            .map(|p| PersistedReticulumPeer {
                dest_hash_hex: hex::encode(p.dest_hash),
                display_name: p.display_name.clone(),
                arch_pubkey_hex: p.arch_pubkey_hex.clone(),
            })
            .collect();
        match serde_json::to_vec(&persisted) {
            Ok(bytes) => {
                if let Err(e) = std::fs::write(&self.peers_file, bytes) {
                    warn!(path = %self.peers_file.display(), "Failed to persist Reticulum peers: {}", e);
                }
            }
            Err(e) => warn!("Failed to serialize Reticulum peers: {}", e),
        }
    }

    fn next_resource_id(&mut self) -> String {
        self.resource_id_counter += 1;
        self.resource_id_counter.to_string()
    }

    /// Handshake is a no-op here — `open()` already waited for the daemon's
    /// `ready` event, so by the time `MeshRadioDevice` calls `initialize()`
    /// the daemon (and the RNS/LXMF stack inside it) is already up.
    pub async fn initialize(&mut self) -> Result<DeviceInfo> {
        Ok(DeviceInfo {
            firmware_version: "reticulum-daemon".to_string(),
            node_id: reticulum_contact_id_from_hash(&self.dest_hash),
            max_contacts: u16::MAX,
            device_type: super::types::DeviceType::Reticulum,
        })
    }

    pub fn advert_name(&self) -> Option<String> {
        self.display_name.clone()
    }

    pub async fn set_advert_name(&mut self, name: &str) -> Result<()> {
        // Live rename: the daemon's `set_name` verb updates the LXMF delivery
        // destination's display_name and re-announces, so peers pick the new
        // name up on their next announce receipt. Also tracked locally so
        // `advert_name()` reflects it immediately.
        self.send_rpc(serde_json::json!({"cmd": "set_name", "name": name}))
            .await?;
        self.display_name = Some(name.to_string());
        Ok(())
    }

    pub async fn send_self_advert(&mut self) -> Result<()> {
        self.send_rpc(serde_json::json!({"cmd": "announce"})).await
    }

    /// Reticulum/LXMF has no shared-PSK broadcast channel like Meshcore's
    /// channel 0 or Meshtastic's primary channel — it's point-to-point +
    /// propagation-node store-and-forward. Treat as unsupported-but-harmless
    /// (no-op success) rather than failing the caller, matching the no-op
    /// pattern used for other not-applicable operations on this enum arm.
    /// Per-network channel support is tracked in the plan's Phase 4.
    pub async fn send_channel_text(&mut self, _channel: u8, _payload: &[u8]) -> Result<()> {
        debug!("Reticulum has no broadcast-channel concept — ignoring channel send");
        Ok(())
    }

    pub async fn send_text_msg(
        &mut self,
        dest_pubkey_prefix: &[u8; 6],
        payload: &[u8],
    ) -> Result<()> {
        let dest_hash = self
            .resolve_dest_hash(dest_pubkey_prefix)
            .with_context(|| {
                format!(
                    "Unknown Reticulum prefix {} — peer hasn't announced yet",
                    hex::encode(dest_pubkey_prefix)
                )
            })?;
        // Typed-envelope payloads (ReadReceipt/Reaction/etc. — anything small
        // enough for the single-frame path) are raw binary CBOR, not text.
        // `from_utf8_lossy` would irreversibly mangle them since `content`
        // round-trips as a JSON string; base64 instead so receive can recover
        // the exact original bytes.
        use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
        let content = if TypedEnvelope::is_typed(payload) {
            format!("{RETICULUM_BINARY_CONTENT_MARKER}{}", B64.encode(payload))
        } else {
            String::from_utf8_lossy(payload).into_owned()
        };
        self.send_rpc(serde_json::json!({
            "cmd": "send",
            "dest_hash": hex::encode(dest_hash),
            "content": content,
            "method": "direct",
        }))
        .await
    }

    /// Send an image to a peer via LXMF's native `FIELD_IMAGE`, instead of our
    /// own typed-envelope wire format — for a stock Sideband/NomadNet peer
    /// (not an archy contact), which has no way to decode our CBOR envelope.
    /// Caller (the RPC layer) gates this on `is_archy_peer(contact_id) ==
    /// false`; archy peers keep using `send_text_msg`/`send_resource` with
    /// the typed envelope so rich fields (caption, cid, thumb) survive.
    pub async fn send_native_image(
        &mut self,
        dest_pubkey_prefix: &[u8; 6],
        mime: &str,
        bytes: &[u8],
        caption: Option<&str>,
    ) -> Result<()> {
        use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
        let dest_hash = self
            .resolve_dest_hash(dest_pubkey_prefix)
            .with_context(|| {
                format!(
                    "Unknown Reticulum prefix {} — peer hasn't announced yet",
                    hex::encode(dest_pubkey_prefix)
                )
            })?;
        self.send_rpc(serde_json::json!({
            "cmd": "send",
            "dest_hash": hex::encode(dest_hash),
            "content": caption.unwrap_or(""),
            "method": "direct",
            "image_format": mime_to_lxmf_format(mime),
            "image_b64": B64.encode(bytes),
        }))
        .await
    }

    /// Send `data` (typically an already-built typed-envelope wire blob) to a
    /// peer over a dedicated RNS Resource transfer instead of the small LXMF
    /// "content" path `send_text_msg` uses — for payloads too large for the
    /// inline-chunk cap but well within what LoRa can carry over a proper RNS
    /// Resource (native chunked transfer with retries, unlike our own
    /// MC-chunk scheme). Fire-and-forget, matching `send_text_msg`'s existing
    /// semantics (no synchronous delivery confirmation) — `resource_sent`/
    /// `resource_failed`/`resource_progress` events are drained and logged by
    /// `handle_event`, not awaited here.
    pub async fn send_resource(&mut self, dest_pubkey_prefix: &[u8; 6], data: &[u8]) -> Result<()> {
        use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
        let dest_hash = self
            .resolve_dest_hash(dest_pubkey_prefix)
            .with_context(|| {
                format!(
                    "Unknown Reticulum prefix {} — peer hasn't announced yet",
                    hex::encode(dest_pubkey_prefix)
                )
            })?;
        let req_id = self.next_resource_id();
        self.send_rpc(serde_json::json!({
            "cmd": "send_resource",
            "id": req_id,
            "dest_hash": hex::encode(dest_hash),
            "data_b64": B64.encode(data),
        }))
        .await
    }

    pub async fn remove_contact(&mut self, _pubkey: &[u8; 32]) -> Result<()> {
        // RNS has no firmware-side contact table to prune — peers simply stop
        // being reachable when their announce/path ages out.
        Ok(())
    }

    pub async fn add_contact(
        &mut self,
        _pubkey: &[u8; 32],
        _contact_type: u8,
        _flags: u8,
        _out_path_len: u8,
        _name: &str,
        _last_advert: u32,
    ) -> Result<()> {
        // No firmware contact table to seed — RNS learns peers from announces
        // (handled by drain_events) and from path requests issued on send.
        Ok(())
    }

    pub async fn get_contacts(&mut self) -> Result<Vec<ParsedContact>> {
        self.drain_events().await;
        Ok(self
            .peers
            .values()
            .map(|p| ParsedContact {
                public_key_hex: hex::encode(p.dest_hash),
                advert_name: p.display_name.clone(),
                last_advert: p.last_advert_at as u32,
                // Deliberately not 1 ("friend"/meshcore type), so the
                // meshcore-only auto-heal `reset_contact_path` loop in
                // `refresh_contacts` (session.rs) skips these — RNS does its
                // own pathfinding, there is no firmware path to reset.
                contact_type: 2,
                path_len: if p.reachable { 1 } else { 0 },
                flags: 0,
                // RNS/LXMF is unconditionally E2E — see `take_rx_encrypted`.
                // This field tracks Meshtastic's per-contact PKC capability,
                // which has no Reticulum analogue (always true, tracked
                // elsewhere via `take_rx_encrypted`), so leave it false here.
                pkc_capable: false,
                // Signal stats from the last direct RNode reception (the
                // sidecar reports them per announce/message; None over TCP
                // or multi-hop, which is the honest answer there). Position
                // stays Meshtastic-only — no Reticulum position-sharing
                // convention is wired up.
                rssi: p.last_rssi,
                snr: p.last_snr,
                lat: None,
                lon: None,
                arch_pubkey_hex: p.arch_pubkey_hex.clone(),
            })
            .collect())
    }

    pub async fn sync_messages(&mut self) -> Result<Vec<InboundFrame>> {
        self.drain_events().await;
        Ok(self.inbound.drain(..).collect())
    }

    pub async fn try_recv_frame(&mut self) -> Result<Option<InboundFrame>> {
        self.drain_events().await;
        if self.daemon_gone {
            // Surface the dead daemon as a hard error so run_mesh_session
            // bails and the outer reconnect loop respawns it, instead of
            // idling on an empty queue until the RX-stall watchdog fires.
            anyhow::bail!("reticulum-daemon is gone (process exited or RPC socket closed)");
        }
        Ok(self.inbound.pop_front())
    }

    /// RNS/LXMF links are end-to-end encrypted by default (no plaintext mode),
    /// so every inbound delivery is E2E. Unlike Meshtastic, which only
    /// sometimes gets PKI delivery, this is unconditionally true.
    pub fn take_rx_encrypted(&mut self) -> bool {
        true
    }

    // ── internals ──────────────────────────────────────────────────────

    async fn send_rpc(&mut self, req: Value) -> Result<()> {
        let mut line = serde_json::to_vec(&req)?;
        line.push(b'\n');
        self.writer
            .write_all(&line)
            .await
            .context("Reticulum daemon RPC write failed")?;
        Ok(())
    }

    /// Drain any buffered daemon events (non-blocking) and translate them into
    /// peer-table updates / synthetic InboundFrames.
    async fn drain_events(&mut self) {
        // A daemon that died without closing the socket cleanly (SIGKILL,
        // OOM) leaves the socket readable-with-EOF or just silent — poll the
        // child's exit status too so death is never mistaken for quiet.
        if !self.daemon_gone {
            if let Ok(Some(status)) = self.child.try_wait() {
                warn!(%status, "reticulum-daemon process exited");
                self.daemon_gone = true;
            }
        }
        loop {
            let mut line = String::new();
            let read =
                tokio::time::timeout(Duration::from_millis(20), self.reader.read_line(&mut line))
                    .await;
            let n = match read {
                Ok(Ok(n)) => n,
                Ok(Err(e)) => {
                    warn!("Reticulum daemon RPC read failed: {}", e);
                    self.daemon_gone = true;
                    break;
                }
                Err(_) => break, // timeout — no data buffered
            };
            if n == 0 {
                warn!("Reticulum daemon RPC connection closed");
                self.daemon_gone = true;
                break;
            }
            let Ok(ev) = serde_json::from_str::<Value>(line.trim()) else {
                continue;
            };
            self.handle_event(ev);
        }
    }

    /// Restart the sidecar daemon: ask it to shut down cleanly and mark the
    /// link dead so the session loop tears down and the outer reconnect loop
    /// respawns it — re-detecting the RNode and reapplying the RF config
    /// from the (possibly just-edited) persisted settings. This IS the
    /// "reboot device" semantic for Reticulum radios, and the apply step of
    /// the .126 LoRa settings panel.
    pub async fn restart_daemon(&mut self) -> Result<()> {
        // Best-effort clean shutdown (lets PyInstaller clear its _MEI dir);
        // the SIGTERM path in Drop/terminate covers an already-dead socket.
        let _ = self.send_rpc(serde_json::json!({"cmd": "shutdown"})).await;
        self.daemon_gone = true;
        Ok(())
    }

    /// Ask the sidecar for the live RNode state and wait briefly for the
    /// reply event. Returns the freshest `radio_state` payload, or `None`
    /// when the daemon didn't answer in time (dead daemon, no radio build).
    pub async fn query_radio_state(&mut self, timeout: Duration) -> Option<Value> {
        self.last_radio_state = None;
        if self
            .send_rpc(serde_json::json!({"cmd": "radio_state"}))
            .await
            .is_err()
        {
            return None;
        }
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            self.drain_events().await;
            if let Some(state) = &self.last_radio_state {
                return Some(state.clone());
            }
            if self.daemon_gone || tokio::time::Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn handle_event(&mut self, ev: Value) {
        match ev.get("event").and_then(Value::as_str) {
            Some("radio_state") => {
                self.last_radio_state = Some(ev);
            }
            Some("announce") => {
                let Some(hash) = ev
                    .get("dest_hash")
                    .and_then(Value::as_str)
                    .and_then(|h| parse_hash16(h).ok())
                else {
                    return;
                };
                let prefix: [u8; 6] = hash[..6].try_into().unwrap();
                self.prefix_to_hash.insert(prefix, hash);
                // Current daemons decode the LXMF announce app_data themselves
                // and hand us clean fields: `display_name` (LXMF-standard
                // msgpack name, Sideband-interoperable) and `archy_blob` (the
                // `ARCHY:n:` identity string, carried as an extra msgpack list
                // element stock clients ignore). The raw `app_data` text path
                // below remains for announces from pre-upgrade archy nodes,
                // whose app_data was EITHER the blob OR a bare-utf8 name.
                let explicit_name = ev
                    .get("display_name")
                    .and_then(Value::as_str)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let explicit_blob = ev
                    .get("archy_blob")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .filter(|s| !s.is_empty());
                let app_data_text = ev
                    .get("app_data")
                    .and_then(Value::as_str)
                    .and_then(|h| hex::decode(h).ok())
                    .map(|b| String::from_utf8_lossy(&b).to_string())
                    .filter(|s| !s.is_empty());

                // If the announce app_data is an ARCHY:n: identity blob (see
                // daemon_command's doc comment), bind the ed25519 hex directly
                // onto this RNS-hash-keyed peer below (unambiguous — it came
                // in the same event as `hash`) AND surface it through the same
                // channel-text path meshcore/Meshtastic identity adverts use
                // (frames::handle_channel_payload -> parse_identity_broadcast
                // -> handle_identity_received), so it also lands on that
                // transport's federation-twin peer for UI/contact purposes.
                // `group_peer_twins` can then collapse the two rows since both
                // now carry the same `arch_pubkey_hex`, instead of relying on
                // `bind_federation_twins`'s advert_name matching, which never
                // matches here — see `display_name` below.
                let legacy_identity = app_data_text
                    .as_deref()
                    .and_then(protocol::parse_identity_broadcast);
                let is_legacy_blob = legacy_identity.is_some();
                let identity_blob_text =
                    explicit_blob.or_else(|| app_data_text.clone().filter(|_| is_legacy_blob));
                let parsed_identity = identity_blob_text
                    .as_deref()
                    .and_then(protocol::parse_identity_broadcast);
                if let Some(text) = identity_blob_text
                    .as_deref()
                    .filter(|_| parsed_identity.is_some())
                {
                    let mut data = Vec::with_capacity(7 + text.len());
                    data.push(0); // channel index — unused by the identity path
                    data.extend_from_slice(&prefix);
                    data.extend_from_slice(text.as_bytes());
                    self.inbound.push_back(InboundFrame {
                        code: protocol::RESP_MESHTASTIC_CHANNEL_TEXT,
                        data,
                        bytes_consumed: 0,
                    });
                }
                let arch_pubkey_hex = parsed_identity.map(|(_did, ed_pubkey, _x25519)| ed_pubkey);

                let announced_name =
                    pick_announced_name(explicit_name, app_data_text, is_legacy_blob);
                // Per-announce signal stats from the sidecar: real numbers on
                // a direct RNode reception, null over TCP or multi-hop. Only
                // overwrite the cached value when the sidecar reports one —
                // an announce relayed over TCP must not blank out the last
                // real RF reading.
                let rssi = ev
                    .get("rssi")
                    .and_then(Value::as_i64)
                    .and_then(|v| i16::try_from(v).ok());
                let snr = ev.get("snr").and_then(Value::as_f64).map(|v| v as f32);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                self.peers
                    .entry(hash)
                    .and_modify(|p| {
                        if let Some(name) = announced_name.clone() {
                            p.display_name = name;
                        }
                        p.reachable = true;
                        p.last_advert_at = now;
                        if arch_pubkey_hex.is_some() {
                            p.arch_pubkey_hex = arch_pubkey_hex.clone();
                        }
                        if rssi.is_some() {
                            p.last_rssi = rssi;
                        }
                        if snr.is_some() {
                            p.last_snr = snr;
                        }
                    })
                    .or_insert_with(|| ReticulumPeer {
                        dest_hash: hash,
                        display_name: announced_name
                            .unwrap_or_else(|| format!("Reticulum {}", hex::encode(&hash[..4]))),
                        arch_pubkey_hex,
                        reachable: true,
                        last_advert_at: now,
                        last_rssi: rssi,
                        last_snr: snr,
                    });
                self.persist_peers();
            }
            Some("recv") => {
                let Some(source_hex) = ev.get("source_hash").and_then(Value::as_str) else {
                    return;
                };
                let Ok(source_hash) = parse_hash16(source_hex) else {
                    return;
                };
                let prefix: [u8; 6] = source_hash[..6].try_into().unwrap();
                self.prefix_to_hash.insert(prefix, source_hash);
                // A peer that messages us without ever announcing still needs
                // to survive a restart — give it a placeholder name (the real
                // one, if any, arrives via a later "announce" and overwrites
                // this) so its routing entry alone doesn't get lost. An
                // existing entry is proof of life too: mark it reachable so a
                // restart-restored (reachable=false) peer that DMs us doesn't
                // stay red-dotted until its next announce.
                let rssi = ev
                    .get("rssi")
                    .and_then(Value::as_i64)
                    .and_then(|v| i16::try_from(v).ok());
                let snr = ev.get("snr").and_then(Value::as_f64).map(|v| v as f32);
                match self.peers.entry(source_hash) {
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(ReticulumPeer {
                            dest_hash: source_hash,
                            display_name: format!("Reticulum {}", hex::encode(&source_hash[..4])),
                            arch_pubkey_hex: None,
                            reachable: true,
                            last_advert_at: 0,
                            last_rssi: rssi,
                            last_snr: snr,
                        });
                        self.persist_peers();
                    }
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        let p = e.get_mut();
                        p.reachable = true;
                        if rssi.is_some() {
                            p.last_rssi = rssi;
                        }
                        if snr.is_some() {
                            p.last_snr = snr;
                        }
                    }
                }

                // A stock LXMF client (Sideband/NomadNet — not an archy peer)
                // carries photos/files in native LXMF fields, not our own
                // typed-envelope wire format. Check those FIRST: if present,
                // build the SAME ContentInline typed envelope our own
                // attachment pipeline uses, so it renders identically in the
                // UI (dispatch.rs's existing ContentInline handling, zero new
                // frontend code) instead of the plain text bytes below.
                use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
                let caption = ev
                    .get("content")
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty());
                if let (Some(fmt), Some(b64)) = (
                    ev.get("image_format").and_then(Value::as_str),
                    ev.get("image_b64").and_then(Value::as_str),
                ) {
                    if let Ok(bytes) = B64.decode(b64) {
                        match build_content_inline_frame(
                            &prefix,
                            image_format_to_mime(fmt),
                            None,
                            caption,
                            bytes,
                        ) {
                            Ok(frame) => {
                                self.inbound.push_back(frame);
                                return;
                            }
                            Err(e) => warn!("Failed to build native image frame: {}", e),
                        }
                    }
                }
                if let (Some(filename), Some(b64)) = (
                    ev.get("attachment_filename").and_then(Value::as_str),
                    ev.get("attachment_b64").and_then(Value::as_str),
                ) {
                    if let Ok(bytes) = B64.decode(b64) {
                        match build_content_inline_frame(
                            &prefix,
                            "application/octet-stream",
                            Some(filename),
                            caption,
                            bytes,
                        ) {
                            Ok(frame) => {
                                self.inbound.push_back(frame);
                                return;
                            }
                            Err(e) => warn!("Failed to build native attachment frame: {}", e),
                        }
                    }
                }

                let content_str = ev.get("content").and_then(Value::as_str).unwrap_or("");
                let content = match content_str.strip_prefix(RETICULUM_BINARY_CONTENT_MARKER) {
                    Some(b64) => match B64.decode(b64) {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            warn!("Failed to decode binary Reticulum content: {}", e);
                            Vec::new()
                        }
                    },
                    None => content_str.as_bytes().to_vec(),
                };
                self.inbound
                    .push_back(build_synthetic_frame(&prefix, &content));
            }
            Some("resource_recv") => {
                let source_hex = ev
                    .get("source_hash")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let Ok(source_hash) = parse_hash16(source_hex) else {
                    // An empty source_hash means the sender's link wasn't
                    // identified (pre-identify daemon build on the far end) —
                    // the blob is undeliverable without attribution, but say
                    // so instead of vanishing it.
                    warn!(
                        source = source_hex,
                        "Dropping inbound Reticulum resource without valid source identity"
                    );
                    return;
                };
                let prefix: [u8; 6] = source_hash[..6].try_into().unwrap();
                self.prefix_to_hash.insert(prefix, source_hash);
                match self.peers.entry(source_hash) {
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(ReticulumPeer {
                            dest_hash: source_hash,
                            display_name: format!("Reticulum {}", hex::encode(&source_hash[..4])),
                            arch_pubkey_hex: None,
                            reachable: true,
                            last_advert_at: 0,
                            // Resource transfers ride an established Link —
                            // the sidecar reports no per-packet phy stats here.
                            last_rssi: None,
                            last_snr: None,
                        });
                        self.persist_peers();
                    }
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        e.get_mut().reachable = true;
                    }
                }
                use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
                let Some(data) = ev
                    .get("data_b64")
                    .and_then(Value::as_str)
                    .and_then(|b64| B64.decode(b64).ok())
                else {
                    warn!("resource_recv event with missing/invalid data_b64");
                    return;
                };
                // Resources carry the complete typed-envelope wire bytes
                // directly (no MC-chunk/base64 textification needed — RNS
                // Resources are already a binary-safe whole-blob transfer),
                // so this is the same payload shape `decode.rs` already
                // accepts for a single-frame (non-chunked) typed envelope.
                self.inbound
                    .push_back(build_synthetic_frame(&prefix, &data));
            }
            Some("resource_progress") => {
                debug!(
                    id = ?ev.get("id"),
                    transferred = ?ev.get("transferred"),
                    total = ?ev.get("total"),
                    "Reticulum resource transfer progress"
                );
            }
            Some("resource_sent") => {
                debug!(id = ?ev.get("id"), "Reticulum resource transfer completed");
            }
            Some("resource_failed") => {
                warn!(
                    id = ?ev.get("id"),
                    reason = ?ev.get("reason"),
                    "Reticulum resource transfer failed"
                );
            }
            Some("delivered") | Some("status") | Some("ready") | Some("error") | None => {}
            _ => {}
        }
    }
}

/// Build the synthetic `RESP_CONTACT_MSG_V3_E2E` InboundFrame for an inbound
/// LXMF message, byte-for-byte matching the layout Meshtastic already
/// produces (meshtastic.rs `parse_meshtastic_frame`) so `frames::handle_frame`
/// needs no Reticulum-specific branch:
///   [snr(1)=0][reserved(2)][sender_prefix(6)][path(1)=0xff][type(1)=0][rx_time(4 LE)][payload]
fn build_synthetic_frame(sender_prefix: &[u8; 6], payload: &[u8]) -> InboundFrame {
    let mut data = Vec::with_capacity(15 + payload.len());
    data.push(0); // SNR unknown (RNS doesn't expose per-packet SNR through LXMF)
    data.extend_from_slice(&[0, 0]); // reserved
    data.extend_from_slice(sender_prefix);
    data.push(0xff); // path: RNS does its own multi-hop routing, not exposed here
    data.push(0); // text type
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32;
    data.extend_from_slice(&now.to_le_bytes());
    data.extend_from_slice(payload);
    InboundFrame {
        // Reticulum/LXMF is always end-to-end encrypted — no plaintext mode.
        code: protocol::RESP_CONTACT_MSG_V3_E2E,
        data,
        bytes_consumed: 0,
    }
}

/// Wrap a native LXMF attachment (image field or file-attachments field, from
/// a stock Sideband/NomadNet peer — see the `Some("recv")` branch above) as
/// the SAME `ContentInline` typed envelope our own attachment pipeline
/// produces, so it renders identically in the UI via the existing
/// `dispatch.rs` `ContentInline` handling — no new frontend code needed.
fn build_content_inline_frame(
    sender_prefix: &[u8; 6],
    mime: &str,
    filename: Option<&str>,
    caption: Option<&str>,
    bytes: Vec<u8>,
) -> Result<InboundFrame> {
    let payload = ContentInlinePayload {
        mime: mime.to_string(),
        filename: filename.map(str::to_string),
        caption: caption.map(str::to_string),
        bytes,
    };
    let encoded = message_types::encode_payload(&payload)?;
    let wire = TypedEnvelope::new(MeshMessageType::ContentInline, encoded).to_wire()?;
    Ok(build_synthetic_frame(sender_prefix, &wire))
}

/// Map an LXMF `FIELD_IMAGE` format string (Sideband uses bare extensions
/// like "png"/"jpg"/"webp", confirmed against its own source) to a MIME type
/// the frontend's `isImageMime`/`<img>` rendering already understands.
fn image_format_to_mime(fmt: &str) -> &'static str {
    match fmt.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        _ => "image/png",
    }
}

/// Inverse of `image_format_to_mime`, for `send_native_image` — our attach
/// pipeline always compresses to JPEG (`imageCompression.ts`) except the
/// 'original' preset, so this covers the mimes that can actually reach here.
fn mime_to_lxmf_format(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        _ => "png",
    }
}

/// Derive a stable `u32` contact id from the 16-byte RNS destination hash,
/// masked to the low (non-federation-synthetic) id space. Sibling to
/// `meshtastic_contact_id` (listener/session.rs). Kept here so `initialize()`
/// can report a `node_id` consistent with what `refresh_contacts` will later
/// assign via the public helper of the same name in session.rs.
pub(crate) fn reticulum_contact_id_from_hash(hash: &[u8; 16]) -> u32 {
    let raw = u32::from_le_bytes([hash[0], hash[1], hash[2], hash[3]]);
    let masked = raw & 0x7FFF_FFFF;
    if masked == 0 {
        1
    } else {
        masked
    }
}

fn parse_hash16(hex_str: &str) -> Result<[u8; 16]> {
    let bytes = hex::decode(hex_str).context("invalid hex")?;
    bytes
        .try_into()
        .map_err(|b: Vec<u8>| anyhow::anyhow!("expected 16 bytes, got {}", b.len()))
}

/// Send the verified RNode KISS detect sequence and look for `DETECT_RESP`.
/// Bytes confirmed against the canonical Reticulum source — see the module
/// doc comment and `docs/RETICULUM-TRANSPORT-PROGRESS.md`.
pub(crate) async fn probe_rnode(path: &str) -> Result<()> {
    let port = serial2_tokio::SerialPort::open(path, PROBE_BAUD)
        .with_context(|| format!("Failed to open {} for Reticulum probe", path))?;
    // ESP32-S3 native-USB boards (Heltec V3/V4 etc. — no separate USB-UART
    // bridge chip) treat a DTR/RTS transition on open as a reset signal, the
    // same mechanism esptool uses to force bootloader entry. Deassert both
    // before writing the probe. Boards behind a USB-UART bridge (CP2102 on
    // the Heltec V3) get the reset pulse from the open() itself, before we
    // can deassert anything — that case is handled by the boot-settle retry
    // below.
    let _ = port.set_dtr(false);
    let _ = port.set_rts(false);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let probe: [u8; 13] = [
        KISS_FEND,
        KISS_CMD_DETECT,
        KISS_DETECT_REQ,
        KISS_FEND,
        KISS_CMD_FW_VERSION,
        0x00,
        KISS_FEND,
        KISS_CMD_PLATFORM,
        0x00,
        KISS_FEND,
        KISS_CMD_MCU,
        0x00,
        KISS_FEND,
    ];
    // Attempt 1: probe immediately. A board that did NOT reset on open (it
    // was already up — e.g. a re-probe of a running RNode) answers in well
    // under a second, so the fast path stays fast.
    tokio::time::timeout(Duration::from_millis(500), port.write_all(&probe))
        .await
        .context("RNode probe write timed out")?
        .context("RNode probe write failed")?;
    let mut buf = [0u8; 256];
    let mut seen = Vec::new();
    if await_detect_resp(&port, &mut buf, &mut seen, PROBE_READ_TIMEOUT).await {
        return Ok(());
    }

    // No DETECT_RESP. If the open() power-cycled the board (verified live on
    // a Heltec V3 RNode behind a CP2102: the ESP32 spends ~2.5-3s in boot
    // ROM + app init and silently eats anything written meanwhile, so the
    // first probe lands in the void), wait for its boot chatter to go quiet
    // and probe once more with a fresh response window.
    const BOOT_QUIET_WINDOW: Duration = Duration::from_millis(800);
    const BOOT_SETTLE_MAX: Duration = Duration::from_secs(6);
    let settle_deadline = tokio::time::Instant::now() + BOOT_SETTLE_MAX;
    let mut last_data = tokio::time::Instant::now();
    while tokio::time::Instant::now() < settle_deadline {
        match tokio::time::timeout(Duration::from_millis(150), port.read(&mut buf)).await {
            Ok(Ok(n)) if n > 0 => {
                seen.extend_from_slice(&buf[..n]);
                // A late DETECT_RESP to the first write still counts.
                if contains_detect_resp(&seen) {
                    return Ok(());
                }
                last_data = tokio::time::Instant::now();
            }
            _ => {
                if last_data.elapsed() >= BOOT_QUIET_WINDOW {
                    break;
                }
            }
        }
    }

    tokio::time::timeout(Duration::from_millis(500), port.write_all(&probe))
        .await
        .context("RNode probe rewrite timed out")?
        .context("RNode probe rewrite failed")?;
    seen.clear();
    if await_detect_resp(&port, &mut buf, &mut seen, PROBE_READ_TIMEOUT).await {
        return Ok(());
    }
    anyhow::bail!(
        "No RNode DETECT_RESP within {:?} (incl. post-boot-settle retry)",
        PROBE_READ_TIMEOUT
    )
}

/// Read from `port` for up to `window`, accumulating into `seen`; true once
/// the KISS DETECT_RESP sequence shows up anywhere in the stream.
async fn await_detect_resp(
    port: &serial2_tokio::SerialPort,
    buf: &mut [u8],
    seen: &mut Vec<u8>,
    window: Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + window;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(150), port.read(buf)).await {
            Ok(Ok(n)) if n > 0 => {
                seen.extend_from_slice(&buf[..n]);
                if contains_detect_resp(seen) {
                    return true;
                }
            }
            _ => continue,
        }
    }
    false
}

/// Look for the `[FEND, CMD_DETECT, DETECT_RESP]` sequence anywhere in the
/// buffer (KISS framing means other command responses may interleave first).
fn contains_detect_resp(buf: &[u8]) -> bool {
    buf.windows(3)
        .any(|w| w == [KISS_FEND, KISS_CMD_DETECT, KISS_DETECT_RESP])
}

/// The display name an announce actually asserted, if any.
///
/// Precedence: the daemon-decoded LXMF display name (`display_name` event
/// field), then — legacy peers only — bare-utf8 app_data that wasn't an
/// identity blob. The bare-utf8 fallback must actually look like text:
/// lossy-decoded msgpack (a new-format announce whose name the daemon failed
/// to decode) is full of U+FFFD/control chars and would otherwise become a
/// mojibake display name. `None` (e.g. a blob-only legacy announce) means
/// "no name asserted" and must NOT stomp a previously-learned name.
fn pick_announced_name(
    explicit_name: Option<String>,
    app_data_text: Option<String>,
    is_legacy_blob: bool,
) -> Option<String> {
    explicit_name
        // A legacy blob-only announce utf8-decodes cleanly, so LXMF's
        // display_name_from_app_data hands the daemon the ENTIRE `ARCHY:…`
        // string as a "name" — seen live from a pre-upgrade Framework PT.
        // An identity blob is never a display name.
        .filter(|s| !s.starts_with("ARCHY:"))
        .or_else(|| {
            app_data_text
                .filter(|_| !is_legacy_blob)
                .filter(|s| !s.chars().any(|c| c.is_control() || c == '\u{FFFD}'))
        })
}

impl Drop for ReticulumLink {
    fn drop(&mut self) {
        // Group-wide SIGTERM with a delayed SIGKILL backstop (`terminate_group`).
        // The old immediate SIGTERM+SIGKILL never let the PyInstaller bootloader
        // run its exit cleanup, stranding a 48M `_MEI*` extraction dir on every
        // reconnect/restart until /tmp filled; the grace period fixes that, and
        // the per-spawn TMPDIR wipe in `spawn` covers any dir that still leaks.
        terminate_group(&self.child);
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelled_daemon_handshake_terminates_child() {
        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let child = Command::new("sleep")
                .arg("60")
                .process_group(0)
                .spawn()
                .unwrap();
            let pid = child.id().unwrap();
            let _starting = StartingDaemon(Some(child));
            started.send(pid).unwrap();
            std::future::pending::<()>().await;
        });
        let pid = ready.await.unwrap();
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, 0);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        tokio::time::timeout(Duration::from_secs(3), async {
            while unsafe { libc::kill(pid as i32, 0) } == 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("cancelled handshake left its child alive");
    }

    #[test]
    fn announced_name_precedence() {
        // Daemon-decoded LXMF name always wins.
        assert_eq!(
            pick_announced_name(
                Some("RNode Shaza".into()),
                Some("ARCHY:2:aa:bb".into()),
                true
            ),
            Some("RNode Shaza".to_string())
        );
        // Legacy bare-utf8 name (old daemon, no explicit field).
        assert_eq!(
            pick_announced_name(None, Some("zaza".into()), false),
            Some("zaza".to_string())
        );
        // Legacy blob-only announce asserts NO name (must not stomp).
        assert_eq!(
            pick_announced_name(None, Some("ARCHY:2:aa:bb".into()), true),
            None
        );
        // Lossy-decoded msgpack must not become a mojibake name.
        assert_eq!(
            pick_announced_name(None, Some("\u{FFFD}\u{FFFD}Shaza\u{FFFD}".into()), false),
            None
        );
        assert_eq!(
            pick_announced_name(None, Some("has\u{1}ctl".into()), false),
            None
        );
        // Nothing at all.
        assert_eq!(pick_announced_name(None, None, false), None);
    }

    #[test]
    fn detect_resp_found_in_kiss_stream() {
        let stream = [
            0x10,
            0x20,
            KISS_FEND,
            KISS_CMD_DETECT,
            KISS_DETECT_RESP,
            0x99,
        ];
        assert!(contains_detect_resp(&stream));
    }

    /// Hardware gate: `probe_rnode` against a real RNode-flashed board. Not run in
    /// CI (no hardware there) — run manually with
    /// `ARCHY_RNODE_TEST_PORT=/dev/ttyUSB0 cargo test -p archipelago --lib
    /// mesh::reticulum::tests::probe_rnode_detects_real_hardware -- --ignored --nocapture`
    /// once an RNode-flashed board is attached. Confirms the byte-for-byte KISS
    /// constants documented above (cited from canonical RNS source) actually work
    /// against real firmware, not just the unit-tested byte matcher above.
    #[tokio::test]
    #[ignore = "requires a real RNode-flashed board; set ARCHY_RNODE_TEST_PORT"]
    async fn probe_rnode_detects_real_hardware() {
        let port = std::env::var("ARCHY_RNODE_TEST_PORT")
            .expect("set ARCHY_RNODE_TEST_PORT to the RNode's serial path");
        probe_rnode(&port)
            .await
            .expect("KISS detect probe failed against real hardware");
    }

    #[test]
    fn detect_resp_absent_in_unrelated_stream() {
        let stream = [0x10, 0x20, 0x30, 0x40];
        assert!(!contains_detect_resp(&stream));
    }

    /// Regression test for the peer-identity persistence fix: a dest hash
    /// round-tripped through `PersistedReticulumPeer` (hex on disk) via
    /// `hex::encode`/`parse_hash16` — the exact pair `persist_peers`/
    /// `load_persisted_peers` use — must come back byte-for-byte identical,
    /// or a restart would silently corrupt peer routing instead of just
    /// losing it.
    #[test]
    fn persisted_peer_hex_roundtrip() {
        let hash: [u8; 16] = [
            0x18, 0x70, 0x74, 0x4d, 0x7c, 0x35, 0xa9, 0x2c, 0xf0, 0x61, 0xfb, 0x81, 0x0e, 0xf3,
            0x41, 0x65,
        ];
        let persisted = vec![PersistedReticulumPeer {
            dest_hash_hex: hex::encode(hash),
            display_name: "zazaticulum".to_string(),
            arch_pubkey_hex: Some("abcdef".to_string()),
        }];

        let dir =
            std::env::temp_dir().join(format!("archy-reticulum-peers-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("peers.json");
        std::fs::write(&path, serde_json::to_vec(&persisted).unwrap()).unwrap();

        let loaded: Vec<PersistedReticulumPeer> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].display_name, "zazaticulum");
        assert_eq!(loaded[0].arch_pubkey_hex.as_deref(), Some("abcdef"));
        let round_tripped_hash = parse_hash16(&loaded[0].dest_hash_hex).unwrap();
        assert_eq!(round_tripped_hash, hash);
        let prefix: [u8; 6] = round_tripped_hash[..6].try_into().unwrap();
        assert_eq!(prefix, hash[..6]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn contact_id_masks_high_bit_and_avoids_zero() {
        let hash_high_bit = {
            let mut h = [0u8; 16];
            h[0..4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
            h
        };
        let id = reticulum_contact_id_from_hash(&hash_high_bit);
        assert!(
            id < 0x8000_0000,
            "must not collide with federation-synthetic space"
        );
        assert_ne!(id, 0);

        let zero_hash = [0u8; 16];
        assert_eq!(reticulum_contact_id_from_hash(&zero_hash), 1);
    }

    // One test covers both directions to avoid two tests racing on the same
    // process-global env var under the parallel test runner.
    #[tokio::test]
    async fn transport_flag_gated_on_daemon_support() {
        let dir = std::env::temp_dir().join(format!("archy-daemon-stub-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let old = dir.join("old-daemon");
        std::fs::write(&old, "#!/bin/sh\necho 'usage: --serial-port --no-radio'\n").unwrap();
        let new = dir.join("new-daemon");
        std::fs::write(
            &new,
            "#!/bin/sh\necho 'usage: --serial-port --enable-transport --no-radio'\n",
        )
        .unwrap();
        for p in [&old, &new] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        // An old daemon whose --help doesn't advertise the flag: never pass it
        // (its argparse would exit and kill the mesh session — the v1.7.117
        // fleet regression this guards against).
        std::env::set_var("ARCHY_RETICULUM_DAEMON_BIN", &old);
        assert!(!daemon_supports_enable_transport().await);

        std::env::set_var("ARCHY_RETICULUM_DAEMON_BIN", &new);
        assert!(daemon_supports_enable_transport().await);

        // A missing binary must also fail safe (probe error → no flag).
        std::env::set_var("ARCHY_RETICULUM_DAEMON_BIN", dir.join("missing"));
        // (falls through to the venv dev path, which doesn't exist in the
        // test environment either → probe fails → false)
        assert!(!daemon_supports_enable_transport().await);

        std::env::remove_var("ARCHY_RETICULUM_DAEMON_BIN");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn synthetic_frame_matches_meshtastic_layout() {
        let prefix = [1, 2, 3, 4, 5, 6];
        let frame = build_synthetic_frame(&prefix, b"hello");
        assert_eq!(frame.code, protocol::RESP_CONTACT_MSG_V3_E2E);
        // header is 15 bytes before the payload, per the documented layout
        assert_eq!(&frame.data[3..9], &prefix);
        assert_eq!(frame.data[9], 0xff);
        assert_eq!(frame.data[10], 0);
        assert_eq!(&frame.data[15..], b"hello");
    }
}
