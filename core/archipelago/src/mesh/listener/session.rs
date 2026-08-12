//! Mesh session lifecycle: connect, initialize, main loop.

use super::super::meshtastic::MeshtasticDevice;
use super::super::reticulum::ReticulumLink;
use super::super::serial::MeshcoreDevice;
use super::super::types::*;
use super::{
    dispatch, frames, MeshCommand, MeshState, ADVERT_INTERVAL, MAX_CONSECUTIVE_WRITE_FAILURES,
    RX_STALL_TIMEOUT, SYNC_INTERVAL,
};
use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

enum MeshRadioDevice {
    Meshcore(MeshcoreDevice),
    Meshtastic(MeshtasticDevice),
    Reticulum(ReticulumLink),
}

impl MeshRadioDevice {
    fn device_type(&self) -> DeviceType {
        match self {
            Self::Meshcore(_) => DeviceType::Meshcore,
            Self::Meshtastic(_) => DeviceType::Meshtastic,
            Self::Reticulum(_) => DeviceType::Reticulum,
        }
    }

    fn advert_name(&self) -> Option<String> {
        match self {
            Self::Meshcore(device) => device.advert_name.clone(),
            Self::Meshtastic(device) => device.advert_name(),
            Self::Reticulum(device) => device.advert_name(),
        }
    }

    async fn set_advert_name(&mut self, name: &str) -> Result<()> {
        match self {
            Self::Meshcore(device) => device.set_advert_name(name).await,
            Self::Meshtastic(device) => device.set_advert_name(name).await,
            Self::Reticulum(device) => device.set_advert_name(name).await,
        }
    }

    /// Provision the operator-configured LoRa region. Meshcore radios manage
    /// their own band on the device, so this is a no-op for them; Meshtastic
    /// radios ship region-UNSET (RF-silent) and must be set or they never mesh.
    /// Returns `Ok(true)` when a region was written (the device reboots to
    /// apply, so the caller should restart the session). No-op for Reticulum:
    /// the daemon's RNodeInterface config carries its own LoRa profile, not
    /// driven through this firmware-admin path.
    async fn ensure_lora_region(&mut self, region: Option<&str>) -> Result<bool> {
        match self {
            Self::Meshcore(_) => Ok(false),
            Self::Meshtastic(device) => device.ensure_lora_region(region).await,
            Self::Reticulum(_) => Ok(false),
        }
    }

    /// Provision the shared archy primary channel so all nodes can decode each
    /// other. No-op for meshcore (it joins its channel by name on the device);
    /// Meshtastic radios can sit on mismatched channels otherwise and silently
    /// drop every packet as undecryptable. Returns `Ok(true)` when a channel was
    /// written (device reboots; caller should restart the session). No-op for
    /// Reticulum: RNS has no shared-PSK channel concept (see
    /// `ReticulumLink::send_channel_text`).
    async fn ensure_channel(&mut self, channel_name: Option<&str>) -> Result<bool> {
        match self {
            Self::Meshcore(_) => Ok(false),
            Self::Meshtastic(device) => device.ensure_channel(channel_name).await,
            Self::Reticulum(_) => Ok(false),
        }
    }

    async fn send_self_advert(&mut self) -> Result<()> {
        match self {
            Self::Meshcore(device) => device.send_self_advert().await,
            Self::Meshtastic(device) => device.send_self_advert().await,
            Self::Reticulum(device) => device.send_self_advert().await,
        }
    }

    /// Lightweight serial keepalive (Meshtastic only). Keeps the firmware
    /// streaming RECEIVED packets to our serial client — without it the radio
    /// can mark a quiet client gone and deliver only our own queue-status.
    /// Meshcore/Reticulum need no such ping (Reticulum's "serial" traffic is
    /// the daemon's own RNS link, not a firmware queue we poll).
    async fn send_keepalive(&mut self) -> Result<()> {
        match self {
            Self::Meshcore(_) => Ok(()),
            Self::Meshtastic(device) => device.send_keepalive().await,
            Self::Reticulum(_) => Ok(()),
        }
    }

    /// Actively advertise our identity over the air. Meshcore already does this
    /// inside `send_self_advert` (CMD_SEND_SELF_ADVERT), so this is a no-op for
    /// it; Meshtastic needs an explicit NodeInfo broadcast or peers never learn
    /// about an already-running node. No-op for Reticulum: its `announce` (via
    /// `send_self_advert`) already covers discovery.
    async fn send_nodeinfo_advert(&mut self, want_response: bool) -> Result<()> {
        match self {
            Self::Meshcore(_) => Ok(()),
            Self::Meshtastic(device) => device.send_nodeinfo_broadcast(want_response).await,
            Self::Reticulum(_) => Ok(()),
        }
    }

    async fn send_channel_text(&mut self, channel: u8, payload: &[u8]) -> Result<()> {
        match self {
            Self::Meshcore(device) => device.send_channel_text(channel, payload).await,
            Self::Meshtastic(device) => device.send_channel_text(channel, payload).await,
            Self::Reticulum(device) => device.send_channel_text(channel, payload).await,
        }
    }

    async fn send_text_msg(&mut self, dest_pubkey_prefix: &[u8; 6], payload: &[u8]) -> Result<()> {
        match self {
            Self::Meshcore(device) => device.send_text_msg(dest_pubkey_prefix, payload).await,
            Self::Meshtastic(device) => device.send_text_msg(dest_pubkey_prefix, payload).await,
            Self::Reticulum(device) => device.send_text_msg(dest_pubkey_prefix, payload).await,
        }
    }

    /// Send an image via native LXMF `FIELD_IMAGE` — Reticulum-only, for a
    /// stock (non-archy) peer that can't decode our typed envelope. See
    /// `ReticulumLink::send_native_image`.
    async fn send_native_image(
        &mut self,
        dest_pubkey_prefix: &[u8; 6],
        mime: &str,
        bytes: &[u8],
        caption: Option<&str>,
    ) -> Result<()> {
        match self {
            Self::Meshcore(_) | Self::Meshtastic(_) => {
                anyhow::bail!("Native image send is Reticulum-only")
            }
            Self::Reticulum(device) => {
                device
                    .send_native_image(dest_pubkey_prefix, mime, bytes, caption)
                    .await
            }
        }
    }

    /// Send `data` over a dedicated RNS Resource transfer instead of the
    /// small-payload "content" path — only Reticulum has anything resembling
    /// this (a native large-binary transfer protocol over a `RNS.Link`).
    /// Meshcore/Meshtastic have no equivalent in our driver; callers must
    /// check `device_type() == DeviceType::Reticulum` before reaching for
    /// this (see `mesh.transport-advice`'s `"resource-mesh"` tier, which is
    /// Reticulum-only), so an Err here means the caller's gating is wrong,
    /// not a legitimate no-op.
    async fn send_resource(&mut self, dest_pubkey_prefix: &[u8; 6], data: &[u8]) -> Result<()> {
        match self {
            Self::Meshcore(_) | Self::Meshtastic(_) => {
                anyhow::bail!("Resource transfer is Reticulum-only")
            }
            Self::Reticulum(device) => device.send_resource(dest_pubkey_prefix, data).await,
        }
    }

    async fn reboot(&mut self, seconds: i64) -> Result<String> {
        match self {
            // No remote reboot in the MeshCore serial protocol — say so
            // instead of silently reporting success (the old `Ok(())` here
            // is why the button "did nothing" for the operator).
            Self::Meshcore(_) => {
                anyhow::bail!("MeshCore radios have no remote reboot — power-cycle the device")
            }
            Self::Meshtastic(device) => {
                device.reboot(seconds).await?;
                Ok(format!(
                    "Radio firmware reboots in {seconds}s and reconnects automatically"
                ))
            }
            // Restarting the sidecar drops the serial port, re-detects the
            // RNode and reapplies the RF config — the closest thing to a
            // reboot the RNS stack has, and exactly what an operator wants
            // after changing settings or on a wedged radio.
            Self::Reticulum(device) => {
                device.restart_daemon().await?;
                Ok("Radio daemon restarting — the RNode re-detects and reconnects in about 15 seconds".to_string())
            }
        }
    }

    /// Live RNode radio state — Reticulum-only (see ReticulumLink::query_radio_state).
    async fn radio_state(&mut self) -> Result<serde_json::Value> {
        match self {
            Self::Meshcore(_) | Self::Meshtastic(_) => {
                anyhow::bail!("Radio state read-back is only available for Reticulum RNode devices")
            }
            Self::Reticulum(device) => device
                .query_radio_state(std::time::Duration::from_secs(5))
                .await
                .ok_or_else(|| anyhow::anyhow!("The radio daemon did not answer the state query")),
        }
    }

    async fn remove_contact(&mut self, pubkey: &[u8; 32]) -> Result<()> {
        match self {
            Self::Meshcore(device) => device.remove_contact(pubkey).await,
            Self::Meshtastic(device) => device.remove_contact(pubkey).await,
            Self::Reticulum(device) => device.remove_contact(pubkey).await,
        }
    }

    async fn add_contact(
        &mut self,
        pubkey: &[u8; 32],
        contact_type: u8,
        flags: u8,
        out_path_len: u8,
        name: &str,
        last_advert: u32,
    ) -> Result<()> {
        match self {
            Self::Meshcore(device) => {
                device
                    .add_contact(pubkey, contact_type, flags, out_path_len, name, last_advert)
                    .await
            }
            Self::Meshtastic(device) => {
                device
                    .add_contact(pubkey, contact_type, flags, out_path_len, name, last_advert)
                    .await
            }
            Self::Reticulum(device) => {
                device
                    .add_contact(pubkey, contact_type, flags, out_path_len, name, last_advert)
                    .await
            }
        }
    }

    async fn get_contacts(&mut self) -> Result<Vec<super::super::protocol::ParsedContact>> {
        match self {
            Self::Meshcore(device) => device.get_contacts().await,
            Self::Meshtastic(device) => device.get_contacts().await,
            Self::Reticulum(device) => device.get_contacts().await,
        }
    }

    async fn reset_contact_path(&mut self, pubkey: &[u8; 32]) -> Result<()> {
        match self {
            Self::Meshcore(device) => device.reset_contact_path(pubkey).await,
            Self::Meshtastic(device) => device.reset_contact_path(pubkey).await,
            // RNS does its own pathfinding — no firmware path table to reset.
            Self::Reticulum(_) => Ok(()),
        }
    }

    async fn sync_messages(&mut self) -> Result<Vec<super::super::protocol::InboundFrame>> {
        match self {
            Self::Meshcore(device) => device.sync_messages().await,
            Self::Meshtastic(device) => device.sync_messages().await,
            Self::Reticulum(device) => device.sync_messages().await,
        }
    }

    async fn try_recv_frame(&mut self) -> Result<Option<super::super::protocol::InboundFrame>> {
        match self {
            Self::Meshcore(device) => device.try_recv_frame().await,
            Self::Meshtastic(device) => device.try_recv_frame().await,
            Self::Reticulum(device) => device.try_recv_frame().await,
        }
    }

    /// PKI-E2E status of the last inbound frame (meshtastic only; meshcore's
    /// per-message E2E is derived in the frames decrypt path). Reticulum/LXMF
    /// is unconditionally E2E (no plaintext mode), so it always reports true.
    /// Take-and-clear.
    fn take_rx_encrypted(&mut self) -> bool {
        match self {
            Self::Meshcore(_) => false,
            Self::Meshtastic(device) => device.take_rx_encrypted(),
            Self::Reticulum(device) => device.take_rx_encrypted(),
        }
    }
}

/// Scan all candidate serial ports and open the first supported mesh device found.
///
/// `device_kind`, when set, pins the expected firmware (operator-confirmed via
/// `MeshConfig.device_kind` — see the plan's §2c reflashable-board note): only
/// that one device's probe runs, so a non-matching firmware's init bytes are
/// never injected into the port. `None` keeps the strict
/// Reticulum→Meshcore→Meshtastic probe order.
async fn auto_detect_and_open(
    data_dir: &Path,
    our_ed_pubkey_hex: &str,
    our_x25519_pubkey_hex: &str,
    device_kind: Option<DeviceType>,
    skip_path: Option<&str>,
    advert_name: Option<&str>,
) -> Result<(String, MeshRadioDevice, DeviceInfo)> {
    let mut paths = super::super::serial::detect_serial_devices().await;
    // When falling back from a just-failed preferred path, don't probe that
    // same device again in the same cycle — every open() toggles DTR/RTS,
    // which resets ESP32-family boards, and back-to-back re-probes are what
    // keeps a mid-boot board from ever finishing its boot.
    if let Some(skip) = skip_path {
        let canon = |p: &str| std::fs::canonicalize(p).unwrap_or_else(|_| p.into());
        let skip_canon = canon(skip);
        paths.retain(|p| canon(p) != skip_canon);
    }
    if paths.is_empty() {
        anyhow::bail!(match skip_path {
            Some(skip) => format!("No serial devices found in /dev besides {skip}, which was already probed this cycle"),
            None => "No serial devices found in /dev".to_string(),
        });
    }
    info!(candidates = ?paths, "Auto-detect candidate ports for this attempt");
    for path in &paths {
        debug!(path = %path, "Probing for mesh radio device");
        // Tried FIRST: `ReticulumLink::open()` gates its expensive daemon
        // spawn behind a cheap (~1s worst case) RNode KISS-detect probe, so a
        // failed match here costs about as much as the Meshcore/Meshtastic
        // probes below. Trying it first matters in practice: on a real RNode
        // board, Meshcore's and Meshtastic's handshake bytes (~5s timeout
        // each, ~10.6s combined) sitting on the wire before Reticulum ever
        // gets a turn was observed to leave the RNode firmware unresponsive
        // by the time its turn came — confirmed on hardware that responds
        // correctly to the same KISS probe on a truly fresh port.
        if device_kind.is_none_or(|k| k == DeviceType::Reticulum) {
            match ReticulumLink::open(
                path,
                data_dir,
                Some(our_ed_pubkey_hex),
                Some(our_x25519_pubkey_hex),
                advert_name,
            )
            .await
            {
                Ok(mut dev) => match dev.initialize().await {
                    Ok(info) => {
                        info!(path = %path, "Found Reticulum (RNode) device via auto-detect");
                        return Ok((path.clone(), MeshRadioDevice::Reticulum(dev), info));
                    }
                    Err(e) => {
                        debug!(path = %path, error = %e, "Reticulum daemon failed to initialize")
                    }
                },
                Err(e) => debug!(path = %path, error = %e, "Not a Reticulum RNode"),
            }
        }
        if device_kind.is_none_or(|k| k == DeviceType::Meshcore) {
            match MeshcoreDevice::open(path).await {
                Ok(mut dev) => match dev.initialize().await {
                    Ok(info) => {
                        info!(path = %path, firmware = %info.firmware_version, "Found Meshcore device via auto-detect");
                        return Ok((path.clone(), MeshRadioDevice::Meshcore(dev), info));
                    }
                    Err(e) => debug!(path = %path, error = %e, "Not a Meshcore device"),
                },
                Err(e) => debug!(path = %path, error = %e, "Could not open serial port"),
            }
        }
        if device_kind.is_none_or(|k| k == DeviceType::Meshtastic) {
            match MeshtasticDevice::open(path).await {
                Ok(mut dev) => match dev.initialize().await {
                    Ok(info) => {
                        info!(path = %path, firmware = %info.firmware_version, "Found Meshtastic device via auto-detect");
                        return Ok((path.clone(), MeshRadioDevice::Meshtastic(dev), info));
                    }
                    Err(e) => debug!(path = %path, error = %e, "Not a Meshtastic device"),
                },
                Err(e) => {
                    debug!(path = %path, error = %e, "Could not open serial port for Meshtastic")
                }
            }
        }
    }
    anyhow::bail!(
        "No supported mesh radio found on {} candidate ports: {:?}",
        paths.len(),
        paths
    )
}

/// What the hot-swap probe learned about a just-plugged radio, without
/// provisioning anything. Serialized straight into the `mesh.probe-device`
/// RPC response for the device setup modal.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceProbe {
    pub path: String,
    /// "reticulum" | "meshcore" | "meshtastic"
    pub kind: String,
    pub firmware_version: Option<String>,
    pub node_id: Option<u32>,
    pub advert_name: Option<String>,
    /// Meshtastic-only: current radio config, read via `want_config`.
    pub region: Option<String>,
    pub modem_preset: Option<String>,
    pub primary_channel: Option<String>,
    pub secondary_channel: Option<String>,
    pub max_contacts: Option<u16>,
}

/// Serializes serial-port open sequences between the listener's session
/// opens and the RPC probe (`mesh.probe-device`). Linux happily double-opens
/// a tty, and two concurrent handshakes corrupt each other into silence —
/// observed live on .116 (2026-07-26): the kiosk browser's hot-swap
/// auto-probe collided with the listener's cycle on every backoff window, so
/// neither ever succeeded, and each collision's open() DTR/RTS-reset the
/// board again. The probe's retry-across-idle-gaps heuristic (5f01ec31)
/// narrowed but could not close the race; this closes it.
static PORT_OPEN_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Probe a serial port for a mesh radio WITHOUT provisioning it: identify the
/// firmware (same strict Reticulum→Meshcore→Meshtastic order as auto-detect,
/// for the same RNode-wedging reason) and read what's currently configured on
/// it. The port is released when this returns, so the listener's reconnect
/// loop can pick the device up afterwards. Reticulum uses the bare KISS
/// DETECT probe — no daemon spawn just to identify a stick.
pub(crate) async fn probe_device(path: &str) -> Result<DeviceProbe> {
    // Retries kept even with PORT_OPEN_LOCK closing the double-open race:
    // the board may still be mid-boot from a previous open's DTR/RTS reset,
    // and a later attempt after a quiet gap can succeed where the first
    // couldn't.
    let mut last_err = None;
    for attempt in 0..3u32 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(7)).await;
        }
        match probe_device_once(path).await {
            Ok(p) => return Ok(p),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("probe failed")))
}

async fn probe_device_once(path: &str) -> Result<DeviceProbe> {
    let _port_guard = PORT_OPEN_LOCK.lock().await;
    if super::super::reticulum::probe_rnode(path).await.is_ok() {
        return Ok(DeviceProbe {
            path: path.to_string(),
            kind: "reticulum".to_string(),
            firmware_version: Some("RNode (KISS)".to_string()),
            node_id: None,
            advert_name: None,
            region: None,
            modem_preset: None,
            primary_channel: None,
            secondary_channel: None,
            max_contacts: None,
        });
    }
    if let Ok(mut dev) = MeshcoreDevice::open(path).await {
        if let Ok(info) = dev.initialize().await {
            let queried = dev.query_device_info().await;
            return Ok(DeviceProbe {
                path: path.to_string(),
                kind: "meshcore".to_string(),
                firmware_version: queried.as_ref().map(|(v, _)| v.clone()),
                node_id: Some(info.node_id),
                advert_name: dev.advert_name(),
                region: None,
                modem_preset: None,
                primary_channel: None,
                secondary_channel: None,
                max_contacts: queried.map(|(_, m)| m).or(Some(info.max_contacts)),
            });
        }
    }
    if let Ok(mut dev) = MeshtasticDevice::open(path).await {
        if let Ok(info) = dev.initialize().await {
            let cfg = dev.probed_config();
            return Ok(DeviceProbe {
                path: path.to_string(),
                kind: "meshtastic".to_string(),
                firmware_version: Some(info.firmware_version.clone()),
                node_id: Some(info.node_id),
                advert_name: dev.advert_name(),
                region: cfg.region,
                modem_preset: cfg.modem_preset,
                primary_channel: cfg.primary_channel,
                secondary_channel: cfg.secondary_channel,
                max_contacts: Some(info.max_contacts),
            });
        }
    }
    anyhow::bail!("no supported mesh radio firmware answered on {path}")
}

async fn open_preferred_path(
    path: &str,
    data_dir: &Path,
    our_ed_pubkey_hex: &str,
    our_x25519_pubkey_hex: &str,
    device_kind: Option<DeviceType>,
    advert_name: Option<&str>,
) -> Result<(MeshRadioDevice, DeviceInfo)> {
    // Pinned: try only the configured firmware and surface its own error —
    // never fall through to (and inject probe bytes into) another firmware's
    // handshake on this port.
    if let Some(kind) = device_kind {
        return match kind {
            DeviceType::Meshcore => {
                let mut dev = MeshcoreDevice::open(path)
                    .await
                    .context("Could not open preferred path as Meshcore")?;
                let info = dev
                    .initialize()
                    .await
                    .context("Preferred path is not a working Meshcore device")?;
                Ok((MeshRadioDevice::Meshcore(dev), info))
            }
            DeviceType::Meshtastic => {
                let mut dev = MeshtasticDevice::open(path)
                    .await
                    .context("Could not open preferred path as Meshtastic")?;
                let info = dev
                    .initialize()
                    .await
                    .context("Preferred path is not a working Meshtastic device")?;
                Ok((MeshRadioDevice::Meshtastic(dev), info))
            }
            DeviceType::Reticulum => {
                let mut dev = ReticulumLink::open(
                    path,
                    data_dir,
                    Some(our_ed_pubkey_hex),
                    Some(our_x25519_pubkey_hex),
                    advert_name,
                )
                .await
                .context("Could not open preferred path as Reticulum")?;
                let info = dev
                    .initialize()
                    .await
                    .context("Preferred path is not a working Reticulum RNode")?;
                Ok((MeshRadioDevice::Reticulum(dev), info))
            }
            DeviceType::Unknown => anyhow::bail!("device_kind cannot be Unknown"),
        };
    }

    // Unpinned: don't probe this path ourselves at all. Confirmed live
    // 2026-07-23 — this function used to run its own Reticulum→Meshcore→
    // Meshtastic sequence here, and the caller (run_mesh_session) falls
    // back to `auto_detect_and_open` on any error, which scans every
    // candidate path (this one included) with the exact same three-protocol
    // sequence. With a single physical radio — the overwhelmingly common
    // case — `path` here IS the one candidate `auto_detect_and_open` is
    // about to try, so every unpinned reconnect was resetting the board via
    // Reticulum/Meshcore/Meshtastic's DTR/RTS toggle TWICE: once here, once
    // again moments later in auto-detect. Bailing immediately (no port
    // access at all) means auto-detect's single pass is the only one that
    // ever touches the port when nothing is pinned yet. (auto_detect_and_open
    // carries the advert_name threading from main, so nothing is lost.)
    let _ = advert_name;
    anyhow::bail!("No device_kind pin — deferring to auto-detect for {path}")
}

/// Bring up a Reticulum daemon over plain TCP — no physical RNode, no
/// `probe_rnode` KISS-detect gate. This is the radio-less counterpart to
/// `auto_detect_and_open`/`open_preferred_path`'s serial paths; see
/// `types::ReticulumTcpConfig`'s doc comment for scope (dev/verification,
/// loopback-only server bind).
async fn open_reticulum_tcp(
    cfg: &ReticulumTcpConfig,
    data_dir: &Path,
    our_ed_pubkey_hex: &str,
    our_x25519_pubkey_hex: &str,
    advert_name: Option<&str>,
) -> Result<(String, MeshRadioDevice, DeviceInfo)> {
    let mut dev = match cfg {
        ReticulumTcpConfig::Server { bind } => ReticulumLink::open_tcp_server(
            bind,
            data_dir,
            Some(our_ed_pubkey_hex),
            Some(our_x25519_pubkey_hex),
            advert_name,
        )
        .await
        .context("Could not open Reticulum TCP server interface")?,
        ReticulumTcpConfig::Client { connect } => ReticulumLink::open_tcp_client(
            connect,
            data_dir,
            Some(our_ed_pubkey_hex),
            Some(our_x25519_pubkey_hex),
            advert_name,
        )
        .await
        .context("Could not open Reticulum TCP client interface")?,
    };
    let info = dev
        .initialize()
        .await
        .context("Reticulum TCP interface failed to initialize")?;
    let label = match cfg {
        ReticulumTcpConfig::Server { bind } => format!("tcp-server:{bind}"),
        ReticulumTcpConfig::Client { connect } => format!("tcp-client:{}", connect.join(",")),
    };
    Ok((label, MeshRadioDevice::Reticulum(dev), info))
}

/// ASCII marker for the original DM-via-channel format:
/// `@DM:` + base64(`[dest_prefix(6)][inner…]`). No sender info on the wire,
/// so the receiver has to guess the sender from its contact table — which
/// misattributes traffic when more than one `Archy-*` peer is in range.
/// Kept for backwards compatibility with peers that haven't been upgraded.
#[allow(dead_code)]
pub(super) const DM_V1_MARKER: &str = "@DM:";

/// ASCII marker for the v2 DM-via-channel format:
/// `@DM2:` + base64(`[dest_prefix(6)][sender_arch_prefix(6)][inner…]`).
/// `sender_arch_prefix` is the first 6 bytes of the sender's archipelago
/// ed25519 public key, which the receiver looks up against its mesh peer
/// table (where federation-seeded peers carry the archipelago key) to
/// route inbound DMs to the correct contact_id thread.
pub(super) const DM_V2_MARKER: &str = "@DM2:";

#[allow(dead_code)] // legacy @DM2-over-channel wrapper; kept for reference now that DMs are native unicast
fn wrap_dm_for_channel(
    dest_pubkey_prefix: &[u8; 6],
    sender_arch_prefix: &[u8; 6],
    inner: &[u8],
) -> String {
    use base64::Engine;
    let mut body = Vec::with_capacity(12 + inner.len());
    body.extend_from_slice(dest_pubkey_prefix);
    body.extend_from_slice(sender_arch_prefix);
    body.extend_from_slice(inner);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&body);
    format!("{}{}", DM_V2_MARKER, b64)
}

/// Compute our sender_arch_prefix from `state.our_ed_pubkey_hex`. Returns
/// `[0u8; 6]` if the stored hex is malformed (which would only happen if a
/// caller constructed `MeshState` with a bad value — empty string yields
/// all-zero, which won't match any real peer on the receiver side).
#[allow(dead_code)] // was used by the @DM2 wrapper; native unicast doesn't need it
fn our_sender_prefix(state: &Arc<MeshState>) -> [u8; 6] {
    let mut out = [0u8; 6];
    if state.our_ed_pubkey_hex.len() >= 12 {
        if let Ok(bytes) = hex::decode(&state.our_ed_pubkey_hex[..12]) {
            if bytes.len() == 6 {
                out.copy_from_slice(&bytes);
            }
        }
    }
    out
}

/// Send an arbitrary-size payload as one or more DM-via-channel frames.
/// Single-frame if the wrapped wire fits under the LoRa budget; otherwise
/// chunked as `MCxxyyzz…` base64 frames (receiver reassembles via
/// `try_chunk_reassemble`). Each chunk is independently `@DM2:`-wrapped
/// with both the recipient and sender prefixes so attribution works on
/// the receiver side.
async fn send_dm_via_channel(
    device: &mut MeshRadioDevice,
    state: &Arc<MeshState>,
    dest_pubkey_prefix: &[u8; 6],
    payload: &[u8],
    consecutive_write_failures: &mut u32,
) {
    use base64::Engine;
    let _ = state; // native unicast carries no separate sender prefix
                   // NATIVE meshcore unicast (CMD_SEND_TXT_MSG): a real direct message to the
                   // contact, NOT a broadcast on the shared public channel. This is the fix
                   // for the long-standing public-channel pollution — archy used to tunnel
                   // every DM/relay/receipt as an `@DM2:` blob on channel 0, which (a) every
                   // mesh participant saw as spam and (b) stock meshcore clients (e.g. a
                   // phone) couldn't decode. A native DM is private and decodes everywhere.
                   // The receive side handles these via the existing RESP_CONTACT_MSG path.
                   //
                   // Small payloads send in one frame; larger ones are base64 + MC-chunked
                   // and reassembled by the receiver (try_chunk_reassemble).
    if payload.len() <= 140 {
        match device.send_text_msg(dest_pubkey_prefix, payload).await {
            Ok(()) => {
                *consecutive_write_failures = 0;
                info!(
                    dest = %hex::encode(dest_pubkey_prefix),
                    len = payload.len(),
                    "Sent mesh DM (native unicast)"
                );
            }
            Err(e) => {
                *consecutive_write_failures += 1;
                warn!(
                    failures = *consecutive_write_failures,
                    "Failed to send native DM: {}", e
                );
            }
        }
        return;
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(payload);
    static CHUNK_MSG_ID: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
    let msg_id = CHUNK_MSG_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let chunk_data_size = 100;
    let chunks: Vec<&str> = encoded
        .as_bytes()
        .chunks(chunk_data_size)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    let total = chunks.len() as u8;
    info!(
        dest = %hex::encode(dest_pubkey_prefix),
        raw_len = payload.len(),
        b64_len = encoded.len(),
        chunks = total,
        "Sending chunked mesh DM (native unicast)"
    );
    let mut any_err = false;
    for (idx, chunk) in chunks.iter().enumerate() {
        let frame = format!("MC{:02x}{:02x}{:02x}{}", msg_id, idx as u8, total, chunk);
        if let Err(e) = device
            .send_text_msg(dest_pubkey_prefix, frame.as_bytes())
            .await
        {
            *consecutive_write_failures += 1;
            warn!(
                failures = *consecutive_write_failures,
                chunk = idx,
                "Chunk native DM send failed: {}",
                e
            );
            any_err = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    if !any_err {
        *consecutive_write_failures = 0;
    }
}

/// Send PLAIN text to a stock meshcore client as one or more native DMs.
/// Unlike `send_dm_via_channel`, this never uses MC-chunk framing (stock
/// clients can't reassemble it) — if the text exceeds one LoRa frame it is
/// split into multiple readable plain messages on UTF-8 char boundaries.
async fn send_plain_native_text(
    device: &mut MeshRadioDevice,
    dest_pubkey_prefix: &[u8; 6],
    text: &[u8],
    consecutive_write_failures: &mut u32,
) {
    // Split on char boundaries so we never break a multi-byte UTF-8 sequence.
    const FRAME: usize = 150; // under MAX_MESSAGE_LEN (160), leaves header room
    let s = String::from_utf8_lossy(text);
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if cur.len() + ch.len_utf8() > FRAME {
            parts.push(std::mem::take(&mut cur));
        }
        cur.push(ch);
    }
    if !cur.is_empty() || parts.is_empty() {
        parts.push(cur);
    }
    let total = parts.len();
    for (idx, part) in parts.iter().enumerate() {
        match device
            .send_text_msg(dest_pubkey_prefix, part.as_bytes())
            .await
        {
            Ok(()) => {
                *consecutive_write_failures = 0;
                info!(
                    dest = %hex::encode(dest_pubkey_prefix),
                    part = idx + 1,
                    total,
                    "Sent plain native DM"
                );
            }
            Err(e) => {
                *consecutive_write_failures += 1;
                warn!(
                    failures = *consecutive_write_failures,
                    "Plain native DM send failed: {}", e
                );
                break;
            }
        }
        if total > 1 {
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    }
}

/// Fetch the contacts list from the device and update the peer cache.
async fn refresh_contacts(device: &mut MeshRadioDevice, state: &Arc<MeshState>) {
    match device.get_contacts().await {
        Ok(contacts) => {
            // Contact blocking is intentionally NOT applied here. A read-time
            // blocklist meant a wiped/re-paired contact could never come back
            // even when it re-advertised (it broke phone re-pairing after a
            // clear). Per-contact blocking will return later as an explicit,
            // user-controlled feature; until then every firmware contact is
            // surfaced. `radio_contact_blocklist` is retained but unused.
            let mut peers = state.peers.write().await;
            let is_meshtastic = matches!(device.device_type(), DeviceType::Meshtastic);
            let is_reticulum = matches!(device.device_type(), DeviceType::Reticulum);
            for (idx, contact) in contacts.iter().enumerate() {
                let contact_id = if is_meshtastic {
                    meshtastic_contact_id(&contact.public_key_hex).unwrap_or(idx as u32)
                } else if is_reticulum {
                    reticulum_contact_id(&contact.public_key_hex).unwrap_or(idx as u32)
                } else {
                    idx as u32
                };
                let existing = peers.get(&contact_id);
                let peer = super::super::types::MeshPeer {
                    contact_id,
                    advert_name: contact.advert_name.clone(),
                    did: existing.and_then(|p| p.did.clone()),
                    pubkey_hex: Some(contact.public_key_hex.clone()),
                    // Reticulum carries the archipelago identity in-band with
                    // the contact snapshot itself (see `ParsedContact::
                    // arch_pubkey_hex`'s doc comment) — prefer it when this
                    // refresh's snapshot has one. Otherwise preserve whatever
                    // was bound by an earlier identity advert (Meshcore/
                    // Meshtastic's `bind_federation_twins` path): NEVER
                    // overwrite a known identity with the firmware contact
                    // key, or a signed `!ai` query from this peer would fail
                    // authentication after the next contact refresh.
                    arch_pubkey_hex: contact
                        .arch_pubkey_hex
                        .clone()
                        .or_else(|| existing.and_then(|p| p.arch_pubkey_hex.clone())),
                    x25519_pubkey: existing.and_then(|p| p.x25519_pubkey),
                    // Meshtastic-only today (see ParsedContact) — falls back to
                    // whatever was already known if this refresh's contact
                    // snapshot doesn't carry a fresher reading (it always does
                    // for Meshtastic, since packet_to_inbound_frame updates the
                    // live contacts map on every heard packet; this fallback
                    // just avoids flapping to None on a transitional refresh).
                    rssi: contact.rssi.or_else(|| existing.and_then(|p| p.rssi)),
                    snr: contact.snr.or_else(|| existing.and_then(|p| p.snr)),
                    last_heard: chrono::Utc::now().to_rfc3339(),
                    hops: 0,
                    last_advert: contact.last_advert,
                    // A non-zero path_len means the firmware has a route (direct
                    // or flood) to this contact — i.e. we can deliver to it.
                    reachable: contact.path_len != 0,
                    // E2E capability only grows (once the radio learns a peer's
                    // PKI key it stays known), so OR with any prior value rather
                    // than letting a transient contact refresh clear the pill.
                    pkc_capable: contact.pkc_capable
                        || existing.map(|p| p.pkc_capable).unwrap_or(false),
                    // Position only ever improves to a fresher fix; never clear
                    // it just because a refresh's snapshot didn't carry one.
                    lat: contact.lat.or_else(|| existing.and_then(|p| p.lat)),
                    lon: contact.lon.or_else(|| existing.and_then(|p| p.lon)),
                    // A contact refresh carries no Lightning datum — it comes
                    // only from an explicit LightningInfo advert. Preserve what
                    // was advertised, or a routine refresh would silently empty
                    // the channel-open picker.
                    lightning_uri: existing.and_then(|p| p.lightning_uri.clone()),
                };
                peers.insert(contact_id, peer);
            }
            // A radio contact that shares an exact advert_name with a known
            // federation peer is the same physical node — bind the federation
            // peer's archipelago identity onto the radio record so a signed
            // `!ai`/typed message over LoRa authenticates (and the contact stops
            // showing as a radio/federation duplicate). Security is unchanged:
            // the bound key is only a candidate the inbound signature must still
            // verify against. See `bind_federation_twins`.
            super::super::bind_federation_twins(&mut peers);
            drop(peers);
            state.update_peer_count().await;
            if !contacts.is_empty() {
                info!(count = contacts.len(), "Refreshed mesh contacts");
                // Auto-heal routing: any type-1 (friend) contact whose stored
                // path_len is 0 has no route established, which makes the
                // meshcore firmware silently drop outbound TXT_MSGs to it.
                // Flip those to OUT_PATH_UNKNOWN (0xFF) via CMD_RESET_PATH so
                // the firmware falls back to flood routing. Advert discovery
                // will fill in a real path later if one becomes available.
                for c in contacts.iter() {
                    if c.contact_type != 1 || c.path_len != 0 {
                        continue;
                    }
                    let pk_bytes = match hex::decode(&c.public_key_hex) {
                        Ok(b) if b.len() == 32 => {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&b);
                            arr
                        }
                        _ => continue,
                    };
                    match device.reset_contact_path(&pk_bytes).await {
                        Ok(()) => info!(
                            name = %c.advert_name,
                            "Reset contact path → flood (was path_len=0)"
                        ),
                        Err(e) => warn!(
                            name = %c.advert_name,
                            "reset_contact_path failed: {}",
                            e
                        ),
                    }
                }
            }
        }
        Err(e) => {
            warn!("Failed to fetch contacts: {}", e);
        }
    }
}

fn meshtastic_contact_id(public_key_hex: &str) -> Option<u32> {
    let bytes = hex::decode(public_key_hex).ok()?;
    if bytes.len() < 15 || &bytes[4..15] != b"meshtastic:" {
        return None;
    }
    let node_num = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if node_num == 0 || node_num == u32::MAX {
        None
    } else {
        Some(node_num)
    }
}

/// Stable `u32` contact id derived from a Reticulum contact's `public_key_hex`
/// (hex of the 16-byte RNS destination hash). Delegates to the canonical
/// derivation in `reticulum.rs` so there is exactly one masking rule (must
/// stay below `FEDERATION_CONTACT_ID_BASE`, mod.rs:53) shared with
/// `ReticulumLink::initialize()`'s reported `node_id`.
fn reticulum_contact_id(public_key_hex: &str) -> Option<u32> {
    let bytes = hex::decode(public_key_hex).ok()?;
    let hash: [u8; 16] = bytes.try_into().ok()?;
    Some(super::super::reticulum::reticulum_contact_id_from_hash(
        &hash,
    ))
}

/// Drain any queued messages from the device.
/// Returns `true` if a write/communication error occurred (for failure tracking).
async fn sync_queued_messages(
    device: &mut MeshRadioDevice,
    state: &Arc<MeshState>,
    our_x25519_secret: &[u8; 32],
) -> bool {
    match device.sync_messages().await {
        Ok(frames) => {
            for frame in &frames {
                frames::handle_frame(frame, state, our_x25519_secret).await;
            }
            if !frames.is_empty() {
                info!(count = frames.len(), "Synced queued mesh messages");
            }
            false
        }
        Err(e) => {
            debug!("Message sync: {}", e);
            true
        }
    }
}

/// How many times we will try to write the LoRa region across reconnects before
/// giving up. A healthy radio accepts it on the first try (the reboot-and-verify
/// resolves on the next session). A radio that silently refuses to persist
/// config — corrupt/full flash, managed mode, etc. — would otherwise reboot-loop
/// forever; after this many attempts we stop, log, and run without it.
const MAX_REGION_PROVISION_ATTEMPTS: u32 = 3;

/// Process-global count of LoRa-region writes attempted (one radio per process).
/// Reset to 0 whenever the radio reports the desired region, so genuine later
/// drift re-provisions but a broken radio doesn't loop.
static REGION_PROVISION_ATTEMPTS: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

/// Same retry-cap idea as the region, for the Meshcore radio-params write.
static RADIO_PARAMS_PROVISION_ATTEMPTS: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

/// Same retry-cap idea as the region, for the shared-channel write.
static CHANNEL_PROVISION_ATTEMPTS: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

/// Run a single mesh session (connect, initialize, main loop).
pub(super) async fn run_mesh_session(
    state: &Arc<MeshState>,
    data_dir: &Path,
    preferred_path: Option<&str>,
    our_did: &str,
    our_ed_pubkey_hex: &str,
    our_x25519_secret: &[u8; 32],
    our_x25519_pubkey_hex: &str,
    server_name: Option<&str>,
    lora_region: Option<&str>,
    lora_radio_params: Option<crate::mesh::LoraRadioParams>,
    channel_name: Option<&str>,
    device_kind: Option<DeviceType>,
    reticulum_tcp: Option<ReticulumTcpConfig>,
    manage_radio: bool,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
    cmd_rx: &mut mpsc::Receiver<MeshCommand>,
) -> Result<()> {
    // Detect device — TCP Reticulum config (radio-less) takes priority when
    // set, otherwise try the preferred serial path, falling back to
    // auto-detect. TCP mode is additive/dev-only; it never changes behavior
    // for existing serial/RNode deployments where `reticulum_tcp` is None.
    //
    // The name we present on the mesh: the operator's configured mesh name /
    // server name, falling back to a DID fragment. Computed BEFORE the open
    // sequence because Reticulum needs it at daemon-spawn time — the RNS
    // announce carries it from the very first announce. (Meshcore/Meshtastic
    // still receive it via set_advert_name after connect, below.)
    let desired_advert_name: String = match server_name {
        // Meshcore firmware limits advert names — truncate to 20 chars.
        Some(name) => name.chars().take(20).collect(),
        None => format!(
            "Archy-{}",
            our_did.chars().skip(8).take(8).collect::<String>()
        ),
    };

    // The whole open sequence runs under PORT_OPEN_LOCK so an RPC probe
    // can't interleave its own handshakes on the same tty (see the lock's
    // doc comment). Held only until the device is opened, then released.
    //
    // The sequence is raced against the shutdown signal: probes/handshakes
    // can take 10s+, and without this a stop() issued mid-probe (config
    // change, disable, rename) always burned the full listener-shutdown
    // timeout and ended in a hard abort — observed live on a test node
    // 2026-07-28. Dropping the open future mid-probe is safe: it holds no
    // session state yet and the port guard/serial handle close with it.
    let open_fut = async {
        let port_guard = PORT_OPEN_LOCK.lock().await;
        let result = if let Some(tcp_cfg) = &reticulum_tcp {
            open_reticulum_tcp(
                tcp_cfg,
                data_dir,
                our_ed_pubkey_hex,
                our_x25519_pubkey_hex,
                Some(&desired_advert_name),
            )
            .await
        } else if let Some(path) = preferred_path {
            if device_kind.is_none() {
                // Unpinned: open_preferred_path bails without ever touching
                // the port, so the preferred path has NOT been probed this
                // cycle — auto-detect must keep it as a candidate
                // (skip_path=None). Passing Some(path) here excluded the only
                // radio on single-device nodes and the mesh never came up.
                auto_detect_and_open(
                    data_dir,
                    our_ed_pubkey_hex,
                    our_x25519_pubkey_hex,
                    device_kind,
                    None,
                    Some(&desired_advert_name),
                )
                .await
            } else {
                match open_preferred_path(
                    path,
                    data_dir,
                    our_ed_pubkey_hex,
                    our_x25519_pubkey_hex,
                    device_kind,
                    Some(&desired_advert_name),
                )
                .await
                {
                    Ok((dev, info)) => Ok((path.to_string(), dev, info)),
                    Err(e) => {
                        warn!(
                            "Preferred path {} probe failed: {} — trying auto-detect",
                            path, e
                        );
                        auto_detect_and_open(
                            data_dir,
                            our_ed_pubkey_hex,
                            our_x25519_pubkey_hex,
                            device_kind,
                            Some(path),
                            Some(&desired_advert_name),
                        )
                        .await
                    }
                }
            }
        } else {
            auto_detect_and_open(
                data_dir,
                our_ed_pubkey_hex,
                our_x25519_pubkey_hex,
                device_kind,
                None,
                Some(&desired_advert_name),
            )
            .await
        };
        drop(port_guard);
        result
    };
    let (device_path, mut device, device_info) = tokio::select! {
        res = open_fut => res?,
        _ = shutdown.changed() => {
            if *shutdown.borrow() {
                info!("Shutdown requested during device open — ending session");
                return Ok(());
            }
            anyhow::bail!("shutdown signal changed during device open");
        }
    };

    // Update status
    {
        let mut status = state.status.write().await;
        status.device_connected = true;
        status.device_type = device.device_type();
        status.device_path = Some(device_path.clone());
        status.firmware_version = Some(device_info.firmware_version.clone());
        status.self_node_id = Some(device_info.node_id);
        status.self_advert_name = device.advert_name();
    }

    let _ = state.event_tx.send(MeshEvent::DeviceConnected(device_info));

    // Provision the LoRa region before anything else. A fresh Meshtastic radio
    // is region-UNSET and therefore RF-silent — it can neither hear nor be
    // heard, so contact discovery and DMs would all silently fail. If we write
    // a new region the firmware reboots to apply it; restart the session so we
    // re-handshake the freshly-rebooted radio (and then set its name on the
    // reconnect, where the region already matches and no reboot occurs).
    use std::sync::atomic::Ordering;
    if !manage_radio {
        // Keep-as-is mode (hot-swap "keep as is" decision): the operator told
        // us to use whatever is flashed/configured on this radio. Skip every
        // config write below — region, channel, PHY params, advert name — and
        // run with the device's own settings.
        info!("manage_radio=false — leaving the radio's own configuration untouched");
    }
    let region_attempts = REGION_PROVISION_ATTEMPTS.load(Ordering::Relaxed);
    if manage_radio && region_attempts < MAX_REGION_PROVISION_ATTEMPTS {
        match device.ensure_lora_region(lora_region).await {
            Ok(true) => {
                REGION_PROVISION_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
                info!(
                    region = lora_region.unwrap_or(""),
                    attempt = region_attempts + 1,
                    max = MAX_REGION_PROVISION_ATTEMPTS,
                    "Provisioned LoRa region — radio rebooting, restarting mesh session"
                );
                // Give the radio time to reboot before the reconnect re-opens it.
                tokio::time::sleep(Duration::from_secs(10)).await;
                return Ok(());
            }
            // Radio reports the desired region (or none configured): clear the
            // attempt counter so a future genuine drift re-provisions cleanly.
            Ok(false) => REGION_PROVISION_ATTEMPTS.store(0, Ordering::Relaxed),
            Err(e) => warn!("Failed to provision LoRa region: {}", e),
        }
    } else if manage_radio && lora_region.is_some() {
        warn!(
            region = lora_region.unwrap_or(""),
            attempts = MAX_REGION_PROVISION_ATTEMPTS,
            "Radio did not persist the configured LoRa region after repeated \
             attempts — continuing without it. The radio likely needs a manual \
             factory reset / reflash; mesh discovery stays offline until its \
             region is set."
        );
    }

    // Provision the shared primary channel (after the region, since both reboot
    // the radio). Without a matching channel two same-region radios still can't
    // decode each other's traffic. Same retry-cap + restart-on-change pattern.
    let channel_attempts = CHANNEL_PROVISION_ATTEMPTS.load(Ordering::Relaxed);
    if manage_radio && channel_attempts < MAX_REGION_PROVISION_ATTEMPTS {
        match device.ensure_channel(channel_name).await {
            Ok(true) => {
                CHANNEL_PROVISION_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
                info!(
                    channel = channel_name.unwrap_or(""),
                    attempt = channel_attempts + 1,
                    max = MAX_REGION_PROVISION_ATTEMPTS,
                    "Provisioned shared mesh channel — radio rebooting, restarting mesh session"
                );
                tokio::time::sleep(Duration::from_secs(10)).await;
                return Ok(());
            }
            Ok(false) => CHANNEL_PROVISION_ATTEMPTS.store(0, Ordering::Relaxed),
            Err(e) => warn!("Failed to provision mesh channel: {}", e),
        }
    } else if manage_radio && channel_name.is_some() {
        warn!(
            channel = channel_name.unwrap_or(""),
            attempts = MAX_REGION_PROVISION_ATTEMPTS,
            "Radio did not persist the shared mesh channel after repeated \
             attempts — continuing without it; the radio may need a manual reset."
        );
    }

    // Provision Meshcore LoRa PHY params (freq/bw/sf/cr) when the operator has
    // configured them. Meshcore-only: Meshtastic radios get region+preset via
    // ensure_lora_region above, and Reticulum carries its own RNode profile.
    // Gated on a persisted marker of the last-applied params rather than the
    // device's SELF_INFO readback (its field offsets shift across firmware
    // versions), so we send the set-command once per configured value and never
    // reboot-loop a radio that refuses it. The firmware reboots on RESP_OK, so
    // a successful write restarts the session like the region path.
    if let (true, Some(params), MeshRadioDevice::Meshcore(dev)) =
        (manage_radio, lora_radio_params, &mut device)
    {
        let marker_path = data_dir.join("meshcore-radio-params.json");
        let applied: Option<crate::mesh::LoraRadioParams> = tokio::fs::read(&marker_path)
            .await
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        if applied != Some(params) {
            let attempts = RADIO_PARAMS_PROVISION_ATTEMPTS.load(Ordering::Relaxed);
            if attempts < MAX_REGION_PROVISION_ATTEMPTS {
                match dev
                    .set_radio_params(params.freq_khz, params.bw_hz, params.sf, params.cr)
                    .await
                {
                    Ok(()) => {
                        RADIO_PARAMS_PROVISION_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
                        if let Ok(json) = serde_json::to_vec(&params) {
                            if let Err(e) = tokio::fs::write(&marker_path, json).await {
                                warn!("Failed to persist radio-params marker: {}", e);
                            }
                        }
                        info!(
                            freq_khz = params.freq_khz,
                            bw_hz = params.bw_hz,
                            sf = params.sf,
                            cr = params.cr,
                            "Provisioned Meshcore radio params — radio rebooting, \
                             restarting mesh session"
                        );
                        tokio::time::sleep(Duration::from_secs(10)).await;
                        return Ok(());
                    }
                    Err(e) => {
                        RADIO_PARAMS_PROVISION_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
                        warn!("Failed to provision Meshcore radio params: {}", e);
                    }
                }
            } else {
                warn!(
                    attempts = MAX_REGION_PROVISION_ATTEMPTS,
                    "Meshcore radio rejected the configured radio params after \
                     repeated attempts — continuing with the device's own settings."
                );
            }
        } else {
            RADIO_PARAMS_PROVISION_ATTEMPTS.store(0, Ordering::Relaxed);
        }
    }

    // Set advert name to the configured mesh/server name (computed above).
    // Skipped in keep-as-is mode for radio-held names — the radio keeps the
    // name it came with (already reflected in status from the connect
    // handshake). Reticulum is exempt from keep-as-is: its display name
    // lives in OUR daemon (the RNode holds no name), so "keep as is" has
    // nothing to preserve and an unnamed node would be anonymous on RNS.
    if manage_radio || matches!(device, MeshRadioDevice::Reticulum(_)) {
        if let Err(e) = device.set_advert_name(&desired_advert_name).await {
            warn!("Failed to set advert name: {}", e);
        } else {
            // Reflect the post-set name in MeshStatus too so the UI can filter
            // its own radio echo from the peer list. Without this, the status
            // still carries whatever pre-set name the firmware reported and the
            // self-filter never matches.
            let mut status = state.status.write().await;
            status.self_advert_name = Some(desired_advert_name.clone());
        }
    }

    // Broadcast our advertisement so other nodes can discover us
    if let Err(e) = device.send_self_advert().await {
        warn!("Failed to send initial advert: {}", e);
    }
    // Actively announce our identity over the air with want_response, so any
    // already-running neighbour both learns about us and replies with its own
    // NodeInfo — immediate two-way discovery instead of waiting for the radio's
    // multi-hour NodeInfo cycle. (No-op for meshcore.)
    if let Err(e) = device.send_nodeinfo_advert(true).await {
        warn!("Failed to send initial NodeInfo advert: {}", e);
    }

    // NOTE: Archipelago identity adverts (`ARCHY:2:{ed}:{x25519}`) are intentionally
    // NOT broadcast on the shared public channel (channel 0). Doing so spams every
    // participant on that channel — including plain Meshtastic/meshcore users who
    // just see raw `ARCHY:2:…` text — on startup and again on every advert tick.
    // The inbound parser in frames.rs still accepts these from any legacy peer that
    // sends them, so trust-binding keeps working when a peer advertises; we simply
    // don't pollute the public channel ourselves. A dedicated control channel (or a
    // DM-targeted handshake) is the proper transport for this and is tracked
    // separately. See encode_identity_broadcast / parse_identity_broadcast.
    let _ = (our_did, our_ed_pubkey_hex, our_x25519_pubkey_hex);

    // Fetch existing contacts from the device
    refresh_contacts(&mut device, state).await;

    // Sync any queued messages from before we connected
    let _ = sync_queued_messages(&mut device, state, our_x25519_secret).await;

    // Main loop
    let mut advert_timer = tokio::time::interval(ADVERT_INTERVAL);
    let mut sync_timer = tokio::time::interval(SYNC_INTERVAL);
    advert_timer.tick().await; // skip first immediate tick
    sync_timer.tick().await;
    let mut consecutive_write_failures: u32 = 0;
    // Backlog #12 RX-stall watchdog — see RX_STALL_TIMEOUT's doc comment.
    // Reset on the very first frame check too (not just successful reads),
    // so a session that never receives anything still gets a full timeout
    // window from startup rather than an immediately-stale clock.
    let mut last_rx_at = Instant::now();

    loop {
        // If too many consecutive writes have failed, the serial port is dead —
        // bail out so the outer loop can reconnect to a (possibly re-enumerated) device.
        if consecutive_write_failures >= MAX_CONSECUTIVE_WRITE_FAILURES {
            error!(
                failures = consecutive_write_failures,
                "Serial port unresponsive — triggering reconnection"
            );
            anyhow::bail!(
                "Serial port unresponsive after {} consecutive write failures",
                consecutive_write_failures
            );
        }
        if last_rx_at.elapsed() >= RX_STALL_TIMEOUT {
            error!(
                stalled_for_secs = last_rx_at.elapsed().as_secs(),
                "No mesh frames received for too long — triggering reconnection"
            );
            anyhow::bail!(
                "RX stalled for over {}s — forcing reconnect",
                RX_STALL_TIMEOUT.as_secs()
            );
        }

        tokio::select! {
            // Check for incoming frames
            frame_result = device.try_recv_frame() => {
                match frame_result {
                    Ok(Some(frame)) => {
                        // Successful read resets the failure counter and the
                        // RX-stall watchdog.
                        consecutive_write_failures = 0;
                        last_rx_at = Instant::now();
                        // For meshtastic, the PKI-E2E status of this frame can't
                        // ride the synthetic meshcore frame — snapshot the message
                        // id high-water mark, dispatch, then stamp the E2E pill on
                        // whatever received message this frame produced.
                        let before_id = dispatch::max_message_id(state).await;
                        let should_action = frames::handle_frame(
                            &frame,
                            state,
                            our_x25519_secret,
                        ).await;
                        if device.take_rx_encrypted() {
                            dispatch::stamp_received_encrypted(state, before_id).await;
                        }
                        if should_action {
                            // Contact discovery or messages waiting — sync both
                            refresh_contacts(&mut device, state).await;
                            if sync_queued_messages(&mut device, state, our_x25519_secret).await {
                                consecutive_write_failures += 1;
                            }
                        }
                    }
                    Ok(None) => {
                        // No complete frame yet, that's fine
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    Err(e) => {
                        error!("Serial read error: {}", e);
                        return Err(e);
                    }
                }
            }

            // Periodic advertisement broadcast + contact refresh
            _ = advert_timer.tick() => {
                debug!("Periodic self-advert broadcast");
                if let Err(e) = device.send_self_advert().await {
                    consecutive_write_failures += 1;
                    warn!(failures = consecutive_write_failures, "Failed to send advert: {}", e);
                } else {
                    consecutive_write_failures = 0;
                }
                // Periodic over-air identity beacon (no want_response, to avoid
                // reply storms) so peers that come online later still discover
                // us between the radio's own infrequent NodeInfo broadcasts.
                // No-op for meshcore (its self-advert above already goes out).
                if let Err(e) = device.send_nodeinfo_advert(false).await {
                    debug!("Periodic NodeInfo advert failed: {}", e);
                }
                // (Identity re-broadcast on the public channel intentionally
                // removed — see the note at session startup. It spammed the
                // shared channel every advert tick.)
                refresh_contacts(&mut device, state).await;
            }

            // Process send commands from MeshService
            Some(cmd) = cmd_rx.recv() => {
                handle_send_command(cmd, &mut device, state, &mut consecutive_write_failures).await;
            }

            // Periodic message sync + serial keepalive
            _ = sync_timer.tick() => {
                // Keep the radio streaming inbound packets to our serial client
                // (best-effort — a failed keepalive shouldn't trip the reconnect
                // counter on its own; a truly dead port is caught by real writes).
                if let Err(e) = device.send_keepalive().await {
                    debug!("Mesh keepalive failed: {}", e);
                }
                if sync_queued_messages(&mut device, state, our_x25519_secret).await {
                    consecutive_write_failures += 1;
                    debug!(failures = consecutive_write_failures, "Message sync failed");
                } else {
                    consecutive_write_failures = 0;
                }
            }

            // Shutdown signal
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    info!("Mesh listener received shutdown signal");
                    return Ok(());
                }
            }
        }
    }
}

/// Process a single outbound command from MeshService.
async fn handle_send_command(
    cmd: MeshCommand,
    device: &mut MeshRadioDevice,
    state: &Arc<MeshState>,
    consecutive_write_failures: &mut u32,
) {
    match cmd {
        MeshCommand::SendText {
            dest_pubkey_prefix,
            payload,
        } => {
            send_dm_via_channel(
                device,
                state,
                &dest_pubkey_prefix,
                &payload,
                consecutive_write_failures,
            )
            .await;
        }
        MeshCommand::SendNativeText {
            dest_pubkey_prefix,
            payload,
        } => {
            send_plain_native_text(
                device,
                &dest_pubkey_prefix,
                &payload,
                consecutive_write_failures,
            )
            .await;
        }
        MeshCommand::SendRaw {
            dest_pubkey_prefix,
            payload,
        } => {
            let wire_payload = if state.stego_mode
                != super::super::steganography::SteganographyMode::Normal
                && payload.first() == Some(&super::super::message_types::TYPED_MESSAGE_MARKER)
            {
                match super::super::steganography::encode_typed_wire(state.stego_mode, &payload) {
                    Ok(stego) => stego,
                    Err(e) => {
                        warn!("Stego encode failed, sending plain: {}", e);
                        payload
                    }
                }
            } else {
                payload
            };
            send_dm_via_channel(
                device,
                state,
                &dest_pubkey_prefix,
                &wire_payload,
                consecutive_write_failures,
            )
            .await;
        }
        MeshCommand::SendResource {
            dest_pubkey_prefix,
            payload,
        } => {
            // No MC-chunk framing here — RNS Resources do their own native
            // chunked transfer at the link layer, so the payload goes through
            // as-is (the receiving daemon hands back the complete blob in one
            // `resource_recv` event).
            if let Err(e) = device.send_resource(&dest_pubkey_prefix, &payload).await {
                *consecutive_write_failures += 1;
                warn!(
                    failures = *consecutive_write_failures,
                    "Failed to send Reticulum resource: {}", e
                );
            } else {
                *consecutive_write_failures = 0;
                info!(
                    dest = %hex::encode(dest_pubkey_prefix),
                    len = payload.len(),
                    "Sent Reticulum resource transfer"
                );
            }
        }
        MeshCommand::SendNativeImage {
            dest_pubkey_prefix,
            mime,
            bytes,
            caption,
        } => {
            if let Err(e) = device
                .send_native_image(&dest_pubkey_prefix, &mime, &bytes, caption.as_deref())
                .await
            {
                *consecutive_write_failures += 1;
                warn!(
                    failures = *consecutive_write_failures,
                    "Failed to send native image: {}", e
                );
            } else {
                *consecutive_write_failures = 0;
                info!(
                    dest = %hex::encode(dest_pubkey_prefix),
                    len = bytes.len(),
                    "Sent native LXMF image"
                );
            }
        }
        MeshCommand::BroadcastChannel { channel, payload } => {
            if let Err(e) = device.send_channel_text(channel, &payload).await {
                *consecutive_write_failures += 1;
                warn!(
                    failures = *consecutive_write_failures,
                    "Failed to broadcast on channel {}: {}", channel, e
                );
            } else {
                *consecutive_write_failures = 0;
                info!(channel, len = payload.len(), "Broadcast on mesh channel");
            }
        }
        MeshCommand::SendAdvert => {
            if let Err(e) = device.send_self_advert().await {
                *consecutive_write_failures += 1;
                warn!(
                    failures = *consecutive_write_failures,
                    "Failed to send advert: {}", e
                );
            } else {
                *consecutive_write_failures = 0;
            }
            // The self-advert alone is a no-op for discovery on Meshtastic
            // (heartbeat + time carry no identity) — the NodeInfo broadcast
            // is what makes peers learn/refresh us. want_response=true so
            // neighbours answer with their own NodeInfo: the user pressed
            // Broadcast to be seen AND to see who's out there. No-op on
            // Meshcore/Reticulum, whose self-advert already carries identity.
            if let Err(e) = device.send_nodeinfo_advert(true).await {
                warn!("Failed to send NodeInfo advert: {}", e);
            }
        }
        MeshCommand::RebootRadio { seconds, reply } => {
            let outcome = device.reboot(seconds).await;
            match &outcome {
                Err(e) => warn!("Failed to reboot radio: {}", e),
                Ok(_) => info!(seconds, "Radio reboot command sent to device"),
            }
            if let Some(reply) = reply {
                let _ = reply.send(outcome.map_err(|e| format!("{e:#}")));
            }
        }
        MeshCommand::QueryRadioState { reply } => {
            let _ = reply.send(device.radio_state().await.map_err(|e| format!("{e:#}")));
        }
        MeshCommand::RefreshContacts => {
            refresh_contacts(device, state).await;
        }
        MeshCommand::RemoveContact { pubkey } => {
            if let Err(e) = device.remove_contact(&pubkey).await {
                warn!(pubkey = %hex::encode(pubkey), "remove_contact failed: {}", e);
            } else {
                info!(pubkey = %hex::encode(&pubkey[..6]), "Removed firmware contact");
            }
        }
        MeshCommand::AddContact { pubkey, name } => {
            // type=1 (chat/user), flags=0, out_path_len=0 (firmware will flood
            // until a path is learned). last_advert=0 lets the firmware keep its
            // own advert timestamp.
            if let Err(e) = device.add_contact(&pubkey, 1, 0, 0, &name, 0).await {
                warn!(pubkey = %hex::encode(&pubkey[..6]), "add_contact failed: {}", e);
            } else {
                info!(pubkey = %hex::encode(&pubkey[..6]), "Imported advert as contact");
            }
        }
    }
}
