// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Async serial driver for Meshcore devices.
//!
//! Handles opening the serial port, reading/writing frames,
//! and the initialization handshake sequence.

use super::protocol::{self, InboundFrame};
use super::types::DeviceInfo;
use anyhow::{Context, Result};
use std::path::Path;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Serial port configuration for Meshcore Companion USB.
const BAUD_RATE: u32 = 115200;

/// Timeout for reading a response frame from the device.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Timeout for writing a frame to the device.
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

/// Buffer size for serial reads.
const READ_BUF_SIZE: usize = 512;

/// Application name sent during handshake.
const APP_NAME: &str = "Archipelago";

/// Async Meshcore device handle.
pub struct MeshcoreDevice {
    port: serial2_tokio::SerialPort,
    read_buf: Vec<u8>,
    pub node_id: Option<u32>,
    pub advert_name: Option<String>,
    pub device_info: Option<DeviceInfo>,
    device_path: String,
}

impl MeshcoreDevice {
    /// Open a serial port and verify it's a Meshcore device.
    pub async fn open(path: &str) -> Result<Self> {
        // Check device exists before trying to open (better error message)
        match tokio::fs::metadata(path).await {
            Ok(meta) => {
                debug!(path = %path, permissions = ?meta.permissions(), "Device node exists");
            }
            Err(e) => {
                anyhow::bail!(
                    "Serial device {} not accessible: {} (check PrivateDevices in systemd, or USB connection)",
                    path, e
                );
            }
        }

        let port = serial2_tokio::SerialPort::open(path, BAUD_RATE).context(format!(
            "Failed to open serial port {} (permission denied? device busy?)",
            path
        ))?;
        // See probe_rnode() in reticulum.rs for why: ESP32-S3 native-USB
        // boards (and CP2102/CH340-bridged boards wired for Arduino-style
        // auto-reset) reset on a DTR/RTS transition, so deassert both and
        // settle before the handshake below. 300ms is nowhere near a real
        // firmware boot time (LoRa radio init alone can take longer) —
        // confirmed live 2026-07-23: with every one of Reticulum/Meshcore/
        // Meshtastic's open() doing this same reset, a single auto-detect
        // cycle trying multiple protocols in sequence kept re-resetting the
        // board before it ever finished booting from the PREVIOUS attempt's
        // reset, on both a Heltec V3 and V4, regardless of firmware family —
        // a self-sustaining "never finishes booting" loop with a boot-time
        // root cause hiding behind what looked like a per-protocol failure.
        let _ = port.set_dtr(false);
        let _ = port.set_rts(false);
        tokio::time::sleep(Duration::from_millis(2000)).await;

        info!(path = %path, baud = BAUD_RATE, "Opened serial port");

        Ok(Self {
            port,
            read_buf: Vec::with_capacity(READ_BUF_SIZE),
            node_id: None,
            advert_name: None,
            device_info: None,
            device_path: path.to_string(),
        })
    }

    /// Run the Meshcore initialization handshake.
    /// Matches the official meshcore_py library sequence:
    /// 1. CMD_APP_START -> RESP_SELF_INFO (this is the first command, not device_query)
    /// 2. CMD_SET_DEVICE_TIME (sync clock)
    pub async fn initialize(&mut self) -> Result<DeviceInfo> {
        info!("Starting Meshcore handshake on {}", self.device_path);

        // Step 1: App start (the official library sends this first)
        self.send_raw(&protocol::build_app_start(APP_NAME)).await?;

        let frame = self
            .recv_frame_timeout(READ_TIMEOUT)
            .await
            .context("No response to APP_START — is this a Meshcore Companion USB device?")?;

        info!(
            code = frame.code,
            data_len = frame.data.len(),
            "Got response to APP_START"
        );

        if frame.code == protocol::RESP_ERR {
            anyhow::bail!("App start failed: {}", protocol::parse_error(&frame.data));
        }

        // The response could be SELF_INFO or something else depending on firmware version
        let (node_id, name) = if frame.code == protocol::RESP_SELF_INFO {
            protocol::parse_self_info(&frame.data).context("Failed to parse self info")?
        } else {
            // Try to parse whatever we got
            info!(
                code = frame.code,
                "Unexpected response code, trying to parse as self info"
            );
            protocol::parse_self_info(&frame.data).unwrap_or((0, String::new()))
        };

        info!(node_id, name = %name, "Meshcore identity");

        self.node_id = Some(node_id);
        self.advert_name = Some(name.clone());

        // Step 2: Sync device clock
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.send_raw(&protocol::build_set_device_time(now)).await?;
        // Time set response is best-effort — don't fail if it times out
        match self.recv_frame_timeout(Duration::from_secs(2)).await {
            Ok(frame) if frame.code == protocol::RESP_OK => {
                debug!("Device clock synced");
            }
            Ok(frame) => {
                warn!(code = frame.code, "Unexpected response to SET_DEVICE_TIME");
            }
            Err(_) => {
                warn!("No response to SET_DEVICE_TIME (continuing anyway)");
            }
        }

        let info = DeviceInfo {
            firmware_version: name.clone(),
            node_id,
            max_contacts: 100,
            device_type: super::types::DeviceType::Meshcore,
        };
        self.device_info = Some(info.clone());

        info!("Meshcore initialization complete on {}", self.device_path);
        Ok(info)
    }

    /// The advert name learned from SELF_INFO during `initialize`.
    pub fn advert_name(&self) -> Option<String> {
        self.advert_name.clone()
    }

    /// Read firmware version + contact capacity via CMD_DEVICE_QUERY (0x16).
    /// Read-only — used by the hot-swap probe to show what's on a
    /// just-plugged radio. Tolerates interleaved push frames and firmware
    /// that doesn't answer the query (returns None rather than erroring).
    pub async fn query_device_info(&mut self) -> Option<(String, u16)> {
        if self
            .send_raw(&protocol::build_device_query())
            .await
            .is_err()
        {
            return None;
        }
        for _ in 0..5 {
            match self.recv_frame_timeout(Duration::from_secs(2)).await {
                Ok(f) if f.code == protocol::RESP_DEVICE_INFO => {
                    return protocol::parse_device_info(&f.data).ok();
                }
                Ok(_) => continue, // push notification — keep waiting
                Err(_) => return None,
            }
        }
        None
    }

    /// Set the advertised name on the mesh network.
    pub async fn set_advert_name(&mut self, name: &str) -> Result<()> {
        self.send_raw(&protocol::build_set_advert_name(name))
            .await?;
        let frame = self.recv_frame_timeout(READ_TIMEOUT).await?;
        if frame.code == protocol::RESP_ERR {
            anyhow::bail!(
                "Set advert name failed: {}",
                protocol::parse_error(&frame.data)
            );
        }
        self.advert_name = Some(name.to_string());
        Ok(())
    }

    /// Set the radio's LoRa PHY parameters (freq/bw/sf/cr, firmware field
    /// units — see `protocol::build_set_radio_params`). On RESP_OK the
    /// firmware persists the params and reboots to apply them, so the caller
    /// must treat the session as gone and reconnect.
    pub async fn set_radio_params(
        &mut self,
        freq_khz: u32,
        bw_hz: u32,
        sf: u8,
        cr: u8,
    ) -> Result<()> {
        self.send_raw(&protocol::build_set_radio_params(freq_khz, bw_hz, sf, cr))
            .await?;
        let frame = self.recv_frame_timeout(READ_TIMEOUT).await?;
        if frame.code == protocol::RESP_ERR {
            anyhow::bail!(
                "Set radio params failed: {}",
                protocol::parse_error(&frame.data)
            );
        }
        Ok(())
    }

    /// Broadcast our advertisement to the mesh.
    pub async fn send_self_advert(&mut self) -> Result<()> {
        self.send_raw(&protocol::build_send_self_advert()).await?;
        // Response is RESP_OK or RESP_SENT
        let frame = self.recv_frame_timeout(READ_TIMEOUT).await?;
        if frame.code == protocol::RESP_ERR {
            anyhow::bail!("Self advert failed: {}", protocol::parse_error(&frame.data));
        }
        Ok(())
    }

    /// Send a text message to a contact by their public key prefix (first 6 bytes).
    /// Returns whether the firmware routed it via flood (true) or direct (false).
    /// The response frame is `RESP_CODE_SENT | mode | tag[4] | est_timeout[4]`
    /// where mode == 1 means flood and mode == 0 means direct.
    pub async fn send_text(&mut self, dest_pubkey_prefix: &[u8; 6], msg: &[u8]) -> Result<bool> {
        let frame_data = protocol::build_send_text(dest_pubkey_prefix, msg)?;
        self.send_raw(&frame_data).await?;
        let frame = self.recv_frame_timeout(READ_TIMEOUT).await?;
        if frame.code == protocol::RESP_ERR {
            anyhow::bail!("Send text failed: {}", protocol::parse_error(&frame.data));
        }
        // RESP_CODE_SENT layout: [mode(1)][tag(4)][est_timeout(4)]
        let sent_via_flood = frame.data.first().copied().unwrap_or(0) == 1;
        tracing::info!(
            dest = %hex::encode(dest_pubkey_prefix),
            mode = if sent_via_flood { "flood" } else { "direct" },
            resp_code = frame.code,
            data_len = frame.data.len(),
            "[diag] send_text response"
        );
        Ok(sent_via_flood)
    }

    /// Broadcast a text message on a channel.
    pub async fn send_channel_text(&mut self, channel: u8, msg: &[u8]) -> Result<()> {
        let frame_data = protocol::build_send_channel_text(channel, msg)?;
        self.send_raw(&frame_data).await?;
        let frame = self.recv_frame_timeout(READ_TIMEOUT).await?;
        if frame.code == protocol::RESP_ERR {
            anyhow::bail!(
                "Channel broadcast failed: {}",
                protocol::parse_error(&frame.data)
            );
        }
        Ok(())
    }

    /// Send a NATIVE meshcore direct message (CMD_SEND_TXT_MSG) to a contact,
    /// addressed by the first 6 bytes of its public key. Unlike the
    /// `@DM2`-over-channel path, this is a real unicast — it does not appear on
    /// the public channel, and a stock meshcore client receives it as a normal
    /// DM. The contact must already exist in the firmware table (with a path).
    pub async fn send_text_msg(&mut self, dest_pubkey_prefix: &[u8; 6], msg: &[u8]) -> Result<()> {
        let frame_data = protocol::build_send_text(dest_pubkey_prefix, msg)?;
        self.send_raw(&frame_data).await?;
        let frame = self.recv_frame_timeout(READ_TIMEOUT).await?;
        if frame.code == protocol::RESP_ERR {
            anyhow::bail!(
                "Direct text send failed: {}",
                protocol::parse_error(&frame.data)
            );
        }
        Ok(())
    }

    /// Clear the stored routing path for a contact so the firmware flood-
    /// routes future messages instead of dropping them when path_len=0.
    pub async fn reset_contact_path(&mut self, pubkey: &[u8; 32]) -> Result<()> {
        self.send_raw(&protocol::build_reset_path(pubkey)).await?;
        let frame = self.recv_frame_timeout(READ_TIMEOUT).await?;
        if frame.code == protocol::RESP_ERR {
            anyhow::bail!("Reset path failed: {}", protocol::parse_error(&frame.data));
        }
        Ok(())
    }

    /// Delete a contact from the firmware's persistent contact table.
    pub async fn remove_contact(&mut self, pubkey: &[u8; 32]) -> Result<()> {
        self.send_raw(&protocol::build_remove_contact(pubkey))
            .await?;
        let frame = self.recv_frame_timeout(READ_TIMEOUT).await?;
        if frame.code == protocol::RESP_ERR {
            anyhow::bail!(
                "Remove contact failed: {}",
                protocol::parse_error(&frame.data)
            );
        }
        Ok(())
    }

    /// Add/update a contact in the firmware table (CMD_ADD_UPDATE_CONTACT).
    /// Used to import a heard advert so it shows up as a contact immediately.
    pub async fn add_contact(
        &mut self,
        pubkey: &[u8; 32],
        contact_type: u8,
        flags: u8,
        out_path_len: u8,
        name: &str,
        last_advert: u32,
    ) -> Result<()> {
        self.send_raw(&protocol::build_add_contact(
            pubkey,
            contact_type,
            flags,
            out_path_len,
            name,
            last_advert,
        ))
        .await?;
        let frame = self.recv_frame_timeout(READ_TIMEOUT).await?;
        if frame.code == protocol::RESP_ERR {
            anyhow::bail!("Add contact failed: {}", protocol::parse_error(&frame.data));
        }
        Ok(())
    }

    /// Get the list of known contacts from the device.
    /// Protocol: CMD_GET_CONTACTS -> CONTACT_START(count) -> N×CONTACT -> CONTACT_END
    pub async fn get_contacts(&mut self) -> Result<Vec<protocol::ParsedContact>> {
        self.send_raw(&protocol::build_get_contacts()).await?;

        let mut contacts = Vec::new();
        loop {
            let frame = self.recv_frame_timeout(READ_TIMEOUT).await?;
            match frame.code {
                protocol::RESP_CONTACT_START => {
                    // Contains the count of contacts to follow
                    let count = if frame.data.len() >= 4 {
                        u32::from_le_bytes([
                            frame.data[0],
                            frame.data[1],
                            frame.data[2],
                            frame.data[3],
                        ])
                    } else {
                        0
                    };
                    debug!(count, "Contact list start");
                }
                protocol::RESP_CONTACT => match protocol::parse_contact(&frame.data) {
                    Ok(contact) => contacts.push(contact),
                    Err(e) => warn!("Failed to parse contact: {}", e),
                },
                protocol::RESP_CONTACT_END => {
                    debug!(count = contacts.len(), "Contact list complete");
                    break;
                }
                protocol::RESP_OK => break,
                protocol::RESP_ERR => {
                    anyhow::bail!(
                        "Get contacts failed: {}",
                        protocol::parse_error(&frame.data)
                    );
                }
                _ => {
                    debug!(code = frame.code, "Unexpected response during contact list");
                    // Don't break — might be a push notification interspersed
                }
            }
        }

        Ok(contacts)
    }

    /// Retrieve queued messages from the device.
    /// Returns raw frames (code + data) for the listener to parse.
    pub async fn sync_messages(&mut self) -> Result<Vec<protocol::InboundFrame>> {
        self.send_raw(&protocol::build_sync_next_message()).await?;

        let mut frames = Vec::new();
        loop {
            let frame = self.recv_frame_timeout(READ_TIMEOUT).await?;
            match frame.code {
                // All message types (v1 and v3)
                protocol::RESP_CONTACT_MSG
                | protocol::RESP_CONTACT_MSG_V3
                | protocol::RESP_CHANNEL_MSG
                | protocol::RESP_CHANNEL_MSG_V3 => {
                    frames.push(frame);
                    // Request next message
                    self.send_raw(&protocol::build_sync_next_message()).await?;
                }
                protocol::RESP_NO_MORE_MESSAGES => break,
                protocol::RESP_OK => break,
                protocol::RESP_ERR => {
                    anyhow::bail!(
                        "Sync messages failed: {}",
                        protocol::parse_error(&frame.data)
                    );
                }
                _ => {
                    // Push notifications can arrive during sync — skip them
                    if protocol::is_push_notification(frame.code) {
                        continue;
                    }
                    debug!(code = frame.code, "Unexpected response during message sync");
                    break;
                }
            }
        }

        Ok(frames)
    }

    /// Write raw bytes to the serial port.
    pub async fn send_raw(&mut self, data: &[u8]) -> Result<()> {
        tokio::time::timeout(WRITE_TIMEOUT, self.port.write_all(data))
            .await
            .context("Serial write timed out")?
            .context("Serial write failed")?;
        Ok(())
    }

    /// Try to read and parse one complete inbound frame.
    /// Returns the frame if one is available, or reads more data from serial.
    pub async fn try_recv_frame(&mut self) -> Result<Option<InboundFrame>> {
        // First check if we already have a complete frame in the buffer
        if let Some(frame) = protocol::decode_frame(&self.read_buf) {
            let consumed = frame.bytes_consumed;
            let result = frame;
            self.read_buf.drain(..consumed);
            return Ok(Some(result));
        }

        // Try to read more data (non-blocking via small timeout)
        let mut tmp = [0u8; READ_BUF_SIZE];
        match tokio::time::timeout(Duration::from_millis(50), self.port.read(&mut tmp)).await {
            Ok(Ok(n)) if n > 0 => {
                self.read_buf.extend_from_slice(&tmp[..n]);
            }
            _ => return Ok(None),
        }

        // Try parsing again with new data
        if let Some(frame) = protocol::decode_frame(&self.read_buf) {
            let consumed = frame.bytes_consumed;
            let result = frame;
            self.read_buf.drain(..consumed);
            return Ok(Some(result));
        }

        Ok(None)
    }

    /// Read one complete inbound frame with timeout.
    pub async fn recv_frame_timeout(&mut self, timeout: Duration) -> Result<InboundFrame> {
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            // Check buffer for a complete frame
            if let Some(frame) = protocol::decode_frame(&self.read_buf) {
                let consumed = frame.bytes_consumed;
                let result = frame;
                self.read_buf.drain(..consumed);
                return Ok(result);
            }

            // Read more data from serial
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                anyhow::bail!("Timeout waiting for serial frame");
            }

            let mut tmp = [0u8; READ_BUF_SIZE];
            match tokio::time::timeout(
                remaining.min(Duration::from_millis(100)),
                self.port.read(&mut tmp),
            )
            .await
            {
                Ok(Ok(0)) => anyhow::bail!("Serial port closed"),
                Ok(Ok(n)) => {
                    self.read_buf.extend_from_slice(&tmp[..n]);
                }
                Ok(Err(e)) => return Err(e).context("Serial read error"),
                Err(_) => continue, // timeout on this read, try again if deadline not reached
            }
        }
    }

    /// Get the device path this handle is connected to.
    pub fn path(&self) -> &str {
        &self.device_path
    }
}

// ─── Device detection ───────────────────────────────────────────────────

/// Candidate serial device paths to check on Linux.
/// /dev/mesh-radio is a stable udev symlink (see 99-mesh-radio.rules).
const SERIAL_CANDIDATES: &[&str] = &[
    "/dev/mesh-radio",
    "/dev/ttyUSB0",
    "/dev/ttyUSB1",
    "/dev/ttyUSB2",
    "/dev/ttyACM0",
    "/dev/ttyACM1",
    "/dev/ttyACM2",
];

const SKIP_SERIAL_MODEL_SUBSTRINGS: &[&str] = &["Sierra_Wireless", "Z-Wave", "Zooz"];

fn likely_non_mesh_serial_device(path: &str) -> bool {
    let Some(name) = Path::new(path).file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    let by_id = Path::new("/dev/serial/by-id");
    let Ok(entries) = std::fs::read_dir(by_id) else {
        return false;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !SKIP_SERIAL_MODEL_SUBSTRINGS
            .iter()
            .any(|needle| file_name.contains(needle))
        {
            continue;
        }
        if let Ok(target) = std::fs::read_link(entry.path()) {
            if target.file_name().and_then(|s| s.to_str()) == Some(name) {
                return true;
            }
        }
    }
    false
}

/// Scan for serial devices that could be Meshcore radios.
/// Returns paths to existing serial device files.
///
/// Dedupes by canonical (symlink-resolved) target: `/dev/mesh-radio` is a
/// stable udev symlink to whatever `/dev/ttyUSB*`/`/dev/ttyACM*` node the
/// primary radio currently enumerates as, so both names always pointed at
/// the same candidate list entry and both passed this scan — confirmed live
/// 2026-07-23, this made an already-connected, working radio (connected via
/// its `/dev/mesh-radio` alias) simultaneously appear as a second, separate
/// "detected but unclaimed" device under its raw `/dev/ttyUSBn` name. The
/// hot-swap UI's active-session guard compares path strings, so it didn't
/// recognize the two aliases as the same port, showed the "device detected"
/// modal for a radio that was already set up, and probing it there opened
/// (and DTR/RTS-reset) the exact port the live session was mid-conversation
/// with — a continuous, UI-driven reset loop that only ran while that view
/// was open (matches the reported "stops when I leave, resumes when I come
/// back"). SERIAL_CANDIDATES lists `/dev/mesh-radio` first, so it wins the
/// dedup and is what's reported when both alias and target are present.
/// (Independently re-discovered and fixed on main 2026-07-26 — both sides
/// of the 2026-07-28 merge carried an equivalent implementation.)
pub async fn detect_serial_devices() -> Vec<String> {
    let mut devices = Vec::new();
    let mut seen_real_paths = std::collections::HashSet::new();
    for path in SERIAL_CANDIDATES {
        if tokio::fs::metadata(path).await.is_ok() {
            if likely_non_mesh_serial_device(path) {
                debug!(path = %path, "Skipping known non-mesh serial device");
                continue;
            }
            let real_path = tokio::fs::canonicalize(path)
                .await
                .unwrap_or_else(|_| std::path::PathBuf::from(path));
            if !seen_real_paths.insert(real_path.clone()) {
                debug!(path = %path, real_path = %real_path.display(), "Skipping duplicate alias for an already-listed device");
                continue;
            }
            devices.push(path.to_string());
        }
    }
    devices
}

/// USB identity of a detected serial port, read from sysfs — lets the UI
/// show the actual board (native-USB boards like T-Deck/RAK4631 report their
/// name in `product`; bridge chips like CP2102/CH340 only identify the chip,
/// so vid:pid is the fallback signal).
#[derive(Debug, Clone, serde::Serialize)]
pub struct DetectedDeviceInfo {
    pub path: String,
    pub vid: Option<String>,
    pub pid: Option<String>,
    pub product: Option<String>,
    pub manufacturer: Option<String>,
    /// Unix epoch seconds of the /dev node's creation — udev recreates the
    /// node on every plug, so this changes on each replug. The UI keys its
    /// "Not Now" dismissals on (path, plugged_at): swapping a stick (or
    /// unplug/replug faster than a status poll) invalidates old dismissals
    /// and the setup modal fires again, per the hot-swap UX (2026-07-22).
    pub plugged_at: Option<u64>,
}

/// Like `detect_serial_devices`, but with USB metadata per port.
pub async fn detect_serial_devices_info() -> Vec<DetectedDeviceInfo> {
    let mut out = Vec::new();
    for path in detect_serial_devices().await {
        let usb = usb_info_for_tty(&path).await;
        // Birth time (btime), falling back to inode-change time (ctime) —
        // NOT mtime: a tty node's mtime bumps on every open()/write, so with
        // mtime here each probe/session open minted a "new" plugged_at, the
        // UI's (path, plugged_at) dismissal key never matched again, and the
        // setup modal re-fired forever on a device that never left the port
        // (observed live on a test node 2026-07-28). btime/ctime only
        // change when udev (re)creates/chowns the node — i.e. on real plugs.
        let plugged_at = tokio::fs::metadata(&path).await.ok().and_then(|m| {
            m.created()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .or_else(|| {
                    use std::os::unix::fs::MetadataExt;
                    u64::try_from(m.ctime()).ok()
                })
        });
        out.push(DetectedDeviceInfo {
            path,
            vid: usb.0,
            pid: usb.1,
            product: usb.2,
            manufacturer: usb.3,
            plugged_at,
        });
    }
    out
}

/// Resolve a tty path (following the /dev/mesh-radio symlink) to its USB
/// device sysfs node and read idVendor/idProduct/product/manufacturer.
/// Best-effort: any miss returns None fields (e.g. non-USB UARTs).
async fn usb_info_for_tty(
    path: &str,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let resolved = tokio::fs::canonicalize(path)
        .await
        .unwrap_or_else(|_| std::path::PathBuf::from(path));
    let Some(name) = resolved.file_name().and_then(|n| n.to_str()) else {
        return (None, None, None, None);
    };
    // /sys/class/tty/<name>/device -> .../usbN/N-M/N-M:1.0/(ttyUSBx|tty). Walk
    // up from the device link until a directory with idVendor appears.
    let mut dir = std::path::PathBuf::from(format!("/sys/class/tty/{name}/device"));
    for _ in 0..6 {
        if tokio::fs::metadata(dir.join("idVendor")).await.is_ok() {
            let read = |f: &str| {
                let p = dir.join(f);
                async move {
                    tokio::fs::read_to_string(p)
                        .await
                        .ok()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                }
            };
            return (
                read("idVendor").await,
                read("idProduct").await,
                read("product").await,
                read("manufacturer").await,
            );
        }
        dir.push("..");
        let Ok(canon) = tokio::fs::canonicalize(&dir).await else {
            break;
        };
        dir = canon;
    }
    (None, None, None, None)
}

/// Try to open and handshake with each detected serial device.
/// Returns the first device that responds as Meshcore.
pub async fn probe_for_meshcore(paths: &[String]) -> Option<(String, DeviceInfo)> {
    for path in paths {
        debug!(path = %path, "Probing for Meshcore device");
        match MeshcoreDevice::open(path).await {
            Ok(mut device) => {
                match device.initialize().await {
                    Ok(info) => {
                        info!(path = %path, firmware = %info.firmware_version, "Found Meshcore device");
                        // Drop the device so the listener can open it
                        drop(device);
                        return Some((path.clone(), info));
                    }
                    Err(e) => {
                        debug!(path = %path, error = %e, "Not a Meshcore device");
                    }
                }
            }
            Err(e) => {
                debug!(path = %path, error = %e, "Could not open serial port");
            }
        }
    }
    None
}
